//! A tab moved to another window with its page as it is.
//!
//! A tab's page lives in its own view, which is not tied to a window: the
//! tab is taken out of one window's list and put into another's, and the
//! page neither reloads nor stops playing.

use super::*;

/// A tab on its way to another window.
pub(crate) struct MovedTab {
    pub(crate) tab: TabInfo,
    host: RefCell<WebViewService>,
    pdf: Option<crate::pdf::Page>,
    muted: bool,
    audible: bool,
}

impl App {
    /// Whether tab `index` has a page that can go to another window as it
    /// is. A tab that never loaded has none, a pinned tab stays, and a tab
    /// with a download running keeps it here.
    fn movable(&self, index: usize) -> bool {
        let Some(tab) = self.browser.saved.tabs.get(index) else {
            return false;
        };
        if tab.pinned {
            return false;
        }
        let ready = |host: &RefCell<WebViewService>| {
            let host = host.borrow();
            host.native.is_some()
                && host
                    .live_view
                    .is_none_or(|view| !self.browser.downloads.owns_active(view))
        };
        if index == self.browser.saved.active {
            self.browser.library.is_none() && ready(&self.webview)
        } else {
            self.browser.sleeping[index]
                .as_ref()
                .is_some_and(|tab| ready(&tab.host))
        }
    }

    /// Takes tab `index` out of this window with its page. `None`, and
    /// nothing changed, where [`Self::movable`] says no. The window keeps
    /// a tab: its last one leaves a new tab behind.
    pub(super) fn detach_tab(&mut self, index: usize) -> Option<MovedTab> {
        if !self.movable(index) {
            return None;
        }
        if self.browser.saved.tabs.len() == 1 {
            self.browser.saved.tabs.push(TabInfo {
                key: next_tab_key(),
                favicon: None,
                title: "New tab".into(),
                url: String::new(),
                pinned: false,
                group: None,
                container: self.browser.saved.tabs[index].container,
            });
            self.browser.sleeping.push(None);
        }
        if index == self.browser.saved.active {
            self.select_tab(if index == 0 { 1 } else { index - 1 });
        }
        let SleepingTab { host, pdf } = self.browser.sleeping.remove(index)?;
        let mut tab = self.browser.saved.tabs.remove(index);
        if self.browser.saved.active > index {
            self.browser.saved.active -= 1;
        }
        // Groups are a window's own.
        tab.group = None;
        self.browser.pdf_requests.remove(&tab.key);
        self.browser.popup_openers.remove(&tab.key);
        self.browser.panel = Panel::None;
        Some(MovedTab {
            muted: self.browser.muted.remove(&tab.key),
            audible: self.browser.audible.remove(&tab.key),
            tab,
            host,
            pdf,
        })
    }

    /// Adds a tab that came from another window, and turns to it.
    pub(crate) fn attach_tab(&mut self, moved: MovedTab) {
        let MovedTab {
            tab,
            host,
            pdf,
            muted,
            audible,
        } = moved;
        if muted {
            self.browser.muted.insert(tab.key);
        }
        if audible {
            self.browser.audible.insert(tab.key);
        }
        let index = self.browser.saved.tabs.len();
        self.browser.saved.tabs.push(tab);
        self.browser.sleeping.push(Some(SleepingTab { host, pdf }));
        self.select_tab(index);
    }

    /// For a window just made: the tab it came with gives way to `moved`.
    pub(crate) fn adopt_only_tab(&mut self, moved: MovedTab) {
        self.attach_tab(moved);
        self.browser.saved.tabs.remove(0);
        self.browser.sleeping.remove(0);
        self.browser.saved.active = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An app whose tabs each have a page, as far as moving them can tell.
    fn app_with_pages(count: usize) -> App {
        let mut app = browser_tests::app();
        app.browser_mode = true;
        app.browser.saved.tabs[0].url = "https://tab0.example/".into();
        for index in 1..count {
            let _ = app.update(c(Command::Action(ActionCommand::VisitBackground(format!(
                "https://tab{index}.example/"
            )))));
        }
        app
    }

    #[test]
    fn a_tab_without_a_page_of_its_own_is_not_moved() {
        let mut app = app_with_pages(3);
        // No tab was ever loaded: there is no page to take along.
        assert!(app.detach_tab(1).is_none());
        assert!(app.detach_tab(0).is_none());
        assert!(app.detach_tab(9).is_none());
        assert_eq!(app.browser.saved.tabs.len(), 3);
        assert_eq!(app.browser.saved.active, 0);
    }

    #[test]
    fn a_tab_that_arrives_keeps_its_name_and_its_silence() {
        let mut app = app_with_pages(2);
        let tab = TabInfo {
            key: next_tab_key(),
            favicon: None,
            url: "https://video.example/watch".into(),
            title: "A film".into(),
            pinned: false,
            group: Some(4),
            container: None,
        };
        app.attach_tab(MovedTab {
            tab,
            host: RefCell::new(WebViewService::new()),
            pdf: None,
            muted: true,
            audible: true,
        });
        assert_eq!(app.browser.saved.active, 2);
        // A tab without a page is sent to its address on arrival, which
        // gives it a new key; what is known of it goes with the key.
        let key = app.browser.saved.tabs[2].key;
        assert_eq!(app.browser.saved.tabs[2].title, "A film");
        assert_eq!(app.address, "https://video.example/watch");
        assert!(app.browser.muted.contains(&key));
        assert!(app.browser.audible.contains(&key));

        // In a window made for it, it is the only tab.
        let mut fresh = browser_tests::app();
        fresh.browser_mode = true;
        let tab = app.browser.saved.tabs[2].clone();
        fresh.adopt_only_tab(MovedTab {
            tab,
            host: RefCell::new(WebViewService::new()),
            pdf: None,
            muted: false,
            audible: false,
        });
        assert_eq!(fresh.browser.saved.tabs.len(), 1);
        assert_eq!(fresh.browser.sleeping.len(), 1);
        assert_eq!(fresh.browser.saved.active, 0);
        assert_eq!(fresh.browser.saved.tabs[0].title, "A film");
    }
}
