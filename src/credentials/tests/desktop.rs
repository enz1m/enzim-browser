use super::*;
use enzui_services::credential_store::{
    CredentialStore, MemoryCredentialStore, UnavailableCredentialStore,
};
use std::sync::Arc;

#[test]
fn desktop_key_authenticates_existing_vault_and_preserves_password_recovery() {
    let scratch = Scratch::new();
    let mut vault = Vault::create(&scratch.path(), MASTER).expect("vault");
    let login = entry("https://example.test/", Some(7));
    vault.upsert(login.clone()).expect("save");
    let key = vault.desktop_secret();
    drop(vault);
    let reopened =
        Vault::unlock_desktop(&scratch.path(), key.expose_secret()).expect("desktop unlock");
    assert_eq!(
        reopened.entries()[0].site,
        login.site,
        "login scope survives"
    );
    assert_eq!(
        reopened.entries()[0].password,
        login.password,
        "password survives"
    );
    assert!(
        !format!("{key:?}").contains(MASTER),
        "key formatting is redacted"
    );
    drop(reopened);
    assert!(
        Vault::unlock(&scratch.path(), MASTER).is_ok(),
        "password recovery still works"
    );
    assert!(
        Vault::unlock_desktop(&scratch.path(), MASTER.as_bytes()).is_err(),
        "master passwords are not key packets"
    );
}

#[test]
fn desktop_keys_reject_other_vaults_changed_ciphertext_and_rekeyed_files() {
    let first = Scratch::new();
    let second = Scratch::new();
    let mut vault = Vault::create(&first.path(), MASTER).expect("first vault");
    let key = vault.desktop_secret();
    drop(Vault::create(&second.path(), MASTER).expect("second vault"));
    assert!(
        Vault::unlock_desktop(&second.path(), key.expose_secret()).is_err(),
        "other vault rejected"
    );
    let original = std::fs::read(first.path()).expect("ciphertext");
    let mut damaged = original.clone();
    let last = damaged.len() - 1;
    damaged[last] ^= 1;
    std::fs::write(first.path(), &damaged).expect("fault ciphertext");
    assert!(
        Vault::unlock_desktop(&first.path(), key.expose_secret()).is_err(),
        "AEAD authentication required"
    );
    std::fs::write(first.path(), &original).expect("restore ciphertext");
    vault = Vault::unlock(&first.path(), MASTER).expect("restore authority");
    vault
        .rekey(MASTER, "a different recovery password")
        .expect("rekey");
    assert!(
        Vault::unlock_desktop(&first.path(), key.expose_secret()).is_err(),
        "old desktop key revoked by rekey"
    );
    assert!(
        Vault::unlock_desktop(&first.path(), vault.desktop_secret().expose_secret()).is_ok(),
        "new desktop key works"
    );
}

#[test]
fn desktop_storage_is_scoped_persistent_and_removable() {
    let scratch = Scratch::new();
    let foreign = Scratch::new();
    let store = Arc::new(MemoryCredentialStore::new());
    let shared: Arc<dyn CredentialStore> = Arc::<MemoryCredentialStore>::clone(&store);
    let vault = Vault::create(&scratch.path(), MASTER).expect("vault");
    let mut desktop = super::super::desktop::DesktopUnlock::with_store(
        &scratch.path(),
        std::sync::Arc::clone(&shared),
    );
    assert!(
        desktop.remember(&vault),
        "remember through credential store"
    );
    drop(desktop);
    let mut other = super::super::desktop::DesktopUnlock::with_store(
        &foreign.path(),
        std::sync::Arc::clone(&shared),
    );
    assert!(
        other.retrieve().expect("lookup").is_none(),
        "different profile has no key"
    );
    let mut fresh = super::super::desktop::DesktopUnlock::with_store(&scratch.path(), shared);
    let secret = fresh.retrieve().expect("lookup").expect("remembered key");
    assert!(
        Vault::unlock_desktop(&scratch.path(), secret.expose_secret()).is_ok(),
        "independent client unlocks"
    );
    fresh.forget().expect("remove remembered key");
    assert!(
        fresh.retrieve().expect("lookup after delete").is_none(),
        "forget removes key"
    );
    assert_eq!(store.entry_count(), 0, "no leftover secret");
}

