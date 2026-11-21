//! What a page waits on the person for: a script dialog, a list's choices,
//! a server's request to sign in.
//!
//! The page that raised one is stopped until it is answered, and the answer
//! is sent to the view on screen. So a request belongs to the tab that raised
//! it: it is on show only while that tab is, nothing else takes its place
//! there, and when the person turns to another tab it goes back into its
//! tab's queue, to open again when the tab does. One answered from another
//! tab would reach no page, and its own would stay stopped for good.

use super::*;
use crate::credentials::Secret;
use enzui_webkit::page::{AuthAnswer, AuthChallenge, PageEvent, ScriptDialogKind};
use iced::widget::{Space, column, container, row, text};

#[derive(Debug, Clone)]
pub(crate) enum PromptCommand {
    /// Enter: the request's own answer, where it has one.
    Confirm,
    User(String),
    Password(Secret),
    /// Send what was typed, or refuse.
    SignIn(bool),
    /// An arrow key in a list of choices: the next or the one before.
    ListMove(i32),
    /// A letter typed in a list of choices: on to the next that starts so.
    ListType(char),
}

/// The height of one choice in a list, and of the list at most.
const CHOICE: f32 = 30.0;
const LIST: f32 = 330.0;

/// The choice a list opens on: the page's current one.
pub(super) fn list_start(options: &enzui_webkit::page::OptionMenuRequest) -> Option<usize> {
    options
        .items
        .iter()
        .position(|item| item.selected && item.enabled && !item.group_label)
}

/// The choosable item `step` away from `from`, staying at the ends.
fn list_step(
    options: &enzui_webkit::page::OptionMenuRequest,
    from: Option<usize>,
    step: i32,
) -> Option<usize> {
    let choosable = |index: &usize| {
        let item = &options.items[*index];
        item.enabled && !item.group_label
    };
    match (from, step > 0) {
        (None, true) => (0..options.items.len()).find(choosable),
        (None, false) => (0..options.items.len()).rev().find(choosable),
        (Some(from), true) => (from + 1..options.items.len()).find(choosable).or(Some(from)),
        (Some(from), false) => (0..from).rev().find(choosable).or(Some(from)),
    }
}

/// The next choosable item after `from` whose text starts with `letter`,
/// going round to the first again.
fn list_letter(
    options: &enzui_webkit::page::OptionMenuRequest,
    from: Option<usize>,
    letter: char,
) -> Option<usize> {
    let count = options.items.len();
    let start = from.map_or(0, |from| from + 1);
    (0..count)
        .map(|offset| (start + offset) % count)
        .find(|index| {
            let item = &options.items[*index];
            item.enabled
                && !item.group_label
                && item
                    .label
                    .trim_start()
                    .chars()
                    .next()
                    .is_some_and(|first| first.to_lowercase().eq(letter.to_lowercase()))
        })
}

/// Where a list of `rows` choices goes for a control at `control` (in the
/// window): under it, or over it where there is more room; as wide as the
/// control at least, and inside the window.
fn list_place(control: iced::Rectangle, rows: usize, window: iced::Size) -> iced::Rectangle {
    let margin = 8.0;
    let wanted = (rows as f32 * CHOICE + 8.0).min(LIST);
    let below = window.height - control.y - control.height - margin;
    let above = control.y - margin;
    let (y, height) = if below >= wanted || below >= above {
        (control.y + control.height, wanted.min(below.max(CHOICE)))
    } else {
        let height = wanted.min(above.max(CHOICE));
        (control.y - height, height)
    };
    let width = control
        .width
        .clamp(160.0, (window.width - 2.0 * margin).max(160.0));
    let x = control
        .x
        .clamp(margin, (window.width - width - margin).max(margin));
    iced::Rectangle {
        x,
        y: y.max(margin),
        width,
        height,
    }
}

/// A server's request to sign in and what the person has typed for it.
#[derive(Debug, Clone)]
pub(crate) struct SignIn {
    pub(crate) challenge: AuthChallenge,
    pub(crate) user: String,
    pub(crate) password: Secret,
}

