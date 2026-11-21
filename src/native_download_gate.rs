#[cfg(feature = "benchmark")]
pub(super) fn run_download_gate(scratch: PathBuf) -> Result<serde_json::Value, String> {
    use enzui_webkit::page::{PageEvent, ScriptWorld};
    let mut router = ViewRouter::new();
    let view = router.create(991);
    let mut worker = Worker::open(
        router,
        view,
        scratch.clone(),
        Some(enzui_webkit::ViewportSize::from_logical(800., 600.).map_err(|e| e.to_string())?),
        MediaGeneration::FIRST,
        WebProfile::ephemeral(),
    ).map_err(|error| format!("download worker bootstrap: {error}"))?;
    worker.session.disable_gpu_presentation();
    worker.gpu_enabled = false;
    let destination = scratch.join("downloads");
    std::fs::create_dir_all(&destination).map_err(|e| e.to_string())?;
    worker
        .session
        .set_download_directory(Some(destination))
        .map_err(|e| e.to_string())?;
    let pdf = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/two-pages.pdf"),
    )
    .map_err(|e| e.to_string())?;
    let bytes = serde_json::to_string(&pdf).map_err(|e| e.to_string())?;
    let mut blobs = Vec::new();
    for mib in [10usize,50,100] {
        let total=mib*1024*1024;
        let html = format!("<button onclick=go()>Download PDF</button><script>function go(){{setTimeout(()=>{{const bytes=new Uint8Array({total});bytes.fill(32);bytes.set({bytes});const a=document.createElement('a');a.href=URL.createObjectURL(new Blob([bytes],{{type:'application/pdf'}}));a.download='generated-{mib}MB.pdf';document.body.append(a);a.click();}},1200);}}</script>");
        worker.session.load_html(&html,"https://qwikpdf.com/",10000).map_err(|e|e.to_string())?;
        worker.session.evaluate_script("go();undefined",&ScriptWorld::Page).map_err(|e|e.to_string())?;
        let deadline=std::time::Instant::now()+Duration::from_secs(30);
        let mut started=false;
        let mut completed=false;
        while std::time::Instant::now()<deadline && !completed {
            worker.session.pump(2);
            for event in worker.session.take_page_events() {
                match event {
                    PageEvent::DownloadStarted{..}=>started=true,
                    PageEvent::DownloadFinished{path,..}=>{
                        let saved=std::fs::read(&path).map_err(|e|e.to_string())?;
                        if !started || saved.len()!=total || !saved.starts_with(&pdf) || !saved[pdf.len()..].iter().all(|b|*b==32) {return Err(format!("Delayed {mib} MiB Blob content/start mismatch"));}
                        blobs.push(serde_json::json!({"mib":mib,"delayed_ms":1200,"exact_bytes":true}));
                        completed=true;
                        eprintln!("BLOB_CHECK {mib} MiB passed");
                    }
                    PageEvent::DownloadFailed{message,..}=>return Err(message),
                    PageEvent::DownloadDeclined{..}=>return Err(format!("Delayed {mib} MiB Blob declined")),
                    _=>{}
                }
            }
        }
        if !completed{return Err(format!("Delayed {mib} MiB Blob timed out"));}
    }
    let ranges=qualify_range_downloads(&mut worker)?;
    Ok(serde_json::json!({"blobs":blobs,"ranges":ranges}))
}

