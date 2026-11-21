#[cfg(feature = "benchmark")]
pub(super) fn run_privacy_gate(scratch: PathBuf) -> Result<serde_json::Value, String> {
    use enzui_webkit::page::{PageEvent, ScriptWorld};
    fn evaluate(worker: &mut Worker, source: &str) -> Result<serde_json::Value, String> {
        let id = worker
            .session
            .evaluate_script(source, &ScriptWorld::Page)
            .map_err(|e| e.to_string())?;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            worker.session.pump(2);
            let mut response = None;
            for event in worker.session.take_page_events() {
                match event {
                    PageEvent::ScriptMessage { handler, json } if handler == "enzimCosmetics" => {
                        worker.cosmetic_message(&json);
                    }
                    PageEvent::ScriptResult { id: found, result } if found == id => {
                        response = Some(
                            result
                                .map_err(|e| format!("{e:?}"))
                                .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string())),
                        );
                    }
                    _ => {}
                }
            }
            if let Some(response) = response {
                return response;
            }
        }
        Err("Privacy fixture evaluation stalled".into())
    }
    fn install(worker: &mut Worker, privacy: &crate::privacy::Privacy) -> Result<(), String> {
        worker.privacy = privacy.clone();
        let (mode, filters) = crate::filters::for_privacy(privacy)?;
        let (job, mut ready) = crate::native_jobs::Job::new();
        worker.configure_filters(mode, filters, job);
        let deadline = std::time::Instant::now() + Duration::from_secs(35);
        loop {
            worker.session.pump(2);
            worker.update(None, false);
            if let Ok(result) = ready.try_recv() {
                result?;
                return Ok(());
            }
            if std::time::Instant::now() > deadline {
                return Err("Scoped protection installation stalled".into());
            }
        }
    }
    fn images(worker: &mut Worker, address: &str, ad_allowed: bool) -> Result<(), String> {
        worker
            .session
            .load(address, 15000)
            .map_err(|e| e.to_string())?;
        let state = evaluate(
            worker,
            "[document.getElementById('ad').naturalWidth,document.getElementById('tracker').naturalWidth]",
        )?;
        if !state[0].as_u64().is_some_and(|n| (n > 0) == ad_allowed) || state[1] != 0 {
            return Err(format!(
                "Site request policy mismatch at {address}: {state}"
            ));
        }
        Ok(())
    }
    let base = std::env::var("ENZIM_ASSET_BASE").map_err(|_| "ENZIM_ASSET_BASE required")?;
    let other = std::env::var("ENZIM_ASSET_OTHER").map_err(|_| "ENZIM_ASSET_OTHER required")?;
    if crate::privacy::Privacy::origin(&base) == crate::privacy::Privacy::origin(&other) {
        return Err("Privacy fixture requires two exact origins".into());
    }
    let private_cache_root = scratch.clone();
    let mut router = ViewRouter::new();
    let view = router.create(997);
    let mut worker = Worker::open(
        router,
        view,
        scratch,
        None,
        MediaGeneration::FIRST,
        WebProfile::ephemeral(),
    )?;
    if !worker.filter_cache.starts_with(&private_cache_root) {
        return Err("Private site choices escaped the temporary filter cache".into());
    }
    worker.session.disable_gpu_presentation();
    worker.gpu_enabled = false;
    for origin in [&base, &other] {
        worker
            .session
            .local_origins()
            .ok_or("No local origin authority")?
            .allow_origin(origin)
            .map_err(|e| e.to_string())?;
    }
    let mut privacy = crate::privacy::Privacy {
        blocking: 2,
        ..Default::default()
    };
    {
        use enzui_webkit::ffi::content_filters::ContentFilterStore;
        let (_, filters) = crate::filters::for_privacy(&privacy)?;
        let store =
            ContentFilterStore::new(&worker.filter_cache).map_err(|error| error.to_string())?;
        for json in &filters {
            let id = ContentFilterStore::identifier_for(json).map_err(|error| error.to_string())?;
            crate::native_filter_cache::seed(store.cache_path(), id.as_str(), json)
                .map_err(|error| error.to_string())?;
            let task = store
                .load(&id, Duration::from_secs(5))
                .map_err(|error| error.to_string())?;
            let deadline = std::time::Instant::now() + Duration::from_secs(6);
            loop {
                worker.session.pump(2);
                if let Some(result) = task.take_result() {
                    result.map_err(|error| format!("Bundled filter did not load: {error}"))?;
                    break;
                }
                if std::time::Instant::now() >= deadline {
                    return Err("Bundled filter load timed out".into());
                }
            }
            std::fs::write(
                store
                    .cache_path()
                    .join(format!("ContentRuleList-{}", id.as_str())),
                b"invalid native filter",
            )
            .map_err(|error| error.to_string())?;
            let rejected = store
                .load(&id, Duration::from_secs(5))
                .map_err(|error| error.to_string())?;
            let deadline = std::time::Instant::now() + Duration::from_secs(6);
            loop {
                worker.session.pump(2);
                if let Some(result) = rejected.take_result() {
                    if result.is_ok() {
                        return Err("Corrupted filter control unexpectedly loaded".into());
                    }
                    break;
                }
                if std::time::Instant::now() >= deadline {
                    return Err("Corrupted filter control timed out".into());
                }
            }
        }
    }
    privacy.toggle_site("ads", &base);
    install(&mut worker, &privacy)?;
    let initial_ids = worker
        .filter_installation
        .as_ref()
        .ok_or("Missing filter bundle")?
        .identifiers()
        .to_vec();
    if initial_ids.len() != 2 {
        return Err("Ad and tracker policies did not retain separate native identities".into());
    }
    let slot = worker
        .filter_installation
        .as_mut()
        .ok_or("Missing bundle")?;
    if slot.replace_all(&[]).is_ok() || slot.identifiers() != initial_ids.as_slice() {
        return Err("Invalid bundle replacement changed active protection".into());
    }
    let allowed = format!("{base}/privacy?phase=A");
    let protected = format!("{other}/privacy?phase=B");
    images(&mut worker, &allowed, true)?;
    images(&mut worker, &protected, false)?;
    worker.session.go_back().map_err(|e| e.to_string())?;
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        worker.session.pump(2);
        if evaluate(&mut worker, "location.href").ok().as_ref()
            == Some(&serde_json::Value::String(allowed.clone()))
            && evaluate(
                &mut worker,
                "document.getElementById('ad')?.naturalWidth>0 && document.getElementById('tracker')?.naturalWidth===0",
            )? == true
        {
            break;
        }
        if std::time::Instant::now() > deadline {
            return Err("Back navigation lost the scoped ad exception or tracker blocking".into());
        }
    }
    privacy.toggle_site("ads", &base);
    install(&mut worker, &privacy)?;
    let ids = worker
        .filter_installation
        .as_ref()
        .ok_or("Missing replacement bundle")?
        .identifiers();
    if ids.len() != 2 || ids[0] == initial_ids[0] || ids[1] != initial_ids[1] {
        return Err(
            "One-category replacement did not preserve the independent tracker identity".into(),
        );
    }
    images(&mut worker, &format!("{base}/privacy?phase=C"), false)?;
    privacy.ads = false;
    install(&mut worker, &privacy)?;
    images(&mut worker, &format!("{base}/privacy?phase=D"), false)?;
    images(&mut worker, &format!("{other}/privacy?phase=E"), true)?;
    for enabled in [false, true] {
        let mut policy = crate::privacy::Privacy {
            blocking: 2,
            ..Default::default()
        };
        if !enabled {
            policy.toggle_site("ads", "https://edition.cnn.com/");
        }
        install(&mut worker, &policy)?;
        Worker::install_scripts_with_policy(
            &mut worker.session,
            true,
            Some("https://edition.cnn.com/"),
            Some(&policy),
            false,
            true,
        );
        worker.session.load_html("<div class=ad-slot-header__wrapper style='height:300px'>Ad</div><main id=ordinary style='height:120px'>Article</main>", "https://edition.cnn.com/", 10000).map_err(|e| e.to_string())?;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let value = evaluate(
                &mut worker,
                "[document.querySelector('.ad-slot-header__wrapper').getBoundingClientRect().height,document.getElementById('ordinary').getBoundingClientRect().height]",
            )?;
            if value == serde_json::json!([if enabled { 0 } else { 300 }, 120]) {
                break;
            }
            if std::time::Instant::now() > deadline {
                return Err(format!(
                    "Scoped cosmetic policy failed: enabled={enabled}, sizes={value}"
                ));
            }
        }
    }
    let youtube = "<script>window.ytInitialPlayerResponse={playerAds:[{ad:true}],adPlacements:[{ad:true}],videoDetails:{videoId:'keep-video'},streamingData:{formats:[{url:'keep-stream'}]}};</script><main>Article</main>";
    for enabled in [false, true] {
        let mut policy = crate::privacy::Privacy::default();
        if !enabled {
            policy.toggle_site("ads", "https://www.youtube.com/");
        }
        worker.privacy = policy.clone();
        worker.filter_mode.set(Some(1));
        Worker::install_scripts_with_policy(
            &mut worker.session,
            true,
            Some("https://www.youtube.com/"),
            Some(&policy),
            false,
            true,
        );
        worker
            .session
            .load_html(
                youtube,
                "https://www.youtube.com/watch?v=privacy-fixture",
                10000,
            )
            .map_err(|e| e.to_string())?;
        let value = evaluate(
            &mut worker,
            "[ytInitialPlayerResponse.playerAds?.length||0,ytInitialPlayerResponse.videoDetails.videoId,ytInitialPlayerResponse.streamingData.formats[0].url]",
        )?;
        if value != serde_json::json!([usize::from(!enabled), "keep-video", "keep-stream"]) {
            return Err(format!(
                "Scoped player-script policy failed: enabled={enabled}, value={value}"
            ));
        }
    }
    Ok(
        serde_json::json!({"site_ads_allowed_without_trackers":true,"other_port_remains_protected":true,"back_navigation_preserves_scoped_policy":true,"same_level_policy_replacement":true,"global_allow_with_site_block_override":true,"independent_native_bundle":true,"invalid_replacement_keeps_protection":true,"cosmetic_policy_matches_site":true,"player_scripts_match_ad_choice":true,"private_filter_cache_is_temporary":true,"bundled_filters_load_without_compilation":true,"invalid_cached_bytecode_recompiles_before_navigation":true}),
    )
}
