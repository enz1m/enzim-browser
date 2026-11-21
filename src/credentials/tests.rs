use super::*;
use enzui_security::{secure_dir::SecureRoot, tempstage::DirectoryStage};
use std::{ffi::OsStr, path::PathBuf};

const MASTER: &str = "a distinct fixture master password";
mod desktop;
pub(crate) struct Scratch(DirectoryStage);
impl Scratch {
    pub(crate) fn new() -> Self {
        let root = SecureRoot::open(
            &std::env::temp_dir().join(format!("enzim-vault-test-{}", std::process::id())),
        )
        .expect("private test root");
        use base64::Engine;
        let name = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(random::<16>().expect("random test identity"));
        Self(
            DirectoryStage::create(&root, OsStr::new(&name))
                .expect("exclusive private test directory"),
        )
    }
    pub(crate) fn path(&self) -> PathBuf {
        self.0.path().join("vault.enzim")
    }
}
fn entry(site: &str, container: Option<u64>) -> Credential {
    let mut entry = Credential::new(Site::parse(site, container).expect("fixture site"))
        .expect("entry identity");
    entry.username = Secret::new("fixture-user".into());
    entry.password = Secret::new("high entropy fixture password".into());
    entry
}

#[test]
fn encrypted_vault_roundtrip_keeps_plaintext_out_of_files_and_debug() {
    let scratch = Scratch::new();
    let mut vault = Vault::create(&scratch.path(), MASTER).expect("new vault");
    let mut login = entry("https://example.test/login", Some(7));
    login.totp = Some(Totp::parse("JBSWY3DPEHPK3PXP").expect("seed"));
    vault.upsert(login.clone()).expect("save login");
    let bytes = std::fs::read(scratch.path()).expect("ciphertext");
    for secret in [
        MASTER,
        "fixture-user",
        "high entropy fixture password",
        "JBSWY3DPEHPK3PXP",
        "example.test",
    ] {
        assert!(
            !bytes
                .windows(secret.len())
                .any(|slice| slice == secret.as_bytes()),
            "vault file leaked a fixture secret"
        );
        assert!(
            !format!("{vault:?} {login:?} {:?}", login.password).contains(secret),
            "debug leaked a fixture secret"
        );
    }
    drop(vault);
    let vault = Vault::unlock(&scratch.path(), MASTER).expect("reopen with password");
    assert_eq!(vault.entries().len(), 1, "saved one login");
    assert_eq!(
        vault.entries()[0].password,
        login.password,
        "password survives encrypted roundtrip"
    );
    assert_eq!(
        vault.entries()[0].totp,
        login.totp,
        "authenticator survives encrypted roundtrip"
    );
}

#[test]
fn wrong_password_and_authenticated_header_ciphertext_changes_fail_closed() {
    let scratch = Scratch::new();
    let vault = Vault::create(&scratch.path(), MASTER).expect("vault");
    drop(vault);
    let original = std::fs::read(scratch.path()).expect("original bytes");
    assert!(
        Vault::unlock(&scratch.path(), "a different fixture password").is_err(),
        "wrong password rejected"
    );
    for offset in [0, 8, 10, 30, original.len() - 1] {
        let mut corrupted = original.clone();
        corrupted[offset] ^= 1;
        std::fs::write(scratch.path(), corrupted).expect("deliberate corruption");
        assert!(
            Vault::unlock(&scratch.path(), MASTER).is_err(),
            "changed authenticated bytes must be rejected"
        );
    }
    std::fs::write(scratch.path(), &original).expect("restore fixture");
    assert!(
        Vault::unlock(&scratch.path(), MASTER).is_ok(),
        "uncorrupted vault accepted"
    );
}

