//! The assistant's panel and its settings.

use super::{Body, CHATGPT, Change, Choice, Decision, DraftField, Msg, Row, ToolStatus, m};
use crate::Message;
use enzui_theme::Palette;
use enzui_widgets::{
    button::{ButtonKind, ButtonSize, enz_button},
    code_block::enz_code_block,
    composer::enz_composer,
    icon::enz_icon,
    markdown::enz_markdown_blocks,
    progress_text::enz_progress_text,
    pressable::{PressableStyle, enz_pressable},
    spinner::enz_spinner,
    tail_follow::enz_tail_follow,
    text::{TextSize, enz_text},
    toolbar_icon::enz_toolbar_icon,
};
use iced::{
    Alignment, Color, Element, Length, Padding,
    font::Weight,
    widget::{column, container, mouse_area, row, space, stack},
};

pub(crate) const COMPOSER: &str = "assistant-composer";
pub(crate) const TRANSCRIPT: &str = "assistant-transcript";
pub(crate) const SERVICE_NAME: &str = "assistant-service-name";
pub(crate) const MODEL_FILTER: &str = "assistant-model-filter";
pub(crate) const PROMPT_EDITOR: &str = "assistant-prompt-editor";
/// The model menu's two columns, and how tall its list grows before it scrolls.
const SERVICES_WIDTH: f32 = 190.0;
const MODELS_WIDTH: f32 = 270.0;
const MODELS_HEIGHT: f32 = 300.0;
/// Below this window width the panel stands in for the page.
const NARROW: f32 = 760.0;
const BODY: f32 = 13.0;
const SMALL: f32 = 12.0;
const TINY: f32 = 11.0;

fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::from_rgba(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

fn seconds(since: std::time::Duration) -> String {
    let s = since.as_secs();
    format!("{}:{:02}", s / 60, s % 60)
}

/// Where a row stands in the transcript.
#[derive(Clone, Copy)]
struct Place {
    /// A turn is running.
    running: bool,
    /// The transcript's last row.
    last: bool,
    /// The first row of an opened group, whose toggle is the group's.
    group_head: bool,
    /// The last reply of its turn, which gets the copy button.
    final_reply: bool,
}

fn chip<'a>(label: String, message: Message) -> Element<'a, Message> {
    enz_pressable(
        container(enz_text(label).size_px(SMALL).wrap(true).into_element())
            .padding([5, 10])
            .into(),
        message,
    )
    .style(PressableStyle::Chip)
    .width(Length::Shrink)
    .into_element()
}

impl crate::App {
    /// The toolbar's button that opens and closes the panel.
    pub(crate) fn assistant_button(&self) -> Element<'_, Message> {
        if !self.assistant_offered() {
            return space().width(0).into();
        }
        enz_toolbar_icon("sparkles", m(Msg::Toggle))
            .aria_label("Assistant")
            .tooltip("Assistant (Ctrl+Shift+A)")
            .expanded(self.assistant.open)
            .into_element()
    }

    /// The page frame with the panel beside it, on the side away from the
    /// tabs.
    ///
    /// While the assistant is offered the frame always stands in the same row,
    /// so opening and closing the panel slide it in and out beside the page
    /// without rebuilding the page's widgets; a closed panel is an empty
    /// slot.
    pub(crate) fn with_assistant<'a>(&'a self, frame: Element<'a, Message>) -> Element<'a, Message> {
        if !self.assistant_offered() {
            return frame;
        }
        let open = self.assistant.open;
        let chat = self.assistant.chat.as_deref().filter(|_| open || self.assistant.closing);
        // A window too narrow for both shows the panel in the page's place
        // until it is closed.
        if open && self.browser.viewport.width < NARROW
            && let Some(chat) = chat
        {
            return container(self.assistant_panel(chat))
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
        }
        let right = self.assistant_on_right();
        let slot: Element<'a, Message> = match chat {
            Some(chat) => {
                let most = (self.browser.viewport.width * 0.6).clamp(super::MIN_WIDTH, super::MAX_WIDTH);
                let width = self.browser.saved.assistant.width.clamp(super::MIN_WIDTH, most);
                let handle = mouse_area(container(space()).width(8).height(Length::Fill))
                    .on_press(m(Msg::ResizeStart))
                    .interaction(iced::mouse::Interaction::ResizingHorizontally);
                let panel = container(self.assistant_panel(chat)).width(width).height(Length::Fill);
                if right {
                    row![handle, panel].height(Length::Fill).into()
                } else {
                    row![panel, handle].height(Length::Fill).into()
                }
            }
            None => space().width(0).into(),
        };
        let slot = enzui_widgets::reveal_width::reveal_width(slot, open);
        if right {
            row![frame, slot].height(Length::Fill).into()
        } else {
            row![slot, frame].height(Length::Fill).into()
        }
    }

    fn assistant_panel<'a>(&'a self, chat: &'a super::Chat) -> Element<'a, Message> {
        let p = &self.look().palette;
        let listing = self.assistant.history.is_some();
        let header = row![
            enz_icon("sparkles").size(15.0).tint(p.accent).into_element(),
            enz_text(if listing { "Chats" } else { "Assistant" }).size_px(14.0).weight(Weight::Semibold).into_element(),
            space().width(Length::Fill),
            enz_toolbar_icon("square-pen", m(Msg::NewChat))
                .aria_label("New chat")
                .tooltip("New chat")
                .into_element(),
            enz_toolbar_icon("history", m(Msg::History))
                .aria_label("Chats")
                .tooltip(if listing { "Back to the chat" } else { "Chats" })
                .expanded(listing)
                .into_element(),
            enz_toolbar_icon("settings", m(Msg::OpenSettings))
                .aria_label("Assistant settings")
                .tooltip("Settings")
                .into_element(),
            enz_toolbar_icon("x", m(Msg::Close))
                .aria_label("Close the assistant")
                .tooltip("Close")
                .into_element(),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        let body: Element<'a, Message> = if let Some(chats) = &self.assistant.history {
            self.assistant_history(chats, chat, p)
        } else if chat.rows.is_empty() {
            self.assistant_welcome(p)
        } else {
            self.assistant_transcript(chat, p)
        };
        column![
            container(header).padding(Padding { top: 0.0, right: 2.0, bottom: 6.0, left: 6.0 }),
            container(body).height(Length::Fill),
            self.assistant_composer(chat, p),
        ]
        .spacing(4)
        .height(Length::Fill)
        .into()
    }

    fn assistant_welcome<'a>(&'a self, p: &Palette) -> Element<'a, Message> {
        let muted = p.text_muted;
        let ready = self.assistant_ready();
        let mut content = column![
            enz_icon("sparkles").size(28.0).tint(p.accent).into_element(),
            enz_text(if ready { "Ask about this page or anything else." } else { "Set up the assistant" })
                .size_px(15.0)
                .weight(Weight::Semibold)
                .into_element(),
        ]
        .spacing(10)
        .align_x(Alignment::Center);
        if ready {
            content = content.push(
                enz_text(match self.browser.saved.assistant.access {
                    super::Access::Full => "It can read and use this tab. It asks before reading other tabs.",
                    super::Access::Moderate => "It can read and use this tab. It asks before reading other tabs or opening, closing or switching tabs.",
                    super::Access::Restricted => "It can read this tab. It asks before reading other tabs or acting on a page.",
                })
                    .size_px(SMALL)
                    .color(muted)
                    .align(enzui_widgets::text::TextAlign::Center)
                    .into_element(),
            );
        } else if self.assistant.signing_in.is_some() {
            content = content.push(
                enz_text("Finish signing in on the page that opened.")
                    .size_px(SMALL)
                    .color(muted)
                    .align(enzui_widgets::text::TextAlign::Center)
                    .into_element(),
            );
            content = content.push(
                enz_button("Cancel", m(Msg::CancelSignIn))
                    .kind(ButtonKind::Secondary)
                    .size(ButtonSize::Sm)
                    .into_element(),
            );
        } else {
            let (note, tint) = match &self.assistant.sign_in_error {
                Some(error) => (error.clone(), p.danger),
                None => ("Connect an OpenAI-compatible service with an API key, or sign in with ChatGPT.".to_owned(), muted),
            };
            content = content.push(
                enz_text(note)
                    .size_px(SMALL)
                    .color(tint)
                    .align(enzui_widgets::text::TextAlign::Center)
                    .into_element(),
            );
            content = content.push(
                row![
                    enz_button("Sign in with ChatGPT", m(Msg::SignIn))
                        .kind(ButtonKind::Accent)
                        .size(ButtonSize::Sm)
                        .into_element(),
                    enz_button("Settings", m(Msg::OpenSettings))
                        .kind(ButtonKind::Secondary)
                        .size(ButtonSize::Sm)
                        .into_element(),
                ]
                .spacing(8),
            );
        }
        container(enzui_widgets::max_width::max_width(content, 300.0))
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .padding(12)
            .into()
    }

    /// The saved chats, newest first; one opens in the panel.
    fn assistant_history<'a>(&'a self, chats: &'a [super::history::Summary], chat: &'a super::Chat, p: &Palette) -> Element<'a, Message> {
        let muted = p.text_muted;
        if chats.is_empty() {
            return container(enz_text("No saved chats yet. A chat is kept once the assistant has answered.").size_px(SMALL).color(muted).align(enzui_widgets::text::TextAlign::Center).into_element())
                .center_x(Length::Fill)
                .padding(24)
                .into();
        }
        let now = chrono::Local::now();
        let mut list = column![].spacing(2).padding(Padding { top: 2.0, right: 10.0, bottom: 8.0, left: 4.0 });
        for saved in chats {
            let open = chat.id.as_deref() == Some(saved.id.as_str());
            let hovered = self.assistant.hovered_chat.as_deref() == Some(saved.id.as_str());
            let mut line = row![enz_text(saved.title.clone()).size_px(BODY).single_line_elided().full_width().into_element()]
                .spacing(6)
                .align_y(Alignment::Center);
            // The button keeps its room while hidden, so the row does not
            // reflow under the pointer.
            line = line.push(if hovered {
                enz_toolbar_icon("trash-2", m(Msg::DeleteChat(saved.id.clone())))
                    .size(22.0, 13.0)
                    .aria_label("Delete this chat")
                    .tooltip("Delete")
                    .into_element()
            } else {
                space().width(22).height(22).into()
            });
            line = line.push(enz_text(super::history::when(saved.updated, now)).size_px(TINY).color(muted).into_element());
            list = list.push(
                mouse_area(
                    enz_pressable(
                        container(line).padding(Padding { top: 4.0, right: 8.0, bottom: 4.0, left: 10.0 }).width(Length::Fill).into(),
                        m(Msg::OpenChat(saved.id.clone())),
                    )
                    .style(PressableStyle::Row)
                    .selected(open)
                    .aria_label(saved.title.clone())
                    .width(Length::Fill)
                    .into_element(),
                )
                .on_enter(m(Msg::HoverChat(Some(saved.id.clone()))))
                .on_exit(m(Msg::HoverChat(None))),
            );
        }
        enzui_widgets::scroll_view::enz_scrollable(list)
            .height(Length::Fill)
            .into()
    }

    fn assistant_transcript<'a>(&'a self, chat: &'a super::Chat, p: &Palette) -> Element<'a, Message> {
        let running = super::Running::answering(chat.running.as_ref());
        let last = chat.rows.len().saturating_sub(1);
        let muted = p.text_muted;
        let mut list = column![].spacing(2).padding(Padding { top: 4.0, right: 12.0, bottom: 10.0, left: 8.0 });
        let mut index = 0;
        while index < chat.rows.len() {
            // A run of finished steps folds into one row.
            let run = chat.rows[index..].iter().take_while(|row| folds(row)).count();
            if run >= 2 {
                let rows = &chat.rows[index..index + run];
                let first = &rows[0];
                let steps = rows.iter().filter(|r| matches!(r.body, Body::Tool { .. })).count();
                let failed = rows.iter().any(|r| matches!(r.body, Body::Tool { status: ToolStatus::Failed, .. }));
                let mut seen: Vec<String> = Vec::new();
                for row in rows {
                    if let Body::Tool { name, arguments, status, .. } = &row.body {
                        let title = if *status == ToolStatus::Stopped {
                            format!("Not done: {}", super::tool_request(name, arguments).to_lowercase())
                        } else {
                            super::tool_title(name, arguments)
                        };
                        if !seen.contains(&title) {
                            seen.push(title);
                        }
                    }
                }
                let summary = match (seen.len(), steps) {
                    (0, _) => "Thought it through".to_owned(),
                    (1, 1) => seen[0].clone(),
                    (1, n) => format!("{} · {n} steps", seen[0]),
                    (_, n) => format!("{}, {} · {n} steps", seen[0], seen[1].to_lowercase()),
                };
                let heading = enz_pressable(
                    container(
                        row![
                            enz_icon(if failed { "circle-alert" } else { "list-checks" })
                                .size(13.0)
                                .tint(if failed { p.danger } else { muted })
                                .into_element(),
                            enz_text(summary)
                                .size_px(SMALL)
                                .color(muted)
                                .ellipsis(iced::widget::text::Ellipsis::End)
                                .into_element(),
                            space().width(Length::Fill),
                            disclosure(first.open, muted),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    )
                    .padding([3, 2])
                    .into(),
                    m(Msg::Toggled(first.key)),
                )
                .style(PressableStyle::Plain)
                .width(Length::Fill)
                .into_element();
                list = list.push(heading);
                if first.open {
                    let mut inner = column![].spacing(2);
                    for (offset, row) in rows.iter().enumerate() {
                        // The group's own toggle is the first row's; inside,
                        // that row shows closed.
                        inner = inner.push(self.assistant_row(chat, row, p, Place { running, last: index + offset == last, group_head: offset == 0, final_reply: false }));
                    }
                    list = list.push(container(inner).padding(Padding { left: 14.0, ..Padding::ZERO }));
                }
                index += run;
                continue;
            }
            let row = &chat.rows[index];
            let final_reply = matches!(row.body, Body::Reply { .. })
                && !chat.rows[index + 1..]
                    .iter()
                    .take_while(|r| !matches!(r.body, Body::User { .. }))
                    .any(|r| matches!(r.body, Body::Reply { .. }));
            list = list.push(self.assistant_row(chat, row, p, Place { running, last: index == last, group_head: false, final_reply }));
            index += 1;
        }
        if !running && !chat.suggestions.is_empty() {
            let mut chips = enzui_widgets::flow::enz_flow().gap(enzui_theme::SpaceToken::Sm);
            for reply in &chat.suggestions {
                chips = chips.push(chip(reply.clone(), m(Msg::Ask(reply.clone()))));
            }
            list = list.push(container(chips.into_element()).padding(Padding { top: 4.0, ..Padding::ZERO }));
        }
        let scroll = enzui_widgets::scroll_view::enz_scrollable(list)
            .id(iced::widget::Id::new(TRANSCRIPT))
            .on_scroll(|scroll| {
                let viewport = &scroll.viewport;
                m(Msg::Scrolled(
                    viewport.absolute_offset().y,
                    viewport.relative_offset().y >= 0.995
                        || viewport.content_bounds().height <= viewport.bounds().height,
                ))
            })
            .height(Length::Fill);
        // Replies, messages and step details can be selected and copied.
        let selectable = enzui_widgets::selection_scope::enz_selection_scope(scroll)
            .id("assistant-transcript-selection")
            .label("Conversation")
            .on_copy(|text| m(Msg::Copy(text)))
            .into_element();
        enz_tail_follow(selectable)
    }

    fn assistant_row<'a>(
        &'a self,
        chat: &'a super::Chat,
        row: &'a Row,
        p: &Palette,
        place: Place,
    ) -> Element<'a, Message> {
        let Place { running, last, group_head, final_reply } = place;
        let muted = p.text_muted;
        let inset = mix(p.background, p.text, 0.07);
        let border = p.border;
        let key = row.key;
        let open = row.open && !group_head;
        match &row.body {
            Body::User { text, images, steering } => {
                let mut bubble = column![].spacing(6).align_x(Alignment::End);
                if !images.is_empty() {
                    let mut pictures = row![].spacing(6);
                    for image in images {
                        pictures = pictures.push(
                            iced::widget::image(image.clone())
                                .height(96)
                                .border_radius(8.0),
                        );
                    }
                    bubble = bubble.push(pictures);
                }
                if !text.is_empty() {
                    bubble = bubble.push(
                        enzui_widgets::max_width::max_width(
                            container(enz_text(text.clone()).size_px(BODY).wrap(true).into_element())
                                .padding([7, 11])
                                .style(move |_| container::Style {
                                    background: Some(inset.into()),
                                    border: iced::Border { radius: 14.0.into(), ..Default::default() },
                                    ..Default::default()
                                }),
                            300.0,
                        ),
                    );
                }
                let mut column = column![].spacing(3).align_x(Alignment::End);
                if *steering {
                    column = column.push(
                        row![
                            enz_icon("corner-down-right").size(11.0).tint(muted).into_element(),
                            enz_text("Steering").size_px(TINY).color(muted).into_element()
                        ]
                        .spacing(3)
                        .align_y(Alignment::Center),
                    );
                }
                container(column.push(bubble))
                    .align_right(Length::Fill)
                    .padding(Padding { top: 8.0, bottom: 8.0, ..Padding::ZERO })
                    .into()
            }
            Body::Reply { text, blocks, streaming, links, .. } => {
                let mut column = column![
                    enz_markdown_blocks(blocks.clone())
                        .body_size(BODY)
                        .heading_scale(1.0)
                        .line_height_multiple(1.45)
                        .block_spacing(8.0)
                        .on_link_clicked(|url| m(Msg::Link(url)))
                        .into_element()
                ]
                .spacing(6);
                if !*streaming {
                    if self.browser.saved.assistant.link_previews {
                        for url in links {
                            column = column.push(link_card(url, None, chat, true, p));
                        }
                    }
                    if !text.is_empty() && final_reply && !(running && last) {
                        column = column.push(
                            row![
                                enz_toolbar_icon("copy", m(Msg::Copy(text.clone())))
                                    .size(22.0, 13.0)
                                    .aria_label("Copy the reply")
                                    .tooltip("Copy")
                                    .into_element()
                            ]
                            .spacing(4),
                        );
                    }
                }
                container(column)
                    .padding(Padding { top: 2.0, bottom: 4.0, right: 2.0, ..Padding::ZERO })
                    .width(Length::Fill)
                    .into()
            }
            Body::Thought { text, live, .. } => {
                let label = if *live { "Thinking" } else { "Thought" };
                let heading = enz_pressable(
                    row![
                        enz_icon("brain").size(13.0).tint(muted).into_element(),
                        if *live {
                            enz_progress_text(label).size(TextSize::Caption).into_element()
                        } else {
                            enz_text(label).size_px(SMALL).color(muted).into_element()
                        },
                        disclosure(open, muted),
                        space().width(Length::Fill)
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center)
                    .into(),
                    m(Msg::Toggled(key)),
                )
                .style(PressableStyle::Plain)
                .into_element();
                let mut column = column![heading].spacing(4);
                if open {
                    column = column.push(
                        container(enz_text(text.clone()).size_px(SMALL).color(muted).wrap(true).into_element())
                            .padding([6, 10])
                            .width(Length::Fill)
                            .style(move |_| container::Style {
                                background: Some(inset.into()),
                                border: iced::Border { radius: 8.0.into(), ..Default::default() },
                                ..Default::default()
                            }),
                    );
                }
                container(column).padding(Padding { top: 1.0, bottom: 3.0, ..Padding::ZERO }).into()
            }
            Body::Tool { name, arguments, output, image, status, .. } => {
                let status_icon: Element<'a, Message> = match status {
                    ToolStatus::Running if running => enz_spinner().size(12.0).into_element(),
                    ToolStatus::Running | ToolStatus::Stopped => enz_icon("square").size(10.0).tint(muted).into_element(),
                    ToolStatus::Done => enz_icon("check").size(12.0).tint(muted).into_element(),
                    ToolStatus::Failed => enz_icon("x").size(12.0).tint(p.danger).into_element(),
                };
                let heading = enz_pressable(
                    container(
                        row![
                            enz_icon(tool_icon(name))
                                .size(13.0)
                                .tint(if *status == ToolStatus::Failed { p.danger } else { muted })
                                .into_element(),
                            enz_text(if *status == ToolStatus::Stopped {
                                format!("Not done: {}", super::tool_request(name, arguments).to_lowercase())
                            } else {
                                super::tool_title(name, arguments)
                            })
                                .size_px(SMALL)
                                .color(muted)
                                .ellipsis(iced::widget::text::Ellipsis::End)
                                .into_element(),
                            space().width(Length::Fill),
                            status_icon,
                            disclosure(open, muted),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    )
                    .padding([3, 2])
                    .into(),
                    m(Msg::Toggled(key)),
                )
                .style(PressableStyle::Plain)
                .width(Length::Fill)
                .into_element();
                let mut column = column![heading].spacing(4);
                if open {
                    // What the assistant looked at shows only with the step's
                    // details.
                    if let Some(image) = image {
                        column = column.push(
                            iced::widget::image(image.clone())
                                .width(Length::Fill)
                                .border_radius(8.0),
                        );
                    }
                    let mut detail = column![].spacing(6);
                    if !arguments.trim().is_empty() && arguments.trim() != "{}" {
                        detail = detail.push(code(pretty(arguments), 90.0));
                    }
                    if !output.is_empty() {
                        detail = detail.push(enz_text("Result").size_px(TINY).color(muted).weight(Weight::Semibold).into_element());
                        detail = detail.push(code(output.trim_end().to_owned(), 180.0));
                    }
                    column = column.push(
                        container(detail)
                            .padding([8, 10])
                            .width(Length::Fill)
                            .style(move |_| container::Style {
                                background: Some(inset.into()),
                                border: iced::Border { radius: 8.0.into(), ..Default::default() },
                                ..Default::default()
                            }),
                    );
                }
                container(column).padding(Padding { bottom: 2.0, ..Padding::ZERO }).into()
            }
            Body::Approval { id, name, arguments, gates, decision } => {
                let mut card = column![
                    row![
                        enz_icon("shield-alert").size(15.0).tint(p.warning).into_element(),
                        enz_text(format!("{}?", super::tool_request(name, arguments)))
                            .size_px(BODY)
                            .weight(Weight::Semibold)
                            .wrap(true)
                            .into_element(),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(8);
                let reason = match gates.first() {
                    Some(super::Gate::OtherTab) => Some("This uses a tab other than the one you are on."),
                    Some(super::Gate::Egress) => Some("This chat holds page content, and the assistant chose this address; the site could receive some of it."),
                    _ => None,
                };
                if let Some(reason) = reason {
                    card = card.push(enz_text(reason).size_px(SMALL).color(muted).wrap(true).into_element());
                }
                if arguments.trim() != "{}" {
                    card = card.push(code(arguments.clone(), 90.0));
                }
                let footer: Element<'a, Message> = match decision {
                    Decision::Pending => row![
                        space().width(Length::Fill),
                        enz_button("Deny", m(Msg::Decide(id.clone(), Choice::Deny)))
                            .kind(ButtonKind::Quiet)
                            .size(ButtonSize::Sm)
                            .into_element(),
                        enz_button("Allow for this chat", m(Msg::Decide(id.clone(), Choice::Chat)))
                            .kind(ButtonKind::Secondary)
                            .size(ButtonSize::Sm)
                            .into_element(),
                        enz_button("Allow", m(Msg::Decide(id.clone(), Choice::Once)))
                            .kind(ButtonKind::Accent)
                            .size(ButtonSize::Sm)
                            .into_element(),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center)
                    .into(),
                    Decision::Allowed => note_line("check", "Allowed", p.success, muted),
                    Decision::Denied => note_line("x", "Denied", muted, muted),
                    Decision::Closed => note_line("square", "No longer needed", muted, muted),
                };
                card = card.push(footer);
                container(
                    container(card).padding(12).width(Length::Fill).style(move |_| container::Style {
                        background: Some(inset.into()),
                        border: iced::Border { radius: 12.0.into(), width: 1.0, color: border },
                        ..Default::default()
                    }),
                )
                .padding(Padding { top: 4.0, bottom: 8.0, ..Padding::ZERO })
                .into()
            }
            Body::Links(links) => {
                let mut cards = column![].spacing(6);
                for link in links {
                    cards = cards.push(link_card(&link.url, Some(link), chat, self.browser.saved.assistant.link_previews, p));
                }
                container(cards).padding(Padding { top: 2.0, bottom: 6.0, ..Padding::ZERO }).into()
            }
            Body::Note { text, error } => {
                let mut column = column![
                    row![
                        enz_icon(if *error { "circle-alert" } else { "square" })
                            .size(13.0)
                            .tint(if *error { p.danger } else { muted })
                            .into_element(),
                        enz_text(text.clone())
                            .size_px(SMALL)
                            .color(if *error { p.text } else { muted })
                            .wrap(true)
                            .into_element(),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center)
                ]
                .spacing(6);
                if *error && !running && last {
                    column = column.push(
                        row![
                            enz_button("Try again", m(Msg::Retry))
                                .kind(ButtonKind::Secondary)
                                .size(ButtonSize::Sm)
                                .icon("rotate-ccw")
                                .into_element(),
                            enz_button("Settings", m(Msg::OpenSettings))
                                .kind(ButtonKind::Quiet)
                                .size(ButtonSize::Sm)
                                .into_element(),
                        ]
                        .spacing(6),
                    );
                }
                container(column).padding(Padding { top: 4.0, bottom: 8.0, ..Padding::ZERO }).into()
            }
        }
    }

    fn assistant_composer<'a>(&'a self, chat: &'a super::Chat, p: &Palette) -> Element<'a, Message> {
        let muted = p.text_muted;
        let running = chat.running.as_ref().filter(|running| !running.settled);
        let empty = chat.composer.text().trim().is_empty();
        let mut leading = vec![
            enz_toolbar_icon("image-plus", m(Msg::AttachFiles))
                .size(26.0, 15.0)
                .aria_label("Attach pictures")
                .tooltip("Attach pictures (or paste them with Ctrl+V)")
                .into_element(),
            self.model_select(),
            self.access_picker("assistant-access", true),
        ];
        if let Some(notice) = &chat.notice {
            leading.push(enz_text(notice.clone()).size_px(TINY).color(p.danger).into_element());
        } else if !chat.attachments.is_empty() {
            leading.push(enz_text(format!("{} attached", chat.attachments.len())).size_px(TINY).color(muted).into_element());
        }
        let mut composer = enz_composer(&chat.composer, |action| m(Msg::Edit(action)))
            .placeholder(if running.is_some() { "Steer the reply…" } else { "Ask anything" })
            .max_rows(8)
            .busy(running.is_some() && empty && chat.attachments.is_empty())
            .on_send(m(Msg::Send))
            .on_stop(m(Msg::Stop))
            .on_paste(m(Msg::Paste))
            .rail_leading(leading)
            .id(COMPOSER)
            .card(true)
            .wrapped_rows(true)
            .aria_label("Message the assistant");
        if running.is_some() && !empty {
            composer = composer.rail_trailing(vec![
                enz_toolbar_icon("square", m(Msg::Stop)).aria_label("Stop").tooltip("Stop").into_element(),
            ]);
        }
        let mut column = column![].spacing(6);
        if let Some(running) = running {
            let activity = running.status.clone().unwrap_or_else(|| activity(chat));
            column = column.push(
                container(
                    row![
                        enz_progress_text(format!("{activity}…")).size(TextSize::Caption).into_element(),
                        space().width(Length::Fill),
                        enz_text(seconds(chat.now.saturating_duration_since(running.since)))
                            .size_px(TINY)
                            .color(muted)
                            .into_element(),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .padding(Padding { top: 0.0, right: 6.0, bottom: 0.0, left: 6.0 }),
            );
        }
        let badge = p.surface_elevated;
        let badge_border = p.border;
        if !chat.attachments.is_empty() {
            let mut strip = row![].spacing(6);
            for (index, attachment) in chat.attachments.iter().enumerate() {
                strip = strip.push(stack![
                    iced::widget::image(attachment.handle.clone())
                        .width(56)
                        .height(56)
                        .content_fit(iced::ContentFit::Cover)
                        .border_radius(8.0),
                    container(
                        enz_pressable(
                            container(enz_icon("x").size(11.0).tint(p.text).into_element())
                                .center(18)
                                .style(move |_| container::Style {
                                    background: Some(badge.into()),
                                    border: iced::Border { radius: 9.0.into(), width: 1.0, color: badge_border },
                                    ..Default::default()
                                })
                                .into(),
                            m(Msg::Unattach(index)),
                        )
                        .style(PressableStyle::Plain)
                        .aria_label("Remove the picture")
                        .width(Length::Shrink)
                        .into_element()
                    )
                    .padding(3)
                    .align_right(56)
                ]);
            }
            column = column.push(strip);
        }
        column
            .push(
                mouse_area(composer.into_element())
                    .on_press(m(Msg::FocusComposer))
                    .interaction(iced::mouse::Interaction::Text),
            )
            .into()
    }

    /// The Assistant section of Settings.
    pub(crate) fn assistant_settings_section(&self) -> Element<'_, Message> {
        use enzui_widgets::inline_view_switcher::{InlineSwitcherPage as Page, enz_inline_view_switcher};
        use enzui_widgets::preferences_row::enz_preferences_row;
        let s = &self.browser.saved.assistant;
        let a = &self.assistant;
        let p = &self.look().palette;
        let mut body = column![
            enz_preferences_row("Assistant")
                .icon("sparkles")
                .subtitle("A chat beside the page that can read and use your tabs. Off, it takes no memory or connection.")
                .switch(s.enabled, |on| m(Msg::Set(Change::Enabled(on))))
                .into_element()
        ]
        .spacing(4);
        if !s.enabled {
            return enzui_widgets::preferences_group::enz_preferences_group(body)
                .title("Assistant")
                .bordered(true)
                .into_element();
        }
        body = body.push(self.assistant_services(p));
        body = body.push(
            enz_preferences_row("System prompt")
                .icon("scroll-text")
                .subtitle(if s.system_prompt.trim().is_empty() {
                    "What the assistant is told before every chat: the browser's own."
                } else {
                    "What the assistant is told before every chat: your own."
                })
                .action("Edit…", m(Msg::EditPrompt))
                .into_element(),
        );
        body = body.push(
            enz_preferences_row("Memory")
                .icon("brain")
                .subtitle(format!(
                    "Lets the assistant keep short notes about you between chats (up to {} characters, kept in this profile) and search your saved chats.",
                    super::memory::LIMIT
                ))
                .switch(s.memory, |on| m(Msg::Set(Change::Memory(on))))
                .into_element(),
        );
        if s.memory {
            let muted = p.text_muted;
            let mut notes = column![].spacing(2).padding(Padding { top: 0.0, right: 12.0, bottom: 8.0, left: 46.0 });
            if s.memories.is_empty() {
                notes = notes.push(enz_text("No notes yet. Ask it to remember something, or it will note lasting facts you share.").size_px(SMALL).color(muted).wrap(true).into_element());
            }
            for (index, note) in s.memories.iter().enumerate() {
                notes = notes.push(
                    row![
                        enz_text(note.clone()).size_px(SMALL).wrap(true).width(Length::Fill).into_element(),
                        enz_toolbar_icon("x", m(Msg::Set(Change::Forget(index))))
                            .size(24.0, 12.0)
                            .aria_label("Forget this note")
                            .tooltip("Forget")
                            .into_element(),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                );
            }
            if !s.memories.is_empty() {
                notes = notes.push(
                    row![
                        space().width(Length::Fill),
                        enz_button("Forget all", m(Msg::Set(Change::ForgetAll))).kind(ButtonKind::Quiet).size(ButtonSize::Sm).into_element(),
                    ],
                );
            }
            body = body.push(notes);
        }
        body = body.push(
            enz_preferences_row("Access to pages")
                .icon("shield-check")
                .subtitle(access_help(s.access))
                .control(self.access_picker("assistant-access-settings", false))
                .accessory_width(190.0)
                .into_element(),
        );
        body = body.push(
            enz_preferences_row("Link previews")
                .icon("link")
                .subtitle("Offer a preview of pages the assistant links; a site sees a plain request when you ask for its preview.")
                .switch(s.link_previews, |on| m(Msg::Set(Change::LinkPreviews(on))))
                .into_element(),
        );
        let center = super::mcp::program("enzui-mcp").is_some();
        let enrolled = super::mcp::credential_path().is_file();
        let apps = enz_preferences_row("Use other apps")
            .icon("blocks")
            .subtitle(if !center {
                "Install the enzUI agent center to let the assistant use other apps.".to_owned()
            } else if let Some(error) = &a.enroll_error {
                error.clone()
            } else if enrolled {
                "Through the agent center, which asks you before each app function runs.".to_owned()
            } else {
                "Connect the assistant to the agent center, which asks you before each app function runs.".to_owned()
            });
        let apps = if center && enrolled {
            apps.switch(s.use_apps, |on| m(Msg::Set(Change::UseApps(on))))
        } else if center {
            apps.action(if a.enrolling { "Connecting…" } else { "Connect" }, m(Msg::Enroll))
        } else {
            apps
        };
        body = body.push(apps.into_element());
        if !matches!(self.browser.saved.layout, crate::browser::Layout::Sidebar | crate::browser::Layout::Icons) {
            let side = enz_inline_view_switcher(
                vec![Page::new("left", "Left").with_icon("panel-left"), Page::new("right", "Right").with_icon("panel-right")],
                usize::from(s.right),
                |i| m(Msg::Set(Change::Right(i == 1))),
            )
            .tab_id_prefix("assistant-side")
            .into_element();
            body = body.push(
                enz_preferences_row("Panel position")
                    .icon("panel-left-right-dashed")
                    .subtitle("With a sidebar, the panel takes the other side.")
                    .control(side)
                    .accessory_width(190.0)
                    .into_element(),
            );
        }
        enzui_widgets::preferences_group::enz_preferences_group(body)
            .title("Assistant")
            .bordered(true)
            .into_element()
    }

    /// The model choice in the composer's rail: its name and a chevron. It
    /// opens a menu of the services; the one under the pointer shows its
    /// models beside it, filtered by typing.
    fn model_select(&self) -> Element<'_, Message> {
        let s = &self.browser.saved.assistant;
        let p = &self.look().palette;
        let current = s.current_model();
        // Services such as OpenRouter name models `vendor/model`; the row
        // shows the model, the menu the full names.
        let short = current.rsplit('/').next().unwrap_or(current);
        let label = if short.is_empty() { "Model" } else { short };
        let open = self.assistant.picker.is_some();
        let ink = if short.is_empty() { p.text_muted } else { p.text };
        let trigger = enz_pressable(
            container(
                row![
                    enz_text(label.to_owned()).size_px(SMALL).color(ink).single_line_elided().text_max_width(150.0).into_element(),
                    enz_icon(if open { "chevron-up" } else { "chevron-down" }).size(12.0).tint(p.text_muted).into_element(),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            )
            .padding(Padding { top: 4.0, right: 6.0, bottom: 4.0, left: 6.0 })
            .into(),
            m(Msg::Picker(!open)),
        )
        .style(PressableStyle::Plain)
        .width(Length::Shrink)
        .aria_label(format!("Model: {}", if current.is_empty() { "none" } else { current }))
        .into_element();
        let popup = self.assistant.picker.as_deref().map(|shown| self.model_menu(shown));
        enzui_widgets::anchored_overlay::dismissible_anchored_menu(
            trigger,
            popup,
            enzui_core::placement::Placement::new(enzui_core::placement::Side::Top)
                .alignment(enzui_core::placement::Alignment::Start)
                .gap(6.0),
            || m(Msg::Picker(false)),
        )
    }

    /// The open model menu: services, and the models of `shown`.
    fn model_menu(&self, shown: &str) -> Element<'_, Message> {
        use enzui_widgets::menu::enz_menu;
        use enzui_widgets::menu_row::enz_menu_row;
        let s = &self.browser.saved.assistant;
        let p = &self.look().palette;
        let mut services: Vec<(String, String, &'static str)> =
            s.services.iter().map(|service| (service.id.clone(), service.name.clone(), "key-round")).collect();
        if !s.chatgpt_email.is_empty() {
            services.push((CHATGPT.to_owned(), "ChatGPT".to_owned(), "message-circle"));
        }
        let mut list = enz_menu().width(SERVICES_WIDTH).id_prefix("assistant-services");
        for (id, name, icon) in &services {
            let row = enz_menu_row(name.clone(), m(Msg::PickerService(id.clone())))
                .icon(*icon)
                .submenu_indicator(true)
                .highlighted(id == shown)
                .selected(*id == s.service)
                .into_element();
            list = list.push(mouse_area(row).on_enter(m(Msg::PickerService(id.clone()))));
        }
        if !services.is_empty() {
            list = list.push(enzui_widgets::menu_row::enz_menu_separator());
        }
        list = list.push(enz_menu_row(if services.is_empty() { "Add a model service…" } else { "Add a service…" }, m(Msg::EditService(None))).icon("plus").into_element());
        list = list.push(enz_menu_row("Manage services", m(Msg::OpenSettings)).icon("settings").into_element());
        if !services.iter().any(|(id, ..)| id == shown) {
            return list.into_element();
        }
        let chosen = if shown == s.service { s.current_model() } else { "" };
        let models = self.assistant.models.get(shown);
        let query = self.assistant.model_query.trim().to_lowercase();
        let mut rows = column![].spacing(0);
        let note = |text: String, color: Color| -> Element<'_, Message> {
            container(enz_text(text).size_px(SMALL).color(color).wrap(true).into_element())
                .padding(Padding { top: 8.0, right: 10.0, bottom: 8.0, left: 10.0 })
                .into()
        };
        match models {
            Some(Ok(models)) => {
                let mut shown_any = false;
                // The model in use leads the list, so it shows without scrolling.
                let ordered = models.iter().filter(|model| *model == chosen).chain(models.iter().filter(|model| *model != chosen));
                for model in ordered.filter(|model| query.is_empty() || model.to_lowercase().contains(&query)) {
                    shown_any = true;
                    rows = rows.push(
                        enz_menu_row(model.clone(), m(Msg::Pick(shown.to_owned(), model.clone())))
                            .selected(model == chosen)
                            .reserve_leading(true)
                            .into_element(),
                    );
                }
                if !shown_any {
                    rows = rows.push(if query.is_empty() {
                        note("This service lists no models.".into(), p.text_muted)
                    } else {
                        // A model the list does not name can still be used.
                        enz_menu_row(format!("Use “{}”", self.assistant.model_query.trim()), m(Msg::Pick(shown.to_owned(), self.assistant.model_query.trim().to_owned())))
                            .icon("corner-down-left")
                            .into_element()
                    });
                }
            }
            Some(Err(error)) => {
                rows = rows.push(note(error.clone(), p.danger));
                rows = rows.push(enz_menu_row("Try again", m(Msg::LoadModels(shown.to_owned()))).icon("refresh-cw").into_element());
            }
            None => rows = rows.push(note("Reading the models…".into(), p.text_muted)),
        }
        let filter = container(
            enzui_widgets::text_field::enz_text_field("", "Filter models", &self.assistant.model_query, |query| m(Msg::ModelQuery(query)))
                .id(iced::advanced::widget::Id::new(MODEL_FILTER))
                .size(enzui_widgets::text_field::TextFieldSize::Sm)
                .leading(enz_icon("search").size(13.0).tint(p.text_muted).into_element())
                .into_element(),
        )
        .padding(Padding { top: 2.0, right: 2.0, bottom: 4.0, left: 2.0 });
        let scroller = enzui_widgets::scroll_view::enz_scrollable(rows).id(iced::widget::Id::new("assistant-model-list"));
        let models = enz_menu()
            .width(MODELS_WIDTH)
            .id_prefix("assistant-models")
            .push(filter)
            .push(enzui_widgets::max_height::MaxHeight::new(scroller, MODELS_HEIGHT));
        row![list.into_element(), models.into_element()].spacing(4).align_y(Alignment::End).into()
    }

    /// The services in Settings: a compact list, each with change and remove,
    /// the ChatGPT sign-in, and Add.
    fn assistant_services<'a>(&'a self, p: &Palette) -> Element<'a, Message> {
        let s = &self.browser.saved.assistant;
        let a = &self.assistant;
        let muted = p.text_muted;
        let (accent, accent_soft, border, field) = (p.accent, p.accent_soft, p.border, p.surface);
        let badge = move |icon: &'static str| -> Element<'a, Message> {
            container(enz_icon(icon).size(15.0).tint(accent).into_element())
                .center(30)
                .style(move |_| container::Style {
                    background: Some(accent_soft.into()),
                    border: iced::Border { radius: 8.0.into(), ..Default::default() },
                    ..Default::default()
                })
                .into()
        };
        let in_use = move || -> Element<'a, Message> {
            container(enz_text("In use").size_px(TINY).color(accent).into_element())
                .padding(Padding { top: 2.0, right: 7.0, bottom: 2.0, left: 7.0 })
                .style(move |_| container::Style {
                    background: Some(accent_soft.into()),
                    border: iced::Border { radius: 9.0.into(), ..Default::default() },
                    ..Default::default()
                })
                .into()
        };
        let line = |icon: &'static str, title: String, detail: String, current: bool, actions: Vec<Element<'a, Message>>| -> Element<'a, Message> {
            let mut text = row![enz_text(title).size_px(BODY).weight(Weight::Semibold).single_line_elided().into_element()]
                .spacing(8)
                .align_y(Alignment::Center);
            if current {
                text = text.push(in_use());
            }
            let mut line = row![
                badge(icon),
                column![text, enz_text(detail).size_px(SMALL).color(muted).single_line_elided().into_element()].spacing(2).width(Length::Fill),
            ]
            .spacing(10)
            .align_y(Alignment::Center);
            for action in actions {
                line = line.push(action);
            }
            container(line).padding(Padding { top: 8.0, right: 8.0, bottom: 8.0, left: 10.0 }).width(Length::Fill).into()
        };
        let mut lines: Vec<Element<'a, Message>> = Vec::new();
        for service in &s.services {
            let host = url::Url::parse(&service.base_url).ok().and_then(|url| url.host_str().map(str::to_owned)).unwrap_or_else(|| service.base_url.clone());
            let model = service.model.rsplit('/').next().unwrap_or_default();
            let detail = match a.models.get(&service.id) {
                Some(Err(_)) => format!("{host} · its models could not be read"),
                _ if model.is_empty() => host,
                _ => format!("{host} · {model}"),
            };
            lines.push(line(
                "key-round",
                service.name.clone(),
                detail,
                s.service == service.id,
                vec![
                    enz_toolbar_icon("pencil", m(Msg::EditService(Some(service.id.clone()))))
                        .size(28.0, 14.0)
                        .aria_label(format!("Change {}", service.name))
                        .tooltip("Change")
                        .into_element(),
                    enz_toolbar_icon("trash-2", m(Msg::RemoveService(service.id.clone())))
                        .size(28.0, 14.0)
                        .aria_label(format!("Remove {}", service.name))
                        .tooltip("Remove, and its key from the keyring")
                        .into_element(),
                ],
            ));
        }
        let chatgpt = if !s.chatgpt_email.is_empty() {
            line("message-circle", "ChatGPT".into(), s.chatgpt_email.clone(), s.service == CHATGPT, vec![
                enz_button("Sign out", m(Msg::SignOut)).kind(ButtonKind::Quiet).size(ButtonSize::Sm).into_element(),
            ])
        } else if a.signing_in.is_some() {
            line("message-circle", "ChatGPT".into(), "Finish signing in on the page that opened.".into(), false, vec![
                enz_button("Cancel", m(Msg::CancelSignIn)).kind(ButtonKind::Quiet).size(ButtonSize::Sm).into_element(),
            ])
        } else {
            line(
                "message-circle",
                "ChatGPT".into(),
                a.sign_in_error.clone().unwrap_or_else(|| "Use your ChatGPT plan.".into()),
                false,
                vec![enz_button("Sign in…", m(Msg::SignIn)).kind(ButtonKind::Secondary).size(ButtonSize::Sm).into_element()],
            )
        };
        lines.push(chatgpt);
        let mut list = column![].spacing(0);
        let count = lines.len();
        for (index, item) in lines.into_iter().enumerate() {
            list = list.push(item);
            if index + 1 < count {
                list = list.push(container(space().height(1)).width(Length::Fill).style(move |_| container::Style {
                    background: Some(border.into()),
                    ..Default::default()
                }));
            }
        }
        let framed = container(list).width(Length::Fill).style(move |_| container::Style {
            background: Some(field.into()),
            border: iced::Border { radius: 10.0.into(), width: 1.0, color: border },
            ..Default::default()
        });
        column![
            row![
                column![
                    enz_text("Model services").size_px(BODY).weight(Weight::Semibold).into_element(),
                    enz_text("Choose the model under the message field. Keys are kept in your desktop keyring.").size_px(SMALL).color(muted).wrap(true).into_element(),
                ]
                .spacing(2)
                .width(Length::Fill),
                enz_button("Add service", m(Msg::EditService(None))).kind(ButtonKind::Secondary).size(ButtonSize::Sm).icon("plus").into_element(),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
            framed,
        ]
        .spacing(10)
        .padding(Padding { top: 6.0, right: 12.0, bottom: 10.0, left: 12.0 })
        .into()
    }

    /// The dialog that edits the system prompt, with the browser's own a
    /// click away.
    pub(crate) fn assistant_prompt_dialog(&self) -> Option<Element<'_, Message>> {
        let prompt = self.assistant.prompt.as_ref()?;
        let p = &self.look().palette;
        let muted = p.text_muted;
        let text = prompt.text();
        let is_default = text.trim() == super::SYSTEM.trim();
        let soft = p.accent_soft;
        let header = row![
            container(enz_icon("scroll-text").size(18.0).tint(p.accent).into_element()).center(36).style(move |_| container::Style {
                background: Some(soft.into()),
                border: iced::Border { radius: 10.0.into(), ..Default::default() },
                ..Default::default()
            }),
            column![
                enz_text("System prompt").size_px(16.0).weight(Weight::Semibold).into_element(),
                enz_text("What the assistant is told before every chat. Each message also names the tab you are on.")
                    .size_px(SMALL)
                    .color(muted)
                    .wrap(true)
                    .into_element(),
            ]
            .spacing(2)
            .width(Length::Fill),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let editor = enzui_widgets::text_area::enz_text_area(prompt, |action| m(Msg::PromptEdit(action)))
            .id(iced::advanced::widget::Id::new(PROMPT_EDITOR))
            .area_height((self.browser.viewport.height - 260.0).clamp(140.0, 340.0))
            .placeholder("Empty, the browser's own prompt is used.")
            .into_element();
        let state = enz_text(if is_default {
            "This is the browser's own prompt."
        } else if text.trim().is_empty() {
            "Empty: the browser's own prompt will be used."
        } else {
            "Changed from the browser's own prompt."
        })
        .size_px(TINY)
        .color(muted)
        .into_element();
        let buttons = row![
            enz_button("Restore default", m(Msg::RestorePrompt))
                .kind(ButtonKind::Quiet)
                .icon("rotate-ccw")
                .enabled(!is_default)
                .into_element(),
            space().width(Length::Fill),
            enz_button("Cancel", m(Msg::ClosePrompt)).kind(ButtonKind::Secondary).into_element(),
            enz_button("Save", m(Msg::SavePrompt)).kind(ButtonKind::Primary).into_element(),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        Some(
            enzui_widgets::modal_surface::enz_modal_surface(column![header, column![editor, state].spacing(6), buttons].spacing(16).into())
                .padding(22.0)
                .width(Length::Fill.max((self.browser.viewport.width - 32.0).min(560.0)))
                .into_element(),
        )
    }

    /// The dialog that adds or changes a service.
    pub(crate) fn assistant_service_dialog(&self) -> Option<Element<'_, Message>> {
        let draft = self.assistant.draft.as_ref()?;
        let p = &self.look().palette;
        let muted = p.text_muted;
        let editing = draft.id.as_deref().and_then(|id| self.browser.saved.assistant.services.iter().find(|service| service.id == id));
        let label = |text: &'static str| enz_text(text).size_px(SMALL).weight(Weight::Semibold).into_element();
        let mut presets = enzui_widgets::flow::enz_flow().gap(enzui_theme::SpaceToken::Sm);
        for (index, (name, url, _)) in super::PRESETS.iter().enumerate() {
            let on = draft.base_url == *url;
            presets = presets.push(
                enz_button(*name, m(Msg::Draft(DraftField::Preset(index))))
                    .kind(if on { ButtonKind::Primary } else { ButtonKind::Secondary })
                    .size(ButtonSize::Sm)
                    .into_element(),
            );
        }
        let needs_key = !super::PRESETS.iter().any(|(_, url, key)| !key && draft.base_url == *url);
        let mut form = column![
            row![
                container(enz_icon("key-round").size(18.0).tint(p.accent).into_element()).center(36).style({
                    let soft = p.accent_soft;
                    move |_| container::Style {
                        background: Some(soft.into()),
                        border: iced::Border { radius: 10.0.into(), ..Default::default() },
                        ..Default::default()
                    }
                }),
                column![
                    enz_text(if editing.is_some() { "Change model service" } else { "Add a model service" }).size_px(16.0).weight(Weight::Semibold).into_element(),
                    enz_text("Any service that speaks the OpenAI API.").size_px(SMALL).color(muted).into_element(),
                ]
                .spacing(2),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
            column![label("Start from"), presets.into_element()].spacing(6),
            column![
                label("Name"),
                enzui_widgets::text_field::enz_text_field("", "Work, Personal, Local…", &draft.name, |v| m(Msg::Draft(DraftField::Name(v))))
                    .id(iced::advanced::widget::Id::new(SERVICE_NAME))
                    .on_submit(m(Msg::SaveService))
                    .into_element(),
            ]
            .spacing(6),
            column![
                label("Address"),
                enzui_widgets::text_field::enz_text_field("", "https://api.openai.com/v1", &draft.base_url, |v| m(Msg::Draft(DraftField::Address(v))))
                    .id(iced::advanced::widget::Id::new("assistant-service-address"))
                    .on_submit(m(Msg::SaveService))
                    .into_element(),
            ]
            .spacing(6),
            column![
                label("API key"),
                enzui_widgets::text_field::enz_text_field(
                    "",
                    if editing.is_some() { "Saved in the keyring; type to replace it" } else if needs_key { "sk-…" } else { "Not needed for a local server" },
                    &draft.key,
                    |v| m(Msg::Draft(DraftField::Key(v))),
                )
                .id(iced::advanced::widget::Id::new("assistant-service-key"))
                .secure(true)
                .on_submit(m(Msg::SaveService))
                .into_element(),
                enz_text("Kept in your desktop keyring, never in the browser's files.").size_px(TINY).color(muted).into_element(),
            ]
            .spacing(6),
        ]
        .spacing(16);
        if let Some(error) = &draft.error {
            form = form.push(enz_text(error.clone()).size_px(SMALL).color(p.danger).wrap(true).into_element());
        }
        form = form.push(
            row![
                space().width(Length::Fill),
                enz_button("Cancel", m(Msg::CloseService)).kind(ButtonKind::Secondary).into_element(),
                enz_button(
                    if draft.saving { "Saving…" } else if editing.is_some() { "Save" } else { "Add service" },
                    m(Msg::SaveService),
                )
                .kind(ButtonKind::Primary)
                .enabled(!draft.saving && !draft.name.trim().is_empty() && !draft.base_url.trim().is_empty())
                .into_element(),
            ]
            .spacing(8),
        );
        Some(
            enzui_widgets::modal_surface::enz_modal_surface(form.into())
                .padding(22.0)
                .width(Length::Fill.max((self.browser.viewport.width - 32.0).min(460.0)))
                .into_element(),
        )
    }

    /// What the assistant may do without asking. In the composer's rail
    /// (`bare`) it is the level's name and a chevron.
    fn access_picker(&self, id: &'static str, bare: bool) -> Element<'_, Message> {
        let access = self.browser.saved.assistant.access;
        let labels: Vec<String> = super::Access::ALL.iter().map(|a| a.label().to_owned()).collect();
        let selected = super::Access::ALL.iter().position(|a| *a == access).unwrap_or(2);
        let picker = enzui_widgets::combo_box::enz_combo_box("", labels.clone(), Some(labels[selected].clone()), |_| m(Msg::Tick))
            .aria_label("What the assistant may do without asking")
            .size(enzui_widgets::text_field::TextFieldSize::Sm)
            .id_prefix(id)
            .selected_index(Some(selected))
            .on_select_index(|index| m(Msg::Set(Change::Access(super::Access::ALL.get(index).copied().unwrap_or_default()))));
        if bare {
            return picker
                .variant(enzui_widgets::tokens::FieldVariant::Inline)
                .hug(110.0)
                .popup_width(170.0)
                .into_element();
        }
        picker
            .option_icon(|index| {
                Some(enzui_widgets::icon::IconSource::from(match index {
                    0 => "shield-off",
                    1 => "shield-half",
                    _ => "shield-check",
                }))
            })
            .width(Length::Fill)
            .into_element()
    }

}

fn activity(chat: &super::Chat) -> String {
    for row in chat.rows.iter().rev() {
        match &row.body {
            Body::Tool { name, arguments, status: ToolStatus::Running, .. } => {
                return super::tool_title(name, arguments).replace("Read the page", "Reading the page").replace("Looked at", "Looking at");
            }
            Body::Approval { decision: Decision::Pending, .. } => return "Waiting for your OK".into(),
            Body::Thought { live: true, .. } => return "Thinking".into(),
            Body::Reply { streaming: true, .. } => return "Writing".into(),
            Body::User { .. } => break,
            _ => {}
        }
    }
    "Thinking".into()
}

/// A finished step that folds with its neighbours.
fn folds(row: &Row) -> bool {
    match &row.body {
        Body::Thought { live, .. } => !live,
        Body::Tool { status, image, .. } => *status != ToolStatus::Running && image.is_none(),
        _ => false,
    }
}

fn disclosure(open: bool, muted: Color) -> Element<'static, Message> {
    enz_icon(if open { "chevron-down" } else { "chevron-right" }).size(12.0).tint(muted).into_element()
}

