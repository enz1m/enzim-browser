use crate::{
    App, Message,
    browser::{Browser, Command, SleepingTab, TabInfo, next_tab_key},
    webview::WebViewService,
};
use std::{cell::RefCell, time::Instant};

pub(crate) fn run() -> Result<serde_json::Value, String> {
    fn make(tabs: usize) -> App {
        let mut browser = Browser::load_with_privacy(true);
        browser.saved.tabs.clear();
        browser.sleeping.clear();
        browser.library = None;
        for index in 0..tabs {
            let url = format!(
                "https://site{index}.example/articles/a-long-document-address?topic=browser&view=full#paragraph"
            );
            browser.saved.tabs.push(TabInfo {
                key: next_tab_key(),
                url: url.clone(),
                title: format!("A retained document in tab {index}"),
                favicon: None,
                page_copy: None,
                pinned: false,
                group: None,
                container: None,
            });
            browser.sleeping.push(if index == 0 {
                None
            } else {
                let mut host = WebViewService::new();
                host.location = Some(url);
                host.dom_report = Some(format!("A retained document in tab {index}"));
                Some(SleepingTab {
                    pdf: None,
                    host: RefCell::new(host),
                })
            });
        }
        browser.saved.active = 0;
        let mut app = App::with_browser(enzui_theme::EnzTheme::dark(), browser);
        app.browser_mode = true;
        app.webview.borrow_mut().location = Some(app.browser.saved.tabs[0].url.clone());
        app.webview.borrow_mut().dom_report = Some(app.browser.saved.tabs[0].title.clone());
        app.sync_browser();
        app
    }
    let mut reports = Vec::new();
    for tabs in [1, 10, 50] {
        let mut app = make(tabs);
        for round in 0..6 {
            for fast in if round % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                app.input_sync_fast = fast;
                for step in 0..100 {
                    drop(
                        app.update(Message::Chrome(Command::Pointer(iced::Point::new(
                            step as f32,
                            100.,
                        )))),
                    );
                }
                let iterations = 10000;
                let started = Instant::now();
                for step in 0..iterations {
                    drop(
                        app.update(Message::Chrome(Command::Pointer(iced::Point::new(
                            (step % 1000) as f32,
                            100.,
                        )))),
                    );
                }
                let elapsed = started.elapsed();
                if app.browser.saved.tabs.len() != tabs
                    || app.browser.saved.tabs[0].title != "A retained document in tab 0"
                {
                    return Err("Input benchmark changed the document model.".into());
                }
                reports.push(serde_json::json!({"tabs":tabs,"fast":fast,"round":round,"iterations":iterations,"wall_ms":elapsed.as_secs_f64()*1000.,"us_per_input":elapsed.as_secs_f64()*1_000_000./f64::from(iterations)}));
            }
        }
    }
    Ok(
        serde_json::json!({"scope":"App update with stable retained metadata; no native renderer or presentation timing","samples":reports}),
    )
}
