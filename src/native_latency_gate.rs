#[cfg(feature = "benchmark")]
pub(super) fn run_latency_gate(scratch: PathBuf) -> Result<serde_json::Value, String> {
    use enzui_webkit::page::{PageEvent, ScriptWorld};
    fn evaluate(
        worker: &mut Worker,
        source: &str,
        world: &ScriptWorld,
    ) -> Result<serde_json::Value, String> {
        let id = worker
            .session
            .evaluate_script(source, world)
            .map_err(|e| e.to_string())?;
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            worker.session.pump(2);
            for event in worker.session.take_page_events() {
                match event {
                    PageEvent::ScriptMessage { handler, json } if handler == "enzimCosmetics" => {
                        worker.cosmetic_message(&json);
                    }
                    PageEvent::ScriptResult { id: found, result } if found == id => {
                        return result.map_err(|e| format!("{e:?}")).and_then(|json| {
                            serde_json::from_str(&json).map_err(|e| e.to_string())
                        });
                    }
                    _ => {}
                }
            }
            if std::time::Instant::now() > deadline {
                return Err("Latency evaluation timed out".into());
            }
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
    worker.filter_mode.set(Some(1));
    Worker::install_scripts(&mut worker.session, true, Some("https://edition.cnn.com/"), true);
    let markup = format!(
        "<div class=ad-slot-header__wrapper style='height:300px'>Ad</div><main id=ordinary style='height:120px'>Article</main><div id=late></div>{}",
        "<section class=known><span class=known>Content</span></section>".repeat(2500)
    );
    worker
        .session
        .load_html(&markup, "https://edition.cnn.com/", 10000)
        .map_err(|e| e.to_string())?;
    for _ in 0..30 {
        worker.session.pump(10);
        worker.update(None, false);
    }
    let chrome = ScriptWorld::Isolated("enzim-browser-chrome".into());
    evaluate(
        &mut worker,
        "globalThis.__enzimCosmeticMetrics={flushes:0,visited:0,surveys:0,ms:0};true",
        &chrome,
    )?;
    evaluate(
        &mut worker,
        "window.ticks=0;window.mutationTimer=setInterval(()=>{document.body.classList.toggle('known');if(++ticks===120)clearInterval(mutationTimer)},16);true",
        &ScriptWorld::Page,
    )?;
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        worker.session.pump(10);
        worker.update(None, false);
        if evaluate(&mut worker, "ticks", &ScriptWorld::Page)? == 120 {
            break;
        }
        if std::time::Instant::now() > deadline {
            return Err("Mutation fixture stalled".into());
        }
    }
    for _ in 0..20 {
        worker.session.pump(10);
        worker.update(None, false);
    }
    let metrics = evaluate(
        &mut worker,
        "JSON.stringify(__enzimCosmeticMetrics)",
        &chrome,
    )?;
    let metrics: serde_json::Value =
        serde_json::from_str(metrics.as_str().ok_or("Missing metrics")?)
            .map_err(|e| e.to_string())?;
    if metrics["visited"]
        .as_u64()
        .is_none_or(|visited| visited > 100)
        || metrics["surveys"] != 0
    {
        return Err(format!(
            "Unchanged descendant classes triggered repeated work: {metrics}"
        ));
    }
    evaluate(
        &mut worker,
        "document.querySelector('#late').className='ADBox';document.querySelector('#late').style.height='200px';true",
        &ScriptWorld::Page,
    )?;
    for _ in 0..30 {
        worker.session.pump(10);
        worker.update(None, false);
    }
    let heights = evaluate(
        &mut worker,
        "[document.querySelector('.ad-slot-header__wrapper').offsetHeight,document.querySelector('#ordinary').offsetHeight,document.querySelector('#late').offsetHeight]",
        &ScriptWorld::Page,
    )?;
    eprintln!("LATENCY_METRICS {metrics}");
    if heights != serde_json::json!([0, 120, 0]) {
        return Err(format!(
            "Cosmetics changed article/late-ad behavior: {heights}"
        ));
    }
    let lookup_started = std::time::Instant::now();
    for _ in 0..1000 {
        std::hint::black_box(crate::filters::cosmetic_rules(
            "https://edition.cnn.com/",
            &["ADBox".to_owned()],
            &[],
        ));
    }
    let lookup_us = lookup_started.elapsed().as_micros();
    Ok(
        serde_json::json!({"mutation_metrics":metrics,"warm_lookup_total_us":lookup_us,"site_and_late_generic_rules_preserved":true}),
    )
}
