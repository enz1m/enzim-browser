//! What the menus and shortcuts do with tabs, links and a page's selection.

use super::*;

/// Links a page may hand to another application. A person's own click on
/// one is passed to the desktop; nothing else a page names is.
const EXTERNAL_SCHEMES: [&str; 3] = ["mailto", "tel", "sms"];

/// Ctrl+9: the last tab, however many there are.
pub(crate) const LAST_TAB: usize = usize::MAX;

#[derive(Debug, Clone)]
pub(crate) enum ActionCommand {
    /// Silence a tab, or let it sound again.
    Mute(usize),
    ReloadTab(usize),
    /// Close every tab but this one and the pinned ones.
    CloseOthers(usize),
    /// Close the tabs after this one, but for the pinned ones.
    CloseAfter(usize),
    /// Move the tab to a window of its own, with its page as it is where
    /// it has one; otherwise close it and open its address there.
    MoveToWindow(usize),
    /// Open an address in a new window; `true` for a private one. From a
    /// private window it is always private.
    OpenWindow(String, bool),
    /// Open a link in a tab without turning to it.
    VisitBackground(String),
    /// Ctrl+1 to Ctrl+9.
    SelectNth(usize),
    /// Reload without the cache.
    HardReload,
    /// Download what a link or an image points at.
    Save(String),
    Saved(Result<(), String>),
    /// A page's link to another application, clicked by the person.
    External(String),
    ExternalOpened(Result<(), String>),
    /// Search for the text selected in the page.
    SearchSelection,
    Selection(Result<String, String>),
    /// Copy the selection, then remove it.
    Cut,
    /// Open the page on show in a translation service, in a new tab.
    Translate,
    /// Open the source of the page on show as text, in a new tab.
    ViewSource,
    Source(Result<String, String>),
    /// Open the readable part of the page on show, set as plain reading
    /// matter, in a new tab.
    Reader,
    Readable(Result<String, String>),
}

/// The page a reader view is: what `reader.js` took from an article, in a
/// column of its own in the desktop's light or dark. `None` where the script
/// found nothing to read. The script escapes every text and keeps only the
/// tags of plain reading matter; the title is escaped here.
pub(crate) fn reader_page(extracted: &str) -> Option<String> {
    let extracted: serde_json::Value = serde_json::from_str(extracted).ok()?;
    let text = |key: &str| extracted.get(key).and_then(serde_json::Value::as_str);
    let (title, site, html) = (text("title")?, text("site")?, text("html")?);
    let escape = |text: &str| {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    Some(format!(
        "<!doctype html>\n<meta charset=\"utf-8\">\n<meta name=\"referrer\" content=\"no-referrer\">\n<title>{title}</title>\n<style>\n\
         :root {{ color-scheme: light dark; }}\n\
         body {{ margin: 0 auto; padding: 48px 24px 96px; max-width: 42rem; font: 19px/1.65 Georgia, 'Times New Roman', serif; background: #fbfaf7; color: #1f1f1f; }}\n\
         h1, h2, h3, h4, h5, h6 {{ font-family: system-ui, sans-serif; line-height: 1.25; margin: 1.6em 0 0.5em; }}\n\
         .site {{ font: 14px system-ui, sans-serif; opacity: 0.6; margin: 0; }}\n\
         .title {{ margin-top: 0.2em; font-size: 2em; }}\n\
         a {{ color: inherit; text-decoration-color: rgba(127, 127, 127, 0.7); }}\n\
         blockquote {{ margin: 1em 0; padding-left: 1em; border-left: 3px solid rgba(127, 127, 127, 0.5); }}\n\
         pre, code {{ font: 15px/1.5 ui-monospace, monospace; }}\n\
         pre {{ overflow-x: auto; padding: 12px; background: rgba(127, 127, 127, 0.14); border-radius: 6px; }}\n\
         @media (prefers-color-scheme: dark) {{ body {{ background: #1c1c1e; color: #e6e4df; }} }}\n\
         </style>\n<p class=\"site\">{site}</p>\n<h1 class=\"title\">{title}</h1>\n{html}\n",
        title = escape(title),
        site = escape(site),
    ))
}

/// The address at which a translation service shows `page` in `language`.
/// The service fetches the page itself: it is told the address, and a page
/// that needs the person's sign-in is not one it can read.
pub(crate) fn translation_address(page: &str, language: &str) -> Option<String> {
    let page = url::Url::parse(page).ok()?;
    if !matches!(page.scheme(), "http" | "https") {
        return None;
    }
    let mut address = url::Url::parse("https://translate.google.com/translate").ok()?;
    address
        .query_pairs_mut()
        .append_pair("sl", "auto")
        .append_pair("tl", language)
        .append_pair("u", page.as_str());
    Some(address.into())
}

/// The language of the desktop, as the two or three letters before the
/// territory in `LC_ALL`, `LC_MESSAGES` or `LANG`; English where none is set.
pub(crate) fn desktop_language(locale: Option<&str>) -> String {
    locale
        .and_then(|locale| locale.split(['_', '.', '@']).next())
        .filter(|language| {
            (2..=3).contains(&language.len()) && language.bytes().all(|b| b.is_ascii_lowercase())
        })
        .unwrap_or("en")
        .to_owned()
}

/// Where the text of a page's source is kept for its tab to show: in the
/// session's runtime folder, which is in memory, is the person's alone and
/// is emptied when they log out.
fn source_folder() -> Option<std::path::PathBuf> {
    Some(std::path::PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?).join("enzim-browser/source"))
}

