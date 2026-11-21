use super::Result;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

#[derive(Deserialize)]
struct Pin {
    binary_sha256: String,
    binary_size: u64,
    development_directory: PathBuf,
}
type Identity = (u64, u64, u64, i64, i64, i64, i64);
static VERIFIED: OnceLock<Mutex<Option<(PathBuf, Identity)>>> = OnceLock::new();

fn identity(metadata: &fs::Metadata) -> Identity {
    (
        metadata.dev(),
        metadata.ino(),
        metadata.len(),
        metadata.mtime(),
        metadata.mtime_nsec(),
        metadata.ctime(),
        metadata.ctime_nsec(),
    )
}
fn overflow_owner(uid_map: &str, overflow: u32) -> Option<u32> {
    let mut mapped = false;
    for line in uid_map.lines() {
        let parts = line
            .split_whitespace()
            .map(str::parse::<u64>)
            .collect::<std::result::Result<Vec<_>, _>>()
            .ok()?;
        if parts.len() != 3 || parts[2] == 0 {
            return None;
        }
        if parts[1] == 0 {
            return None;
        }
        mapped = true;
    }
    mapped.then_some(overflow)
}
fn unmapped_system_owner() -> Option<u32> {
    static OWNER: OnceLock<Option<u32>> = OnceLock::new();
    *OWNER.get_or_init(|| {
        let map = fs::read_to_string("/proc/self/uid_map").ok()?;
        let overflow = fs::read_to_string("/proc/sys/kernel/overflowuid")
            .ok()?
            .trim()
            .parse()
            .ok()?;
        overflow_owner(&map, overflow)
    })
}
fn owner_compatible(metadata: &fs::Metadata) -> bool {
    let uid = metadata.uid();
    // User namespaces hide system owners behind the overflow UID; executable bytes still require the pin.
    uid == 0 || uid == rustix::process::geteuid().as_raw() || unmapped_system_owner() == Some(uid)
}
fn verify(path: &Path, pin: &Pin) -> Result<PathBuf> {
    for parent in path.ancestors().skip(1) {
        let metadata = fs::symlink_metadata(parent)
            .map_err(|_| "The bundled Bitwarden helper is unavailable.".to_owned())?;
        let mode = metadata.permissions().mode();
        let sticky_root = (metadata.uid() == 0 || unmapped_system_owner() == Some(metadata.uid()))
            && mode & 0o1000 != 0;
        if !metadata.is_dir() || !owner_compatible(&metadata) || mode & 0o022 != 0 && !sticky_root {
            return Err("The bundled Bitwarden helper directory has unsafe permissions.".into());
        }
    }
    let mut file: File = OpenOptions::new()
        .read(true)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(path)
        .map_err(|_| {
            "The bundled Bitwarden helper is unavailable. Repair this browser installation."
                .to_owned()
        })?;
    let metadata = file
        .metadata()
        .map_err(|_| "Could not inspect the bundled Bitwarden helper.".to_owned())?;
    let mode = metadata.permissions().mode();
    if !metadata.is_file()
        || !owner_compatible(&metadata)
        || metadata.nlink() != 1
        || mode & 0o022 != 0
        || mode & 0o111 == 0
        || metadata.len() != pin.binary_size
    {
        return Err(
            "The bundled Bitwarden helper has an invalid file identity or permissions.".into(),
        );
    }
    let stamp = identity(&metadata);
    let cached = VERIFIED.get_or_init(|| Mutex::new(None));
    let mut cached = cached
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if cached
        .as_ref()
        .is_some_and(|(known, identity)| known == path && *identity == stamp)
    {
        return Ok(path.into());
    }
    let mut hash = Sha256::new();
    let mut bytes = [0u8; 8192];
    loop {
        let count = file
            .read(&mut bytes)
            .map_err(|_| "Could not verify the bundled Bitwarden helper.".to_owned())?;
        if count == 0 {
            break;
        }
        hash.update(&bytes[..count]);
    }
    if format!("{:x}", hash.finalize()) != pin.binary_sha256
        || file
            .metadata()
            .ok()
            .is_none_or(|metadata| identity(&metadata) != stamp)
    {
        return Err("The bundled Bitwarden helper failed its integrity check. Repair this browser installation.".into());
    }
    *cached = Some((path.into(), stamp));
    Ok(path.into())
}

pub(super) fn bitwarden() -> Result<PathBuf> {
    let pin: Pin = serde_json::from_str(include_str!("../../assets/connectors/bitwarden.json"))
        .map_err(|_| "Invalid bundled Bitwarden version metadata.".to_owned())?;
    let executable = std::env::current_exe()
        .map_err(|_| "Could not locate this browser installation.".to_owned())?;
    let parent = executable
        .parent()
        .ok_or("Could not locate this browser installation.")?;
    let candidates = [
        parent.join("../libexec/enzim-browser/bitwarden/bw"),
        parent.join("libexec/enzim-browser/bitwarden/bw"),
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(pin.development_directory.clone())
            .join("bw"),
    ];
    // A supplied bundle must verify; a damaged installation must not fall back to ambient PATH.
    for candidate in candidates {
        if fs::symlink_metadata(&candidate).is_ok() {
            return verify(&candidate, &pin);
        }
    }
    Err("The bundled Bitwarden connector is missing. Repair this browser installation.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_namespace_ownership_does_not_relax_host_checks() {
        assert_eq!(overflow_owner("0 0 4294967295\n", 65534), None);
        assert_eq!(overflow_owner("1000 1000 1\n", 65534), Some(65534));
        assert_eq!(overflow_owner("1000 1000 1\n0 0 1\n", 65534), None);
        assert_eq!(overflow_owner("", 65534), None);
        assert_eq!(overflow_owner("malformed", 65534), None);
        assert_eq!(overflow_owner("1000 1000 0", 65534), None);
    }
    #[test]
    fn bundled_helper_rejects_changed_bytes_permissions_and_symlinks() {
        let scratch = crate::credentials::tests::Scratch::new();
        let path = scratch.path().with_file_name("bundled-bw");
        fs::write(&path, b"test helper").expect("helper");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).expect("private executable");
        let pin = Pin {
            binary_sha256: format!("{:x}", Sha256::digest(b"test helper")),
            binary_size: 11,
            development_directory: PathBuf::new(),
        };
        assert_eq!(verify(&path, &pin).expect("verified helper"), path);
        fs::write(&path, b"test tamper").expect("tamper");
        assert!(
            verify(&path, &pin).is_err(),
            "changed executable must not reuse cached verification"
        );
        fs::write(&path, b"test helper").expect("restore");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o722)).expect("unsafe mode");
        assert!(verify(&path, &pin).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).expect("restore mode");
        let link = path.with_file_name("bundle-link");
        std::os::unix::fs::symlink(&path, &link).expect("symlink");
        assert!(verify(&link, &pin).is_err());
    }
    #[test]
    fn bundled_bitwarden_resolves_without_a_cli_in_path() {
        let path = bitwarden().expect("bundled helper is part of the prepared browser");
        assert!(path.is_absolute());
        assert_eq!(path.file_name().and_then(|name| name.to_str()), Some("bw"));
    }
}
