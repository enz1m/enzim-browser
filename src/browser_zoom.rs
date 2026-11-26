use super::*;
use enzui_core::menu_model::{CheckState, CloseCause, MenuEntryModel, MenuModel};
use enzui_widgets::menu_button::{MenuButtonEvent, apply_event};
use iced::widget::{Space, row, text};

pub(super) fn preset_model(current: i32, pdf: bool) -> MenuModel {
    let mut entries = vec![
        MenuEntryModel::command("reset", "Reset to 100%")
            .icon("rotate-ccw")
            .shortcut("Ctrl+0"),
    ];
    entries.push(MenuEntryModel::separator("presets"));
    entries.extend(
        (25..=if pdf { 400 } else { 300 })
            .step_by(25)
            .map(|percent| {
                MenuEntryModel::command(format!("zoom:{percent}"), format!("{percent}%"))
                    .radio(true)
                    .check(if percent == current {
                        CheckState::Checked
                    } else {
                        CheckState::Unchecked
                    })
            }),
    );
    MenuModel::new(entries)
}
fn adjacent(current: i32, increase: bool, maximum: i32) -> i32 {
    if increase {
        ((current / 25 + 1) * 25).clamp(25, maximum)
    } else {
        (((current - 1) / 25) * 25).clamp(25, maximum)
    }
}
impl App {
    fn current_zoom(&self) -> i32 {
        if self.browser.library.is_some() {
            return 100;
        }
        if self.browser.pdf.is_none()
            && InternalPage::from_url(&self.browser.saved.tabs[self.browser.saved.active].url)
                .is_some()
        {
            return 100;
        }
        self.browser.pdf.as_ref().map_or(self.browser.zoom, |page| {
            let size = self
                .browser
                .input
                .bounds
                .map_or(iced::Size::new(800., 600.), |(_, bounds)| bounds.size());
            (page.scale(size) * 100.).round() as i32
        })
    }
    pub(super) fn apply_page_zoom(&mut self, percent: i32) -> Task<Message> {
        if !(25..=400).contains(&percent) {
            return Task::none();
        }
        self.browser.input.anchor = None;
        if self.browser.pdf.is_some() {
            let current = self.current_zoom();
            if current == percent {
                return Task::none();
            }
            return self.pdf_command(crate::browser_pdf::PdfCommand::Zoom(if percent == 100 {
                0
            } else {
                percent - current
            }));
        }
        let percent = percent.min(300);
        self.browser.zoom = percent;
        self.webview.borrow_mut().zoom_percent = percent;
        self.remember_site_zoom(percent);
        self.sync_zoom_model();
        if percent == 100 {
            self.browser.zoom_menu_state.close(CloseCause::Activated);
        }
        self.browser.chrome_revision = self.browser.chrome_revision.wrapping_add(1);
        self.submit_browser_action(BrowserAction::Zoom(f64::from(percent) / 100.));
        Task::none()
    }
    pub(super) fn zoom_menu_event(&mut self, event: MenuButtonEvent) -> Task<Message> {
        if matches!(event, MenuButtonEvent::Open(_)) {
            self.browser.panel = Panel::None;
            self.browser.menu_state.close(CloseCause::Escape);
        }
        let task = enzui_widgets::menu_button::focus_restore_operation(
            &event,
            Some(&iced::widget::Id::new("address-zoom")),
        )
        .map_or_else(Task::none, iced::advanced::widget::operate);
        if let Some(id) = apply_event(
            &mut self.browser.zoom_menu_model,
            &mut self.browser.zoom_menu_state,
            &event,
        ) {
            let percent = if id.as_str() == "reset" {
                Some(100)
            } else {
                id.as_str()
                    .strip_prefix("zoom:")
                    .and_then(|v| v.parse().ok())
            };
            if let Some(percent) = percent {
                return Task::batch([task, self.browser_command(Command::ZoomTo(percent))]);
            }
        }
        task
    }
    pub(crate) fn sync_zoom_model(&mut self) {
        let key = (self.current_zoom(), self.browser.pdf.is_some());
        if self.browser.zoom_menu_key != key {
            self.browser.zoom_menu_key = key;
            self.browser.zoom_menu_model = preset_model(key.0, key.1);
        }
    }
    pub(super) fn address_zoom_button(&self) -> Element<'_, Message> {
        let percent = self.current_zoom();
        if percent == 100 {
            return Space::new().width(0).into();
        }
        enzui_widgets::menu_button::enz_menu_button(
            format!("{percent}%"),
            &self.browser.zoom_menu_model,
            &self.browser.zoom_menu_state,
            |event| c(Command::ZoomMenu(event)),
        )
        .kind(enzui_widgets::menu_button::MenuButtonKind::Quiet(
            enzui_widgets::button::ButtonSize::Sm,
        ))
        .aria_label("Page zoom")
        .menu_id(iced::widget::Id::new("address-zoom"))
        .popup_width(220.)
        .alignment(enzui_core::placement::Alignment::End)
        .into_element()
    }
    pub(super) fn clear_browser_menu_hover(&mut self) {
        let _ = apply_event(
            &mut self.browser.menu_model,
            &mut self.browser.menu_state,
            &MenuButtonEvent::SubmenuClosed,
        );
        self.browser.menu_model.clear_active();
    }
    pub(super) fn browser_zoom_row(&self) -> Element<'_, Message> {
        let percent = self.current_zoom();
        let enabled = self.browser.library.is_none()
            && (self.browser.pdf.is_some()
                || InternalPage::from_url(&self.browser.saved.tabs[self.browser.saved.active].url)
                    .is_none());
        let maximum = if self.browser.pdf.is_some() { 400 } else { 300 };
        let control = |symbol, label, value, available| {
            enzui_widgets::toolbar_icon::enz_toolbar_icon(symbol, c(Command::MenuZoom(value)))
                .aria_label(label)
                .id(iced::widget::Id::from(format!(
                    "browser-menu-zoom-{symbol}"
                )))
                .enabled(enabled && available)
                .into_element()
        };
        iced::widget::mouse_area(
            iced::widget::container(
                row![
                    text("Zoom").size(13).width(Length::Fill),
                    control(
                        "minus",
                        "Zoom out",
                        adjacent(percent, false, maximum),
                        percent > 25
                    ),
                    text(format!("{percent}%"))
                        .size(13)
                        .width(46)
                        .align_x(iced::Alignment::Center),
                    control(
                        "plus",
                        "Zoom in",
                        adjacent(percent, true, maximum),
                        percent < maximum
                    ),
                    control("rotate-ccw", "Reset zoom", 100, percent != 100)
                ]
                .spacing(4)
                .align_y(iced::Alignment::Center),
            )
            .padding([6, 8])
            .id(iced::widget::Id::new("browser-menu-zoom-row")),
        )
        .on_enter(c(Command::MenuZoomHover))
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_tests_zoom_hover_clears_previous_rows_and_submenus_without_closing_menu() {
        use enzui_core::menu_model::OpenCause;
        for (theme_name, theme) in [
            ("dark", enzui_theme::EnzTheme::dark()),
            ("light", enzui_theme::EnzTheme::light()),
        ] {
            for previous in ["containers", "bookmarks"] {
                for target in ["label", "minus", "plus", "padding"] {
                    let mut app = super::super::browser_tests::app();
                    app.theme = enzui_core::theme_file::integration::ThemeIntegration::unmanaged_compiled_theme(theme.clone()).0;
                    app.browser.private = true;
                    app.browser_mode = true;
                    app.browser.saved.tabs[0].url = "https://example.test/".into();
                    let _ = app
                        .browser_command(Command::Menu(MenuButtonEvent::Open(OpenCause::Pointer)));
                    let _ = app.browser_command(Command::Menu(MenuButtonEvent::Highlight(
                        previous.into(),
                    )));
                    assert_eq!(
                        app.browser
                            .menu_model
                            .active()
                            .map(enzui_core::menu_model::EntryId::as_str),
                        Some(previous)
                    );
                    assert_eq!(
                        app.browser.menu_state.open_submenu().is_some(),
                        previous == "containers"
                    );
                    let mut ui = iced_test::Simulator::with_size(
                        Default::default(),
                        iced::Size::new(700., 900.),
                        app.browser_menu_level(&app.browser.menu_model, true),
                    );
                    let row = ui
                        .find(iced::widget::Id::new("browser-menu-zoom-row"))
                        .expect("zoom row")
                        .bounds();
                    let point = match target {
                        "label" => ui.find("Zoom").expect("zoom label").bounds().center(),
                        "padding" => iced::Point::new(row.x + 1., row.center_y()),
                        _ => ui
                            .find(iced::widget::Id::from(format!(
                                "browser-menu-zoom-{target}"
                            )))
                            .expect("zoom control")
                            .bounds()
                            .center(),
                    };
                    if previous == "containers"
                        && target == "plus"
                        && let Some(output) = std::env::var_os("ENZIM_VISUAL_OUTPUT")
                    {
                        let path = std::path::PathBuf::from(output)
                            .join(format!("zoom-hover-{theme_name}-before"));
                        assert!(
                            ui.snapshot(&app.theme.iced_theme())
                                .expect("open submenu render")
                                .matches_image(path)
                                .expect("PNG capture")
                        );
                    }
                    ui.point_at(point);
                    ui.simulate([iced::Event::Mouse(iced::mouse::Event::CursorMoved {
                        position: point,
                    })]);
                    let messages = ui.into_messages().collect::<Vec<_>>();
                    assert!(
                        messages.iter().any(|message| matches!(
                            message,
                            Message::Chrome(Command::MenuZoomHover)
                        )),
                        "zoom hover must route from the whole row, including controls and padding"
                    );
                    for message in messages {
                        let _ = app.update(message);
                    }
                    assert!(
                        app.browser.menu_state.is_open(),
                        "zoom hover keeps the root menu open"
                    );
                    assert!(
                        app.browser.menu_model.active().is_none(),
                        "previous menu highlight must clear"
                    );
                    assert!(
                        app.browser.menu_state.open_submenu().is_none(),
                        "previous submenu must close"
                    );
                    let mut ui = iced_test::Simulator::with_size(
                        Default::default(),
                        iced::Size::new(700., 900.),
                        app.browser_menu_level(&app.browser.menu_model, true),
                    );
                    if previous == "containers"
                        && target == "plus"
                        && let Some(output) = std::env::var_os("ENZIM_VISUAL_OUTPUT")
                    {
                        let path = std::path::PathBuf::from(output)
                            .join(format!("zoom-hover-{theme_name}-after"));
                        assert!(
                            ui.snapshot(&app.theme.iced_theme())
                                .expect("cleared menu render")
                                .matches_image(path)
                                .expect("PNG capture")
                        );
                    }
                    ui.click(iced::widget::Id::new("browser-menu-zoom-plus"))
                        .expect("zoom in remains clickable");
                    for message in ui.into_messages().collect::<Vec<_>>() {
                        let _ = app.update(message);
                    }
                    assert_eq!(app.current_zoom(), 125);
                    assert!(app.browser.menu_state.is_open());
                    assert!(app.browser.menu_state.open_submenu().is_none());
                }
            }
        }
    }
    #[test]
    fn browser_tests_zoom_controls_keep_menu_open_and_presets_follow_the_selected_tab() {
        use enzui_core::menu_model::OpenCause;
        let mut app = App::new(enzui_theme::EnzTheme::dark());
        app.browser.saved = Saved::default();
        app.browser.saved.tabs.push(TabInfo {
            key: next_tab_key(),
            favicon: None,
            page_copy: None,
            title: "Example".into(),
            url: "https://example.org/".into(),
            pinned: false,
            group: None,
            container: None,
        });
        app.browser.sleeping = vec![None];
        app.browser_mode = true;
        app.browser.private = true;
        let first = app.browser.saved.tabs[0].key;
        let _ = app.browser_command(Command::Menu(MenuButtonEvent::Open(OpenCause::Pointer)));
        let _ = app.browser_command(Command::MenuZoom(adjacent(110, true, 300)));
        assert!(app.browser.menu_state.is_open());
        assert_eq!(app.browser.zoom, 125);
        assert_eq!(app.webview.borrow().zoom_percent, 125);
        let _ = app.browser_command(Command::ZoomMenu(MenuButtonEvent::Open(OpenCause::Pointer)));
        assert!(app.browser.zoom_menu_state.is_open());
        assert!(!app.browser.menu_state.is_open());
        let _ = app.browser_command(Command::ZoomMenu(MenuButtonEvent::Activated {
            id: "zoom:150".into(),
            check: CheckState::Checked,
        }));
        assert_eq!(app.browser.zoom, 150);
        assert!(!app.browser.zoom_menu_state.is_open());
        let _ = app.browser_command(Command::ZoomTo(150));
        assert_eq!(app.browser.zoom, 150);
        app.new_tab("https://example.net/".into());
        assert_eq!(app.browser.zoom, 100);
        let _ = app.browser_command(Command::ZoomTo(75));
        let index = app
            .browser
            .saved
            .tabs
            .iter()
            .position(|tab| tab.key == first)
            .unwrap();
        app.select_tab(index);
        assert_eq!(app.browser.zoom, 150);
        let _ = app.browser_command(Command::ZoomMenu(MenuButtonEvent::Open(OpenCause::Pointer)));
        let _ = app.browser_command(Command::ZoomMenu(MenuButtonEvent::Activated {
            id: "reset".into(),
            check: CheckState::None,
        }));
        assert_eq!(app.browser.zoom, 100);
        assert_eq!(app.webview.borrow().zoom_percent, 100);
        let _ = app.browser_command(Command::ZoomTo(-50));
        assert_eq!(app.browser.zoom, 100);
        assert_eq!(adjacent(110, false, 300), 100);
        assert_eq!(adjacent(25, false, 300), 25);
        assert_eq!(adjacent(300, true, 300), 300);
    }
}
