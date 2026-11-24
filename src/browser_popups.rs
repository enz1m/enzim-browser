//! Windows that pages open (`window.open`, a link with a target). Each is a
//! real view related to its opener, so the two can talk: a sign-in window
//! hands its answer back. A window the page asked for as a popup (a size, or
//! bars turned off, as sign-in windows do) opens as a small window of its own
//! at that size; any other becomes a tab. Either loads nothing before it has
//! its tab's filters, scripts and policy.

use super::*;

/// A popup on its way to a window of its own.
pub(crate) struct PopupWindow {
    /// The tab the window shows.
    pub(crate) tab: TabInfo,
    /// The address the page opened it for, loaded once the window has it.
    pub(crate) target: String,
    /// The engine's view, kept by the opener's worker until it is taken.
    pub(crate) popup: u64,
    /// The size of page area the page asked for, in CSS pixels.
    pub(crate) size: Option<(u32, u32)>,
}

/// The window size for `page` (the page area asked for): the page area plus
/// the popup's bar and frame, kept on a screen.
pub(crate) fn popup_window_size(page: Option<(u32, u32)>) -> iced::Size {
    #[allow(clippy::cast_precision_loss, reason = "window sizes are small")]
    let (width, height) = page.map_or((520.0, 640.0), |(w, h)| (w as f32, h as f32));
    // The bar above and the frame's margin on three sides.
    let (chrome_width, chrome_height) = (2.0 * 10.0, POPUP_BAR_HEIGHT + 10.0);
    iced::Size::new(
        (width + chrome_width).clamp(320.0, 1600.0),
        (height + chrome_height).clamp(260.0, 1200.0),
    )
}

/// What a window a page opened shows before its first document commits.
const POPUP_INITIAL_URL: &str = "about:blank";

/// The height of a popup window's bar.
pub(crate) const POPUP_BAR_HEIGHT: f32 = 44.0;

impl App {
    /// The page of tab `opener` opened a window, which that tab's worker
    /// keeps under `popup`.
    pub(super) fn popup_opened(&mut self, opener: u64, popup: u64, url: String) {
        let tabs = &self.browser.saved.tabs;
        // An opener closed meanwhile takes its window with it: nothing asks
        // for it, and the worker closes what no tab took.
        let Some(container) = tabs.iter().find(|tab| tab.key == opener).map(|tab| tab.container)
        else {
            return;
        };
        let features = crate::webview_native::take_popup_features(popup);
        // Until its first navigation commits, the window shows a blank document
        // its opener can write to, so it is not yet `url`.
        let shown = String::from(POPUP_INITIAL_URL);
        // A popup window has no tabs to put another in: what it opens is a
        // popup window too.
        if (features.is_some_and(|features| features.popup) || self.popup_window)
            && self.popup_out.is_none()
        {
            self.popup_out = Some(PopupWindow {
                tab: TabInfo {
                    key: next_tab_key(),
                    favicon: None,
                    title: site_name(&shown),
                    url: shown,
                    pinned: false,
                    group: None,
                    container,
                },
                target: url,
                popup,
                size: features.and_then(|features| features.size),
            });
            return;
        }
        let live: std::collections::BTreeSet<u64> = tabs.iter().map(|tab| tab.key).collect();
        self.browser
            .popup_openers
            .retain(|tab, _| live.contains(tab));
        let key = next_tab_key();
        self.browser.mark_tab_entrance(key);
        let index = self.browser.saved.tabs.len();
        self.browser.saved.tabs.push(TabInfo {
            key,
            favicon: None,
            title: site_name(&shown),
            url: shown,
            pinned: false,
            group: None,
            container,
        });
        self.discard_tab_reservation();
        let mut host = WebViewService::new();
        host.adopt = Some(popup);
        self.browser.sleeping.push(Some(SleepingTab {
            host: RefCell::new(host),
            pdf: None,
        }));
        self.browser.popup_openers.insert(key, opener);
        self.select_tab(index);
        self.submit_browser_action(BrowserAction::ResumePopup(url));
    }

    /// For a window just made around a popup: its only tab is the popup,
    /// which loads what it was opened for.
    pub(crate) fn take_popup_window(&mut self, window: PopupWindow) {
        let PopupWindow {
            tab, target, popup, ..
        } = window;
        self.popup_window = true;
        self.browser.library = None;
        self.browser.saved.tabs = vec![tab];
        self.browser.sleeping = vec![None];
        self.browser.saved.active = 0;
        {
            let mut host = self.webview.borrow_mut();
            host.adopt = Some(popup);
            // Shown, as a selected tab is: a page that thinks itself hidden
            // has its timers held back, and a sign-in page that closes itself
            // after a moment would not.
            host.show_view();
        }
        self.address.clone_from(&self.browser.saved.tabs[0].url);
        self.submit_browser_action(BrowserAction::ResumePopup(target));
    }