#[test]
fn desktop_provider_enrolls_manual_unlock_and_refreshes_after_password_change() {
    let scratch = Scratch::new();
    let store: Arc<dyn CredentialStore> = Arc::new(MemoryCredentialStore::new());
    let mut provider = Builtin::with_desktop_store(scratch.path(), Arc::clone(&store));
    provider.open(MASTER, true).expect("create and remember");
    assert!(
        provider.desktop_remembered(),
        "creation enrolls desktop key"
    );
    provider
        .save(entry("https://example.test/", None))
        .expect("save login");
    provider.lock();
    assert!(provider.desktop_unlock().expect("desktop unlock"));
    assert_eq!(provider.list(None).expect("saved logins").len(), 1);
    assert!(
        provider.csv("incorrect recovery password").is_err(),
        "sensitive export still requires password"
    );
    provider
        .rekey(MASTER, "a different recovery password")
        .expect("change recovery password");
    let mut restarted = Builtin::with_desktop_store(scratch.path(), store);
    assert!(
        restarted.desktop_unlock().expect("updated key"),
        "rekey updates desktop copy"
    );
    restarted.forget_desktop().expect("forget");
    restarted.lock();
    assert!(!restarted.desktop_unlock().expect("no key"));
    restarted
        .open("a different recovery password", false)
        .expect("password fallback");
}

#[test]
fn desktop_unavailable_and_disabled_keep_password_fallback_without_reads() {
    let scratch = Scratch::new();
    let mut unavailable =
        Builtin::with_desktop_store(scratch.path(), Arc::new(UnavailableCredentialStore));
    unavailable
        .open(MASTER, true)
        .expect("keyring absence does not prevent creation");
    assert!(
        !unavailable.desktop_remembered(),
        "no false storage success"
    );
    unavailable.lock();
    assert!(!unavailable.desktop_unlock().expect("unavailable"));
    unavailable
        .open(MASTER, false)
        .expect("password still unlocks");
    let store = Arc::new(MemoryCredentialStore::new());
    let mut disabled =
        Builtin::with_desktop_store(scratch.path(), Arc::<MemoryCredentialStore>::clone(&store));
    disabled.desktop_enabled(false);
    assert!(!disabled.desktop_unlock().expect("disabled"));
    assert_eq!(
        store.get_call_count(),
        0,
        "disabled desktop unlock never reads secrets"
    );
    disabled.open(MASTER, false).expect("manual unlock");
    assert_eq!(
        store.entry_count(),
        0,
        "disabled policy does not enroll secrets"
    );
}

#[test]
#[ignore = "requires an isolated unlocked Secret Service provider"]
fn desktop_secret_service_roundtrip() {
    let scratch = Scratch::new();
    let mut vault = Vault::create(&scratch.path(), MASTER).expect("vault");
    vault
        .upsert(entry("https://example.test/", None))
        .expect("fixture login");
    let active = Arc::new(std::sync::Mutex::new(None));
    let mut desktop =
        super::super::desktop::DesktopUnlock::new(&scratch.path(), std::sync::Arc::clone(&active));
    assert!(desktop.remember(&vault), "real provider stores key");
    drop(desktop);
    drop(vault);
    let mut fresh = super::super::desktop::DesktopUnlock::new(&scratch.path(), active);
    let key = fresh
        .retrieve()
        .expect("real provider lookup")
        .expect("stored key");
    let reopened =
        Vault::unlock_desktop(&scratch.path(), key.expose_secret()).expect("real provider unlock");
    assert_eq!(reopened.entries().len(), 1);
    fresh.forget().expect("remove fixture key");
    assert!(fresh.retrieve().expect("confirm deletion").is_none());
}

