//! enzim Browser: native enzUI chrome around independently retained WPE pages.
mod browser_clipboard;
mod browser_input;
mod browser_pdf;
mod browser_profile;
mod container_privacy;
mod cosmetic_engine;
mod credential_bridge;
mod credentials;
mod desktop;
mod desktop_files;
mod startup;
mod surface_input;
mod user_agent;
mod viewport_background;

mod browser;
mod diagnostic_log;
mod download_activity;
mod downloads;
mod favicons;
#[cfg(feature = "benchmark")]
mod filter_memory_bench;
mod filter_rules;
mod filters;
mod history;
mod imports;
#[cfg(feature = "benchmark")]
mod input_sync_bench;
mod message;
#[cfg(feature = "wpe")]
mod native_filter_cache;
#[cfg(feature = "wpe")]
mod native_filter_identity;
mod native_interest;
mod native_jobs;
mod page_loading;
mod pdf;
mod prepared_tab;
mod privacy;
mod profiles;
mod routes;
mod search;
mod search_suggestions;
mod site_permissions;
mod update;
mod web_fonts;
mod website_notifications;
mod webview;
mod webview_native;
mod webview_scratch;
mod welcome;
mod page_ids;
mod windows;

mod allocator;
mod appearance;
mod corners;
mod glass;
mod app;
mod assistant;
pub(crate) use app::App;

pub(crate) use message::{Message, drain_a11y_actions};

