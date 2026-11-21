use super::*;
use iced::widget::{Space, column, container, row, text};

impl App {
    pub(super) fn startup_settings(&self) -> Element<'_, Message> {
        let mode = match self.browser.saved.startup {
            crate::startup::Mode::Restore => 0,
            crate::startup::Mode::Empty => 1,
            crate::startup::Mode::Page => 2,
        };
        let options = vec![
            "Continue where you left off".into(),
            "Open an empty new tab".into(),
            "Open a specific page".into(),
        ];
        let select = enzui_widgets::combo_box::enz_combo_box(
            "",
            options.clone(),
            Some(options[mode].clone()),
            |_| c(Command::Startup(0)),
        )
        .selected_index(Some(mode))
        .on_select_index(|index| c(Command::Startup(index)))
        .aria_label("On startup")
        .id_prefix("settings-startup")
        .width(260)
        .into_element();
        let mut body = column![
            enzui_widgets::preferences_row::enz_preferences_row("On startup")
                .icon("power")
                .subtitle("Choose what opens when the browser restarts.")
                .control(select)
                .accessory_width(280.)
                .into_element()
        ]
        .spacing(8);
        if mode == 2 {
            body = body.push(
                enzui_widgets::text_field::enz_text_field(
                    "Startup page",
                    "https://example.com",
                    &self.browser.saved.startup_page,
                    |value| c(Command::StartupPage(value)),
                )
                .aria_label("Startup page address")
                .into_element(),
            );
            if crate::startup::page_url(&self.browser.saved.startup_page).is_none() {
                body = body.push(
                    text("Enter a valid website address. An empty new tab opens until one is set.")
                        .size(12)
                        .color(palette(&self.theme()).text_muted),
                );
            }
        }
        body.into()
    }

    pub(super) fn search_settings(&self) -> Element<'_, Message> {
        let engines = self.browser.saved.search.engines();
        let index = engines
            .iter()
            .position(|e| e.id == self.browser.saved.search.selected);
        let selected = engines.get(index.unwrap_or(0)).map(|e| e.name.clone());
        let selector = enzui_widgets::combo_box::enz_combo_box(
            "",
            engines.iter().map(|e| e.name.clone()).collect(),
            selected,
            |_| c(Command::SelectEngine(0)),
        )
        .id_prefix("settings-search-engine")
        .aria_label("Default search engine")
        .selected_index(index)
        .on_select_index(|i| c(Command::SelectEngine(i)))
        .width(190)
        .into_element();
        let mut body = column![
            enzui_widgets::preferences_row::enz_preferences_row("Search engine")
                .icon("search")
                .control(selector)
                .accessory_width(190.0)
                .into_element(),
            enzui_widgets::preferences_row::enz_preferences_row("Search suggestions")
                .icon("message-circle")
                .subtitle("Send search text to your search provider for suggestions, where supported. Off in private windows.")
                .switch(self.browser.saved.search.suggestions, |value| c(Command::SearchSuggestionsEnabled(value)))
                .into_element(),
            container(row![Space::new().width(Length::Fill),
                enzui_widgets::button::enz_button("Add custom engine", c(Command::ToggleEngineEditor))
                    .icon("plus").kind(enzui_widgets::button::ButtonKind::Quiet)
                    .width(Length::Shrink).into_element()
            ]).padding([0, 12])
        ].spacing(8).width(Length::Fill);
        for engine in &self.browser.saved.search.custom {
            body = body.push(
                container(
                    row![
                        text(engine.name.clone()).size(13).width(Length::Fill),
                        ink_icon(
                            "trash-2",
                            "Remove search engine",
                            c(Command::RemoveEngine(engine.id.clone())),
                            28.0,
                            14.0
                        )
                    ]
                    .spacing(10)
                    .align_y(iced::Alignment::Center),
                )
                .padding([0, 12]),
            );
        }
        body.into()
    }
    pub(super) fn search_engine_dialog(&self) -> Element<'_, Message> {
        let mut form = column![
            heading("search", "Add search engine", ""),
            field(
                "Name",
                &self.browser.engine_name,
                |q| c(Command::EngineName(q)),
                "engine-name"
            ),
            field(
                "Search URL with {query} or %s",
                &self.browser.engine_template,
                |q| c(Command::EngineTemplate(q)),
                "engine-template"
            ),
            text("Example: https://example.com/search?q={query}")
                .size(12)
                .color(palette(&self.theme()).text_muted),
        ]
        .spacing(12);
        if !self.browser.engine_error.is_empty() {
            form = form.push(text(&self.browser.engine_error).size(13));
        }
        form = form.push(
            row![
                Space::new().width(Length::Fill),
                action("Cancel", "x", Command::ToggleEngineEditor),
                primary("Add engine", "plus", Command::AddEngine)
            ]
            .spacing(8),
        );
        container(form)
            .padding(20)
            .width((self.browser.viewport.width - 32.).min(480.))
            .style(|theme| {
                let p = palette(theme);
                container::Style {
                    background: Some(p.surface_elevated.into()),
                    border: iced::Border {
                        radius: 14.into(),
                        width: 1.,
                        color: p.border,
                    },
                    ..Default::default()
                }
            })
            .into()
    }
}