fn write_source(name: &str, source: &str) -> Result<String, String> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    let folder = source_folder().ok_or("No runtime folder for this session.")?;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&folder)
        .map_err(|error| error.to_string())?;
    let path = folder.join(name);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
        .map_err(|error| error.to_string())?;
    std::io::Write::write_all(&mut file, source.as_bytes()).map_err(|error| error.to_string())?;
    url::Url::from_file_path(&path)
        .map(String::from)
        .map_err(|()| "The source has no address.".to_owned())
}

/// A file name for the source of the page at `address`: its host and a
/// number, so that two sources never share a file.
fn source_name(address: &str, number: u64) -> String {
    let host = url::Url::parse(address)
        .ok()
        .and_then(|address| address.host_str().map(str::to_owned))
        .unwrap_or_else(|| "page".into());
    let host: String = host
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
        .take(60)
        .collect();
    format!("source-of-{host}-{number}.txt")
}

/// The address a page's message names, if it is a link to another
/// application that the browser hands on.
pub(crate) fn external_link(json: &str) -> Option<String> {
    let link: String = serde_json::from_str(json).ok()?;
    let parsed = url::Url::parse(&link).ok()?;
    (link.len() <= 2048 && EXTERNAL_SCHEMES.contains(&parsed.scheme())).then_some(link)
}

