use crate::{
    Message,
    browser::Command,
    browser_input::{InputCommand, Surface},
};
use iced::advanced::{
    Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{Operation, Tree, tree},
};
use iced::{Element, Event, Length, Rectangle, Size, Vector};

#[derive(Default)]
struct State {
    modifiers: iced::keyboard::Modifiers,
    bounds: Option<Rectangle>,
    target: Option<Surface>,
    middle: bool,
    swallow: Option<iced::mouse::Button>,
    wheel: f32,
}
pub(crate) struct SurfaceInput<'a> {
    content: Element<'a, Message>,
    target: Surface,
    allow_middle: bool,
    active: bool,
    link: Option<String>,
}
pub(crate) fn surface_input<'a>(
    content: Element<'a, Message>,
    target: Surface,
    allow_middle: bool,
    active: bool,
    link: Option<String>,
) -> Element<'a, Message> {
    Element::new(SurfaceInput {
        content,
        target,
        allow_middle,
        active,
        link,
    })
}
impl Widget<Message, iced::Theme, iced::Renderer> for SurfaceInput<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }
    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
        let state = tree.state.downcast_mut::<State>();
        if state.target != Some(self.target) {
            *state = State {
                target: Some(self.target),
                modifiers: state.modifiers,
                swallow: state
                    .swallow
                    .or_else(|| state.middle.then_some(iced::mouse::Button::Middle)),
                ..Default::default()
            };
        }
    }
    fn frame_interest(&self, tree: &Tree) -> bool {
        tree.children.iter().any(Tree::has_frame_interest)
    }
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }
    fn layout(&mut self, tree: &mut Tree, renderer: &iced::Renderer, limits: &layout::Limits) {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        iced::advanced::layout::pass_through(tree, 0);
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content.as_widget_mut().operate(
            &mut tree.children[0],
            layout,
            viewport,
            renderer,
            operation,
        );
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        if state.bounds != Some(layout.bounds()) {
            state.bounds = Some(layout.bounds());
            shell.publish(Message::Chrome(Command::Input(InputCommand::Bounds(
                self.target,
                layout.bounds(),
            ))));
        }
        if let Event::Keyboard(
            iced::keyboard::Event::KeyPressed { key, modifiers, .. }
            | iced::keyboard::Event::KeyReleased { key, modifiers, .. },
        ) = event
            && modifiers.control()
            && !modifiers.alt()
            && matches!(
                key.as_ref(),
                iced::keyboard::Key::Character("+" | "=" | "-" | "_" | "0")
            )
        {
            state.modifiers = *modifiers;
            shell.capture_event();
            return;
        }
        match event {
            Event::Keyboard(iced::keyboard::Event::ModifiersChanged(m)) => state.modifiers = *m,
            Event::Keyboard(
                iced::keyboard::Event::KeyPressed { modifiers, .. }
                | iced::keyboard::Event::KeyReleased { modifiers, .. },
            ) => state.modifiers = *modifiers,
            Event::Window(iced::window::Event::Unfocused) => {
                state.modifiers = iced::keyboard::Modifiers::default();
                state.middle = false;
                state.swallow = None;
                if self.active {
                    shell.publish(Message::Chrome(Command::Input(InputCommand::Cancel)));
                }
            }
            Event::Mouse(iced::mouse::Event::CursorLeft) if self.active => {
                shell.publish(Message::Chrome(Command::Input(InputCommand::Cancel)));
            }
            Event::Mouse(iced::mouse::Event::ButtonReleased(button))
                if state.swallow == Some(*button) =>
            {
                state.swallow = None;
                shell.capture_event();
                return;
            }
            Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Middle))
                if state.middle =>
            {
                state.middle = false;
                shell.publish(Message::Chrome(Command::Input(InputCommand::Release)));
                if matches!(self.target, Surface::Pdf(_)) {
                    shell.capture_event();
                    return;
                }
            }
            Event::Mouse(iced::mouse::Event::ButtonPressed(button))
                if self.active && cursor.is_over(layout.bounds()) =>
            {
                state.swallow = Some(*button);
                shell.publish(Message::Chrome(Command::Input(InputCommand::Cancel)));
                shell.capture_event();
                return;
            }
            Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Middle))
                if self.link.is_some() && cursor.is_over(layout.bounds()) =>
            {
                if let Some(link) = &self.link {
                    state.swallow = Some(iced::mouse::Button::Middle);
                    shell.publish(Message::Chrome(Command::Action(
                        crate::browser::actions::ActionCommand::VisitBackground(link.clone()),
                    )));
                }
                shell.capture_event();
                return;
            }
            Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Middle))
                if self.allow_middle && cursor.is_over(layout.bounds()) =>
            {
                if let Some(point) = cursor.position() {
                    state.middle = true;
                    shell.publish(Message::Chrome(Command::Input(InputCommand::Start {
                        surface: self.target,
                        window: point,
                        local: point - layout.position(),
                    })));
                }
                if matches!(self.target, Surface::Pdf(_)) {
                    shell.capture_event();
                    return;
                }
            }
            Event::Mouse(iced::mouse::Event::WheelScrolled { delta })
                if cursor.is_over(layout.bounds())
                    && state.modifiers.control()
                    && !state.modifiers.alt() =>
            {
                let step = match delta {
                    iced::mouse::ScrollDelta::Lines { y, .. } => y.signum(),
                    iced::mouse::ScrollDelta::Pixels { y, .. } => *y / 60.,
                };
                state.wheel += step;
                let steps = state.wheel.trunc().clamp(-4., 4.) as i32;
                if steps != 0 {
                    state.wheel -= steps as f32;
                    shell.publish(Message::Chrome(Command::Zoom(steps * 10)));
                }
                shell.capture_event();
                return;
            }
            Event::Mouse(iced::mouse::Event::WheelScrolled { .. }) if self.active => {
                shell.publish(Message::Chrome(Command::Input(InputCommand::Cancel)));
                shell.capture_event();
                return;
            }
            Event::Mouse(iced::mouse::Event::CursorMoved { .. }) if self.active => {
                shell.capture_event();
                return;
            }
            _ => {}
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            shell,
            viewport,
        );
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        if self.active && cursor.is_over(layout.bounds()) {
            mouse::Interaction::Move
        } else {
            self.content.as_widget().mouse_interaction(
                &tree.children[0],
                layout,
                cursor,
                viewport,
                renderer,
            )
        }
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
        window: Size,
    ) -> Vec<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
            window,
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn ui(enabled: bool, active: bool) -> iced_test::Simulator<'static, Message> {
        let child = iced::widget::mouse_area(
            iced::widget::Space::new()
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .on_scroll(|_| Message::Chrome(Command::PdfClose))
        .on_middle_press(Message::Chrome(Command::PdfClose));
        iced_test::Simulator::with_size(
            Default::default(),
            Size::new(500., 400.),
            surface_input(child.into(), Surface::Pdf(9), enabled, active, None),
        )
    }
    #[test]
    fn browser_tests_web_autoscroll_preserves_the_native_middle_gesture() {
        let child = || {
            iced::widget::mouse_area(
                iced::widget::Space::new()
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .on_middle_press(Message::Chrome(Command::PdfClose))
            .on_middle_release(Message::Chrome(Command::Print))
        };
        let surface = Surface::Web(enzui_webkit::web::ViewId::new(9));
        let mut view = iced_test::Simulator::with_size(
            Default::default(),
            Size::new(500., 400.),
            surface_input(child().into(), surface, true, false, None),
        );
        view.point_at((200., 200.));
        view.simulate([Event::Mouse(iced::mouse::Event::ButtonPressed(
            iced::mouse::Button::Middle,
        ))]);
        let mut messages: Vec<_> = view.drain().collect();
        view = view.rebuild(surface_input(child().into(), surface, true, true, None));
        view.simulate([Event::Mouse(iced::mouse::Event::ButtonReleased(
            iced::mouse::Button::Middle,
        ))]);
        messages.extend(view.drain());
        assert!(messages.iter().any(|m| matches!(
            m,
            Message::Chrome(Command::Input(InputCommand::Start { .. }))
        )));
        assert!(
            messages
                .iter()
                .any(|m| matches!(m, Message::Chrome(Command::PdfClose)))
        );
        assert!(
            messages
                .iter()
                .any(|m| matches!(m, Message::Chrome(Command::Print)))
        );
    }
    #[test]
    fn browser_tests_middle_link_opens_tab_without_autoscroll_or_native_press() {
        let child = || {
            iced::widget::mouse_area(
                iced::widget::Space::new()
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .on_middle_press(Message::Chrome(Command::PdfClose))
            .on_middle_release(Message::Chrome(Command::PdfClose))
        };
        let mut view = iced_test::Simulator::with_size(
            Default::default(),
            Size::new(500., 400.),
            surface_input(
                child().into(),
                Surface::Pdf(9),
                true,
                false,
                Some("https://example.com/".into()),
            ),
        );
        view.point_at((200., 200.));
        view.simulate([Event::Mouse(iced::mouse::Event::ButtonPressed(
            iced::mouse::Button::Middle,
        ))]);
        let mut messages: Vec<_> = view.drain().collect();
        view = view.rebuild(surface_input(
            child().into(),
            Surface::Pdf(10),
            true,
            false,
            None,
        ));
        view.simulate([Event::Mouse(iced::mouse::Event::ButtonReleased(
            iced::mouse::Button::Middle,
        ))]);
        messages.extend(view.drain());
        // In a tab of its own, behind the page on show.
        assert!(messages.iter().any(|m| matches!(
            m,
            Message::Chrome(Command::Action(
                crate::browser::actions::ActionCommand::VisitBackground(url)
            )) if url == "https://example.com/"
        )));
        assert!(!messages.iter().any(|m| matches!(
            m,
            Message::Chrome(Command::PdfClose | Command::Input(InputCommand::Start { .. }))
        )));
    }
    #[test]
    fn browser_tests_surface_input_consumes_zoom_and_autoscroll_before_children() {
        let mut view = ui(false, false);
        view.point_at((200., 200.));
        view.scroll(iced::mouse::ScrollDelta::Lines { x: 0., y: 1. });
        assert!(
            view.drain()
                .any(|m| matches!(m, Message::Chrome(Command::PdfClose)))
        );
        view.simulate([Event::Keyboard(iced::keyboard::Event::ModifiersChanged(
            iced::keyboard::Modifiers::CTRL,
        ))]);
        view.scroll(iced::mouse::ScrollDelta::Lines { x: 0., y: 1. });
        let messages: Vec<_> = view.drain().collect();
        assert!(
            messages
                .iter()
                .any(|m| matches!(m, Message::Chrome(Command::Zoom(10))))
        );
        assert!(
            !messages
                .iter()
                .any(|m| matches!(m, Message::Chrome(Command::PdfClose)))
        );
        view.simulate([Event::Mouse(iced::mouse::Event::ButtonPressed(
            iced::mouse::Button::Middle,
        ))]);
        assert!(
            view.drain()
                .any(|m| matches!(m, Message::Chrome(Command::PdfClose)))
        );
        let mut view = ui(true, false);
        view.point_at((200., 200.));
        view.simulate([
            Event::Mouse(iced::mouse::Event::ButtonPressed(
                iced::mouse::Button::Middle,
            )),
            Event::Mouse(iced::mouse::Event::ButtonReleased(
                iced::mouse::Button::Middle,
            )),
        ]);
        let messages: Vec<_> = view.drain().collect();
        assert!(messages.iter().any(|m| matches!(
            m,
            Message::Chrome(Command::Input(InputCommand::Start { .. }))
        )));
        assert!(
            messages
                .iter()
                .any(|m| matches!(m, Message::Chrome(Command::Input(InputCommand::Release))))
        );
        assert!(
            !messages
                .iter()
                .any(|m| matches!(m, Message::Chrome(Command::PdfClose)))
        );
        let mut view = ui(true, true);
        view.point_at((200., 200.));
        view.simulate([Event::Mouse(iced::mouse::Event::ButtonPressed(
            iced::mouse::Button::Left,
        ))]);
        assert!(
            view.drain()
                .any(|m| matches!(m, Message::Chrome(Command::Input(InputCommand::Cancel))))
        );
    }
}