fn note_line<'a>(icon: &'static str, label: &'static str, tint: Color, muted: Color) -> Element<'a, Message> {
    row![
        enz_icon(icon).size(13.0).tint(tint).into_element(),
        enz_text(label).size_px(SMALL).color(muted).into_element()
    ]
    .spacing(5)
    .align_y(Alignment::Center)
    .into()
}

fn code<'a>(text: String, height: f32) -> Element<'a, Message> {
    enz_code_block(text)
        .show_header(false)
        .show_copy(false)
        .body_size(TINY)
        .code_padding_x(0.0)
        .code_padding_y(0.0)
        .code_max_height(height)
        .into_element()
}

fn pretty(arguments: &str) -> String {
    serde_json::from_str::<serde_json::Value>(arguments)
        .ok()
        .and_then(|v| serde_json::to_string_pretty(&v).ok())
        .unwrap_or_else(|| arguments.to_owned())
}

fn tool_icon(name: &str) -> &'static str {
    match name {
        "take_snapshot" => "file-text",
        "take_screenshot" => "camera",
        "list_pages" | "list_windows" | "select_page" => "app-window",
        "list_containers" => "boxes",
        "new_page" | "new_window" | "navigate_page" => "globe",
        "close_page" | "close_window" => "x",
        "click" | "hover" => "mouse-pointer-click",
        "fill" | "fill_form" | "type_text" | "press_key" => "keyboard",
        "scroll" => "mouse",
        "wait_for" => "hourglass",
        "evaluate_script" => "code",
        "handle_dialog" => "message-square",
        n if n.starts_with("enzui_") => "blocks",
        _ => "wrench",
    }
}

