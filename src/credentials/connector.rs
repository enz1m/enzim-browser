use super::{
    Credential, Result, Secret, Site,
    accounts::{Authentication, Connection},
};
use base64::Engine;
use enzui_security::cancellation::CancelToken;
use enzui_services::process_transport::{self, ProcessRequest, StdinPolicy};
use serde_json::{Value, json};
use std::{
    cell::{Ref, RefCell},
    os::unix::fs::MetadataExt,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use zeroize::{Zeroize, Zeroizing};

#[derive(Debug)]
pub(super) struct PrivateBytes(enzui_security::secret::Secret);
impl std::ops::Deref for PrivateBytes {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        self.0.expose_secret()
    }
}

pub(super) struct ActiveRequest {
    pub(super) token: CancelToken,
    pub(super) deadline: std::time::Instant,
    pub(super) settings: Option<(PathBuf, enzui_core::state_authority::StateIdentity)>,
}
pub(super) type ActiveProcess = Arc<Mutex<Option<ActiveRequest>>>;

pub(super) fn require_active_request(active: &ActiveProcess) -> Result<()> {
    let active = active
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if active.as_ref().is_some_and(|request| {
        request.token.cause().is_some() || std::time::Instant::now() >= request.deadline
    }) {
        Err("The password-manager request expired or was cancelled. Try again.".into())
    } else {
        Ok(())
    }
}

pub(super) struct Json(Value);
impl std::ops::Deref for Json {
    type Target = Value;
    fn deref(&self) -> &Value {
        &self.0
    }
}
impl std::ops::DerefMut for Json {
    fn deref_mut(&mut self) -> &mut Value {
        &mut self.0
    }
}
impl Drop for Json {
    fn drop(&mut self) {
        wipe_json(&mut self.0);
    }
}
fn wipe_json(value: &mut Value) {
    match value {
        Value::String(value) => value.zeroize(),
        Value::Array(values) => values.iter_mut().for_each(wipe_json),
        Value::Object(values) => values.values_mut().for_each(wipe_json),
        _ => {}
    }
}
impl Json {
    fn parse(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes)
            .map(Self)
            .map_err(|_| "The password-manager connector returned an unsupported response.".into())
    }
    fn bytes(&self) -> Result<Zeroizing<Vec<u8>>> {
        serde_json::to_vec(&self.0)
            .map(Zeroizing::new)
            .map_err(|_| "Could not encode this login for the password manager.".into())
    }
}

