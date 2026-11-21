//! The whole screen, for a page that asks for it (a video, a game) or for
//! the person who does (F11).
//!
//! The engine tells a page it is fullscreen and lays its element out across
//! the view; the window and what is drawn around the view are the browser's.
//! So while either asked, the window takes the screen and a website is shown
//! bare, without the bars and the tabs. A page's request belongs to its tab:
//! turning to another tab ends it.

use super::*;

/// Runs in the page to end the fullscreen it asked for.
const LEAVE: &str =
    "(document.exitFullscreen||document.webkitExitFullscreen||function(){}).call(document)";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Fullscreen {
    /// The person asked: until they ask again.
    pub(crate) window: bool,
    /// A page asked: the tab it is in.
    pub(crate) page: Option<u64>,
}

impl Fullscreen {
    pub(crate) fn on(self) -> bool {
        self.window || self.page.is_some()
    }
}

impl App {
    fn window_mode(&self) -> Task<Message> {
        let mode = if self.browser.fullscreen.on() {
            iced::window::Mode::Fullscreen
        } else {
            iced::window::Mode::Windowed
        };
        match self.window_id {
            Some(id) => iced::window::set_mode(id, mode),
            None => iced::window::latest().and_then(move |id| iced::window::set_mode(id, mode)),
        }
    }

    /// The page of the tab on screen entered or left fullscreen.
    pub(super) fn page_fullscreen(&mut self, entered: bool) {
        let was = self.browser.fullscreen.on();
        let tab = self.browser.saved.tabs[self.browser.saved.active].key;
        if entered {
            self.browser.fullscreen.page = Some(tab);
            if !was {
                self.browser.toasts.done("Press Esc to leave full screen");
            }
        } else if self.browser.fullscreen.page == Some(tab) {
            self.browser.fullscreen.page = None;
        }
        if was != self.browser.fullscreen.on() {
            let task = self.window_mode();
            self.pending_tasks.push(task);
        }
    }

    /// F11.
    pub(super) fn fullscreen_toggle(&mut self) -> Task<Message> {
        if self.browser.fullscreen.on() {
            self.browser.fullscreen.window = false;
            self.leave_page_fullscreen();
        } else {
            self.browser.fullscreen.window = true;
            self.browser.toasts.done("Press F11 to leave full screen");
        }
        self.window_mode()
    }

    /// Escape: ends a page's fullscreen. `None` where there was none to end.
    pub(super) fn fullscreen_escape(&mut self) -> Option<Task<Message>> {
        self.browser.fullscreen.page?;
        self.leave_page_fullscreen();
        Some(self.window_mode())
    }

    /// Tells the page that asked that its fullscreen is over. The page's own
    /// report of leaving finds nothing left to do.
    fn leave_page_fullscreen(&mut self) {
        let Some(owner) = self.browser.fullscreen.page.take() else {
            return;
        };
        let action = BrowserAction::Script(LEAVE.into());
        if self.browser.saved.tabs[self.browser.saved.active].key == owner {
            self.submit_browser_action(action);
        } else if let Some(tab) = self
            .browser
            .saved
            .tabs
            .iter()
            .position(|tab| tab.key == owner)
            .and_then(|index| self.browser.sleeping.get(index))
            .and_then(Option::as_ref)
        {
            let host = tab.host.borrow();
            if let (Some(page), Some(view)) = (&host.native, host.live_view) {
                let _ = page.browse(&host.router, view, action);
            }
        }
    }

    /// A page's fullscreen ends when its tab is no longer the one on screen.
    /// Runs after every message.
    pub(crate) fn settle_fullscreen(&mut self) {
        let Some(owner) = self.browser.fullscreen.page else {
            return;
        };
        let active = self
            .browser
            .saved
            .tabs
            .get(self.browser.saved.active)
            .map(|tab| tab.key);
        if active == Some(owner) && self.browser.library.is_none() {
            return;
        }
        self.leave_page_fullscreen();
        let task = self.window_mode();
        self.pending_tasks.push(task);
    }

    /// Whether the website is shown alone. The bars stay for the browser's
    /// own pages, which are reached through them, and come back for a panel
    /// that hangs from one of their buttons.
    pub(crate) fn fullscreen_bare(&self) -> bool {
        self.browser.fullscreen.on()
            && self.browser.library.is_none()
            && self.browser.pdf.is_none()
            && !self.browser.devtools.open
            && InternalPage::from_url(&self.browser.saved.tabs[self.browser.saved.active].url)
                .is_none()
            && self.webview.borrow().native_error.is_none()
            && self.webview.borrow().live_view.is_some()
            && matches!(
                self.browser.panel,
                Panel::None | Panel::Script | Panel::SignIn | Panel::Options
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_tabs() -> App {
        let mut app = browser_tests::app();
        app.browser_mode = true;
        let mut tab = app.browser.saved.tabs[0].clone();
        tab.key = next_tab_key();
        app.browser.saved.tabs.push(tab);
        app.browser.sleeping.push(Some(SleepingTab {
            host: RefCell::new(WebViewService::new()),
            pdf: None,
        }));
        for tab in &mut app.browser.saved.tabs {
            tab.url = "https://video.example/".into();
        }
        app
    }

    #[test]
    fn a_pages_fullscreen_takes_the_window_and_ends_with_its_tab() {
        let mut app = two_tabs();
        let first = app.browser.saved.tabs[0].key;
        app.webview
            .borrow_mut()
            .page_events
            .push(enzui_webkit::page::PageEvent::Fullscreen(true));
        app.sync_browser();
        assert_eq!(app.browser.fullscreen.page, Some(first));
        assert!(app.browser.fullscreen.on());
        // The frame is not rounded where the window is the screen.
        assert_eq!(app.frame_cut(), 0.0);

        // Another tab is not the page that asked.
        let _ = app.update(c(Command::Cycle(false)));
        assert_eq!(app.browser.saved.active, 1);
        assert!(!app.browser.fullscreen.on());

        // The person's own request outlasts a page's.
        let _ = app.update(c(Command::Fullscreen));
        assert!(app.browser.fullscreen.window);
        app.webview
            .borrow_mut()
            .page_events
            .extend([true, false].map(enzui_webkit::page::PageEvent::Fullscreen));
        app.sync_browser();
        assert_eq!(app.browser.fullscreen.page, None);
        assert!(app.browser.fullscreen.on());
        // Escape ends a page's fullscreen only.
        let _ = app.update(c(Command::Escape));
        assert!(app.browser.fullscreen.on());
        let _ = app.update(c(Command::Fullscreen));
        assert!(!app.browser.fullscreen.on());
    }

    #[test]
    fn escape_ends_the_fullscreen_a_page_asked_for() {
        let mut app = two_tabs();
        app.webview
            .borrow_mut()
            .page_events
            .push(enzui_webkit::page::PageEvent::Fullscreen(true));
        app.sync_browser();
        assert!(app.browser.fullscreen.on());
        let _ = app.update(c(Command::Escape));
        assert!(!app.browser.fullscreen.on());
    }
}
