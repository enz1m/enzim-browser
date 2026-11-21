use std::{
    fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

// The SDK provisions this private directory and derives identity from the exact source.
pub(crate) fn seed(cache: &Path, identity: &str, json: &str) -> std::io::Result<()> {
    let Some(bytecode) = crate::filters::native_bytecode(json) else {
        return Ok(());
    };
    let target = cache.join(format!("ContentRuleList-{identity}"));
    if target.try_exists()? {
        return Ok(());
    }
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let temporary = cache.join(format!(
        ".bundled-filter-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)?;
    let result = file
        .write_all(bytecode)
        .and_then(|()| match fs::hard_link(&temporary, &target) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Err(error) => Err(error),
        });
    let _ = fs::remove_file(temporary);
    // Every reader now uses the copy on disk. The embedded pages that were just read
    // would otherwise stay resident for the whole session: about 20 MiB per rule set.
    let _ = enzui_webkit::ffi::runtime::release_resident_pages(bytecode);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    #[test]
    fn seeding_is_private_atomic_and_preserves_existing_cache_entries() {
        static NEXT_TEST: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "enzim-filter-cache-{}-{}",
            std::process::id(),
            NEXT_TEST.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .expect("private cache directory");
        let source = crate::filters::compiled(9).expect("bundled rules");
        let identity =
            enzui_webkit::ffi::content_filters::ContentFilterStore::identifier_for(&source.json)
                .expect("source identity");
        seed(&directory, identity.as_str(), &source.json).expect("seed native cache");
        let entry = directory.join(format!("ContentRuleList-{}", identity.as_str()));
        assert_eq!(
            fs::read(&entry).expect("cache bytes"),
            crate::filters::native_bytecode(&source.json).expect("prepared bytecode")
        );
        assert_eq!(
            fs::metadata(&entry)
                .expect("cache metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(fs::read_dir(&directory).expect("cache entries").count(), 1);
        fs::write(&entry, b"existing entry").expect("replace test cache");
        seed(&directory, identity.as_str(), &source.json).expect("preserve existing entry");
        assert_eq!(fs::read(&entry).expect("existing cache"), b"existing entry");
        seed(&directory, identity.as_str(), "[]").expect("custom policy skips bundled entry");
        assert_eq!(fs::read_dir(&directory).expect("cache entries").count(), 1);
        fs::remove_dir_all(directory).expect("remove test cache");
    }
}
