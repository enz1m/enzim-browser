use super::{
    Credential, Result, Site,
    provider::{CredentialProvider, Summary},
    store::Document,
};
use enzui_core::state_authority::{CustodySnapshot, StateAuthority};
use std::{
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};

static WRITES: Mutex<()> = Mutex::new(());

pub(crate) struct System {
    keyring: super::desktop::DesktopUnlock,
    settings_path: PathBuf,
    authority: Option<StateAuthority>,
    revision: Option<StateAuthority>,
    revision_path: PathBuf,
    open: bool,
    epoch: u64,
    touched: Instant,
    timeout: Duration,
}
impl System {
    pub(crate) fn new(
        vault_path: PathBuf,
        settings_path: PathBuf,
        active: super::desktop::ActiveOperation,
    ) -> Self {
        Self {
            keyring: super::desktop::DesktopUnlock::for_record(
                &vault_path,
                "password-document-v1",
                "enzim Browser passwords",
                active,
            ),
            revision_path: revision_path(&vault_path),
            settings_path,
            authority: None,
            revision: None,
            open: false,
            epoch: 0,
            touched: Instant::now(),
            timeout: Duration::from_secs(300),
        }
    }
    pub(super) fn use_desktop_context(&mut self, context: super::desktop::DesktopContext) {
        self.keyring.use_context(context);
    }
    fn current(&self) -> bool {
        [&self.revision, &self.authority].iter().all(|authority| {
            authority.as_ref().is_some_and(|authority| {
                StateAuthority::open_existing(authority.path())
                    .is_ok_and(|current| current.identity() == authority.identity())
            })
        })
    }
    fn require_open(&mut self) -> Result<()> {
        if !self.unlocked() {
            return Err("Unlock the system password store first.".into());
        }
        self.touched = Instant::now();
        Ok(())
    }
    fn read_document(&mut self) -> Result<Document> {
        let Some(secret) = self.keyring.retrieve_required()? else {
            return Ok(Document::default());
        };
        if secret.expose_secret().len() > super::MAX_BYTES {
            return Err("The system password store exceeds its size limit.".into());
        }
        let document: Document = serde_json::from_slice(secret.expose_secret()).map_err(|_| {
            "The system password store has an unsupported or damaged format.".to_owned()
        })?;
        document.validate()?;
        Ok(document)
    }
    fn change<T>(&mut self, apply: impl FnOnce(&mut Document) -> Result<T>) -> Result<T> {
        self.require_open()?;
        let _guard = WRITES
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut document = self.read_document()?;
        let result = apply(&mut document)?;
        document.validate()?;
        let mut bytes = zeroize::Zeroizing::new(
            serde_json::to_vec(&document)
                .map_err(|_| "Could not encode the system password store.".to_owned())?,
        );
        if bytes.len() > super::MAX_BYTES {
            return Err("The system password store exceeds its size limit.".into());
        }
        if !self.current() {
            self.lock();
            return Err("Password settings changed. Unlock the selected store again.".into());
        }
        let revision = self
            .revision
            .as_mut()
            .ok_or("The system password revision is unavailable.")?;
        revision.publish(&super::random::<32>()?).map_err(|_| {
            "System passwords changed. Unlock the store again before saving.".to_owned()
        })?;
        if !self
            .keyring
            .store_secret(enzui_security::secret::Secret::new(std::mem::take(
                &mut *bytes,
            )))
        {
            self.lock();
            return Err("The desktop keyring did not confirm saving. Refresh the store before retrying; no browser-file fallback was used.".into());
        }
        Ok(result)
    }
    #[cfg(test)]
    fn with_store(
        vault_path: PathBuf,
        settings_path: PathBuf,
        store: std::sync::Arc<dyn enzui_services::credential_store::CredentialStore>,
    ) -> Self {
        let active = std::sync::Arc::new(Mutex::new(None));
        let mut result = Self::new(
            vault_path.clone(),
            settings_path,
            std::sync::Arc::clone(&active),
        );
        result.keyring = super::desktop::DesktopUnlock::with_record_store(
            &vault_path,
            "password-document-v1",
            "enzim Browser passwords",
            store,
            active,
        );
        result
    }
}
impl CredentialProvider for System {
    fn open_sources(&self) -> Vec<String> {
        if self.open {
            vec!["system".into()]
        } else {
            Vec::new()
        }
    }
    fn touch_source(&mut self, source: &str) -> Result<()> {
        if source != "system" {
            return Err("This password source is unavailable.".into());
        }
        self.require_open()
    }