#[test]
fn desktop_lock_cancels_an_outstanding_keyring_request_without_polling() {
    use enzui_services::credential_store::{
        CredentialKey, CredentialOperation, CredentialOutcome, PendingCredentialOp,
    };
    use std::{
        sync::atomic::{AtomicBool, Ordering},
        time::{Duration, Instant},
    };
    struct BlockedReads {
        store: MemoryCredentialStore,
        entered: Arc<AtomicBool>,
    }
    impl CredentialStore for BlockedReads {
        fn get(&self, key: CredentialKey) -> PendingCredentialOp {
            let entered = Arc::clone(&self.entered);
            PendingCredentialOp::new(CredentialOperation::Get, key, async move {
                entered.store(true, Ordering::Release);
                std::future::pending::<CredentialOutcome>().await
            })
        }
        fn set(
            &self,
            key: CredentialKey,
            secret: enzui_security::secret::Secret,
        ) -> PendingCredentialOp {
            self.store.set(key, secret)
        }
        fn delete(&self, key: CredentialKey) -> PendingCredentialOp {
            self.store.delete(key)
        }
    }
    let scratch = Scratch::new();
    drop(Vault::create(&scratch.path(), MASTER).expect("existing vault"));
    let config_path = scratch.path().with_file_name("settings.json");
    let mut settings = Settings::enabled_defaults();
    settings.desktop_unlock = true;
    config::save(&config_path, &settings).expect("isolated settings");
    let entered = Arc::new(AtomicBool::new(false));
    let store = Arc::new(BlockedReads {
        store: MemoryCredentialStore::new(),
        entered: Arc::clone(&entered),
    });
    let service =
        service::Service::start_with_store(scratch.path(), config_path, store).expect("worker");
    let mut reply = service
        .request(service::Operation::OpenDesktop { manual: false })
        .expect("pending keyring read");
    let deadline = Instant::now() + Duration::from_secs(3);
    while !entered.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline, "keyring read did not start");
        std::thread::sleep(Duration::from_millis(5));
    }
    service.lock();
    let reply = loop {
        if let Ok(reply) = reply.try_recv() {
            break reply;
        }
        assert!(
            Instant::now() < deadline,
            "lock did not cancel the keyring read"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(
        !reply.unlocked && reply.result.is_err(),
        "cancelled lookup cannot unlock"
    );
    assert!(!service.lease().valid(), "no grant survives cancellation");
}

#[test]
#[ignore = "provisions an isolated desktop-keyring visual fixture"]
fn desktop_prepare_visual_fixture() {
    let root = PathBuf::from(
        std::env::var_os("ENZIM_DESKTOP_QA_ROOT").expect("explicit fixture directory"),
    )
    .canonicalize()
    .expect("existing fixture directory");
    assert!(
        root.starts_with(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("work/vault-desktop")),
        "fixture must stay in the browser test directory"
    );
    let path = root.join("data/enzim-browser/credentials/vault.enzim");
    let mut vault = Vault::create(&path, MASTER).expect("fresh isolated vault");
    let mut login = entry("https://example.test/", None);
    login.name = Secret::new("Desktop keyring test".into());
    vault.upsert(login).expect("fixture login");
    let mut desktop =
        super::super::desktop::DesktopUnlock::new(&path, Arc::new(std::sync::Mutex::new(None)));
    assert!(
        desktop.remember(&vault),
        "fixture remembered by isolated provider"
    );
    let mut settings = Settings::enabled_defaults();
    settings.desktop_unlock = true;
    config::save(
        &root.join("config/enzim-browser/credentials.json"),
        &settings,
    )
    .expect("fixture settings");
}

#[test]
fn manual_shared_lock_blocks_field_focus_reopening_until_explicit_unlock() {
    let scratch = Scratch::new();
    let config_path = scratch.path().with_file_name("settings.json");
    let mut settings = Settings::enabled_defaults();
    settings.desktop_unlock = true;
    config::save(&config_path, &settings).expect("settings");
    let store = Arc::new(MemoryCredentialStore::new());
    let first = Arc::new(
        service::Service::start_with_store(
            scratch.path(),
            config_path,
            Arc::<MemoryCredentialStore>::clone(&store),
        )
        .expect("shared worker"),
    );
    let second = Arc::clone(&first);
    assert!(
        ask(
            &first,
            service::Operation::Open {
                password: Secret::new(MASTER.into()),
                create: true
            }
        )
        .result
        .is_ok()
    );
    first.lock_manually();
    let reads = store.get_call_count();
    assert!(matches!(
        ask(&second, service::Operation::OpenDesktop { manual: false }).result,
        Ok(service::Output::DesktopOpened(false))
    ));
    assert!(!second.unlocked());
    assert_eq!(
        store.get_call_count(),
        reads,
        "field focus must not consult the keyring after manual lock"
    );
    assert!(matches!(
        ask(&second, service::Operation::OpenDesktop { manual: true }).result,
        Ok(service::Output::DesktopOpened(true))
    ));
    assert!(first.unlocked());
    assert_eq!(store.get_call_count(), reads + 1);
}
