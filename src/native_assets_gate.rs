#[cfg(feature = "benchmark")]
pub(super) fn run_assets_gate(scratch: PathBuf) -> Result<serde_json::Value, String> {
    use enzui_webkit::page::PageEvent;
    let base = std::env::var("ENZIM_ASSET_BASE")
        .map_err(|_| "ENZIM_ASSET_BASE fixture origin required")?;
    let mut router = ViewRouter::new();
    let view = router.create(996);
    let mut worker = Worker::open(
        router,
        view,
        scratch,
        None,
        MediaGeneration::FIRST,
        WebProfile::ephemeral(),
    )?;
    worker.session.disable_gpu_presentation();
    worker.gpu_enabled = false;
    worker
        .session
        .local_origins()
        .ok_or("Missing local origin authority")?
        .allow_origin(&base)
        .map_err(|e| e.to_string())?;
    worker.privacy.javascript = false;
    worker.privacy.apply(&worker.session);
    let address = format!("{base}/csp");
    worker
        .session
        .load(&address, 10000)
        .map_err(|e| e.to_string())?;
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    let icon = loop {
        worker.session.pump(5);
        let update = worker.update(None, false);
        let found = update.page_events.into_iter().find_map(|event| {
            if let PageEvent::ScriptMessage { handler, json } = event
                && handler == "enzimIcon"
            {
                Some(json)
            } else {
                None
            }
        });
        if let Some(icon) = found {
            break icon;
        }
        if std::time::Instant::now() >= deadline {
            return Err(
                "Native favicon publication missing with JavaScript disabled/CSP connect-src none"
                    .into(),
            );
        }
    };
    let value: serde_json::Value = serde_json::from_str(&icon).map_err(|e| e.to_string())?;
    if value["url"] != address {
        return Err("Native favicon has stale document provenance".into());
    }
    let cache = crate::favicons::Cache::new(true);
    if cache.receive(&address, &icon).is_none() || cache.get(&address).is_none() {
        return Err("Native icon did not enter private memory cache".into());
    }
    if worker.favicons.due.is_some() || worker.favicons.task.is_some() {
        return Err("Completed favicon leaves idle polling armed".into());
    }
    let (job, mut ready) = crate::native_jobs::Job::new();
    worker.start_filter(
        1,
        Arc::new(
            r#"[{"trigger":{"url-filter":"/blocked\\.png$"},"action":{"type":"block"}}]"#.into(),
        ),
        job,
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        worker.session.pump(5);
        worker.update(None, false);
        if let Ok(result) = ready.try_recv() {
            result?;
            break;
        }
        if std::time::Instant::now() >= deadline {
            return Err("Native image filter installation stalled".into());
        }
    }
    worker
        .session
        .load(&format!("{base}/blocked"), 10000)
        .map_err(|e| e.to_string())?;
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while std::time::Instant::now() < deadline {
        worker.session.pump(5);
        if worker.update(None, false).page_events.iter().any(
            |event| matches!(event,PageEvent::ScriptMessage{handler,..} if handler=="enzimIcon"),
        ) {
            return Err("Native favicon bypassed the installed content filter".into());
        }
    }
    Ok(
        serde_json::json!({"native_favicon_with_javascript_disabled_and_csp":true,"document_provenance":true,"private_cache":true,"no_idle_polling_after_completion":true,"native_icon_content_filter":true}),
    )
}
