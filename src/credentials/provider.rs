use super::{Credential, Result, Site, Vault};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Clone)]
pub(crate) struct Summary {
    pub(crate) id: String,
    pub(crate) site: Site,
    pub(crate) name: super::Secret,
    pub(crate) username: super::Secret,
    pub(crate) has_totp: bool,
    pub(crate) has_password: bool,
    /// The relying party ID and credential ID of the login's passkey.
    pub(crate) passkey: Option<(String, String)>,
    pub(crate) source: String,
    pub(crate) uri_rules: Option<Vec<(super::Secret, Option<u64>)>>,
}
impl Summary {
    pub(crate) fn matches_site(&self, site: &Site) -> bool {
        self.uri_rules.as_ref().map_or_else(
            || self.site == *site,
            |rules| {
                rules.iter().any(|(pattern, mode)| {
                    super::connector::uri_pattern_site(
                        pattern.expose(),
                        *mode,
                        self.site.container,
                        Some(site),
                    )
                    .is_some()
                })
            },
        )
    }
    pub(crate) fn site_label(&self) -> std::borrow::Cow<'_, str> {
        if !self.site.origin.is_empty() {
            return std::borrow::Cow::Borrowed(&self.site.origin);
        }
        if let Some((pattern, _)) = self.uri_rules.as_ref().and_then(|rules| rules.first())
            && let Ok(url) = url::Url::parse(pattern.expose())
        {
            return std::borrow::Cow::Owned(url.origin().ascii_serialization());
        }
        std::borrow::Cow::Borrowed("No website saved")
    }
    pub(crate) fn matches_query(&self, query: &str) -> bool {
        [
            self.name.expose(),
            self.username.expose(),
            &self.site.origin,
        ]
        .iter()
        .any(|value| value.to_lowercase().contains(query))
            || self.uri_rules.as_ref().is_some_and(|rules| {
                rules
                    .iter()
                    .any(|(pattern, _)| pattern.expose().to_lowercase().contains(query))
            })
    }
}
impl std::fmt::Debug for Summary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CredentialSummary([redacted])")
    }
}
impl From<&Credential> for Summary {
    fn from(entry: &Credential) -> Self {
        Self {
            id: entry.id.clone(),
            site: entry.site.clone(),
            name: entry.name.clone(),
            username: entry.username.clone(),
            has_totp: entry.totp.is_some(),
            has_password: !entry.password.is_empty(),
            passkey: entry
                .passkey
                .as_ref()
                .map(|passkey| (passkey.rp_id.clone(), passkey.credential_id.clone())),
            source: String::new(),
            uri_rules: None,
        }
    }
}

pub(crate) trait CredentialProvider: Send {
    fn open(&mut self, password: &str, create: bool) -> Result<()>;
    fn open_source(
        &mut self,
        source: &str,
        authentication: &super::accounts::Authentication,
        create: bool,
    ) -> Result<()> {
        if source != "browser" {
            return Err("This password destination is unavailable.".into());
        }
        self.open(authentication.password.expose(), create)
    }
    fn list_source(&mut self, source: &str, site: Option<&Site>) -> Result<Vec<Summary>> {
        if source != "browser" {
            return Err("This password destination is unavailable.".into());
        }
        self.list(site)
    }
    fn save_to(&mut self, source: &str, entry: Credential) -> Result<()> {
        if source != "browser" {
            return Err("This password destination is unavailable.".into());
        }
        self.save(entry)
    }
    /// A deliberate chrome selection, independent of automatic website matching.
    fn read_selected(&mut self, id: &str) -> Result<Credential> {
        self.read(id, None)
    }
    fn identity(&self) -> Option<enzui_core::state_authority::StateIdentity>;
    fn authorization_path(&self) -> Option<PathBuf> {
        None
    }
    fn authorization_bindings(&self) -> Vec<(PathBuf, enzui_core::state_authority::StateIdentity)> {
        self.authorization_path()
            .zip(self.identity())
            .into_iter()
            .collect()
    }
    fn revocation_epoch(&self) -> u64 {
        0
    }
    fn open_sources(&self) -> Vec<String> {
        Vec::new()
    }
    fn touch_source(&mut self, _source: &str) -> Result<()> {
        Ok(())
    }
    fn timeout(&mut self, minutes: u16);
    fn idle_deadline(&self) -> Option<Instant>;
    fn lock(&mut self);
    fn unlocked(&mut self) -> bool;
    fn synchronize(&mut self) -> Result<()> {
        Ok(())
    }
    fn list(&mut self, site: Option<&Site>) -> Result<Vec<Summary>>;
    fn read(&mut self, id: &str, site: Option<&Site>) -> Result<Credential>;
    fn save(&mut self, credential: Credential) -> Result<()>;
    fn delete(&mut self, id: &str) -> Result<()>;
    fn import_to(&mut self, source: &str, entries: Vec<Credential>) -> Result<usize> {
        if source != "browser" {
            return Err("This password destination cannot import logins.".into());
        }
        self.import(entries)
    }
    fn import(&mut self, entries: Vec<Credential>) -> Result<usize>;
    fn backup(&mut self, current: &str, password: &str) -> Result<Vec<u8>>;
    fn csv(&mut self, current: &str) -> Result<zeroize::Zeroizing<Vec<u8>>>;
    fn rekey(&mut self, current: &str, password: &str) -> Result<()>;
    fn desktop_enabled(&mut self, _enabled: bool) {}
    fn desktop_unlock(&mut self) -> Result<bool> {
        Ok(false)
    }
    fn desktop_remembered(&self) -> bool {
        false
    }
    fn forget_desktop(&mut self) -> Result<()> {
        Ok(())
    }
}