pub(crate) fn main() -> iced::Result {
    #[cfg(target_os = "linux")]
    startup::prepare_descriptor_table();
    #[cfg(all(target_os = "linux", feature = "wpe"))]
    if std::path::Path::new("/.flatpak-info").is_file()
        && let Err(error) = enzui_webkit::ffi::deployment::configure_flatpak_application(
            "org.enzim.Browser",
            "/app/lib/wpe-extensions",
        )
    {
        eprintln!("Could not initialize the Flatpak web engine: {error}");
        std::process::exit(1);
    }
    allocator::configure_startup();
    if let Some(index) = std::env::args().position(|arg| arg == "--write-desktop-entry") {
        let Some(path) = std::env::args().nth(index + 1) else {
            eprintln!("--write-desktop-entry needs a path");
            std::process::exit(2);
        };
        if let Err(error) = std::fs::write(path, desktop::desktop_entry()) {
            eprintln!("Could not write the desktop entry: {error}");
            std::process::exit(1);
        }
        return Ok(());
    }

    #[cfg(all(feature = "benchmark", feature = "wpe"))]
    if std::env::args().any(|arg| arg == "--opening-check") {
        match webview_native::run_opening_gate() {
            Ok(value) => eprintln!("OPENING_CHECK {value}"),
            Err(error) => {
                eprintln!("OPENING_CHECK_FAILED {error}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    #[cfg(feature = "benchmark")]
    if std::env::args().any(|arg| arg == "--filter-memory-bench") {
        let result = webview_scratch::SecureScratch::open_for_native_webview()
            .and_then(|scratch| filter_memory_bench::run(scratch.path().to_path_buf()));
        match result {
            Ok(value) => eprintln!("FILTER_MEMORY_BENCH {value}"),
            Err(error) => {
                eprintln!("FILTER_MEMORY_BENCH_FAILED {error}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    #[cfg(feature = "benchmark")]
    if std::env::args().any(|arg| arg == "--credentials-check") {
        let result = webview_scratch::SecureScratch::open_for_native_webview()
            .and_then(|scratch| webview_native::run_credentials_gate(scratch.path().to_path_buf()));
        match result {
            Ok(value) => eprintln!("CREDENTIALS_CHECK {value}"),
            Err(error) => {
                eprintln!("CREDENTIALS_CHECK_FAILED {error}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    #[cfg(feature = "benchmark")]
    if std::env::args().any(|arg| arg == "--passkeys-check") {
        let result = webview_scratch::SecureScratch::open_for_native_webview()
            .and_then(|scratch| webview_native::run_passkeys_gate(scratch.path().to_path_buf()));
        match result {
            Ok(value) => eprintln!("PASSKEYS_CHECK {value}"),
            Err(error) => {
                eprintln!("PASSKEYS_CHECK_FAILED {error}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    if let Some(position) = std::env::args().position(|v| v == "--pdf-bench") {
        #[cfg(feature = "benchmark")]
        {
            let Some(path) = std::env::args().nth(position + 1) else {
                eprintln!("A PDF path is required");
                std::process::exit(2);
            };
            match pdf::benchmark(path.into()) {
                Ok(result) => eprintln!("PDF_BENCH {result}"),
                Err(error) => {
                    eprintln!("PDF_BENCH_FAILED {error}");
                    std::process::exit(1);
                }
            }
            return Ok(());
        }
        #[cfg(not(feature = "benchmark"))]
        {
            let _ = position;
            eprintln!("Build with --features benchmark to run --pdf-bench");
            std::process::exit(2);
        }
    }
    #[cfg(feature = "benchmark")]
    if std::env::args().any(|arg| arg == "--filters-bench") {
        match filters::benchmark() {
            Ok(value) => eprintln!("FILTERS_BENCH {value}"),
            Err(error) => {
                eprintln!("FILTERS_BENCH_FAILED {error}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    if std::env::args().any(|arg| arg == "--chrome-input-bench") {
        #[cfg(feature = "benchmark")]
        {
            match input_sync_bench::run() {
                Ok(value) => eprintln!("CHROME_INPUT_BENCH {value}"),
                Err(error) => {
                    eprintln!("CHROME_INPUT_BENCH_FAILED {error}");
                    std::process::exit(1);
                }
            }
            return Ok(());
        }
        #[cfg(not(feature = "benchmark"))]
        {
            eprintln!("Build with --features benchmark to run --chrome-input-bench");
            std::process::exit(2);
        }
    }
    if std::env::args().any(|arg| arg == "--encrypted-media-check") {
        #[cfg(feature = "benchmark")]
        {
            match webview_scratch::SecureScratch::open_for_native_webview().and_then(|scratch| {
                webview_native::run_encrypted_media_gate(scratch.path().to_path_buf())
            }) {
                Ok(value) => eprintln!("ENCRYPTED_MEDIA_CHECK {value}"),
                Err(error) => {
                    eprintln!("ENCRYPTED_MEDIA_CHECK_FAILED {error}");
                    std::process::exit(1);
                }
            }
            return Ok(());
        }
        #[cfg(not(feature = "benchmark"))]
        {
            eprintln!("Build with --features benchmark to run --encrypted-media-check");
            std::process::exit(2);
        }
    }
    if std::env::args().any(|arg| arg == "--devtools-check") {
        #[cfg(feature = "benchmark")]
        {
            match webview_scratch::SecureScratch::open_for_native_webview()
                .and_then(|scratch| webview_native::run_devtools_gate(scratch.path().to_path_buf()))
            {
                Ok(value) => eprintln!("DEVTOOLS_CHECK {value}"),
                Err(error) => {
                    eprintln!("DEVTOOLS_CHECK_FAILED {error}");
                    std::process::exit(1);
                }
            }
            return Ok(());
        }
        #[cfg(not(feature = "benchmark"))]
        {
            eprintln!("Build with --features benchmark to run --devtools-check");
            std::process::exit(2);
        }
    }
    #[cfg(feature = "benchmark")]
    if std::env::args().any(|arg| arg == "--theme-check") {
        let result = webview_scratch::SecureScratch::open_for_native_webview()
            .and_then(|s| webview_native::run_theme_gate(s.path().to_path_buf()));
        match result {
            Ok(value) => eprintln!("THEME_CHECK {value}"),
            Err(error) => {
                eprintln!("THEME_CHECK_FAILED {error}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    #[cfg(not(feature = "benchmark"))]
    if std::env::args().any(|arg| arg == "--youtube-check") {
        eprintln!("Build with --features benchmark to run --youtube-check");
        std::process::exit(2);
    }
    #[cfg(feature = "benchmark")]
    if std::env::args().any(|arg| arg == "--youtube-check") {
        let result = webview_scratch::SecureScratch::open_for_native_webview()
            .and_then(|s| webview_native::run_youtube_gate(s.path().to_path_buf()));
        match result {
            Ok(value) => eprintln!("YOUTUBE_CHECK {value}"),
            Err(error) => {
                eprintln!("YOUTUBE_CHECK_FAILED {error}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    #[cfg(not(feature = "benchmark"))]
    if std::env::args().any(|arg| arg == "--downloads-check") {
        eprintln!("Build with --features benchmark to run --downloads-check");
        std::process::exit(2);
    }
    #[cfg(feature = "benchmark")]
    if std::env::args().any(|arg| arg == "--downloads-check") {
        let result = webview_scratch::SecureScratch::open_for_native_webview()
            .and_then(|s| webview_native::run_download_gate(s.path().to_path_buf()));
        match result {
            Ok(value) => eprintln!("DOWNLOADS_CHECK {value}"),
            Err(error) => {
                eprintln!("DOWNLOADS_CHECK_FAILED {error}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    #[cfg(feature = "benchmark")]
    if std::env::args().any(|arg| arg == "--filter-info") {
        for mode in [1, 2] {
            let filter = filters::compiled(mode);
            match filter {
                Ok(value) => eprintln!(
                    "FILTER_INFO mode={mode} source_bytes={} rules={} input={}",
                    value.json.len(),
                    value.rules,
                    value.input
                ),
                Err(_) => std::process::exit(1),
            }
        }
        return Ok(());
    }
    #[cfg(feature = "benchmark")]
    if std::env::args().any(|arg| arg == "--metadata-check" || arg == "--metadata-bench") {
        let result =
            webview_scratch::SecureScratch::open_for_native_webview().and_then(|scratch| {
                if std::env::args().any(|arg| arg == "--metadata-bench") {
                    webview_native::run_metadata_benchmark(scratch.path().to_path_buf())
                } else {
                    webview_native::run_metadata_gate(scratch.path().to_path_buf())
                }
            });
        if let Err(error) = result {
            eprintln!("METADATA_CHECK_FAILED {error}");
            std::process::exit(1);
        }
        return Ok(());
    }

    #[cfg(feature = "benchmark")]
    if std::env::args().any(|arg| arg == "--browser-feature-check") {
        let result =
            webview_scratch::SecureScratch::open_for_native_webview().and_then(|scratch| {
                webview_native::run_browser_feature_gate(scratch.path().to_path_buf())
            });
        match result {
            Ok(report) => eprintln!("BROWSER_FEATURE_CHECK {report}"),
            Err(error) => {
                eprintln!("BROWSER_FEATURE_CHECK_FAILED {error}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }

    #[cfg(feature = "benchmark")]
    if std::env::args().any(|arg| arg == "--followup-check") {
        let result = webview_scratch::SecureScratch::open_for_native_webview()
            .and_then(|scratch| webview_native::run_followup_gate(scratch.path().to_path_buf()));
        match result {
            Ok(report) => eprintln!("FOLLOWUP_CHECK {report}"),
            Err(error) => {
                eprintln!("FOLLOWUP_CHECK_FAILED {error}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }

    #[cfg(feature = "benchmark")]
    if std::env::args().any(|arg| arg == "--privacy-check") {
        let result = webview_scratch::SecureScratch::open_for_native_webview()
            .and_then(|scratch| webview_native::run_privacy_gate(scratch.path().to_path_buf()));
        match result {
            Ok(report) => eprintln!("PRIVACY_CHECK {report}"),
            Err(error) => {
                eprintln!("PRIVACY_CHECK_FAILED {error}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }

    #[cfg(feature = "benchmark")]
    if std::env::args().any(|arg| arg == "--assets-check") {
        let result = webview_scratch::SecureScratch::open_for_native_webview()
            .and_then(|scratch| webview_native::run_assets_gate(scratch.path().to_path_buf()));
        match result {
            Ok(report) => eprintln!("ASSETS_CHECK {report}"),
            Err(error) => {
                eprintln!("ASSETS_CHECK_FAILED {error}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }

    #[cfg(feature = "benchmark")]
    if std::env::args().any(|arg| arg == "--volume-check") {
        let result = webview_scratch::SecureScratch::open_for_native_webview()
            .and_then(|scratch| webview_native::run_volume_gate(scratch.path().to_path_buf()));
        match result {
            Ok(report) => eprintln!("VOLUME_CHECK {report}"),
            Err(error) => {
                eprintln!("VOLUME_CHECK_FAILED {error}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }

    #[cfg(feature = "benchmark")]
    if std::env::args().any(|arg| arg == "--latency-check") {
        let result = webview_scratch::SecureScratch::open_for_native_webview()
            .and_then(|scratch| webview_native::run_latency_gate(scratch.path().to_path_buf()));
        match result {
            Ok(report) => eprintln!("LATENCY_CHECK {report}"),
            Err(error) => {
                eprintln!("LATENCY_CHECK_FAILED {error}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }

    #[cfg(feature = "benchmark")]
    if std::env::args().any(|arg| arg == "--storage-check") {
        let result = webview_scratch::SecureScratch::open_for_native_webview()
            .and_then(|scratch| webview_native::run_storage_gate(scratch.path().to_path_buf()));
        match result {
            Ok(report) => eprintln!("STORAGE_CHECK {report}"),
            Err(error) => {
                eprintln!("STORAGE_CHECK_FAILED {error}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }

    if std::env::args().any(|arg| arg == "--help" || arg == "-h") {
        eprintln!(
            "enzim-browser [--browser URL] [--private-window] [-P [PROFILE]] [--profile PROFILE] [--create-profile NAME]\n-P opens the profile chooser. Names and profile IDs are accepted. --create-profile creates and opens a fresh profile."
        );
        return Ok(());
    }
    match browser_profile::configure(&std::env::args().collect::<Vec<_>>()) {
        Ok(true) => return Ok(()),
        Ok(false) => {}
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }

    // Keep browser menus and site panels reachable inside compact windows.
    enzui_widgets::anchored_overlay::set_default_popup_overflow(
        enzui_widgets::anchored_overlay::PopupOverflow::Never,
    );

    let _ = enzui_clipboard_runtime::adopt_process_clipboard();

    // The accessibility bridge is seated before the first window and owned for
    // the whole loop.
    let result = enzui_core::app_runtime::run(|| {
        iced::daemon(
            windows::Windows::boot,
            windows::Windows::update,
            windows::Windows::view,
        )
        .settings(iced::Settings {
            id: Some("org.enzim.Browser".into()),
            ..Default::default()
        })
        .title(windows::Windows::title)
        .style(|_, theme| iced::theme::Style {
            background_color: iced::Color::TRANSPARENT,
            text_color: theme.palette().background.base.text,
        })
        .theme(windows::Windows::theme)
        .subscription(windows::Windows::subscription)
        .run()
    });
    let _ = enzui_clipboard_runtime::keep_clipboard_after_exit();
    result
}