pub(super) struct Connector {
    pub(super) connection: Connection,
    pub(super) session: Secret,
    active: ActiveProcess,
    scope: String,
    cache: RefCell<Option<(CacheStamp, Json)>>,
}
type CacheStamp = (u64, u64, u64, i64, i64, i64, i64);
impl Connector {
    pub(super) fn new(
        connection: Connection,
        active: ActiveProcess,
        profile: &std::path::Path,
    ) -> Self {
        use sha2::{Digest, Sha256};
        Self {
            connection,
            session: Secret::default(),
            active,
            scope: format!(
                "{:x}",
                Sha256::digest(profile.as_os_str().as_encoded_bytes())
            ),
            cache: RefCell::new(None),
        }
    }
    pub(super) fn lock(&mut self) {
        self.session = Secret::default();
        self.cache.get_mut().take();
    }
    /// Ends this connection for good: signs the connector's profile out of
    /// the account and removes the profile this browser made for it. A
    /// profile directory the user chose is left where it is.
    pub(super) fn sign_out(&mut self) {
        self.lock();
        let _ = self.run_bounded(&["logout"], None, Duration::from_secs(15));
        if self.connection.directory.is_none() {
            let _ = std::fs::remove_dir_all(self.connection.state_directory());
        }
    }
    fn cache_stamp(&self) -> Result<CacheStamp> {
        let metadata =
            std::fs::symlink_metadata(self.directory()?.join("data.json")).map_err(|_| {
                "The encrypted Bitwarden cache is unavailable. Unlock this account again."
                    .to_owned()
            })?;
        if !metadata.is_file()
            || metadata.uid() != rustix::process::geteuid().as_raw()
            || metadata.nlink() != 1
            || metadata.mode() & 0o077 != 0
        {
            return Err("The encrypted Bitwarden cache has unsafe permissions.".into());
        }
        Ok((
            metadata.dev(),
            metadata.ino(),
            metadata.len(),
            metadata.mtime(),
            metadata.mtime_nsec(),
            metadata.ctime(),
            metadata.ctime_nsec(),
        ))
    }
    fn snapshot(&self) -> Result<Ref<'_, Json>> {
        if self.session.is_empty() {
            return Err("Unlock this Bitwarden account first.".into());
        }
        let stamp = self.cache_stamp()?;
        if self
            .cache
            .borrow()
            .as_ref()
            .is_none_or(|(known, _)| *known != stamp)
        {
            self.cache.borrow_mut().take();
            let mut items = Json::parse(&self.run(&["list", "items"], None)?)?;
            let values = items
                .as_array_mut()
                .ok_or("Bitwarden returned an unsupported login list.")?;
            // Cards, identities and notes are never used here and are not
            // kept decrypted in memory.
            values.retain_mut(|item| {
                let login = item["type"] == 1;
                if !login {
                    wipe_json(item);
                }
                login
            });
            if values.len() > super::MAX_ENTRIES {
                return Err("This password account exceeds the supported login limit.".into());
            }
            *self.cache.borrow_mut() = Some((self.cache_stamp()?, items));
        }
        Ok(Ref::map(self.cache.borrow(), |cache| {
            &cache.as_ref().expect("the bounded snapshot is loaded").1
        }))
    }
    fn directory(&self) -> Result<PathBuf> {
        let directory = self.connection.state_directory();
        enzui_security::secure_dir::ensure_private_dir(&directory)
            .map_err(|_| "The connector profile directory is unsafe or unavailable.".to_owned())?;
        Ok(directory)
    }
    pub(super) fn run(
        &self,
        args: &[&str],
        input: Option<Zeroizing<Vec<u8>>>,
    ) -> Result<PrivateBytes> {
        self.run_bounded(args, input, Duration::from_secs(90))
    }
    fn run_bounded(
        &self,
        args: &[&str],
        input: Option<Zeroizing<Vec<u8>>>,
        timeout: Duration,
    ) -> Result<PrivateBytes> {
        self.run_with_env(args, input, timeout, &[])
    }
    fn run_with_env(
        &self,
        args: &[&str],
        input: Option<Zeroizing<Vec<u8>>>,
        timeout: Duration,
        environment: &[(&str, &str)],
    ) -> Result<PrivateBytes> {
        let executable = if !self.connection.executable.is_empty() {
            PathBuf::from(&self.connection.executable)
        } else {
            super::bundle::bitwarden()?
        };
        let (token, deadline) = self
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .map_or_else(
                || (CancelToken::new(), Duration::from_secs(90)),
                |active| {
                    (
                        active.token.share(),
                        active
                            .deadline
                            .saturating_duration_since(std::time::Instant::now()),
                    )
                },
            );
        if deadline.is_zero() || token.cause().is_some() {
            return Err("The password-manager request expired or was cancelled. Try again.".into());
        }
        let deadline = deadline.min(timeout);
        let input = input.map_or(StdinPolicy::Null, |mut bytes| {
            StdinPolicy::Bytes(std::mem::take(&mut *bytes))
        });
        let mut request =
            ProcessRequest::new(executable, input, super::MAX_BYTES, 32 * 1024, deadline)
                .args(args.iter().copied());
        // Only explicit desktop/network variables enter the connector; ambient account tokens never do.
        for name in [
            "HOME",
            "PATH",
            "LANG",
            "LC_ALL",
            "XDG_RUNTIME_DIR",
            "DBUS_SESSION_BUS_ADDRESS",
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "HTTPS_PROXY",
            "HTTP_PROXY",
            "ALL_PROXY",
            "NO_PROXY",
            "https_proxy",
            "http_proxy",
            "all_proxy",
            "no_proxy",
            "SSL_CERT_FILE",
            "SSL_CERT_DIR",
            "NODE_EXTRA_CA_CERTS",
        ] {
            if let Some(value) = std::env::var_os(name) {
                request = request.env(name, value);
            }
        }
        if std::env::var_os("PATH").is_none() {
            request = request.env("PATH", "/usr/local/bin:/usr/bin:/bin");
        }
        request = request
            .env("BITWARDENCLI_APPDATA_DIR", self.directory()?)
            .env("BW_NOINTERACTION", "true");
        if !self.session.is_empty() {
            request = request.env("BW_SESSION", self.session.expose());
        }
        for (name, value) in environment {
            request = request.env(name, value);
        }
        let result = process_transport::run_sensitive(request, &token);
        let output = result.map_err(|_| {
            "The included Bitwarden connector could not complete. Check your connection and account authorization, then try again.".to_owned()
        })?;
        let bytes = PrivateBytes(output.stdout);
        if !output.status.success() {
            return Err(bitwarden_error(output.stderr.expose_secret()));
        }
        Ok(bytes)
    }
    fn bw_status(&self) -> Result<Json> {
        Json::parse(&self.run(&["status"], None)?)
    }
    fn verify_bw(&self, status: &Value) -> Result<()> {
        let authenticated = match status["status"].as_str() {
            Some("unauthenticated") => false,
            Some("locked" | "unlocked") => true,
            _ => return Err("Bitwarden returned an unsupported account status.".into()),
        };
        let account_matches = status["userEmail"]
            .as_str()
            .is_some_and(|email| email.eq_ignore_ascii_case(&self.connection.account));
        if authenticated && !account_matches
            || status["userEmail"]
                .as_str()
                .is_some_and(|_| !account_matches)
        {
            return Err("The connector profile belongs to a different Bitwarden account. Choose its matching account or a fresh CLI profile.".into());
        }
        let expected = if self.connection.server.is_empty() {
            "https://vault.bitwarden.com"
        } else {
            &self.connection.server
        };
        if status["serverUrl"]
            .as_str()
            .is_none_or(|server| server.trim_end_matches('/') != expected.trim_end_matches('/'))
        {
            return Err("The Bitwarden profile uses a different server. Choose the matching server or a fresh CLI profile.".into());
        }
        Ok(())
    }
    pub(super) fn unlock(&mut self, authentication: &Authentication) -> Result<()> {
        self.lock();
        self.connection.validate()?;
        if self.connection.account.is_empty() {
            return Err("Enter this Bitwarden account's email address.".into());
        }
        if authentication.password.is_empty() {
            return Err("Enter this Bitwarden account's master password.".into());
        }
        let mut status = self.bw_status()?;
        let logged_out = status["status"].as_str() == Some("unauthenticated");
        if logged_out {
            let server = if self.connection.server.is_empty() {
                "https://vault.bitwarden.com"
            } else {
                &self.connection.server
            };
            self.run(&["config", "server", server], None)?;
            status = self.bw_status()?;
        }
        self.verify_bw(&status)?;
        let mut password = Zeroizing::new(authentication.password.expose().as_bytes().to_vec());
        password.push(b'\n');
        let bytes = if logged_out {
            if !authentication.client_id.is_empty() || !authentication.client_secret.is_empty() {
                if !authentication.client_id.starts_with("user.")
                    || authentication.client_id.len() > 256
                    || authentication.client_id.chars().any(char::is_control)
                    || authentication.client_secret.is_empty()
                    || authentication.client_secret.expose().len() > 4096
                    || authentication
                        .client_secret
                        .expose()
                        .chars()
                        .any(char::is_control)
                {
                    return Err("Enter the personal API key's client_id and client_secret from Bitwarden account settings.".into());
                }
                self.run_with_env(
                    &["login", "--apikey", "--raw"],
                    None,
                    Duration::from_secs(90),
                    &[
                        ("BW_CLIENTID", &authentication.client_id),
                        ("BW_CLIENTSECRET", authentication.client_secret.expose()),
                    ],
                )?;
                let status = self.bw_status()?;
                self.verify_bw(&status)?;
                self.run(
                    &["unlock", "--passwordfile", "/dev/stdin", "--raw"],
                    Some(password),
                )?
            } else if !authentication.second_factor.is_empty() {
                let code = authentication.second_factor.expose().trim();
                if code.len() != 6 || !code.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err(
                        "Enter the current six-digit code from your authenticator app.".into(),
                    );
                }
                let mut input = Zeroizing::new(code.as_bytes().to_vec());
                input.push(b'\n');
                // The official interactive prompt reads the code from its private stdin pipe, never argv.
                self.run_with_env(
                    &[
                        "login",
                        &self.connection.account,
                        "--passwordenv",
                        "ENZIM_BITWARDEN_PASSWORD",
                        "--method",
                        "0",
                        "--raw",
                    ],
                    Some(input),
                    Duration::from_secs(90),
                    &[
                        ("ENZIM_BITWARDEN_PASSWORD", authentication.password.expose()),
                        ("BW_NOINTERACTION", "false"),
                    ],
                )?
            } else {
                self.run(
                    &[
                        "login",
                        &self.connection.account,
                        "--passwordfile",
                        "/dev/stdin",
                        "--method",
                        "0",
                        "--raw",
                    ],
                    Some(password),
                )?
            }
        } else {
            self.run(
                &["unlock", "--passwordfile", "/dev/stdin", "--raw"],
                Some(password),
            )?
        };
        let token = std::str::from_utf8(&bytes)
            .map_err(|_| "Bitwarden returned an invalid unlock session.".to_owned())?
            .trim();
        if token.is_empty() || token.len() > 4096 || token.chars().any(char::is_control) {
            return Err("Bitwarden returned an invalid unlock session.".into());
        }
        self.session = Secret::new(token.into());
        let status = self.bw_status()?;
        if let Err(error) = self.verify_bw(&status) {
            self.session = Secret::default();
            return Err(error);
        }
        if status["status"] != "unlocked" {
            self.session = Secret::default();
            return Err("Bitwarden is still locked.".into());
        }
        drop(self.snapshot()?);
        Ok(())
    }
    pub(super) fn resume_bitwarden(&mut self, session: Secret) -> Result<()> {
        self.lock();
        self.session = session;
        let status = self.bw_status();
        let result = status.and_then(|status| {
            self.verify_bw(&status)?;
            if status["status"] == "unlocked" {
                drop(self.snapshot()?);
                Ok(())
            } else {
                Err("The saved Bitwarden session has expired. Unlock this account again.".into())
            }
        });
        if result.is_err() {
            self.lock();
        }
        result
    }
    fn scope_value(&self, container: Option<u64>) -> String {
        container.map_or_else(String::new, |container| {
            format!("{}:{container}", self.scope)
        })
    }
    fn bw_container(&self, item: &Value) -> Option<Option<u64>> {
        let scope = item["fields"]
            .as_array()
            .and_then(|fields| {
                fields
                    .iter()
                    .find(|field| field["name"] == "enzim.container")
            })
            .and_then(|field| field["value"].as_str())
            .unwrap_or_default();
        if scope.is_empty() {
            Some(None)
        } else {
            let (profile, id) = scope.split_once(':')?;
            if profile != self.scope {
                return None;
            }
            Some(Some(id.parse().ok()?))
        }
    }
    fn bw_site(&self, item: &Value, wanted: Option<&Site>) -> Option<Site> {
        if item["type"] != 1 || item["deletedDate"].as_str().is_some() {
            return None;
        }
        let container = self.bw_container(item)?;
        item["login"]["uris"]
            .as_array()?
            .iter()
            .find_map(|uri| uri_site(uri, container, wanted))
    }
    fn bw_credential(&self, item: &Value, wanted: Option<&Site>) -> Result<Credential> {
        self.bw_login(item, wanted, false)
    }
    fn bw_login(&self, item: &Value, wanted: Option<&Site>, selected: bool) -> Result<Credential> {
        if item["type"] != 1 || item["deletedDate"].as_str().is_some() {
            return Err("This item is not an available login.".into());
        }
        let container = self
            .bw_container(item)
            .ok_or("This login belongs to a different browser profile.")?;
        if item["reprompt"]
            .as_u64()
            .is_some_and(|reprompt| reprompt != 0)
        {
            return Err(
                "This protected login requires reauthentication in Bitwarden itself.".into(),
            );
        }
        let site = self
            .bw_site(item, wanted)
            .or_else(|| {
                (selected && wanted.is_none()).then(|| Site {
                    origin: String::new(),
                    container,
                    url: None,
                })
            })
            .ok_or("This login is unavailable for this HTTPS website and container.")?;
        let mut entry = Credential::new(site)?;
        entry.id = item["id"]
            .as_str()
            .ok_or("Bitwarden returned a login without an identity.")?
            .into();
        entry.name = secret(&item["name"]);
        entry.username = secret(&item["login"]["username"]);
        entry.password = secret(&item["login"]["password"]);
        entry.notes = secret(&item["notes"]);
        if let Some(uri) = item["login"]["uris"].as_array().and_then(|uris| {
            uris.iter()
                .find(|uri| uri_site(uri, entry.site.container, wanted).is_some())
        }) {
            let original = secret(&uri["uri"]);
            entry.remote_uri = Some(super::model::RemoteUri {
                edited: original.clone(),
                original,
            });
        }
        if selected
            && entry.remote_uri.is_none()
            && let Some(original) = item["login"]["uris"]
                .as_array()
                .into_iter()
                .flatten()
                .find_map(|uri| {
                    let value = uri["uri"].as_str()?;
                    let url = url::Url::parse(value).ok()?;
                    (matches!(url.scheme(), "http" | "https")
                        && url.username().is_empty()
                        && url.password().is_none())
                    .then(|| Secret::new(value.into()))
                })
        {
            entry.remote_uri = Some(super::model::RemoteUri {
                original: original.clone(),
                edited: original,
            });
        }
        // An authenticator field this browser cannot read (a Steam secret, an
        // unknown parameter) leaves the login usable without codes. Saving
        // writes the field only when it was changed here, so it is kept.
        entry.totp = item["login"]["totp"]
            .as_str()
            .filter(|value| !value.is_empty())
            .and_then(|value| super::Totp::parse(value).ok());
        entry.passkey = item["login"]["fido2Credentials"]
            .as_array()
            .into_iter()
            .flatten()
            .find_map(super::passkey::Passkey::from_bitwarden)
            .map(Box::new);
        if selected {
            entry.validate_contents()?;
        } else {
            entry.validate()?;
        }
        Ok(entry)
    }
    pub(super) fn list(&self, site: Option<&Site>) -> Result<Vec<super::provider::Summary>> {
        let items = self.snapshot()?;
        let items = items
            .as_array()
            .ok_or("Bitwarden returned an unsupported login list.")?;
        if items.len() > super::MAX_ENTRIES {
            return Err("This password account exceeds the supported login limit.".into());
        }
        Ok(items
            .iter()
            .filter_map(|item| {
                let entry = self.bw_login(item, site, site.is_none()).ok()?;
                let mut summary = super::provider::Summary::from(&entry);
                summary.uri_rules = Some(
                    item["login"]["uris"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|uri| {
                            let pattern = &*with_scheme(uri["uri"].as_str()?);
                            if !uri["match"].is_null() && uri["match"].as_u64().is_none() {
                                return None;
                            }
                            let url = url::Url::parse(pattern).ok()?;
                            if !matches!(url.scheme(), "http" | "https")
                                || !url.username().is_empty()
                                || url.password().is_some()
                                || pattern.len() > 8192
                            {
                                return None;
                            }
                            Some((Secret::new(pattern.into()), uri["match"].as_u64()))
                        })
                        .collect(),
                );
                Some(summary)
            })
            .collect())
    }
    pub(super) fn read(&self, id: &str, site: Option<&Site>) -> Result<Credential> {
        valid_id(id)?;
        let items = self.snapshot()?;
        let item = items
            .as_array()
            .and_then(|items| items.iter().find(|item| item["id"] == id))
            .ok_or("This Bitwarden login is no longer available. Refresh the password manager.")?;
        self.bw_credential(item, site)
    }
    pub(super) fn read_selected(&self, id: &str) -> Result<Credential> {
        valid_id(id)?;
        let items = self.snapshot()?;
        let item = items
            .as_array()
            .and_then(|items| items.iter().find(|item| item["id"] == id))
            .ok_or("This login is no longer available. Refresh the password manager.")?;
        self.bw_login(item, None, true)
    }
    pub(super) fn synchronize(&self) -> Result<()> {
        self.run_bounded(&["sync"], None, Duration::from_secs(12))?;
        self.cache.borrow_mut().take();
        Ok(())
    }
    pub(super) fn save(&self, entry: Credential) -> Result<()> {
        entry.validate()?;
        let updating = entry.id.len() == 36
            && entry
                .id
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() || byte == b'-');
        // What the user was shown: the login as the snapshot had it before
        // the synchronization below replaces it.
        let shown = if updating {
            self.cache.borrow().as_ref().and_then(|(_, items)| {
                let item = items.as_array()?.iter().find(|item| item["id"] == *entry.id)?;
                self.bw_login(item, None, true).ok()
            })
        } else {
            None
        };
        let mut item = if updating {
            self.synchronize().map_err(|_| UNREACHABLE_FOR_EDIT.to_owned())?;
            Json::parse(&self.run(&["get", "item", &entry.id], None)?)?
        } else {
            Json(
                json!({"type":1,"name":"","notes":null,"login":{"uris":[],"username":"","password":"","totp":null},"fields":[],"favorite":false}),
            )
        };
        if updating {
            self.bw_credential(&item, None)?;
            if let Some(uri) = &entry.remote_uri {
                let selected = item["login"]["uris"].as_array_mut()
                    .and_then(|uris| uris.iter_mut().find(|value| value["uri"].as_str() == Some(uri.original.expose())))
                    .ok_or("This website was changed in Bitwarden. Refresh the login before editing it.")?;
                if uri.edited != uri.original {
                    replace(
                        &mut selected["uri"],
                        Value::String(uri.edited.expose().into()),
                    );
                    replace(&mut selected["uriChecksum"], Value::Null);
                }
            } else {
                self.bw_credential(&item, Some(&entry.site))?;
            }
        }
        let passkey = merge_login(&mut item, shown.as_ref(), &entry)?;
        if !updating {
            item["login"]["uris"] = json!([{"uri":stored_website(&entry),"match":1}]);
        }
        let mut fields = std::mem::take(&mut item["fields"])
            .as_array_mut()
            .map(std::mem::take)
            .unwrap_or_default();
        fields.retain_mut(|field| {
            if field["name"] == "enzim.container" {
                wipe_json(field);
                false
            } else {
                true
            }
        });
        if entry.site.container.is_some() {
            fields.push(json!({"name":"enzim.container","value":self.scope_value(entry.site.container),"type":0}));
        }
        item["fields"] = Value::Array(fields);
        let payload = Zeroizing::new(
            base64::engine::general_purpose::STANDARD
                .encode(&*item.bytes()?)
                .into_bytes(),
        );
        let args = if updating {
            vec!["edit", "item", &entry.id]
        } else {
            vec!["create", "item"]
        };
        let saved = Json::parse(&self.run(&args, Some(payload))?)?;
        let verified = self.bw_credential(&saved, None)?;
        let expected_uri = if updating {
            entry.website()
        } else {
            stored_website(&entry)
        };
        let website_saved = saved["login"]["uris"].as_array().is_some_and(|uris| {
            uris.iter()
                .any(|uri| uri["uri"].as_str() == Some(expected_uri.expose()))
        });
        if passkey && verified.passkey != entry.passkey {
            // A connector that drops the passkey must not leave a login that
            // claims to hold one; a login made just now is taken back.
            if !updating && let Some(id) = saved["id"].as_str().filter(|id| valid_id(id).is_ok()) {
                let _ = self.run(&["delete", "item", id], None);
            }
            self.cache.borrow_mut().take();
            return Err("Bitwarden did not keep the passkey. Nothing was saved.".into());
        }
        if verified.password != entry.password
            || verified.username != entry.username
            || verified.site.container != entry.site.container
            || !website_saved
        {
            return Err("Bitwarden returned different login data. Refresh before retrying.".into());
        }
        self.cache.borrow_mut().take();
        Ok(())
    }
    pub(super) fn delete(&self, id: &str) -> Result<()> {
        valid_id(id)?;
        self.run(&["delete", "item", id], None)?;
        self.cache.borrow_mut().take();
        Ok(())
    }
}
/// The website a new login is saved under: the page's address without its
/// query and fragment, which can carry sign-in state or one-time tokens.
fn stored_website(entry: &Credential) -> Secret {
    let website = entry.website();
    match url::Url::parse(website.expose()) {
        Ok(mut url) => {
            url.set_query(None);
            url.set_fragment(None);
            Secret::new(url.into())
        }
        Err(_) => website,
    }
}
/// What an edit answers when the account cannot be synchronized first. The
/// account stays unlocked: nothing was written and nothing is in doubt.
pub(super) const UNREACHABLE_FOR_EDIT: &str = "Bitwarden could not be reached to check for newer changes. Nothing was saved. Try again when you are online.";
/// Writes `entry` into a Bitwarden item, and says whether a passkey was
/// written. An existing item takes only the fields that differ from `shown`,
/// the login as the user saw it: a field edited on another device since then
/// keeps what that device wrote. Without `shown` nothing can be told apart,
/// and every field the user saved is written.
fn merge_login(item: &mut Value, shown: Option<&Credential>, entry: &Credential) -> Result<bool> {
    let changed =
        |field: fn(&Credential) -> &Secret| shown.is_none_or(|shown| field(shown) != field(entry));
    if changed(|login| &login.name) {
        replace(&mut item["name"], Value::String(entry.name.expose().into()));
    }
    if changed(|login| &login.notes) {
        replace(
            &mut item["notes"],
            Value::String(entry.notes.expose().into()),
        );
    }
    if changed(|login| &login.username) {
        replace(
            &mut item["login"]["username"],
            Value::String(entry.username.expose().into()),
        );
    }
    if changed(|login| &login.password) {
        replace(
            &mut item["login"]["password"],
            Value::String(entry.password.expose().into()),
        );
    }
    if shown.is_none_or(|shown| shown.totp != entry.totp) {
        replace(
            &mut item["login"]["totp"],
            entry.totp.as_ref().map_or(Value::Null, |totp| {
                Value::String(totp.uri_secret().expose().into())
            }),
        );
    }
    let Some(passkey) = entry
        .passkey
        .as_ref()
        .filter(|passkey| shown.is_none_or(|shown| shown.passkey.as_ref() != Some(*passkey)))
    else {
        return Ok(false);
    };
    let ours = passkey.to_bitwarden();
    let held = item["login"]["fido2Credentials"]
        .as_array_mut()
        .map(std::mem::take)
        .unwrap_or_default();
    // A passkey the item already holds is never replaced by another.
    if held
        .iter()
        .any(|other| other["credentialId"] != ours["credentialId"])
    {
        item["login"]["fido2Credentials"] = Value::Array(held);
        return Err("This Bitwarden login already has a passkey. Save the new one as a separate login.".into());
    }
    item["login"]["fido2Credentials"] = Value::Array(vec![ours]);
    Ok(true)
}
fn uri_site(uri: &Value, container: Option<u64>, wanted: Option<&Site>) -> Option<Site> {
    let pattern = uri["uri"].as_str()?;
    if wanted.is_some() && !uri["match"].is_null() && uri["match"].as_u64().is_none() {
        return None;
    }
    uri_pattern_site(pattern, uri["match"].as_u64(), container, wanted)
}