#[test]
fn stale_window_and_occupied_create_cannot_overwrite_a_newer_vault() {
    let scratch = Scratch::new();
    let mut first = Vault::create(&scratch.path(), MASTER).expect("vault");
    let mut stale = Vault::unlock(&scratch.path(), MASTER).expect("second window");
    first
        .upsert(entry("https://one.test", None))
        .expect("first publication");
    let before = std::fs::read(scratch.path()).expect("newer vault bytes");
    assert!(
        stale.upsert(entry("https://two.test", None)).is_err(),
        "stale window refused"
    );
    assert!(
        Vault::create(&scratch.path(), "another sufficiently long master").is_err(),
        "occupied vault refused"
    );
    assert_eq!(
        std::fs::read(scratch.path()).expect("current vault"),
        before,
        "newer vault remains byte-identical"
    );
    let current = Vault::unlock(&scratch.path(), MASTER).expect("newer vault opens");
    assert_eq!(
        current.entries()[0].site.origin,
        "https://one.test",
        "only the current writer persisted"
    );
}

#[test]
fn vault_refuses_symlinks_hardlinks_and_public_file_permissions() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let scratch = Scratch::new();
    drop(Vault::create(&scratch.path(), MASTER).expect("vault"));
    let alias = scratch.0.path().join("alias");
    symlink(scratch.path(), &alias).expect("symlink fixture");
    assert!(Vault::unlock(&alias, MASTER).is_err(), "symlink refused");
    std::fs::remove_file(&alias).expect("remove fixture alias");
    std::fs::hard_link(scratch.path(), &alias).expect("hardlink fixture");
    assert!(
        Vault::unlock(&scratch.path(), MASTER).is_err(),
        "multiply linked vault refused"
    );
    std::fs::remove_file(&alias).expect("remove fixture hardlink");
    std::fs::set_permissions(scratch.path(), std::fs::Permissions::from_mode(0o644))
        .expect("public mode fault");
    assert!(
        Vault::unlock(&scratch.path(), MASTER).is_err(),
        "public permissions refused"
    );
}

#[test]
fn backup_reauthentication_and_rekey_preserve_data() {
    let scratch = Scratch::new();
    let mut vault = Vault::create(&scratch.path(), MASTER).expect("vault");
    vault
        .upsert(entry("https://example.test", None))
        .expect("saved login");
    assert!(
        vault
            .encrypted_export("wrong password", "distinct backup password")
            .is_err(),
        "export requires reauthentication"
    );
    let backup = vault
        .encrypted_export(MASTER, "distinct backup password")
        .expect("encrypted backup");
    assert!(
        Vault::read_backup(MASTER, &backup).is_err(),
        "backup uses its own password"
    );
    assert_eq!(
        Vault::read_backup("distinct backup password", &backup)
            .expect("backup unlock")
            .len(),
        1,
        "backup includes login"
    );
    vault
        .rekey(MASTER, "a replacement master password")
        .expect("change master password");
    drop(vault);
    assert!(
        Vault::unlock(&scratch.path(), MASTER).is_err(),
        "old password revoked"
    );
    assert_eq!(
        Vault::unlock(&scratch.path(), "a replacement master password")
            .expect("new master")
            .entries()
            .len(),
        1,
        "rekey preserves records"
    );
}

#[test]
fn credential_matching_requires_exact_https_origin_port_and_container() {
    let site = Site::parse("https://example.test/login?secret=fixture", Some(7)).expect("site");
    assert_eq!(
        site.origin, "https://example.test",
        "query/path not retained in vault site identity"
    );
    for (other, container) in [
        ("https://example.test.evil.test", Some(7)),
        ("https://sub.example.test", Some(7)),
        ("https://example.test:8443", Some(7)),
        ("https://example.test", None),
        ("https://example.test", Some(8)),
    ] {
        assert!(
            !site.matches(&Site::parse(other, container).expect("other site")),
            "origin/container mismatch refused"
        );
    }
    for other in [
        "http://example.test",
        "javascript:alert(1)",
        "https://user:password@example.test",
        "enzim://settings",
    ] {
        assert!(
            Site::parse(other, None).is_err(),
            "untrusted or credential-bearing origin refused"
        );
    }
    assert!(
        site.matches(
            &Site::parse("https://EXAMPLE.test:443/other", Some(7)).expect("canonical origin")
        ),
        "canonical origin accepted"
    );
}