impl SignIn {
    pub(crate) fn new(challenge: AuthChallenge) -> Self {
        Self {
            challenge,
            user: String::new(),
            password: Secret::default(),
        }
    }
}

/// Whether `event` asks for the person while another request of the tab is
/// on show (`showing`): it waits its turn rather than replace it unanswered.
/// A list of choices replaces a list, which the page has closed by then.
pub(super) fn waits(showing: bool, list_open: bool, event: &PageEvent) -> bool {
    match event {
        PageEvent::ScriptDialog(_) | PageEvent::Authentication(_) => showing,
        PageEvent::OptionMenu(_) => showing && !list_open,
        _ => false,
    }
}

/// The field a request's panel opens with the caret in.
pub(super) fn first_field(event: &PageEvent) -> Option<&'static str> {
    match event {
        PageEvent::ScriptDialog(dialog)
            if matches!(dialog.kind, ScriptDialogKind::Prompt { .. }) =>
        {
            Some("script-prompt")
        }
        PageEvent::Authentication(_) => Some("sign-in-user"),
        _ => None,
    }
}

impl App {
    fn prompt_panel(&self) -> Option<Panel> {
        if self.browser.dialog.is_some() {
            Some(Panel::Script)
        } else if self.browser.sign_in.is_some() {
            Some(Panel::SignIn)
        } else if self.browser.options.is_some() {
            Some(Panel::Options)
        } else {
            None
        }
    }

    /// Keeps a page's request with its tab; see the module's note. Runs
    /// after every message, before the next view is drawn.
    pub(crate) fn settle_page_prompts(&mut self) {
        let Some(owner) = self.browser.prompt_tab else {
            return;
        };
        let Some(panel) = self.prompt_panel() else {
            self.browser.prompt_tab = None;
            return;
        };
        let active = self
            .browser
            .saved
            .tabs
            .get(self.browser.saved.active)
            .map(|tab| tab.key);
        if active == Some(owner) && self.browser.library.is_none() {
            self.browser.panel = panel;
            return;
        }
        self.browser.prompt_tab = None;
        let mut waiting = Vec::new();
        if let Some(dialog) = self.browser.dialog.take() {
            waiting.push(PageEvent::ScriptDialog(dialog));
        }
        if let Some(sign_in) = self.browser.sign_in.take() {
            waiting.push(PageEvent::Authentication(sign_in.challenge));
        }
        if let Some(options) = self.browser.options.take() {
            waiting.push(PageEvent::OptionMenu(options));
        }
        if matches!(
            self.browser.panel,
            Panel::Script | Panel::SignIn | Panel::Options
        ) {
            self.browser.panel = Panel::None;
        }
        let host = if active == Some(owner) {
            Some(&self.webview)
        } else {
            self.browser
                .saved
                .tabs
                .iter()
                .position(|tab| tab.key == owner)
                .and_then(|index| self.browser.sleeping.get(index))
                .and_then(Option::as_ref)
                .map(|tab| &tab.host)
        };
        // A tab that is gone took its page with it; nothing waits there.
        if let Some(host) = host {
            host.borrow_mut().page_events.splice(0..0, waiting);
        }
    }

