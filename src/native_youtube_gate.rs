#[cfg(feature = "benchmark")]
pub(super) fn run_youtube_gate(scratch: PathBuf) -> Result<serde_json::Value, String> {
    use enzui_webkit::page::{PageEvent, ScriptFrames, ScriptTiming, ScriptWorld, UserScript};
    fn evaluate(worker: &mut Worker, source: &str) -> Result<serde_json::Value, String> {
        let id = worker
            .session
            .evaluate_script(source, &ScriptWorld::Page)
            .map_err(|e| e.to_string())?;
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            worker.session.pump(2);
            for event in worker.session.take_page_events() {
                if let PageEvent::ScriptResult { id: found, result } = event
                    && found == id
                {
                    return result
                        .map_err(|e| format!("{e:?}"))
                        .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()));
                }
            }
            if std::time::Instant::now() > deadline {
                return Err("YouTube qualification evaluation stalled".into());
            }
        }
    }
    let mut router = ViewRouter::new();
    let view = router.create(993);
    let mut worker = Worker::open(
        router,
        view,
        scratch,
        None,
        MediaGeneration::FIRST,
        WebProfile::ephemeral(),
    )?;
    let default_agent = worker.default_user_agent.clone();
    let policy = crate::privacy::Privacy::default();
    policy.apply_for_site(
        &worker.session,
        "https://www.youtube.com/",
        false,
        &default_agent,
    );
    let compatible_agent = worker.session.user_agent();
    if !compatible_agent.contains("Macintosh; Intel Mac OS X 10_15_7")
        || compatible_agent.split_once(')').map(|(_, engine)| engine)
            != default_agent.split_once(')').map(|(_, engine)| engine)
    {
        return Err("YouTube compatibility did not preserve the native engine identity".into());
    }
    let custom = crate::privacy::Privacy {
        user_agent: "Enzim explicit agent".into(),
        ..Default::default()
    };
    custom.apply_for_site(
        &worker.session,
        "https://www.youtube.com/",
        false,
        &default_agent,
    );
    if worker.session.user_agent() != custom.user_agent {
        return Err("YouTube compatibility replaced an explicit user agent".into());
    }
    policy.apply_for_site(
        &worker.session,
        "https://example.org/",
        false,
        &default_agent,
    );
    if worker.session.user_agent() != default_agent {
        return Err("YouTube compatibility escaped its site scope".into());
    }
    worker.session.disable_gpu_presentation();
    worker.gpu_enabled = false;
    let mut controls = Vec::new();
    for (enabled, site_exception) in [(true, false), (false, false), (false, true)] {
        worker.session.remove_user_scripts();
        worker.session.add_user_script(&UserScript {
            source: r#"
                globalThis.playerFixture = {
                    playerAds: [{ad:true}], adPlacements: [{ad:true}], adSlots: [{ad:true}],
                    videoDetails: {videoId:'keep-content', lengthSeconds:'600'},
                    streamingData: {formats:[{url:'keep-stream'}]}, ordinary: {caption:'keep-nested'}
                };
                globalThis.fixtureFor = function(url) {
                    if (url.includes('resolve_url')) return {endpoint:{watchEndpoint:{unserializedPrefetchData:{playerResponse:playerFixture}}}};
                    if (url.includes('shape=array')) return [{playerResponse:playerFixture}];
                    if (url.includes('shape=envelope')) return {playerResponse:playerFixture};
                    return playerFixture;
                };
                globalThis.fetch = function(input) {
                    const url = typeof input === 'string' ? input : input.url;
                    return Promise.resolve(new Response(JSON.stringify(fixtureFor(url)), {headers:{'content-type':'application/json'}}));
                };
                globalThis.XMLHttpRequest = class extends EventTarget {
                    open(method, url) { this.fixtureUrl=url; }
                    send() {}
                    get responseText() { return JSON.stringify(fixtureFor(this.fixtureUrl)); }
                    get response() { return this.responseType === 'json' ? fixtureFor(this.fixtureUrl) : JSON.stringify(fixtureFor(this.fixtureUrl)); }
                };
            "#.into(),
            world: ScriptWorld::Page,
            timing: ScriptTiming::DocumentStart,
            frames: ScriptFrames::TopFrame,
        }).map_err(|e| e.to_string())?;
        let mut policy = crate::privacy::Privacy {
            ads: enabled || site_exception,
            ..Default::default()
        };
        if site_exception {
            policy.toggle_site("ads", "https://www.youtube.com/");
        }
        let mut scoped_bytes = 0;
        let mut scoped_scripts = 0;
        for mut script in
            Worker::policy_scripts(true, Some("https://www.youtube.com/"), Some(&policy), false)
        {
            if !script.matches.is_empty() {
                scoped_bytes += script.script.source.len();
                scoped_scripts += 1;
                script.script.source =
                    "globalThis.__enzimScopeCanary=(globalThis.__enzimScopeCanary??0)+1;"
                        .to_owned()
                        + &script.script.source;
                if std::env::var_os("ENZIM_SCOPE_FAULT").is_some_and(|value| value == "1") {
                    script.matches.clear();
                }
            }
            script.install(&worker.session).map_err(|e| e.to_string())?;
        }
        worker.session.load_html("<title>Player response transport fixture</title><body><div id=movie_player><div class=video-ads><button class=ytp-ad-skip-button-modern>Skip</button></div></div></body>","https://www.youtube.com/watch?v=fixture",10000).map_err(|e|e.to_string())?;
        evaluate(
            &mut worker,
            r#"
                globalThis.transportResults = undefined;
                globalThis.transportFailure = undefined;
                (async () => {
                    const result = [];
                    const endpoints = ['/youtubei/v1/player', '/youtubei/v1/player?prettyPrint=false',
                        '/youtubei/v1/player?shape=envelope', '/youtubei/v1/player?shape=array',
                        '/youtubei/v1/next', '/youtubei/v1/get_watch?shape=envelope',
                        '/youtubei/v1/get_watch?shape=array', '/youtubei/v1/resolve_url?prettyPrint=false', '/ordinary'];
                    function record(endpoint, data, transport = 'fetch') {
                        const player = data[0]?.playerResponse ?? data.playerResponse ?? data.endpoint?.watchEndpoint?.unserializedPrefetchData?.playerResponse ?? data;
                        result.push({endpoint, transport, ads:!!player.playerAds, placements:!!player.adPlacements, slots:!!player.adSlots,
                            video:player.videoDetails?.videoId, stream:player.streamingData?.formats?.[0]?.url, nested:player.ordinary?.caption});
                    }
                    for (const endpoint of endpoints) {
                        globalThis.transportStage = endpoint;
                        const response = await fetch(new Request('https://www.youtube.com' + endpoint));
                        record(endpoint, await response.json());
                        const xhr = new XMLHttpRequest();
                        xhr.open('POST', 'https://www.youtube.com' + endpoint); xhr.send();
                        record(endpoint, JSON.parse(xhr.responseText), 'xhr');
                    }
                    globalThis.transportStage = 'iframe';
                    const frame = document.createElement('iframe');
                    frame.hidden = true;
                    document.documentElement.appendChild(frame);
                    const url = 'data:application/json,' + encodeURIComponent(JSON.stringify(playerFixture)) + '#/youtubei/v1/player?iframe';
                    record('iframe-player', await (await frame.contentWindow.fetch(new frame.contentWindow.Request(url))).json(), 'iframe');
                    frame.remove();
                    transportResults = result;
                })().catch(error => { globalThis.transportFailure = {message:String(error), stack:error.stack?.slice(0,1500), stage:globalThis.transportStage}; });
                undefined
            "#,
        )?;
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        let values = loop {
            let value = evaluate(
                &mut worker,
                "transportFailure?{error:transportFailure}:transportResults||null",
            )?;
            if !value.is_null() {
                break value;
            }
            if std::time::Instant::now() > deadline {
                return Err("Response transport fixture stalled".into());
            }
        };
        let rows = values
            .as_array()
            .ok_or_else(|| format!("Response fixture returned no rows: {values}"))?;
        if rows.len() != 19 {
            return Err(format!(
                "Response fixture returned {} of 19 rows",
                rows.len()
            ));
        }
        for row in rows {
            let blocked = enabled && row["endpoint"] != "/ordinary";
            if row["ads"] != !blocked
                || row["placements"] != !blocked
                || row["slots"] != !blocked
                || row["video"] != "keep-content"
                || row["stream"] != "keep-stream"
                || row["nested"] != "keep-nested"
            {
                return Err(format!(
                    "enabled={enabled}: response altered incorrectly: {row}"
                ));
            }
        }
        let matched = evaluate(&mut worker, "globalThis.__enzimScopeCanary??0")?;
        if matched != serde_json::json!(1) {
            return Err(format!(
                "Native host matching evaluated {matched} scriptlets on YouTube"
            ));
        }
        worker
            .session
            .load_html(
                "<title>Unrelated script scope</title>",
                "https://outside.test/",
                10000,
            )
            .map_err(|e| e.to_string())?;
        let unrelated = evaluate(&mut worker, "globalThis.__enzimScopeCanary??0")?;
        if unrelated != serde_json::json!(0) {
            return Err(format!(
                "Native host matching evaluated {unrelated} unrelated scriptlets"
            ));
        }
        controls.push(serde_json::json!({"ads_enabled":enabled,"site_exception":site_exception,"responses":values,
            "scoped_scripts":scoped_scripts,"scoped_source_bytes":scoped_bytes,"unrelated_evaluated":unrelated}));
    }
    worker.session.remove_user_scripts();
    worker
        .session
        .load_html(
            "<title>Native video ad endpoint control</title>",
            "https://www.youtube.com/",
            10000,
        )
        .map_err(|e| e.to_string())?;
    let mut network_controls = Vec::new();
    for (enabled, site_exception) in [(true, false), (false, true), (false, false)] {
        let mut policy = crate::privacy::Privacy {
            ads: enabled || site_exception,
            trackers: false,
            ..Default::default()
        };
        if site_exception {
            policy.toggle_site("ads", "https://www.youtube.com/");
        }
        let (mode, filters) = crate::filters::for_privacy(&policy)?;
        let (job, mut receiver) = crate::native_jobs::Job::new();
        worker.configure_filters(mode, filters, job);
        let deadline = std::time::Instant::now() + Duration::from_secs(35);
        loop {
            worker.session.pump(2);
            for mut operation in std::mem::take(&mut worker.native_operations) {
                if !operation(&mut worker) {
                    worker.native_operations.push(operation);
                }
            }
            if let Ok(result) = receiver.try_recv() {
                result?;
                break;
            }
            if std::time::Instant::now() > deadline {
                return Err("Native video filter installation stalled".into());
            }
        }
        evaluate(
            &mut worker,
            r#"globalThis.endpointResult=null;fetch('https://www.youtube.com/youtubei/v1/player/ad_break',{signal:AbortSignal.timeout(8000)}).then(r=>{endpointResult={resolved:true,status:r.status}}).catch(e=>{endpointResult={resolved:false,error:e.name}});undefined"#,
        )?;
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let result = loop {
            let result = evaluate(&mut worker, "endpointResult")?;
            if !result.is_null() {
                break result;
            }
            if std::time::Instant::now() > deadline {
                return Err("Native video ad endpoint control stalled".into());
            }
        };
        if result["resolved"] != !enabled || result["error"] == "TimeoutError" {
            return Err(format!("ads={enabled}: unexpected network result {result}"));
        }
        network_controls.push(serde_json::json!({"ads_enabled":enabled,"site_exception":site_exception,"result":result}));
    }
    Ok(
        serde_json::json!({"youtube_default_compatibility_identity":true,"explicit_agent_preserved":true,"other_sites_keep_default_identity":true,"transport_controls":controls,"balanced_native_network":network_controls}),
    )
}
