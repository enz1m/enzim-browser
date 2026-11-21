#[cfg(feature = "benchmark")]
pub(super) fn run_encrypted_media_gate(scratch: PathBuf) -> Result<serde_json::Value, String> {
    use enzui_webkit::{
        page::{PageEvent, ScriptWorld},
        web::policy::PermissionKind,
    };
    let base = std::env::var("ENZIM_CLEARKEY_BASE").map_err(|_| "ENZIM_CLEARKEY_BASE required")?;
    let certificate = std::fs::read(
        std::env::var("ENZIM_CLEARKEY_CERT").map_err(|_| "ENZIM_CLEARKEY_CERT required")?,
    )
    .map_err(|e| e.to_string())?;
    let origin = crate::privacy::Privacy::origin(&base).ok_or("Invalid fixture origin")?;
    let mut results = Vec::new();
    for (enabled, allow) in [(false, true), (true, false), (true, true)] {
        let mut router = ViewRouter::new();
        let view = router.create(989);
        let mut worker = Worker::open(
            router,
            view,
            scratch.clone(),
            None,
            MediaGeneration::FIRST,
            WebProfile::ephemeral(),
        )?;
        if !allow {
            let mut policy = enzui_webkit::web::WebPolicy::strict();
            policy.allow_downloads = true;
            policy.allow_popups = true;
            worker
                .session
                .install_native_policy(&worker.router, worker.view, policy, |request| {
                    Some(request.id)
                })
                .map_err(|e| e.to_string())?;
            worker
                .session
                .set_origin_permission_mode(true)
                .map_err(|e| e.to_string())?;
        }
        worker.session.disable_gpu_presentation();
        worker.gpu_enabled = false;
        worker
            .session
            .local_origins()
            .ok_or("Missing local origin authority")?
            .allow_origin(&origin)
            .map_err(|e| e.to_string())?;
        worker
            .session
            .allow_tls_certificate_for_host(&certificate, "127.0.0.1")
            .map_err(|e| e.to_string())?;
        let privacy = crate::privacy::Privacy {
            encrypted_media: enabled,
            autoplay: true,
            ..Default::default()
        };
        privacy.apply(&worker.session);
        worker
            .session
            .load(&format!("{base}/clearkey-page"), 15_000)
            .map_err(|e| e.to_string())?;
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        let mut prompts = 0;
        loop {
            let id = worker
                .session
                .evaluate_script("window.eme || null", &ScriptWorld::Page)
                .map_err(|e| e.to_string())?;
            let state = loop {
                worker.session.pump(2);
                let mut state = None;
                for event in worker.session.take_page_events() {
                    match event {
                        PageEvent::PermissionRequested(request) => {
                            if request.permission.kind != PermissionKind::MediaKeySystem
                                || request.permission.requesting_origin.as_str() != origin
                                || request.permission.top_origin.as_str() != origin
                            {
                                return Err(format!(
                                    "Unexpected key-system permission authority: {request:?}"
                                ));
                            }
                            prompts += 1;
                            worker
                                .session
                                .answer_permission(request.id, allow)
                                .map_err(|e| e.to_string())?;
                        }
                        PageEvent::ScriptResult { id: found, result } if found == id => {
                            state = Some(result.map_err(|e| format!("{e:?}"))?);
                        }
                        _ => {}
                    }
                }
                if let Some(state) = state {
                    break serde_json::from_str::<serde_json::Value>(&state)
                        .map_err(|e| e.to_string())?;
                }
                if std::time::Instant::now() > deadline {
                    return Err("Encrypted-media fixture evaluation stalled".into());
                }
            };
            if !enabled && state["api"] == "undefined" {
                if prompts != 0 {
                    return Err("Disabled encrypted media requested permission".into());
                }
                results.push(serde_json::json!({"enabled":false,"prompts":prompts,"state":state}));
                break;
            }
            if enabled
                && !allow
                && state["error"]
                    .as_str()
                    .is_some_and(|e| e.starts_with("NotSupportedError"))
                && prompts <= 1
            {
                results.push(serde_json::json!({"enabled":true,"allowed":false,"prompts":prompts,"state":state}));
                break;
            }
            if enabled
                && allow
                && state["time"].as_f64().is_some_and(|t| t > 1.5)
                && state["lit"].as_u64().is_some_and(|n| n > 500)
                && state["width"] == 320
                && state["licensed"] == true
                && prompts <= 1
            {
                if state["requested"] != serde_json::json!(["ASNFZ4mrze8BI0VniavN7w"]) {
                    return Err(format!("Wrong requested key ID: {state}"));
                }
                results.push(serde_json::json!({"enabled":true,"allowed":true,"prompts":prompts,"state":state}));
                break;
            }
            if enabled && state["error"].is_string() && allow
                || std::time::Instant::now() > deadline
            {
                return Err(format!(
                    "Browser encrypted media enabled={enabled}, allow={allow}, prompts={prompts}: {state}"
                ));
            }
            worker.session.pump(20);
        }
    }
    Ok(
        serde_json::json!({"origin_mode":true,"key_system_authority":"explicit view policy grant","controls":results}),
    )
}
