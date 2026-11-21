use super::*;
use enzui_widgets::scroll_view::enz_scrollable as scrollable;
use iced::widget::{Space, column, container, row, text};

#[derive(Clone, Debug)]
pub(crate) enum NetworkCommand {
    Filter(String),
    Kind(usize),
    Failed(bool),
    Select(Option<u64>),
    Clear,
}
#[derive(Default)]
pub(super) struct Network {
    context: Option<(u64, u64)>,
    rows: Vec<serde_json::Value>,
    selected: Option<u64>,
    filter: String,
    kind: usize,
    failed_only: bool,
    dropped: u64,
    notice: String,
}
fn resource_kind(row: &serde_json::Value) -> usize {
    let mime = row["mime"]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if mime.contains("html") {
        1
    } else if mime.contains("css") {
        2
    } else if mime.contains("javascript") {
        3
    } else if mime.starts_with("image/") {
        4
    } else if mime.starts_with("audio/") || mime.starts_with("video/") {
        5
    } else if mime.contains("font") || mime.contains("woff") {
        6
    } else {
        7
    }
}
impl Network {
    fn matches(&self, row: &serde_json::Value) -> bool {
        let query = self.filter.to_lowercase();
        (self.kind == 0 || self.kind == resource_kind(row))
            && (!self.failed_only
                || row["failed"] == true
                || row["status"].as_u64().is_some_and(|status| status >= 400))
            && (query.is_empty()
                || ["url", "mime", "method", "status"].into_iter().any(|key| {
                    row[key]
                        .as_str()
                        .map_or_else(|| row[key].to_string(), str::to_owned)
                        .to_lowercase()
                        .contains(&query)
                }))
    }
}
impl App {
    pub(super) fn network_command(&mut self, command: NetworkCommand) -> Task<Message> {
        let state = &mut self.browser.devtools.network;
        match command {
            NetworkCommand::Filter(value) => state.filter = value,
            NetworkCommand::Kind(index) => state.kind = index.min(7),
            NetworkCommand::Failed(on) => state.failed_only = on,
            NetworkCommand::Select(id) => state.selected = id,
            NetworkCommand::Clear => {
                state.rows.clear();
                state.selected = None;
            }
        }
        Task::none()
    }
    pub(super) fn native_resource_events(&mut self, json: &str) {
        if !self.browser.devtools.open {
            return;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
            return;
        };
        let state = &mut self.browser.devtools.network;
        if let Some(error) = value["error"].as_str() {
            error.clone_into(&mut state.notice);
        }
        state.dropped = value["dropped"].as_u64().unwrap_or_default();
        if let Some(events) = value["events"].as_array() {
            for event in events {
                let context = (
                    event["log"].as_u64().unwrap_or_default(),
                    event["navigation"].as_u64().unwrap_or_default(),
                );
                if state.context != Some(context) {
                    if state
                        .context
                        .is_some_and(|(log, navigation)| log == context.0 && navigation > context.1)
                    {
                        continue;
                    }
                    state.context = Some(context);
                    state.rows.clear();
                    state.selected = None;
                }
                let Some(id) = event["id"].as_u64() else {
                    continue;
                };
                let index = state
                    .rows
                    .iter()
                    .position(|row| row["id"].as_u64() == Some(id))
                    .unwrap_or_else(|| {
                        if state.rows.len() >= 128 {
                            state.rows.remove(0);
                        }
                        state.rows.push(serde_json::json!({"id":id}));
                        state.rows.len() - 1
                    });
                if let (Some(target), Some(fields)) =
                    (state.rows[index].as_object_mut(), event.as_object())
                {
                    if !target.contains_key("started_ms") {
                        target.insert("started_ms".into(), event["elapsed"].clone());
                    }
                    let start = target
                        .get("started_ms")
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or_default();
                    if let Some(elapsed) = event["elapsed"].as_f64() {
                        target.insert(
                            "duration_ms".into(),
                            serde_json::json!((elapsed - start).max(0.)),
                        );
                    }
                    for (key, value) in fields {
                        if !value.is_null() {
                            target.insert(key.clone(), value.clone());
                        }
                    }
                }
            }
        }
    }
    pub(super) fn network_panel(&self) -> Element<'_, Message> {
        let state = &self.browser.devtools.network;
        if let Some(selected) = state
            .selected
            .and_then(|id| state.rows.iter().find(|row| row["id"].as_u64() == Some(id)))
        {
            let mut details = column![
                row![
                    action(
                        "Requests",
                        "arrow-left",
                        Command::Network(NetworkCommand::Select(None))
                    ),
                    Space::new().width(Length::Fill),
                    action(
                        "Copy details",
                        "copy",
                        Command::CopyText(
                            serde_json::to_string_pretty(selected).unwrap_or_default()
                        )
                    )
                ]
                .spacing(8)
            ]
            .spacing(12);
            for (label, key) in [
                ("URL (query values redacted)", "url"),
                ("Method", "method"),
                ("Status", "status"),
                ("MIME type", "mime"),
                ("Declared length", "length"),
                ("Native failure", "failure"),
            ] {
                let value = match &selected[key] {
                    serde_json::Value::Null => "Unavailable".into(),
                    serde_json::Value::String(s) => s.clone(),
                    value => value.to_string(),
                };
                if key != "failure" || !selected[key].is_null() {
                    details = details
                        .push(column![text(label).size(11), text(value).size(13)].spacing(3));
                }
            }
            for (label, key) in [
                ("Request headers", "request_headers"),
                ("Response headers", "response_headers"),
            ] {
                let mut headers = column![].spacing(6);
                if let Some(values) = selected[key].as_object() {
                    for (name, value) in values {
                        headers = headers.push(
                            row![
                                text(name.clone()).size(12).width(120),
                                text(value.as_str().unwrap_or_default().to_owned())
                                    .size(12)
                                    .width(Length::Fill)
                            ]
                            .spacing(8),
                        );
                    }
                    if values.is_empty() {
                        headers = headers.push(text("No allowlisted fields reported.").size(12));
                    }
                } else {
                    headers = headers.push(text("Unavailable").size(12));
                }
                details = details.push(section(label, headers));
            }
            details = details.push(text("Safe header subset only. Cookie/authentication headers, bodies and DNS/TLS phase timings are unavailable. Time is measured between native request events; DNS/TLS phase timings are unavailable.").size(11));
            return scrollable(container(details).padding(iced::Padding::new(0.).right(14)))
                .height(Length::Fill)
                .into();
        }
        let filter =
            enzui_widgets::text_field::enz_text_field("", "Filter requests", &state.filter, |s| {
                c(Command::Network(NetworkCommand::Filter(s)))
            })
            .leading(glyph("search", 12.))
            .quiet(true)
            .size(enzui_widgets::text_field::TextFieldSize::Sm)
            .aria_label("Filter network requests")
            .into_element();
        let labels: Vec<String> = [
            "All types",
            "Documents",
            "Styles",
            "Scripts",
            "Images",
            "Media",
            "Fonts",
            "Data / other",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let kind = enzui_widgets::combo_box::enz_combo_box(
            "",
            labels.clone(),
            Some(labels[state.kind].clone()),
            |_| c(Command::Network(NetworkCommand::Kind(0))),
        )
        .selected_index(Some(state.kind))
        .on_select_index(|index| c(Command::Network(NetworkCommand::Kind(index))))
        .id_prefix("network-type")
        .aria_label("Network resource type")
        .width(108)
        .into_element();
        let failed = iced::widget::checkbox(state.failed_only)
            .label("Errors")
            .size(12)
            .text_size(11)
            .on_toggle(|on| c(Command::Network(NetworkCommand::Failed(on))));
        let buttons = row![
            kind,
            failed,
            ink_icon(
                "refresh-cw",
                "Reload page to capture requests",
                c(Command::Reload),
                26.,
                13.
            ),
            ink_icon(
                "ban",
                "Clear requests",
                c(Command::Network(NetworkCommand::Clear)),
                26.,
                13.
            )
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center);
        let toolbar: Element<'_, Message> = if self.content_width() < 620. {
            column![filter, buttons].spacing(4).into()
        } else {
            row![filter, buttons]
                .spacing(6)
                .align_y(iced::Alignment::Center)
                .into()
        };
        let mut body = column![toolbar].spacing(4);
        let mut entries = column![].spacing(0);
        let compact = self.content_width() < 620.;
        let mut header = row![
            text("Resource").size(11).width(Length::Fill),
            text("Status").size(11).width(54),
            text("Time").size(11).width(72)
        ]
        .spacing(8);
        if !compact {
            header = header
                .push(text("MIME").size(11).width(120))
                .push(text("Length").size(11).width(80));
        }
        entries = entries
            .push(container(header).padding([6, 8]))
            .push(iced::widget::rule::horizontal(1));
        for resource in state.rows.iter().filter(|resource| state.matches(resource)) {
            let id = resource["id"].as_u64().unwrap_or_default();
            let url = resource["url"].as_str().unwrap_or("Unavailable URL");
            let parsed = url::Url::parse(url).ok();
            let name = parsed
                .as_ref()
                .and_then(url::Url::path_segments)
                .and_then(|mut parts| parts.next_back())
                .filter(|s| !s.is_empty())
                .unwrap_or(url);
            let name: String = name.chars().take(if compact { 24 } else { 60 }).collect();
            let status = if resource["failed"] == true {
                "Failed".into()
            } else {
                resource["status"].as_u64().map_or_else(
                    || {
                        if resource["event"] == "finished" {
                            "-".into()
                        } else {
                            "…".into()
                        }
                    },
                    |status| status.to_string(),
                )
            };
            let mut line = row![
                row![
                    glyph(
                        match resource_kind(resource) {
                            1 => "file-code",
                            2 => "paintbrush",
                            3 => "code",
                            4 => "image",
                            5 => "play",
                            6 => "type",
                            _ => "file",
                        },
                        12.
                    ),
                    text(name).size(12).width(Length::Fill)
                ]
                .spacing(6)
                .align_y(iced::Alignment::Center)
                .width(Length::Fill),
                text(status).size(12).width(54),
                text(format!(
                    "{:.0} ms",
                    resource["duration_ms"].as_f64().unwrap_or_default()
                ))
                .size(12)
                .width(72)
            ]
            .spacing(8)
            .width(Length::Fill);
            if !compact {
                let mime: String = resource["mime"]
                    .as_str()
                    .unwrap_or("-")
                    .chars()
                    .take(20)
                    .collect();
                line = line.push(text(mime).size(11).width(120)).push(
                    text(
                        resource["length"]
                            .as_u64()
                            .map_or_else(|| "-".into(), |n| format!("{:.1} KB", n as f64 / 1024.)),
                    )
                    .size(11)
                    .width(80),
                );
            }
            entries = entries
                .push(
                    enzui_widgets::focus_button::focus_button(
                        line,
                        c(Command::Network(NetworkCommand::Select(Some(id)))),
                    )
                    .padding([5, 8])
                    .width(Length::Fill),
                )
                .push(iced::widget::rule::horizontal(1));
        }
        if state.rows.is_empty() {
            entries = entries.push(
                container(
                    column![
                        glyph("network", 24.),
                        text("Reload this page to capture native requests.").size(13)
                    ]
                    .spacing(10),
                )
                .padding(16),
            );
        }
        if state.dropped > 0 {
            entries = entries.push(
                text(format!(
                    "{} resource events were omitted by the native limit.",
                    state.dropped
                ))
                .size(11),
            );
        }
        if !state.notice.is_empty() {
            entries = entries.push(text(state.notice.clone()).size(12));
        }
        body = body.push(
            scrollable(container(entries).padding(iced::Padding::new(0.).right(14)))
                .height(Length::Fill),
        );
        let shown = state.rows.iter().filter(|row| state.matches(row)).count();
        let known_bytes: u64 = state
            .rows
            .iter()
            .filter(|row| state.matches(row))
            .filter_map(|row| row["length"].as_u64())
            .sum();
        body.push(
            text(format!(
                "{shown} / {} requests · {:.1} KiB declared",
                state.rows.len(),
                known_bytes as f64 / 1024.
            ))
            .size(11),
        )
        .height(Length::Fill)
        .into()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn network_duration_starts_at_each_request_and_closed_tools_ignore_events() {
        let mut app = super::super::browser_tests::app();
        let first = r#"{"events":[{"log":1,"navigation":1,"id":10,"event":"started","elapsed":5000,"url":"https://example.org/a.css"}]}"#;
        app.native_resource_events(first);
        assert!(app.browser.devtools.network.rows.is_empty());
        app.browser.devtools.open = true;
        app.native_resource_events(first);
        app.native_resource_events(r#"{"events":[{"log":1,"navigation":1,"id":10,"event":"finished","elapsed":5123,"status":404,"mime":"text/css"}]}"#);
        let state = &mut app.browser.devtools.network;
        assert_eq!(state.rows[0]["duration_ms"], 123.);
        state.kind = 2;
        state.failed_only = true;
        assert!(state.matches(&state.rows[0]));
        state.kind = 4;
        assert!(!state.matches(&state.rows[0]));
        state.kind = 0;
        state.filter = "404".into();
        assert!(state.matches(&state.rows[0]));
        app.native_resource_events(r#"{"events":[{"log":1,"navigation":2,"id":11,"event":"started","elapsed":5500,"url":"https://example.org/b"}]}"#);
        app.native_resource_events(first);
        assert_eq!(app.browser.devtools.network.rows.len(), 1);
        assert_eq!(app.browser.devtools.network.rows[0]["id"], 11);
    }
}