fn open_external(link: String) -> Result<(), String> {
    use enzui_core::portal::open_uri::{OpenUri, OpenUriRequest};
    let uri = OpenUri::parse(link).map_err(|error| format!("{error:?}"))?;
    let mut backend = enzui_core::portal::open_uri::desktop_launcher::host_open_uri_backend();
    let mut pending = OpenUriRequest::new(uri, enzui_core::portal::parent::PortalParent::None)
        .start(backend.as_mut(), |_| false)
        .map_err(|error| format!("{error:?}"))?;
    let end = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while std::time::Instant::now() < end {
        if let Some(result) = pending.poll(backend.as_mut()) {
            return if result.is_completed() {
                Ok(())
            } else {
                Err(format!("{result:?}"))
            };
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    Err("the desktop did not answer".into())
}

fn on_thread<T: Send + 'static>(
    name: &str,
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
    done: impl Fn(Result<T, String>) -> ActionCommand + Send + 'static,
) -> Task<Message> {
    let name = name.to_owned();
    Task::perform(
        async move {
            let (send, receive) = iced::futures::channel::oneshot::channel();
            let started = std::thread::Builder::new().name(name).spawn(move || {
                let _ = send.send(work());
            });
            match started {
                Ok(_) => receive
                    .await
                    .unwrap_or_else(|_| Err("the work was dropped".into())),
                Err(error) => Err(error.to_string()),
            }
        },
        move |result| c(Command::Action(done(result))),
    )
}

impl App {
    /// Sends an action to a tab's view, on screen or not. `false` where the
    /// tab has no page loaded to take it.
    pub(super) fn tab_action(&mut self, index: usize, action: BrowserAction) -> bool {
        if index == self.browser.saved.active {
            let loaded = self.webview.borrow().native.is_some();
            if loaded {
                self.submit_browser_action(action);
            }
            return loaded;
        }
        let Some(Some(tab)) = self.browser.sleeping.get(index) else {
            return false;
        };
        let host = tab.host.borrow();
        match (&host.native, host.live_view) {
            (Some(page), Some(view)) => page.browse(&host.router, view, action).is_ok(),
            _ => false,
        }
    }

    /// A tab's page started or stopped sounding.
    pub(super) fn tab_sounds(&mut self, key: u64, sounding: bool) {
        let changed = if sounding {
            self.browser.audible.insert(key)
        } else {
            self.browser.audible.remove(&key)
        };
        if changed {
            self.browser.chrome_revision = self.browser.chrome_revision.wrapping_add(1);
        }
        // A page loaded again starts unmuted in the engine; the person's
        // choice for the tab is told to it again.
        if sounding
            && self.browser.muted.contains(&key)
            && let Some(index) = self.browser.saved.tabs.iter().position(|tab| tab.key == key)
        {
            self.tab_action(index, BrowserAction::Mute(true));
        }
    }

    /// Opens an address in a new tab and stays on the tab in view.
    ///
    /// The tab in view is never swapped out, not even for a frame: the new
    /// tab takes a prepared page, hidden, and is sent to the address there.
    /// Without a prepared page it loads when it is first shown, as a
    /// restored tab does. Staying put also keeps the hovered link, so the
    /// next middle-click on it opens in the background too.
    fn visit_background(&mut self, url: String) {
        let container = self.browser.saved.tabs[self.browser.saved.active].container;
        let index = self.browser.saved.tabs.len();
        let key = next_tab_key();
        self.browser.mark_tab_entrance(key);
        self.browser.saved.tabs.push(TabInfo {
            key,
            favicon: None,
            title: InternalPage::from_url(&url)
                .map_or_else(|| site_name(&url), |page| page.title().to_owned()),
            url: url.clone(),
            pinned: false,
            group: None,
            container,
        });
        let prepared = if url.is_empty() || InternalPage::from_url(&url).is_some() {
            None
        } else {
            self.take_prepared_tab(container)
        };
        self.browser
            .sleeping
            .push(prepared.map(|host| SleepingTab { host, pdf: None }));
        self.tab_action(index, BrowserAction::Navigate(url));
    }

    pub(super) fn close_tabs(&mut self, doomed: impl Fn(usize) -> bool) -> Task<Message> {
        let mut tasks = Vec::new();
        // From the end, so that the indices still to come stay valid.
        for index in (0..self.browser.saved.tabs.len()).rev() {
            // Closing leaves a pinned tab where it is.
            if doomed(index) {
                tasks.push(self.browser_command(Command::Close(index)));
            }
        }
        Task::batch(tasks)
    }

    pub(super) fn action_command(&mut self, command: ActionCommand) -> Task<Message> {
        use ActionCommand::*;
        // What the person chose closes the menu it was chosen from; what
        // arrives later on its own leaves the panels alone.
        if !matches!(
            command,
            Saved(_)
                | External(_)
                | ExternalOpened(_)
                | Selection(_)
                | Source(_)
                | Readable(_)
        ) {
            self.browser.panel = Panel::None;
        }
        match command {
            Mute(index) => {
                let Some(key) = self.browser.saved.tabs.get(index).map(|tab| tab.key) else {
                    return Task::none();
                };
                let muted = !self.browser.muted.remove(&key);
                if muted {
                    self.browser.muted.insert(key);
                }
                self.tab_action(index, BrowserAction::Mute(muted));
            }
            ReloadTab(index) => {
                if index < self.browser.saved.tabs.len() {
                    self.tab_action(index, BrowserAction::Reload);
                }
            }
            CloseOthers(keep) => return self.close_tabs(|index| index != keep),
            CloseAfter(last) => return self.close_tabs(|index| index > last),
            MoveToWindow(index) => {
                let Some(tab) = self.browser.saved.tabs.get(index) else {
                    return Task::none();
                };
                if self.browser.saved.tabs.len() < 2 || tab.pinned {
                    return Task::none();
                }
                let url = tab.url.clone();
                // With its page as it is where it has one; otherwise its
                // address is opened there.
                if let Some(moved) = self.detach_tab(index) {
                    self.moved_out = Some(moved);
                    return Task::none();
                }
                self.new_window = Some((self.browser.private, Some(url)));
                return self.browser_command(Command::Close(index));
            }
            OpenWindow(url, private) => {
                self.new_window = Some((private || self.browser.private, Some(url)));
            }
            VisitBackground(url) => self.visit_background(url),
            SelectNth(nth) => {
                let last = self.browser.saved.tabs.len() - 1;
                self.select_tab(nth.min(last));
            }
            HardReload => self.submit_browser_action(BrowserAction::ReloadFresh),
            Save(url) => {
                let (job, receiver) = crate::native_jobs::Job::new();
                self.submit_browser_action(BrowserAction::DownloadUrl { url, job });
                return Task::perform(crate::native_jobs::receive(receiver), |result| {
                    c(Command::Action(Saved(result.map(|_| ()))))
                });
            }
            Saved(Err(error)) => self.browser.toasts.error("download", error),
            Saved(Ok(())) | ExternalOpened(Ok(())) => {}
            External(link) => {
                return on_thread("open-link", move || open_external(link), ExternalOpened);
            }
            ExternalOpened(Err(error)) => self.browser.toasts.error(
                "external-link",
                format!("No application took this link ({error})."),
            ),
            SearchSelection => {
                let (job, receiver) = crate::native_jobs::Job::new();
                self.submit_browser_action(BrowserAction::Evaluate {
                    source: "String(getSelection()).slice(0, 2000)".into(),
                    world: enzui_webkit::page::ScriptWorld::Isolated(
                        "enzim-browser-chrome".into(),
                    ),
                    job,
                });
                return Task::perform(crate::native_jobs::receive(receiver), |result| {
                    c(Command::Action(Selection(result.and_then(
                        |payload| match payload {
                            crate::native_jobs::Payload::Json(serde_json::Value::String(text)) => {
                                Ok(text)
                            }
                            _ => Err("The page gave no selection.".into()),
                        },
                    ))))
                });
            }
            Selection(Ok(text)) => {
                let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
                if !text.is_empty() {
                    self.new_tab(self.browser.saved.search.search_url(&text));
                }
            }
            Selection(Err(error)) => self.browser.toasts.error("search", error),
            Translate => {
                let language = desktop_language(
                    ["LC_ALL", "LC_MESSAGES", "LANG"]
                        .iter()
                        .find_map(|name| std::env::var(name).ok().filter(|v| !v.is_empty()))
                        .as_deref(),
                );
                let page = self.browser.saved.tabs[self.browser.saved.active].url.clone();
                match translation_address(&page, &language) {
                    Some(address) => self.new_tab(address),
                    None => self
                        .browser
                        .toasts
                        .error("translate", "Only a website can be translated."),
                }
            }
            ViewSource => {
                let (job, receiver) = crate::native_jobs::Job::new();
                self.submit_browser_action(BrowserAction::ExportHtml { job });
                return Task::perform(crate::native_jobs::receive(receiver), |result| {
                    c(Command::Action(Source(result.and_then(
                        |payload| match payload {
                            crate::native_jobs::Payload::Json(serde_json::Value::String(html))
                                if html.len() <= 64 * 1024 * 1024 =>
                            {
                                Ok(html)
                            }
                            crate::native_jobs::Payload::Bytes(bytes)
                                if bytes.len() <= 64 * 1024 * 1024 =>
                            {
                                Ok(String::from_utf8_lossy(&bytes).into_owned())
                            }
                            _ => Err("The page gave no source.".into()),
                        },
                    ))))
                });
            }
            Source(Ok(source)) => {
                let name = source_name(
                    &self.browser.saved.tabs[self.browser.saved.active].url,
                    next_tab_key(),
                );
                match write_source(&name, &source) {
                    Ok(address) => self.new_tab(address),
                    Err(error) => self.browser.toasts.error("source", error),
                }
            }
            Source(Err(error)) => self.browser.toasts.error("source", error),
            Reader => {
                let (job, receiver) = crate::native_jobs::Job::new();
                self.submit_browser_action(BrowserAction::Evaluate {
                    source: include_str!("reader.js").into(),
                    world: enzui_webkit::page::ScriptWorld::Isolated(
                        "enzim-browser-chrome".into(),
                    ),
                    job,
                });
                return Task::perform(crate::native_jobs::receive(receiver), |result| {
                    c(Command::Action(Readable(result.and_then(
                        |payload| match payload {
                            crate::native_jobs::Payload::Json(serde_json::Value::String(text)) => {
                                Ok(text)
                            }
                            _ => Err("This page has nothing a reader view could show.".into()),
                        },
                    ))))
                });
            }
            Readable(Ok(extracted)) => {
                let Some(page) = reader_page(&extracted) else {
                    self.browser
                        .toasts
                        .error("reader", "This page has nothing a reader view could show.");
                    return Task::none();
                };
                let name = source_name(
                    &self.browser.saved.tabs[self.browser.saved.active].url,
                    next_tab_key(),
                )
                .replacen("source-of-", "reading-", 1)
                .replace(".txt", ".html");
                match write_source(&name, &page) {
                    Ok(address) => self.new_tab(address),
                    Err(error) => self.browser.toasts.error("reader", error),
                }
            }
            Readable(Err(error)) => self.browser.toasts.error("reader", error),
            Cut => {
                // The copy reads the selection in the page's process before
                // the removal that follows it there.
                let task = self.update(Message::WebViewNativeCopy);
                self.submit_browser_action(BrowserAction::Script(
                    "document.execCommand('delete')".into(),
                ));
                return task;
            }
        }
        Task::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tabs(count: usize) -> App {
        let mut app = browser_tests::app();
        app.browser_mode = true;
        app.browser.saved.tabs[0].url = "https://tab0.example/".into();
        for index in 1..count {
            let mut tab = app.browser.saved.tabs[0].clone();
            tab.key = next_tab_key();
            tab.url = format!("https://tab{index}.example/");
            app.browser.saved.tabs.push(tab);
            app.browser.sleeping.push(None);
        }
        app
    }

    fn addresses(app: &App) -> Vec<&str> {
        app.browser
            .saved
            .tabs
            .iter()
            .map(|tab| tab.url.as_str())
            .collect()
    }

    #[test]
    fn only_links_to_mail_and_phone_are_handed_to_the_desktop() {
        assert_eq!(
            external_link(r#""mailto:someone@example.com?subject=Hi""#).as_deref(),
            Some("mailto:someone@example.com?subject=Hi")
        );
        assert!(external_link(r#""tel:+3531234567""#).is_some());
        for refused in [
            r#""https://example.com/""#,
            r#""file:///etc/passwd""#,
            r#""javascript:alert(1)""#,
            r#""ms-msdt:/id""#,
            r#"{"href":"mailto:a@b.c"}"#,
            "mailto:a@b.c",
        ] {
            assert_eq!(external_link(refused), None, "{refused}");
        }
        let long = format!(r#""mailto:{}@example.com""#, "a".repeat(3000));
        assert_eq!(external_link(&long), None);
    }

    #[test]
    fn a_page_is_translated_by_its_address_into_the_desktops_language() {
        assert_eq!(desktop_language(Some("hu_HU.UTF-8")), "hu");
        assert_eq!(desktop_language(Some("de")), "de");
        assert_eq!(desktop_language(Some("C")), "en");
        assert_eq!(desktop_language(Some("POSIX")), "en");
        assert_eq!(desktop_language(None), "en");
        assert_eq!(
            translation_address("https://example.org/a b?q=1&r=2#top", "hu").as_deref(),
            Some(
                "https://translate.google.com/translate?sl=auto&tl=hu&u=https%3A%2F%2Fexample.org%2Fa%2520b%3Fq%3D1%26r%3D2%23top"
            )
        );
        assert_eq!(translation_address("file:///home/me/notes.html", "hu"), None);
        assert_eq!(translation_address("enzim://settings", "hu"), None);
    }

    #[test]
    fn a_reader_view_is_the_articles_text_under_an_escaped_title() {
        let page = reader_page(
            r#"{"title":"Tom & <Jerry>","site":"news.example","html":"<p>One <a href=\"https://a.example/\">link</a>.</p>"}"#,
        )
        .expect("a page");
        assert!(page.contains("<title>Tom &amp; &lt;Jerry&gt;</title>"));
        assert!(page.contains("<h1 class=\"title\">Tom &amp; &lt;Jerry&gt;</h1>"));
        assert!(page.contains("<p class=\"site\">news.example</p>"));
        assert!(page.contains("<p>One <a href=\"https://a.example/\">link</a>.</p>"));
        assert!(!page.contains("<script"));
        // What the script could not read is no page.
        assert_eq!(reader_page(r#"{"error":"nothing to read"}"#), None);
        assert_eq!(reader_page("not json"), None);
    }

    #[test]
    fn a_pages_source_is_named_after_its_host_and_nothing_else_of_its_address() {
        assert_eq!(
            source_name("https://example.org/private/path?token=secret", 7),
            "source-of-example.org-7.txt"
        );
        assert_eq!(source_name("not an address", 8), "source-of-page-8.txt");
        assert_eq!(
            source_name("https://xn--bcher-kva.example/../../etc", 9),
            "source-of-xn--bcher-kva.example-9.txt"
        );
    }

    #[test]
    fn closing_the_other_tabs_keeps_the_chosen_and_the_pinned() {
        let mut app = tabs(5);
        app.browser.saved.tabs[0].pinned = true;
        let _ = app.update(c(Command::Action(ActionCommand::CloseOthers(2))));
        assert_eq!(
            addresses(&app),
            ["https://tab0.example/", "https://tab2.example/"]
        );

        let mut app = tabs(5);
        app.browser.saved.tabs[4].pinned = true;
        let _ = app.update(c(Command::Action(ActionCommand::CloseAfter(1))));
        assert_eq!(
            addresses(&app),
            [
                "https://tab0.example/",
                "https://tab1.example/",
                "https://tab4.example/"
            ]
        );
    }

    #[test]
    fn a_link_opened_in_the_background_leaves_the_tab_in_view() {
        let mut app = tabs(2);
        app.address = "https://tab0.example/".into();
        let _ = app.update(c(Command::Action(ActionCommand::VisitBackground(
            "https://linked.example/".into(),
        ))));
        assert_eq!(app.browser.saved.active, 0);
        assert_eq!(
            addresses(&app),
            [
                "https://tab0.example/",
                "https://tab1.example/",
                "https://linked.example/"
            ]
        );
        assert_eq!(app.address, "https://tab0.example/");
    }

    #[test]
    fn the_number_keys_choose_tabs_and_nine_the_last() {
        let mut app = tabs(4);
        let _ = app.update(c(Command::Action(ActionCommand::SelectNth(2))));
        assert_eq!(app.browser.saved.active, 2);
        let _ = app.update(c(Command::Action(ActionCommand::SelectNth(LAST_TAB))));
        assert_eq!(app.browser.saved.active, 3);
        // A number past the tabs there are is the last one too.
        let _ = app.update(c(Command::Action(ActionCommand::SelectNth(0))));
        let _ = app.update(c(Command::Action(ActionCommand::SelectNth(7))));
        assert_eq!(app.browser.saved.active, 3);
    }

    #[test]
    fn a_tab_is_muted_by_its_own_key_and_moved_out_with_its_address() {
        let mut app = tabs(3);
        let key = app.browser.saved.tabs[1].key;
        let _ = app.update(c(Command::Action(ActionCommand::Mute(1))));
        assert!(app.browser.muted.contains(&key));
        app.tab_sounds(key, true);
        assert!(app.browser.audible.contains(&key));
        // The tab takes a new key when it is sent to another document; it
        // is the same tab and stays muted.
        let next = next_tab_key();
        app.browser.replace_tab_key(1, next);
        assert!(app.browser.muted.contains(&next) && app.browser.audible.contains(&next));
        assert!(!app.browser.muted.contains(&key));
        let _ = app.update(c(Command::Action(ActionCommand::Mute(1))));
        assert!(!app.browser.muted.contains(&next));

        let _ = app.update(c(Command::Action(ActionCommand::MoveToWindow(1))));
        assert_eq!(
            app.new_window,
            Some((false, Some("https://tab1.example/".into())))
        );
        assert_eq!(
            addresses(&app),
            ["https://tab0.example/", "https://tab2.example/"]
        );
    }
}
