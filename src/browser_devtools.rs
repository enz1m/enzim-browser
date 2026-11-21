use super::*;
use enzui_widgets::scroll_view::enz_scrollable as scrollable;
use iced::widget::{Space, column, container, row, text};

#[derive(Clone, Debug)]
pub(crate) enum DevCommand {
    Toggle,
    Console,
    Tab(usize),
    Input(String),
    Selector(String),
    Filter(String),
    Level(usize),
    Keep(bool),
    History(i32),
    ResizeStart,
    ResizeEnd,
    Run,
    Clear,
    Copy(usize),
    Ready(
        u64,
        u64,
        String,
        Result<crate::native_jobs::Payload, String>,
    ),
}
struct ConsoleEntry {
    input: String,
    output: String,
    level: String,
    command: bool,
}
#[derive(Default)]
pub(crate) struct DevTools {
    pub(crate) open: bool,
    pub(super) tab: usize,
    pub(super) inspector: super::inspector::Inspector,
    pub(crate) resizing: bool,
    pub(crate) height: Option<f32>,
    pub(super) keep_log: bool,
    input: String,
    filter: String,
    level: usize,
    evaluation: Option<u64>,
    history: Vec<String>,
    history_cursor: Option<usize>,
    draft: String,
    entries: Vec<ConsoleEntry>,
    pub(super) network: super::network::Network,
}
impl DevTools {
    fn push(&mut self, entry: ConsoleEntry) {
        if self.entries.len() >= 100 {
            self.entries.remove(0);
        }
        self.entries.push(entry);
    }
    fn recall(&mut self, direction: i32) {
        if self.history.is_empty() {
            return;
        }
        let next = if direction < 0 {
            if self.history_cursor.is_none() {
                self.draft.clone_from(&self.input);
            }
            Some(
                self.history_cursor
                    .unwrap_or(self.history.len())
                    .saturating_sub(1),
            )
        } else {
            self.history_cursor
                .and_then(|index| (index + 1 < self.history.len()).then_some(index + 1))
        };
        self.history_cursor = next;
        self.input = next.map_or_else(|| self.draft.clone(), |index| self.history[index].clone());
    }
    fn matches(&self, entry: &ConsoleEntry) -> bool {
        let level = match self.level {
            1 => entry.level == "error",
            2 => entry.level == "warn",
            3 => entry.level != "error" && entry.level != "warn",
            _ => true,
        };
        let query = self.filter.to_lowercase();
        level
            && (query.is_empty()
                || entry.input.to_lowercase().contains(&query)
                || entry.output.to_lowercase().contains(&query))
    }
}
impl App {
    pub(crate) fn clear_devtools_document(&mut self) {
        if !self.browser.devtools.keep_log {
            self.browser.devtools.entries.clear();
        }
        self.browser.devtools.evaluation = None;
        self.browser.devtools.network = Default::default();
        self.browser.devtools.inspector = Default::default();
    }
    pub(crate) fn reset_devtools_tab(&mut self) {
        self.browser.devtools.entries.clear();
        self.clear_devtools_document();
    }
    pub(super) fn console_message(&mut self, json: &str) {
        if !self.browser.devtools.open {
            return;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
            return;
        };
        let Some(message) = value["text"].as_str() else {
            return;
        };
        let level = value["level"]
            .as_str()
            .unwrap_or("log")
            .chars()
            .take(12)
            .collect();
        self.browser.devtools.push(ConsoleEntry {
            input: site_name(&self.browser.saved.tabs[self.browser.saved.active].url),
            output: message.chars().take(4000).collect(),
            level,
            command: false,
        });
    }
    pub(super) fn dev_command(&mut self, command: DevCommand) -> Task<Message> {
        use DevCommand::*;
        match command {
            Toggle => {
                self.cancel_inspector_picker();
                self.browser.devtools.open = !self.browser.devtools.open;
                self.browser.devtools.resizing = false;
                if !self.browser.devtools.open {
                    self.browser.devtools.inspector = Default::default();
                }
                self.submit_browser_action(BrowserAction::DeveloperTools(
                    self.browser.devtools.open,
                ));
                if self.browser.devtools.open && self.browser.devtools.tab == 0 {
                    return focus("dev-console");
                }
            }
            Console => {
                self.browser.devtools.open = true;
                self.submit_browser_action(BrowserAction::DeveloperTools(true));
                self.browser.devtools.tab = 0;
                return focus("dev-console");
            }
            Tab(tab) => {
                self.browser.devtools.tab = tab.min(2);
                if tab == 1 && self.browser.devtools.inspector.element.is_none() {
                    return self.inspector_command(super::inspector::InspectCommand::Refresh);
                }
            }
            Input(value) => {
                self.browser.devtools.input = value.chars().take(32768).collect();
                self.browser.devtools.history_cursor = None;
            }
            Selector(value) => self.browser.devtools.inspector.selector = value,
            Filter(value) => self.browser.devtools.filter = value,
            Level(value) => self.browser.devtools.level = value.min(3),
            Keep(value) => self.browser.devtools.keep_log = value,
            History(direction) => self.browser.devtools.recall(direction),
            ResizeStart => self.browser.devtools.resizing = true,
            ResizeEnd => self.browser.devtools.resizing = false,
            Clear => self.browser.devtools.entries.clear(),
            Copy(index) => {
                if let Some(entry) = self.browser.devtools.entries.get(index) {
                    return iced::clipboard::write(entry.output.clone())
                        .map(|_| c(Command::MaintenancePoll));
                }
            }
            Run => {
                let state = &mut self.browser.devtools;
                if state.evaluation.is_some() || state.input.trim().is_empty() {
                    return Task::none();
                }
                let input = state.input.clone();
                if state.history.last() != Some(&input) {
                    if state.history.len() >= 50 {
                        state.history.remove(0);
                    }
                    state.history.push(input.clone());
                }
                state.history_cursor = None;
                state.draft.clear();
                let (job, receiver) = crate::native_jobs::Job::new();
                let id = job.id;
                state.evaluation = Some(id);
                let key = self.browser.saved.tabs[self.browser.saved.active].key;
                self.submit_browser_action(BrowserAction::Console {
                    source: input.clone(),
                    job,
                });
                return Task::perform(crate::native_jobs::receive(receiver), move |result| {
                    c(Command::Dev(Ready(key, id, input, result)))
                });
            }
            Ready(key, id, input, result) => {
                if self.browser.devtools.evaluation != Some(id) {
                    return Task::none();
                }
                self.browser.devtools.evaluation = None;
                if self.browser.saved.tabs[self.browser.saved.active].key != key {
                    return Task::none();
                }
                let (output, error) = console_output(result);
                self.browser.devtools.push(ConsoleEntry {
                    input,
                    output,
                    level: if error { "error" } else { "result" }.into(),
                    command: true,
                });
                self.browser.devtools.input.clear();
                return iced::widget::operation::snap_to_end(
                    "browser-console-log",
                    iced::widget::operation::Animation::Instant,
                );
            }
        }
        Task::none()
    }
    pub(crate) fn devtools_height(&self) -> f32 {
        self.browser
            .devtools
            .height
            .unwrap_or(self.browser.viewport.height * 0.42)
            .clamp(190., (self.browser.viewport.height - 140.).max(190.))
    }
    pub(super) fn devtools_panel(&self) -> Element<'_, Message> {
        let state = &self.browser.devtools;
        let mut tokens = enzui_widgets::view_switcher::SwitcherTokens::INLINE_SEGMENTED;
        tokens.strip_radius = 0.;
        tokens.tab_radius = 0.;
        tokens.tab_height = 28.;
        tokens.strip_height = 28.;
        tokens.tab_padding_x = 10.;
        tokens.label_size = 12.;
        let tabs = enzui_widgets::inline_view_switcher::enz_inline_view_switcher(
            vec![
                enzui_widgets::inline_view_switcher::InlineSwitcherPage::new("console", "Console")
                    .with_icon("terminal"),
                enzui_widgets::inline_view_switcher::InlineSwitcherPage::new(
                    "elements", "Elements",
                )
                .with_icon("code"),
                enzui_widgets::inline_view_switcher::InlineSwitcherPage::new("network", "Network")
                    .with_icon("network"),
            ],
            state.tab,
            |index| c(Command::Dev(DevCommand::Tab(index))),
        )
        .tab_id_prefix("devtools")
        .tokens(tokens)
        .selection_indicator(enzui_widgets::tab_pill::TabSelectionStyle::TranslucentAccent)
        .into_element();
        let handle = iced::widget::mouse_area(
            container(iced::widget::rule::horizontal(1))
                .height(5)
                .center_y(5),
        )
        .on_press(c(Command::Dev(DevCommand::ResizeStart)))
        .interaction(iced::mouse::Interaction::ResizingVertically);
        let header = container(
            row![
                enzui_widgets::toolbar_icon::enz_toolbar_icon(
                    "mouse-pointer-2",
                    c(Command::Inspect(if state.inspector.picking {
                        super::inspector::InspectCommand::Cancel
                    } else {
                        super::inspector::InspectCommand::Pick
                    }))
                )
                .aria_label("Pick an element")
                .tooltip("Pick an element")
                .size(28., 14.)
                .active(state.inspector.picking)
                .into_element(),
                tabs,
                Space::new().width(Length::Fill),
                ink_icon(
                    "x",
                    "Close developer tools",
                    c(Command::Dev(DevCommand::Toggle)),
                    28.,
                    14.
                )
            ]
            .align_y(iced::Alignment::Center)
            .spacing(4),
        )
        .padding([0, 6]);
        let body = match state.tab {
            1 => self.inspector_panel(),
            2 => self.network_panel(),
            _ => self.console_panel(),
        };
        container(
            column![
                handle,
                header,
                iced::widget::rule::horizontal(1),
                container(body).padding(6).height(Length::Fill)
            ]
            .spacing(0),
        )
        .height(self.devtools_height())
        .width(Length::Fill)
        .style(|theme: &iced::Theme| container::Style {
            background: Some(palette(theme).surface.into()),
            ..Default::default()
        })
        .into()
    }
    fn console_panel(&self) -> Element<'_, Message> {
        let state = &self.browser.devtools;
        let filter = enzui_widgets::text_field::enz_text_field(
            "",
            "Filter console",
            &state.filter,
            |value| c(Command::Dev(DevCommand::Filter(value))),
        )
        .leading(glyph("search", 12.))
        .quiet(true)
        .size(enzui_widgets::text_field::TextFieldSize::Sm)
        .aria_label("Filter console messages")
        .into_element();
        let levels = vec![
            "All levels".into(),
            "Errors".into(),
            "Warnings".into(),
            "Info".into(),
        ];
        let level = enzui_widgets::combo_box::enz_combo_box(
            "",
            levels.clone(),
            Some(levels[state.level].clone()),
            |_| c(Command::Dev(DevCommand::Level(0))),
        )
        .id_prefix("console-level")
        .aria_label("Console message level")
        .selected_index(Some(state.level))
        .on_select_index(|index| c(Command::Dev(DevCommand::Level(index))))
        .width(104)
        .into_element();
        let keep = iced::widget::checkbox(state.keep_log)
            .label("Keep log")
            .size(12)
            .text_size(11)
            .on_toggle(|value| c(Command::Dev(DevCommand::Keep(value))));
        let toolbar = row![
            filter,
            level,
            keep,
            ink_icon(
                "ban",
                "Clear console",
                c(Command::Dev(DevCommand::Clear)),
                26.,
                13.
            )
        ]
        .align_y(iced::Alignment::Center)
        .spacing(6);
        let mut log = column![].spacing(0);
        let mut matched = 0;
        for (index, entry) in state
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| state.matches(entry))
        {
            matched += 1;
            let p = &self.look().palette;
            let error = entry.level == "error";
            let warning = entry.level == "warn";
            let ink = if error {
                p.danger
            } else if warning {
                p.warning
            } else {
                p.text
            };
            let mut lines = column![].spacing(3);
            if entry.command {
                lines = lines.push(
                    text(entry.input.clone())
                        .size(11)
                        .font(iced::Font::MONOSPACE)
                        .wrapping(iced::widget::text::Wrapping::None)
                        .ellipsis(iced::widget::text::Ellipsis::End)
                        .width(Length::Fill),
                );
            }
            lines = lines.push(
                text(entry.output.clone())
                    .size(12)
                    .font(iced::Font::MONOSPACE)
                    .color(ink),
            );
            log = log
                .push(
                    container(
                        row![
                            container(glyph(
                                if error {
                                    "circle-alert"
                                } else if warning {
                                    "triangle-alert"
                                } else if entry.command {
                                    "chevron-left"
                                } else {
                                    "chevron-right"
                                },
                                12.
                            ))
                            .width(16),
                            lines.width(Length::Fill),
                            ink_icon(
                                "copy",
                                "Copy result",
                                c(Command::Dev(DevCommand::Copy(index))),
                                24.,
                                12.
                            )
                        ]
                        .spacing(5),
                    )
                    .padding([5, 4]),
                )
                .push(iced::widget::rule::horizontal(1));
        }
        let output: Element<'_, Message> = if matched == 0 {
            container(
                column![
                    glyph("terminal", 22.),
                    text(if state.entries.is_empty() {
                        "Console is ready"
                    } else {
                        "No matching messages"
                    })
                    .size(13),
                    text(if state.entries.is_empty() {
                        "Enter an expression, or await a promise."
                    } else {
                        "Change the filter or message level to see more."
                    })
                    .size(11)
                ]
                .spacing(8)
                .align_x(iced::Alignment::Center),
            )
            .center(Length::Fill)
            .into()
        } else {
            enzui_widgets::tail_follow::enz_tail_follow(
                scrollable(log)
                    .id("browser-console-log")
                    .height(Length::Fill),
            )
        };
        let input = enzui_widgets::text_field::enz_text_field(
            "",
            "JavaScript expression",
            &state.input,
            |value| c(Command::Dev(DevCommand::Input(value))),
        )
        .id(iced::advanced::widget::Id::new("dev-console"))
        .aria_label("Console JavaScript")
        .leading(glyph("chevron-right", 13.))
        .quiet(true)
        .subtle_focus(true)
        .on_submit(c(Command::Dev(DevCommand::Run)))
        .into_element();
        let input =
            enzui_widgets::keyboard_capture::keyboard_capture_any_focus(input, |key, modifiers| {
                if modifiers.control() || modifiers.alt() || modifiers.shift() {
                    return None;
                }
                match key {
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowUp) => {
                        Some(c(Command::Dev(DevCommand::History(-1))))
                    }
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowDown) => {
                        Some(c(Command::Dev(DevCommand::History(1))))
                    }
                    _ => None,
                }
            });
        column![
            toolbar,
            output,
            iced::widget::rule::horizontal(1),
            row![
                input,
                enzui_widgets::toolbar_icon::enz_toolbar_icon(
                    "play",
                    c(Command::Dev(DevCommand::Run))
                )
                .aria_label(if state.evaluation.is_some() {
                    "Evaluating JavaScript"
                } else {
                    "Run JavaScript"
                })
                .tooltip("Run JavaScript")
                .enabled(state.evaluation.is_none())
                .size(28., 14.)
                .into_element()
            ]
            .align_y(iced::Alignment::Center)
            .spacing(4)
        ]
        .spacing(4)
        .into()
    }
}
fn console_output(result: Result<crate::native_jobs::Payload, String>) -> (String, bool) {
    match result {
        Ok(crate::native_jobs::Payload::Json(mut value)) => {
            if let Some(preview) = value.get("preview").and_then(serde_json::Value::as_str) {
                let output = preview.to_owned();
                value = serde_json::Value::String(
                    if value.get("kind").and_then(serde_json::Value::as_str) == Some("json") {
                        serde_json::from_str::<serde_json::Value>(&output)
                            .ok()
                            .and_then(|value| serde_json::to_string_pretty(&value).ok())
                            .unwrap_or(output)
                    } else {
                        output
                    },
                );
            }
            (
                match value {
                    serde_json::Value::String(value) => value,
                    value => serde_json::to_string_pretty(&value).unwrap_or_default(),
                },
                false,
            )
        }
        Ok(_) => ("Unexpected page result.".into(), true),
        Err(error) => (error, true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn devtools_history_restores_the_draft_and_filters_errors_without_losing_results() {
        let mut tools = DevTools {
            input: "draft".into(),
            history: vec!["one".into(), "two".into()],
            ..Default::default()
        };
        tools.recall(-1);
        assert_eq!(tools.input, "two");
        tools.recall(-1);
        assert_eq!(tools.input, "one");
        tools.recall(1);
        assert_eq!(tools.input, "two");
        tools.recall(1);
        assert_eq!(tools.input, "draft");
        let error = ConsoleEntry {
            input: "test".into(),
            output: "missing value".into(),
            level: "error".into(),
            command: true,
        };
        tools.filter = "VALUE".into();
        tools.level = 1;
        assert!(tools.matches(&error));
        tools.level = 2;
        assert!(!tools.matches(&error));
        tools.level = 1;
        tools.filter = "other".into();
        assert!(!tools.matches(&error));
    }
    #[test]
    fn devtools_stale_evaluation_cannot_complete_a_replacement_job() {
        let mut app = super::super::browser_tests::app();
        let key = app.browser.saved.tabs[0].key;
        app.browser.devtools.evaluation = Some(42);
        let _ = app.dev_command(DevCommand::Ready(
            key,
            41,
            "old".into(),
            Err("old result".into()),
        ));
        assert_eq!(app.browser.devtools.evaluation, Some(42));
        assert!(app.browser.devtools.entries.is_empty());
        let _ = app.dev_command(DevCommand::Ready(
            key,
            42,
            "current".into(),
            Ok(crate::native_jobs::Payload::Json(
                serde_json::json!({"answer":42}),
            )),
        ));
        assert!(app.browser.devtools.evaluation.is_none());
        assert_eq!(app.browser.devtools.entries.len(), 1);
        app.browser.devtools.keep_log = true;
        app.clear_devtools_document();
        assert_eq!(app.browser.devtools.entries.len(), 1);
        app.reset_devtools_tab();
        assert!(app.browser.devtools.entries.is_empty());
        app.console_message(r#"{"level":"log","text":"closed page"}"#);
        assert!(app.browser.devtools.entries.is_empty());
    }
}
