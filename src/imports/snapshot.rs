use enzui_security::{
    secure_dir::{SecureRoot, ensure_private_dir},
    tempstage::DirectoryStage,
};
use rusqlite::{Connection, OpenFlags};
use std::{
    ffi::OsStr,
    fs::File,
    io::{self, Read},
    os::unix::fs::MetadataExt,
    path::Path,
    time::{Duration, Instant},
};

const MAX_DATABASE: u64 = 1024 * 1024 * 1024;
pub(super) const MAX_JSON: u64 = 16 * 1024 * 1024;
fn stamp(file: &File) -> io::Result<(u64, u64, u64, i64, i64, i64, i64)> {
    let m = file.metadata()?;
    if !m.is_file() {
        return Err(io::Error::other("The source is not a regular file"));
    }
    Ok((
        m.dev(),
        m.ino(),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    ))
}
pub(super) struct Profile {
    root: SecureRoot,
    scratch: SecureRoot,
}
pub(super) struct Database {
    pub(super) connection: Connection,
    _stage: DirectoryStage,
}
impl Profile {
    pub(super) fn open(path: &Path, cache: &Path) -> Result<Self, String> {
        let root =
            SecureRoot::open(path).map_err(|e| format!("Could not read this profile: {e}"))?;
        let private = ensure_private_dir(cache).map_err(|e| e.to_string())?;
        let scratch = SecureRoot::open(&private).map_err(|e| e.to_string())?;
        Ok(Self { root, scratch })
    }
    pub(super) fn has(&self, name: &str) -> bool {
        !self
            .root
            .open_entry_read(OsStr::new(name))
            .is_err_and(|error| error.kind() == io::ErrorKind::NotFound)
    }
    pub(super) fn bytes(&self, name: &str) -> Result<Vec<u8>, String> {
        let mut file = self
            .root
            .open_entry_read(OsStr::new(name))
            .map_err(|e| e.to_string())?;
        let before = stamp(&file).map_err(|e| e.to_string())?;
        if before.2 > MAX_JSON {
            return Err("The source file exceeds the 16 MiB import limit".into());
        }
        let mut bytes = Vec::new();
        (&mut file)
            .take(MAX_JSON + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 != before.2 || stamp(&file).map_err(|e| e.to_string())? != before {
            return Err(
                "The source changed while it was read. Close that browser and try again.".into(),
            );
        }
        Ok(bytes)
    }
    pub(super) fn database(&self, name: &str) -> Result<Database, String> {
        for _ in 0..3 {
            let name_stage = format!(
                "snapshot-{}-{}",
                std::process::id(),
                crate::browser::next_tab_key()
            );
            let stage = DirectoryStage::create(&self.scratch, OsStr::new(&name_stage))
                .map_err(|e| e.to_string())?;
            let mut files = Vec::new();
            for (leaf, optional) in [(name.to_owned(), false), (format!("{name}-wal"), true)] {
                match self.root.open_entry_read(OsStr::new(&leaf)) {
                    Ok(file) => {
                        let state = stamp(&file).map_err(|e| e.to_string())?;
                        if state.2 > MAX_DATABASE {
                            return Err("A database exceeds the 1 GiB import limit.".into());
                        }
                        files.push((leaf, file, state));
                    }
                    Err(error) if optional && error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.to_string()),
                }
            }
            for (leaf, file, _) in &mut files {
                let mut copy = stage
                    .root()
                    .create_entry_private(OsStr::new(leaf))
                    .map_err(|e| e.to_string())?;
                io::copy(&mut file.take(MAX_DATABASE + 1), &mut copy).map_err(|e| e.to_string())?;
            }
            // A database and its WAL must describe one quiescent interval; only the copies may recover SQLite journals.
            let stable = files.iter().all(|(leaf, file, before)| {
                stamp(file).is_ok_and(|after| after == *before)
                    && self
                        .root
                        .open_entry_read(OsStr::new(leaf))
                        .and_then(|file| stamp(&file))
                        .is_ok_and(|after| after == *before)
            });
            let wal_present = self
                .root
                .open_entry_read(OsStr::new(&format!("{name}-wal")))
                .is_ok();
            if !stable || wal_present != files.iter().any(|(leaf, _, _)| leaf.ends_with("-wal")) {
                continue;
            }
            let path = stage.path().join(name);
            let connection = Connection::open_with_flags(
                path,
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(|e| e.to_string())?;
            connection.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH, 1024 * 1024);
            connection.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA query_only=ON; PRAGMA cache_size=-4096; PRAGMA mmap_size=0;").map_err(|e| e.to_string())?;
            let deadline = Instant::now() + Duration::from_secs(10);
            connection.progress_handler(10_000, Some(move || Instant::now() > deadline));
            return Ok(Database {
                connection,
                _stage: stage,
            });
        }
        Err("This profile is changing. Close its browser and try again.".into())
    }
}