#[cfg(feature = "benchmark")]
fn qualify_range_downloads(worker: &mut Worker) -> Result<serde_json::Value, String> {
    use enzui_core::download::{Phase, Validator};
    use enzui_webkit::page::{PageEvent, ScriptWorld};
    let base = std::env::var("ENZIM_DOWNLOAD_BASE").map_err(|e| e.to_string())?;
    worker
        .session
        .begin_load_uri(&format!("{base}/fixture"))
        .map_err(|e| format!("download fixture navigation: {e}"))?;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while worker.session.page_title().ok().flatten().as_deref() != Some("Download fixture") {
        worker.session.pump(5);
        if std::time::Instant::now() > deadline {
            return Err("Authenticated fixture did not load".into());
        }
    }
    worker
        .session
        .evaluate_script(
            "window.downloadSentinel='document-preserved'",
            &ScriptWorld::Page,
        )
        .map_err(|e| format!("download fixture sentinel: {e}"))?;
    worker.session.pump(20);
    worker.session.take_page_events();
    let owner = worker.view;
    let mut results = serde_json::Map::new();
    for mode in [
        "normal",
        "ignore",
        "changed",
        "wrong",
        "malformed",
        "liechanged",
        "short",
        "redirect",
        "interrupt",
        "chunked",
        "gzip",
    ] {
        let mut store = crate::downloads::Store::default();
        worker
            .session
            .start_download(&format!("{base}/download?mode={mode}"))
            .map_err(|error| format!("HTTP download {mode}: {error}"))?;
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        let mut paused = false;
        let mut resumed = false;
        let mut captured = false;
        let mut assembling = false;
        let outcome = loop {
            if std::time::Instant::now() > deadline {
                return Err(format!(
                    "Range {mode} timed out; {} entries",
                    store.entries.len()
                ));
            }
            worker.session.pump(5);
            for event in worker.session.take_page_events() {
                if crate::downloads::is_event(&event) {
                    if matches!(
                        event,
                        PageEvent::DownloadPaused { .. }
                            | PageEvent::DownloadInterrupted { .. }
                            | PageEvent::DownloadFailed { .. }
                            | PageEvent::DownloadFinished { .. }
                            | PageEvent::DownloadResponse { .. }
                    ) {
                        eprintln!("RANGE_EVENT {mode} {event:?}");
                    }
                    store.accept(owner, &event);
                }
            }
            for (_, id) in std::mem::take(&mut store.cancel_native) {
                worker.session.cancel_download(id);
            }
            let Some(e) = store.entries.last_mut() else {
                continue;
            };
            if !paused
                && !matches!(mode, "chunked" | "gzip")
                && mode != "interrupt"
                && e.model.received > 65536
                && e.model.phase == Phase::Active
            {
                paused = worker
                    .session
                    .pause_download(e.native.ok_or("No native id")?);
            }
            if !resumed
                && e.partial.is_some()
                && matches!(e.model.phase, Phase::Paused | Phase::Failed)
            {
                let partial = e.partial.as_ref().ok_or("No partial")?;
                let validator = partial
                    .response
                    .etag
                    .as_deref()
                    .and_then(Validator::strong_etag)
                    .or_else(|| {
                        partial
                            .response
                            .modified
                            .as_deref()
                            .and_then(Validator::last_modified)
                    })
                    .and_then(|v| v.if_range_value().map(str::to_owned))
                    .ok_or("No safe validator")?;
                let offset = partial.received;
                let uri = partial.response.url.as_str().to_owned();
                if e.model.phase == Phase::Paused {
                    e.model.resume();
                } else {
                    e.model.retry();
                    e.model.progress(e.model.attempt, offset);
                }
                e.url.clone_from(&uri);
                e.retrying = true;
                e.resume_requested = true;
                e.range_decision = None;
                e.error.clear();
                worker
                    .session
                    .start_range_download(&uri, offset, &validator)
                    .map_err(|error| format!("HTTP resume {mode}: {error}"))?;
                captured = true;
                resumed = true;
            }
            if let Some((partial, path)) = e.assembly.take() {
                let path = crate::downloads::assemble_download(&partial, path.as_deref())?;
                e.path = Some(path);
                e.model.finish(e.model.attempt, Phase::Complete);
                assembling = true;
            }
            if e.model.phase == Phase::Complete {
                let path = e.path.as_ref().ok_or("No complete path")?;
                let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
                let mut expected = if mode == "changed" {
                    b"changed-representation\n".to_vec()
                } else {
                    b"enzim-download-fixture\n".to_vec()
                };
                let octets: Vec<u8> = if mode == "changed" {
                    (0..=255).rev().collect()
                } else {
                    (0..=255).collect()
                };
                for _ in 0..8192 {
                    expected.extend_from_slice(&octets);
                }
                if bytes != expected {
                    return Err(format!(
                        "{mode}: completed file bytes mismatch: {} versus {}",
                        bytes.len(),
                        expected.len()
                    ));
                }
                if matches!(mode, "wrong" | "malformed" | "liechanged" | "short") {
                    return Err(format!("{mode}: corrupt range was accepted"));
                }
                if !matches!(mode, "chunked" | "gzip") && !resumed {
                    return Err(format!("{mode}: continuation was never exercised"));
                }
                break serde_json::json!({"exact_bytes":true,"resumed":resumed,"assembled":assembling});
            }
            if matches!(e.model.phase, Phase::Failed | Phase::Cancelled)
                && (resumed || !captured && e.partial.is_none())
            {
                if !matches!(mode, "wrong" | "malformed" | "liechanged" | "short") {
                    return Err(format!("{mode}: {}", e.error));
                }
                break serde_json::json!({"rejected":true,"reason":e.error});
            }
        };
        let query = worker
            .session
            .evaluate_script("window.downloadSentinel", &ScriptWorld::Page)
            .map_err(|e| e.to_string())?;
        let deadline = std::time::Instant::now() + Duration::from_secs(4);
        let mut intact = false;
        while std::time::Instant::now() < deadline && !intact {
            worker.session.pump(5);
            for event in worker.session.take_page_events() {
                if let PageEvent::ScriptResult { id, result } = event
                    && id == query
                {
                    intact = result
                        .as_ref()
                        .is_ok_and(|s| s.contains("document-preserved"));
                }
            }
        }
        if !intact
            || worker.session.current_uri().ok().flatten().as_deref()
                != Some(format!("{base}/fixture").as_str())
        {
            return Err(format!("{mode}: document changed during download"));
        }
        eprintln!("RANGE_QUALIFIED {mode} {outcome}");
        results.insert(mode.into(), outcome);
    }
    Ok(serde_json::Value::Object(results))
}
