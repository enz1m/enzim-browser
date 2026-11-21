use super::Result;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::PathBuf};

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Storage {
    #[default]
    Browser,
    System,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Kind {
    Bitwarden,
}
impl Kind {
    pub(crate) const ALL: [Self; 1] = [Self::Bitwarden];
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Bitwarden => "Bitwarden",
        }
    }
    pub(crate) fn documentation(self) -> &'static str {
        match self {
            Self::Bitwarden => "https://bitwarden.com/help/cli/",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Connection {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) kind: Kind,
    pub(crate) executable: String,
    pub(crate) account: String,
    pub(crate) vault: String,
    pub(crate) server: String,
    pub(crate) directory: Option<PathBuf>,
    pub(crate) remember: bool,
    #[serde(default)]
    pub(crate) lock_minutes: Option<u16>,
    pub(crate) enabled: bool,
}
impl Connection {
    pub(crate) fn new(kind: Kind, name: &str) -> Result<Self> {
        use base64::Engine;
        let result = Self {
            id: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(super::random::<16>()?),
            name: name.trim().into(),
            kind,
            executable: String::new(),
            account: String::new(),
            vault: String::new(),
            server: String::new(),
            directory: None,
            remember: true,
            lock_minutes: None,
            enabled: true,
        };
        result.validate()?;
        Ok(result)
    }
    pub(crate) fn validate(&self) -> Result<()> {
        if self
            .lock_minutes
            .is_some_and(|minutes| ![0, 1, 5, 15, 30, 60].contains(&minutes))
            || self.lock_minutes.is_some() && self.remember
        {
            return Err("Choose a supported lock policy. Timed and restart locking cannot remember an unlock session.".into());
        }

        if self.id.len() != 22
            || !self
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
            || self.name.trim().is_empty()
            || self.name.chars().count() > 60
            || self.name.chars().any(char::is_control)
        {
            return Err("Choose an account name between 1 and 60 characters.".into());
        }
        // The account is handed to the connector as an argument.
        if self.account.starts_with('-') {
            return Err("Enter the account's email address.".into());
        }
        for (value, max) in [
            (&self.account, 256),
            (&self.vault, 256),
            (&self.executable, 4096),
            (&self.server, 2048),
        ] {
            if value.len() > max || value.chars().any(char::is_control) {
                return Err("The connection has an invalid or oversized setting.".into());
            }
        }
        if !self.executable.is_empty() && !std::path::Path::new(&self.executable).is_absolute() {
            return Err("Choose an absolute connector executable path, or leave it empty for the installed connector.".into());
        }
        if self
            .directory
            .as_ref()
            .is_some_and(|directory| !directory.is_absolute())
        {
            return Err("Choose an absolute CLI profile directory.".into());
        }
        if !self.server.is_empty() {
            let server = url::Url::parse(&self.server)
                .map_err(|_| "Use a complete HTTPS server address.".to_owned())?;
            let local = server.host_str().is_some_and(|host| {
                host == "localhost"
                    || host
                        .parse::<std::net::IpAddr>()
                        .is_ok_and(|ip| ip.is_loopback())
            });
            if server.host_str().is_none()
                || !(server.scheme() == "https" || (server.scheme() == "http" && local))
                || !server.username().is_empty()
                || server.password().is_some()
                || server.query().is_some()
                || server.fragment().is_some()
            {
                return Err("Use an HTTPS server address without URL credentials or query parameters. HTTP is accepted only on loopback.".into());
            }
        }
        Ok(())
    }
    pub(super) fn session_namespace(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut digest = Sha256::new();
        let directory = self
            .directory
            .as_deref()
            .map_or(&[][..], |path| path.as_os_str().as_encoded_bytes());
        for field in [
            self.kind.label().as_bytes(),
            self.executable.as_bytes(),
            self.account.as_bytes(),
            self.vault.as_bytes(),
            self.server.as_bytes(),
            directory,
        ] {
            digest.update(field.len().to_le_bytes());
            digest.update(field);
        }
        format!("provider-session-v2-{}-{:x}", self.id, digest.finalize())
    }
    pub(crate) fn state_directory(&self) -> PathBuf {
        self.directory.clone().unwrap_or_else(|| {
            crate::browser_profile::paths()
                .data
                .join("credentials/accounts")
                .join(&self.id)
        })
    }
}