pub(crate) struct Builtin {
    path: PathBuf,
    vault: Option<Vault>,
    touched: Instant,
    timeout: Duration,
    retry_after: Instant,
    failures: u32,
    desktop: super::desktop::DesktopUnlock,
    desktop_enabled: bool,
    remembered: bool,
    epoch: u64,
}
impl Builtin {
    pub(super) fn with_cancellation(
        path: PathBuf,
        active: super::desktop::ActiveOperation,
    ) -> Self {
        Self {
            desktop: super::desktop::DesktopUnlock::new(&path, active),
            desktop_enabled: false,
            remembered: false,
            epoch: 0,
            path,
            vault: None,
            touched: Instant::now(),
            timeout: Duration::from_secs(300),
            retry_after: Instant::now(),
            failures: 0,
        }
    }
    pub(super) fn use_desktop_context(&mut self, context: super::desktop::DesktopContext) {
        self.desktop.use_context(context);
    }
    pub(crate) fn open(&mut self, password: &str, create: bool) -> Result<()> {
        if Instant::now() < self.retry_after {
            return Err("Wait a moment before trying to unlock again.".into());
        }
        self.lock();
        let result = if create {
            Vault::create(&self.path, password)
        } else {
            Vault::unlock(&self.path, password)
        };
        match result {
            Ok(vault) => {
                self.remembered = self.desktop_enabled && self.desktop.remember(&vault);
                self.vault = Some(vault);
                self.touched = Instant::now();
                self.failures = 0;
                Ok(())
            }
            Err(error) => {
                self.failures = self.failures.saturating_add(1);
                self.retry_after =
                    Instant::now() + Duration::from_secs(u64::from(self.failures.min(10)));
                Err(error)
            }
        }
    }
    pub(crate) fn identity(&self) -> Option<enzui_core::state_authority::StateIdentity> {
        self.vault.as_ref().and_then(Vault::identity)
    }
    pub(crate) fn timeout(&mut self, minutes: u16) {
        self.timeout = Duration::from_secs(u64::from(minutes.clamp(1, 60)) * 60);
        self.expire();
    }
    pub(crate) fn expire(&mut self) {
        if self.touched.elapsed() >= self.timeout
            || self.vault.as_ref().is_some_and(|vault| !vault.current())
        {
            self.lock();
        }
    }
    pub(crate) fn vault(&mut self) -> Result<&mut Vault> {
        self.expire();
        self.touched = Instant::now();
        self.vault
            .as_mut()
            .ok_or_else(|| "Unlock the password manager first.".into())
    }
    #[cfg(test)]
    pub(super) fn with_desktop_store(
        path: PathBuf,
        store: std::sync::Arc<dyn enzui_services::credential_store::CredentialStore>,
    ) -> Self {
        Self::with_desktop_store_cancellation(
            path,
            store,
            std::sync::Arc::new(std::sync::Mutex::new(None)),
        )
    }
    #[cfg(test)]
    pub(super) fn with_desktop_store_cancellation(
        path: PathBuf,
        store: std::sync::Arc<dyn enzui_services::credential_store::CredentialStore>,
        active: super::desktop::ActiveOperation,
    ) -> Self {
        let mut provider = Self::with_cancellation(path.clone(), std::sync::Arc::clone(&active));
        provider.desktop =
            super::desktop::DesktopUnlock::with_store_cancellation(&path, store, active);
        provider.desktop_enabled = true;
        provider
    }
}
impl CredentialProvider for Builtin {
    fn open_sources(&self) -> Vec<String> {
        if self.vault.is_some() {
            vec!["browser".into()]
        } else {
            Vec::new()
        }
    }
    fn touch_source(&mut self, source: &str) -> Result<()> {
        self.expire();
        if source != "browser" || self.vault.is_none() {
            return Err("Unlock the browser vault first.".into());
        }
        self.touched = Instant::now();
        Ok(())
    }

