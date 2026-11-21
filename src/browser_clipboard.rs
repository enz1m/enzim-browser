use crate::{App, Message};
use enzui_webkit::{
    input::{InputEvent, KeyCode, Modifiers, NamedKey},
    web::ViewId,
};
use iced::Task;

#[derive(Clone, Copy, Debug)]
pub(crate) struct PasteTarget {
    keyboard: bool,
    tab: u64,
    view: ViewId,
    epoch: u64,
}

pub(crate) fn paste_shortcut(event: &InputEvent) -> bool {
    matches!(event, InputEvent::KeyPressed { key: KeyCode::Character(key), modifiers, .. }
        if modifiers.contains(Modifiers::CTRL) && !modifiers.contains(Modifiers::ALT) && key.eq_ignore_ascii_case("v"))
        || matches!(event, InputEvent::KeyPressed { key: KeyCode::Named(NamedKey::Insert), modifiers, .. }
            if modifiers.shift() && !modifiers.contains(Modifiers::CTRL) && !modifiers.contains(Modifiers::ALT))
}

impl App {
    pub(crate) fn webpage_keyboard_blocked(&self) -> bool {
        self.browser_mode
            && (self.browser.panel != crate::browser::Panel::None
                || self.browser.library.is_some()
                || self.browser.chooser.is_fallback_open()
                || self.browser.menu_state.is_open()
                || self.browser.container_menu_state.is_open()
                || self.browser.zoom_menu_state.is_open()
                || self.browser.address_suggestions_open)
    }
    pub(crate) fn suspend_webpage_keyboard(&mut self) {
        let mut host = self.webview.borrow_mut();
        host.cancel_pending_paste();
        host.page_close_ime(enzui_webkit::web::ImeClose::FocusLost);
        let _ = host.focus.focus_lost();
        if let (Some(native), Some(view)) = (&host.native, host.live_view) {
            native.focus_out(view);
        }
    }
    fn paste_tab(&self) -> u64 {
        if self.browser_mode {
            self.browser.saved.tabs[self.browser.saved.active].key
        } else {
            0
        }
    }
    pub(crate) fn request_web_paste(&mut self, keyboard: bool) -> Task<Message> {
        if self.webpage_keyboard_blocked() {
            return Task::none();
        }
        let mut host = self.webview.borrow_mut();
        if !host.is_visible() || (keyboard && !host.focus.is_focused()) {
            return Task::none();
        }
        let Some(view) = host.live_view else {
            return Task::none();
        };
        host.cancel_pending_paste();
        let target = PasteTarget {
            keyboard,
            tab: self.paste_tab(),
            view,
            epoch: host.paste_epoch,
        };
        iced::clipboard::read_text().map(move |text| {
            Message::WebViewPasteReady(
                target,
                text.ok()
                    .map(|text| enzui_security::secret::SecretText::new(text.to_string())),
            )
        })
    }
    fn accepts_paste(&self, target: PasteTarget) -> bool {
        if self.webpage_keyboard_blocked() {
            return false;
        }
        let host = self.webview.borrow();
        target.tab == self.paste_tab()
            && host.live_view == Some(target.view)
            && host.router.accepts(target.view)
            && host.is_visible()
            && !host.loading
            && target.epoch == host.paste_epoch
            && (!target.keyboard || host.focus.is_focused())
    }
    pub(crate) fn finish_web_paste(
        &mut self,
        target: PasteTarget,
        text: Option<enzui_security::secret::SecretText>,
    ) {
        if self.accepts_paste(target)
            && let Some(text) = text
        {
            let mut host = self.webview.borrow_mut();
            host.native_paste(text.expose_secret());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_paste_replies_cannot_follow_focus_navigation_tabs_or_recreation() {
        let mut app = App::new(enzui_theme::EnzTheme::dark());
        app.browser_mode = true;
        let target = |app: &App| {
            let host = app.webview.borrow();
            PasteTarget {
                keyboard: false,
                tab: app.paste_tab(),
                view: host.live_view.expect("live fixture"),
                epoch: host.paste_epoch,
            }
        };
        let first = target(&app);
        assert!(app.accepts_paste(first));
        app.webview.borrow_mut().cancel_pending_paste();
        assert!(!app.accepts_paste(first));
        let before_blur = target(&app);
        app.webview
            .borrow_mut()
            .route_widget_input(&InputEvent::FocusLost {
                view: before_blur.view,
                modifiers: Modifiers::NONE,
            });
        assert!(!app.accepts_paste(before_blur));
        let before_reload = target(&app);
        app.submit_browser_action(crate::webview_native::BrowserAction::Reload);
        assert!(!app.accepts_paste(before_reload));
        let before_tab = target(&app);
        app.browser.saved.tabs[app.browser.saved.active].url = "https://example.org/".into();
        let _ = app.browser_command(crate::browser::Command::Duplicate(app.browser.saved.active));
        assert!(!app.accepts_paste(before_tab));
        app = App::new(enzui_theme::EnzTheme::dark());
        app.browser_mode = true;
        let before_recreate = target(&app);
        assert!(app.accepts_paste(before_recreate));
        app.webview.borrow_mut().recreate_view();
        assert!(!app.accepts_paste(before_recreate));
        let view = app.webview.borrow().live_view.expect("recreated fixture");
        let key = |key, modifiers| InputEvent::KeyPressed {
            view,
            key,
            modifiers,
        };
        assert!(paste_shortcut(&key(
            KeyCode::Character("v".into()),
            Modifiers::CTRL
        )));
        assert!(paste_shortcut(&key(
            KeyCode::Named(NamedKey::Insert),
            Modifiers::SHIFT
        )));
        assert!(!paste_shortcut(&key(
            KeyCode::Character("v".into()),
            Modifiers::CTRL | Modifiers::ALT
        )));
    }
}

#[cfg(test)]
mod chrome_focus_tests {
    use super::*;
    use crate::browser::{Panel, credentials_ui::CredentialCommand};
    fn target(app: &App) -> PasteTarget {
        let host = app.webview.borrow();
        PasteTarget {
            keyboard: true,
            tab: app.paste_tab(),
            view: host.live_view.expect("fixture view"),
            epoch: host.paste_epoch,
        }
    }
    fn focus_page(app: &mut App) {
        let view = app.webview.borrow().live_view.expect("fixture view");
        app.webview
            .borrow_mut()
            .route_widget_input(&InputEvent::FocusGained {
                view,
                modifiers: Modifiers::NONE,
            });
    }
    #[test]
    fn browser_forms_revoke_page_keys_ime_and_paste_before_and_after_dismissal() {
        let mut app = App::new(enzui_theme::EnzTheme::dark());
        app.browser_mode = true;
        app.browser.saved.tabs[app.browser.saved.active].url = "https://example.test/".into();
        app.webview.borrow_mut().location = Some("https://example.test/".into());
        focus_page(&mut app);
        let pending = target(&app);
        assert!(
            app.accepts_paste(pending),
            "focused visible page permits its own paste"
        );
        app.browser.panel = Panel::Passwords;
        assert!(
            !app.accepts_paste(pending),
            "even a stale focused page cannot accept paste under the vault popup"
        );
        let epoch = app.webview.borrow().paste_epoch;
        let _ = app.request_web_paste(true);
        assert_eq!(
            app.webview.borrow().paste_epoch,
            epoch,
            "browser forms start no page clipboard read"
        );
        app.webview.borrow_mut().last_input = Some("unchanged fixture input".into());
        let _ = app.update(Message::WebViewInput(InputEvent::KeyPressed {
            view: pending.view,
            key: KeyCode::Character("fixture-key-sentinel".into()),
            modifiers: Modifiers::NONE,
        }));
        assert_eq!(
            app.webview.borrow().last_input.as_deref(),
            Some("unchanged fixture input"),
            "vault typing never reaches native page input or diagnostics"
        );
        app.webview.borrow_mut().last_ime = Some("unchanged fixture IME".into());
        let _ = app.update(Message::WebViewImeEvent(
            enzui_webkit::web::ToolkitImeEvent::Commit("fixture-ime-sentinel".into()),
        ));
        assert_eq!(
            app.webview.borrow().last_ime.as_deref(),
            Some("unchanged fixture IME"),
            "vault IME never reaches native page input"
        );
        app.browser.panel = Panel::None;
        focus_page(&mut app);
        let pending = target(&app);
        let _ = app.update(Message::Chrome(crate::browser::Command::Credential(
            CredentialCommand::OpenSite,
        )));
        assert!(
            !app.webview.borrow().focus.is_focused(),
            "opening a credential popup revokes page keyboard focus immediately"
        );
        app.browser.panel = Panel::None;
        focus_page(&mut app);
        assert!(
            !app.accepts_paste(pending),
            "dismissal and a later page focus cannot revive an old clipboard reply"
        );
        assert!(
            app.accepts_paste(target(&app)),
            "a fresh page gesture after dismissal works normally"
        );
        let message = Message::WebViewPasteReady(
            pending,
            Some(enzui_security::secret::SecretText::new(
                "fixture-clipboard-sentinel".into(),
            )),
        );
        assert!(
            !format!("{message:?}").contains("fixture-clipboard-sentinel"),
            "clipboard reply diagnostics redact pasted text"
        );
    }
}
