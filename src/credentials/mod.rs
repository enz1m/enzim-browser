pub(crate) mod accounts;
mod bundle;
pub(crate) mod config;
mod connector;
mod crypto;
mod desktop;
pub(crate) mod exchange;
mod model;
pub(crate) mod passkey;
pub(crate) mod provider;
pub(crate) mod router;
pub(crate) mod service;
mod store;
mod system;
mod totp;

pub(crate) use model::{Credential, Secret, Settings, Site};

/// One text secret in the desktop keyring, such as a container's proxy
/// password or the assistant's API key.
pub(crate) struct KeyringText(desktop::DesktopUnlock);

/// The desktop keyring's record of one container's proxy password.
pub(crate) type ProxyPassword = KeyringText;

impl KeyringText {
    /// The record for the container stored as `storage_name`.
    pub(crate) fn for_container(storage_name: &str) -> Self {
        Self::named(&format!("proxy-{storage_name}"), "Container proxy password")
    }

    /// The record `name` of this profile, described to the keyring as `label`.
    pub(crate) fn named(name: &str, label: &str) -> Self {
        Self(desktop::DesktopUnlock::for_record(
            &Vault::default_path(),
            name,
            label,
            Default::default(),
        ))
    }

    /// The password, if the keyring has one.
    pub(crate) fn read(&mut self) -> Result<Option<Secret>> {
        Ok(self.0.retrieve_required()?.and_then(|secret| {
            std::str::from_utf8(secret.expose_secret())
                .ok()
                .map(|text| Secret::new(text.to_owned()))
        }))
    }

    /// Keep `password`; whether the keyring confirmed it.
    pub(crate) fn store(&mut self, password: &Secret) -> bool {
        self.0.store_secret(enzui_security::secret::Secret::new(
            password.expose().as_bytes().to_vec(),
        ))
    }

    /// Remove the password.
    pub(crate) fn forget(&mut self) -> Result<()> {
        self.0.forget()
    }

    #[cfg(test)]
    fn with_store(
        storage_name: &str,
        store: std::sync::Arc<dyn enzui_services::credential_store::CredentialStore>,
    ) -> Self {
        Self(desktop::DesktopUnlock::with_record_store(
            &Vault::default_path(),
            &format!("proxy-{storage_name}"),
            "Container proxy password",
            store,
            Default::default(),
        ))
    }
}

#[cfg(test)]
mod proxy_password_tests {
    use super::*;
    use enzui_services::credential_store::MemoryCredentialStore;
    use std::sync::Arc;

    #[test]
    fn a_proxy_password_is_kept_read_back_and_forgotten_per_container() {
        let store = Arc::new(MemoryCredentialStore::new());
        let shared = || -> Arc<dyn enzui_services::credential_store::CredentialStore> {
            Arc::<MemoryCredentialStore>::clone(&store)
        };
        let mut work = ProxyPassword::with_store("container-1", shared());
        let mut home = ProxyPassword::with_store("container-2", shared());
        assert_eq!(work.read().expect("read"), None);
        assert!(work.store(&Secret::new("hunter2".into())));
        assert_eq!(
            ProxyPassword::with_store("container-1", shared())
                .read()
                .expect("read")
                .as_ref()
                .map(Secret::expose),
            Some("hunter2"),
            "a new reader finds what was kept"
        );
        assert_eq!(home.read().expect("read"), None, "each container has its own");
        work.forget().expect("forget");
        assert_eq!(work.read().expect("read"), None);
    }
}
#[cfg(test)]
pub(crate) use provider::Builtin;
pub(crate) use provider::CredentialProvider;
pub(crate) use store::Vault;
pub(crate) use totp::Totp;

pub(crate) type Result<T> = std::result::Result<T, String>;
pub(crate) const MAX_BYTES: usize = 16 * 1024 * 1024;
pub(crate) const MAX_ENTRIES: usize = 10_000;

pub(crate) fn protect_process() -> Result<()> {
    #[cfg(not(test))]
    rustix::process::set_dumpable_behavior(rustix::process::DumpableBehavior::NotDumpable)
        .map_err(|_| "The operating system could not protect credential input.".to_owned())?;
    Ok(())
}

pub(crate) fn random<const N: usize>() -> Result<[u8; N]> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes).map_err(|_| "Secure randomness is unavailable.".to_owned())?;
    Ok(bytes)
}

pub(crate) fn generate(length: usize) -> Result<Secret> {
    const ALPHABET: &[u8] =
        b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789!@#$%^&*()-_=+";
    if !(16..=128).contains(&length) {
        return Err("Choose a password length between 16 and 128.".into());
    }
    let mut output = zeroize::Zeroizing::new(String::with_capacity(length));
    let limit = 256 - 256 % ALPHABET.len();
    while output.len() < length {
        for byte in random::<64>()? {
            if usize::from(byte) < limit {
                output.push(char::from(ALPHABET[usize::from(byte) % ALPHABET.len()]));
                if output.len() == length {
                    break;
                }
            }
        }
    }
    Ok(Secret::new(std::mem::take(&mut *output)))
}

#[cfg(test)]
pub(crate) mod tests;

/// Password manager diagnostics on standard error with
/// `ENZIM_CREDENTIAL_TRACE=1`: decisions, origins and counts, never a
/// username, a password or a code.
pub(crate) fn credential_trace(args: std::fmt::Arguments<'_>) {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ON.get_or_init(|| std::env::var_os("ENZIM_CREDENTIAL_TRACE").is_some_and(|v| v == "1")) {
        eprintln!("credential-trace {args}");
    }
}