#[test]
fn totp_matches_all_rfc6238_vectors_and_preserves_leading_zeroes() {
    let times = [
        59,
        1_111_111_109,
        1_111_111_111,
        1_234_567_890,
        2_000_000_000,
        20_000_000_000,
    ];
    let vectors = [
        (
            "SHA1",
            "12345678901234567890",
            [
                "94287082", "07081804", "14050471", "89005924", "69279037", "65353130",
            ],
        ),
        (
            "SHA256",
            "12345678901234567890123456789012",
            [
                "46119246", "68084774", "67062674", "91819424", "90698825", "77737706",
            ],
        ),
        (
            "SHA512",
            "1234567890123456789012345678901234567890123456789012345678901234",
            [
                "90693936", "25091201", "99943326", "93441116", "38618901", "47863826",
            ],
        ),
    ];
    for (algorithm, seed, codes) in vectors {
        let encoded = data_encoding::BASE32_NOPAD.encode(seed.as_bytes());
        let totp = Totp::parse(&format!(
            "otpauth://totp/fixture?secret={encoded}&algorithm={algorithm}&digits=8"
        ))
        .expect("RFC token");
        for (time, expected) in times.into_iter().zip(codes) {
            assert_eq!(
                totp.code(time).expect("RFC code").0.expose(),
                expected,
                "RFC6238 code must match exactly"
            );
        }
    }
    for invalid in [
        "otpauth://hotp/test?secret=JBSWY3DPEHPK3PXP",
        "otpauth://totp/test?secret=JBSWY3DPEHPK3PXP&period=0",
        "otpauth://totp/test?secret=JBSWY3DPEHPK3PXP&digits=20",
        "not-a-seed",
    ] {
        assert!(Totp::parse(invalid).is_err(), "unsupported token rejected");
    }
}

#[test]
fn totp_reads_what_other_apps_write_and_counts_down_its_period() {
    // Computed independently (HMAC-SHA1 of the time step, RFC 4226 truncation).
    let plain = Totp::parse("JBSWY3DPEHPK3PXP").expect("seed");
    assert_eq!(plain.code(59).expect("code").0.expose(), "996554");
    for written in [
        "jbsw y3dp ehpk 3pxp",
        "JBSW-Y3DP-EHPK-3PXP",
        "JBSWY3DPEHPK3PXP====",
        "otpauth://totp/Example:alex?secret=JBSWY3DPEHPK3PXP&issuer=Example&image=https%3A%2F%2Fexample.test%2Ficon.png&lock=false",
    ] {
        assert_eq!(
            Totp::parse(written).expect(written).code(59).expect("code").0.expose(),
            "996554",
            "{written}"
        );
    }
    // Six digits, and the seconds left in the step.
    let (code, remaining) = plain.code(30_031).expect("code");
    assert_eq!((code.expose(), remaining), ("082801", 29));
    assert_eq!(plain.code(30_059).expect("code").1, 1);
    let minute = Totp::parse("otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&period=60").expect("period");
    let (code, remaining) = minute.code(30_010).expect("code");
    assert_eq!((code.expose(), remaining), ("204022", 50));
}