fn preview_card<'a>(preview: &super::preview::Preview, link: Option<&super::worker::Link>, p: &Palette) -> Element<'a, Message> {
    let muted = p.text_muted;
    let border = p.border;
    let fill = mix(p.background, p.text, 0.04);
    let title = link.filter(|l| !l.title.is_empty()).map_or_else(|| preview.title.clone(), |l| l.title.clone());
    let detail = link.filter(|l| !l.note.is_empty()).map_or_else(|| preview.description.clone(), |l| l.note.clone());
    let mut text = column![
        enz_text(preview.site.clone()).size_px(TINY).color(muted).ellipsis(iced::widget::text::Ellipsis::End).into_element(),
        enz_text(title).size_px(SMALL).weight(Weight::Semibold).wrap(true).into_element(),
    ]
    .spacing(2)
    .width(Length::Fill);
    if !detail.is_empty() {
        text = text.push(enz_text(detail).size_px(TINY).color(muted).wrap(true).into_element());
    }
    let mut content = row![].spacing(10).align_y(Alignment::Center);
    if let Some(image) = &preview.image {
        content = content.push(
            iced::widget::image(image.clone())
                .width(84)
                .height(56)
                .content_fit(iced::ContentFit::Cover)
                .border_radius(6.0),
        );
    }
    content = content.push(text);
    enz_pressable(
        container(content)
            .padding(8)
            .width(Length::Fill)
            .style(move |_| container::Style {
                background: Some(fill.into()),
                border: iced::Border { radius: 10.0.into(), width: 1.0, color: border },
                ..Default::default()
            })
            .into(),
        m(Msg::Link(preview.url.clone())),
    )
    .style(PressableStyle::Plain)
    .width(Length::Fill)
    .into_element()
}