    pub(super) fn prompt_command(&mut self, command: PromptCommand) -> Task<Message> {
        match command {
            PromptCommand::Confirm => match self.browser.panel {
                Panel::Script => return self.browser_command(Command::AnswerDialog(true)),
                Panel::SignIn => return self.prompt_command(PromptCommand::SignIn(true)),
                Panel::Options => {
                    if let Some(index) = self.browser.option_highlight {
                        return self.browser_command(Command::AnswerOption(index));
                    }
                }
                Panel::BookmarkEdit => {
                    return self.bookmark_command(BookmarkCommand::Save);
                }
                Panel::Color => return self.color_command(ColorCommand::Choose),
                _ => {}
            },
            PromptCommand::ListMove(step) => {
                if let Some(options) = &self.browser.options {
                    self.browser.option_highlight =
                        list_step(options, self.browser.option_highlight, step);
                    return self.list_follow();
                }
            }
            PromptCommand::ListType(letter) => {
                if let Some(options) = &self.browser.options
                    && let Some(index) = list_letter(options, self.browser.option_highlight, letter)
                {
                    self.browser.option_highlight = Some(index);
                    return self.list_follow();
                }
            }
            PromptCommand::User(user) => {
                if let Some(sign_in) = &mut self.browser.sign_in {
                    sign_in.user = user;
                }
            }
            PromptCommand::Password(password) => {
                if let Some(sign_in) = &mut self.browser.sign_in {
                    sign_in.password = password;
                }
            }
            PromptCommand::SignIn(send) => {
                if let Some(sign_in) = self.browser.sign_in.take() {
                    let answer = if send {
                        AuthAnswer::Credentials {
                            user: sign_in.user,
                            password: enzui_security::secret::SecretText::new(
                                sign_in.password.expose().to_owned(),
                            ),
                        }
                    } else {
                        AuthAnswer::Cancel
                    };
                    self.submit_browser_action(BrowserAction::Authentication(
                        sign_in.challenge.id,
                        answer,
                    ));
                }
                self.browser.prompt_tab = None;
                self.browser.panel = Panel::None;
            }
        }
        Task::none()
    }

    /// Scrolls the list so that the highlighted choice is in it.
    fn list_follow(&self) -> Task<Message> {
        let Some(index) = self.browser.option_highlight else {
            return Task::none();
        };
        iced::advanced::widget::operate(
            iced::advanced::widget::operation::scrollable::scroll_to(
                iced::advanced::widget::Id::new("option-list"),
                iced::widget::scrollable::AbsoluteOffset {
                    x: Some(0.0),
                    y: Some((index as f32 * CHOICE - LIST / 2.0).max(0.0)),
                },
                iced::advanced::widget::operation::Animation::Instant,
            ),
        )
    }

