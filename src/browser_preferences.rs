//! Choices that follow a site or the profile: the zoom a site is read at and
//! the folder downloads are saved to.

use super::*;

/// The chooser's tag for the download folder.
pub(super) const FOLDER_TAG: u64 = u64::MAX - 6;

#[derive(Debug, Clone)]
pub(crate) enum PreferenceCommand {
    /// Ask for the folder downloads go to.
    ChooseDownloadFolder,
    /// Back to `Downloads`.
    DefaultDownloadFolder,
    /// Whether each download asks where it is saved.
    AskDownloadPlace(bool),
}

/// What a zoom is remembered for: the site's host, with its port where it
/// has one. `None` for anything that is not a website.
pub(crate) fn zoom_site(address: &str) -> Option<String> {
    let address = url::Url::parse(address).ok()?;
    if !matches!(address.scheme(), "http" | "https") {
        return None;
    }
    let host = address.host_str()?.to_ascii_lowercase();
    Some(match address.port() {
        Some(port) => format!("{host}:{port}"),
        None => host,
    })
}

impl App {
    /// The zoom chosen for the site of `address`; 100 where none was.
    pub(crate) fn site_zoom(&self, address: &str) -> i32 {
        zoom_site(address)
            .and_then(|site| self.browser.saved.site_zoom.get(&site).copied())
            .filter(|percent| (25..=300).contains(percent))
            .unwrap_or(100)
    }

    /// Remembers the zoom the person set for the site on show. 100 is no
    /// choice and is not kept.
    pub(super) fn remember_site_zoom(&mut self, percent: i32) {
        let Some(site) = zoom_site(&self.browser.saved.tabs[self.browser.saved.active].url) else {
            return;
        };
        let changed = if percent == 100 {
            self.browser.saved.site_zoom.remove(&site).is_some()
        } else {
            self.browser.saved.site_zoom.insert(site.clone(), percent) != Some(percent)
        };
        self.webview.borrow_mut().zoom_site = Some(site);
        if changed && !cfg!(test) {
            self.browser.save();
        }
    }

    /// Brings the page on show to its site's zoom when the site is another
    /// than the one the view was last zoomed for: after a link to another
    /// site, a tab selected, a page restored.
    pub(super) fn follow_site_zoom(&mut self) {
        if self.browser.library.is_some() || self.browser.pdf.is_some() {
            return;
        }
        let address = &self.browser.saved.tabs[self.browser.saved.active].url;
        let Some(site) = zoom_site(address) else {
            return;
        };
        if self.webview.borrow().zoom_site.as_deref() == Some(site.as_str()) {
            return;
        }
        let percent = self.site_zoom(address);
        let mut service = self.webview.borrow_mut();
        service.zoom_site = Some(site);
        if service.zoom_percent == percent {
            return;
        }
        service.zoom_percent = percent;
        drop(service);
        self.browser.zoom = percent;
        self.sync_zoom_model();
        self.browser.chrome_revision = self.browser.chrome_revision.wrapping_add(1);
        self.submit_browser_action(BrowserAction::Zoom(f64::from(percent) / 100.));
    }

    /// Tells the engine where downloads go: every view that is open, and
    /// through [`crate::downloads::set_folder`] every one opened later.
    fn apply_download_folder(&mut self) {
        let folder = self.browser.saved.download_folder.clone();
        crate::downloads::set_folder(folder.as_ref().map(std::path::PathBuf::from));
        let folder = crate::downloads::folder();
        for index in 0..self.browser.saved.tabs.len() {
            self.tab_action(index, BrowserAction::DownloadFolder(folder.clone()));
        }
        if !cfg!(test) {
            self.browser.save();
        }
    }

    pub(super) fn preference_command(&mut self, command: PreferenceCommand) -> Task<Message> {
        match command {
            PreferenceCommand::ChooseDownloadFolder => {
                if let Err(error) = self.browser.chooser.start(
                    (FOLDER_TAG, enzui_webkit::page::PageRequestId::new(0)),
                    enzui_core::portal::backend::PortalOperation::OpenDirectory,
                    enzui_core::portal::chooser::ChooserOptions::new()
                        .title("Choose where downloads are saved"),
                ) {
                    self.browser
                        .toasts
                        .error("downloads", format!("Could not open chooser: {error:?}"));
                }
            }
            PreferenceCommand::DefaultDownloadFolder => {
                self.browser.saved.download_folder = None;
                self.apply_download_folder();
            }
            PreferenceCommand::AskDownloadPlace(ask) => {
                self.browser.saved.ask_download_place = ask;
                if !cfg!(test) {
                    self.browser.save();
                }
            }
        }
        Task::none()
    }

    pub(super) fn download_folder_chosen(&mut self, paths: Vec<std::path::PathBuf>) {
        let Some(folder) = paths.into_iter().next() else {
            return;
        };
        // The engine takes the folder as text and writes only into one that
        // is there.
        match folder.to_str() {
            Some(text) if folder.is_absolute() && folder.is_dir() => {
                self.browser.saved.download_folder = Some(text.to_owned());
                self.apply_download_folder();
            }
            _ => self
                .browser
                .toasts
                .error("downloads", "Downloads cannot be saved to that folder."),
        }
    }

    pub(super) fn download_settings(&self) -> Element<'_, Message> {
        let mut buttons = iced::widget::row![].spacing(6);
        if self.browser.saved.download_folder.is_some() {
            buttons = buttons.push(action(
                "Default",
                "rotate-ccw",
                Command::Preference(PreferenceCommand::DefaultDownloadFolder),
            ));
        }
        buttons = buttons.push(action(
            "Choose…",
            "folder-open",
            Command::Preference(PreferenceCommand::ChooseDownloadFolder),
        ));
        enzui_widgets::preferences_row::enz_preferences_row("Save downloads to")
            .icon("folder-down")
            .subtitle(crate::downloads::folder().display().to_string())
            .control(buttons)
            .accessory_width(220.)
            .into_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sites_zoom_is_kept_by_host_and_port_and_only_for_websites() {
        assert_eq!(
            zoom_site("https://Docs.Example.org/a/b?c#d").as_deref(),
            Some("docs.example.org")
        );
        assert_eq!(
            zoom_site("http://localhost:8080/").as_deref(),
            Some("localhost:8080")
        );
        for other in ["file:///home/me/notes.txt", "about:blank", "enzim://settings", ""] {
            assert_eq!(zoom_site(other), None, "{other}");
        }
    }

    #[test]
    fn a_zoom_set_on_a_site_comes_back_with_the_site() {
        let mut app = browser_tests::app();
        app.browser_mode = true;
        app.browser.saved.tabs[0].url = "https://read.example/long".into();
        let _ = app.update(c(Command::Zoom(50)));
        assert_eq!(app.browser.zoom, 150);
        assert_eq!(app.site_zoom("https://read.example/other"), 150);
        assert_eq!(app.site_zoom("https://else.example/"), 100);

        // The same tab on another site is read at that site's own zoom.
        app.browser.saved.tabs[0].url = "https://else.example/".into();
        app.follow_site_zoom();
        assert_eq!(app.browser.zoom, 100);
        assert_eq!(app.webview.borrow().zoom_percent, 100);
        // And back on the first at the zoom chosen there.
        app.browser.saved.tabs[0].url = "https://read.example/again".into();
        app.follow_site_zoom();
        assert_eq!(app.browser.zoom, 150);

        // 100 is no choice: nothing is kept for the site.
        let _ = app.update(c(Command::Zoom(0)));
        assert!(app.browser.saved.site_zoom.is_empty());
    }
}