#[test]
fn browser_and_bitwarden_imports_roundtrip_quotes_newlines_and_totp_without_logging_secrets() {
    let csv = b"name,url,username,password,note,totp\n\"Name, one\",https://example.test,person,\"quoted\"\"password\",\"two\nlines\",JBSWY3DPEHPK3PXP\n";
    let entries = exchange::import(csv, Some(8)).expect("browser CSV import");
    assert_eq!(
        entries[0].password.expose(),
        "quoted\"password",
        "quoted password preserved"
    );
    assert_eq!(
        entries[0].notes.expose(),
        "two\nlines",
        "multiline note preserved"
    );
    let output = exchange::export_csv(&entries).expect("CSV export");
    let roundtrip = exchange::import(&output, Some(8)).expect("CSV roundtrip");
    assert_eq!(
        entries[0].totp, roundtrip[0].totp,
        "authenticator parameters preserved"
    );
    assert_eq!(
        entries[0].password, roundtrip[0].password,
        "password preserved"
    );
    let bitwarden = br#"{"encrypted":false,"items":[{"type":1,"name":"Fixture","login":{"username":"person","password":"fixture-secret","totp":"JBSWY3DPEHPK3PXP","uris":[{"uri":"https://example.test/login"}]}}]}"#;
    assert_eq!(
        exchange::import(bitwarden, None).expect("Bitwarden import")[0]
            .password
            .expose(),
        "fixture-secret",
        "Bitwarden login mapped"
    );
    let passkeys = br#"{"items":[{"type":1,"name":"Passkey fixture","login":{"fido2Credentials":[{"keyValue":"fixture-private-key-sentinel"}],"uris":[{"uri":"https://example.test"}]}}]}"#;
    let error =
        exchange::import(passkeys, None).expect_err("passkeys are never silently discarded");
    assert!(
        error.contains("passkeys") && !error.contains("fixture-private-key-sentinel"),
        "unsupported passkey migration is explicit and redacted"
    );
    let bad = b"url,username,password\nhttp://unsafe.test,person,DO_NOT_LOG_THIS_PASSWORD\n";
    let error = exchange::import(bad, None).expect_err("unsafe import rejected");
    assert!(
        !error.contains("DO_NOT_LOG_THIS_PASSWORD"),
        "import diagnostic must not disclose a secret"
    );
}

#[test]
fn password_generator_has_bounded_lengths_and_independent_random_outputs() {
    assert!(generate(15).is_err(), "short generator request refused");
    assert!(
        generate(129).is_err(),
        "oversized generator request refused"
    );
    let first = generate(24).expect("secure password");
    let second = generate(24).expect("another secure password");
    assert_eq!(first.expose().len(), 24, "requested length respected");
    assert_ne!(first, second, "independent secure-random passwords");
}

fn service(scratch: &Scratch) -> service::Service {
    let config = scratch.path().with_file_name("settings.json");
    config::save(&config, &Settings::enabled_defaults()).expect("enabled isolated settings");
    service::Service::start(scratch.path(), config).expect("credential worker")
}
fn ask(service: &service::Service, operation: service::Operation) -> service::Reply {
    service
        .request(operation)
        .expect("bounded request")
        .blocking_recv()
        .expect("worker reply")
}
#[test]
fn credential_worker_lock_revokes_pending_grants_and_refuses_secret_reads() {
    let scratch = Scratch::new();
    let service = service(&scratch);
    assert!(
        ask(
            &service,
            service::Operation::Open {
                password: Secret::new(MASTER.into()),
                create: true
            }
        )
        .unlocked,
        "vault unlocked"
    );
    let login = entry("https://example.test", None);
    let id = login.id.clone();
    assert!(
        ask(&service, service::Operation::Save(login))
            .result
            .is_ok(),
        "saved fixture"
    );
    let lease = service.lease();
    assert!(
        lease.authorize(),
        "live lease has current private file custody"
    );
    service.lock();
    assert!(!lease.valid(), "locking revokes the grant synchronously");
    let denied = ask(&service, service::Operation::Read(id, None));
    assert!(
        !denied.unlocked && denied.result.is_err(),
        "locked worker releases no login"
    );
}
#[test]
fn credential_worker_cancelled_unlock_cannot_reopen_the_vault() {
    let scratch = Scratch::new();
    let service = service(&scratch);
    let pending = service
        .request(service::Operation::Open {
            password: Secret::new(MASTER.into()),
            create: true,
        })
        .expect("pending unlock");
    service.lock();
    let reply = pending.blocking_recv().expect("cancelled reply");
    assert!(
        !reply.unlocked && reply.result.is_err(),
        "late crypto completion cannot unlock after cancellation"
    );
    assert!(!service.unlocked(), "published lock status remains closed");
}
#[test]
fn credential_worker_global_disable_and_foreign_rekey_revoke_existing_access() {
    let scratch = Scratch::new();
    let service = service(&scratch);
    assert!(
        ask(
            &service,
            service::Operation::Open {
                password: Secret::new(MASTER.into()),
                create: true
            }
        )
        .unlocked,
        "fixture unlock"
    );
    let lease = service.lease();
    let mut foreign = Vault::unlock(&scratch.path(), MASTER).expect("another window");
    foreign
        .rekey(MASTER, "a new master from another window")
        .expect("foreign rekey");
    assert!(
        !lease.authorize(),
        "changed vault identity refuses a queued native fill immediately"
    );
    let reply = ask(&service, service::Operation::List(None));
    assert!(
        !reply.unlocked && reply.result.is_err(),
        "foreign rekey retires the old key and snapshot"
    );
    assert!(
        ask(
            &service,
            service::Operation::Open {
                password: Secret::new("a new master from another window".into()),
                create: false
            }
        )
        .unlocked,
        "new password works"
    );
    config::save(
        &scratch.path().with_file_name("settings.json"),
        &Settings::default(),
    )
    .expect("disable in another window");
    let reply = ask(&service, service::Operation::List(None));
    assert!(
        !reply.unlocked && reply.result.is_err(),
        "disabled provider releases no metadata or secrets"
    );
}

