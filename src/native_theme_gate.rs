#[cfg(feature = "benchmark")]
pub(super) fn run_theme_gate(scratch: PathBuf) -> Result<serde_json::Value, String> {
    let mut router = ViewRouter::new();
    let view = router.create(992);
    let mut worker = Worker::open(
        router,
        view,
        scratch,
        Some(enzui_webkit::ViewportSize::from_logical(800., 600.).map_err(|e| e.to_string())?),
        MediaGeneration::FIRST,
        WebProfile::ephemeral(),
    )?;
    worker.session.disable_gpu_presentation();
    worker.gpu_enabled = false;
    worker
        .session
        .set_preferred_dark(false)
        .map_err(|e| e.to_string())?;
    worker.session.load_html("<style>:root{color-scheme:light dark}body{background:white}@media(prefers-color-scheme:dark){body{background:rgb(12,12,12)}}</style><script>let q=matchMedia('(prefers-color-scheme:dark)');function report(){document.title=(q.matches?'dark':'light')+':'+getComputedStyle(document.body).backgroundColor}q.addEventListener('change',report);addEventListener('load',report)</script><body>Theme qualification</body>", "https://example.org/",10000).map_err(|e|e.to_string())?;
    let mut samples = Vec::new();
    for (dark, expected) in [
        (false, "light:rgb(255, 255, 255)"),
        (true, "dark:rgb(12, 12, 12)"),
        (false, "light:rgb(255, 255, 255)"),
    ] {
        worker
            .session
            .set_preferred_dark(dark)
            .map_err(|e| e.to_string())?;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            worker.session.pump(5);
            let title = worker
                .session
                .page_title()
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            if title == expected {
                samples.push(title);
                break;
            }
            if std::time::Instant::now() > deadline {
                return Err(format!(
                    "Native color scheme expected {expected}, got {title}"
                ));
            }
        }
    }
    let mut fonts = Vec::new();
    if worker.session.set_system_font_name("").is_ok()
        || worker.session.set_system_font_name("bad\0font 10").is_ok()
    {
        return Err("Invalid desktop font reached native settings".into());
    }
    worker
        .session
        .set_system_font_name("Arial 10")
        .map_err(|e| e.to_string())?;
    worker
        .session
        .load_html(
            "<style>body{font:600 16px/24px system-ui}</style><p>System font qualification</p>",
            "https://example.org/",
            10_000,
        )
        .map_err(|e| e.to_string())?;
    for family in ["Arial"] {
        worker
            .session
            .set_system_font_name(&format!("{family} 10"))
            .map_err(|e| e.to_string())?;
        worker.session.pump(30);
        let source = format!(
            r#"(()=>{{const c=document.createElement('canvas').getContext('2d');const text='The quick brown fox jumps over the lazy dog 0123456789';const width=f=>{{c.font='16px '+f;return c.measureText(text).width}};const style=getComputedStyle(document.body);return {{system:width('system-ui'),expected:width({family:?}),other:width({other:?}),sans:width('sans-serif'),arial:width('Arial'),serif:width('serif'),times:width('Times New Roman'),size:style.fontSize,weight:style.fontWeight}}}})()"#,
            other = if family == "Arial" {
                "Times New Roman"
            } else {
                "Arial"
            }
        );
        let id = worker
            .session
            .evaluate_script(&source, &enzui_webkit::page::ScriptWorld::Page)
            .map_err(|e| e.to_string())?;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let result = loop {
            worker.session.pump(2);
            let mut response = None;
            for event in worker.session.take_page_events() {
                if let enzui_webkit::page::PageEvent::ScriptResult {
                    id: received,
                    result,
                } = event
                    && received == id
                {
                    response = Some(result.map_err(|e| format!("{e:?}"))?);
                }
            }
            if let Some(value) = response {
                break value;
            }
            if std::time::Instant::now() > deadline {
                return Err("System-font evaluation stalled".into());
            }
        };
        let value: serde_json::Value = serde_json::from_str(&result).map_err(|e| e.to_string())?;
        if value["system"] != value["expected"]
            || value["system"] == value["other"]
            || value["size"] != "16px"
            || value["weight"] != "600"
            || value["sans"] != value["arial"]
            || value["serif"] != value["times"]
        {
            return Err(format!("Desktop font {family}: {value}"));
        }
        fonts.push(value);
    }
    Ok(serde_json::json!({"native_media_query_and_css":samples,"startup_system_fonts":fonts}))
}
