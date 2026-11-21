#[cfg(feature = "benchmark")]
pub(super) fn run_devtools_gate(scratch: PathBuf) -> Result<serde_json::Value, String> {
    use enzui_webkit::page::{PageEvent, ScriptWorld};
    fn evaluate(
        worker: &mut Worker,
        source: &str,
        messages: &mut Vec<String>,
    ) -> Result<serde_json::Value, String> {
        let id = worker
            .session
            .evaluate_script(source, &ScriptWorld::Page)
            .map_err(|e| e.to_string())?;
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        loop {
            worker.session.pump(2);
            let mut result = None;
            for event in worker.session.take_page_events() {
                match event {
                    PageEvent::ScriptMessage { handler, json } if handler == "enzimConsole" => {
                        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&json) {
                            messages.push(value["text"].as_str().unwrap_or_default().into());
                        }
                    }
                    PageEvent::ScriptResult {
                        id: found,
                        result: value,
                    } if found == id => result = Some(value),
                    _ => {}
                }
            }
            if let Some(result) = result {
                return result
                    .map_err(|e| format!("{e:?}"))
                    .and_then(|value| serde_json::from_str(&value).map_err(|e| e.to_string()));
            }
            if std::time::Instant::now() > deadline {
                return Err("Developer tools lifecycle evaluation stalled".into());
            }
        }
    }
    fn require(value: serde_json::Value, label: &str) -> Result<(), String> {
        if value == true {
            Ok(())
        } else {
            Err(format!("{label}: {value}"))
        }
    }
    let mut router = ViewRouter::new();
    let view = router.create(994);
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
    let mut messages = Vec::new();
    worker
        .session
        .load_html(
            "<title>Developer tools lifecycle</title><body>Page</body>",
            "https://example.org/",
            10000,
        )
        .map_err(|e| e.to_string())?;
    require(
        evaluate(
            &mut worker,
            "globalThis.initialLog=console.log;globalThis.initialAssert=console.assert;console.log('closed-initial');!globalThis[Symbol.for('enzim.devtools.console')]&&!window.webkit?.messageHandlers?.enzimConsole",
            &mut messages,
        )?,
        "Closed document installs no console capture",
    )?;
    if worker.session.unregister_message_handler(
        "enzimInspect",
        &ScriptWorld::Isolated("enzim-developer-tools".into()),
    ) {
        return Err("Closed developer tools registered an inspector channel".into());
    }
    worker.configure_developer_tools(true);
    require(
        isolated(
            &worker,
            "Boolean(window.webkit?.messageHandlers?.enzimInspect)",
        )?,
        "Opening registers the isolated inspector channel",
    )?;
    require(
        evaluate(
            &mut worker,
            "globalThis.firstWrapped=console.log;console.log('opened-once');console.log!==initialLog&&!!globalThis[Symbol.for('enzim.devtools.console')]&&Boolean(window.webkit?.messageHandlers?.enzimConsole)",
            &mut messages,
        )?,
        "Opening captures the current document",
    )?;
    require(
        evaluate(
            &mut worker,
            "globalThis.previewEffects=0;globalThis.previewFunction=()=>17;previewFunction.toString=()=>{++previewEffects;return 'custom coercion'};console.log('bounded-preview',previewFunction,{get value(){++previewEffects;return 9}});previewEffects===0",
            &mut messages,
        )?,
        "Console previews do not invoke function coercion or property getters",
    )?;
    worker.configure_developer_tools(true);
    require(
        evaluate(
            &mut worker,
            "console.log('opened-twice');console.log===firstWrapped",
            &mut messages,
        )?,
        "Repeated open does not wrap twice",
    )?;
    worker.configure_developer_tools(false);
    require(
        evaluate(
            &mut worker,
            "console.log('closed-again');console.log===initialLog&&console.assert===initialAssert&&!globalThis[Symbol.for('enzim.devtools.console')]&&!window.webkit?.messageHandlers?.enzimConsole",
            &mut messages,
        )?,
        "Closing restores original functions",
    )?;
    worker.configure_developer_tools(true);
    evaluate(
        &mut worker,
        "globalThis.pageOwnedLog=()=>17;console.log=pageOwnedLog;true",
        &mut messages,
    )?;
    worker.configure_developer_tools(false);
    require(
        evaluate(
            &mut worker,
            "console.log===pageOwnedLog&&!globalThis[Symbol.for('enzim.devtools.console')]&&!window.webkit?.messageHandlers?.enzimConsole",
            &mut messages,
        )?,
        "Closing preserves a site's subsequent console replacement",
    )?;
    worker
        .session
        .load_html(
            "<title>Closed tools reload</title>",
            "https://example.org/next",
            10000,
        )
        .map_err(|e| e.to_string())?;
    require(
        evaluate(
            &mut worker,
            "console.log('closed-reloaded');console.log.toString().includes('[native code]')&&!globalThis[Symbol.for('enzim.devtools.console')]&&!window.webkit?.messageHandlers?.enzimConsole",
            &mut messages,
        )?,
        "Closed reload installs no console capture",
    )?;
    worker.configure_developer_tools(true);
    worker
        .session
        .load_html(
            "<title>Open tools reload</title>",
            "https://example.org/final",
            10000,
        )
        .map_err(|e| e.to_string())?;
    require(
        evaluate(
            &mut worker,
            "console.log('opened-reloaded');!!globalThis[Symbol.for('enzim.devtools.console')]&&Boolean(window.webkit?.messageHandlers?.enzimConsole)",
            &mut messages,
        )?,
        "Open reload captures at document start",
    )?;
    worker.configure_developer_tools(false);
    evaluate(&mut worker, "true", &mut messages)?;
    if messages
        != [
            "opened-once",
            "bounded-preview [Function] {value: [Getter]}",
            "opened-twice",
            "opened-reloaded",
        ]
    {
        return Err(format!("Unexpected console capture scope: {messages:?}"));
    }
    fn isolated(worker: &Worker, source: &str) -> Result<serde_json::Value, String> {
        let id = worker
            .session
            .evaluate_script(
                source,
                &ScriptWorld::Isolated("enzim-developer-tools".into()),
            )
            .map_err(|e| e.to_string())?;
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        loop {
            worker.session.pump(2);
            for event in worker.session.take_page_events() {
                if let PageEvent::ScriptResult { id: found, result } = event
                    && found == id
                {
                    return result
                        .map_err(|e| format!("{e:?}"))
                        .and_then(|value| serde_json::from_str(&value).map_err(|e| e.to_string()));
                }
            }
            if std::time::Instant::now() > deadline {
                return Err("DOM inspector qualification stalled.".into());
            }
        }
    }
    worker.configure_developer_tools(true);
    evaluate(
        &mut worker,
        "document.body.innerHTML='<main id=sample><button id=child>Button</button></main>';true",
        &mut messages,
    )?;
    let inspected = isolated(
        &worker,
        &format!("{};__enzimInspect('#sample')", include_str!("inspector.js")),
    )?;
    if inspected["childNodes"][0]["tag"] != "button"
        || inspected["parents"]
            .as_array()
            .is_none_or(|parents| parents.len() != 2)
    {
        return Err(format!("DOM navigation snapshot: {inspected}"));
    }
    let child = inspected["childNodes"][0]["node"]
        .as_u64()
        .ok_or("Child identity missing")?;
    require(
        isolated(
            &worker,
            &format!("__enzimInspectNode({child}).id==='child'"),
        )?,
        "Navigate to child identity",
    )?;
    require(
        isolated(
            &worker,
            "__enzimSelected.style.setProperty('color','rgb(3, 5, 7)');__enzimSnapshot(__enzimSelected).styles.color==='rgb(3, 5, 7)'",
        )?,
        "Edit inline CSS",
    )?;
    isolated(&worker, "__enzimBeginPick()")?;
    worker.configure_developer_tools(false);
    require(
        isolated(
            &worker,
            "!globalThis.__enzimSelected&&!globalThis[Symbol.for('enzim.devtools.dom')].pick&&!globalThis[Symbol.for('enzim.devtools.dom')].move&&globalThis[Symbol.for('enzim.devtools.dom')].nodes.size===0",
        )?,
        "Closing releases inspector nodes and listeners",
    )?;
    fn await_reply(
        worker: &Worker,
        body: &str,
    ) -> Result<Result<serde_json::Value, enzui_webkit::page::ScriptError>, String> {
        let id = worker
            .session
            .evaluate_async_function_preview(body, &ScriptWorld::Page)
            .map_err(|e| e.to_string())?;
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        loop {
            worker.session.pump(2);
            for event in worker.session.take_page_events() {
                if let PageEvent::ScriptResult { id: found, result } = event
                    && found == id
                {
                    return Ok(result.and_then(|json| {
                        serde_json::from_str(&json)
                            .map_err(|e| enzui_webkit::page::ScriptError::Failed(e.to_string()))
                    }));
                }
            }
            if std::time::Instant::now() > deadline {
                return Err("Awaited console qualification stalled.".into());
            }
        }
    }
    let preview = await_reply(
        &worker,
        "globalThis.awaitedRuns=(globalThis.awaitedRuns||0)+1;return await Promise.resolve(43)",
    )?
    .map_err(|e| format!("{e:?}"))?;
    if preview["kind"] != "json" || preview["preview"] != "43" {
        return Err(format!("Awaited console result: {preview}"));
    }
    require(
        evaluate(&mut worker, "awaitedRuns===1", &mut messages)?,
        "Awaited expression runs once",
    )?;
    if !matches!(await_reply(&worker,"return await Promise.reject(new Error('native-await-rejected'))")?,Err(enzui_webkit::page::ScriptError::Failed(message)) if message.contains("native-await-rejected"))
    {
        return Err("Awaited rejection did not surface the error.".into());
    }
    worker
        .privacy
        .site_settings
        .entry("https://example.org".into())
        .or_default()
        .insert("javascript".into(), false);
    for enabled in [true, false] {
        worker.configure_developer_tools(enabled);
        worker.session.load_html("<title>Site JavaScript stays disabled</title><body><script>document.title='Site JavaScript ran'</script></body>","https://example.org/site-choice",10000).map_err(|e|e.to_string())?;
        if worker
            .session
            .page_title()
            .map_err(|e| e.to_string())?
            .as_deref()
            != Some("Site JavaScript stays disabled")
        {
            return Err("Developer tools changed the site's disabled JavaScript choice".into());
        }
    }
    Ok(
        serde_json::json!({"captured":messages,"original_functions_restored":true,"page_replacement_preserved":true,"closed_reload_uninstrumented":true,"dom_navigation":true,"inline_css":true,"picker_cleanup":true,"await_single_execution":true,"await_rejection":true,"site_privacy_preserved":true}),
    )
}