    /// The bar of a popup window: who the page is from, where it is, and the
    /// window's controls. A sign-in window shows its address so that the
    /// person can see which site asks for their password; it cannot be
    /// edited, as in other browsers' popups.
    pub(super) fn popup_bar(&self) -> Element<'_, Message> {
        use iced::widget::{container, row, text};
        let p = &self.look().palette;
        let url = &self.browser.saved.tabs[self.browser.saved.active].url;
        // The origin is what the person must read; the rest is detail.
        let (origin, rest) = url::Url::parse(url).ok().map_or_else(
            || (url.clone(), String::new()),
            |parsed| {
                if !parsed.origin().is_tuple() {
                    return (url.clone(), String::new());
                }
                let origin = parsed.origin().ascii_serialization();
                let rest = url.strip_prefix(&origin).unwrap_or_default().to_owned();
                (origin, rest)
            },
        );
        let (strong, muted) = (p.text, p.text_muted);
        let address = container(
            row![
                // A long origin loses its start, never the registrable domain at its end.
                text(origin)
                    .size(13)
                    .wrapping(iced::widget::text::Wrapping::None)
                    .ellipsis(iced::widget::text::Ellipsis::Start)
                    .style(move |_| text::Style { color: Some(strong) }),
                text(rest)
                    .size(13)
                    .width(Length::Fill)
                    .wrapping(iced::widget::text::Wrapping::None)
                    .ellipsis(iced::widget::text::Ellipsis::End)
                    .style(move |_| text::Style { color: Some(muted) }),
            ]
            .align_y(iced::Alignment::Center),
        )
        .clip(true)
        .width(Length::Fill);
        let field = p.field;
        let body = container(
            row![self.address_identity(), address]
                .spacing(4)
                .align_y(iced::Alignment::Center),
        )
        .padding([0, 6])
        .height(32)
        .width(Length::Fill)
        .align_y(iced::Alignment::Center)
        .style(move |_| container::Style {
            background: Some(field.into()),
            border: iced::Border {
                radius: 9.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });
        let loading = self.webview.borrow().loading;
        row![
            body,
            enzui_widgets::toolbar_icon::enz_toolbar_icon(
                if loading { "x" } else { "rotate-cw" },
                if loading { c(Command::Stop) } else { Message::BrowserReload },
            )
            .aria_label(if loading { "Stop loading" } else { "Reload" })
            .into_element(),
            enzui_widgets::window_controls::enz_window_controls(c(Command::Exit))
                .show_minimize(true, c(Command::Minimize))
                .into_element(),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center)
        .into()
    }

    /// A page asked to close its window. Only a window a page opened may be
    /// closed by a page; its opener comes back to the front.
    pub(super) fn popup_close_requested(&mut self, key: u64) -> Task<Message> {
        // A popup window holds only the window the page opened.
        if self.popup_window && self.browser.saved.tabs.iter().any(|tab| tab.key == key) {
            return Task::done(c(Command::Window(
                enzui_widgets::title_bar_gesture::WindowCommand::Close,
            )));
        }
        let Some(opener) = self.browser.popup_openers.remove(&key) else {
            return Task::none();
        };
        let Some(index) = self.browser.saved.tabs.iter().position(|tab| tab.key == key) else {
            return Task::none();
        };
        let task = self.browser_command(Command::Close(index));
        if let Some(opener) = self
            .browser
            .saved
            .tabs
            .iter()
            .position(|tab| tab.key == opener)
        {
            self.select_tab(opener);
        }
        task
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_a_page_opens_is_a_tab_in_its_openers_container_and_closes_back_to_it() {
        let mut app = browser_tests::app();
        app.browser_mode = true;
        app.browser.saved.tabs[0].url = "https://shop.example/".into();
        app.browser.saved.tabs[0].container = Some(3);
        let opener = app.browser.saved.tabs[0].key;
        app.popup_opened(opener, 41, "https://id.example/sign-in".into());
        assert_eq!(app.browser.saved.tabs.len(), 2);
        assert_eq!(app.browser.saved.active, 1);
        let popup = &app.browser.saved.tabs[1];
        assert_eq!(popup.container, Some(3));
        assert_eq!(
            popup.url, "about:blank",
            "an uncommitted window took its target address"
        );
        // The tab took a new key when its window was let load; it is still
        // known as the window of its opener.
        let key = popup.key;
        assert_eq!(app.browser.popup_openers.get(&key), Some(&opener));

        // The opener moves on to another document and still gets its tab back.
        app.browser.replace_tab_key(0, next_tab_key());
        let _ = app.popup_close_requested(key);
        assert_eq!(app.browser.saved.tabs.len(), 1);
        assert_eq!(app.browser.saved.active, 0);
        assert!(app.browser.popup_openers.is_empty());
    }

    #[test]
    fn a_page_cannot_close_a_tab_the_person_opened() {
        let mut app = browser_tests::app();
        app.browser_mode = true;
        let _ = app.update(c(Command::Action(ActionCommand::VisitBackground(
            "https://other.example/".into(),
        ))));
        assert_eq!(app.browser.saved.tabs.len(), 2);
        let key = app.browser.saved.tabs[1].key;
        let _ = app.popup_close_requested(key);
        assert_eq!(app.browser.saved.tabs.len(), 2);
        // Nor does a window whose opener is gone open a tab.
        app.popup_opened(key + 1000, 5, "https://id.example/".into());
        assert_eq!(app.browser.saved.tabs.len(), 2);
    }
}