#[test]
fn credential_provider_handoff_disables_builtin_without_fallback() {
    let scratch = Scratch::new();
    let settings = scratch.0.path().join("settings.json");
    let mut configuration = Settings::enabled_defaults();
    configuration.provider = "future.extension.provider".into();
    config::save(&settings, &configuration).expect("provider handoff");
    let service = service::Service::start(scratch.path(), settings).expect("worker");
    let reply = ask(
        &service,
        service::Operation::Open {
            password: Secret::new(MASTER.into()),
            create: true,
        },
    );
    assert!(
        reply.result.is_err() && !reply.unlocked,
        "another provider cannot fall back to a built-in vault"
    );
    assert!(
        !scratch.path().exists(),
        "disabled provider did not create a vault"
    );
}

#[test]
fn credential_fill_encoding_binds_origin_container_and_keeps_totp_separate() {
    use crate::credential_bridge::{Document, Event, fill_script};
    let site = Site::parse("https://example.test", Some(7)).expect("site");
    let document = Document {
        document: "0123456789abcdef0123456789abcdef".into(),
        origin: site.origin.clone(),
        target: "1".into(),
        username_only: false,
    };
    let mut login = entry("https://example.test", Some(7));
    login.totp = Some(Totp::parse("JBSWY3DPEHPK3PXP").expect("authenticator"));
    let script = fill_script(&document, &site, &login, true).expect("TOTP fill");
    assert!(
        script.expose_secret().contains(",null,null,"),
        "code-only fill releases neither username nor password"
    );
    assert!(
        !script.expose_secret().contains(login.password.expose()),
        "code-only source excludes password"
    );
    assert!(
        !script.expose_secret().contains("JBSWY3DPEHPK3PXP"),
        "source excludes authenticator seed"
    );
    let password = fill_script(&document, &site, &login, false).expect("password fill");
    assert!(
        !format!("{password:?}").contains(login.password.expose()),
        "source diagnostics are redacted"
    );
    let username_document = Document {
        username_only: true,
        ..document.clone()
    };
    let username_source =
        fill_script(&username_document, &site, &login, false).expect("username fill");
    assert!(
        username_source
            .expose_secret()
            .contains(login.username.expose())
    );
    assert!(
        !username_source
            .expose_secret()
            .contains(login.password.expose()),
        "a username-only form receives no password"
    );
    assert!(!username_source.expose_secret().contains("JBSWY3DPEHPK3PXP"));
    let other = Site::parse("https://example.test", Some(8)).expect("other container");
    assert!(
        fill_script(&document, &other, &login, false).is_err(),
        "container mismatch refused"
    );
    let other = Document {
        origin: "https://evil.test".into(),
        ..document
    };
    assert!(
        fill_script(&other, &site, &login, false).is_err(),
        "document origin mismatch refused"
    );
    let submitted = enzui_security::secret::SecretText::new(r#"{"kind":"submitted","document":"fixture","origin":"https://example.test","action":"https://example.test","username":"fixture-user-sentinel","password":"fixture-password-sentinel"}"#.into());
    let event = Event::parse(&submitted).expect("private submission");
    assert!(
        !format!("{event:?}").contains("sentinel"),
        "captured fields are never exposed in Debug"
    );
}

#[test]
fn updating_a_submitted_password_preserves_authenticator_notes_and_scope() {
    let mut existing = entry("https://example.test", Some(7));
    existing.name = Secret::new("My named login".into());
    existing.notes = Secret::new("Keep this note".into());
    existing.totp = Some(Totp::parse("JBSWY3DPEHPK3PXP").expect("authenticator"));
    let original = existing.clone();
    let mut submitted = entry("https://example.test", Some(7));
    submitted.password = Secret::new("a newly submitted fixture password".into());
    existing
        .update_password(&submitted)
        .expect("matching account update");
    assert_eq!(existing.password, submitted.password, "new password saved");
    assert_eq!(
        (
            &existing.id,
            &existing.name,
            &existing.notes,
            &existing.totp,
            &existing.site
        ),
        (
            &original.id,
            &original.name,
            &original.notes,
            &original.totp,
            &original.site
        ),
        "password update preserves other saved data"
    );
    for (site, username) in [
        (
            Site::parse("https://evil.test", Some(7)).expect("other site"),
            submitted.username.clone(),
        ),
        (
            Site::parse("https://example.test", Some(8)).expect("other container"),
            submitted.username.clone(),
        ),
        (submitted.site.clone(), Secret::new("another user".into())),
    ] {
        let wrong = Credential {
            site,
            username,
            ..submitted.clone()
        };
        assert!(
            existing.update_password(&wrong).is_err(),
            "other origin/account/container update refused"
        );
        assert_eq!(
            existing.password, submitted.password,
            "refused update preserved current password"
        );
    }
}

#[test]
fn credential_worker_sleeps_until_change_and_retires_without_a_timer() {
    let scratch = Scratch::new();
    let settings_root = Scratch::new();
    let settings_path = settings_root.path().with_file_name("settings.json");
    config::save(&settings_path, &Settings::enabled_defaults()).expect("enabled fixture");
    drop(Vault::create(&scratch.path(), MASTER).expect("existing vault"));
    let service = service::Service::start(scratch.path(), settings_path.clone()).expect("worker");
    assert!(
        ask(
            &service,
            service::Operation::Open {
                password: Secret::new(MASTER.into()),
                create: false,
            }
        )
        .unlocked
    );
    let mut changes = service.test_changes();
    let _ = changes.borrow_and_update();
    std::thread::sleep(std::time::Duration::from_millis(150));
    let before = service.test_cycles();
    std::thread::sleep(std::time::Duration::from_millis(1250));
    assert_eq!(
        service.test_cycles(),
        before,
        "an idle unlocked provider performs no periodic work"
    );
    let lease = service.lease();
    let mut foreign = Vault::unlock(&scratch.path(), MASTER).expect("foreign window");
    foreign
        .rekey(MASTER, "another distinct fixture password")
        .expect("foreign publication");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("test executor");
    runtime.block_on(async {
        tokio::time::timeout(std::time::Duration::from_secs(3), changes.changed())
            .await
            .expect("change notification without a request")
            .expect("live worker");
    });
    assert!(
        !service.unlocked() && !lease.valid(),
        "directory notification retires foreign vault custody without polling"
    );
    assert!(
        ask(
            &service,
            service::Operation::Open {
                password: Secret::new("another distinct fixture password".into()),
                create: false,
            }
        )
        .unlocked
    );
    let _ = changes.borrow_and_update();
    config::save(&settings_path, &Settings::default()).expect("foreign disable");
    runtime.block_on(async {
        tokio::time::timeout(std::time::Duration::from_secs(3), changes.changed())
            .await
            .expect("settings notification without polling")
            .expect("live worker");
    });
    assert!(
        !service.unlocked(),
        "external disable revokes keys without a UI tick"
    );
    drop(service);
    runtime.block_on(async {
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while changes.changed().await.is_ok() {}
        })
        .await
        .expect("dropping service releases its watcher and worker");
    });
}
