#[cfg(feature = "benchmark")]
pub(super) fn run_followup_gate(scratch: PathBuf) -> Result<serde_json::Value, String> {
    use enzui_webkit::page::{PageEvent, ScriptWorld};
    fn evaluate(worker: &mut Worker, source: &str) -> Result<serde_json::Value, String> {
        let id = worker
            .session
            .evaluate_script(source, &ScriptWorld::Page)
            .map_err(|e| e.to_string())?;
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            worker.session.pump(2);
            let mut response = None;
            for event in worker.session.take_page_events() {
                if let PageEvent::ScriptMessage {
                    ref handler,
                    ref json,
                } = event
                    && handler == "enzimCosmetics"
                {
                    worker.cosmetic_message(json);
                }
                if let PageEvent::ScriptResult { id: found, result } = event
                    && id == found
                {
                    response = Some(
                        result
                            .map_err(|e| format!("{e:?}"))
                            .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string())),
                    );
                }
            }
            if let Some(response) = response {
                return response;
            }
        }
        Err("Page evaluation timed out".into())
    }
    fn copy(worker: &mut Worker, view: enzui_webkit::web::ViewId) -> Result<String, String> {
        let started = std::time::Instant::now();
        let mut update = worker.clipboard(view, HostGesture::Present, None);
        let dispatch = started.elapsed();
        if dispatch > Duration::from_millis(50) {
            return Err(format!("Copy blocked the UI for {dispatch:?}"));
        }
        while update.selection.is_none()
            && update.clipboard.is_none()
            && started.elapsed() < Duration::from_secs(5)
        {
            worker.session.pump(2);
            update = worker.update(None, false);
        }
        eprintln!(
            "COPY_RESPONSIVENESS dispatch_us={} complete_us={}",
            dispatch.as_micros(),
            started.elapsed().as_micros()
        );
        update
            .selection
            .map(|selection| selection.text)
            .ok_or_else(|| format!("Copy failed: {:?}", update.clipboard))
    }
    fn permission(worker: &mut Worker, allow: bool) -> Result<(), String> {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            worker.session.pump(2);
            for event in worker.session.take_page_events() {
                if let PageEvent::PermissionRequested(request) = event {
                    if request.permission.kind != enzui_webkit::web::PermissionKind::Microphone {
                        return Err(format!("Unexpected request: {request:?}"));
                    }
                    worker
                        .session
                        .answer_permission(request.id, allow)
                        .map_err(|e| format!("Permission reply rejected: {e:?}"))?;
                    return Ok(());
                }
            }
        }
        Err("Microphone permission request missing".into())
    }
    let mut router = ViewRouter::new();
    let view = router.create(995);
    let mut worker = Worker::open(
        router,
        view,
        scratch,
        Some(enzui_webkit::ViewportSize::from_logical(800., 600.).map_err(|e| e.to_string())?),
        MediaGeneration::FIRST,
        WebProfile::ephemeral(),
    )?;
    crate::privacy::Privacy::default().apply(&worker.session);
    if !worker.session.set_mock_capture_devices(true) {
        return Err("Synthetic devices unavailable".into());
    }
    worker.session.load_html("<p id=selection>Copy Ω selected body</p><input id=field value='Copy Ω input selection'><p id=ordinary>Article survives</p>","https://mictests.com/",10_000).map_err(|e|e.to_string())?;
    worker.session.focus_view();
    if evaluate(&mut worker, "typeof navigator.mediaDevices?.getUserMedia")? != "function" {
        return Err("Media devices API missing".into());
    }
    // Evaluation submits the request; permission events must remain queued for the native prompt check.
    worker.session.evaluate_script("window.captureResult='pending';navigator.mediaDevices.getUserMedia({audio:true}).then(s=>{window.captureResult=s.getAudioTracks().length+' audio';s.getTracks().forEach(t=>t.stop())},e=>window.captureResult=e.name);undefined",&ScriptWorld::Page).map_err(|e|e.to_string())?;
    permission(&mut worker, false)?;
    worker.session.pump(200);
    if evaluate(&mut worker, "captureResult")? != "NotAllowedError" {
        return Err("Denied microphone request captured audio".into());
    }
    worker.session.evaluate_script("window.captureResult='pending';navigator.mediaDevices.getUserMedia({audio:true}).then(s=>{window.captureResult=s.getAudioTracks().length+' audio';s.getTracks().forEach(t=>t.stop())},e=>window.captureResult=e.name);undefined",&ScriptWorld::Page).map_err(|e|e.to_string())?;
    permission(&mut worker, true)?;
    worker.session.pump(500);
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let value = evaluate(&mut worker, "captureResult")?;
        if value == "1 audio" {
            break;
        }
        if value != "pending" || std::time::Instant::now() > deadline {
            return Err(format!("Allowed synthetic capture failed: {value}"));
        }
        worker.session.pump(100);
    }
    evaluate(
        &mut worker,
        "getSelection().selectAllChildren(document.querySelector('#selection'));'selected'",
    )?;
    if copy(&mut worker, view)? != "Copy Ω selected body" {
        return Err("Body copy returned unexpected text".into());
    }
    if worker
        .clipboard(view, HostGesture::Absent, None)
        .selection
        .is_some()
    {
        return Err("Copy without gesture accepted".into());
    }
    evaluate(
        &mut worker,
        "getSelection().removeAllRanges();const field=document.querySelector('#field');field.focus();field.setSelectionRange(7,12);'selected'",
    )?;
    if copy(&mut worker, view)? != "input" {
        return Err("Input copy returned unexpected text".into());
    }
    evaluate(
        &mut worker,
        "document.addEventListener('copy',e=>{e.preventDefault();e.clipboardData.setData('text/plain','Custom copy handler')});'ready'",
    )?;
    worker
        .session
        .evaluate_script(
            "const busyUntil=performance.now()+1000;while(performance.now()<busyUntil){};undefined",
            &ScriptWorld::Page,
        )
        .map_err(|e| e.to_string())?;
    if copy(&mut worker, view)? != "Custom copy handler" {
        return Err("Busy-page copy handler was bypassed".into());
    }
    evaluate(
        &mut worker,
        "document.addEventListener('copy',()=>{const until=performance.now()+200;while(performance.now()<until){}});'ready'",
    )?;
    worker.clipboard(view, HostGesture::Present, None);
    worker.session.blur_view();
    worker.session.pump(300);
    if worker
        .update(None, false)
        .selection
        .map(|selection| selection.text)
        .as_deref()
        != Some("Custom copy handler")
    {
        return Err("Accepted copy was cancelled by ordinary window focus loss".into());
    }
    worker.session.focus_view();
    worker.clipboard(view, HostGesture::Present, None);
    worker.visible = false;
    worker.session.pump(300);
    if worker.update(None, false).selection.is_some() || worker.pending_copy.is_some() {
        return Err("Hidden page published a pending clipboard result".into());
    }
    worker.visible = true;
    worker.session.focus_view();
    let mut pending = worker
        .session
        .begin_page_copy(&worker.display, &worker.router, view, HostGesture::Present)
        .map_err(|e| e.to_string())?;
    worker
        .session
        .begin_load_uri("about:blank")
        .map_err(|e| e.to_string())?;
    worker.session.pump(300);
    if !matches!(
        pending.poll(&worker.session, &worker.display, &worker.router),
        Some(Err(_))
    ) {
        return Err("Navigated document completed a stale clipboard request".into());
    }
    Worker::install_scripts(
        &mut worker.session,
        true,
        Some("https://www.youtube.com/watch?v=fixture"),
        true,
    );
    worker.filter_mode.set(Some(1));
    let youtube_fixture = "<script>window.ytInitialPlayerResponse={playerAds:[{ad:true}],adPlacements:[{ad:true}],adSlots:[{ad:true}],videoDetails:{videoId:'keep-video'},streamingData:{formats:[{url:'keep-stream'}]}};window.parsed=JSON.parse('{\"adPlacements\":[{\"ad\":true}],\"videoDetails\":{\"videoId\":\"keep-json\"}}');</script><ytd-ad-slot-renderer>Ad container</ytd-ad-slot-renderer><main id=ordinary>Keep article</main>";
    worker
        .session
        .load_html(
            youtube_fixture,
            "https://www.youtube.com/watch?v=fixture",
            10_000,
        )
        .map_err(|e| e.to_string())?;
    let values = evaluate(
        &mut worker,
        "JSON.stringify({ads:ytInitialPlayerResponse.playerAds,placements:ytInitialPlayerResponse.adPlacements,slots:ytInitialPlayerResponse.adSlots,video:ytInitialPlayerResponse.videoDetails.videoId,stream:ytInitialPlayerResponse.streamingData.formats[0].url})",
    )?;
    let values: serde_json::Value =
        serde_json::from_str(values.as_str().ok_or("Invalid scriptlet result")?)
            .map_err(|e| e.to_string())?;
    if values.get("ads").is_some_and(|v| !v.is_null())
        || values.get("placements").is_some_and(|v| !v.is_null())
        || values.get("slots").is_some_and(|v| !v.is_null())
        || values["video"] != "keep-video"
        || values["stream"] != "keep-stream"
    {
        return Err(format!(
            "Player ad pruning failed or damaged content: {values}"
        ));
    }
    Worker::install_scripts(&mut worker.session, false, None, true);
    worker.filter_mode.set(Some(0));
    worker
        .session
        .load_html(
            youtube_fixture,
            "https://www.youtube.com/watch?v=fixture",
            10_000,
        )
        .map_err(|e| e.to_string())?;
    if evaluate(&mut worker, "ytInitialPlayerResponse.playerAds.length")? != 1 {
        return Err("Turning off protection did not restore the positive ad control".into());
    }
    worker.filter_mode.set(Some(1));
    let rules = crate::filters::cosmetic_rules("https://edition.cnn.com/", &[], &[]);
    if !rules.iter().any(|s| s.contains(".ad-slot-header__wrapper")) {
        return Err("CNN wrapper filter missing".into());
    }
    Worker::install_scripts(&mut worker.session, true, Some("https://edition.cnn.com/"), true);
    worker.session.load_html("<div class=ad-slot-header__wrapper style='height:300px'>Ad</div><main id=ordinary style='height:120px'>Keep article</main>","https://edition.cnn.com/",10_000).map_err(|e|e.to_string())?;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        worker.session.pump(20);
        let _ = worker.update(None, false);
        let value = evaluate(
            &mut worker,
            "[document.querySelector('.ad-slot-header__wrapper').getBoundingClientRect().height,document.querySelector('#ordinary').getBoundingClientRect().height]",
        )?;
        if value == serde_json::json!([0, 120]) {
            break;
        }
        if std::time::Instant::now() > deadline {
            return Err(format!(
                "Cosmetic container collapse failed: {value}; metadata={:?}; actual={:?}",
                worker.metadata_snapshot().0.location,
                evaluate(&mut worker, "location.href")?
            ));
        }
    }
    Ok(
        serde_json::json!({"microphone_api":true,"microphone_request_deny_then_allow":true,"synthetic_audio_tracks":1,"busy_page_custom_copy_handler":true,"nonblocking_copy_dispatch":true,"body_copy":true,"input_copy":true,"copy_requires_gesture":true,"youtube_player_ad_pruning_preserves_video":true,"youtube_protection_off_restores_ad_control":true,"cnn_container_collapses_preserving_article":true}),
    )
}
