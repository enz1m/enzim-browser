use super::{Result, Settings};
use enzui_core::state_authority::{CustodySnapshot, StateAuthority};
use std::path::{Path, PathBuf};

pub(crate) fn path() -> PathBuf {
    crate::browser_profile::paths()
        .config
        .join("credentials.json")
}
pub(crate) fn load(path: &Path) -> Result<Settings> {
    load_authorized(path).map(|(settings, _)| settings)
}
pub(crate) fn load_authorized(
    path: &Path,
) -> Result<(Settings, Option<enzui_core::state_authority::StateIdentity>)> {
    let authority = match StateAuthority::open_existing(path) {
        Ok(authority) => authority,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((Settings::default(), None));
        }
        Err(_) => {
            return Err(
                "Password manager settings have unsafe permissions or are unavailable.".into(),
            );
        }
    };
    let CustodySnapshot::Current {
        bytes,
        overflow: false,
        ..
    } = authority
        .read_bounded(64 * 1024)
        .map_err(|_| "Password manager settings changed while reading.".to_owned())?
    else {
        return Err("Password manager settings are invalid.".into());
    };
    let mut document: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| "Password manager settings are invalid.".to_owned())?;
    retire_removed_providers(&mut document);
    let mut settings: Settings = serde_json::from_value(document)
        .map_err(|_| "Password manager settings are invalid.".to_owned())?;
    settings.lock_minutes = settings.lock_minutes.clamp(1, 60);
    if settings.provider.len() > 256 {
        return Err("The credential provider identity is invalid.".into());
    }
    super::accounts::validate_connections(&settings.accounts, &settings.save_to)?;
    super::accounts::validate_connections(&settings.accounts, &settings.preferred_source)?;
    Ok((settings, authority.identity()))
}
/// Drops connections to providers this build no longer has (1Password,
/// LastPass), and the choices that named them, from settings written by an
/// earlier build: the rest of the settings stay readable, and the next save
/// writes them without those connections.
fn retire_removed_providers(document: &mut serde_json::Value) {
    use serde_json::Value;
    let removed = |kind: &Value| matches!(kind.as_str(), Some("one_password" | "last_pass"));
    let Some(settings) = document.as_object_mut() else {
        return;
    };
    if settings.get("setup_provider").is_some_and(removed) {
        let _ = settings.insert("setup_provider".into(), Value::Null);
    }
    let mut retired = Vec::new();
    if let Some(accounts) = settings.get_mut("accounts").and_then(Value::as_array_mut) {
        accounts.retain(|account| {
            let retire = removed(&account["kind"]);
            if retire && let Some(id) = account["id"].as_str() {
                retired.push(id.to_owned());
            }
            !retire
        });
    }
    for choice in ["save_to", "preferred_source"] {
        if settings
            .get(choice)
            .and_then(Value::as_str)
            .is_some_and(|id| retired.iter().any(|retired| retired == id))
        {
            let _ = settings.insert(choice.into(), Value::String(String::new()));
        }
    }
}
pub(crate) fn save(path: &Path, settings: &Settings) -> Result<()> {
    super::accounts::validate_connections(&settings.accounts, &settings.save_to)?;
    super::accounts::validate_connections(&settings.accounts, &settings.preferred_source)?;
    let parent = path
        .parent()
        .ok_or("Password manager settings path is invalid.")?;
    enzui_security::secure_dir::ensure_private_dir(parent)
        .map_err(|_| "Could not create private password manager settings.".to_owned())?;
    let mut authority = StateAuthority::open_or_create(path)
        .map_err(|_| "Could not open private password manager settings.".to_owned())?;
    let bytes = serde_json::to_vec(settings)
        .map_err(|_| "Could not encode password manager settings.".to_owned())?;
    authority.publish(&bytes).map_err(|_| {
        "Could not confirm password manager settings. Reopen settings before trying again."
            .to_owned()
    })?;
    Ok(())
}

