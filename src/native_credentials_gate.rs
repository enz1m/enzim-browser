#[cfg(feature = "benchmark")]
pub(super) fn run_credentials_gate(scratch: PathBuf) -> Result<serde_json::Value, String> {
    use crate::{
        credential_bridge as bridge,
        credentials::{
            self, Credential, Secret, Site,
            service::{Operation, Service},
        },
    };
    use enzui_security::secret::SecretText;
    static CAPTURES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    fn events(worker: &Worker) -> Result<(), String> {
        for event in worker.session.take_page_events() {
            if format!("{event:?}").contains("credential-fixture-password") {
                return Err("Private fixture leaked into diagnostics".into());
            }
            if let enzui_webkit::page::PageEvent::SecretScriptMessage { json, .. } = event
                && matches!(
                    bridge::Event::parse(&json),
                    Ok(bridge::Event::Submitted { .. })
                )
            {
                CAPTURES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
        Ok(())
    }
    fn private(worker: &mut Worker, source: &str) -> Result<SecretText, String> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let operation = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut job = worker
            .session
            .evaluate_secret_script(&SecretText::new(source.to_owned()), bridge::WORLD)
            .map_err(|_| "Private evaluation did not start".to_owned())?;
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        loop {
            worker.session.pump(2);
            events(worker)?;
            if let Some(result) = job.take() {
                return result
                    .map_err(|error| format!("Private evaluation {operation} failed: {error:?}"));
            }
            if std::time::Instant::now() > deadline {
                return Err("Private evaluation timed out".into());
            }
        }
    }
    fn page_boolean(worker: &mut Worker, source: &str) -> Result<(), String> {
        use enzui_webkit::page::{PageEvent, ScriptWorld};
        let id = worker
            .session
            .evaluate_script(source, &ScriptWorld::Page)
            .map_err(|_| "Page fixture did not start".to_owned())?;
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        loop {
            worker.session.pump(2);
            for event in worker.session.take_page_events() {
                if let PageEvent::ScriptResult { id: found, result } = event
                    && id == found
                {
                    return if result.is_ok_and(|json| json == "true") {
                        Ok(())
                    } else {
                        Err("Page fixture assertion failed".into())
                    };
                }
            }
            if std::time::Instant::now() > deadline {
                return Err("Page fixture timed out".into());
            }
        }
    }
    fn renew_fixture_grant(service: &Service) -> Result<(), String> {
        service.lock();
        let (_, settings_identity) =
            credentials::config::load_authorized(&credentials::config::path())?;
        service
            .request(Operation::OpenSource {
                source: "browser".into(),
                create: false,
                authentication: credentials::accounts::Authentication {
                    password: Secret::new("isolated credential gate vault password".into()),
                    settings_identity,
                    client_id: String::new(),
                    client_secret: Secret::default(),
                    second_factor: Secret::default(),
                },
            })?
            .blocking_recv()
            .map_err(|_| "Fixture unlock reply was lost".to_owned())?
            .result
            .map(|_| ())
    }
    fn refused(
        worker: &mut Worker,
        service: &Service,
        site: &Site,
        source: &SecretText,
        label: &str,
    ) -> Result<(), String> {
        let lease = service.lease();
        if !lease.authorize() {
            return Err(format!(
                "Credential refusal fixture has no valid grant: {label}"
            ));
        }
        let (job, mut receive) = crate::native_jobs::Job::new();
        worker.credential_job(
            lease,
            site.clone(),
            worker.profile.clone(),
            source.clone(),
            job,
        );
        if matches!(receive.try_recv(), Ok(Err(_))) {
            Ok(())
        } else {
            Err(format!("Native credential gate accepted {label}"))
        }
    }
    fn require(value: &SecretText, expected: &str, label: &str) -> Result<(), String> {
        if value.expose_secret() == expected {
            Ok(())
        } else {
            Err(format!("Credential check failed: {label}"))
        }
    }
    if std::env::var("ENZIM_CREDENTIAL_CHECK").as_deref() != Ok("isolated-fixtures") {
        return Err("The credential gate requires the isolated fixture runner".into());
    }
    credentials::protect_process()?;
    let mut router = ViewRouter::new();
    let view = router.create(996);
    let mut worker = Worker::open(
        router,
        view,
        scratch.clone(),
        Some(enzui_webkit::ViewportSize::from_logical(800., 600.).map_err(|e| e.to_string())?),
        MediaGeneration::FIRST,
        WebProfile::ephemeral(),
    )?;
    worker.session.disable_gpu_presentation();
    worker.gpu_enabled = false;
    worker.visible = true;
    worker
        .session
        .load("https://example.com/", 20_000)
        .map_err(|_| "HTTPS fixture failed to load".to_owned())?;
    if !worker
        .session
        .connection_tls_info()
        .is_some_and(|tls| tls.errors == 0)
    {
        return Err("HTTPS fixture has no authenticated TLS connection".into());
    }
    require(
        &private(
            &mut worker,
            "isSecureContext && location.origin === 'https://example.com'",
        )?,
        "true",
        "authenticated secure context",
    )?;
    let fixture = "document.body.innerHTML='<form id=login><input id=user autocomplete=username><input id=pass type=password autocomplete=current-password><input id=otp autocomplete=one-time-code><button>Sign in</button></form><iframe></iframe>';document.querySelector('form').addEventListener('submit',event=>event.preventDefault());document.querySelector('iframe').contentDocument.body.innerHTML='<input type=password>';document.getElementById('pass').focus();true";
    page_boolean(&mut worker, fixture)?;
    private(&mut worker, bridge::SOURCE)?;
    page_boolean(
        &mut worker,
        "Boolean(document.querySelector('iframe').contentDocument.querySelector('input'))",
    )?;
    let inspected = private(&mut worker, "__enzimCredentials.inspect()")?;
    let document: bridge::Document = serde_json::from_str(inspected.expose_secret())
        .map_err(|_| "Fixture form identity failed".to_owned())?;
    let site = Site::parse("https://example.com/", None)?;
    let mut login = Credential::new(site.clone())?;
    login.username = Secret::new("credential-fixture-user".into());
    login.password = Secret::new("credential-fixture-password".into());
    login.totp = Some(credentials::Totp::parse("JBSWY3DPEHPK3PXP")?);
    let source = bridge::fill_script(&document, &site, &login, false)?;
    let (config, settings_identity) =
        credentials::config::load_authorized(&credentials::config::path())?;
    if !config.enabled || !config.private_fill {
        return Err("The fixture requires enabled private autofill".into());
    }
    let service = Service::start(
        scratch.join("fixture-vault.enzim"),
        credentials::config::path(),
    )?;
    let opened = service
        .request(Operation::OpenSource {
            source: "browser".into(),
            authentication: credentials::accounts::Authentication {
                password: Secret::new("isolated credential gate vault password".into()),
                settings_identity,
                client_id: String::new(),
                client_secret: Secret::default(),
                second_factor: Secret::default(),
            },
            create: true,
        })?
        .blocking_recv()
        .map_err(|_| "Vault fixture worker stopped".to_owned())?;
    opened.result?;
    let (job, mut receive) = crate::native_jobs::Job::new();
    worker.credential_job(
        service.lease(),
        site.clone(),
        worker.profile.clone(),
        source.clone(),
        job,
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let filled = loop {
        worker.session.pump(2);
        for mut operation in std::mem::take(&mut worker.native_operations) {
            if !operation(&mut worker) {
                worker.native_operations.push(operation);
            }
        }
        if let Ok(result) = receive.try_recv() {
            break result?;
        }
        if std::time::Instant::now() > deadline {
            return Err("Native credential dispatch stalled".into());
        }
    };
    if !matches!(filled, crate::native_jobs::Payload::SecretJson(ref value) if value.expose_secret() == "true")
    {
        return Err("Native credential dispatch refused the authenticated fixture".into());
    }
    page_boolean(
        &mut worker,
        "document.getElementById('user').value==='credential-fixture-user' && document.getElementById('pass').value==='credential-fixture-password' && document.querySelector('iframe').contentDocument.querySelector('input').value === ''",
    )?;
    require(
        &private(&mut worker, source.expose_secret())?,
        "false",
        "nonempty password cannot be overwritten",
    )?;
    private(
        &mut worker,
        "document.getElementById('pass').value='';document.getElementById('user').value='';document.querySelector('form').action='https://example.org/';true",
    )?;
    require(
        &private(&mut worker, source.expose_secret())?,
        "false",
        "cross-origin form rejected",
    )?;
    private(
        &mut worker,
        "document.querySelector('form').action='';document.getElementById('pass').style.display='none';true",
    )?;
    require(
        &private(&mut worker, source.expose_secret())?,
        "false",
        "hidden password rejected",
    )?;
    private(
        &mut worker,
        "document.getElementById('pass').style.display='';document.getElementById('pass').outerHTML='<input id=pass type=password autocomplete=current-password>';document.getElementById('pass').focus();true",
    )?;
    require(
        &private(&mut worker, source.expose_secret())?,
        "false",
        "replaced field identity rejected",
    )?;
    let document: bridge::Document =
        serde_json::from_str(private(&mut worker, "__enzimCredentials.inspect()")?.expose_secret())
            .map_err(|_| "Updated fixture identity failed".to_owned())?;
    let code = bridge::fill_script(&document, &site, &login, true)?;
    require(
        &private(&mut worker, code.expose_secret())?,
        "true",
        "authenticator fill",
    )?;
    require(
        &private(
            &mut worker,
            "document.getElementById('otp').value.length===6 && document.getElementById('pass').value==='' && document.getElementById('user').value===''",
        )?,
        "true",
        "authenticator never releases password",
    )?;
    private(&mut worker, "__enzimCredentials.disable();true")?;
    let source = bridge::fill_script(&document, &site, &login, false)?;
    require(
        &private(&mut worker, source.expose_secret())?,
        "false",
        "disabled bridge rejects fill",
    )?;
    page_boolean(
        &mut worker,
        "typeof globalThis.__enzimCredentials === 'undefined' && typeof window.webkit?.messageHandlers?.enzimCredentials === 'undefined'",
    )?;
    private(
        &mut worker,
        "__enzimCredentials.configure(true);document.getElementById('pass').value='credential-fixture-password';document.getElementById('user').value='credential-fixture-user';document.getElementById('pass').focus();document.getElementById('pass').dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true}));document.querySelector('form').dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));true",
    )?;
    if CAPTURES.load(std::sync::atomic::Ordering::Relaxed) != 0 {
        return Err("Synthetic page events captured a password".into());
    }
    use enzui_webkit::input::{InputEvent, KeyCode, Modifiers, NamedKey};
    for event in [
        InputEvent::FocusGained {
            view,
            modifiers: Modifiers::NONE,
        },
        InputEvent::KeyPressed {
            view,
            key: KeyCode::Named(NamedKey::Enter),
            modifiers: Modifiers::NONE,
        },
        InputEvent::KeyReleased {
            view,
            key: KeyCode::Named(NamedKey::Enter),
            modifiers: Modifiers::NONE,
        },
    ] {
        if !worker.session.forward_input(&event) {
            return Err("Trusted credential fixture input refused".into());
        }
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while CAPTURES.load(std::sync::atomic::Ordering::Relaxed) != 1 {
        worker.session.pump(5);
        events(&worker)?;
        if std::time::Instant::now() > deadline {
            return Err("Trusted submission did not produce exactly one private save offer".into());
        }
    }
    private(
        &mut worker,
        "const host=document.createElement('div');document.body.append(host);const root=host.attachShadow({mode:'open'});root.innerHTML='<form><input autocomplete=username><input type=password autocomplete=current-password></form>';root.querySelector('input[type=password]').focus();true",
    )?;
    let shadow: bridge::Document =
        serde_json::from_str(private(&mut worker, "__enzimCredentials.inspect()")?.expose_secret())
            .map_err(|_| "Shadow form identity failed".to_owned())?;
    let shadow_source = bridge::fill_script(&shadow, &site, &login, false)?;
    require(
        &private(&mut worker, shadow_source.expose_secret())?,
        "true",
        "open shadow-root login fill",
    )?;
    page_boolean(
        &mut worker,
        "document.body.innerHTML='<form><input name=username value=decoy><button id=sink type=button>Other control</button></form><form><input id=step name=username type=text></form>';document.getElementById('step').focus();true",
    )?;
    let username_document: bridge::Document =
        serde_json::from_str(private(&mut worker, "__enzimCredentials.inspect()")?.expose_secret())
            .map_err(|_| "Username-only identity failed".to_owned())?;
    if !username_document.username_only {
        return Err("Username-only form was not recognized".into());
    }
    let username_source = bridge::fill_script(&username_document, &site, &login, false)?;
    page_boolean(&mut worker,"document.getElementById('sink').focus();true")?;
    if username_source
        .expose_secret()
        .contains(login.password.expose())
    {
        return Err("Username-only operation received a password".into());
    }
    require(
        &private(&mut worker, username_source.expose_secret())?,
        "true",
        "username-only step fill after focus moves to another form",
    )?;
    page_boolean(
        &mut worker,
        "document.getElementById('step').value==='credential-fixture-user' && document.querySelector('input').value==='decoy' && !document.querySelector('input[type=password]')",
    )?;
    page_boolean(
        &mut worker,
        "document.body.innerHTML='<form><input name=q autocomplete=off></form>';document.querySelector('input').focus();true",
    )?;
    require(
        &private(
            &mut worker,
            "!__enzimCredentials.inspect().username_only && __enzimCredentials.inspect().target === ''",
        )?,
        "true",
        "ordinary text/search input is not a username-only form",
    )?;
    page_boolean(&mut worker,
        "document.body.innerHTML='<form action=\"https://example.org/submit\"><input name=username type=email></form>';document.activeElement.blur();true")?;
    require(&private(&mut worker,
        "__enzimCredentials.inspect().username_only && __enzimCredentials.inspect().action === 'https://example.org'")?,
        "true", "unfocused dynamically inserted input retains its actual form action")?;
    page_boolean(&mut worker,
        "document.querySelector('form').action=location.href;true")?;
    let late_document: bridge::Document = serde_json::from_str(private(&mut worker, "__enzimCredentials.inspect()")?.expose_secret())
        .map_err(|_| "Late form readiness was not JSON".to_owned())?;
    let late_fill = bridge::fill_script(&late_document, &site, &login, false)?;
    require(&private(&mut worker, late_fill.expose_secret())?, "true", "unfocused email-only form fills after explicit inspection")?;
    page_boolean(&mut worker, "document.querySelector('input').value === 'credential-fixture-user'")?;
    let (job, mut receive) = crate::native_jobs::Job::new();
    worker.credential_job(
        service.lease(),
        site.clone(),
        WebProfile::ephemeral(),
        source.clone(),
        job,
    );
    if !matches!(receive.try_recv(), Ok(Err(_))) {
        return Err("Native credential gate accepted another storage profile".into());
    }
    worker.visible = false;
    refused(&mut worker, &service, &site, &source, "hidden page")?;
    worker.visible = true;
    let other_site = Site::parse("https://example.org/", None)?;
    refused(
        &mut worker,
        &service,
        &other_site,
        &source,
        "another committed origin",
    )?;
    let mut restricted = config.clone();
    restricted.autofill = false;
    credentials::config::save(&credentials::config::path(), &restricted)?;
    renew_fixture_grant(&service)?;
    refused(&mut worker, &service, &site, &source, "autofill disabled")?;
    restricted.autofill = true;
    restricted.private_fill = false;
    credentials::config::save(&credentials::config::path(), &restricted)?;
    renew_fixture_grant(&service)?;
    refused(
        &mut worker,
        &service,
        &site,
        &source,
        "private-window access disabled",
    )?;
    credentials::config::save(&credentials::config::path(), &config)?;
    renew_fixture_grant(&service)?;
    worker
        .session
        .load_html(
            "<form><input type=password></form>",
            "https://example.com/",
            10_000,
        )
        .map_err(|_| "Unauthenticated fixture load failed".to_owned())?;
    if worker
        .session
        .connection_tls_info()
        .is_some_and(|tls| tls.errors == 0)
    {
        return Err("Synthetic HTTPS base unexpectedly has certificate custody".into());
    }
    refused(
        &mut worker,
        &service,
        &site,
        &source,
        "HTTPS base without authenticated TLS",
    )?;
    service.lock();
    let (job, mut receive) = crate::native_jobs::Job::new();
    worker.credential_job(service.lease(), site, worker.profile.clone(), source, job);
    if !matches!(receive.try_recv(), Ok(Err(_))) {
        return Err("Locked vault dispatched a native fill".into());
    }
    let mut disabled = config.clone();
    disabled.enabled = false;
    credentials::config::save(&credentials::config::path(), &disabled)?;
    worker.refresh_document_scripts(Some("https://example.com/"));
    worker.configure_credential_document();
    if worker.credential_active {
        return Err("Disabled manager retained an active form bridge".into());
    }
    let mut router = ViewRouter::new();
    let off_view = router.create(997);
    let mut off = Worker::open(
        router,
        off_view,
        scratch.join("disabled-worker"),
        None,
        MediaGeneration::FIRST,
        WebProfile::ephemeral(),
    )?;
    if off.credential_active {
        return Err("Disabled startup installed a form bridge".into());
    }
    require(
        &private(
            &mut off,
            "typeof window.webkit?.messageHandlers?.enzimCredentials",
        )?,
        "\"undefined\"",
        "disabled startup has no credential handler",
    )?;
    credentials::config::save(&credentials::config::path(), &config)?;
    off.refresh_document_scripts(None);
    off.configure_credential_document();
    if !off.credential_active {
        return Err("Enabling an existing page did not register its bridge".into());
    }
    require(
        &private(
            &mut off,
            "typeof window.webkit?.messageHandlers?.enzimCredentials",
        )?,
        "\"object\"",
        "enabling registers private handler",
    )?;
    disabled.enabled = false;
    credentials::config::save(&credentials::config::path(), &disabled)?;
    off.refresh_document_scripts(None);
    off.configure_credential_document();
    if off.credential_active {
        return Err("Turning manager off retained its active bridge".into());
    }
    Ok(
        serde_json::json!({"authenticated_tls_after_process_protection":true,"native_fill":true,"top_frame_only":true,"nonempty_hidden_cross_origin_and_replaced_fields_refused":true,"totp_without_password":true,"disabled_and_locked_refused":true,"page_world_bridge_hidden":true,"synthetic_submission_refused":true,"trusted_submission_captured_once":true,"open_shadow_root_fill":true,"disabled_startup_has_no_handler":true,"existing_page_enable_disable":true,"native_hidden_origin_tls_and_policy_denials":true,"native_storage_profile_denial":true,"username_only_without_password":true,"ordinary_text_field_ignored":true,"unfocused_dynamic_form_fill":true,"unfocused_cross_origin_action_checked":true}),
    )
}
