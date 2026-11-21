//! Pickers for a page's date and time fields. The engine lets them be typed
//! into and draws nothing to choose from; a click on one opens a calendar or
//! a list of times beside it, and the field can still be typed into.

use super::*;
use enzui_core::calendar_nav::MonthController;
use enzui_core::civil_date::{Date, DateBounds, Weekday};
use enzui_widgets::calendar::CalendarEvent;
use iced::widget::{column, container, text};

/// What a field holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FieldKind {
    Date,
    Time,
    /// A day and a time of day.
    DateTime,
    Month,
    Week,
}

impl FieldKind {
    fn named(name: &str) -> Option<Self> {
        Some(match name {
            "date" => Self::Date,
            "time" => Self::Time,
            "datetime-local" => Self::DateTime,
            "month" => Self::Month,
            "week" => Self::Week,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) enum DateCommand {
    Calendar(CalendarEvent),
    /// A time of day, in minutes after midnight.
    Time(u32),
    /// The page took what was chosen, or its field was gone.
    Set,
}

/// A date or time field with its picker open.
#[derive(Debug, Clone)]
pub(crate) struct DateChoosing {
    /// The tab whose page has the field.
    tab: u64,
    kind: FieldKind,
    /// What the field held when it was clicked.
    value: String,
    /// The field's place in the page, in the page's own pixels.
    area: [f32; 4],
    months: MonthController,
    selected: Option<Date>,
}

fn date(text: &str) -> Option<Date> {
    let mut parts = text.get(..10)?.split('-');
    let (year, month, day) = (parts.next()?, parts.next()?, parts.next()?);
    Date::new(year.parse().ok()?, month.parse().ok()?, day.parse().ok()?)
}

/// The day a field's text names: the day itself, a month's first day, or a
/// week's Monday.
pub(crate) fn field_date(kind: FieldKind, text: &str) -> Option<Date> {
    match kind {
        FieldKind::Date | FieldKind::DateTime => date(text),
        FieldKind::Month => date(&format!("{}-01", text.get(..7)?)),
        FieldKind::Week => {
            let (year, week) = text.split_once("-W")?;
            let monday = chrono::NaiveDate::from_isoywd_opt(
                year.parse().ok()?,
                week.parse().ok()?,
                chrono::Weekday::Mon,
            )?;
            use chrono::Datelike;
            Date::new(monday.year(), monday.month(), monday.day())
        }
        FieldKind::Time => None,
    }
}

/// What a field of `kind` is set to for a chosen day. A field with a time of
/// day keeps the time it had, or starts at noon.
pub(crate) fn field_value(kind: FieldKind, day: Date, before: &str) -> String {
    let (year, month, date) = (day.year(), day.month(), day.day());
    match kind {
        FieldKind::Date | FieldKind::Time => format!("{year:04}-{month:02}-{date:02}"),
        FieldKind::DateTime => {
            let time = before
                .split_once('T')
                .map(|(_, time)| time)
                .filter(|time| time.len() >= 5 && time.is_ascii())
                .unwrap_or("12:00");
            format!("{year:04}-{month:02}-{date:02}T{time}")
        }
        FieldKind::Month => format!("{year:04}-{month:02}"),
        FieldKind::Week => {
            use chrono::Datelike;
            chrono::NaiveDate::from_ymd_opt(year, month, date).map_or_else(String::new, |day| {
                let week = day.iso_week();
                format!("{:04}-W{:02}", week.year(), week.week())
            })
        }
    }
}

/// A time of day as a time field takes it.
pub(crate) fn time_value(minutes: u32) -> String {
    format!("{:02}:{:02}", minutes / 60 % 24, minutes % 60)
}

/// The script that sets the field last clicked, with `value` passed as a
/// string the page cannot break out of.
fn set_script(value: &str) -> String {
    format!(
        "globalThis.enzimSetDateField && globalThis.enzimSetDateField({})",
        serde_json::Value::String(value.to_owned())
    )
}

/// Where a picker of `size` goes for a field at `field` (in the window):
/// under it, or over it where there is more room, and inside the window.
pub(crate) fn picker_place(field: iced::Rectangle, size: iced::Size, window: iced::Size) -> iced::Point {
    let margin = 8.0;
    let below = window.height - field.y - field.height - margin;
    let above = field.y - margin;
    let y = if below >= size.height || below >= above {
        field.y + field.height + 2.0
    } else {
        field.y - size.height - 2.0
    };
    iced::Point {
        x: field
            .x
            .clamp(margin, (window.width - size.width - margin).max(margin)),
        y: y.clamp(margin, (window.height - size.height - margin).max(margin)),
    }
}

impl App {
    /// The page's script reported a click on a date or time field, or that
    /// the page moved under an open picker.
    pub(super) fn date_field_message(&mut self, tab: u64, json: &str) {
        let Ok(message) = serde_json::from_str::<serde_json::Value>(json) else {
            return;
        };
        let text = |key: &str| message.get(key).and_then(serde_json::Value::as_str);
        if text("kind") != Some("open") {
            self.browser.date = None;
            return;
        }
        let Some(kind) = text("type").and_then(FieldKind::named) else {
            return;
        };
        let area: Vec<f32> = message
            .get("rect")
            .and_then(serde_json::Value::as_array)
            .map(|rect| {
                rect.iter()
                    .filter_map(serde_json::Value::as_f64)
                    .map(|part| part as f32)
                    .filter(|part| part.is_finite())
                    .collect()
            })
            .unwrap_or_default();
        let Ok(area) = <[f32; 4]>::try_from(area) else {
            return;
        };
        let value = text("value").unwrap_or_default().to_owned();
        let bound = |key: &str| text(key).and_then(|bound| field_date(kind, bound));
        let bounds =
            DateBounds::new(bound("min"), bound("max")).unwrap_or_else(DateBounds::unbounded);
        let selected = field_date(kind, &value);
        let today = {
            use chrono::Datelike;
            let now = chrono::Local::now().date_naive();
            Date::new(now.year(), now.month(), now.day())
        };
        let Some(start) = selected.or(today) else {
            return;
        };
        self.browser.date = Some(DateChoosing {
            tab,
            kind,
            value,
            area,
            months: MonthController::new(start, Weekday::Monday, bounds),
            selected,
        });
        self.browser.chrome_revision = self.browser.chrome_revision.wrapping_add(1);
    }

    /// Closes a picker whose page is no longer the one on show. Runs after
    /// every message.
    pub(crate) fn settle_date(&mut self) {
        let Some(choosing) = &self.browser.date else {
            return;
        };
        let active = self.browser.saved.tabs[self.browser.saved.active].key;
        if choosing.tab != active
            || self.browser.library.is_some()
            || self.browser.panel != Panel::None
        {
            self.browser.date = None;
        }
    }

    pub(super) fn date_command(&mut self, command: DateCommand) -> Task<Message> {
        if matches!(command, DateCommand::Set) {
            return Task::none();
        }
        let Some(choosing) = &mut self.browser.date else {
            return Task::none();
        };
        let chosen = match command {
            DateCommand::Calendar(event) => {
                enzui_widgets::calendar::apply_event(&mut choosing.months, event)
                    .map(|day| field_value(choosing.kind, day, &choosing.value))
            }
            DateCommand::Time(minutes) => Some(time_value(minutes)),
            DateCommand::Set => None,
        };
        self.browser.chrome_revision = self.browser.chrome_revision.wrapping_add(1);
        if let Some(value) = chosen.filter(|value| !value.is_empty()) {
            self.browser.date = None;
            // In the world the field's script lives in, where its setter is.
            let (job, receiver) = crate::native_jobs::Job::new();
            self.submit_browser_action(BrowserAction::Evaluate {
                source: set_script(&value),
                world: enzui_webkit::page::ScriptWorld::Isolated("enzim-browser-chrome".into()),
                job,
            });
            return Task::perform(crate::native_jobs::receive(receiver), |_| {
                c(Command::Date(DateCommand::Set))
            });
        }
        Task::none()
    }

    /// The picker of the field on show, over the page beside the field.
    pub(super) fn date_picker_over<'a>(&'a self, page: Element<'a, Message>) -> Element<'a, Message> {
        let Some(choosing) = &self.browser.date else {
            return page;
        };
        let surface = self.browser.input.bounds.map_or_else(
            || iced::Rectangle::with_size(self.browser.viewport),
            |(_, bounds)| bounds,
        );
        // The script measures in the page's pixels; the window's are those
        // times the zoom.
        let zoom = self.browser.zoom as f32 / 100.0;
        let field = iced::Rectangle {
            x: surface.x + choosing.area[0] * zoom,
            y: surface.y + choosing.area[1] * zoom,
            width: choosing.area[2] * zoom,
            height: choosing.area[3] * zoom,
        };
        let p = self.look().palette.clone();
        let (picker, size): (Element<'a, Message>, iced::Size) = if choosing.kind == FieldKind::Time {
            let now = choosing
                .value
                .split_once(':')
                .and_then(|(hour, minute)| {
                    Some(hour.parse::<u32>().ok()? * 60 + minute.get(..2)?.parse::<u32>().ok()?)
                });
            let mut times = column![].padding(4);
            for minutes in (0..24 * 60).step_by(30) {
                let (hover, ink) = (p.surface_hover, p.text);
                let chosen = now.is_some_and(|now| now / 30 * 30 == minutes);
                times = times.push(
                    iced::widget::button(text(time_value(minutes)).size(13))
                        .on_press(c(Command::Date(DateCommand::Time(minutes))))
                        .width(Length::Fill)
                        .padding([5, 10])
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
            let height = (self.browser.viewport.height - 80.0).clamp(120.0, 260.0);
            (
                enzui_widgets::scroll_view::enz_scrollable(times)
                    .height(Length::Fill)
                    .into(),
                iced::Size::new(110.0, height),
            )
        } else {
            // Seven columns and, with the month's name and the weekdays,
            // eight rows of cells, in a window as small as 360 px high.
            let cell = ((self.browser.viewport.height - 110.0) / 8.0).clamp(24.0, 34.0);
            let today = {
                use chrono::Datelike;
                let now = chrono::Local::now().date_naive();
                Date::new(now.year(), now.month(), now.day())
            };
            (
                enzui_widgets::calendar::enz_calendar(&choosing.months, |event| {
                    c(Command::Date(DateCommand::Calendar(event)))
                })
                .selected(choosing.selected)
                .today(today)
                .cell_size(cell)
                .into_element(),
                iced::Size::new(7.0 * cell + 18.0, 8.0 * cell + 26.0),
            )
        };
        let place = picker_place(field, size, self.browser.viewport);
        let (ground, edge) = (p.surface_elevated, p.border);
        // A list of times scrolls in the height given it; a calendar is as
        // tall as its month.
        let picker = container(picker)
            .width(size.width)
            .height(if choosing.kind == FieldKind::Time {
                Length::Fixed(size.height)
            } else {
                Length::Shrink
            })
            .style(move |_| container::Style {
                background: Some(ground.into()),
                border: iced::Border {
                    radius: 8.into(),
                    width: 1.0,
                    color: edge,
                },
                ..Default::default()
            });
        // A press beside the picker closes it and goes no further.
        let beside = iced::widget::mouse_area(
            container(picker)
                .padding(iced::Padding::default().left(place.x).top(place.y))
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .on_press(c(Command::Dismiss))
        .on_right_press(c(Command::Dismiss));
        iced::widget::stack![page, beside].into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(year: i32, month: u32, date: u32) -> Date {
        Date::new(year, month, date).expect("date")
    }

    #[test]
    fn a_fields_text_names_a_day_and_a_chosen_day_is_written_as_the_field_takes_it() {
        assert_eq!(field_date(FieldKind::Date, "2026-11-20"), Some(day(2026, 11, 20)));
        assert_eq!(
            field_date(FieldKind::DateTime, "2026-11-20T08:30"),
            Some(day(2026, 11, 20))
        );
        assert_eq!(field_date(FieldKind::Month, "2026-02"), Some(day(2026, 2, 1)));
        // The first week of 2026 starts in 2025.
        assert_eq!(field_date(FieldKind::Week, "2026-W01"), Some(day(2025, 12, 29)));
        for (kind, bad) in [
            (FieldKind::Date, ""),
            (FieldKind::Date, "2026-13-01"),
            (FieldKind::Date, "20/11/2026"),
            (FieldKind::Month, "2026"),
            (FieldKind::Week, "2026-W60"),
            (FieldKind::Time, "08:30"),
        ] {
            assert_eq!(field_date(kind, bad), None, "{bad}");
        }
        let chosen = day(2026, 3, 5);
        assert_eq!(field_value(FieldKind::Date, chosen, ""), "2026-03-05");
        assert_eq!(field_value(FieldKind::Month, chosen, "2025-01"), "2026-03");
        assert_eq!(field_value(FieldKind::Week, chosen, ""), "2026-W10");
        // The time of day typed before stays; without one it is noon.
        assert_eq!(
            field_value(FieldKind::DateTime, chosen, "2026-11-20T08:30"),
            "2026-03-05T08:30"
        );
        assert_eq!(field_value(FieldKind::DateTime, chosen, ""), "2026-03-05T12:00");
        assert_eq!(time_value(0), "00:00");
        assert_eq!(time_value(13 * 60 + 30), "13:30");
    }

    #[test]
    fn what_is_chosen_reaches_the_page_as_a_string_and_nothing_else() {
        assert_eq!(
            set_script("2026-03-05"),
            "globalThis.enzimSetDateField && globalThis.enzimSetDateField(\"2026-03-05\")"
        );
        // Whatever a value held, it stays inside its quotes.
        assert!(set_script("\");alert(1);(\"").ends_with("(\"\\\");alert(1);(\\\"\")"));
    }

    #[test]
    fn a_picker_opens_under_its_field_or_over_it_and_stays_in_the_window() {
        let window = iced::Size::new(800.0, 600.0);
        let size = iced::Size::new(250.0, 290.0);
        let field = |x, y| iced::Rectangle {
            x,
            y,
            width: 160.0,
            height: 30.0,
        };
        assert_eq!(picker_place(field(100.0, 100.0), size, window), iced::Point::new(100.0, 132.0));
        // No room below: over the field.
        assert_eq!(picker_place(field(100.0, 500.0), size, window), iced::Point::new(100.0, 208.0));
        // At the right edge it is moved in, and in a window too small for
        // it, it starts at the margin.
        assert_eq!(picker_place(field(700.0, 100.0), size, window).x, 542.0);
        let small = iced::Size::new(240.0, 200.0);
        assert_eq!(picker_place(field(10.0, 10.0), size, small), iced::Point::new(8.0, 8.0));
    }

    #[test]
    fn a_click_on_a_date_field_opens_its_calendar_and_a_day_closes_it() {
        let mut app = browser_tests::app();
        app.browser_mode = true;
        let tab = app.browser.saved.tabs[0].key;
        app.date_field_message(
            tab,
            r#"{"kind":"open","type":"date","value":"2026-11-20","min":"2026-11-01","max":"","rect":[10,20,160,30]}"#,
        );
        let choosing = app.browser.date.as_ref().expect("picker");
        assert_eq!(choosing.selected, Some(day(2026, 11, 20)));
        // Paging the months keeps it open; a day closes it.
        let command = |command| c(Command::Date(command));
        let _ = app.update(command(DateCommand::Calendar(CalendarEvent::Paged(1))));
        assert!(app.browser.date.is_some());
        let _ = app.update(command(DateCommand::Calendar(CalendarEvent::DaySelected(day(
            2026, 12, 3,
        )))));
        assert!(app.browser.date.is_none());

        // Not a field this browser draws a picker for, or not a message.
        for bad in [
            r#"{"kind":"open","type":"text","value":"","rect":[0,0,1,1]}"#,
            r#"{"kind":"open","type":"date","value":"","rect":[0,0,1]}"#,
            "{",
        ] {
            app.date_field_message(tab, bad);
            assert!(app.browser.date.is_none(), "{bad}");
        }
        // The page scrolled, the tab was left, or Escape: closed.
        let open = r#"{"kind":"open","type":"time","value":"08:30","min":"","max":"","rect":[10,20,160,30]}"#;
        app.date_field_message(tab, open);
        app.date_field_message(tab, r#"{"kind":"close"}"#);
        assert!(app.browser.date.is_none());
        app.date_field_message(tab + 1, open);
        app.settle_date();
        assert!(app.browser.date.is_none());
        app.date_field_message(tab, open);
        let _ = app.update(c(Command::Escape));
        assert!(app.browser.date.is_none());
    }
}