#[derive(Clone, Debug)]
pub(crate) struct Publication {
    pub(crate) identity: enzui_core::state_authority::StateIdentity,
    pub(crate) warning: Option<String>,
}
pub(crate) fn save_with_session_cleanup(
    path: &Path,
    expected: Option<enzui_core::state_authority::StateIdentity>,
    settings: &Settings,
) -> Result<Publication> {
    super::accounts::validate_connections(&settings.accounts, &settings.save_to)?;
    super::accounts::validate_connections(&settings.accounts, &settings.preferred_source)?;
    let old = load(path)?;
    let revoked = revoked_connections(&old, settings);
    let parent = path.parent().ok_or("Password settings path is invalid.")?;
    enzui_security::secure_dir::ensure_private_dir(parent)
        .map_err(|_| "Could not open private password settings.".to_owned())?;
    let mut authority = StateAuthority::open_or_create(path)
        .map_err(|_| "Could not authorize password settings.".to_owned())?;
    if authority.identity() != expected {
        return Err("Password settings changed in another window. Reopen password settings before trying again.".into());
    }
    let bytes = serde_json::to_vec(settings)
        .map_err(|_| "Could not encode password settings.".to_owned())?;
    authority.publish(&bytes).map_err(|_| {
        "Password settings changed before saving. Reopen settings and review the new choices."
            .to_owned()
    })?;
    let identity = authority
        .identity()
        .ok_or("Password settings publication was not confirmed.")?;
    let context = super::desktop::DesktopContext::default();
    let active = Default::default();
    let mut failed = Vec::new();
    for connection in revoked {
        let mut desktop = super::desktop::DesktopUnlock::for_record(
            &super::Vault::default_path(),
            &connection.session_namespace(),
            "Password manager connection",
            std::sync::Arc::clone(&active),
        );
        desktop.use_context(context.clone());
        if desktop.forget().is_err() {
            failed.push(connection.name.clone());
        }
    }
    // A connection that was removed is signed out, so that its connector
    // profile does not stay authorized on disk.
    for removed in old
        .accounts
        .iter()
        .filter(|previous| !settings.accounts.iter().any(|kept| kept.id == previous.id))
    {
        super::connector::Connector::new(
            removed.clone(),
            Default::default(),
            &super::Vault::default_path(),
        )
        .sign_out();
    }
    let warning = (!failed.is_empty()).then(|| format!("Connection settings saved. The desktop keyring could not remove remembered sessions for {}. Remove those enzim Browser connection records in your keyring manager, or reconnect and turn off Remember when the keyring is available.", failed.join(", ")));
    Ok(Publication { identity, warning })
}
fn revoked_connections<'a>(
    old: &'a Settings,
    new: &Settings,
) -> Vec<&'a super::accounts::Connection> {
    old.accounts
        .iter()
        .filter(|previous| {
            previous.remember
                && !new.accounts.iter().any(|current| {
                    current.id == previous.id
                        && current.remember
                        && current.enabled
                        && current.session_namespace() == previous.session_namespace()
                })
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_from_a_build_with_removed_providers_keep_everything_else() {
        use super::super::accounts::{Connection, Kind};
        let scratch = super::super::tests::Scratch::new();
        let kept = Connection::new(Kind::Bitwarden, "Personal").expect("connection");
        let mut settings = Settings::enabled_defaults();
        settings.accounts = vec![kept.clone()];
        settings.save_to.clone_from(&kept.id);
        save(&scratch.path(), &settings).expect("current settings");
        // What an earlier build wrote: two more connections, one of them the
        // read source, and a pending setup of a removed provider.
        let mut document: serde_json::Value =
            serde_json::from_slice(&std::fs::read(scratch.path()).expect("settings file"))
                .expect("settings document");
        let mut removed = Vec::new();
        for (kind, name) in [("one_password", "Work"), ("last_pass", "Old")] {
            let mut connection =
                serde_json::to_value(Connection::new(Kind::Bitwarden, name).expect("connection"))
                    .expect("connection document");
            connection["kind"] = kind.into();
            removed.push(connection["id"].as_str().expect("id").to_owned());
            document["accounts"]
                .as_array_mut()
                .expect("accounts")
                .push(connection);
        }
        document["preferred_source"] = removed[1].clone().into();
        document["setup_provider"] = "one_password".into();
        let mut authority = StateAuthority::open_or_create(&scratch.path()).expect("authority");
        authority
            .publish(&serde_json::to_vec(&document).expect("document"))
            .expect("earlier build's settings");

        let restored = load(&scratch.path()).expect("settings stay readable");
        assert_eq!(restored.accounts, vec![kept.clone()]);
        assert_eq!(restored.save_to, kept.id, "a kept choice is untouched");
        assert!(
            restored.preferred_source.is_empty(),
            "a choice of a removed connection is cleared"
        );
        assert!(restored.setup_provider.is_none());
        assert!(restored.enabled, "unrelated settings are kept");
    }
    #[test]
    #[ignore = "Requires an isolated, unlocked native Secret Service provider."]
    fn native_connection_cleanup_removes_only_revoked_remembered_sessions() {
        use super::super::{
            accounts::{Connection, Kind},
            desktop::{DesktopContext, DesktopUnlock},
        };
        let scratch = super::super::tests::Scratch::new();
        let path = scratch.path().with_file_name("settings.json");
        let mut settings = Settings::enabled_defaults();
        for name in ["Personal", "Business"] {
            settings
                .accounts
                .push(Connection::new(Kind::Bitwarden, name).expect("connection"));
        }
        save(&path, &settings).expect("initial settings");
        let context = DesktopContext::default();
        let mut records = settings
            .accounts
            .iter()
            .map(|connection| {
                let mut record = DesktopUnlock::for_record(
                    &super::super::Vault::default_path(),
                    &connection.session_namespace(),
                    "Password manager connection",
                    Default::default(),
                );
                record.use_context(context.clone());
                assert!(record.store_secret(enzui_security::secret::Secret::new(
                    b"synthetic session token".to_vec()
                )));
                record
            })
            .collect::<Vec<_>>();
        let (_, identity) = load_authorized(&path).expect("settings identity");
        settings.accounts[0].remember = false;
        settings.accounts[1].name = "Renamed business".into();
        let publication =
            save_with_session_cleanup(&path, identity, &settings).expect("unremember");
        assert!(publication.warning.is_none());
        assert!(
            records[0]
                .retrieve_required()
                .expect("revoked record lookup")
                .is_none()
        );
        let retained = records[1]
            .retrieve_required()
            .expect("retained record lookup")
            .expect("business session");
        assert_eq!(retained.expose_secret(), b"synthetic session token");
        settings.accounts.remove(1);
        let publication = save_with_session_cleanup(&path, Some(publication.identity), &settings)
            .expect("disconnect");
        assert!(publication.warning.is_none());
        assert!(
            records[1]
                .retrieve_required()
                .expect("disconnected record lookup")
                .is_none()
        );
    }
    #[test]
    fn stale_settings_cannot_resurrect_a_disconnected_password_account() {
        let scratch = super::super::tests::Scratch::new();
        let path = scratch.path().with_file_name("settings.json");
        let mut old = Settings::enabled_defaults();
        for name in ["Personal", "Business"] {
            let mut account = super::super::accounts::Connection::new(
                super::super::accounts::Kind::Bitwarden,
                name,
            )
            .expect("connection");
            account.remember = false;
            old.accounts.push(account);
        }
        save(&path, &old).expect("settings");
        let (_, identity) = load_authorized(&path).expect("loaded custody");
        let mut next = old.clone();
        next.accounts.remove(1);
        let publication = save_with_session_cleanup(&path, identity, &next).expect("disconnect");
        assert!(publication.warning.is_none());
        assert_eq!(
            load_authorized(&path).expect("new identity").1,
            Some(publication.identity)
        );
        assert!(save_with_session_cleanup(&path, identity, &old).is_err());
        assert_eq!(load(&path).expect("still disconnected"), next);
    }
    #[test]
    fn changed_disconnected_or_unremembered_accounts_revoke_only_their_own_sessions() {
        let mut old = Settings::enabled_defaults();
        old.accounts.push(
            super::super::accounts::Connection::new(
                super::super::accounts::Kind::Bitwarden,
                "Personal",
            )
            .expect("personal"),
        );
        old.accounts.push(
            super::super::accounts::Connection::new(
                super::super::accounts::Kind::Bitwarden,
                "Business",
            )
            .expect("business"),
        );
        assert!(revoked_connections(&old, &old).is_empty());
        let mut new = old.clone();
        new.accounts[0].remember = false;
        assert_eq!(
            revoked_connections(&old, &new)
                .iter()
                .map(|account| &account.id)
                .collect::<Vec<_>>(),
            vec![&old.accounts[0].id]
        );
        new.accounts.remove(1);
        assert_eq!(revoked_connections(&old, &new).len(), 2);
        new = old.clone();
        new.accounts[0].server = "https://other.example.test".into();
        assert_eq!(revoked_connections(&old, &new).len(), 1);
    }
}
