#[cfg(feature = "benchmark")]
fn document_timing_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var_os("ENZIM_BENCH_DOCUMENT_TIMING").is_some_and(|value| value == "1")
    })
}

#[cfg(feature = "benchmark")]
impl Worker {
    fn register_document_timing(session: &mut WebSession) {
        if document_timing_enabled() {
            let registered = session.register_message_handler(
                "enzimBenchmarkTiming",
                &enzui_webkit::page::ScriptWorld::Isolated("enzim-browser-timing".into()),
            );
            if !registered {
                eprintln!("DOCUMENT_TIMING_ERROR message handler registration refused");
            }
        }
    }

    fn install_document_timing(session: &WebSession) {
        if !document_timing_enabled() {
            return;
        }
        let resources = if std::env::var_os("ENZIM_BENCH_RESOURCE_PHASE_AUDIT")
            .is_some_and(|value| value == "1")
        {
            include_str!("benchmark_resource_phases.js")
        } else if std::env::var_os("ENZIM_BENCH_LATE_BUNDLE_AUDIT")
            .is_some_and(|value| value == "1")
        {
            include_str!("benchmark_bundle_inventory_late.js")
        } else if std::env::var_os("ENZIM_BENCH_RESOURCE_ENTRIES").is_some_and(|value| value == "1")
        {
            include_str!("benchmark_resource_entries.js")
        } else if std::env::var_os("ENZIM_BENCH_RESOURCE_TIMING").is_some_and(|value| value == "1")
        {
            concat!(
                include_str!("benchmark_resource_timing.js"),
                "\n",
                include_str!("benchmark_bundle_inventory.js")
            )
        } else {
            ""
        };
        let bundle_labels = if std::env::var_os("ENZIM_BENCH_DOCUMENT_BUNDLE_LABELS")
            .is_some_and(|value| value == "1")
        {
            include_str!("benchmark_document_bundle_labels.js")
        } else {
            ""
        };
        let source = format!(
            "globalThis.__enzimBenchmarkEmit=payload=>window.webkit.messageHandlers.enzimBenchmarkTiming.postMessage(payload);\n{}\n{bundle_labels}\n{resources}",
            include_str!("benchmark_timing.js")
        );
        let result = session.add_user_script(&enzui_webkit::page::UserScript {
            source,
            world: enzui_webkit::page::ScriptWorld::Isolated("enzim-browser-timing".into()),
            timing: enzui_webkit::page::ScriptTiming::DocumentStart,
            frames: enzui_webkit::page::ScriptFrames::TopFrame,
        });
        if let Err(error) = result {
            eprintln!("DOCUMENT_TIMING_ERROR {error}");
        }
    }

    fn record_document_timing(&self, event: &enzui_webkit::page::PageEvent) -> bool {
        let enzui_webkit::page::PageEvent::ScriptMessage { handler, json } = event else {
            return false;
        };
        if handler != "enzimBenchmarkTiming" || !document_timing_enabled() {
            return false;
        }
        if json.len() > 65_536 {
            eprintln!("DOCUMENT_TIMING_ERROR oversized payload");
            return true;
        }
        match serde_json::from_str::<serde_json::Value>(json) {
            Ok(payload) => {
                let category = if payload.get("kind").and_then(serde_json::Value::as_str)
                    == Some("resource-diagnostic")
                {
                    "DOCUMENT_RESOURCE"
                } else {
                    "DOCUMENT_TIMING"
                };
                eprintln!(
                    "{category} {}",
                    serde_json::json!({
                        "view": format!("{:?}", self.view), "payload": payload
                    })
                );
            }
            Err(error) => eprintln!("DOCUMENT_TIMING_ERROR {error}"),
        }
        true
    }
}
