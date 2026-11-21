use super::*;
use iced::widget::{column, row};
fn login_choice<'a>(label: &str, source: &str, action: CredentialCommand) -> Element<'a, Message> {
    let content = row![
        enzui_widgets::icon::enz_icon("user-round").size(16.),
        column![
            text(label.to_owned()).size(14),
            enzui_widgets::enz_text::enz_text(source.to_owned())
                .size_px(12.)
                .tone(enzui_widgets::enz_text::TextTone::Muted)
                .into_element()
        ]
        .spacing(2)
        .width(iced::Length::Fill)
    ]
    .spacing(10)
    .align_y(iced::Alignment::Center);
    enzui_widgets::list_row_surface::enz_list_row_surface(content, message(action))
        .label(format!("{label}, {source}"))
        .row_height(52.)
        .content_padding(8.)
        .outer_gutter(0.)
        .into_element()
}
fn choice<'a>(label: &str, icon: &str, action: CredentialCommand) -> Element<'a, Message> {
    enzui_widgets::button::enz_button(label.to_owned(), message(action))
        .icon(icon)
        .kind(enzui_widgets::button::ButtonKind::Quiet)
        .size(enzui_widgets::button::ButtonSize::Md)
        .width(iced::Length::Fill)
        .into_element()
}
use iced::{Element, Rectangle};
fn host<'a>(
    content: Element<'a, Message>,
    popup: Option<Element<'a, Message>>,
    anchor: Rectangle,
) -> Element<'a, Message> {
    enzui_widgets::anchored_overlay::dismissible_anchored_menu_at(
        content,
        popup,
        anchor,
        enzui_core::placement::Placement::new(enzui_core::placement::Side::Bottom)
            .alignment(enzui_core::placement::Alignment::Start),
        || message(CredentialCommand::DismissInline),
    )
}

pub(super) struct Offer {
    pub(super) serial: u64,
    pub(super) target: FormDocument,
    pub(super) anchor: Rectangle,
    pub(super) loaded: bool,
}
impl State {
    pub(crate) fn inline_open(&self) -> bool {
        self.inline.as_ref().is_some_and(|offer| {
            self.unlocked
                && offer.loaded
                && self.entries.iter().any(|entry| {
                    entry.site == offer.target.site
                        && (offer.target.password && entry.has_password
                            || offer.target.document.username_only && !entry.username.is_empty()
                            || offer.target.otp && entry.has_totp)
                })
        })
    }
    pub(crate) fn dismiss_inline(&mut self) {
        self.inline = None;
    }
}
pub(crate) use crate::credentials::credential_trace;

