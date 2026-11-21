#[cfg(feature = "benchmark")]
pub(super) fn run_browser_feature_gate(scratch: PathBuf) -> Result<serde_json::Value, String> {
    use enzui_webkit::page::{PageEvent, ScriptWorld};
    use std::{cell::RefCell, rc::Rc, time::Instant};
    fn wait<T>(
        worker: &Worker,
        result: &Rc<RefCell<Option<Result<T, String>>>>,
    ) -> Result<T, String> {
        let deadline = Instant::now() + Duration::from_secs(25);
        loop {
            worker.session.pump(2);
            if let Some(result) = result.borrow_mut().take() {
                return result;
            }
            if Instant::now() > deadline {
                return Err("Native feature operation timed out.".into());
            }
        }
    }
    fn receive_script(
        worker: &Worker,
        id: enzui_webkit::page::PageRequestId,
    ) -> Result<serde_json::Value, String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            worker.session.pump(2);
            for event in worker.session.take_page_events() {
                if let PageEvent::ScriptResult { id: found, result } = event
                    && found == id
                {
                    return result
                        .map_err(|e| format!("{e:?}"))
                        .and_then(|json| serde_json::from_str(&json).map_err(|e| e.to_string()));
                }
            }
            if Instant::now() > deadline {
                return Err("Script evaluation timed out.".into());
            }
        }
    }
    fn evaluate_in(
        worker: &Worker,
        source: &str,
        world: &ScriptWorld,
    ) -> Result<serde_json::Value, String> {
        let id = worker
            .session
            .evaluate_script(source, world)
            .map_err(|e| e.to_string())?;
        receive_script(worker, id)
    }
    fn evaluate(worker: &Worker, source: &str) -> Result<serde_json::Value, String> {
        evaluate_in(worker, source, &ScriptWorld::Page)
    }
    fn preview(worker: &Worker, source: &str) -> Result<serde_json::Value, String> {
        let id = worker
            .session
            .evaluate_script_preview(source, &ScriptWorld::Page)
            .map_err(|e| e.to_string())?;
        receive_script(worker, id)
    }
    fn page_export(
        worker: &Worker,
        format: enzui_webkit::page_export::PageExportFormat,
    ) -> Result<Vec<u8>, String> {
        let task = worker
            .session
            .export_page(
                format,
                &enzui_webkit::page_export::PageExportOptions {
                    max_bytes: 16 * 1024 * 1024,
                    ..Default::default()
                },
            )
            .map_err(|e| e.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            worker.session.pump(2);
            if let Some(result) = task.take_result() {
                return result
                    .map(|artifact| artifact.as_bytes().to_vec())
                    .map_err(|e| e.to_string());
            }
            if Instant::now() >= deadline {
                return Err("Native export task timed out.".into());
            }
        }
    }
    fn full_snapshot(worker: &Worker) -> Result<image::RgbaImage, String> {
        let bytes = page_export(
            worker,
            enzui_webkit::page_export::PageExportFormat::FullDocumentPng,
        )?;
        image::load_from_memory(&bytes)
            .map(image::DynamicImage::into_rgba8)
            .map_err(|e| e.to_string())
    }
    fn network(worker: &Worker) -> Result<String, String> {
        let _ = evaluate(
            worker,
            "window.__enzimNetwork='pending';fetch('https://www.google-analytics.com/analytics.js?enzim_browser_gate=1',{mode:'no-cors',cache:'no-store'}).then(()=>window.__enzimNetwork='allowed',()=>window.__enzimNetwork='blocked'); 'started'",
        )?;
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            worker.session.pump(10);
            let value = evaluate(worker, "window.__enzimNetwork")?;
            if value != "pending" {
                return value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "Unexpected network probe response.".into());
            }
            if Instant::now() > deadline {
                return Err("Network positive control timed out.".into());
            }
        }
    }
    let mut router = ViewRouter::new();
    let view = router.create(991);
    let size = enzui_webkit::ViewportSize::from_logical(400., 300.).map_err(|e| e.to_string())?;
    let mut worker = Worker::open(
        router,
        view,
        scratch.clone(),
        Some(size),
        MediaGeneration::FIRST,
        WebProfile::ephemeral(),
    )?;
    if worker.metadata_snapshot().0.location.is_some() {
        return Err("Private bootstrap URL leaked into browser metadata.".into());
    }
    worker.session.disable_gpu_presentation();
    worker.gpu_enabled = false;
    let html = "<!doctype html><title>enzim feature qualification</title><style>html,body{margin:0;width:400px;height:1600px;background:#f00000}#bottom{position:absolute;top:1400px;width:400px;height:200px;background:#00f000}</style><p>enzim export text marker</p><div id=bottom>Full document bottom</div>";
    worker
        .session
        .load_html(html, "https://example.com/", 10000)
        .map_err(|e| e.to_string())?;
    worker.session.pump(300);
    if worker.metadata_snapshot().0.dom_report.as_deref() != Some("enzim feature qualification") {
        return Err("Caller document positive control was hidden as a private bootstrap.".into());
    }

    let visible = worker
        .session
        .snapshot(
            enzui_webview::convert::engine_generation(worker.generation),
            FrameSeq(1),
            5000,
        )
        .map_err(|e| e.to_string())?;
    if visible.height() != 300 {
        return Err(format!(
            "Visible capture positive control: {}x{}",
            visible.width(),
            visible.height()
        ));
    }
    let full = full_snapshot(&worker)?;
    if full.width() != 400 || full.height() < 1600 {
        return Err(format!(
            "Full-document capture omitted scroll area: {}x{}",
            full.width(),
            full.height()
        ));
    }
    let pixel = (1500 * full.width() as usize + 20) * 4;
    if full.as_raw().get(pixel..pixel + 3) != Some(&[0, 240, 0]) {
        return Err("Full-document capture lacks the off-screen green marker.".into());
    }
    let out = std::env::var_os("ENZIM_FEATURE_ARTIFACTS")
        .map_or_else(|| scratch.join("feature-artifacts"), PathBuf::from);
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    image::save_buffer(
        out.join("full-page.png"),
        full.as_raw(),
        full.width(),
        full.height(),
        image::ColorType::Rgba8,
    )
    .map_err(|e| e.to_string())?;
    let archive = page_export(&worker, enzui_webkit::page_export::PageExportFormat::Mhtml)?;
    let text = String::from_utf8_lossy(&archive);
    if !text.contains("multipart/related") || !text.contains("enzim export text marker") {
        return Err("MHTML archive lacks its MIME structure or document marker.".into());
    }
    std::fs::write(out.join("page.mhtml"), archive).map_err(|e| e.to_string())?;
    let pdf = worker
        .session
        .export_pdf(&Default::default())
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        worker.session.pump(2);
        let mut done = false;
        for event in worker.session.take_page_events() {
            if let PageEvent::PdfExportFinished { id, result } = event
                && id == pdf
            {
                let bytes = result.map_err(|e| e.to_string())?;
                if !bytes.as_bytes().starts_with(b"%PDF-") {
                    return Err("PDF completion did not contain a PDF.".into());
                }
                std::fs::write(out.join("page.pdf"), bytes.as_bytes())
                    .map_err(|e| e.to_string())?;
                done = true;
            }
        }
        if done {
            break;
        }
        if Instant::now() > deadline {
            return Err("PDF export timed out.".into());
        }
    }
    worker.configure_developer_tools(true);
    worker.session.take_page_events();
    worker
        .session
        .evaluate_script(
            "console.log('enzim native log marker', {n: 7}); true",
            &ScriptWorld::Page,
        )
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        worker.session.pump(2);
        if worker.session.take_page_events().into_iter().any(|event| matches!(event, PageEvent::ScriptMessage { handler, json } if handler == "enzimConsole" && json.contains("enzim native log marker") && json.contains("n: 7"))) { break; }
        if Instant::now() >= deadline {
            return Err("Native page console messages were not delivered.".into());
        }
    }
    let undefined = preview(&worker, "window.__enzimOnce=0;undefined")?;
    let null = preview(&worker, "null")?;
    let string = preview(&worker, "'undefined'")?;
    if undefined["kind"] != "undefined"
        || null["kind"] != "json"
        || null["preview"] != "null"
        || string["preview"] != "\"undefined\""
    {
        return Err(format!(
            "Console primitive previews differ: {undefined} {null} {string}"
        ));
    }
    preview(&worker, "window.__enzimOnce++;undefined")?;
    if evaluate(&worker, "window.__enzimOnce")? != 1 {
        return Err("Console evaluated an expression more than once.".into());
    }
    let (job, mut receiver) = crate::native_jobs::Job::new();
    worker.evaluate_job(
        "window.__enzimPromiseOnce=(window.__enzimPromiseOnce||0)+1;Promise.resolve(42)",
        &ScriptWorld::Page,
        job,
        true,
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        worker.session.pump(2);
        let _ = worker.update(None, false);
        if let Ok(result) = receiver.try_recv() {
            if !matches!(result, Ok(crate::native_jobs::Payload::Json(value)) if value["kind"]=="unavailable")
            {
                return Err("Console misclassified a completed Promise as a thrown error.".into());
            }
            break;
        }
        if Instant::now() > deadline {
            return Err("Promise completion classification timed out.".into());
        }
    }
    if evaluate(&worker, "window.__enzimPromiseOnce")? != 1 {
        return Err("Console retried a Promise expression.".into());
    }
    if !preview(&worker, "throw new Error('Unsupported result type')")
        .is_err_and(|error| error.contains("Failed") && error.contains("Unsupported result type"))
    {
        return Err("Console confused a thrown lookalike error with an unavailable value.".into());
    }
    let inspector_world = ScriptWorld::Isolated("enzim-developer-tools".into());
    let inspect = format!("{};__enzimInspect('#bottom')", include_str!("inspector.js"));
    let node = evaluate_in(&worker, &inspect, &inspector_world)?;
    if node["tag"] != "div" || node["styles"]["background-color"] != "rgb(0, 240, 0)" {
        return Err("Element inspection did not return its actual computed style.".into());
    }
    let node = evaluate_in(
        &worker,
        "__enzimSelected.style.setProperty('background-color','rgb(0, 0, 240)');__enzimSnapshot(__enzimSelected)",
        &inspector_world,
    )?;
    if node["styles"]["background-color"] != "rgb(0, 0, 240)" {
        return Err("Live CSS edit did not change the computed style.".into());
    }
    worker.session.pump(100);
    let edited = full_snapshot(&worker)?;
    if edited.as_raw().get(pixel..pixel + 3) != Some(&[0, 0, 240]) {
        return Err("Live CSS changed metadata without changing rendered pixels.".into());
    }
    worker.session.find("enzim", false, 10000);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        worker.session.pump(2);
        let events = worker.session.take_page_events();
        if events
            .iter()
            .any(|e| matches!(e,PageEvent::FindMatches(Some(n)) if *n>=1))
        {
            break;
        }
        if events.iter().any(|e| matches!(e, PageEvent::FindFailed)) || Instant::now() > deadline {
            return Err("Native page find missed a visible marker.".into());
        }
    }
    worker.session.find_finish();
    worker.configure_developer_tools(false);
    let profile = worker
        .profiles
        .borrow_mut()
        .session_for(&worker.profile)
        .map_err(|e| e.to_string())?;
    profile
        .add_cookie("example.com", "/", "keep", "yes")
        .map_err(|e| e.to_string())?;
    profile
        .add_cookie("example.com", "/", "remove", "yes")
        .map_err(|e| e.to_string())?;
    profile
        .add_cookie("other.test", "/", "other", "yes")
        .map_err(|e| e.to_string())?;
    let inventory = Rc::new(RefCell::new(None));
    let completion = Rc::clone(&inventory);
    worker
        .session
        .cookies(move |r| *completion.borrow_mut() = Some(r));
    let records = wait(&worker, &inventory)?;
    if records.len() != 3 {
        return Err(format!(
            "Cookie inventory positive control expected 3, got {}",
            records.len()
        ));
    }
    let keys = records
        .iter()
        .filter(|c| c.key.name == "remove")
        .map(|c| c.key.clone())
        .collect();
    let deletion = Rc::new(RefCell::new(None));
    let completion = Rc::clone(&deletion);
    worker
        .session
        .delete_cookies(keys, move |r| *completion.borrow_mut() = Some(r));
    if wait(&worker, &deletion)? != 1 {
        return Err("Selective deletion count was wrong.".into());
    }
    let inventory = Rc::new(RefCell::new(None));
    let completion = Rc::clone(&inventory);
    worker
        .session
        .cookies(move |r| *completion.borrow_mut() = Some(r));
    let records = wait(&worker, &inventory)?;
    if records.len() != 2
        || !records.iter().any(|c| c.key.name == "keep")
        || !records.iter().any(|c| c.key.domain.contains("other.test"))
    {
        return Err("Cookie deletion removed unrelated cookies or retained its target.".into());
    }
    if network(&worker)? != "allowed" {
        return Err(
            "Adblock network positive control could not fetch the live analytics resource.".into(),
        );
    }
    let filters = crate::filters::compiled(2)?;
    let store =
        enzui_webkit::ffi::content_filters::ContentFilterStore::new(out.join("upstream-filters"))
            .map_err(|e| e.to_string())?;
    let task = store
        .compile(&filters.json, Duration::from_secs(15))
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(20);
    let filter = loop {
        worker.session.pump(2);
        if let Some(result) = task.take_result() {
            break result.map_err(|e| e.to_string())?;
        }
        if Instant::now() > deadline {
            return Err("Filter compilation timed out.".into());
        }
    };
    let installation = worker
        .session
        .install_content_filter(&filter)
        .map_err(|e| e.to_string())?;
    if network(&worker)? != "blocked" {
        return Err("Native adblock allowed its blocked analytics resource.".into());
    }
    drop(installation);
    if network(&worker)? != "allowed" {
        return Err("Turning native filtering off did not restore the positive control.".into());
    }
    let store = Rc::new(store);
    worker.filter_store = Some(Rc::clone(&store));
    let id = enzui_webkit::ffi::content_filters::ContentFilterStore::identifier_for(&filters.json)
        .map_err(|e| e.to_string())?;
    let occupied = [
        store.load(&id, Duration::from_secs(10)),
        store.load(&id, Duration::from_secs(10)),
    ];
    if occupied.iter().any(Result::is_err) {
        return Err("Filter admission positive control could not occupy both slots.".into());
    }
    let mut replies = Vec::new();
    for _ in 0..3 {
        worker
            .filter_generation
            .set(worker.filter_generation.get().wrapping_add(1));
        let (job, receiver) = crate::native_jobs::Job::new();
        worker.start_filter(2, Arc::clone(&filters.json), job);
        replies.push(receiver);
    }
    for mut operation in std::mem::take(&mut worker.native_operations) {
        if !operation(&mut worker) {
            worker.native_operations.push(operation);
        }
    }
    let mut final_reply = replies.pop().ok_or("No final protection reply.")?;
    if !matches!(
        final_reply.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Empty)
    ) {
        return Err("Busy native filter admission failed instead of waiting.".into());
    }
    for task in occupied.into_iter().flatten() {
        task.cancel();
    }
    let deadline = Instant::now() + Duration::from_secs(18);
    loop {
        worker.session.pump(2);
        for mut operation in std::mem::take(&mut worker.native_operations) {
            if !operation(&mut worker) {
                worker.native_operations.push(operation);
            }
        }
        if let Ok(result) = final_reply.try_recv() {
            result?;
            break;
        }
        if Instant::now() >= deadline {
            return Err(
                "Superseding protection requests did not recover from native admission pressure."
                    .into(),
            );
        }
    }
    if network(&worker)? != "blocked" {
        return Err("Recovered protection request failed to install its native filter.".into());
    }
    worker.filter_installation.take();
    let mut local_fixture = false;
    if let Ok(origin) = std::env::var("ENZIM_TEST_ORIGIN") {
        worker
            .session
            .local_origins()
            .ok_or("No local-origin policy table.")?
            .allow_origin(&origin)
            .map_err(|e| e.to_string())?;
        worker
            .session
            .load(&format!("{origin}/request-lifetime"), 15000)
            .map_err(|error| error.to_string())?;
        if !evaluate(&worker, "document.body.textContent")?
            .as_str()
            .is_some_and(|body| body.contains("Upstream stays open"))
        {
            return Err(
                "Destination guard closed the upstream write side before the HTTP response.".into(),
            );
        }
        evaluate(
            &worker,
            "window.__enzimPostLifetime='pending';fetch('/request-lifetime',{method:'POST',body:'enzim request body'}).then(r=>r.text()).then(text=>window.__enzimPostLifetime=text.includes('Upstream stays open'),()=>window.__enzimPostLifetime=false);'started'",
        )?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            worker.session.pump(2);
            let result = evaluate(&worker, "window.__enzimPostLifetime")?;
            if result == true {
                break;
            }
            if result == false || Instant::now() >= deadline {
                return Err("Destination guard ended the framed POST before its response.".into());
            }
        }
        let privacy = crate::privacy::Privacy {
            user_agent: "enzimBrowser-Qualification/1.0".into(),
            ..Default::default()
        };
        privacy.apply(&worker.session);
        worker.privacy.clone_from(&privacy);
        worker.configure_developer_tools(true);
        worker
            .session
            .load(&format!("{origin}/ua"), 15000)
            .map_err(|e| e.to_string())?;
        let body = evaluate(&worker, "document.body.textContent")?;
        let response: serde_json::Value =
            serde_json::from_str(body.as_str().ok_or("No UA echo body.")?)
                .map_err(|e| e.to_string())?;
        if response["user_agent"] != "enzimBrowser-Qualification/1.0"
            || evaluate(&worker, "navigator.userAgent")? != "enzimBrowser-Qualification/1.0"
        {
            return Err(
                "HTTP and JavaScript user agents do not match the selected override.".into(),
            );
        }
        worker
            .session
            .load(&format!("{origin}/cookies"), 15000)
            .map_err(|e| e.to_string())?;
        let task = worker
            .session
            .cookie_inventory()
            .map_err(|e| e.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(10);
        let records = loop {
            worker.session.pump(2);
            if let Some(result) = task.take_result() {
                break result.map_err(|e| e.to_string())?;
            }
            if Instant::now() > deadline {
                return Err("HttpOnly inventory timed out.".into());
            }
        };
        if !records
            .iter()
            .any(|cookie| cookie.key.name == "http_only_fixture" && cookie.http_only)
        {
            return Err("Native inventory missed a genuine HttpOnly cookie.".into());
        }
        if evaluate(&worker, "document.cookie")?
            .as_str()
            .is_some_and(|cookie| cookie.contains("http_only_fixture"))
        {
            return Err("HttpOnly cookie was exposed to page JavaScript.".into());
        }
        let html = page_export(
            &worker,
            enzui_webkit::page_export::PageExportFormat::MainResourceHtml,
        )?;
        if !String::from_utf8_lossy(&html).contains("Cookie fixture") {
            return Err("Native original HTML export omitted the loaded source.".into());
        }
        let mut events = Vec::new();
        worker.collect_resource_events(&mut events);
        if !events.iter().any(|event|matches!(event,PageEvent::ScriptMessage {handler,json} if handler=="enzimNativeResources" && json.contains("\"status\":200") && json.contains("/ua"))) { return Err("Native resource bridge omitted real request/response metadata.".into()); }
        local_fixture = true;
    }
    let common = WebProfile::persistent("document-cookie-qualification");
    let mut r = ViewRouter::new();
    let v = r.create(992);
    let mut first = Worker::open(
        r,
        v,
        scratch.clone(),
        Some(size),
        MediaGeneration::FIRST,
        common.clone(),
    )?;
    crate::privacy::Privacy::default().apply(&first.session);
    first
        .session
        .load("https://example.org/", 15_000)
        .map_err(|e| e.to_string())?;
    let cookie = evaluate(
        &first,
        "document.cookie='enzimDocumentSession=1; Secure; Path=/';document.cookie='enzimDocumentPersistent=1; Secure; Max-Age=3600; Path=/';document.cookie",
    )?;
    if !cookie.as_str().is_some_and(|s| {
        s.contains("enzimDocumentSession=1") && s.contains("enzimDocumentPersistent=1")
    }) {
        return Err(format!(
            "Document cookie write positive control failed: {cookie}"
        ));
    }
    let inventory = Rc::new(RefCell::new(None));
    let completion = Rc::clone(&inventory);
    first
        .session
        .cookies(move |r| *completion.borrow_mut() = Some(r));
    let records = wait(&first, &inventory)?;
    if !records.iter().any(|c| c.key.name == "enzimDocumentSession") {
        return Err(format!(
            "Native inventory cannot see the document-written session cookie; native names: {:?}",
            records.iter().map(|c| &c.key.name).collect::<Vec<_>>()
        ));
    }
    let mut r = ViewRouter::new();
    let v = r.create(993);
    let mut second = Worker::open(
        r,
        v,
        scratch.clone(),
        Some(size),
        MediaGeneration::FIRST,
        common.clone(),
    )?;
    crate::privacy::Privacy::default().apply(&second.session);
    second
        .session
        .load("https://example.org/", 15_000)
        .map_err(|e| e.to_string())?;
    let cookie = evaluate(&second, "document.cookie")?;
    if !cookie.as_str().is_some_and(|s| {
        s.contains("enzimDocumentSession=1") && s.contains("enzimDocumentPersistent=1")
    }) {
        return Err(format!(
            "Second live view cannot see document-written cookies: {cookie}"
        ));
    }
    drop(first);
    drop(second);
    let mut r = ViewRouter::new();
    let v = r.create(994);
    let mut third = Worker::open(r, v, scratch, Some(size), MediaGeneration::FIRST, common)?;
    crate::privacy::Privacy::default().apply(&third.session);
    third
        .session
        .load("https://example.org/", 15_000)
        .map_err(|e| e.to_string())?;
    let cookie = evaluate(&third, "document.cookie")?;
    if !cookie
        .as_str()
        .is_some_and(|s| s.contains("enzimDocumentSession=1"))
    {
        return Err(format!(
            "Session cookies disappeared while the browser stayed open without a website tab: {cookie}"
        ));
    }
    eprintln!("FEATURE_ARTIFACTS {}", out.display());
    Ok(
        serde_json::json!({"http_request_lifetime":true,"console_unavailable_is_not_throw":true,"full_document_scroll_area":true,"full_document_bottom_pixels":true,"native_mhtml":true,"native_pdf":true,"native_cookie_inventory":true,"document_cookie_cross_view":true,"cookie_survives_no_website_tabs":true,"selective_cookie_delete":true,"native_adblock_off_on_off":true,"private_bootstrap_metadata_hidden":true,"native_resource_bridge":true,"upstream_original_html":true,"console_page_logs":true,"rapid_filter_admission_recovery":true,"console_primitives_and_once":true,"live_css_rendered_pixels":true,"native_find":true,"local_http_useragent_and_httponly":local_fixture,"converted_filters":filters.converted,"native_rules":filters.rules}),
    )
}