    fn revocation_epoch(&self) -> u64 {
        self.epoch
    }
    fn open(&mut self, _password: &str, _create: bool) -> Result<()> {
        self.lock();
        super::protect_process()?;
        let settings = super::config::load(&self.settings_path)?;
        if !settings.enabled
            || settings.provider != "enzim.builtin"
            || !(settings.system_store || settings.storage == super::accounts::Storage::System)
        {
            return Err("The system password store is disabled.".into());
        }
        let _guard = WRITES
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let authority = StateAuthority::open_existing(&self.settings_path)
            .map_err(|_| "Could not authorize the system password store.".to_owned())?;
        if super::config::load(&self.settings_path)? != settings {
            return Err("Password settings changed while unlocking.".into());
        }
        self.read_document()?;
        let parent = self
            .revision_path
            .parent()
            .ok_or("Invalid system password revision path.")?;
        enzui_security::secure_dir::ensure_private_dir(parent)
            .map_err(|_| "Could not protect the system password revision.".to_owned())?;
        let mut revision = StateAuthority::open_or_create(&self.revision_path)
            .map_err(|_| "Could not authorize the system password revision.".to_owned())?;
        let initialize = match revision
            .read_bounded(32)
            .map_err(|_| "Could not read system password authorization.".to_owned())?
        {
            CustodySnapshot::Absent => true,
            CustodySnapshot::Current {
                bytes,
                overflow: false,
                ..
            } if bytes.is_empty() => true,
            CustodySnapshot::Current {
                bytes,
                overflow: false,
                ..
            } if bytes.len() == 32 => false,
            _ => return Err(
                "System password authorization is invalid or changed. Reopen the password manager."
                    .into(),
            ),
        };
        if initialize {
            revision
                .publish(&super::random::<32>()?)
                .map_err(|_| "Could not initialize system password authorization.".to_owned())?;
        }
        if !matches!(
            authority.read_bounded(64 * 1024),
            Ok(CustodySnapshot::Current {
                overflow: false,
                ..
            })
        ) {
            return Err("Password settings changed while unlocking.".into());
        }
        self.authority = Some(authority);
        self.revision = Some(revision);
        self.open = true;
        self.touched = Instant::now();
        Ok(())
    }
    fn identity(&self) -> Option<enzui_core::state_authority::StateIdentity> {
        self.revision.as_ref().and_then(StateAuthority::identity)
    }
    fn authorization_path(&self) -> Option<PathBuf> {
        Some(self.revision_path.clone())
    }
    fn timeout(&mut self, minutes: u16) {
        self.timeout = Duration::from_secs(u64::from(minutes.clamp(1, 60)) * 60);
    }
    fn idle_deadline(&self) -> Option<Instant> {
        self.open.then(|| self.touched + self.timeout)
    }
    fn lock(&mut self) {
        if self.open {
            self.epoch = self.epoch.wrapping_add(1);
        }
        self.open = false;
        self.authority = None;
        self.revision = None;
    }
    fn unlocked(&mut self) -> bool {
        if self.open && (self.touched.elapsed() >= self.timeout || !self.current()) {
            self.lock();
        }
        self.open
    }
    fn desktop_unlock(&mut self) -> Result<bool> {
        self.open("", false).map(|()| true)
    }
    fn list(&mut self, site: Option<&Site>) -> Result<Vec<Summary>> {
        self.require_open()?;
        Ok(self
            .read_document()?
            .entries
            .iter()
            .filter(|entry| site.is_none_or(|site| site.matches(&entry.site)))
            .map(Summary::from)
            .collect())
    }
    fn read(&mut self, id: &str, site: Option<&Site>) -> Result<Credential> {
        self.require_open()?;
        self.read_document()?
            .entries
            .into_iter()
            .find(|entry| entry.id == id && site.is_none_or(|site| site.matches(&entry.site)))
            .ok_or_else(|| {
                "This system login is unavailable for the selected website and container.".into()
            })
    }
    fn save(&mut self, credential: Credential) -> Result<()> {
        credential.validate()?;
        self.change(move |document| {
            if let Some(entry) = document
                .entries
                .iter_mut()
                .find(|entry| entry.id == credential.id)
            {
                *entry = credential;
            } else {
                document.entries.push(credential);
            }
            Ok(())
        })
    }
    fn delete(&mut self, id: &str) -> Result<()> {
        self.change(|document| {
            let before = document.entries.len();
            document.entries.retain(|entry| entry.id != id);
            if before == document.entries.len() {
                return Err("This system login no longer exists.".into());
            }
            Ok(())
        })
    }
    fn import(&mut self, entries: Vec<Credential>) -> Result<usize> {
        for entry in &entries {
            entry.validate()?;
        }
        self.change(move |document| {
            let mut added = 0;
            for mut entry in entries {
                if document.entries.iter().any(|existing| {
                    existing.site == entry.site
                        && existing.username == entry.username
                        && existing.password == entry.password
                        && existing.totp == entry.totp
                }) {
                    continue;
                }
                if document
                    .entries
                    .iter()
                    .any(|existing| existing.id == entry.id)
                {
                    entry.id = Credential::new(entry.site.clone())?.id;
                }
                document.entries.push(entry);
                added += 1;
            }
            Ok(added)
        })
    }
    fn backup(&mut self, _current: &str, _password: &str) -> Result<Vec<u8>> {
        Err("Encrypted browser backups apply to the browser vault. Manage system-store backups with your desktop keyring.".into())
    }
    fn csv(&mut self, _current: &str) -> Result<zeroize::Zeroizing<Vec<u8>>> {
        Err("CSV export with a vault password applies to the browser vault.".into())
    }
    fn rekey(&mut self, _current: &str, _password: &str) -> Result<()> {
        Err("Change the system keyring password in your desktop's password manager.".into())
    }
}