    /// A page's list of choices, at the control it belongs to and over the
    /// page: a press beside it closes it.
    pub(super) fn option_list_over<'a>(&'a self, page: Element<'a, Message>) -> Element<'a, Message> {
        let (Some(options), Panel::Options) = (&self.browser.options, self.browser.panel) else {
            return page;
        };
        let surface = self
            .browser
            .input
            .bounds
            .map_or_else(
                || iced::Rectangle::with_size(self.browser.viewport),
                |(_, bounds)| bounds,
            );
        let control = iced::Rectangle {
            x: surface.x + options.area.x as f32,
            y: surface.y + options.area.y as f32,
            width: options.area.width as f32,
            height: options.area.height as f32,
        };
        let place = list_place(control, options.items.len(), self.browser.viewport);
        let p = self.look().palette.clone();
        let mut list = column![].padding(4);
        for (index, item) in options.items.iter().enumerate() {
            let inset = if item.group_child { 22.0 } else { 10.0 };
            let label = text(item.label.clone())
                .size(13)
                .wrapping(iced::widget::text::Wrapping::None);
            if item.group_label || !item.enabled {
                list = list.push(
                    container(label.color(p.text_muted))
                        .height(CHOICE)
                        .padding(iced::Padding::default().left(inset))
                        .align_y(iced::Alignment::Center),
                );
                continue;
            }
            let chosen = self.browser.option_highlight == Some(index);
            let (hover, ink) = (p.surface_hover, p.text);
            list = list.push(
                iced::widget::button(
                    container(label)
                        .height(Length::Fill)
                        .align_y(iced::Alignment::Center),
                )
                .on_press(c(Command::AnswerOption(index)))
                .width(Length::Fill)
                .height(CHOICE)
                .padding(iced::Padding::default().left(inset).right(10.0))
                .style(move |_, status| iced::widget::button::Style {
                    background: (chosen
                        || matches!(
                            status,
                            iced::widget::button::Status::Hovered
                                | iced::widget::button::Status::Pressed
                        ))
                    .then(|| hover.into()),
                    text_color: ink,
                    border: iced::Border {
                        radius: 6.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            );
        }
        let (ground, edge) = (p.surface_elevated, p.border);
        let list = container(
            enzui_widgets::scroll_view::enz_scrollable(list)
                .id(iced::advanced::widget::Id::new("option-list"))
                .height(Length::Fill),
        )
        .width(place.width)
        .height(place.height)
        .style(move |_| container::Style {
            background: Some(ground.into()),
            border: iced::Border {
                radius: 8.into(),
                width: 1.0,
                color: edge,
            },
            ..Default::default()
        });
        let beside = iced::widget::mouse_area(
            container(list)
                .padding(iced::Padding::default().left(place.x).top(place.y))
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .on_press(c(Command::Dismiss))
        .on_right_press(c(Command::Dismiss));
        iced::widget::stack![page, beside].into()
    }

    /// A script dialog, in the words that fit what it asks.
    pub(super) fn script_dialog_view(&self) -> Option<Element<'_, Message>> {
        let dialog = self.browser.dialog.as_ref()?;
        let site = site_name(&self.address);
        let mut body = column![].spacing(10);
        let (decline, accept) = match &dialog.kind {
            // What a page says as it is left is not shown: a page could say
            // anything to keep the person on it.
            ScriptDialogKind::BeforeUnload => {
                body = body
                    .push(heading("log-out", "Leave this page?", site))
                    .push(text("Changes you made may not be saved.").size(14));
                (Some(("Stay", "x")), ("Leave", "log-out"))
            }
            kind => {
                body = body
                    .push(heading("message-square", site, "Message from this page"))
                    .push(text(dialog.message.clone()).size(14));
                if matches!(kind, ScriptDialogKind::Prompt { .. }) {
                    body = body.push(field(
                        "Response",
                        &self.browser.prompt,
                        |s| c(Command::PromptText(s)),
                        "script-prompt",
                    ));
                }
                // An alert only tells; there is nothing to decline.
                let decline =
                    (!matches!(kind, ScriptDialogKind::Alert)).then_some(("Cancel", "x"));
                (decline, ("OK", "check"))
            }
        };
        let mut actions = row![Space::new().width(Length::Fill)].spacing(8);
        if let Some((label, glyph)) = decline {
            actions = actions.push(action(label, glyph, Command::AnswerDialog(false)));
        }
        actions = actions.push(primary(accept.0, accept.1, Command::AnswerDialog(true)));
        Some(body.push(actions).into())
    }

    /// A server's request to sign in. The realm is the server's own text and
    /// is shown as such, under the host the browser itself vouches for.
    pub(super) fn sign_in_view(&self) -> Option<Element<'_, Message>> {
        let sign_in = self.browser.sign_in.as_ref()?;
        let challenge = &sign_in.challenge;
        let host = format!("{}:{}", challenge.host, challenge.port);
        let mut body = column![heading("key-round", "Sign in", host)].spacing(10);
        let realm = challenge.realm.trim();
        if !realm.is_empty() {
            body = body.push(text(format!("The site says: “{realm}”")).size(13));
        }
        if challenge.retry {
            body = body.push(text("That user name and password were not accepted.").size(13));
        }
        let prompt = |command| c(Command::Prompt(command));
        body = body
            .push(field(
                "User name",
                &sign_in.user,
                move |user| prompt(PromptCommand::User(user)),
                "sign-in-user",
            ))
            .push(
                enzui_widgets::text_field::enz_text_field(
                    "",
                    "Password",
                    sign_in.password.expose(),
                    move |password| prompt(PromptCommand::Password(Secret::new(password))),
                )
                .id(iced::advanced::widget::Id::new("sign-in-password"))
                .secure(true)
                .into_element(),
            )
            .push(
                row![
                    Space::new().width(Length::Fill),
                    action(
                        "Cancel",
                        "x",
                        Command::Prompt(PromptCommand::SignIn(false))
                    ),
                    primary(
                        "Sign in",
                        "check",
                        Command::Prompt(PromptCommand::SignIn(true))
                    )
                ]
                .spacing(8),
            );
        Some(body.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use enzui_webkit::page::{PageRequestId, ScriptDialogRequest};

    fn two_tabs() -> App {
        let mut app = browser_tests::app();
        app.browser_mode = true;
        for (index, url) in ["https://one.example/", "https://two.example/"]
            .into_iter()
            .enumerate()
        {
            if index == 1 {
                let mut tab = app.browser.saved.tabs[0].clone();
                tab.key = next_tab_key();
                app.browser.saved.tabs.push(tab);
                app.browser.sleeping.push(Some(SleepingTab {
                    host: RefCell::new(WebViewService::new()),
                    pdf: None,
                }));
            }
            app.browser.saved.tabs[index].url = url.into();
        }
        app.address = "https://one.example/".into();
        app
    }

    fn alert(id: u64) -> PageEvent {
        PageEvent::ScriptDialog(ScriptDialogRequest {
            id: PageRequestId::new(id),
            kind: ScriptDialogKind::Alert,
            message: "said by the page".into(),
        })
    }

    fn waiting_dialogs(host: &RefCell<WebViewService>) -> usize {
        host.borrow()
            .page_events
            .iter()
            .filter(|event| matches!(event, PageEvent::ScriptDialog(_)))
            .count()
    }

    #[test]
    fn a_pages_dialog_stays_with_the_tab_that_raised_it() {
        let mut app = two_tabs();
        let first = app.browser.saved.tabs[0].key;
        app.webview.borrow_mut().page_events.push(alert(7));
        app.sync_browser();
        assert_eq!(app.browser.panel, Panel::Script);
        assert_eq!(app.browser.prompt_tab, Some(first));

        // Another panel does not take the place of a dialog its page waits on.
        let _ = app.update(c(Command::Panel(Panel::Downloads)));
        assert_eq!(app.browser.panel, Panel::Script);

        // Turning to the other tab leaves the dialog with the first one.
        let _ = app.update(c(Command::Cycle(false)));
        assert_eq!(app.browser.saved.active, 1);
        assert_eq!(app.browser.panel, Panel::None);
        assert!(app.browser.dialog.is_none());
        let parked = &app.browser.sleeping[0].as_ref().unwrap().host;
        assert_eq!(waiting_dialogs(parked), 1);
        // Nothing can be answered from here.
        let _ = app.update(c(Command::Prompt(PromptCommand::Confirm)));
        assert_eq!(
            waiting_dialogs(&app.browser.sleeping[0].as_ref().unwrap().host),
            1
        );

        // Back on its tab it is on show again, and Enter answers it.
        let _ = app.update(c(Command::Cycle(false)));
        assert_eq!(app.browser.saved.active, 0);
        assert_eq!(app.browser.panel, Panel::Script);
        assert_eq!(
            app.browser.dialog.as_ref().map(|dialog| dialog.id),
            Some(PageRequestId::new(7))
        );
        let _ = app.update(c(Command::Prompt(PromptCommand::Confirm)));
        assert!(app.browser.dialog.is_none());
        assert_eq!(app.browser.prompt_tab, None);
        assert_eq!(app.browser.panel, Panel::None);
        assert_eq!(waiting_dialogs(&app.webview), 0);
    }

    #[test]
    fn a_second_request_waits_until_the_first_is_answered() {
        let mut app = two_tabs();
        app.webview
            .borrow_mut()
            .page_events
            .extend([alert(1), alert(2)]);
        app.sync_browser();
        assert_eq!(
            app.browser.dialog.as_ref().map(|dialog| dialog.id),
            Some(PageRequestId::new(1))
        );
        assert_eq!(waiting_dialogs(&app.webview), 1);
        let _ = app.update(c(Command::AnswerDialog(true)));
        assert_eq!(
            app.browser.dialog.as_ref().map(|dialog| dialog.id),
            Some(PageRequestId::new(2))
        );
    }

    #[test]
    fn a_sign_in_request_is_asked_and_answered_once() {
        let mut app = two_tabs();
        let challenge = AuthChallenge {
            id: PageRequestId::new(3),
            host: "one.example".into(),
            port: 443,
            realm: "staff".into(),
            scheme: enzui_webkit::page::AuthScheme::Basic,
            retry: false,
        };
        app.webview
            .borrow_mut()
            .page_events
            .push(PageEvent::Authentication(challenge));
        app.sync_browser();
        assert_eq!(app.browser.panel, Panel::SignIn);
        let _ = app.update(c(Command::Prompt(PromptCommand::User("me".into()))));
        let _ = app.update(c(Command::Prompt(PromptCommand::Password(Secret::new(
            "word".into(),
        )))));
        assert_eq!(app.browser.sign_in.as_ref().unwrap().user, "me");
        // Escape refuses rather than leave the page waiting.
        let _ = app.update(c(Command::Escape));
        assert!(app.browser.sign_in.is_none());
        assert_eq!(app.browser.panel, Panel::None);
        assert_eq!(app.browser.prompt_tab, None);
    }

    fn choices(labels: &[&str]) -> enzui_webkit::page::OptionMenuRequest {
        enzui_webkit::page::OptionMenuRequest {
            id: PageRequestId::new(1),
            items: labels
                .iter()
                .map(|label| enzui_webkit::page::OptionMenuItem {
                    label: label.trim_start_matches(['#', '-']).to_owned(),
                    tooltip: None,
                    group_label: label.starts_with('#'),
                    group_child: false,
                    enabled: !label.starts_with('-'),
                    selected: *label == "Belgium",
                })
                .collect(),
            area: enzui_webkit::page::ViewRect {
                x: 0,
                y: 0,
                width: 100,
                height: 20,
            },
        }
    }

    #[test]
    fn a_list_is_walked_with_the_arrows_and_by_first_letters() {
        let list = choices(&["#Europe", "Austria", "Belgium", "-Closed", "Bulgaria", "Croatia"]);
        assert_eq!(list_start(&list), Some(2));
        // Headings and choices that cannot be made are stepped over.
        assert_eq!(list_step(&list, Some(2), 1), Some(4));
        assert_eq!(list_step(&list, Some(4), -1), Some(2));
        assert_eq!(list_step(&list, Some(1), -1), Some(1));
        assert_eq!(list_step(&list, Some(5), 1), Some(5));
        assert_eq!(list_step(&list, None, 1), Some(1));
        assert_eq!(list_step(&list, None, -1), Some(5));
        // A letter goes on to the next choice that starts with it, and round.
        assert_eq!(list_letter(&list, Some(2), 'b'), Some(4));
        assert_eq!(list_letter(&list, Some(4), 'B'), Some(2));
        assert_eq!(list_letter(&list, Some(2), 'c'), Some(5));
        assert_eq!(list_letter(&list, Some(2), 'z'), None);
    }

    #[test]
    fn a_list_opens_at_its_control_and_stays_in_the_window() {
        let window = iced::Size::new(800.0, 600.0);
        let control = |x, y| iced::Rectangle {
            x,
            y,
            width: 120.0,
            height: 24.0,
        };
        // Under the control, as wide as it is at least.
        let under = list_place(control(300.0, 100.0), 5, window);
        assert_eq!((under.x, under.y), (300.0, 124.0));
        assert_eq!((under.width, under.height), (160.0, 158.0));
        // Over it near the bottom, where there is more room.
        let over = list_place(control(300.0, 560.0), 5, window);
        assert_eq!(over.y + over.height, 560.0);
        // A long list is as high as the room allows, and scrolls.
        let long = list_place(control(300.0, 100.0), 200, window);
        assert_eq!(long.height, LIST);
        // At the right edge it is moved in.
        let edge = list_place(control(760.0, 100.0), 5, window);
        assert_eq!(edge.x + edge.width, 792.0);
    }

    #[test]
    fn a_request_waits_its_turn_and_opens_in_its_field() {
        assert!(waits(true, false, &alert(1)));
        assert!(!waits(false, false, &alert(1)));
        assert_eq!(first_field(&alert(1)), None);
        let prompt = PageEvent::ScriptDialog(ScriptDialogRequest {
            id: PageRequestId::new(2),
            kind: ScriptDialogKind::Prompt {
                default_text: String::new(),
            },
            message: String::new(),
        });
        assert_eq!(first_field(&prompt), Some("script-prompt"));
    }
}