pub(crate) fn validate_connections(connections: &[Connection], save_to: &str) -> Result<()> {
    if connections.len() > 12 {
        return Err("Up to 12 password-manager accounts are supported.".into());
    }
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    for connection in connections {
        connection.validate()?;
        if !names.insert(connection.name.trim().to_lowercase()) {
            return Err("Use distinct names for your password-manager connections.".into());
        }
        if !ids.insert(&connection.id) {
            return Err("Password-manager accounts must have distinct identities.".into());
        }
    }
    if !save_to.is_empty()
        && save_to != "browser"
        && save_to != "system"
        && !connections
            .iter()
            .any(|connection| connection.id == save_to && connection.enabled)
    {
        return Err("The selected password destination is unavailable.".into());
    }
    Ok(())
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Authentication {
    pub(crate) password: super::Secret,
    pub(crate) settings_identity: Option<enzui_core::state_authority::StateIdentity>,
    pub(crate) client_id: String,
    pub(crate) client_secret: super::Secret,
    pub(crate) second_factor: super::Secret,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_review_lock_policy_rejects_remembered_or_invalid_timeouts_and_reads_legacy_accounts()
     {
        let mut connection = Connection::new(Kind::Bitwarden, "Personal").expect("connection");
        let mut legacy = serde_json::to_value(&connection).expect("legacy fixture");
        legacy
            .as_object_mut()
            .expect("object")
            .remove("lock_minutes");
        assert!(
            serde_json::from_value::<Connection>(legacy)
                .expect("legacy account")
                .lock_minutes
                .is_none()
        );
        connection.lock_minutes = Some(5);
        assert!(
            connection.validate().is_err(),
            "a persisted unlock key must not bypass timed locking"
        );
        connection.remember = false;
        connection.validate().expect("timed policy");
        connection.lock_minutes = Some(0);
        connection.validate().expect("restart policy");
        connection.lock_minutes = Some(2);
        assert!(connection.validate().is_err());
    }
    #[test]
    fn remembered_sessions_are_bound_to_connection_authority_but_not_its_display_label() {
        let initial = Connection::new(Kind::Bitwarden, "Personal").expect("connection");
        let namespace = initial.session_namespace();
        let mut changed = initial.clone();
        changed.name = "Renamed personal".into();
        changed.remember = false;
        assert_eq!(changed.session_namespace(), namespace);
        for field in ["account", "server", "executable", "directory", "vault"] {
            let mut changed = initial.clone();
            match field {
                "account" => changed.account = "another@example.test".into(),
                "server" => changed.server = "https://another.example.test".into(),
                "executable" => changed.executable = "/another/bw".into(),
                "directory" => changed.directory = Some("/another/profile".into()),
                "vault" => changed.vault = "Another".into(),
                _ => unreachable!(),
            }
            assert_ne!(changed.session_namespace(), namespace);
        }
        assert_ne!(
            Connection::new(Kind::Bitwarden, "Business")
                .expect("other connection")
                .session_namespace(),
            namespace
        );
    }
    #[test]
    fn password_connections_keep_two_accounts_distinct_and_validate_transport() {
        let a = Connection::new(Kind::Bitwarden, "Personal").expect("personal");
        let mut b = Connection::new(Kind::Bitwarden, "Business").expect("business");
        assert_ne!(a.id, b.id);
        assert_ne!(a.state_directory(), b.state_directory());
        assert!(validate_connections(&[a.clone(), b.clone()], &b.id).is_ok());
        assert!(validate_connections(&[a.clone(), a.clone()], &a.id).is_err());
        for server in [
            "http://passwords.example/",
            "https://user:password@example.org",
            "https://example.org/?token=secret",
            "file:///tmp/vault",
        ] {
            b.server = server.into();
            assert!(b.validate().is_err());
        }
        for server in [
            "https://vault.example/",
            "http://127.0.0.1:8080",
            "http://localhost:8080",
        ] {
            b.server = server.into();
            assert!(b.validate().is_ok());
        }
    }
}