    fn revocation_epoch(&self) -> u64 {
        self.epoch
    }
    fn authorization_path(&self) -> Option<PathBuf> {
        Some(self.path.clone())
    }
    fn desktop_enabled(&mut self, enabled: bool) {
        self.desktop_enabled = enabled;
    }
    fn desktop_remembered(&self) -> bool {
        self.remembered
    }
    fn desktop_unlock(&mut self) -> Result<bool> {
        if !self.desktop_enabled {
            return Ok(false);
        }
        self.lock();
        super::protect_process()?;
        let Some(secret) = self.desktop.retrieve()? else {
            return Ok(false);
        };
        let vault = Vault::unlock_desktop(&self.path, secret.expose_secret())?;
        self.vault = Some(vault);
        self.touched = Instant::now();
        self.remembered = true;
        Ok(true)
    }
    fn forget_desktop(&mut self) -> Result<()> {
        self.remembered = false;
        self.desktop.forget()
    }
    fn open(&mut self, password: &str, create: bool) -> Result<()> {
        Self::open(self, password, create)
    }
    fn identity(&self) -> Option<enzui_core::state_authority::StateIdentity> {
        Self::identity(self)
    }
    fn timeout(&mut self, minutes: u16) {
        Self::timeout(self, minutes);
    }
    fn idle_deadline(&self) -> Option<Instant> {
        self.vault.as_ref().map(|_| self.touched + self.timeout)
    }
    fn import(&mut self, entries: Vec<Credential>) -> Result<usize> {
        self.vault()?.import(entries)
    }
    fn backup(&mut self, current: &str, password: &str) -> Result<Vec<u8>> {
        self.vault()?.encrypted_export(current, password)
    }
    fn csv(&mut self, current: &str) -> Result<zeroize::Zeroizing<Vec<u8>>> {
        let vault = self.vault()?;
        vault.verify_password(current)?;
        super::exchange::export_csv(vault.entries())
    }
    fn rekey(&mut self, current: &str, password: &str) -> Result<()> {
        let result = self.vault()?.rekey(current, password);
        if result.is_ok() && self.desktop_enabled {
            self.remembered = self
                .desktop
                .remember(self.vault.as_ref().expect("rekeyed vault"));
        }
        self.lock();
        result
    }
    fn lock(&mut self) {
        if self.vault.is_some() {
            self.epoch = self.epoch.wrapping_add(1);
        }
        self.vault = None;
    }
    fn unlocked(&mut self) -> bool {
        self.expire();
        self.vault.is_some()
    }
    fn list(&mut self, site: Option<&Site>) -> Result<Vec<Summary>> {
        Ok(self
            .vault()?
            .entries()
            .iter()
            .filter(|entry| site.is_none_or(|site| entry.site.matches(site)))
            .map(Summary::from)
            .collect())
    }
    fn read(&mut self, id: &str, site: Option<&Site>) -> Result<Credential> {
        self.vault()?
            .entries()
            .iter()
            .find(|entry| entry.id == id && site.is_none_or(|site| entry.site.matches(site)))
            .cloned()
            .ok_or_else(|| "This login is unavailable for this website and container.".into())
    }
    fn save(&mut self, credential: Credential) -> Result<()> {
        let result = self.vault()?.upsert(credential);
        if result.is_err() {
            self.lock();
        }
        result
    }
    fn delete(&mut self, id: &str) -> Result<()> {
        let result = self.vault()?.delete(id);
        if result.is_err() {
            self.lock();
        }
        result
    }
}

#[cfg(test)]
mod idle_tests {
    use super::*;
    #[test]
    fn editor_activity_extends_an_open_vault_without_reviving_an_expired_one() {
        let scratch = super::super::tests::Scratch::new();
        let mut provider = Builtin::with_cancellation(scratch.path(), Default::default());
        provider
            .open("a distinct fixture master password", true)
            .expect("vault");
        provider.touched = Instant::now() - Duration::from_secs(299);
        provider.touch_source("browser").expect("editor activity");
        assert!(
            provider.idle_deadline().expect("deadline") > Instant::now() + Duration::from_secs(298)
        );
        provider.touched = Instant::now() - Duration::from_secs(301);
        assert!(provider.touch_source("browser").is_err());
        assert!(!provider.unlocked());
        assert!(provider.open_sources().is_empty());
    }
}
