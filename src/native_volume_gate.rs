#[cfg(feature = "benchmark")]
pub(super) fn run_volume_gate(scratch: PathBuf) -> Result<serde_json::Value, String> {
    use enzui_webkit::page::{PageEvent, ScriptWorld};
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
                        .and_then(|json| serde_json::from_str(&json).map_err(|e| e.to_string()));
                }
            }
            if std::time::Instant::now() >= deadline {
                return Err("Volume probe evaluation timed out".into());
            }
        }
    }
    let mut router = ViewRouter::new();
    let view = router.create(995);
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
        .load_html(
            "<title>Media volume qualification</title>",
            "https://example.org/",
            10000,
        )
        .map_err(|e| e.to_string())?;
    evaluate(
        &mut worker,
        r#"
        window.volumeProbe = 'pending';
        (async()=>{
            const wait = ms => new Promise(resolve => setTimeout(resolve, ms));
            const measure = async media => {
                const report = { immediate: [], events: [] };
                const changed = () => report.events.push(media.volume);
                media.addEventListener('volumechange', changed);
                for (let i = 0; i < 20; ++i) {
                    const wanted = .13 + (i % 10) / 100;
                    media.volume = wanted;
                    report.immediate.push([wanted, media.volume]);
                    await wait(40);
                }
                await wait(250);
                report.settled = media.volume;
                const before = report.events.length;
                for (let i = 0; i < 10; ++i) { media.volume = .22; await wait(40); }
                await wait(250);
                report.repeatedEvents = report.events.length - before;
                media.removeEventListener('volumechange', changed);
                report.exact = report.immediate.every(([wanted, actual]) => wanted === actual) && report.settled === .22;
                report.healthy = report.exact && report.repeatedEvents === 0;
                return report;
            };
            const unloaded = await measure(new Audio());
            const bytes = new Uint8Array(16044), data = new DataView(bytes.buffer);
            const text = (at, value) => Array.from(value).forEach((char, i) => bytes[at+i] = char.charCodeAt(0));
            text(0,'RIFF');data.setUint32(4,16036,true);text(8,'WAVE');text(12,'fmt ');data.setUint32(16,16,true);data.setUint16(20,1,true);data.setUint16(22,1,true);data.setUint32(24,8000,true);data.setUint32(28,16000,true);data.setUint16(32,2,true);data.setUint16(34,16,true);text(36,'data');data.setUint32(40,16000,true);
            const media = new Audio();
            media.src = 'data:audio/wav;base64,' + btoa(String.fromCharCode(...bytes));
            media.preload = 'auto';media.load();
            for (let i = 0; media.readyState < 1 && !media.error && i < 400; ++i) await wait(20);
            if (media.readyState < 1) throw new Error('WAV decoding failed: ' + (media.error?.code || 'timeout'));
            const decoded = await measure(media);
            window.volumeProbe = JSON.stringify({unloaded, decoded, decodedReadyState: media.readyState});
        })().catch(error => window.volumeProbe = JSON.stringify({error: String(error)}));
        'started'
    "#,
    )?;
    let deadline = std::time::Instant::now() + Duration::from_secs(25);
    let report = loop {
        worker.session.pump(20);
        let value = evaluate(&mut worker, "window.volumeProbe")?;
        if let Some(value) = value.as_str()
            && value != "pending"
        {
            break serde_json::from_str::<serde_json::Value>(value).map_err(|e| e.to_string())?;
        }
        if std::time::Instant::now() >= deadline {
            return Err("Volume qualification did not complete".into());
        }
    };
    eprintln!("VOLUME_METRICS {report}");
    if let Some(error) = report.get("error") {
        return Err(format!("Volume media control unavailable: {error}"));
    }
    if report["unloaded"]["healthy"] != true {
        return Err("Unloaded media volume control failed".into());
    }
    if report["decoded"]["healthy"] != true {
        return Err("Decoded media changes requested volume or emits events for repeated values; engine issue #24".into());
    }
    Ok(report)
}