/// A link's preview once fetched, else its address with a button that fetches
/// the preview: a site learns of a link only when the person asks.
fn link_card<'a>(url: &str, link: Option<&super::worker::Link>, chat: &super::Chat, previews: bool, p: &Palette) -> Element<'a, Message> {
    let requested = match chat.previews.get(url) {
        Some(Some(preview)) => return preview_card(preview, link, p),
        found => found.is_some(),
    };
    let bare = super::worker::Link { url: url.to_owned(), title: String::new(), note: String::new() };
    let card = plain_link_card(link.unwrap_or(&bare), p);
    if !previews || requested {
        return card;
    }
    row![
        card,
        enz_toolbar_icon("eye", m(Msg::LoadPreview(url.to_owned())))
            .aria_label("Show a preview")
            .tooltip("Show a preview")
            .into_element(),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .into()
}

fn plain_link_card<'a>(link: &super::worker::Link, p: &Palette) -> Element<'a, Message> {
    let host = url::Url::parse(&link.url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.trim_start_matches("www.").to_owned()))
        .unwrap_or_default();
    let preview = super::preview::Preview {
        url: link.url.clone(),
        title: if link.title.is_empty() { host.clone() } else { link.title.clone() },
        description: link.note.clone(),
        site: host,
        image: None,
    };
    preview_card(&preview, None, p)
}

fn access_help(access: super::Access) -> &'static str {
    match access {
        super::Access::Full => "The assistant acts on pages and tabs without asking. Reading another tab, or reaching a new address once it has read pages, still waits for your OK.",
        super::Access::Moderate => "It acts on the tab you are on without asking; opening, closing or switching tabs and acting on other tabs wait for your OK.",
        super::Access::Restricted => "Clicking, typing, navigating, opening or closing tabs and running scripts wait for your OK.",
    }
}
