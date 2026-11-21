use enzui_webkit::ffi::content_filters::ContentFilterStore;
use enzui_webkit::ffi::runtime::{HeadlessDisplay, WebSession};
use enzui_webkit::page::{PageEvent, ScriptWorld};
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn snapshot(pid: u32) -> Result<serde_json::Value, String> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup"))
        .map_err(|error| error.to_string())?;
    let mut values = serde_json::Map::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if matches!(
            key,
            "Pss" | "Pss_Anon" | "Pss_File" | "Pss_Shmem" | "AnonHugePages" | "LazyFree" | "Swap"
        ) {
            let kib = value
                .split_whitespace()
                .next()
                .ok_or("Missing mapping value")?
                .parse::<u64>()
                .map_err(|error| error.to_string())?;
            values.insert(key.to_owned(), (kib * 1024).into());
        }
    }
    Ok(values.into())
}

fn evaluate(web: &WebSession, script: &str) -> Result<serde_json::Value, String> {
    let id = web
        .evaluate_script(script, &ScriptWorld::Page)
        .map_err(|error| error.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        web.pump(2);
        for event in web.take_page_events() {
            if let PageEvent::ScriptResult { id: found, result } = event
                && found == id
            {
                return serde_json::from_str(&result.map_err(|error| format!("{error:?}"))?)
                    .map_err(|error| error.to_string());
            }
        }
        if Instant::now() > deadline {
            return Err("Filter memory evaluation timed out".into());
        }
    }
}

pub(crate) fn run(root: PathBuf) -> Result<serde_json::Value, String> {
    let address = std::env::var("ENZIM_BENCH_FILTER_FIXTURE").map_err(|error| error.to_string())?;
    let url = url::Url::parse(&address).map_err(|error| error.to_string())?;
    if url.scheme() != "http"
        || url.host_str() != Some("filter-memory.localhost")
        || url.port().is_none()
        || url.path() != "/filter-memory.html"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("An owned loopback filter fixture is required".into());
    }
    let display = HeadlessDisplay::new().map_err(|error| error.to_string())?;
    let mut web = WebSession::new_trusted_local(&display).map_err(|error| error.to_string())?;
    web.load(&address, 10_000)
        .map_err(|error| error.to_string())?;
    if evaluate(
        &web,
        "document.title === 'Filter allocation fixture' && document.body.textContent === 'AdVisible content'",
    )? != true
    {
        return Err("The owned filter document is incorrect".into());
    }
    let pid = web
        .renderer_process_id()
        .map_err(|error| format!("{error:?}"))?
        .ok_or("Missing fixture renderer")?;
    if evaluate(
        &web,
        "getComputedStyle(document.querySelector('.ad_banner')).display === 'none'",
    )? != false
    {
        return Err("Unfiltered control is unexpectedly hidden".into());
    }
    let baseline = snapshot(pid)?;
    let store = ContentFilterStore::new(&root).map_err(|error| error.to_string())?;
    let task = store.compile(r#"[{"trigger":{"url-filter":".*"},"action":{"type":"css-display-none","selector":".ad_banner"}}]"#, Duration::from_secs(10)).map_err(|error| error.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(12);
    let control = loop {
        web.pump(2);
        if let Some(result) = task.take_result() {
            break result.map_err(|error| error.to_string())?;
        }
        if Instant::now() > deadline {
            return Err("Control CSS compilation timed out".into());
        }
    };
    let control_installation = web
        .install_content_filter(&control)
        .map_err(|error| error.to_string())?;
    web.load(&address, 10_000)
        .map_err(|error| error.to_string())?;
    if evaluate(
        &web,
        "getComputedStyle(document.querySelector('.ad_banner')).display === 'none'",
    )? != true
    {
        return Err("Single-selector native CSS positive control failed".into());
    }
    drop(control_installation);
    let (_, bundles) = crate::filters::for_privacy(&crate::privacy::Privacy::default())?;
    let mut compiled = Vec::new();
    for rules in bundles {
        let task = store
            .compile(&rules, Duration::from_secs(30))
            .map_err(|error| error.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(32);
        loop {
            web.pump(2);
            if let Some(result) = task.take_result() {
                compiled.push(result.map_err(|error| error.to_string())?);
                break;
            }
            if Instant::now() > deadline {
                return Err("Filter compilation timed out".into());
            }
        }
    }
    let refs = compiled.iter().collect::<Vec<_>>();
    let _installation = web
        .install_content_filters(&refs)
        .map_err(|error| error.to_string())?;
    web.load(&address, 10_000)
        .map_err(|error| error.to_string())?;
    let hidden = evaluate(
        &web,
        "getComputedStyle(document.querySelector('.ad_banner')).display === 'none'",
    )?;
    let expected =
        !std::env::var_os("ENZIM_BENCH_NATIVE_COSMETICS").is_some_and(|value| value == "0");
    if hidden != expected {
        return Err(format!(
            "Native cosmetic control: expected {expected}, got {hidden}"
        ));
    }
    let deadline = Instant::now() + Duration::from_secs(12);
    while Instant::now() < deadline {
        web.pump(10);
    }
    if web
        .renderer_process_id()
        .map_err(|error| format!("{error:?}"))?
        != Some(pid)
    {
        return Err("Fixture process changed; allocation delta is not comparable".into());
    }
    let loaded = snapshot(pid)?;
    web.release_memory().map_err(|error| error.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        web.pump(10);
    }
    Ok(
        serde_json::json!({"renderer_pid":pid,"baseline":baseline,"loaded":loaded,
        "released":snapshot(pid)?,"native_cosmetics":expected,"sentinel_hidden":hidden}),
    )
}
