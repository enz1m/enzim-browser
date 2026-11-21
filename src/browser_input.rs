use crate::{App, Message};
use enzui_webkit::{
    input::{InputEvent, Modifiers, ScrollUnit, ViewPoint},
    web::ViewId,
};
use iced::{Element, Point, Rectangle, Task, widget::canvas};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Surface {
    Web(ViewId),
    Pdf(u64),
}
#[derive(Debug, Clone)]
pub(crate) enum InputCommand {
    Bounds(Surface, Rectangle),
    Start {
        surface: Surface,
        window: Point,
        local: iced::Vector,
    },
    Release,
    Cancel,
    Click(iced::mouse::Button),
    Tick(std::time::Instant),
    Enabled(bool),
}
#[derive(Debug, Clone)]
pub(crate) struct Anchor {
    pub(crate) surface: Surface,
    pub(crate) window: Point,
    pub(crate) local: Point,
    pub(crate) last: std::time::Instant,
}
#[derive(Default)]
pub(crate) struct State {
    pub(crate) bounds: Option<(Surface, Rectangle)>,
    pub(crate) hovered_link: Option<String>,
    pub(crate) anchor: Option<Anchor>,
}
pub(crate) fn speed(distance: f32) -> f32 {
    let distance = distance.abs() - 12.;
    if distance <= 0. {
        0.
    } else {
        (distance * distance * 0.22 + distance * 4.).min(2600.)
    }
}
impl App {
    pub(crate) fn surface_input<'a>(
        &'a self,
        content: Element<'a, Message>,
        surface: Surface,
    ) -> Element<'a, Message> {
        let anchor = self
            .browser
            .input
            .anchor
            .as_ref()
            .filter(|a| a.surface == surface);
        let p = self.look().palette.clone();
        let content = iced::widget::stack![
            content,
            canvas::Canvas::new(Indicator {
                at: anchor.map(|a| a.local),
                ink: p.text,
                fill: p.surface_elevated
            })
            .width(iced::Length::Fill)
            .height(iced::Length::Fill)
        ]
        .into();
        crate::surface_input::surface_input(
            content,
            surface,
            self.browser.saved.middle_autoscroll
                && self.browser.panel == crate::browser::Panel::None
                && !self.browser.chooser.is_fallback_open()
                && (matches!(surface, Surface::Pdf(_))
                    || self.browser.input.hovered_link.is_none()),
            anchor.is_some(),
            if matches!(surface, Surface::Web(_)) {
                self.browser.input.hovered_link.as_ref().and_then(|link| {
                    url::Url::parse(link)
                        .or_else(|_| {
                            url::Url::parse(&self.browser.saved.tabs[self.browser.saved.active].url)
                                .and_then(|base| base.join(link))
                        })
                        .ok()
                        .filter(|u| matches!(u.scheme(), "http" | "https"))
                        .map(Into::into)
                })
            } else {
                None
            },
        )
    }
    pub(crate) fn input_command(&mut self, command: InputCommand) -> Task<Message> {
        match command {
            InputCommand::Bounds(surface, bounds) => {
                if self
                    .browser
                    .input
                    .bounds
                    .is_some_and(|old| old != (surface, bounds))
                {
                    self.browser.input.anchor = None;
                    self.browser.chrome_revision = self.browser.chrome_revision.wrapping_add(1);
                }
                self.browser.input.bounds = Some((surface, bounds));
            }
            InputCommand::Start {
                surface,
                window,
                local,
            } => {
                let current = match surface {
                    Surface::Pdf(key) => {
                        self.browser.pdf.is_some()
                            && self.browser.saved.tabs[self.browser.saved.active].key == key
                    }
                    Surface::Web(view) => {
                        self.browser.pdf.is_none() && self.webview.borrow().live_view == Some(view)
                    }
                };
                if !self.browser.saved.middle_autoscroll
                    || !current
                    || self.browser.panel != crate::browser::Panel::None
                    || self.browser.chooser.is_fallback_open()
                {
                    return Task::none();
                }
                self.browser.input.anchor = Some(Anchor {
                    surface,
                    window,
                    local: Point::new(local.x, local.y),
                    last: std::time::Instant::now(),
                });
                self.browser.chrome_revision = self.browser.chrome_revision.wrapping_add(1);
            }
            InputCommand::Enabled(value) => {
                self.browser.saved.middle_autoscroll = value;
                self.browser.input.anchor = None;
                self.browser.save();
            }
            InputCommand::Click(button) => {
                if button != iced::mouse::Button::Middle
                    || self
                        .browser
                        .input
                        .bounds
                        .is_none_or(|(_, bounds)| !bounds.contains(self.browser.pointer))
                {
                    return self.input_command(InputCommand::Cancel);
                }
            }
            InputCommand::Cancel => {
                if self.browser.input.anchor.take().is_some() {
                    self.browser.chrome_revision = self.browser.chrome_revision.wrapping_add(1);
                }
            }
            InputCommand::Release => {
                if self
                    .browser
                    .input
                    .anchor
                    .as_ref()
                    .is_some_and(|a| a.window.distance(self.browser.pointer) > 12.)
                {
                    self.browser.input.anchor = None;
                    self.browser.chrome_revision = self.browser.chrome_revision.wrapping_add(1);
                }
            }
            InputCommand::Tick(now) => {
                let valid = self.browser.input.anchor.as_ref().is_some_and(|a| {
                    self.browser.panel == crate::browser::Panel::None
                        && match a.surface {
                            Surface::Web(view) => {
                                self.webview.borrow().live_view == Some(view)
                                    && self.browser.pdf.is_none()
                            }
                            Surface::Pdf(key) => {
                                self.browser.pdf.is_some()
                                    && self.browser.saved.tabs[self.browser.saved.active].key == key
                            }
                        }
                });
                if !valid {
                    if self.browser.input.anchor.take().is_some() {
                        self.browser.chrome_revision = self.browser.chrome_revision.wrapping_add(1);
                    }
                    return Task::none();
                }
                if let Some(a) = self.browser.input.anchor.as_mut() {
                    let dt = now
                        .saturating_duration_since(a.last)
                        .as_secs_f32()
                        .min(0.05);
                    a.last = now;
                    let distance = self.browser.pointer.y - a.window.y;
                    let delta = speed(distance) * distance.signum() * dt;
                    if delta != 0. {
                        match a.surface {
                            Surface::Web(view) => {
                                self.webview.borrow_mut().route_widget_input(
                                    &InputEvent::WheelScrolledAt {
                                        view,
                                        at: ViewPoint {
                                            x: a.local.x,
                                            y: a.local.y,
                                        },
                                        delta_x: 0.,
                                        delta_y: -delta,
                                        unit: ScrollUnit::Pixels,
                                        modifiers: Modifiers::NONE,
                                    },
                                );
                            }
                            Surface::Pdf(key) => {
                                return iced::advanced::widget::operate(
                                    iced::advanced::widget::operation::scrollable::scroll_by(
                                        iced::advanced::widget::Id::from(format!(
                                            "pdf-scroll-{key}"
                                        )),
                                        iced::widget::scrollable::AbsoluteOffset {
                                            x: 0.,
                                            y: delta,
                                        },
                                        iced::advanced::widget::operation::Animation::Instant,
                                    ),
                                );
                            }
                        }
                    }
                }
            }
        }
        Task::none()
    }
}
struct Indicator {
    at: Option<Point>,
    ink: iced::Color,
    fill: iced::Color,
}
impl canvas::Program<Message> for Indicator {
    type State = ();
    fn draw(
        &self,
        (): &(),
        renderer: &iced::Renderer,
        _: &iced::Theme,
        bounds: Rectangle,
        _: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let Some(at) = self.at else {
            return Vec::new();
        };
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let circle = canvas::Path::circle(at, 14.);
        frame.fill(&circle, self.fill);
        frame.stroke(
            &circle,
            canvas::Stroke::default()
                .with_color(self.ink.scale_alpha(0.55))
                .with_width(1.),
        );
        let arrows = canvas::Path::new(|b| {
            b.move_to(Point::new(at.x - 4., at.y - 4.));
            b.line_to(Point::new(at.x, at.y - 8.));
            b.line_to(Point::new(at.x + 4., at.y - 4.));
            b.move_to(Point::new(at.x - 4., at.y + 4.));
            b.line_to(Point::new(at.x, at.y + 8.));
            b.line_to(Point::new(at.x + 4., at.y + 4.));
        });
        frame.stroke(
            &arrows,
            canvas::Stroke::default()
                .with_color(self.ink)
                .with_width(1.5),
        );
        vec![frame.into_geometry()]
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_tests_scroll_marker_preserves_the_document_scroll_position() {
        use iced::advanced::widget::{
            Id, Operation,
            operation::{
                Animation, Scrollable,
                scrollable::{AbsoluteOffset, scroll_to},
            },
        };
        #[derive(Default)]
        struct Offset(Option<f32>);
        impl Operation for Offset {
            fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
                visit(self);
            }
            fn scrollable(
                &mut self,
                _: Option<&Id>,
                _: Rectangle,
                _: iced::Size,
                translation: iced::Vector,
                _: &mut dyn Scrollable,
            ) {
                self.0 = Some(translation.y);
            }
        }
        let idle = App::new(enzui_theme::EnzTheme::dark());
        let mut active = App::new(enzui_theme::EnzTheme::dark());
        let surface = Surface::Pdf(19);
        active.browser.input.anchor = Some(Anchor {
            surface,
            window: Point::new(200., 200.),
            local: Point::new(200., 200.),
            last: std::time::Instant::now(),
        });
        let child = || -> Element<'_, Message> {
            iced::widget::scrollable(iced::widget::Space::new().height(3000).width(500))
                .id(Id::new("marker-scroll"))
                .height(iced::Length::Fill)
                .into()
        };
        let mut ui = iced_test::Simulator::with_size(
            Default::default(),
            iced::Size::new(500., 400.),
            idle.surface_input(child(), surface),
        );
        ui.operate(&mut scroll_to(
            Id::new("marker-scroll"),
            AbsoluteOffset {
                x: None,
                y: Some(400.),
            },
            Animation::Instant,
        ));
        let mut before = Offset::default();
        ui.operate(&mut before);
        assert_eq!(before.0, Some(400.));
        ui = ui.rebuild(active.surface_input(child(), surface));
        let mut after = Offset::default();
        ui.operate(&mut after);
        assert_eq!(after.0, before.0);
    }
    #[test]
    fn browser_tests_command_zoom_does_not_type_into_native_fields_and_altgr_still_types() {
        let field = || {
            enzui_widgets::text_field::enz_text_field("", "Address", "", |s| {
                Message::Chrome(crate::browser::Command::Query(s))
            })
            .into_element()
        };
        let mut ui = iced_test::Simulator::with_size(
            Default::default(),
            iced::Size::new(500., 100.),
            field(),
        );
        ui.point_at((100., 16.));
        ui.simulate(iced_test::simulator::click());
        let press = |value: &str, modifiers| {
            let mut event = iced_test::simulator::press_key(
                iced::keyboard::Key::Character(value.into()),
                Some(value.into()),
            );
            if let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                modifiers: state,
                ..
            }) = &mut event
            {
                *state = modifiers;
            }
            event
        };
        ui.simulate([
            iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(
                iced::keyboard::Modifiers::CTRL,
            )),
            press("0", iced::keyboard::Modifiers::CTRL),
        ]);
        assert!(
            !ui.drain()
                .any(|m| matches!(m, Message::Chrome(crate::browser::Command::Query(_))))
        );
        ui.simulate([
            iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(
                iced::keyboard::Modifiers::CTRL | iced::keyboard::Modifiers::ALT,
            )),
            press(
                "@",
                iced::keyboard::Modifiers::CTRL | iced::keyboard::Modifiers::ALT,
            ),
        ]);
        assert!(ui.drain().any(
            |m| matches!(m,Message::Chrome(crate::browser::Command::Query(value)) if value=="@")
        ));
    }
    #[test]
    fn browser_tests_address_first_focus_replaces_all_but_later_click_places_caret() {
        let make = |select| {
            enzui_widgets::text_field::enz_text_field(
                "",
                "Address",
                "https://example.org/a-long-address",
                |s| Message::Chrome(crate::browser::Command::Query(s)),
            )
            .leading(iced::widget::text("⌕"))
            .select_all_on_focus(select)
            .into_element()
        };
        let type_x = || {
            iced_test::simulator::press_key(
                iced::keyboard::Key::Character("x".into()),
                Some("x".into()),
            )
        };
        for (select, second_click, replace_all) in [
            (true, false, true),
            (true, true, false),
            (false, false, false),
        ] {
            let mut ui = iced_test::Simulator::with_size(
                Default::default(),
                iced::Size::new(500., 100.),
                make(select),
            );
            ui.point_at((100., 16.));
            ui.simulate(iced_test::simulator::click());
            if second_click {
                ui.point_at((220., 16.));
                ui.simulate(iced_test::simulator::click());
            }
            ui.simulate([type_x()]);
            let values: Vec<_> = ui
                .drain()
                .filter_map(|message| match message {
                    Message::Chrome(crate::browser::Command::Query(value)) => Some(value),
                    _ => None,
                })
                .collect();
            assert_eq!(values.len(), 1);
            assert_eq!(
                values[0] == "x",
                replace_all,
                "select={select}, second={second_click}, values={values:?}"
            );
        }
    }
}
