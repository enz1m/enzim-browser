use super::{
    Credential, Result, Secret, Site,
    accounts::{Authentication, Connection, Storage},
    connector::{ActiveProcess, Connector},
    desktop::{ActiveOperation, DesktopContext, DesktopUnlock},
    provider::{Builtin, CredentialProvider, Summary},
};
use base64::Engine;
use enzui_core::state_authority::{CustodySnapshot, StateAuthority, StateIdentity};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Destination {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) writable: bool,
}
pub(crate) fn destinations(settings: &super::Settings, vault_exists: bool) -> Vec<Destination> {
    let mut sources = Vec::new();
    if settings.browser_store && (settings.storage == Storage::Browser || vault_exists) {
        sources.push(Destination {
            id: "browser".into(),
            name: "Browser vault".into(),
            writable: true,
        });
    }
    if settings.storage == Storage::System || settings.system_store {
        sources.push(Destination {
            id: "system".into(),
            name: "System keyring".into(),
            writable: true,
        });
    }
    sources.extend(
        settings
            .accounts
            .iter()
            .filter(|account| account.enabled)
            .map(|account| Destination {
                id: account.id.clone(),
                name: account.name.clone(),
                writable: true,
            }),
    );
    sources
}
pub(crate) fn default_destination(settings: &super::Settings) -> String {
    if !settings.save_to.is_empty() {
        settings.save_to.clone()
    } else {
        let sources = destinations(settings, false);
        let local = match settings.storage {
            Storage::Browser => "browser",
            Storage::System => "system",
        };
        sources
            .iter()
            .find(|source| source.id == local)
            .or_else(|| sources.iter().find(|source| source.writable))
            .map_or_else(String::new, |source| source.id.clone())
    }
}
pub(crate) fn default_unlock_source(settings: &super::Settings) -> String {
    if settings.preferred_source.is_empty() {
        default_destination(settings)
    } else {
        settings.preferred_source.clone()
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reference {
    store: String,
    item: String,
}
pub(crate) fn reference(store: &str, item: &str) -> String {
    format!(
        "ref:{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&Reference {
                store: store.into(),
                item: item.into()
            })
            .expect("string reference")
        )
    )
}
pub(crate) fn split_reference(id: &str) -> Result<(String, String)> {
    let Some(encoded) = id.strip_prefix("ref:") else {
        return Ok(("browser".into(), id.into()));
    };
    if encoded.len() > 1024 {
        return Err("The password reference is invalid.".into());
    }
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| "The password reference is invalid.".to_owned())?;
    let value: Reference = serde_json::from_slice(&bytes)
        .map_err(|_| "The password reference is invalid.".to_owned())?;
    if value.store.is_empty() || value.item.is_empty() || value.item.len() > 128 {
        return Err("The password reference is invalid.".into());
    }
    Ok((value.store, value.item))
}
struct Remote {
    connector: Connector,
    keyring: DesktopUnlock,
    open: bool,
    epoch: u64,
    touched: Instant,
    timeout: Duration,
}
impl Remote {
    fn new(
        connection: Connection,
        active: ActiveOperation,
        process: ActiveProcess,
        vault: &std::path::Path,
        context: DesktopContext,
    ) -> Self {
        let mut keyring = DesktopUnlock::for_record(
            vault,
            &connection.session_namespace(),
            "Password manager connection",
            active,
        );
        keyring.use_context(context);
        let timeout = match connection.lock_minutes {
            Some(0) => Duration::MAX,
            Some(minutes) => Duration::from_secs(u64::from(minutes) * 60),
            None => Duration::from_secs(300),
        };
        Self {
            connector: Connector::new(connection, process, vault),
            keyring,
            open: false,
            epoch: 0,
            touched: Instant::now(),
            timeout,
        }
    }
    fn lock(&mut self) {
        if self.open {
            self.epoch = self.epoch.wrapping_add(1);
        }
        self.open = false;
        self.connector.lock();
    }
    fn unlocked(&mut self) -> bool {
        if self.open && self.touched.elapsed() >= self.timeout {
            self.lock();
        }
        self.open
    }
    fn require_open(&mut self) -> Result<()> {
        if !self.unlocked() {
            return Err("Unlock this password-manager account first.".into());
        }
        self.touched = Instant::now();
        Ok(())
    }
    fn open(&mut self, authentication: &Authentication) -> Result<()> {
        self.lock();
        self.connector.unlock(authentication)?;
        self.open = true;
        self.touched = Instant::now();
        if self.connector.connection.remember && !self.connector.session.is_empty() {
            self.keyring
                .store_secret(enzui_security::secret::Secret::new(
                    self.connector.session.expose().as_bytes().to_vec(),
                ));
        }
        Ok(())
    }
    fn resume(&mut self) -> Result<bool> {
        if !self.connector.connection.remember {
            return Ok(false);
        }
        let Some(secret) = self.keyring.retrieve()? else {
            return Ok(false);
        };
        let token = std::str::from_utf8(secret.expose_secret())
            .map_err(|_| "The saved password-manager session is invalid.".to_owned())?;
        if token.len() > 4096 || token.is_empty() || token.chars().any(char::is_control) {
            return Err("The saved password-manager session is invalid.".into());
        }
        self.connector.resume_bitwarden(Secret::new(token.into()))?;
        self.open = true;
        self.touched = Instant::now();
        Ok(true)
    }
}
pub(super) struct Router {
    builtin: Builtin,
    system: super::system::System,
    remotes: Vec<Remote>,
    settings_path: PathBuf,
    vault_path: PathBuf,
    settings: Option<super::Settings>,
    authority: Option<StateAuthority>,
    active: ActiveOperation,
    process: ActiveProcess,
    last_source: String,
    desktop_context: DesktopContext,
    epoch: u64,
}
impl Router {
    pub(super) fn new(
        vault_path: PathBuf,
        settings_path: PathBuf,
        active: ActiveOperation,
        process: ActiveProcess,
    ) -> Self {
        let desktop_context = DesktopContext::default();
        let mut builtin =
            Builtin::with_cancellation(vault_path.clone(), std::sync::Arc::clone(&active));
        builtin.use_desktop_context(desktop_context.clone());
        let mut system = super::system::System::new(
            vault_path.clone(),
            settings_path.clone(),
            std::sync::Arc::clone(&active),
        );
        system.use_desktop_context(desktop_context.clone());
        Self {
            builtin,
            system,
            desktop_context,
            remotes: Vec::new(),
            settings_path,
            vault_path,
            settings: None,
            authority: None,
            active,
            process,
            last_source: "browser".into(),
            epoch: 0,
        }
    }
    fn config_current(&self) -> bool {
        self.authority.as_ref().is_some_and(|authority| {
            StateAuthority::open_existing(authority.path())
                .is_ok_and(|current| current.identity() == authority.identity())
        })
    }
    fn refresh(&mut self) -> Result<()> {
        let settings = super::config::load(&self.settings_path)?;
        if !settings.enabled || settings.provider != "enzim.builtin" {
            self.lock();
            return Err("The password manager is disabled.".into());
        }
        if self.settings.as_ref() != Some(&settings) || !self.config_current() {
            self.lock();
            let authority = StateAuthority::open_existing(&self.settings_path)
                .map_err(|_| "Could not authorize password-manager connections.".to_owned())?;
            if super::config::load(&self.settings_path)? != settings
                || !matches!(
                    authority.read_bounded(64 * 1024),
                    Ok(CustodySnapshot::Current {
                        overflow: false,
                        ..
                    })
                )
            {
                return Err("Password settings changed. Reopen the password manager.".into());
            }
            self.remotes = settings
                .accounts
                .iter()
                .filter(|account| account.enabled)
                .map(|connection| {
                    Remote::new(
                        connection.clone(),
                        std::sync::Arc::clone(&self.active),
                        std::sync::Arc::clone(&self.process),
                        &self.vault_path,
                        self.desktop_context.clone(),
                    )
                })
                .collect();
            self.authority = Some(authority);
            self.settings = Some(settings);
        }
        Ok(())
    }
    fn finish_source<T>(&mut self, source: &str, result: Result<T>) -> Result<T> {
        let offline_edit = result
            .as_ref()
            .err()
            .is_some_and(|error| error == super::connector::UNREACHABLE_FOR_EDIT);
        if result.is_err() && !offline_edit {
            match source {
                "browser" => self.builtin.lock(),
                "system" => self.system.lock(),
                _ => self.remote(source)?.lock(),
            }
            self.epoch = self.epoch.wrapping_add(1);
        }
        result
    }
    fn save_raw_to(&mut self, source: &str, entry: Credential) -> Result<()> {
        self.require_source(source)?;
        let result = match source {
            "browser" => self.builtin.save(entry),
            "system" => self.system.save(entry),
            _ => {
                let remote = self.remote(source)?;
                remote.require_open()?;
                remote.connector.save(entry)
            }
        };
        self.finish_source(source, result)
    }
    fn remote(&mut self, id: &str) -> Result<&mut Remote> {
        self.remotes
            .iter_mut()
            .find(|remote| remote.connector.connection.id == id)
            .ok_or_else(|| "This password-manager connection is unavailable.".into())
    }
    fn annotate(&self, source: &str, mut entries: Vec<Summary>) -> Vec<Summary> {
        let label = if source == "browser" {
            "Browser vault"
        } else if source == "system" {
            "System keyring"
        } else {
            self.remotes
                .iter()
                .find(|remote| remote.connector.connection.id == source)
                .map_or("Password manager", |remote| {
                    &remote.connector.connection.name
                })
        };
        for entry in &mut entries {
            entry.id = reference(source, &entry.id);
            entry.source = label.into();
        }
        entries
    }
    fn selected(&self) -> String {
        self.settings
            .as_ref()
            .map_or_else(|| "browser".into(), default_destination)
    }
    fn source_enabled(&self, source: &str) -> bool {
        self.settings.as_ref().is_some_and(|settings| {
            destinations(settings, self.vault_path.exists())
                .iter()
                .any(|destination| destination.id == source)
        })
    }
    fn require_source(&self, source: &str) -> Result<()> {
        if self.source_enabled(source) {
            Ok(())
        } else {
            Err("This password destination is disabled or disconnected.".into())
        }
    }
}
impl CredentialProvider for Router {
    fn open_sources(&self) -> Vec<String> {
        self.builtin
            .open_sources()
            .into_iter()
            .chain(self.system.open_sources())
            .chain(
                self.remotes
                    .iter()
                    .filter(|remote| remote.open)
                    .map(|remote| remote.connector.connection.id.clone()),
            )
            .collect()
    }
    fn touch_source(&mut self, source: &str) -> Result<()> {
        self.refresh()?;
        self.require_source(source)?;
        match source {
            "browser" => self.builtin.touch_source(source),
            "system" => self.system.touch_source(source),
            _ => self.remote(source)?.require_open(),
        }
    }

