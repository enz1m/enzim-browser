use super::*;
use iced::widget::{Space, column, container, row, text};
impl App {
    /// Below this content width the find field sits above its controls.
    fn find_bar_stacked(&self) -> bool {
        self.content_width() < 620.
    }
    /// How far the find bar's top sits above the window's bottom edge (its
    /// height plus the page's 10 px margin), for keeping toasts above it.
    pub(crate) fn find_bar_clearance(&self) -> f32 {
        10. + if self.find_bar_stacked() { 94. } else { 52. }
    }
    pub(super) fn find_bar(&self) -> Element<'_, Message> {
        let input = enzui_widgets::text_field::enz_text_field(
            "",
            "Find in page",
            &self.browser.find,
            |q| c(Command::Find(q)),
        )
        .id(iced::advanced::widget::Id::new("page-find"))
        .aria_label("Find in page")
        .leading(glyph("search", 15.))
        .on_submit(c(Command::FindNext(false)))
        .into_element();
        let controls = row![
            text(self.browser.find_status.clone()).size(12),
            Space::new().width(Length::Fill),
            enzui_widgets::button::enz_button("Aa", c(Command::FindCase))
                .aria_label(if self.browser.find_case {
                    "Match case: on"
                } else {
                    "Match case: off"
                })
                .kind(if self.browser.find_case {
                    enzui_widgets::button::ButtonKind::Accent
                } else {
                    enzui_widgets::button::ButtonKind::Quiet
                })
                .into_element(),
            ink_icon(
                "chevron-up",
                "Previous match",
                c(Command::FindNext(true)),
                28.,
                14.
            ),
            ink_icon(
                "chevron-down",
                "Next match",
                c(Command::FindNext(false)),
                28.,
                14.
            ),
            enzui_widgets::button::enz_button("Done", c(Command::Dismiss))
                .aria_label("Close find")
                .kind(enzui_widgets::button::ButtonKind::Quiet)
                .size(enzui_widgets::button::ButtonSize::Sm)
                .into_element()
        ]
        .spacing(4)
        .align_y(iced::Alignment::Center);
        let body: Element<'_, Message> = if self.find_bar_stacked() {
            column![input, controls].spacing(6).into()
        } else {
            row![
                container(input).width(Length::FillPortion(2)),
                container(controls).width(Length::FillPortion(3))
            ]
            .spacing(12)
            .align_y(iced::Alignment::Center)
            .into()
        };
        container(body)
            .padding([8, 14])
            .width(Length::Fill)
            .style(|theme: &iced::Theme| {
                let p = palette(theme);
                container::Style {
                    background: Some(p.surface_elevated.into()),
                    ..Default::default()
                }
            })
            .into()
    }
}
