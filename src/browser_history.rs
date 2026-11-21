use super::*;
use crate::history::Range;
use iced::widget::{Space, column, container, row, text};

impl App {
    pub(super) fn refresh_history(&mut self) {
        let start = self
            .browser
            .history_range
            .start(chrono::Utc::now().timestamp_millis());
        let result = (|| -> Result<(), String> {
            self.browser.history_total = self
                .browser
                .history_store
                .count(&self.browser.history_filter, start)?;
            self.browser.history_page = self
                .browser
                .history_page
                .min(self.browser.history_total.saturating_sub(1) / 100);
            self.browser.history_matches = self.browser.history_store.page(
                &self.browser.history_filter,
                start,
                100,
                self.browser.history_page * 100,
            )?;
            self.browser.saved.history = self.browser.history_store.query("", 0)?;
            Ok(())
        })();
        if let Err(e) = result {
            self.browser
                .toasts
                .error("history", format!("History could not be read: {e}"));
        }
    }

    pub(super) fn history_page(&self) -> Element<'_, Message> {
        if self.browser.private {
            return self.page_document(
                "History",
                "venetian-mask",
                "Private browsing",
                column![
                    text("This window does not keep browsing history.").size(20).font(enzui_theme::text_defaults::font(enzui_theme::FontRole::Ui, enzui_theme::WeightRung::Semibold)),
                    text("Pages from private windows are never added to your regular history.")
                        .size(14)
                ]
                .spacing(12)
                .into(),
            );
        }
        let search = enzui_widgets::text_field::enz_text_field(
            "",
            "Search history",
            &self.browser.history_filter,
            |q| c(Command::HistoryFilter(q)),
        )
        .aria_label("Search history")
        .leading(container(glyph("search", 16.0)).width(18).height(18))
        .into_element();
        let range = enzui_widgets::combo_box::enz_combo_box(
            "",
            Range::OPTIONS.iter().map(|r| r.label().into()).collect(),
            Some(self.browser.history_range.label().into()),
            |label| {
                c(Command::HistoryRange(
                    Range::OPTIONS
                        .into_iter()
                        .find(|r| r.label() == label)
                        .unwrap_or_default(),
                ))
            },
        )
        .id_prefix("history-range")
        .aria_label("History time range")
        .width(180)
        .into_element();
        let clear = enzui_widgets::button::enz_button(
            if self.browser.history_filter.trim().is_empty()
                && self.browser.history_range == Range::All
            {
                "Clear all"
            } else {
                "Clear matching visits"
            },
            c(Command::HistoryClearRequest),
        )
        .icon("trash-2")
        .kind(enzui_widgets::button::ButtonKind::DestructiveOutlined)
        .enabled(self.browser.history_total > 0)
        .into_element();
        let tools: Element<'_, Message> = if self.content_width() < 600.0 {
            column![range, clear].spacing(10).into()
        } else {
            row![range, Space::new().width(Length::Fill), clear]
                .spacing(12)
                .align_y(iced::Alignment::Center)
                .into()
        };
        let mut body = column![search, tools].spacing(16).width(Length::Fill);
        if self.browser.history_matches.is_empty() {
            body = body.push(
                container(
                    column![
                        container(glyph("history", 32.0)).width(36).height(36),
                        text(if self.browser.history_filter.is_empty() {
                            "No visits in this period"
                        } else {
                            "No matching visits"
                        })
                        .size(20).font(enzui_theme::text_defaults::font(enzui_theme::FontRole::Ui, enzui_theme::WeightRung::Semibold)),
                        text("Try another time range or search.").size(13)
                    ]
                    .spacing(16)
                    .align_x(iced::Alignment::Center),
                )
                .padding([36, 12])
                .center_x(Length::Fill),
            );
        }
        let mut date = String::new();
        for visit in &self.browser.history_matches {
            let label = visit.date_label();
            if label != date {
                body = body.push(
                    container(text(label.clone()).size(12)).padding(iced::Padding {
                        top: 16.0,
                        right: 0.0,
                        bottom: 0.0,
                        left: 0.0,
                    }),
                );
                date = label;
            }
            let label = column![
                text(visit.title.clone())
                    .size(14)
                    .wrapping(iced::widget::text::Wrapping::None)
                    .ellipsis(iced::widget::text::Ellipsis::End),
                text(visit.url.clone())
                    .size(12)
                    .wrapping(iced::widget::text::Wrapping::None)
                    .ellipsis(iced::widget::text::Ellipsis::End),
            ]
            .spacing(4)
            .width(Length::Fill);
            let open = enzui_widgets::focus_button::focus_button(
                row![
                    text(visit.time_label()).size(11).width(42),
                    self.site_icon(&visit.url, 18),
                    label
                ]
                .spacing(12)
                .align_y(iced::Alignment::Center),
                c(Command::Visit(visit.url.clone())),
            )
            .width(Length::Fill)
            .padding([6, 8])
            .style_focus(|theme, _| iced::widget::button::Style {
                background: None,
                text_color: palette(theme).text,
                ..Default::default()
            });
            body = body.push(
                enzui_widgets::hover_group::enz_hover_group(
                    row![
                        open,
                        ink_icon(
                            "trash-2",
                            "Remove visit",
                            c(Command::HistoryDelete(visit.id)),
                            28.0,
                            14.0
                        )
                    ]
                    .align_y(iced::Alignment::Center),
                )
                .width(Length::Fill)
                .style(
                    |theme, hovered| enzui_widgets::hover_group::HoverGroupStyle {
                        background: if hovered {
                            palette(theme).surface_hover
                        } else {
                            iced::Color::TRANSPARENT
                        },
                        border_color: iced::Color::TRANSPARENT,
                        border_width: 0.,
                        radius: 7.,
                    },
                )
                .into_element(),
            );
        }
        if self.browser.history_total > 100 {
            body = body.push(
                row![
                    enzui_widgets::toolbar_icon::enz_toolbar_icon(
                        "chevron-left",
                        c(Command::HistoryPage(-1))
                    )
                    .aria_label("Previous history page")
                    .enabled(self.browser.history_page > 0)
                    .into_element(),
                    text(format!(
                        "Page {} of {}",
                        self.browser.history_page + 1,
                        self.browser.history_total.div_ceil(100)
                    ))
                    .size(13),
                    enzui_widgets::toolbar_icon::enz_toolbar_icon(
                        "chevron-right",
                        c(Command::HistoryPage(1))
                    )
                    .aria_label("Next history page")
                    .enabled((self.browser.history_page + 1) * 100 < self.browser.history_total)
                    .into_element()
                ]
                .spacing(12)
                .align_y(iced::Alignment::Center),
            );
        }
        self.page_document(
            "History",
            "history",
            format!("{} matching visits", self.browser.history_total),
            body.into(),
        )
    }

    pub(super) fn history_clear_dialog(&self) -> Element<'_, Message> {
        let Some((filter, _, count)) = &self.browser.history_cleanup else {
            return Space::new().into();
        };
        let display_filter: String = filter.chars().take(80).collect();
        let display_filter = if display_filter.len() < filter.len() {
            format!("{display_filter}…")
        } else {
            display_filter
        };
        let scope = if filter.is_empty() {
            self.browser.history_range.label().into()
        } else {
            format!(
                "{} · matching “{display_filter}”",
                self.browser.history_range.label()
            )
        };
        container(column![
            heading("trash-2","Clear browsing history",scope),
            text(format!("Remove {count} matching visits? Bookmarks and website sign-ins will stay saved.")).size(14),
            row![Space::new().width(Length::Fill),action("Cancel","x",Command::Dismiss),
                enzui_widgets::button::enz_button("Clear visits",c(Command::HistoryClearConfirm)).icon("trash-2")
                    .kind(enzui_widgets::button::ButtonKind::Destructive).into_element()
            ].spacing(10)
        ].spacing(20)).padding(20).width((self.browser.viewport.width-32.0).min(480.0))
            .style(|theme| { let p=palette(theme);container::Style {background:Some(p.surface_elevated.into()),border:iced::Border{radius:14.0.into(),color:p.border,width:1.0},..Default::default()} }).into()
    }
}