    fn open(&mut self, password: &str, create: bool) -> Result<()> {
        self.refresh()?;
        let source = self.selected();
        self.open_source(
            &source,
            &Authentication {
                password: Secret::new(password.into()),
                settings_identity: self.authority.as_ref().and_then(StateAuthority::identity),
                client_id: String::new(),
                client_secret: Secret::default(),
                second_factor: Secret::default(),
            },
            create,
        )
    }
    fn open_source(
        &mut self,
        source: &str,
        authentication: &Authentication,
        create: bool,
    ) -> Result<()> {
        self.refresh()?;
        self.require_source(source)?;
        if authentication.settings_identity.is_none()
            || authentication.settings_identity
                != self.authority.as_ref().and_then(StateAuthority::identity)
        {
            return Err("Password connection settings changed. Reopen the password manager and review the account before unlocking.".into());
        }
        super::protect_process()?;
        match source {
            "browser" => self.builtin.open(authentication.password.expose(), create),
            "system" => self.system.open("", false),
            _ => self.remote(source)?.open(authentication),
        }
    }
    fn identity(&self) -> Option<StateIdentity> {
        if self.last_source == "browser" {
            self.builtin.identity()
        } else if self.last_source == "system" {
            self.system.identity()
        } else {
            self.authority.as_ref().and_then(StateAuthority::identity)
        }
    }
    fn authorization_path(&self) -> Option<PathBuf> {
        Some(if self.last_source == "browser" {
            self.vault_path.clone()
        } else if self.last_source == "system" {
            super::system::revision_path(&self.vault_path)
        } else {
            self.settings_path.clone()
        })
    }
    fn authorization_bindings(&self) -> Vec<(PathBuf, StateIdentity)> {
        let mut bindings: Vec<_> = self
            .authorization_path()
            .zip(self.identity())
            .into_iter()
            .collect();
        if let Some(authority) = &self.authority
            && let Some(identity) = authority.identity()
        {
            bindings.push((self.settings_path.clone(), identity));
        } else {
            bindings.clear();
        }
        bindings
    }
    fn revocation_epoch(&self) -> u64 {
        self.remotes.iter().fold(
            self.epoch
                .wrapping_add(self.builtin.revocation_epoch())
                .wrapping_add(self.system.revocation_epoch()),
            |epoch, remote| epoch.wrapping_add(remote.epoch),
        )
    }
    fn timeout(&mut self, minutes: u16) {
        self.builtin.timeout(minutes);
        self.system.timeout(minutes);
        for remote in &mut self.remotes {
            remote.timeout = match remote.connector.connection.lock_minutes {
                Some(0) => Duration::MAX,
                Some(value) => Duration::from_secs(u64::from(value) * 60),
                None => Duration::from_secs(u64::from(minutes.clamp(1, 60)) * 60),
            };
        }
    }
    fn idle_deadline(&self) -> Option<Instant> {
        self.builtin
            .idle_deadline()
            .into_iter()
            .chain(self.system.idle_deadline())
            .chain(
                self.remotes
                    .iter()
                    .filter(|remote| remote.open)
                    .filter_map(|remote| remote.touched.checked_add(remote.timeout)),
            )
            .min()
    }
    fn lock(&mut self) {
        self.builtin.lock();
        self.system.lock();
        for remote in &mut self.remotes {
            remote.lock();
        }
        self.epoch = self.epoch.wrapping_add(1);
    }
    fn unlocked(&mut self) -> bool {
        if self.refresh().is_err() {
            self.lock();
            return false;
        }
        let mut open = self.builtin.unlocked();
        open |= self.system.unlocked();
        for remote in &mut self.remotes {
            open |= remote.unlocked();
        }
        open
    }
    fn list_source(&mut self, source: &str, site: Option<&Site>) -> Result<Vec<Summary>> {
        self.refresh()?;
        self.require_source(source)?;
        let entries = match source {
            "browser" => self.builtin.list(site),
            "system" => self.system.list(site),
            _ => {
                let remote = self.remote(source)?;
                remote.require_open()?;
                remote.connector.list(site)
            }
        };
        let entries = self.finish_source(source, entries)?;
        Ok(self.annotate(source, entries))
    }
    fn synchronize(&mut self) -> Result<()> {
        self.refresh()?;
        let mut failed = Vec::new();
        for remote in &mut self.remotes {
            super::connector::require_active_request(&self.process)?;
            if remote.unlocked() && remote.connector.synchronize().is_err() {
                failed.push(remote.connector.connection.name.clone());
            }
        }
        if failed.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "Could not refresh {}. Cached logins remain available; check the connection and try again.",
                failed.join(", ")
            ))
        }
    }
    fn list(&mut self, site: Option<&Site>) -> Result<Vec<Summary>> {
        self.refresh()?;
        let mut entries = Vec::new();
        let mut failed = None;
        let mut attempted = false;
        let sources = destinations(
            self.settings.as_ref().expect("loaded settings"),
            self.vault_path.exists(),
        );
        for source in sources {
            let open = match source.id.as_str() {
                "browser" => self.builtin.unlocked(),
                "system" => self.system.unlocked(),
                _ => self.remote(&source.id)?.unlocked(),
            };
            if !open {
                continue;
            }
            attempted = true;
            match self.list_source(&source.id, site) {
                Ok(mut values) => entries.append(&mut values),
                Err(error) => {
                    failed = Some(error);
                }
            }
        }
        if entries.is_empty()
            && let Some(error) = failed
        {
            return Err(error);
        }
        if !attempted {
            return Err("Unlock a password destination first.".into());
        }
        Ok(entries)
    }
    fn read(&mut self, id: &str, site: Option<&Site>) -> Result<Credential> {
        self.refresh()?;
        let (source, item) = split_reference(id)?;
        self.require_source(&source)?;
        let entry = match source.as_str() {
            "browser" => self.builtin.read(&item, site),
            "system" => self.system.read(&item, site),
            _ => {
                let remote = self.remote(&source)?;
                remote.require_open()?;
                remote.connector.read(&item, site)
            }
        };
        let mut entry = self.finish_source(&source, entry)?;
        entry.id = reference(&source, &entry.id);
        self.last_source = source;
        Ok(entry)
    }
    fn read_selected(&mut self, id: &str) -> Result<Credential> {
        self.refresh()?;
        let (source, item) = split_reference(id)?;
        self.require_source(&source)?;
        let result = match source.as_str() {
            "browser" => self.builtin.read(&item, None),
            "system" => self.system.read(&item, None),
            _ => {
                let remote = self.remote(&source)?;
                remote.require_open()?;
                remote.connector.read_selected(&item)
            }
        };
        let mut entry = self.finish_source(&source, result)?;
        entry.id = reference(&source, &entry.id);
        self.last_source = source;
        Ok(entry)
    }
    fn save(&mut self, mut entry: Credential) -> Result<()> {
        self.refresh()?;
        let source = if entry.id.starts_with("ref:") {
            let (source, item) = split_reference(&entry.id)?;
            entry.id = item;
            source
        } else {
            self.selected()
        };
        self.save_raw_to(&source, entry)
    }
    fn save_to(&mut self, source: &str, mut entry: Credential) -> Result<()> {
        self.refresh()?;
        self.require_source(source)?;
        if entry.id.starts_with("ref:") {
            let (origin, item) = split_reference(&entry.id)?;
            if origin != source {
                return Err("An existing login cannot silently move to another password manager. Create a separate login in that destination.".into());
            }
            entry.id = item;
        }
        self.save_raw_to(source, entry)
    }

    fn delete(&mut self, id: &str) -> Result<()> {
        self.refresh()?;
        let (source, item) = split_reference(id)?;
        self.require_source(&source)?;
        let result = match source.as_str() {
            "browser" => self.builtin.delete(&item),
            "system" => self.system.delete(&item),
            _ => {
                let remote = self.remote(&source)?;
                remote.require_open()?;
                remote.connector.delete(&item)
            }
        };
        self.finish_source(&source, result)
    }
    fn import(&mut self, entries: Vec<Credential>) -> Result<usize> {
        self.refresh()?;
        self.import_to(&self.selected(), entries)
    }
    fn import_to(&mut self, source: &str, entries: Vec<Credential>) -> Result<usize> {
        self.refresh()?;
        self.require_source(source)?;
        match source {
            "browser" => self.builtin.import(entries),
            "system" => self.system.import(entries),
            _ => Err("Use the connected password manager's own import tool. Browser imports apply to Browser vault or System keyring.".into()),
        }
    }
    fn backup(&mut self, current: &str, password: &str) -> Result<Vec<u8>> {
        self.refresh()?;
        self.last_source = "browser".into();
        self.builtin.backup(current, password)
    }
    fn csv(&mut self, current: &str) -> Result<zeroize::Zeroizing<Vec<u8>>> {
        self.refresh()?;
        self.last_source = "browser".into();
        self.builtin.csv(current)
    }
    fn rekey(&mut self, current: &str, password: &str) -> Result<()> {
        self.refresh()?;
        self.builtin.rekey(current, password)
    }
    fn desktop_enabled(&mut self, enabled: bool) {
        self.builtin.desktop_enabled(enabled);
    }
    fn desktop_remembered(&self) -> bool {
        self.builtin.desktop_remembered()
    }
    fn desktop_unlock(&mut self) -> Result<bool> {
        self.refresh()?;
        super::connector::require_active_request(&self.process)?;
        let mut opened = false;
        let mut error = None;
        if self.vault_path.exists() && self.source_enabled("browser") {
            match if self.builtin.unlocked() {
                Ok(true)
            } else {
                self.builtin.desktop_unlock()
            } {
                Ok(value) => opened |= value,
                Err(value) => error = Some(value),
            }
        }
        if self.source_enabled("system") {
            super::connector::require_active_request(&self.process)?;
            match if self.system.unlocked() {
                Ok(true)
            } else {
                self.system.desktop_unlock()
            } {
                Ok(value) => opened |= value,
                Err(value) => error = Some(value),
            }
        }
        for remote in &mut self.remotes {
            super::connector::require_active_request(&self.process)?;
            match if remote.unlocked() {
                Ok(true)
            } else {
                remote.resume()
            } {
                Ok(value) => opened |= value,
                Err(value) => error = Some(value),
            }
        }
        if !opened && let Some(error) = error {
            return Err(error);
        }
        Ok(opened)
    }
    fn forget_desktop(&mut self) -> Result<()> {
        self.builtin.forget_desktop()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelled_or_expired_unlock_requests_do_not_continue_into_another_store() {
        let scratch = super::super::tests::Scratch::new();
        let path = scratch.path().with_file_name("settings.json");
        super::super::config::save(&path, &super::super::Settings::enabled_defaults())
            .expect("settings");
        for cancelled in [false, true] {
            let token = enzui_security::cancellation::CancelToken::new();
            let deadline = if cancelled {
                token.cancel(enzui_security::cancellation::CancelCause::Cancelled);
                Instant::now() + Duration::from_secs(90)
            } else {
                Instant::now() - Duration::from_secs(1)
            };
            let process = std::sync::Arc::new(std::sync::Mutex::new(Some(
                super::super::connector::ActiveRequest {
                    token,
                    deadline,
                    settings: None,
                },
            )));
            let mut router = Router::new(scratch.path(), path.clone(), Default::default(), process);
            assert!(
                router
                    .desktop_unlock()
                    .expect_err("cancelled request")
                    .contains("cancelled")
            );
            assert!(!router.unlocked());
            assert!(!scratch.path().exists());
        }
    }
    #[test]
    fn explicit_import_destination_does_not_follow_the_global_save_default() {
        let scratch = super::super::tests::Scratch::new();
        let path = scratch.path().with_file_name("settings.json");
        let mut settings = super::super::Settings::enabled_defaults();
        settings.system_store = true;
        settings.save_to = "system".into();
        super::super::config::save(&path, &settings).expect("settings");
        let mut router = Router::new(scratch.path(), path, Default::default(), Default::default());
        router.refresh().expect("configuration");
        router
            .builtin
            .open("a distinct fixture master password", true)
            .expect("browser vault");
        let mut login =
            Credential::new(Site::parse("https://import.example.test", None).expect("site"))
                .expect("login");
        login.username = Secret::new("import fixture user".into());
        login.password = Secret::new("import fixture password".into());
        assert_eq!(
            router
                .import_to("browser", vec![login.clone()])
                .expect("explicit browser import"),
            1
        );
        let entries = router.builtin.list(None).expect("browser entries");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].username, login.username);
        assert!(router.import_to("system", vec![login.clone()]).is_err());
        assert!(router.import_to("disconnected", vec![login]).is_err());
        assert_eq!(router.builtin.list(None).expect("unchanged vault").len(), 1);
    }
    #[test]
    fn a_stale_unlock_form_cannot_send_its_password_to_a_changed_connection() {
        let scratch = super::super::tests::Scratch::new();
        let path = scratch.path().with_file_name("settings.json");
        let mut settings = super::super::Settings::enabled_defaults();
        let mut connection = Connection::new(super::super::accounts::Kind::Bitwarden, "Personal")
            .expect("connection");
        connection.account = "fixture@example.test".into();
        connection.server = "https://initial.example.test".into();
        connection.executable = "/nonexistent/fixture-connector".into();
        let source = connection.id.clone();
        settings.accounts.push(connection);
        super::super::config::save(&path, &settings).expect("initial settings");
        let (_, displayed) =
            super::super::config::load_authorized(&path).expect("displayed settings");
        settings.accounts[0].server = "https://changed.example.test".into();
        super::super::config::save(&path, &settings).expect("changed server");
        let mut router = Router::new(scratch.path(), path, Default::default(), Default::default());
        let error = router
            .open_source(
                &source,
                &Authentication {
                    password: Secret::new("fixture master password".into()),
                    settings_identity: displayed,
                    client_id: String::new(),
                    client_secret: Secret::default(),
                    second_factor: Secret::default(),
                },
                false,
            )
            .expect_err("stale consent");
        assert!(error.contains("settings changed"));
        assert!(!router.unlocked());
    }
    #[test]
    fn provider_review_bitwarden_lock_policies_survive_global_timeout_and_restart() {
        let scratch = super::super::tests::Scratch::new();
        let settings_path = scratch.path().with_file_name("settings.json");
        let mut settings = super::super::Settings::enabled_defaults();
        for (name, minutes) in [("Timed", 1), ("Restart", 0)] {
            let mut connection =
                Connection::new(super::super::accounts::Kind::Bitwarden, name).expect("connection");
            connection.remember = false;
            connection.lock_minutes = Some(minutes);
            settings.accounts.push(connection);
        }
        super::super::config::save(&settings_path, &settings).expect("settings");
        let mut router = Router::new(
            scratch.path(),
            settings_path.clone(),
            Default::default(),
            Default::default(),
        );
        router.refresh().expect("connections");
        router.timeout(60);
        assert_eq!(router.remotes[0].timeout, Duration::from_secs(60));
        assert_eq!(router.remotes[1].timeout, Duration::MAX);
        for remote in &mut router.remotes {
            remote.open = true;
            remote.touched -= Duration::from_secs(61);
        }
        assert!(router.remotes[0].require_open().is_err());
        assert!(router.remotes[1].require_open().is_ok());
        assert!(
            router.idle_deadline().is_none(),
            "restart-only source has no inactivity timer"
        );
        for remote in &mut router.remotes {
            remote.lock();
            assert!(
                !remote
                    .resume()
                    .expect("master password required after lock")
            );
        }
        let mut restarted = Router::new(
            scratch.path(),
            settings_path,
            Default::default(),
            Default::default(),
        );
        restarted.refresh().expect("restarted connections");
        for remote in &mut restarted.remotes {
            assert!(remote.require_open().is_err());
            assert!(!remote.resume().expect("no remembered restart session"));
        }
        settings.browser_store = false;
        assert!(
            !destinations(&settings, true)
                .iter()
                .any(|source| source.id == "browser")
        );
        assert_eq!(default_destination(&settings), settings.accounts[0].id);
    }
    #[test]
    fn one_expired_or_failed_source_revokes_grants_without_locking_other_accounts() {
        let scratch = super::super::tests::Scratch::new();
        let settings_path = scratch.path().with_file_name("settings.json");
        let mut settings = super::super::Settings::enabled_defaults();
        settings.accounts.push(
            Connection::new(super::super::accounts::Kind::Bitwarden, "Personal").expect("personal"),
        );
        settings.accounts.push(
            Connection::new(super::super::accounts::Kind::Bitwarden, "Business").expect("business"),
        );
        super::super::config::save(&settings_path, &settings).expect("settings");
        let mut router = Router::new(
            scratch.path(),
            settings_path,
            Default::default(),
            Default::default(),
        );
        router
            .open("a distinct fixture master password", true)
            .expect("browser");
        for remote in &mut router.remotes {
            remote.open = true;
        }
        let before = router.revocation_epoch();
        router.remotes[0].timeout = Duration::ZERO;
        assert!(router.remotes[0].require_open().is_err());
        assert_ne!(router.revocation_epoch(), before);
        assert!(router.unlocked());
        assert!(router.builtin.unlocked());
        assert!(router.remotes[1].unlocked());
        let before = router.revocation_epoch();
        let source = router.remotes[1].connector.connection.id.clone();
        assert!(
            router
                .finish_source::<()>(&source, Err("fixture transport failure".into()))
                .is_err()
        );
        assert_ne!(router.revocation_epoch(), before);
        assert!(!router.remotes[1].unlocked());
        assert!(router.builtin.unlocked());
        let before = router.revocation_epoch();
        router.builtin.timeout(1);
        router.builtin.lock();
        assert_ne!(router.revocation_epoch(), before);
    }
    #[test]
    fn routed_legacy_reference_ids_are_decoded_only_once_and_settings_revoke_leases() {
        let scratch = super::super::tests::Scratch::new();
        let path = scratch.path();
        let settings_path = path.with_file_name("settings.json");
        let settings = super::super::Settings::enabled_defaults();
        super::super::config::save(&settings_path, &settings).expect("settings");
        let mut router = Router::new(
            path,
            settings_path.clone(),
            Default::default(),
            Default::default(),
        );
        router
            .open("a distinct fixture master password", true)
            .expect("create");
        let site = Site::parse("https://reference.example.test", None).expect("site");
        let mut unrelated = Credential::new(site.clone()).expect("unrelated");
        unrelated.username = Secret::new("unrelated".into());
        let mut legacy = Credential::new(site).expect("legacy");
        legacy.id = reference("browser", &unrelated.id);
        legacy.username = Secret::new("legacy".into());
        router
            .builtin
            .save(unrelated.clone())
            .expect("existing unrelated identity");
        router
            .builtin
            .save(legacy.clone())
            .expect("existing legacy identity");
        let opaque = reference("browser", &legacy.id);
        let mut updated = router.read(&opaque, None).expect("routed read");
        updated.password = Secret::new("updated fixture password".into());
        router.save(updated.clone()).expect("routed save");
        router
            .save_to("browser", updated)
            .expect("explicit destination save");
        assert_eq!(
            router
                .read(&opaque, None)
                .expect("legacy read")
                .password
                .expose(),
            "updated fixture password"
        );
        assert_eq!(
            router
                .read(&reference("browser", &unrelated.id), None)
                .expect("unrelated read")
                .username
                .expose(),
            "unrelated"
        );
        let bindings = router.authorization_bindings();
        assert_eq!(bindings.len(), 2);
        super::super::config::save(&settings_path, &settings).expect("replace same settings");
        assert!(bindings.iter().any(|(path, identity)| {
            StateAuthority::open_existing(path)
                .expect("authority")
                .identity()
                != Some(*identity)
        }));
    }
}
