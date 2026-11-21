use super::*;
use enzui_widgets::scroll_view::enz_scrollable as scrollable;
use iced::widget::{column, container, row, text};
#[derive(Clone, Debug)]
pub(crate) enum InspectCommand {
    Refresh,
    Node(u64),
    Detail(usize),
    Style(String, String),
    Pick,
    Cancel,
    Property(String),
    Value(String),
    Apply,
    Remove,
    Ready(u64, u64, Result<crate::native_jobs::Payload, String>),
}
#[derive(Default)]
pub(crate) struct Inspector {
    pub(super) selector: String,
    pub(super) picking: bool,
    pub(super) element: Option<serde_json::Value>,
    property: String,
    value: String,
    notice: String,
    pending: Option<u64>,
    detail: usize,
}
impl App {
    pub(crate) fn cancel_inspector_picker(&mut self) {
        if self.browser.devtools.inspector.picking {
            self.browser.devtools.inspector.picking = false;
            self.submit_browser_action(BrowserAction::IsolatedScript(
                "globalThis.__enzimCancelPick?.()".into(),
            ));
        }
    }
    pub(super) fn inspector_command(&mut self, command: InspectCommand) -> Task<Message> {
        use InspectCommand::*;
        match command {
            Detail(tab) => self.browser.devtools.inspector.detail = tab.min(1),
            Style(property, value) => {
                self.browser.devtools.inspector.property = property;
                self.browser.devtools.inspector.value = value;
            }
            Property(value) => self.browser.devtools.inspector.property = value,
            Value(value) => self.browser.devtools.inspector.value = value,
            Cancel => self.cancel_inspector_picker(),
            Pick => {
                self.browser.devtools.open = true;
                self.submit_browser_action(BrowserAction::DeveloperTools(true));
                self.browser.devtools.tab = 1;
                self.browser.devtools.inspector.picking = true;
                self.submit_browser_action(BrowserAction::IsolatedScript(format!(
                    "{};__enzimBeginPick()",
                    include_str!("inspector.js")
                )));
            }
            Ready(key, job, result) => {
                if self.browser.devtools.inspector.pending != Some(job)
                    || !self.browser.devtools.open
                    || self.browser.saved.tabs[self.browser.saved.active].key != key
                {
                    return Task::none();
                }
                self.browser.devtools.inspector.pending = None;
                match result {
                    Ok(crate::native_jobs::Payload::Json(value)) => self.inspector_picked(value),
                    Ok(_) => {
                        self.browser.devtools.inspector.notice =
                            "Unexpected inspection result.".into();
                    }
                    Err(error) => self.browser.devtools.inspector.notice = error,
                }
            }
            Refresh | Node(_) | Apply | Remove => {
                if self.browser.devtools.inspector.pending.is_some() {
                    return Task::none();
                }
                let state = &self.browser.devtools.inspector;
                let tail = match command {
                    Refresh => format!(
                        "__enzimInspect({})",
                        serde_json::to_string(&state.selector).unwrap_or_default()
                    ),
                    Node(id) => format!("__enzimInspectNode({id})"),
                    Apply => format!(
                        "(()=>{{const e=globalThis.__enzimSelected;if(!e?.isConnected)throw new Error('Inspect an element first.');const p={},v={};if(!CSS.supports(p,v))throw new Error('This CSS property or value is not supported.');e.style.setProperty(p,v);return __enzimSnapshot(e)}})()",
                        serde_json::to_string(state.property.trim()).unwrap_or_default(),
                        serde_json::to_string(&state.value).unwrap_or_default()
                    ),
                    _ => format!(
                        "(()=>{{const e=globalThis.__enzimSelected;if(!e?.isConnected)throw new Error('Inspect an element first.');e.style.removeProperty({});return __enzimSnapshot(e)}})()",
                        serde_json::to_string(state.property.trim()).unwrap_or_default()
                    ),
                };
                let source = format!("{};{tail}", include_str!("inspector.js"));
                let (job, receiver) = crate::native_jobs::Job::new();
                let key = self.browser.saved.tabs[self.browser.saved.active].key;
                let operation = job.id;
                self.browser.devtools.inspector.pending = Some(operation);
                self.browser.devtools.inspector.notice.clear();
                self.submit_browser_action(BrowserAction::Evaluate {
                    source,
                    world: enzui_webkit::page::ScriptWorld::Isolated(
                        "enzim-developer-tools".into(),
                    ),
                    job,
                });
                return Task::perform(crate::native_jobs::receive(receiver), move |result| {
                    c(Command::Inspect(Ready(key, operation, result)))
                });
            }
        }
        Task::none()
    }
    pub(super) fn inspector_picked(&mut self, value: serde_json::Value) {
        self.browser.devtools.inspector.picking = false;
        self.browser.devtools.inspector.notice.clear();
        self.browser.devtools.inspector.element = Some(value);
        self.browser.devtools.open = true;
        self.browser.devtools.tab = 1;
    }
    pub(super) fn inspector_panel(&self) -> Element<'_, Message> {
        let state = &self.browser.devtools.inspector;
        let compact = self.content_width() < 760.;
        let selector = enzui_widgets::text_field::enz_text_field(
            "",
            "CSS selector",
            &state.selector,
            |value| c(Command::Dev(devtools::DevCommand::Selector(value))),
        )
        .on_submit(c(Command::Inspect(InspectCommand::Refresh)))
        .quiet(true)
        .size(enzui_widgets::text_field::TextFieldSize::Sm)
        .aria_label("Inspect CSS selector")
        .into_element();
        let parent = state
            .element
            .as_ref()
            .and_then(|element| element["parents"].as_array())
            .and_then(|parents| parents.last())
            .and_then(|parent| parent["node"].as_u64());
        let mut body = column![
            row![
                navigation_button(
                    "arrow-up",
                    "Inspect parent element",
                    c(Command::Inspect(InspectCommand::Node(
                        parent.unwrap_or_default()
                    ))),
                    parent.is_some()
                ),
                selector,
                ink_icon(
                    "refresh-cw",
                    "Refresh inspection",
                    c(Command::Inspect(InspectCommand::Refresh)),
                    28.,
                    14.
                )
            ]
            .spacing(6)
            .align_y(iced::Alignment::Center)
        ]
        .spacing(6);
        if state.picking {
            body = body.push(row![
                text("Click an element · Esc cancels")
                    .size(12)
                    .width(Length::Fill),
                ink_icon(
                    "x",
                    "Cancel picking",
                    c(Command::Inspect(InspectCommand::Cancel)),
                    24.,
                    12.
                )
            ]);
        }
        if !state.notice.is_empty() {
            body = body.push(text(&state.notice).size(12));
        }
        let Some(element) = &state.element else {
            return body
                .push(
                    container(
                        column![
                            glyph("mouse-pointer-2", 22.),
                            text("Pick an element or enter a CSS selector.").size(13)
                        ]
                        .spacing(10),
                    )
                    .center(Length::Fill),
                )
                .height(Length::Fill)
                .into();
        };
        let mut dom = column![].spacing(0);
        let parents = element["parents"].as_array().map_or(&[][..], Vec::as_slice);
        for (depth, parent) in parents.iter().enumerate() {
            dom = dom.push(dom_node(parent, depth.min(4), false));
        }
        dom = dom.push(dom_node(element, parents.len().min(4), true));
        let depth = (parents.len() + 1).min(5);
        if let Some(attributes) = element["attributes"].as_array() {
            for attribute in attributes {
                if let Some(name) = attribute[0].as_str() {
                    dom = dom.push(
                        container(
                            row![
                                text(name.to_owned()).size(11).font(iced::Font::MONOSPACE),
                                text(format!(
                                    "=\"{}\"",
                                    attribute[1].as_str().unwrap_or_default()
                                ))
                                .size(11)
                                .font(iced::Font::MONOSPACE)
                                .width(Length::Fill)
                            ]
                            .spacing(4),
                        )
                        .padding(iced::Padding::new(3.).left(12. + depth as f32 * 10.)),
                    );
                }
            }
        }
        if let Some(value) = element["text"].as_str().filter(|value| !value.is_empty()) {
            dom = dom.push(
                container(
                    text(format!("\"{value}\""))
                        .size(11)
                        .font(iced::Font::MONOSPACE),
                )
                .padding(iced::Padding::new(4.).left(12. + depth as f32 * 10.)),
            );
        }
        if let Some(children) = element["childNodes"].as_array() {
            for child in children {
                dom = dom.push(dom_node(child, depth, false));
            }
            if element["children"].as_u64().unwrap_or_default() > children.len() as u64 {
                dom = dom.push(text("Showing the first 80 child elements.").size(11));
            }
        }
        let dom: Element<'_, Message> =
            scrollable(container(dom).padding(iced::Padding::new(0.).right(8)))
                .height(Length::Fill)
                .into();
        let mut styles = column![text("Inline styles").size(12)].spacing(5);
        let property =
            enzui_widgets::text_field::enz_text_field("", "Property", &state.property, |value| {
                c(Command::Inspect(InspectCommand::Property(value)))
            })
            .quiet(true)
            .size(enzui_widgets::text_field::TextFieldSize::Sm)
            .id(iced::widget::Id::new("inspect-property"))
            .into_element();
        let value = enzui_widgets::text_field::enz_text_field("", "Value", &state.value, |value| {
            c(Command::Inspect(InspectCommand::Value(value)))
        })
        .quiet(true)
        .size(enzui_widgets::text_field::TextFieldSize::Sm)
        .id(iced::widget::Id::new("inspect-value"))
        .on_submit(c(Command::Inspect(InspectCommand::Apply)))
        .into_element();
        styles = styles.push(
            row![
                property,
                value,
                ink_icon(
                    "check",
                    "Apply inline style",
                    c(Command::Inspect(InspectCommand::Apply)),
                    26.,
                    13.
                ),
                ink_icon(
                    "eraser",
                    "Remove inline style",
                    c(Command::Inspect(InspectCommand::Remove)),
                    26.,
                    13.
                )
            ]
            .spacing(4),
        );
        if let Some(inline) = element["inline"].as_array() {
            for style in inline {
                styles = styles.push(style_row(
                    style[0].as_str().unwrap_or_default(),
                    style[1].as_str().unwrap_or_default(),
                ));
            }
        }
        styles = styles
            .push(iced::widget::rule::horizontal(1))
            .push(text("Computed").size(12));
        let bounds = &element["bounds"];
        styles = styles.push(
            text(format!(
                "{:.0} × {:.0} px · x {:.0} · y {:.0}",
                bounds["width"].as_f64().unwrap_or_default(),
                bounds["height"].as_f64().unwrap_or_default(),
                bounds["x"].as_f64().unwrap_or_default(),
                bounds["y"].as_f64().unwrap_or_default()
            ))
            .size(11),
        );
        if let Some(computed) = element["styles"].as_object() {
            for (name, value) in computed {
                styles = styles.push(style_row(name, value.as_str().unwrap_or_default()));
            }
        }
        let styles: Element<'_, Message> = scrollable(container(styles).padding([4, 8]))
            .height(Length::Fill)
            .into();
        let detail: Element<'_, Message> = if compact {
            let selector = enzui_widgets::inline_view_switcher::enz_inline_view_switcher(
                vec![
                    enzui_widgets::inline_view_switcher::InlineSwitcherPage::new("dom", "DOM"),
                    enzui_widgets::inline_view_switcher::InlineSwitcherPage::new(
                        "styles", "Styles",
                    ),
                ],
                state.detail,
                |index| c(Command::Inspect(InspectCommand::Detail(index))),
            )
            .tab_id_prefix("inspect-detail")
            .into_element();
            column![selector, if state.detail == 0 { dom } else { styles }]
                .spacing(4)
                .height(Length::Fill)
                .into()
        } else {
            row![
                container(dom).width(Length::FillPortion(3)),
                iced::widget::rule::vertical(1),
                container(styles).width(Length::FillPortion(2))
            ]
            .height(Length::Fill)
            .into()
        };
        body.push(detail).height(Length::Fill).into()
    }
}
fn dom_node(value: &serde_json::Value, depth: usize, selected: bool) -> Element<'static, Message> {
    let tag = value["tag"].as_str().unwrap_or("element");
    let id = value["id"].as_str().unwrap_or_default();
    let classes = value["classes"].as_str().unwrap_or_default();
    let label = format!(
        "<{tag}{}{}>",
        if id.is_empty() {
            String::new()
        } else {
            format!("#{id}")
        },
        if classes.is_empty() {
            String::new()
        } else {
            format!(".{classes}")
        }
    );
    let node = value["node"].as_u64().unwrap_or_default();
    enzui_widgets::focus_button::focus_button(
        row![
            glyph(
                if selected {
                    "chevron-down"
                } else if value["children"].as_u64().unwrap_or_default() > 0 {
                    "chevron-right"
                } else {
                    "minus"
                },
                12.
            ),
            text(label)
                .size(12)
                .font(iced::Font::MONOSPACE)
                .width(Length::Fill)
        ]
        .spacing(4),
        c(Command::Inspect(InspectCommand::Node(node))),
    )
    .width(Length::Fill)
    .padding(iced::Padding::new(4.).left(4. + depth as f32 * 10.))
    .style_focus(move |theme, _| iced::widget::button::Style {
        background: selected.then(|| {
            let mut color = palette(theme).accent;
            color.a = 0.16;
            color.into()
        }),
        text_color: palette(theme).text,
        ..Default::default()
    })
    .into()
}
fn style_row(name: &str, value: &str) -> Element<'static, Message> {
    enzui_widgets::focus_button::focus_button(
        row![
            text(name.to_owned())
                .size(11)
                .font(iced::Font::MONOSPACE)
                .width(Length::FillPortion(2)),
            text(value.to_owned())
                .size(11)
                .font(iced::Font::MONOSPACE)
                .width(Length::FillPortion(3))
        ]
        .spacing(6),
        c(Command::Inspect(InspectCommand::Style(
            name.into(),
            value.into(),
        ))),
    )
    .padding([3, 4])
    .width(Length::Fill)
    .style_focus(|theme, status| iced::widget::button::Style {
        text_color: palette(theme).text,
        background: matches!(status, enzui_widgets::focus_button::FocusStatus::Hovered)
            .then(|| palette(theme).surface_hover.into()),
        ..Default::default()
    })
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inspector_stale_reply_cannot_replace_a_new_operation_or_reopen_closed_tools() {
        let mut app = super::super::browser_tests::app();
        let key = app.browser.saved.tabs[0].key;
        app.browser.devtools.open = true;
        app.browser.devtools.inspector.pending = Some(20);
        let _ = app.inspector_command(InspectCommand::Ready(
            key,
            19,
            Ok(crate::native_jobs::Payload::Json(
                serde_json::json!({"tag":"old"}),
            )),
        ));
        assert_eq!(app.browser.devtools.inspector.pending, Some(20));
        assert!(app.browser.devtools.inspector.element.is_none());
        app.browser.devtools.open = false;
        let _ = app.inspector_command(InspectCommand::Ready(
            key,
            20,
            Ok(crate::native_jobs::Payload::Json(
                serde_json::json!({"tag":"late"}),
            )),
        ));
        assert!(!app.browser.devtools.open);
        assert!(app.browser.devtools.inspector.element.is_none());
    }
}
