use crate::{
    App, Message,
    browser::{Command as BrowserCommand, c},
    credentials,
};
use iced::{
    Element, Length, Task,
    widget::{Space, column, container, row, text},
};

#[derive(Clone, Debug)]
pub(crate) enum Command {
    Next,
    Back,
    Engine(String),
    Privacy(usize),
    Toggle(&'static str, bool),
    Passwords(usize),
    Finish,
    Skip,
    Completed(Result<(), String>),
}

pub(crate) struct State {
    pub(crate) has_launch_urls: bool,
    step: usize,
    engine: String,
    privacy: crate::privacy::Privacy,
    passwords: usize,
    desktop_unlock: bool,
    notifications: bool,
    busy: bool,
    notice: String,
}
impl State {
    pub(crate) fn new(saved: &crate::browser::Saved, settings: &credentials::Settings) -> Self {
        Self {
            has_launch_urls: false,
            step: 0,
            engine: saved.search.selected.clone(),
            privacy: saved.privacy.clone(),
            passwords: if let Some(kind) = settings.setup_provider {
                3 + credentials::accounts::Kind::ALL
                    .iter()
                    .position(|entry| *entry == kind)
                    .unwrap_or(0)
            } else if settings.enabled {
                if settings.storage == credentials::accounts::Storage::System {
                    2
                } else {
                    1
                }
            } else {
                1
            },
            desktop_unlock: settings.desktop_unlock,
            notifications: true,
            busy: false,
            notice: String::new(),
        }
    }
    fn settings(&self, old: &credentials::Settings) -> credentials::Settings {
        let mut settings = old.clone();
        settings.enabled = self.passwords > 0;
        settings.provider = "enzim.builtin".into();
        settings.storage = if self.passwords == 2 {
            credentials::accounts::Storage::System
        } else {
            credentials::accounts::Storage::Browser
        };
        settings.system_store |= self.passwords == 2;
        settings.desktop_unlock = self.desktop_unlock;
        settings.setup_provider = self
            .passwords
            .checked_sub(3)
            .and_then(|index| credentials::accounts::Kind::ALL.get(index).copied());
        settings.preferred_source.clear();
        settings.save_to.clear();
        if let Some(kind) = settings.setup_provider
            && let Some(account) = settings
                .accounts
                .iter()
                .find(|account| account.enabled && account.kind == kind)
        {
            settings.preferred_source.clone_from(&account.id);
            settings.save_to.clone_from(&account.id);
            settings.setup_provider = None;
        }
        settings
    }
    fn apply_preferences(&self, saved: &mut crate::browser::Saved) {
        if crate::search::builtins()
            .iter()
            .any(|engine| engine.id == self.engine)
        {
            saved.search.selected.clone_from(&self.engine);
        }
        saved.privacy = self.privacy.clone();
        saved.notifications = if self.notifications {
            crate::website_notifications::Policy::Ask
        } else {
            crate::website_notifications::Policy::Block
        };
        saved.welcome_version = Some(1);
    }
    fn content(&self, theme: &enzui_theme::EnzTheme, width: f32) -> Element<'_, Message> {
        let p = &theme.palette;
        let title = ["Search", "Privacy", "Passwords"][self.step];
        let subtitle = [
            "Choose where your searches begin. You can change this whenever you like.",
            "Choose what websites can load and what they can ask for.",
            "Keep logins here, use your desktop keyring, or connect a password manager.",
        ][self.step];
        let symbol = ["search", "shield-check", "key-round"][self.step];
        let mut details = column![].spacing(8).width(Length::Fill);
        if self.step == 0 {
            let engines = crate::search::builtins();
            for pair in engines.chunks(2) {
                let mut choices = row![].spacing(10);
                for engine in pair {
                    let selected = self.engine == engine.id;
                    choices = choices.push(
                        enzui_widgets::focus_button::focus_button(
                            row![
                                text(engine.name.clone()).size(14).width(Length::Fill),
                                enzui_widgets::icon::enz_icon(if selected {
                                    "circle-check"
                                } else {
                                    "circle"
                                })
                                .size(16.)
                                .tint(if selected { p.accent } else { p.text_muted })
                                .into_element(),
                            ]
                            .spacing(10)
                            .align_y(iced::Alignment::Center),
                            message(Command::Engine(engine.id.clone())),
                        )
                        .semantics(
                            enzui_core::semantics::Semantics::new(
                                enzui_core::semantics::Role::RadioButton,
                            )
                            .label(engine.name.clone())
                            .toggled(if selected {
                                enzui_core::semantics::Toggle::True
                            } else {
                                enzui_core::semantics::Toggle::False
                            })
                            .selected(selected),
                        )
                        .width(Length::Fill)
                        .padding([12, 14])
                        .style_focus(move |theme, status| {
                            let p = &crate::browser::palette(theme);
                            iced::widget::button::Style {
                                background: Some(
                                    if matches!(
                                        status,
                                        enzui_widgets::focus_button::FocusStatus::Hovered
                                    ) {
                                        p.surface_hover
                                    } else {
                                        p.surface
                                    }
                                    .into(),
                                ),
                                text_color: p.text,
                                border: iced::Border {
                                    color: if matches!(
                                        status,
                                        enzui_widgets::focus_button::FocusStatus::Focused
                                    ) {
                                        p.accent
                                    } else {
                                        p.text_muted.scale_alpha(0.18)
                                    },
                                    width: if matches!(
                                        status,
                                        enzui_widgets::focus_button::FocusStatus::Focused
                                    ) {
                                        2.
                                    } else {
                                        1.
                                    },
                                    radius: 10.0.into(),
                                },
                                ..Default::default()
                            }
                        }),
                    );
                }
                if pair.len() == 1 {
                    choices = choices.push(Space::new().width(Length::Fill));
                }
                details = details.push(choices);
            }
            details = details.push(
                text("Your address bar searches and new-tab searches will use this choice.")
                    .size(12)
                    .color(p.text_muted),
            );
        } else if self.step == 1 {
            let names = vec!["Balanced".into(), "Strict".into(), "No blocking".into()];
            let selected = match self.privacy.blocking {
                0 => 2,
                2 => 1,
                _ => 0,
            };
            let control = enzui_widgets::combo_box::enz_combo_box(
                "",
                names.clone(),
                Some(names[selected].clone()),
                |_| message(Command::Privacy(0)),
            )
            .selected_index(Some(selected))
            .id_prefix("welcome-privacy")
            .on_select_index(|index| message(Command::Privacy(index)))
            .width(160)
            .into_element();
            details = details.push(
                enzui_widgets::preferences_row::enz_preferences_row("Request blocking")
                    .icon("shield")
                    .subtitle("Balanced preserves compatibility. Strict uses stronger filtering.")
                    .control(control)
                    .accessory_width(160.)
                    .tokens(compact_row())
                    .into_element(),
            );
            details = details.push(
                column![
                    text("Additional controls").size(14),
                    text("Choose these separately from the blocking level.")
                        .size(12)
                        .color(p.text_muted),
                ]
                .spacing(3)
                .padding(iced::Padding {
                    top: 14.,
                    bottom: 4.,
                    ..Default::default()
                }),
            );
            let mut controls = column![].spacing(0);
            for (key, title, detail, value) in [
                (
                    "ads",
                    "Block advertisements",
                    "Hide known advertising requests and page elements.",
                    self.privacy.value("ads"),
                ),
                (
                    "trackers",
                    "Block trackers",
                    "Reduce requests that follow you between websites.",
                    self.privacy.value("trackers"),
                ),
                (
                    "cookies",
                    "Block third-party cookies",
                    "Keep cross-site cookies out of your browsing session.",
                    !self.privacy.third_party_cookies,
                ),
                (
                    "links",
                    "Clean tracking links",
                    "Remove common advertising identifiers from links.",
                    self.privacy.clean_links,
                ),
                (
                    "notifications",
                    "Let websites ask for notifications",
                    "Each website still needs your permission.",
                    self.notifications,
                ),
            ] {
                controls = controls.push(
                    enzui_widgets::preferences_row::enz_preferences_row(title)
                        .tokens(compact_row())
                        .subtitle(detail)
                        .switch(value, move |value| message(Command::Toggle(key, value)))
                        .into_element(),
                );
            }
            details = details.push(controls);
        } else {
            let options = [1, 2, 3, 0];
            let labels = vec![
                "Browser only".into(),
                "System keyring".into(),
                "Bitwarden".into(),
                "Not now".into(),
            ];
            let selected = options
                .iter()
                .position(|index| *index == self.passwords)
                .unwrap_or(0);
            let choice = enzui_widgets::combo_box::enz_combo_box(
                "",
                labels.clone(),
                Some(labels[selected].clone()),
                |_| message(Command::Passwords(0)),
            )
            .selected_index(Some(selected))
            .separator_before(3)
            .id_prefix("welcome-passwords")
            .on_select_index(move |index| message(Command::Passwords(options[index])))
            .width(180)
            .into_element();
            details = details.push(
                enzui_widgets::preferences_row::enz_preferences_row("Password manager")
                    .icon("key-round")
                    .control(choice)
                    .accessory_width(180.)
                    .into_element(),
            );
            let explanation = match self.passwords {
                0 => {
                    "You can add a password manager later in Settings. Password saving and autofill stay off for now."
                }
                1 => {
                    "An encrypted vault stored by this browser. Set its password later in Passwords."
                }
                2 => {
                    "Store your logins in the desktop's Secret Service provider, including GNOME Keyring or oo7."
                }
                3 => {
                    "The Bitwarden connector is included. Connect your personal or business accounts later in Passwords."
                }
                _ => "Choose a password manager here or connect one later in Settings.",
            };
            details = details.push(text(explanation).size(14).color(p.text_muted));
            if self.passwords == 1 {
                details=details.push(enzui_widgets::preferences_row::enz_preferences_row("Unlock with my desktop keyring").subtitle("Keep the encrypted vault here and remember its unlock key on this desktop.").switch(self.desktop_unlock,|value|message(Command::Toggle("desktop",value))).into_element());
            }
            details = details.push(
                text("You can add more password managers later in Settings.")
                    .size(13)
                    .color(p.text_muted),
            );
        }
        let mut progress = row![].spacing(8);
        for step in 0..3 {
            let ink = if step <= self.step {
                p.accent
            } else {
                p.text_muted.scale_alpha(0.2)
            };
            progress = progress.push(container(Space::new()).width(Length::Fill).height(6).style(
                move |_| container::Style {
                    background: Some(ink.into()),
                    border: iced::Border {
                        radius: 3.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            ));
        }
        let heading = column![
            progress,
            row![
                enzui_widgets::motion::animated(
                    enzui_widgets::icon::enz_icon(symbol)
                        .size(28.)
                        .tint(p.accent)
                        .into_element()
                )
                .enters_from(iced::Vector::new(0., 6.), 0.6),
                text(title).size(if width < 540. { 25 } else { 30 }),
            ]
            .spacing(12)
            .align_y(iced::Alignment::Center),
            text(subtitle).size(14).color(p.text_muted),
        ]
        .spacing(16);
        let mut body = column![
            heading,
            enzui_widgets::motion::fade_on_change(self.step, details)
        ]
        .spacing(20);
        if !self.notice.is_empty() {
            body = body.push(text(&self.notice).size(13).color(p.text_muted));
        }
        let next = if self.step == 2 {
            Command::Finish
        } else {
            Command::Next
        };
        let actions = row![
            enzui_widgets::button::enz_button(
                if self.step == 0 {
                    "Use defaults"
                } else {
                    "Back"
                },
                message(if self.step == 0 {
                    Command::Skip
                } else {
                    Command::Back
                })
            )
            .kind(enzui_widgets::button::ButtonKind::Quiet)
            .enabled(!self.busy)
            .into_element(),
            Space::new().width(Length::Fill),
            enzui_widgets::button::enz_button(
                if self.busy {
                    "Saving…"
                } else if self.step == 2 {
                    "Start browsing"
                } else {
                    "Continue"
                },
                message(next)
            )
            .icon(if self.step == 2 {
                "arrow-up-right"
            } else {
                "arrow-right"
            })
            .kind(enzui_widgets::button::ButtonKind::Accent)
            .enabled(!self.busy)
            .into_element()
        ]
        .spacing(12)
        .align_y(iced::Alignment::Center);
        let controls = enzui_widgets::window_controls::enz_window_controls(c(BrowserCommand::Exit))
            .show_minimize(true, c(BrowserCommand::Minimize))
            .show_maximize(true, c(BrowserCommand::Maximize), false)
            .into_element();
        let header: Element<'_, Message> = row![
            enzui_widgets::icon::enz_icon("compass")
                .size(18.)
                .tint(p.accent)
                .into_element(),
            text("enzim Browser").size(14),
            Space::new().width(Length::Fill),
            controls,
        ]
        .align_y(iced::Alignment::Center)
        .spacing(10)
        .padding(12)
        .into();
        column![
            enzui_widgets::titlebar_drag_region::titlebar_drag_region(header)
                .on_gesture(|command| c(BrowserCommand::Window(command))),
            enzui_widgets::scroll_view::enz_scrollable(
                container(container(body).width((width - 48.).clamp(180., 680.)))
                    .padding(iced::Padding {
                        top: 16.,
                        right: 24.,
                        bottom: 16.,
                        left: 24.
                    })
                    .center_x(Length::Fill)
            )
            .id(iced::widget::Id::new("welcome-content"))
            .height(Length::Fill),
            container(actions).padding(16).center_x(Length::Fill),
        ]
        .height(Length::Fill)
        .into()
    }
    pub(crate) fn view<'a>(&'a self, theme: &'a enzui_theme::EnzTheme) -> Element<'a, Message> {
        iced::widget::responsive(move |size| self.content(theme, size.width)).into()
    }
}
fn compact_row() -> enzui_widgets::preferences_row::PreferencesRowTokens {
    enzui_widgets::preferences_row::PreferencesRowTokens {
        row_height_with_subtitle: 48.,
        wrap_vertical_padding: 6.,
        padding_x: 10.,
        ..Default::default()
    }
}
fn message(command: Command) -> Message {
    c(BrowserCommand::Welcome(command))
}

impl App {
    pub(crate) fn welcome_command(&mut self, command: Command) -> Task<Message> {
        let Some(state) = self.welcome.as_mut() else {
            return Task::none();
        };
        if state.busy && !matches!(command, Command::Completed(_)) {
            return Task::none();
        }
        let reset_scroll = matches!(command, Command::Next | Command::Back);
        match command {
            Command::Next => state.step = (state.step + 1).min(2),
            Command::Back => state.step = state.step.saturating_sub(1),
            Command::Engine(engine) => {
                if crate::search::builtins()
                    .iter()
                    .any(|entry| entry.id == engine)
                {
                    state.engine = engine;
                }
            }
            Command::Passwords(index) => state.passwords = index.min(3),
            Command::Privacy(index) => {
                state.privacy.blocking = match index {
                    1 => 2,
                    2 => 0,
                    _ => 1,
                };
                state.privacy.ads = index != 2;
                state.privacy.trackers = index != 2;
            }
            Command::Toggle("desktop", value) => state.desktop_unlock = value,
            Command::Toggle("notifications", value) => state.notifications = value,
            Command::Toggle("cookies", value) => state.privacy.third_party_cookies = !value,
            Command::Toggle(key, value) => state.privacy.set_control(key, value),
            Command::Skip | Command::Finish => {
                if matches!(command, Command::Skip) {
                    state.passwords = 1;
                    state.engine = "duckduckgo".into();
                    state.privacy = Default::default();
                    state.notifications = true;
                }
                let mut saved = self.browser.saved.clone();
                state.apply_preferences(&mut saved);
                let settings = state.settings(&self.browser.credentials.settings);
                state.busy = true;
                state.notice.clear();
                let path = crate::browser::state_path();
                return crate::browser::import_page::background(
                    "welcome-preferences",
                    move || {
                        credentials::config::save(&credentials::config::path(), &settings)?;
                        let parent = path
                            .parent()
                            .ok_or("Invalid browser profile settings path.")?;
                        enzui_security::secure_dir::ensure_private_dir(parent).map_err(|_| {
                            "Could not open this profile's settings safely.".to_owned()
                        })?;
                        let mut authority =
                            enzui_core::state_authority::StateAuthority::open_or_create_tightening(
                                &path,
                            )
                            .map_err(|_| {
                                "Could not open this profile's settings safely.".to_owned()
                            })?;
                        let bytes = serde_json::to_vec(&saved)
                            .map_err(|_| "Could not encode welcome preferences.".to_owned())?;
                        authority.publish(&bytes).map_err(|_| {
                            "Could not save welcome preferences. Try again.".to_owned()
                        })?;
                        Ok(())
                    },
                    |result| message(Command::Completed(result)),
                );
            }
            Command::Completed(result) => {
                state.busy = false;
                if let Err(error) = result {
                    state.notice = error;
                    return Task::none();
                }
                let has_launch_urls = state.has_launch_urls;
                state.apply_preferences(&mut self.browser.saved);
                self.browser.credentials.settings =
                    state.settings(&self.browser.credentials.settings);
                if let Ok((settings, Some(identity))) =
                    credentials::config::load_authorized(&credentials::config::path())
                {
                    self.credential_preferences_changed(&settings, identity);
                }
                self.welcome = None;
                if has_launch_urls {
                    let task = self.update(Message::BrowserGo);
                    self.browser.save();
                    return task;
                }
                return self.browser_command(BrowserCommand::Navigate(
                    "https://browser.enzim.dev/".into(),
                ));
            }
        }
        if reset_scroll {
            iced::widget::operation::snap_to(
                iced::widget::Id::new("welcome-content"),
                iced::widget::scrollable::RelativeOffset::START,
                iced::widget::operation::Animation::Instant,
            )
        } else {
            Task::none()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn welcome_completion_opens_browser_home_for_every_password_choice() {
        for passwords in 0..6 {
            let mut app = crate::browser::browser_tests::app();
            let mut state = State::new(&app.browser.saved, &app.browser.credentials.settings);
            state.passwords = passwords;
            app.welcome = Some(state);
            app.browser.library = Some(crate::browser::InternalPage::Passwords);
            let _ = app.welcome_command(Command::Completed(Ok(())));
            assert!(app.welcome.is_none(), "setup must finish");
            assert!(
                app.browser.library.is_none(),
                "finish must leave internal pages"
            );
            assert_eq!(app.address, "https://browser.enzim.dev/");
        }
    }

    #[test]
    fn welcome_completion_opens_the_initial_launch_url() {
        let mut app = crate::browser::browser_tests::app();
        app.welcome = Some(State::new(
            &app.browser.saved,
            &app.browser.credentials.settings,
        ));
        let url = "https://example.org/initial";
        app.open_launch_urls(vec![url.into()]);
        assert!(app.pending_navigation.is_none());

        drop(app.welcome_command(Command::Completed(Ok(()))));

        assert_eq!(app.address, url);
        assert_eq!(app.browser.saved.tabs.len(), 1);
        assert!(matches!(
            app.pending_navigation,
            Some(crate::webview_native::BrowserAction::Navigate(ref target)) if target == url
        ));
    }

    #[test]
    fn welcome_completion_keeps_launch_order_when_urls_arrive_during_publication() {
        let mut app = crate::browser::browser_tests::app();
        let mut state = State::new(&app.browser.saved, &app.browser.credentials.settings);
        state.busy = true;
        app.welcome = Some(state);
        let urls = [
            "https://example.org/initial",
            "https://example.org/forwarded",
            "https://example.org/last",
        ];
        app.open_launch_urls(vec![urls[0].into()]);
        drop(app.open_external_urls(urls[1..].iter().map(|url| (*url).into()).collect()));
        drop(app.welcome_command(Command::Completed(Err("try again".into()))));
        assert!(app.pending_navigation.is_none());

        drop(app.welcome_command(Command::Completed(Ok(()))));

        let saved = &app.browser.saved;
        assert_eq!(
            saved.tabs.iter().map(|tab| tab.url.as_str()).collect::<Vec<_>>(),
            urls
        );
        assert_eq!(saved.active, 2);
        assert_eq!(app.address, urls[2]);
        assert_eq!(app.browser.sleeping.len(), saved.tabs.len());
    }

    #[test]
    fn welcome_failed_publication_keeps_the_current_step_and_can_retry() {
        let mut app = crate::browser::browser_tests::app();
        let mut state = State::new(&app.browser.saved, &app.browser.credentials.settings);
        state.step = 2;
        state.busy = true;
        app.welcome = Some(state);
        let address = app.address.clone();
        let _ = app.welcome_command(Command::Completed(Err("storage unavailable".into())));
        let state = app.welcome.as_ref().expect("setup remains open");
        assert!(!state.busy, "the finish action must be usable again");
        assert_eq!(state.step, 2);
        assert_eq!(state.notice, "storage unavailable");
        assert_eq!(app.address, address);
    }

    #[test]
    fn welcome_choices_preserve_other_preferences_and_keep_passwords_optional() {
        let mut saved = crate::browser::Saved {
            layout: crate::browser::Layout::Icons,
            ..Default::default()
        };
        let settings = credentials::Settings::default();
        let mut state = State::new(&saved, &settings);
        state.engine = "brave".into();
        state.privacy.ads = false;
        state.notifications = false;
        state.apply_preferences(&mut saved);
        assert_eq!(saved.search.selected, "brave");
        assert_eq!(saved.layout, crate::browser::Layout::Icons);
        assert_eq!(saved.welcome_version, Some(1));
        assert!(!saved.privacy.value("ads"));
        assert_eq!(
            saved.notifications,
            crate::website_notifications::Policy::Block
        );
        assert!(state.settings(&settings).enabled);
        state.passwords = 0;
        assert!(!state.settings(&settings).enabled);
        state.passwords = 2;
        let system = state.settings(&settings);
        assert!(system.enabled && system.system_store);
        assert_eq!(system.storage, credentials::accounts::Storage::System);
    }
}