pub(super) fn revision_path(vault: &std::path::Path) -> PathBuf {
    vault.with_file_name("system-revision")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::tests::Scratch;
    fn entry(origin: &str, container: Option<u64>) -> Credential {
        let mut login = Credential::new(Site::parse(origin, container).expect("test site"))
            .expect("test login");
        login.username = super::super::Secret::new("fixture@example.test".into());
        login.password = super::super::Secret::new("private fixture password".into());
        login
    }
    use enzui_services::credential_store::{MemoryCredentialStore, UnavailableCredentialStore};
    use std::sync::Arc;
    fn settings(scratch: &Scratch) -> PathBuf {
        let path = scratch.path().with_file_name("settings.json");
        let mut settings = super::super::Settings::enabled_defaults();
        settings.storage = super::super::accounts::Storage::System;
        settings.system_store = true;
        super::super::config::save(&path, &settings).expect("system settings");
        path
    }
    #[test]
    fn editor_activity_extends_system_idle_without_creating_a_local_vault() {
        let scratch = Scratch::new();
        let config = settings(&scratch);
        let mut system = System::with_store(
            scratch.path(),
            config,
            Arc::new(MemoryCredentialStore::new()),
        );
        system.open("", false).expect("system");
        system.touched = Instant::now() - Duration::from_secs(299);
        system.touch_source("system").expect("editor activity");
        assert!(
            system.idle_deadline().expect("deadline") > Instant::now() + Duration::from_secs(298)
        );
        assert!(!scratch.path().exists());
        system.touched = Instant::now() - Duration::from_secs(301);
        assert!(system.touch_source("system").is_err());
        assert!(system.open_sources().is_empty());
    }
    #[test]
    fn system_passwords_live_in_the_keyring_and_preserve_scope_across_clients() {
        let scratch = Scratch::new();
        let config = settings(&scratch);
        let store = Arc::new(MemoryCredentialStore::new());
        let mut system = System::with_store(
            scratch.path(),
            config.clone(),
            Arc::<MemoryCredentialStore>::clone(&store),
        );
        system.open("", false).expect("native store unlock");
        let login = entry("https://example.test/", Some(7));
        let id = login.id.clone();
        system.save(login.clone()).expect("save in keyring");
        assert!(!scratch.path().exists(), "no local vault file is created");
        assert_eq!(
            store.entry_count(),
            1,
            "credentials are held by Secret Service"
        );
        system.lock();
        assert!(system.read(&id, None).is_err());
        let mut reopened = System::with_store(
            scratch.path(),
            config,
            Arc::<MemoryCredentialStore>::clone(&store),
        );
        reopened.open("", false).expect("fresh native client");
        assert_eq!(
            reopened
                .read(&id, Some(&login.site))
                .expect("matching site")
                .password,
            login.password
        );
        assert!(
            reopened
                .read(
                    &id,
                    Some(&Site::parse("https://example.test/", None).expect("other container"))
                )
                .is_err()
        );
        let foreign = Scratch::new();
        let other_config = settings(&foreign);
        let mut other = System::with_store(foreign.path(), other_config, store);
        other.open("", false).expect("other profile");
        assert!(other.list(None).expect("other profile list").is_empty());
        reopened.delete(&id).expect("delete system login");
        assert!(reopened.list(None).expect("empty").is_empty());
    }
    #[test]
    fn system_mutations_revoke_other_clients_before_a_new_keyring_value_is_used() {
        let scratch = Scratch::new();
        let config = settings(&scratch);
        let store = Arc::new(MemoryCredentialStore::new());
        let mut first = System::with_store(
            scratch.path(),
            config.clone(),
            Arc::<MemoryCredentialStore>::clone(&store),
        );
        first.open("", false).expect("first client");
        let mut second = System::with_store(scratch.path(), config, store);
        second.open("", false).expect("second client");
        let revision = second.identity().expect("authorized revision");
        let path = second.authorization_path().expect("revision path");
        assert_eq!(
            StateAuthority::open_existing(&path)
                .expect("held revision")
                .identity(),
            Some(revision)
        );
        first
            .save(entry("https://changed.example/", None))
            .expect("mutation");
        assert_ne!(
            StateAuthority::open_existing(&path)
                .expect("new revision")
                .identity(),
            Some(revision)
        );
        assert!(!second.unlocked());
        assert!(second.list(None).is_err());
        assert!(!scratch.path().exists());
        assert_eq!(std::fs::read(path).expect("non-secret revision").len(), 32);
    }
    #[test]
    fn unavailable_system_store_never_creates_a_plaintext_or_browser_fallback() {
        let scratch = Scratch::new();
        let config = settings(&scratch);
        let mut system =
            System::with_store(scratch.path(), config, Arc::new(UnavailableCredentialStore));
        assert!(system.open("", false).is_err());
        assert!(!scratch.path().exists());
        assert!(!system.unlocked());
    }
    #[test]
    #[ignore = "requires an isolated unlocked Secret Service provider"]
    fn system_secret_service_roundtrip() {
        let scratch = Scratch::new();
        let config = settings(&scratch);
        let active = Arc::new(Mutex::new(None));
        let mut system = System::new(
            scratch.path(),
            config.clone(),
            std::sync::Arc::clone(&active),
        );
        system.open("", false).expect("unlock");
        let login = entry("https://system-fixture.example/", None);
        let id = login.id.clone();
        system.save(login.clone()).expect("real system save");
        drop(system);
        let mut reopened = System::new(scratch.path(), config, active);
        reopened.open("", false).expect("fresh native client");
        assert_eq!(
            reopened
                .read(&id, Some(&login.site))
                .expect("real system read")
                .password,
            login.password
        );
        assert!(!scratch.path().exists());
        reopened.delete(&id).expect("real system delete");
        reopened
            .keyring
            .forget()
            .expect("remove fixture keyring document");
    }
}