pub(super) fn uri_pattern_site(
    pattern: &str,
    mode: Option<u64>,
    container: Option<u64>,
    wanted: Option<&Site>,
) -> Option<Site> {
    let pattern = &*with_scheme(pattern);
    let site = Site::parse(pattern, container).ok()?;
    let Some(wanted) = wanted else {
        return Some(site);
    };
    if site != *wanted {
        return None;
    }
    let root = format!("{}/", wanted.origin);
    let target = wanted.url.as_ref().map_or(root.as_str(), Secret::expose);
    let normalized = url::Url::parse(pattern).ok()?;
    let matched = match mode {
        None => true,
        Some(0 | 1) => true,
        Some(2) => target.starts_with(normalized.as_str()),
        Some(3) => target == normalized.as_str(),
        _ => false,
    };
    matched.then(|| wanted.clone())
}

/// A saved website as a URL: Bitwarden keeps what the user typed, and
/// `example.com` there means the HTTPS site.
fn with_scheme(pattern: &str) -> std::borrow::Cow<'_, str> {
    let pattern = pattern.trim();
    if pattern.is_empty() || pattern.contains("://") {
        pattern.into()
    } else {
        format!("https://{pattern}").into()
    }
}

fn bitwarden_error(stderr: &[u8]) -> String {
    let contains = |text: &str| {
        stderr
            .windows(text.len())
            .any(|bytes| bytes.eq_ignore_ascii_case(text.as_bytes()))
    };
    if contains("code is required") || contains("no provider selected") {
        "Bitwarden requires verification. Enter the current code from your authenticator app. For email, security-key or other verification, choose Personal API key sign-in.".into()
    } else if contains("totp")
        || contains("two-step token is invalid")
        || contains("two-step login code is invalid")
    {
        "Bitwarden rejected the verification code. Enter a fresh authenticator code and try again."
            .into()
    } else if contains("bot") || contains("captcha") || contains("additional authentication") {
        "Bitwarden requires an additional authentication challenge. Choose Personal API key sign-in; your master password is still needed to unlock the vault.".into()
    } else if contains("username or password")
        || contains("invalid master password")
        || contains("invalid password")
    {
        "Bitwarden rejected the account email or master password. Check the account's US, EU or self-hosted server as well.".into()
    } else if contains("client_id") || contains("client_secret") || contains("invalid_client") {
        "Bitwarden rejected the personal API key. Check its client_id and client_secret in your Bitwarden account settings.".into()
    } else if contains("no providers available") {
        "This Bitwarden two-step method is not supported by its connector. Choose Personal API key sign-in.".into()
    } else {
        "Bitwarden could not complete sign-in or the requested operation. Check the account server and connection, then try again. If your account uses two-step login, enter its authenticator code or choose Personal API key sign-in.".into()
    }
}
fn replace(target: &mut Value, value: Value) {
    wipe_json(target);
    *target = value;
}
fn secret(value: &Value) -> Secret {
    Secret::new(value.as_str().unwrap_or_default().into())
}
fn valid_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.starts_with('-')
        || id.len() > 128
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        Err("The password-manager item identity is invalid.".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::{accounts::Kind, tests::Scratch};
    #[test]
    fn password_review_manual_bitwarden_selection_supports_missing_and_legacy_urls_without_automatic_matching()
     {
        let scratch = Scratch::new();
        let connector = Connector::new(
            Connection::new(Kind::Bitwarden, "Review").expect("connection"),
            Default::default(),
            &scratch.path(),
        );
        let mut item = Json(
            json!({"id":"review-item","type":1,"name":"Account without a URL","fields":[],"login":{"uris":[],"username":"review-user","password":"synthetic-password"}}),
        );
        assert!(connector.bw_credential(&item, None).is_err());
        let login = connector
            .bw_login(&item, None, true)
            .expect("explicit selection of an unbound login");
        assert!(login.site.origin.is_empty());
        assert_eq!(login.password.expose(), "synthetic-password");
        assert!(
            login.validate().is_err(),
            "unbound records cannot be saved as browser logins"
        );
        let site = Site::parse("https://signin.example.test", None).expect("target");
        let document = crate::credential_bridge::Document {
            document: "0123456789abcdef0123456789abcdef".into(),
            origin: site.origin.clone(),
            target: "1".into(),
            username_only: false,
        };
        assert!(crate::credential_bridge::fill_script(&document, &site, &login, false).is_err());
        assert!(
            crate::credential_bridge::fill_selected_script(&document, &site, &login, false).is_ok()
        );
        item["login"]["uris"] = json!([{"uri":"http://legacy.example.test/login"}]);
        let legacy = connector
            .bw_login(&item, None, true)
            .expect("legacy URL is manually accessible");
        assert!(legacy.site.origin.is_empty());
        assert_eq!(
            legacy.website().expose(),
            "http://legacy.example.test/login"
        );
        item["fields"] = json!([{"name":"enzim.container","value":"another-profile:7"}]);
        assert!(
            connector.bw_login(&item, None, true).is_err(),
            "manual access does not cross profiles"
        );
        item["fields"] = json!([]);
        item["reprompt"] = json!(1);
        assert!(
            connector.bw_login(&item, None, true).is_err(),
            "protected items still require provider reauthentication"
        );
        item["reprompt"] = json!(0);
        item["login"]["password"] = json!("x".repeat(4097));
        assert!(
            connector.bw_login(&item, None, true).is_err(),
            "manual reads retain secret bounds"
        );
    }
    #[test]
    fn unlocked_bitwarden_snapshot_serves_matches_and_secrets_without_a_helper_and_retires_on_change()
     {
        use std::os::unix::fs::PermissionsExt;
        let scratch = Scratch::new();
        let mut connection =
            Connection::new(Kind::Bitwarden, "Cached fixture").expect("connection");
        connection.executable = "/bin/false".into();
        connection.directory = Some(scratch.path().with_file_name("cached-profile"));
        let mut connector = Connector::new(connection, Default::default(), &scratch.path());
        let data = connector.directory().expect("directory").join("data.json");
        std::fs::write(&data, b"encrypted-placeholder").expect("cache file");
        std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o600))
            .expect("private cache");
        connector.session = Secret::new("synthetic-session".into());
        *connector.cache.borrow_mut() = Some((
            connector.cache_stamp().expect("stamp"),
            Json(json!([
                {"id":"fixture","type":1,"name":"Cached login","fields":[],"login":{"uris":[{"uri":"https://example.test/login","match":null}],"username":"user","password":"private"}}
            ])),
        ));
        let site = Site::parse("https://example.test/reset-password", None).expect("site");
        assert_eq!(connector.list(Some(&site)).expect("no helper").len(), 1);
        assert_eq!(
            connector
                .read("fixture", Some(&site))
                .expect("no helper")
                .username
                .expose(),
            "user"
        );
        assert!(
            connector
                .list(Some(
                    &Site::parse("https://other.test", None).expect("other")
                ))
                .expect("negative match")
                .is_empty()
        );
        assert!(
            connector
                .list(Some(
                    &Site::parse("https://example.test", Some(9)).expect("container")
                ))
                .expect("container scope")
                .is_empty()
        );
        std::fs::write(&data, b"replaced-encrypted-cache").expect("external change");
        assert!(
            connector.list(Some(&site)).is_err(),
            "changed encrypted data requires a fresh helper read"
        );
        assert!(connector.cache.borrow().is_none());
        connector.lock();
        assert!(connector.read("fixture", Some(&site)).is_err());
    }
    #[test]
    fn cached_uri_matching_respects_exact_prefix_never_and_origin_boundaries() {
        let wanted = Site::parse("https://example.test/login/step?mode=one", None).expect("site");
        for (pattern, mode, matches) in [
            ("https://example.test/login/step?mode=one", 3, true),
            ("https://example.test/login", 3, false),
            ("https://example.test/login/", 2, true),
            ("https://example.test/logins/", 2, false),
            ("https://example.test", 5, false),
            ("https://example.test", 4, false),
            ("https://example.test", 99, false),
            ("https://example.test", 1, true),
            ("https://other.example.test", 0, false),
            ("https://example.test:444", 1, false),
        ] {
            let value = json!({"uri":pattern,"match":mode});
            assert_eq!(
                uri_site(&value, None, Some(&wanted)).is_some(),
                matches,
                "{pattern}, {mode}"
            );
        }
        assert!(
            uri_site(&json!({"uri":"https://example.test","match":5}), None, None).is_some(),
            "a Never rule remains editable in the manager"
        );
    }
    #[test]
    #[ignore = "requires an isolated Vaultwarden account and the bundled Bitwarden connector"]
    fn real_bitwarden_cached_reads_and_uri_edit_preserve_other_data() {
        let fixture_path = std::env::var_os("ENZIM_CONNECTOR_FIXTURE").expect("fixture path");
        let fixture = Json::parse(&Zeroizing::new(
            std::fs::read(fixture_path).expect("fixture"),
        ))
        .expect("json");
        let account = &fixture["accounts"][0];
        let scratch = Scratch::new();
        let mut connection =
            Connection::new(Kind::Bitwarden, "Cache and edit").expect("connection");
        connection.account = account["email"].as_str().expect("email").into();
        connection.server = fixture["server"].as_str().expect("server").into();
        connection.directory = Some(scratch.path().with_file_name("cache-edit-profile"));
        let mut connector = Connector::new(connection, Default::default(), &scratch.path());
        connector
            .unlock(&Authentication {
                password: secret(&account["password"]),
                settings_identity: None,
                client_id: String::new(),
                client_secret: Secret::default(),
                second_factor: Secret::default(),
            })
            .expect("official sign-in");
        let old = Site::parse("https://enzim-cache.example/login", None).expect("site");
        let mut login = Credential::new(old.clone()).expect("login");
        login.username = Secret::new(format!("cache-fixture-{}", login.id));
        login.password = Secret::new("Synthetic cache fixture password".into());
        connector.save(login.clone()).expect("create");
        let id = connector
            .list(Some(&old))
            .expect("list")
            .into_iter()
            .find(|entry| entry.username == login.username)
            .expect("created login")
            .id;
        let mut raw = Json::parse(
            &connector
                .run(&["get", "item", &id], None)
                .expect("raw item"),
        )
        .expect("json");
        raw["login"]["uris"] = json!([{"uri":"https://enzim-cache.example/login","match":3},{"uri":"https://secondary.example/keep","match":1}]);
        raw["fields"] = json!([{"name":"Unrelated field","value":"Keep this value","type":0}]);
        raw["login"]["fido2Credentials"] = json!([{"credentialId":"fixture-passkey-id","keyType":"public-key","keyAlgorithm":"ECDSA","keyCurve":"P-256","keyValue":"synthetic-key","rpId":"enzim-cache.example","userHandle":"fixture-handle","userName":"fixture-user","counter":"0","rpName":"Fixture","userDisplayName":"Fixture","discoverable":"true","creationDate":"2026-01-01T00:00:00.000Z"}]);
        let payload = Zeroizing::new(
            base64::engine::general_purpose::STANDARD
                .encode(&*raw.bytes().expect("bytes"))
                .into_bytes(),
        );
        let seeded = Json::parse(
            &connector
                .run(&["edit", "item", &id], Some(payload))
                .expect("seed unrelated data"),
        )
        .expect("json");
        connector.cache.borrow_mut().take();
        let mut edited = connector.read(&id, None).expect("editable full URI");
        assert_eq!(
            edited.website().expose(),
            "https://enzim-cache.example/login"
        );
        let executable =
            std::mem::replace(&mut connector.connection.executable, "/bin/false".into());
        let started = std::time::Instant::now();
        for _ in 0..100 {
            assert!(
                connector
                    .list(Some(&old))
                    .expect("cached list")
                    .iter()
                    .any(|entry| entry.id == id)
            );
            assert_eq!(
                connector
                    .read(&id, Some(&old))
                    .expect("cached secret")
                    .password,
                login.password
            );
        }
        let millis = started.elapsed().as_secs_f64() * 1000.;
        assert!(millis < 1000., "100 list/read pairs took {millis} ms");
        connector.connection.executable = executable;
        let new_url = "https://changed-cache.example/reset-password/index.html";
        edited.set_website(new_url).expect("new URL");
        connector.save(edited).expect("edit URL");
        let saved = Json::parse(
            &connector
                .run(&["get", "item", &id], None)
                .expect("saved raw item"),
        )
        .expect("json");
        assert_eq!(saved["login"]["uris"][0]["uri"], new_url);
        assert_eq!(saved["login"]["uris"][0]["match"], 3);
        assert_eq!(saved["login"]["uris"][1], seeded["login"]["uris"][1]);
        assert_eq!(saved["fields"], seeded["fields"]);
        assert_eq!(
            saved["login"]["fido2Credentials"],
            seeded["login"]["fido2Credentials"]
        );
        assert!(
            connector
                .list(Some(&old))
                .expect("old site retired")
                .iter()
                .all(|entry| entry.id != id)
        );
        let changed = Site::parse(new_url, None).expect("new site");
        assert!(
            connector
                .list(Some(&changed))
                .expect("new site indexed")
                .iter()
                .any(|entry| entry.id == id)
        );
        let bytes = Zeroizing::new(
            std::fs::read(connector.directory().expect("directory").join("data.json"))
                .expect("encrypted cache"),
        );
        assert!(
            !bytes
                .windows(login.password.expose().len())
                .any(|part| part == login.password.expose().as_bytes())
        );
        eprintln!(
            "BITWARDEN_CACHE_RESULT {}",
            json!({"list_read_pairs":100,"milliseconds":millis,"helper_disabled_during_reads":true,"url_edit_preserved_secondary_uri_fields_passkey":true})
        );
        connector.delete(&id).expect("cleanup");
        connector.lock();
        assert!(connector.cache.borrow().is_none());
    }
    #[test]
    fn bitwarden_authentication_errors_are_actionable_and_redacted() {
        for (message, expected) in [
            ("Code is required.", "requires verification"),
            ("Two-step token is invalid.", "fresh authenticator code"),
            (
                "Your authentication request appears to be coming from a bot.",
                "authentication challenge",
            ),
            ("Username or password is incorrect.", "US, EU"),
            ("invalid_client", "personal API key"),
            ("No providers available for this client.", "not supported"),
            ("Unrecognised private server response", "could not complete"),
        ] {
            let error = bitwarden_error(format!("{message}\nprivate-response-sentinel").as_bytes());
            assert!(error.contains(expected), "{expected}");
            assert!(!error.contains("private-response-sentinel"));
            assert!(!error.contains("Unrecognised private"));
        }
    }
    #[test]
    #[ignore = "requires an isolated TOTP-enabled Vaultwarden fixture and the bundled Bitwarden connector"]
    fn real_bitwarden_authenticator_and_api_key_sign_in() {
        let file = std::env::var_os("ENZIM_CONNECTOR_FIXTURE").expect("private fixture path");
        let bytes = Zeroizing::new(std::fs::read(file).expect("fixture"));
        let fixture = Json::parse(&bytes).expect("json");
        let account = &fixture["accounts"][1];
        let scratch = Scratch::new();
        let connection = |label: &str| {
            let mut connection = Connection::new(Kind::Bitwarden, label).expect("connection");
            connection.account = account["email"].as_str().expect("email").into();
            connection.server = fixture["server"].as_str().expect("server").into();
            connection.directory = Some(scratch.path().with_file_name(label));
            connection.remember = false;
            connection
        };
        let auth = || Authentication {
            password: secret(&account["password"]),
            settings_identity: None,
            client_id: String::new(),
            client_secret: Secret::default(),
            second_factor: Secret::default(),
        };
        let mut connector = Connector::new(connection("totp"), Default::default(), &scratch.path());
        let error = connector.unlock(&auth()).expect_err("TOTP is required");
        assert!(error.contains("requires verification"));
        let mut invalid = auth();
        invalid.second_factor = Secret::new("invalid-code".into());
        assert!(
            connector
                .unlock(&invalid)
                .expect_err("invalid syntax")
                .contains("six-digit")
        );
        invalid.second_factor = Secret::new("000000".into());
        assert!(
            connector
                .unlock(&invalid)
                .expect_err("wrong TOTP")
                .contains("verification code")
        );
        let mut valid = auth();
        let key = fixture["authenticator_secret"]
            .as_str()
            .expect("fixture seed");
        valid.second_factor = super::super::Totp::parse(key)
            .expect("TOTP")
            .code(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("clock")
                    .as_secs(),
            )
            .expect("code")
            .0;
        connector
            .unlock(&valid)
            .expect("actual authenticator sign-in");
        assert_eq!(connector.bw_status().expect("status")["status"], "unlocked");
        let mut api = Connector::new(connection("api"), Default::default(), &scratch.path());
        let mut valid = auth();
        valid.client_id = fixture["client_id"].as_str().expect("client id").into();
        valid.client_secret = secret(&fixture["client_secret"]);
        api.unlock(&valid)
            .expect("actual API-key sign-in and master-password unlock");
        assert_eq!(api.bw_status().expect("status")["status"], "unlocked");
    }
    #[test]
    fn bitwarden_unlock_status_requires_the_named_account_and_server() {
        let scratch = Scratch::new();
        let mut connection = Connection::new(Kind::Bitwarden, "Personal").expect("connection");
        connection.account = "fixture@example.test".into();
        let connector = Connector::new(connection, Default::default(), &scratch.path());
        for state in ["locked", "unlocked"] {
            let valid = json!({"status":state,"userEmail":"fixture@example.test","serverUrl":"https://vault.bitwarden.com/"});
            assert!(connector.verify_bw(&valid).is_ok());
            for field in ["status", "userEmail", "serverUrl"] {
                let mut missing = valid.clone();
                missing.as_object_mut().expect("object").remove(field);
                assert!(connector.verify_bw(&missing).is_err(), "missing {field}");
            }
            for (field, value) in [
                ("userEmail", "another@example.test"),
                ("serverUrl", "https://another.example.test"),
                ("status", "unknown"),
            ] {
                let mut wrong = valid.clone();
                wrong[field] = Value::String(value.into());
                assert!(connector.verify_bw(&wrong).is_err(), "wrong {field}");
            }
        }
        assert!(connector.verify_bw(&json!({"status":"unauthenticated","userEmail":null,"serverUrl":"https://vault.bitwarden.com"})).is_ok());
    }
    #[test]
    fn sensitive_transport_wipes_owned_requests_successes_and_failed_captures() {
        use enzui_security::secret::observer;
        observer::reset();
        let request = ProcessRequest::new(
            "/bin/cat",
            StdinPolicy::Bytes(b"input-fixture".to_vec()),
            64,
            64,
            Duration::from_secs(2),
        )
        .arg("argument-fixture")
        .env("SYNTHETIC_SECRET", "environment-fixture");
        drop(request);
        let expected =
            b"input-fixture".len() + b"argument-fixture".len() + b"environment-fixture".len();
        assert_eq!(observer::snapshot().wiped_bytes, expected);
        assert_eq!(observer::snapshot().zeroed_bytes, expected);
        let output = process_transport::run_sensitive(
            ProcessRequest::new(
                "/bin/cat",
                StdinPolicy::Bytes(b"output-fixture".to_vec()),
                64,
                64,
                Duration::from_secs(2),
            ),
            &CancelToken::new(),
        )
        .expect("bounded native helper");
        assert!(output.status.success());
        assert_eq!(output.stdout.expose_secret(), b"output-fixture");
        assert!(!format!("{output:?}").contains("output-fixture"));
        observer::reset();
        drop(output);
        assert_eq!(observer::snapshot().zeroed_bytes, b"output-fixture".len());
        let error = process_transport::run_sensitive(
            ProcessRequest::new(
                "/bin/cat",
                StdinPolicy::Bytes(b"overflow-fixture".to_vec()),
                3,
                64,
                Duration::from_secs(2),
            ),
            &CancelToken::new(),
        )
        .expect_err("bounded failure");
        let retained = error.stdout().len() + error.stderr().len();
        assert!(retained > 0);
        observer::reset();
        drop(error);
        assert_eq!(observer::snapshot().zeroed_bytes, retained);
    }
    #[test]
    fn expired_or_cancelled_connector_requests_never_spawn_another_helper() {
        let scratch = Scratch::new();
        let token = CancelToken::new();
        let active = Arc::new(Mutex::new(Some(ActiveRequest {
            token: token.share(),
            deadline: std::time::Instant::now() - Duration::from_secs(1),
            settings: None,
        })));
        let mut connection =
            Connection::new(Kind::Bitwarden, "Deadline fixture").expect("connection");
        connection.executable = "/bin/cat".into();
        connection.directory = Some(scratch.path().with_file_name("cli"));
        let connector = Connector::new(connection, Arc::clone(&active), &scratch.path());
        assert!(
            connector
                .run(&[], None)
                .expect_err("expired")
                .contains("expired")
        );
        active
            .lock()
            .expect("context")
            .as_mut()
            .expect("request")
            .deadline = std::time::Instant::now() + Duration::from_secs(90);
        assert_eq!(
            &*connector.run(&[], None).expect("allowed fresh request"),
            b""
        );
        token.cancel(enzui_security::cancellation::CancelCause::Cancelled);
        assert!(
            connector
                .run(&[], None)
                .expect_err("cancelled")
                .contains("cancelled")
        );
    }
    #[test]
    fn an_edit_writes_only_what_changed_since_the_login_was_shown() {
        let site = Site::parse("https://example.test/", None).expect("site");
        let mut shown = Credential::new(site).expect("login");
        shown.name = Secret::new("Example".into());
        shown.username = Secret::new("alex".into());
        shown.password = Secret::new("first password".into());
        shown.notes = Secret::new("old note".into());
        // What another device saved since: a note and an authenticator seed.
        let newer = || {
            json!({"type":1,"name":"Example","notes":"note from the phone","login":{
                "username":"alex","password":"first password",
                "totp":"otpauth://totp/Example:alex?secret=JBSWY3DPEHPK3PXP&issuer=Example"}})
        };
        let mut entry = shown.clone();
        entry.password = Secret::new("second password".into());
        let mut item = newer();
        assert!(!merge_login(&mut item, Some(&shown), &entry).expect("merge"));
        assert_eq!(item["login"]["password"], "second password");
        assert_eq!(item["notes"], "note from the phone", "the other device's note stays");
        assert_eq!(
            item["login"]["totp"],
            "otpauth://totp/Example:alex?secret=JBSWY3DPEHPK3PXP&issuer=Example",
            "its authenticator seed stays, label and issuer included"
        );
        // A field the user did change is written, also when emptied.
        entry.notes = Secret::default();
        let mut item = newer();
        let _ = merge_login(&mut item, Some(&shown), &entry).expect("merge");
        assert_eq!(item["notes"], "");
        // A login saved without having been shown writes everything.
        let mut item = newer();
        let _ = merge_login(&mut item, None, &shown).expect("merge");
        assert_eq!(item["notes"], "old note");
        assert!(item["login"]["totp"].is_null());
    }
    #[test]
    fn saved_websites_without_a_scheme_match_and_new_ones_drop_query_and_fragment() {
        let wanted = Site::parse("https://example.test/login", None).expect("site");
        for (pattern, mode, matches) in [
            ("example.test", None, true),
            ("example.test/login", Some(2), true),
            ("https://example.test", Some(0), true),
            ("other.test", None, false),
            ("example.test", Some(5), false),
        ] {
            assert_eq!(
                uri_pattern_site(pattern, mode, None, Some(&wanted)).is_some(),
                matches,
                "{pattern} with match {mode:?}"
            );
        }
        let entry = Credential::new(
            Site::parse("https://example.test/login?state=abc&token=1#step", None).expect("site"),
        )
        .expect("login");
        assert_eq!(stored_website(&entry).expose(), "https://example.test/login");
        assert!(valid_id("-option").is_err());
        assert!(valid_id("0a7a603b-ee96-4582-8d63-39641875c89a").is_ok());
    }
    #[test]
    fn connector_scope_filters_exact_origin_container_and_profile() {
        let scratch = Scratch::new();
        let connection = Connection::new(Kind::Bitwarden, "Personal").expect("connection");
        let connector = Connector::new(connection, Arc::new(Mutex::new(None)), &scratch.path());
        let mut item = Json(
            json!({"id":"fixture","type":1,"name":"Fixture","fields":[],"login":{"uris":[{"uri":"https://accounts.example.test/login"}],"username":"user","password":"private"}}),
        );
        let site = Site::parse("https://accounts.example.test/", None).expect("site");
        assert!(connector.bw_credential(&item, Some(&site)).is_ok());
        for url in [
            "https://evil.example.test/",
            "https://accounts.example.test.evil.test/",
            "http://accounts.example.test/",
        ] {
            if let Ok(site) = Site::parse(url, None) {
                assert!(connector.bw_credential(&item, Some(&site)).is_err());
            }
        }
        let container = Site::parse(&site.origin, Some(7)).expect("container");
        assert!(connector.bw_credential(&item, Some(&container)).is_err());
        item["fields"] =
            json!([{"name":"enzim.container","value":connector.scope_value(Some(7)),"type":0}]);
        assert!(connector.bw_credential(&item, Some(&container)).is_ok());
        assert!(connector.bw_credential(&item, Some(&site)).is_err());
        let foreign = Scratch::new();
        let foreign = Connector::new(
            Connection::new(Kind::Bitwarden, "Business").expect("connection"),
            Arc::new(Mutex::new(None)),
            &foreign.path(),
        );
        assert!(foreign.bw_credential(&item, None).is_err());
        item["reprompt"] = Value::from(1);
        assert!(connector.bw_credential(&item, None).is_err());
        let wrong = json!({"userEmail":"other@example.test","serverUrl":"https://vault.bitwarden.com","status":"unlocked"});
        assert!(connector.verify_bw(&wrong).is_err());
    }
    #[test]
    #[ignore = "requires an isolated Vaultwarden account and the official Bitwarden CLI"]
    fn real_bitwarden_windows_share_one_worker_and_keep_reply_authority() {
        use super::super::{
            config,
            service::{Operation, Output, Service},
        };
        let file = std::env::var_os("ENZIM_CONNECTOR_FIXTURE").expect("private fixture path");
        let bytes = Zeroizing::new(std::fs::read(file).expect("private fixture"));
        let fixture = Json::parse(&bytes).expect("fixture JSON");
        let account = &fixture["accounts"][0];
        let scratch = Scratch::new();
        let directory = scratch.path().with_file_name("cli-profile");
        enzui_security::secure_dir::ensure_private_dir(&directory).expect("private CLI directory");
        std::fs::copy(
            PathBuf::from(account["directory"].as_str().expect("directory")).join("data.json"),
            directory.join("data.json"),
        )
        .expect("independent cached profile");
        let mut connection =
            Connection::new(Kind::Bitwarden, "Shared browser window fixture").expect("connection");
        connection.executable = fixture["executable"].as_str().expect("executable").into();
        connection.server = fixture["server"].as_str().expect("server").into();
        connection.account = account["email"].as_str().expect("account").into();
        connection.directory = Some(directory);
        connection.remember = false;
        let source = connection.id.clone();
        let config_path = scratch.path().with_file_name("settings.json");
        let mut settings = super::super::Settings::enabled_defaults();
        settings.accounts.push(connection);
        config::save(&config_path, &settings).expect("connection settings");
        let (_, identity) = config::load_authorized(&config_path).expect("settings identity");
        let first = Service::shared(scratch.path(), config_path.clone()).expect("first window");
        let second = Service::shared(scratch.path(), config_path).expect("second window");
        assert!(
            Arc::ptr_eq(&first, &second),
            "windows must serialize the same CLI profile"
        );
        let ask = |service: &Service, operation| {
            service
                .request(operation)
                .expect("queue")
                .blocking_recv()
                .expect("reply")
        };
        let unlock = || Operation::OpenSource {
            source: source.clone(),
            create: false,
            authentication: Authentication {
                password: secret(&account["password"]),
                settings_identity: identity,
                client_id: String::new(),
                client_secret: Secret::default(),
                second_factor: Secret::default(),
            },
        };
        assert!(ask(&first, unlock()).result.is_ok());
        let generation = first.generation();
        assert!(matches!(
            ask(&second, Operation::OpenDesktop { manual: false }).result,
            Ok(Output::DesktopOpened(true))
        ));
        assert_eq!(
            first.generation(),
            generation,
            "another window must reuse the open source"
        );
        let mut login = Credential::new(
            Site::parse("https://enzim-multiwindow-fixture.example", None).expect("site"),
        )
        .expect("login");
        login.username = Secret::new(format!("shared-window-{}", login.id));
        login.password = Secret::new("Synthetic shared-window password only".into());
        assert!(
            ask(&first, Operation::SaveTo(source.clone(), login.clone()))
                .result
                .is_ok()
        );
        let Ok(Output::Entries(entries)) = ask(
            &first,
            Operation::ListSource(source.clone(), Some(login.site.clone())),
        )
        .result
        else {
            panic!("real shared-window inventory")
        };
        let id = entries
            .iter()
            .find(|entry| entry.username == login.username)
            .expect("saved record")
            .id
            .clone();
        let read = ask(
            &first,
            Operation::Read(id.clone(), Some(login.site.clone())),
        );
        let lease = first.reply_lease(&read, Duration::from_secs(10));
        assert!(read.result.is_ok() && lease.authorize());
        assert!(ask(&second, unlock()).result.is_ok());
        assert!(!lease.valid(), "re-unlocking revokes earlier fill replies");
        assert!(
            matches!(ask(&first, Operation::Read(id.clone(), Some(login.site))).result,
            Ok(Output::Entry(entry)) if entry.password == login.password)
        );
        assert!(ask(&second, Operation::Delete(id)).result.is_ok());
    }
    #[test]
    #[ignore = "requires an isolated Vaultwarden account and the official Bitwarden CLI"]
    fn real_bitwarden_keeps_a_passkey_made_here_and_signs_with_it() {
        use super::super::passkey::{Incoming, Keeper};
        use sha2::{Digest, Sha256};
        let file = std::env::var_os("ENZIM_CONNECTOR_FIXTURE").expect("private fixture path");
        let bytes = Zeroizing::new(std::fs::read(file).expect("private fixture"));
        let fixture = Json::parse(&bytes).expect("fixture json");
        let account = &fixture["accounts"][0];
        let scratch = Scratch::new();
        let mut connection =
            Connection::new(Kind::Bitwarden, account["name"].as_str().expect("name"))
                .expect("connection");
        connection.executable = fixture["executable"].as_str().expect("executable").into();
        connection.server = fixture["server"].as_str().expect("server").into();
        connection.account = account["email"].as_str().expect("email").into();
        connection.directory = Some(PathBuf::from(
            account["directory"].as_str().expect("directory"),
        ));
        connection.remember = false;
        let mut connector = Connector::new(connection, Arc::new(Mutex::new(None)), &scratch.path());
        connector
            .unlock(&Authentication {
                password: secret(&account["password"]),
                settings_identity: None,
                client_id: String::new(),
                client_secret: Secret::default(),
                second_factor: Secret::default(),
            })
            .expect("real CLI unlock");
        let origin = "https://enzim-passkey-fixture.example";
        let request = |json: &str| match Incoming::parse(json, origin, origin, true) {
            Some(Incoming::Request(request)) => *request,
            other => panic!("not a request: {other:?}"),
        };
        let registration = request(
            r#"{"kind":"create","id":1,"options":{"rp":{"name":"Passkey fixture"},
            "user":{"id":"Zml4dHVyZS11c2Vy","name":"fixture-user","displayName":"Fixture user"},
            "challenge":"cmVnaXN0cmF0aW9uLWNoYWxsZW5nZQ","pubKeyCredParams":[{"type":"public-key","alg":-7}]}}"#,
        );
        let (passkey, _) = registration.create(Keeper::Bitwarden).expect("new passkey");
        let site = Site::parse(origin, None).expect("site");
        let mut login = Credential::new(site.clone()).expect("login");
        login.name = Secret::new("enzim passkey fixture".into());
        login.username = Secret::new(format!("passkey-{}", login.id));
        login.passkey = Some(Box::new(passkey.clone()));
        connector.save(login.clone()).expect("official CLI create with a passkey");

        let summary = connector
            .list(Some(&site))
            .expect("official CLI list")
            .into_iter()
            .find(|entry| entry.username == login.username)
            .expect("the saved login");
        assert_eq!(
            summary.passkey,
            Some((passkey.rp_id.clone(), passkey.credential_id.clone())),
            "the list names the passkey's relying party and credential"
        );
        let read = connector.read_selected(&summary.id).expect("read back");
        assert_eq!(read.passkey.as_deref(), Some(&passkey), "the passkey comes back whole");
        // The item as Bitwarden's own clients read it.
        let raw = Json::parse(
            &connector
                .run(&["get", "item", &summary.id], None)
                .expect("raw item"),
        )
        .expect("raw item JSON");
        let native = &raw["login"]["fido2Credentials"][0];
        assert_eq!(native["credentialId"].as_str().expect("id").len(), 36);
        assert_eq!(native["rpId"], "enzim-passkey-fixture.example");
        assert_eq!(native["keyAlgorithm"], "ECDSA");
        assert_eq!(native["discoverable"], "true");

        // A sign-in with the passkey as read from Bitwarden verifies.
        let sign_in = request(
            r#"{"kind":"get","id":2,"options":{"challenge":"c2lnbi1pbi1jaGFsbGVuZ2U"}}"#,
        );
        let stored = read.passkey.as_ref().expect("passkey");
        let asserted = sign_in.assert(stored, Keeper::Bitwarden).expect("assertion");
        let field = |key: &str| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(asserted[key].as_str().expect(key))
                .expect(key)
        };
        let mut signed = field("authenticatorData");
        signed.extend_from_slice(&Sha256::digest(field("clientDataJSON")));
        assert!(stored.verifies(&signed, &field("signature")));

        // A password added later leaves the passkey in place.
        let mut updated = read.clone();
        updated.password = Secret::new("Added after the passkey".into());
        connector.save(updated.clone()).expect("official CLI edit");
        let after = connector.read_selected(&summary.id).expect("read updated");
        assert_eq!(after.password, updated.password);
        assert_eq!(after.passkey.as_deref(), Some(&passkey));
        // Another passkey is never written over the one the login holds.
        let (other, _) = registration.create(Keeper::Bitwarden).expect("second passkey");
        let mut second = after;
        second.passkey = Some(Box::new(other));
        assert!(connector.save(second).is_err());
        assert_eq!(
            connector
                .read_selected(&summary.id)
                .expect("read after refusal")
                .passkey
                .as_deref(),
            Some(&passkey)
        );
        connector.delete(&summary.id).expect("official CLI delete");
    }
    #[test]
    #[ignore = "requires two isolated Vaultwarden accounts and the official Bitwarden CLI"]
    fn real_bitwarden_two_account_crud_preserves_unrelated_fields() {
        let file = std::env::var_os("ENZIM_CONNECTOR_FIXTURE").expect("private fixture path");
        let bytes = Zeroizing::new(std::fs::read(file).expect("private fixture"));
        let fixture = Json::parse(&bytes).expect("fixture json");
        let scratch = Scratch::new();
        let mut connectors = Vec::new();
        for account in fixture["accounts"].as_array().expect("accounts") {
            let mut connection =
                Connection::new(Kind::Bitwarden, account["name"].as_str().expect("name"))
                    .expect("connection");
            connection.executable = fixture["executable"].as_str().expect("executable").into();
            connection.server = fixture["server"].as_str().expect("server").into();
            connection.account = account["email"].as_str().expect("email").into();
            connection.directory = Some(PathBuf::from(
                account["directory"].as_str().expect("directory"),
            ));
            connection.remember = false;
            let mut connector =
                Connector::new(connection, Arc::new(Mutex::new(None)), &scratch.path());
            connector
                .unlock(&Authentication {
                    password: secret(&account["password"]),
                    settings_identity: None,
                    client_id: String::new(),
                    client_secret: Secret::default(),
                    second_factor: Secret::default(),
                })
                .expect("real CLI unlock");
            connectors.push(connector);
        }
        assert_eq!(connectors.len(), 2);
        let site = Site::parse("https://enzim-connector-fixture.example/", Some(7)).expect("site");
        let mut ids = Vec::new();
        for (index, connector) in connectors.iter().enumerate() {
            let mut login = Credential::new(site.clone()).expect("login");
            login.name = Secret::new("enzim integration fixture".into());
            login.username = Secret::new(format!("account-{index}-{}", login.id));
            login.password = Secret::new("Original fixture password only".into());
            login.totp = Some(super::super::Totp::parse("JBSWY3DPEHPK3PXP").expect("totp"));
            connector.save(login.clone()).expect("official CLI create");
            let entries = connector.list(Some(&site)).expect("official CLI list");
            let saved = entries
                .iter()
                .find(|entry| entry.username == login.username)
                .expect("saved record");
            let id = saved.id.clone();
            let mirror_directory = scratch.path().with_file_name(format!("mirror-{index}"));
            enzui_security::secure_dir::ensure_private_dir(&mirror_directory)
                .expect("independent client directory");
            std::fs::copy(
                connector
                    .directory()
                    .expect("primary directory")
                    .join("data.json"),
                mirror_directory.join("data.json"),
            )
            .expect("independent client's cached vault");
            let mut mirror_connection = connector.connection.clone();
            mirror_connection.directory = Some(mirror_directory);
            let mut mirror = Connector::new(mirror_connection, Default::default(), &scratch.path());
            mirror.session = connector.session.clone();
            let mut raw =
                Json::parse(&connector.run(&["get", "item", &id], None).expect("get raw"))
                    .expect("raw item");
            raw["favorite"] = Value::Bool(true);
            raw["login"]["fido2Credentials"] = json!([{
                "credentialId":"c3ludGhldGljLWZpeHR1cmUtY3JlZGVudGlhbA",
                "keyType":"public-key","keyAlgorithm":"ECDSA","keyCurve":"P-256",
                "keyValue":"c3ludGhldGljLWZpeHR1cmUta2V5",
                "rpId":"enzim-connector-fixture.example","userHandle":"Zml4dHVyZQ",
                "userName":"fixture-user","counter":"0","rpName":"Fixture",
                "userDisplayName":"Fixture user","discoverable":"true",
                "creationDate":"2026-11-03T00:00:00.000Z"
            }]);
            raw["fields"].as_array_mut().expect("fields").push(
                json!({"name":"Preserve this field","value":"independent metadata","type":0}),
            );
            let payload = Zeroizing::new(
                base64::engine::general_purpose::STANDARD
                    .encode(&*raw.bytes().expect("serialize raw"))
                    .into_bytes(),
            );
            mirror
                .run(&["edit", "item", &id], Some(payload))
                .expect("independent client adds unrelated data");
            let seeded = Json::parse(
                &mirror
                    .run(&["get", "item", &id], None)
                    .expect("seeded native record"),
            )
            .expect("seeded JSON");
            let expected_passkeys = Json(seeded["login"]["fido2Credentials"].clone());
            assert_eq!(
                expected_passkeys
                    .as_array()
                    .expect("native passkey array")
                    .len(),
                1
            );
            let stale = Json::parse(
                &connector
                    .run(&["get", "item", &id], None)
                    .expect("cached primary record"),
            )
            .expect("cached JSON");
            assert_ne!(
                &stale["login"]["fido2Credentials"], &*expected_passkeys,
                "independent client's changes have not reached the cache"
            );
            let mut saved = connector.read(&id, Some(&site)).expect("official CLI read");
            assert_eq!(saved.password, login.password);
            assert!(saved.totp.is_some());
            saved.password = Secret::new("Updated fixture password only".into());
            connector.save(saved.clone()).expect("official CLI edit");
            assert_eq!(
                connector
                    .read(&id, Some(&site))
                    .expect("read updated")
                    .password,
                saved.password
            );
            let raw = Json::parse(
                &connector
                    .run(&["get", "item", &id], None)
                    .expect("get raw updated"),
            )
            .expect("raw updated");
            assert_eq!(raw["favorite"], true);
            assert_eq!(&raw["login"]["fido2Credentials"], &*expected_passkeys);
            assert!(
                raw["fields"]
                    .as_array()
                    .expect("fields")
                    .iter()
                    .any(|field| field["name"] == "Preserve this field"
                        && field["value"] == "independent metadata")
            );
            assert!(
                connector
                    .read(
                        &id,
                        Some(&Site::parse(&site.origin, None).expect("default scope"))
                    )
                    .is_err()
            );
            ids.push(id);
        }
        assert!(connectors[0].read(&ids[1], Some(&site)).is_err());
        assert!(connectors[1].read(&ids[0], Some(&site)).is_err());
        for (connector, id) in connectors.iter().zip(&ids) {
            connector.delete(id).expect("official CLI delete");
            assert!(connector.read(id, Some(&site)).is_err());
        }
        let mut wrong = connectors.remove(0);
        wrong.connection.account = "wrong-fixture@example.test".into();
        let status = wrong.bw_status().expect("native status");
        assert!(wrong.verify_bw(&status).is_err());
    }
}