impl App {
    pub(super) fn credential_field_focus(
        &mut self,
        focus: Option<crate::credential_bridge::FieldFocus>,
    ) {
        let Some(focus) = focus else {
            if self.browser.credentials.inline.is_some() {
                self.browser.chrome_revision = self.browser.chrome_revision.wrapping_add(1);
            }
            self.browser.credentials.dismiss_inline();
            return;
        };
        let host = self.webview.borrow();
        credential_trace(format_args!(
            "focus: document={} autofill={} destination={} panel_closed={} keyboard_blocked={} view_focused={} unlocked={}",
            self.current_credential_document().is_some_and(|target| target.password || target.otp || target.document.username_only),
            self.browser.credentials.settings.autofill,
            self.browser.credentials.has_destination(),
            self.browser.panel == Panel::None,
            self.webpage_keyboard_blocked(),
            host.focus.is_focused(),
            self.browser.credentials.unlocked,
        ));
        let Some(target) = self
            .current_credential_document()
            .cloned()
            .filter(|target| {
                self.browser.credentials.settings.autofill
                    && self.browser.credentials.has_destination()
                    && self.browser.panel == Panel::None
                    && !self.webpage_keyboard_blocked()
                    && host.focus.is_focused()
                    && (target.password || target.otp || target.document.username_only)
            })
        else {
            return;
        };
        let Some((crate::browser_input::Surface::Web(view), viewport)) = self.browser.input.bounds
        else {
            return;
        };
        if host.live_view != Some(view) {
            credential_trace(format_args!("focus: input bounds belong to another view"));
            return;
        }
        let Some(anchor) = focus.anchor(viewport) else {
            credential_trace(format_args!("focus: field outside the page area"));
            return;
        };
        drop(host);
        let state = &mut self.browser.credentials;
        if state.inline.as_ref().is_some_and(|offer| {
            offer.target.tab == target.tab
                && offer.target.site.matches_page(&target.site)
                && offer.target.document.document == target.document.document
                && offer.target.document.target == target.document.target
                && offer.anchor == anchor
        }) {
            return;
        }
        self.browser.chrome_revision = self.browser.chrome_revision.wrapping_add(1);
        state.inline_serial = state.inline_serial.wrapping_add(1);
        let site = target.site.clone();
        state.inline = Some(Offer {
            serial: state.inline_serial,
            target,
            anchor,
            loaded: false,
        });
        credential_trace(format_args!(
            "focus: offer for {} (unlocked={}, desktop unlock available={})",
            site.origin,
            state.unlocked,
            state.desktop_available() && !state.desktop_attempted
        ));
        if state.unlocked {
            let serial = state.inline_serial;
            let task =
                self.credential_request(Purpose::InlineList(serial), Operation::List(Some(site)));
            self.pending_tasks.push(task);
        } else if state.desktop_available() && !state.desktop_attempted {
            let task = self.credential_desktop_unlock(false);
            self.pending_tasks.push(task);
        }
    }
    pub(super) fn take_inline_offer(&mut self, serial: u64) -> Option<FormDocument> {
        if self.browser.credentials.inline.as_ref()?.serial != serial {
            return None;
        }
        let offer = self.browser.credentials.inline.take()?;
        let current = self.current_credential_document()?;
        (offer.target.tab == current.tab
            && offer.target.site.matches_page(&current.site)
            && offer.target.profile.shares_storage(&current.profile)
            && offer.target.document.document == current.document.document
            && offer.target.document.target == current.document.target
            && offer.target.document.username_only == current.document.username_only)
            .then_some(offer.target)
    }
    pub(crate) fn dismiss_stale_credential_offer(&mut self) {
        let stale = self.webpage_keyboard_blocked()
            || self
                .browser
                .credentials
                .inline
                .as_ref()
                .is_some_and(|offer| {
                    self.current_credential_document().is_none_or(|current| {
                        current.tab != offer.target.tab
                            || !current.site.matches_page(&offer.target.site)
                            || current.document.document != offer.target.document.document
                            || current.document.target != offer.target.document.target
                    })
                });
        if stale && self.browser.credentials.inline.is_some() {
            self.browser.chrome_revision = self.browser.chrome_revision.wrapping_add(1);
            self.browser.credentials.dismiss_inline();
        }
    }
    pub(crate) fn credential_inline_host<'a>(
        &'a self,
        content: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let Some(offer) = self.browser.credentials.inline.as_ref() else {
            return host(content, None, Rectangle::default());
        };
        let Some(current) = self.current_credential_document() else {
            return host(content, None, Rectangle::default());
        };
        if current.tab != offer.target.tab
            || current.document.document != offer.target.document.document
            || current.document.target != offer.target.document.target
            || self.webpage_keyboard_blocked()
        {
            return host(content, None, Rectangle::default());
        }
        let state = &self.browser.credentials;
        if !state.inline_open() {
            return host(content, None, Rectangle::default());
        }
        let mut body = column![
            row![
                enzui_widgets::icon::enz_icon("key-round").size(14.),
                text("Saved passwords").size(12)
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center)
        ]
        .spacing(6);
        {
            for entry in state
                .entries
                .iter()
                .filter(|entry| entry.site == current.site)
            {
                if current.password && entry.has_password
                    || current.document.username_only && !entry.username.is_empty()
                {
                    let label = if entry.username.is_empty() {
                        entry.name.expose()
                    } else {
                        entry.username.expose()
                    };
                    body = body.push(login_choice(
                        label,
                        &entry.source,
                        CredentialCommand::FillInline(offer.serial, entry.id.clone(), false),
                    ));
                }
                if current.otp && entry.has_totp {
                    body = body.push(choice(
                        "Fill verification code",
                        "timer",
                        CredentialCommand::FillInline(offer.serial, entry.id.clone(), true),
                    ));
                }
            }
        }
        let rows = {
            state
                .entries
                .iter()
                .filter(|entry| entry.site == current.site)
                .map(|entry| {
                    usize::from(
                        current.password && entry.has_password
                            || current.document.username_only && !entry.username.is_empty(),
                    ) + usize::from(current.otp && entry.has_totp)
                })
                .sum::<usize>()
        };
        let popup_height = (rows as f32 * 52. + 24.).clamp(56., 280.);
        let palette = self.look().palette.clone();
        let popup: Element<'_, Message> =
            iced::widget::container(scrollable(body).height(popup_height))
                .width(offer.anchor.width.clamp(240., 380.))
                .padding(10)
                .style(move |_| iced::widget::container::Style {
                    background: Some(palette.surface_elevated.into()),
                    text_color: Some(palette.text),
                    border: iced::Border {
                        color: palette.border,
                        width: 1.,
                        radius: 12.0.into(),
                    },
                    shadow: iced::Shadow {
                        color: iced::Color::BLACK.scale_alpha(0.15),
                        offset: iced::Vector::new(0., 4.),
                        blur_radius: 14.,
                    },
                    ..Default::default()
                })
                .into();
        host(content, Some(popup), offer.anchor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn autofill_popup_waits_for_an_unlocked_matching_login() {
        let mut app = focused_fixture();
        app.credential_bridge_message(focus("https://example.test"));
        let state = &mut app.browser.credentials;
        assert!(
            !state.inline_open(),
            "a locked account has no confirmed matches"
        );
        state.unlocked = true;
        assert!(!state.inline_open(), "pending lookups do not paint a popup");
        state.inline.as_mut().expect("pending offer").loaded = true;
        assert!(!state.inline_open(), "empty results do not paint a popup");
        let mut login = Credential::new(Site::parse("https://example.test", None).expect("site"))
            .expect("login");
        login.password = Secret::new("fixture-password".into());
        state
            .entries
            .push(credentials::provider::Summary::from(&login));
        assert!(state.inline_open());
        state.entries[0].site = Site::parse("https://other.test", None).expect("other");
        assert!(!state.inline_open());
    }
    fn focused_fixture() -> App {
        let mut app = App::new(enzui_theme::EnzTheme::dark());
        app.browser_mode = true;
        app.browser.credentials.settings = Settings::enabled_defaults();
        app.browser.credentials.vault_exists = true;
        app.browser.saved.tabs[app.browser.saved.active].url = "https://example.test/login".into();
        let mut host = app.webview.borrow_mut();
        host.location = Some("https://example.test/login".into());
        host.tls = Some("WebKit reported no certificate errors".into());
        let view = host.live_view.expect("fixture view");
        host.route_widget_input(&enzui_webkit::input::InputEvent::FocusGained {
            view,
            modifiers: enzui_webkit::input::Modifiers::NONE,
        });
        drop(host);
        app.browser.input.bounds = Some((
            crate::browser_input::Surface::Web(view),
            Rectangle {
                x: 240.,
                y: 56.,
                width: 800.,
                height: 600.,
            },
        ));
        app
    }
    fn focus(action: &str) -> enzui_security::secret::SecretText {
        enzui_security::secret::SecretText::new(serde_json::json!({"kind":"ready","document":"0123456789abcdef0123456789abcdef","origin":"https://example.test","target":"1","action":action,"password":true,"otp":false,"focus":{"x":100,"y":200,"width":280,"height":38,"viewport_width":800,"viewport_height":600}}).to_string())
    }
    #[test]
    fn field_focus_uses_desktop_unlock_once_and_respects_explicit_lock() {
        use enzui_services::credential_store::MemoryCredentialStore;
        use std::{
            sync::Arc,
            time::{Duration, Instant},
        };
        let scratch = credentials::tests::Scratch::new();
        let config = scratch.path().with_file_name("settings.json");
        let mut settings = Settings::enabled_defaults();
        settings.desktop_unlock = true;
        credentials::config::save(&config, &settings).expect("isolated settings");
        let store = Arc::new(MemoryCredentialStore::new());
        let service = Service::start_with_store(
            scratch.path(),
            config.clone(),
            Arc::<MemoryCredentialStore>::clone(&store),
        )
        .expect("worker");
        let reply = service
            .request(Operation::Open {
                password: Secret::new("a distinct fixture password".into()),
                create: true,
            })
            .expect("enroll request")
            .blocking_recv()
            .expect("enroll reply");
        assert!(
            reply.unlocked && reply.result.is_ok(),
            "fixture vault enrolled"
        );
        service.lock();
        let mut app = focused_fixture();
        app.browser.credentials.settings = settings;
        app.browser.credentials.config = config;
        app.browser.credentials.service = Some(std::sync::Arc::new(service));
        app.credential_bridge_message(focus("https://example.test"));
        assert!(
            app.browser.credentials.busy,
            "field focus schedules desktop unlock"
        );
        let deadline = Instant::now() + Duration::from_secs(3);
        while store.get_call_count() == 0
            || !app
                .browser
                .credentials
                .service
                .as_ref()
                .expect("worker")
                .unlocked()
        {
            assert!(
                Instant::now() < deadline,
                "desktop unlock did not reach the worker"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(store.get_call_count(), 1, "one lookup for focused field");
        app.credential_bridge_message(focus("https://example.test"));
        assert_eq!(
            store.get_call_count(),
            1,
            "unchanged field does not repeat lookup"
        );
        app.browser.credentials.lock();
        app.credential_bridge_message(focus("https://example.test"));
        assert_eq!(
            store.get_call_count(),
            1,
            "explicit lock suppresses automatic reopening"
        );
    }
    #[test]
    fn inline_credentials_preserve_page_keys_bind_selection_and_drop_stale_scopes() {
        let mut app = focused_fixture();
        let before = app.browser.chrome_revision;
        app.credential_bridge_message(focus("https://example.test"));
        assert!(
            app.browser.chrome_revision > before,
            "native focus metadata requests an immediate popup redraw"
        );
        let offer = app
            .browser
            .credentials
            .inline
            .as_ref()
            .expect("field offer");
        let serial = offer.serial;
        assert_eq!(
            offer.anchor,
            Rectangle {
                x: 340.,
                y: 256.,
                width: 280.,
                height: 38.
            }
        );
        assert!(
            !app.webpage_keyboard_blocked(),
            "suggestions do not interrupt page typing or paste"
        );
        assert!(app.webview.borrow().focus.is_focused(), "field keeps focus");
        assert!(app.take_inline_offer(serial + 1).is_none());
        assert_eq!(
            app.browser
                .credentials
                .inline
                .as_ref()
                .expect("new offer survives old click")
                .serial,
            serial
        );
        let key = app.browser.saved.tabs[app.browser.saved.active].key;
        app.browser.saved.tabs[app.browser.saved.active].key = key + 1;
        app.dismiss_stale_credential_offer();
        assert!(
            app.browser.credentials.inline.is_none(),
            "switching tabs retires the offer"
        );
        app.browser.saved.tabs[app.browser.saved.active].key = key;
        app.credential_bridge_message(focus("https://evil.test"));
        assert!(
            app.browser.credentials.inline.is_none(),
            "cross-origin form action offers no fill"
        );
        app.credential_bridge_message(focus("https://example.test"));
        assert!(app.browser.credentials.inline.is_some());
        let _ = app.update(message(CredentialCommand::OpenSite));
        assert_eq!(app.browser.panel, Panel::Passwords);
        assert!(
            app.webpage_keyboard_blocked(),
            "unlock form revokes webpage keyboard ownership"
        );
        assert!(!app.webview.borrow().focus.is_focused());
    }
    #[test]
    fn inline_popup_rebuild_preserves_content_input_focus() {
        let idle = focused_fixture();
        let mut active = focused_fixture();
        active.credential_bridge_message(focus("https://example.test"));
        active.browser.credentials.unlocked = true;
        active
            .browser
            .credentials
            .inline
            .as_mut()
            .expect("offer")
            .loaded = true;
        let mut login = Credential::new(Site::parse("https://example.test", None).expect("site"))
            .expect("login");
        login.password = Secret::new("fixture-password".into());
        active
            .browser
            .credentials
            .entries
            .push(credentials::provider::Summary::from(&login));
        assert!(active.browser.credentials.inline_open());
        let field = || {
            enzui_widgets::text_field::enz_text_field("", "Content field", "", |value| {
                c(Command::Query(value))
            })
            .into_element()
        };
        let mut ui = iced_test::Simulator::with_size(
            Default::default(),
            iced::Size::new(800., 600.),
            idle.credential_inline_host(field()),
        );
        ui.point_at((100., 16.));
        ui.simulate(iced_test::simulator::click());
        ui = ui.rebuild(active.credential_inline_host(field()));
        ui.simulate([iced_test::simulator::press_key(
            iced::keyboard::Key::Character("x".into()),
            Some("x".into()),
        )]);
        assert!(
            ui.drain().any(
                |message| matches!(message, Message::Chrome(Command::Query(value)) if value=="x")
            ),
            "opening suggestions must not reset content focus"
        );
        ui = ui.rebuild(idle.credential_inline_host(field()));
        ui.simulate([iced_test::simulator::press_key(
            iced::keyboard::Key::Character("y".into()),
            Some("y".into()),
        )]);
        assert!(
            ui.drain().any(
                |message| matches!(message, Message::Chrome(Command::Query(value)) if value=="y")
            ),
            "dismissing suggestions must not reset content focus"
        );
    }
    #[test]
    fn ordinary_page_tabs_do_not_wait_for_fixture_titles_or_fabricate_widget_exit() {
        let app = focused_fixture();
        let mut host = app.webview.borrow_mut();
        let view = host.live_view.expect("view");
        for _ in 0..3 {
            let result = host.route_widget_input(&enzui_webkit::input::InputEvent::KeyPressed {
                view,
                key: enzui_webkit::input::KeyCode::Named(enzui_webkit::input::NamedKey::Tab),
                modifiers: enzui_webkit::input::Modifiers::NONE,
            });
            assert!(
                result.is_none(),
                "ordinary document input does not fabricate native traversal acknowledgements"
            );
            assert!(
                host.focus.is_focused(),
                "a missing fixture title cannot steal page focus"
            );
        }
    }
    #[test]
    fn inline_field_anchors_scale_clip_and_refuse_invalid_geometry() {
        let viewport = Rectangle {
            x: 30.,
            y: 50.,
            width: 800.,
            height: 600.,
        };
        let mut focus = crate::credential_bridge::FieldFocus {
            x: 10.,
            y: 20.,
            width: 100.,
            height: 24.,
            viewport_width: 400.,
            viewport_height: 300.,
        };
        assert_eq!(
            focus.anchor(viewport),
            Some(Rectangle {
                x: 50.,
                y: 90.,
                width: 200.,
                height: 48.
            })
        );
        focus.x = 900.;
        assert!(focus.anchor(viewport).is_none());
        focus.x = f32::NAN;
        assert!(focus.anchor(viewport).is_none());
        focus.x = 10.;
        focus.viewport_width = 0.;
        assert!(focus.anchor(viewport).is_none());
    }
}
