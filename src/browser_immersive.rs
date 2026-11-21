//! Immersive view: the page across the window, the sidebar over it on demand.
//!
//! The page keeps only a thin frame. Everything the top bar holds moves into
//! the sidebar: above the tabs the navigation buttons and the window
//! controls (a row the window is dragged by), the address field, and the
//! site's tools with a new-tab button; at its foot the library buttons, downloads and the menu. The
//! sidebar waits outside the
//! window and slides in over the page, on glass of the chosen style tinted
//! with the colour scheme, when the pointer rests at its edge for a moment,
//! so a pointer passing by does not call it. It stays while the pointer is
//! over it and goes when the pointer moves over the page or leaves the
//! window. It also comes in for the address field (Ctrl+L) at once, and
//! stays while something opened from it is open: the address suggestions,
//! the menu, a panel or popup, or a context menu.

use super::*;

use std::time::{Duration, Instant};

/// The frame around the page.
pub(crate) const EDGE: f32 = 6.0;
/// How much of the address field's fill covers the glass behind it, on
/// every style but the solid one.
pub(crate) const FIELD_ALPHA: f32 = 0.55;
/// How far in from the window's side the pointer calls the sidebar.
const REACH: f32 = 36.0;
/// How long the pointer rests at the edge before the sidebar comes.
const DWELL: Duration = Duration::from_millis(175);
/// The sidebar sliding in under a resting pointer reports the pointer
/// leaving it; for this long after it was called it is not sent away.
const ARRIVAL: Duration = Duration::from_millis(350);

/// The sidebar's state.
#[derive(Debug, Default)]
pub(crate) struct ImmersiveState {
    /// Whether the sidebar is out.
    pub(crate) shown: bool,
    /// When it was called in.
    called: Option<Instant>,
    /// When the pointer reached the edge, while it stays there.
    waiting: Option<Instant>,
    /// Whether the pointer is over it.
    over: bool,
    /// The compositor is moving or resizing the window by the sidebar's
    /// drag row. It takes the pointer, which leaves the window as far as
    /// the browser knows; until the pointer is back the sidebar stays.
    moving: bool,
}

impl ImmersiveState {
    /// Calls the sidebar in at once.
    pub(crate) fn call(&mut self) {
        if !self.shown {
            self.called = Some(Instant::now());
        }
        self.shown = true;
        self.waiting = None;
    }
    /// Keeps the sidebar out through a move of the window it started.
    pub(crate) fn hold_for_move(&mut self) {
        self.moving = true;
    }
    fn arriving(&self) -> bool {
        self.called.is_some_and(|called| called.elapsed() < ARRIVAL)
    }
    fn send_away(&mut self) {
        self.shown = false;
        self.over = false;
        self.waiting = None;
        self.moving = false;
    }
}

/// What the sidebar is made of.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ImmersiveStyle {
    /// Clear glass with a bending rim.
    Glass,
    /// Frosted and darker, for a bar that carries text.
    #[default]
    Frosted,
    /// A light frost.
    Subtle,
    /// A flat blur, without a rim.
    Blurred,
    /// The theme's opaque panel.
    Solid,
}

impl ImmersiveStyle {
    pub(crate) const ALL: [Self; 5] =
        [Self::Glass, Self::Frosted, Self::Subtle, Self::Blurred, Self::Solid];
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Glass => "Glass",
            Self::Frosted => "Frost",
            Self::Subtle => "Mist",
            Self::Blurred => "Blur",
            Self::Solid => "Solid",
        }
    }
    fn material(self) -> enzui_theme::MaterialStyle {
        use enzui_theme::MaterialStyle as M;
        match self {
            Self::Glass => M::Regular,
            Self::Frosted => M::Frosted,
            Self::Subtle => M::Subtle,
            Self::Blurred => M::FlatFrosted,
            Self::Solid => M::Solid,
        }
    }
    /// The material's optics. Clear glass bends the page only in a band
    /// along its rim and shows it unbent across a face as wide as the
    /// sidebar's; here the bevel spans the face, so it bends all the way
    /// across.
    fn tokens(self) -> enzui_theme::MaterialTokens {
        let tokens = self.material().tokens();
        match self {
            // A bevel past half the sidebar's width is capped there, so its
            // whole face is one curved slab.
            Self::Glass => enzui_theme::MaterialTokens {
                bevel_width: 400.0,
                refraction: 1.1,
                ..tokens
            },
            _ => tokens,
        }
    }
    /// How strongly the frame's colour tints the glass: enough for the
    /// colour scheme to show on the clearer styles too.
    fn tint(self) -> f32 {
        match self {
            Self::Glass => 0.35,
            Self::Frosted => 0.4,
            Self::Subtle => 0.3,
            Self::Blurred => 0.45,
            Self::Solid => 1.0,
        }
    }
}

/// What the person and the pointer do to immersive view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImmersiveCommand {
    /// Turns immersive view on or off.
    Toggle,
    /// Makes the sidebar of another material.
    Style(ImmersiveStyle),
    /// The pointer reached (or left) the edge that calls the sidebar.
    Edge { inside: bool },
    /// The time to rest at the edge has passed.
    Dwell,
    /// The pointer is over the sidebar, or left it.
    Bar { over: bool },
    /// The pointer moved over the page.
    PageMoved,
    /// The pointer left the window.
    LeftWindow,
}

impl ImmersiveCommand {
    /// Whether this only follows the pointer, rather than being something
    /// the person asked for: it leaves open menus open.
    pub(crate) fn is_pointer_tracking(self) -> bool {
        !matches!(self, Self::Toggle | Self::Style(_))
    }
}

/// The top bar's controls, laid out in the sidebar instead.
pub(crate) struct Toolbar<'a> {
    pub(crate) navigation: Element<'a, Message>,
    pub(crate) address: Element<'a, Message>,
    /// The site's tools the address field holds outside immersive view.
    pub(crate) tools: Vec<Element<'a, Message>>,
    pub(crate) downloads: Element<'a, Message>,
    pub(crate) menu: Element<'a, Message>,
    pub(crate) controls: Element<'a, Message>,
}

impl App {
    pub(crate) fn immersive_command(&mut self, command: ImmersiveCommand) -> Task<Message> {
        // Something opened from the sidebar keeps it: moving the pointer onto
        // a popup crosses the page, which would otherwise send it away.
        let in_use = self.browser.panel != Panel::None
            || self.browser.address_suggestions_open
            || self.browser.menu_state.is_open()
            || self.browser.tabs_open
            || self.browser.context.is_open();
        let state = &mut self.browser.immersive;
        match command {
            ImmersiveCommand::Toggle => {
                self.browser.saved.immersive = !self.browser.saved.immersive;
                self.browser.immersive = ImmersiveState::default();
                self.browser.save();
            }
            ImmersiveCommand::Style(style) => {
                self.browser.saved.immersive_style = style;
                self.browser.save();
            }
            ImmersiveCommand::Edge { inside } => {
                if !inside {
                    state.waiting = None;
                } else if !state.shown && state.waiting.is_none() {
                    state.waiting = Some(Instant::now());
                    return Task::perform(tokio::time::sleep(DWELL), |()| {
                        c(Command::Immersive(ImmersiveCommand::Dwell))
                    });
                }
            }
            ImmersiveCommand::Dwell => {
                // Only a pointer that stayed at the edge the whole time.
                if state
                    .waiting
                    .is_some_and(|since| since.elapsed() + Duration::from_millis(30) >= DWELL)
                {
                    state.call();
                }
            }
            ImmersiveCommand::Bar { over } => {
                if over {
                    state.moving = false;
                }
                if over || !state.arriving() {
                    state.over = over;
                }
            }
            // The first movement after a move ends it; the pointer may be
            // anywhere then, so it does not send the sidebar away yet.
            ImmersiveCommand::PageMoved if state.moving => state.moving = false,
            ImmersiveCommand::PageMoved => {
                if state.shown
                    && !state.over
                    && state.waiting.is_none()
                    && !state.arriving()
                    && !in_use
                {
                    state.send_away();
                }
            }
            ImmersiveCommand::LeftWindow => {
                if !in_use && !state.moving {
                    state.send_away();
                }
            }
        }
        Task::none()
    }

    /// The glass's protected inset: every style keeps the frosted glass's,
    /// so the sidebar and what is in it stay put when the style changes; the
    /// flat blur alone would hold none.
    fn immersive_inset() -> f32 {
        enzui_widgets::surface::enz_surface::<Message>(iced::widget::Space::new())
            .material(ImmersiveStyle::Frosted.material())
            .material_protected_inset()
            .unwrap_or(0.0)
    }

    /// Where the sidebar's glass is across the window with its content
    /// `sidebar` wide, as its left edge and its width, for what hangs from it.
    pub(crate) fn immersive_sidebar_span(&self, sidebar: f32) -> (f32, f32) {
        let width = sidebar + 2.0 * Self::immersive_inset();
        if self.browser.saved.right {
            (self.browser.viewport.width - EDGE - width, width)
        } else {
            (EDGE, width)
        }
    }

    /// Whether this window is in immersive view.
    pub(crate) fn immersive(&self) -> bool {
        self.browser.saved.immersive && !self.popup_window
    }

    /// The window in immersive view: `frame` (the page) across it, and the
    /// sidebar, holding `toolbar` above the tabs, that slides over it.
    pub(super) fn immersive_view<'a>(
        &'a self,
        frame: Element<'a, Message>,
        toolbar: Toolbar<'a>,
        radius: f32,
    ) -> Element<'a, Message> {
        use enzui_widgets::motion::animated;
        use iced::widget::{Space, column, container, mouse_area, row, stack};
        let command = |command| c(Command::Immersive(command));
        let style = self.browser.saved.immersive_style;
        let shown = self.browser.immersive.shown;
        let right = self.browser.saved.right;
        let width = self.chrome_metrics().sidebar;

        // The pointer moving over the page sends the sidebar away. The page
        // is always wrapped, so showing the sidebar does not remount it, and
        // listens only while the sidebar is out.
        let base = mouse_area(
            container(frame)
                .padding(EDGE)
                .width(Length::Fill)
                .height(Length::Fill),
        );
        let base = if shown {
            base.on_move(move |_| command(ImmersiveCommand::PageMoved))
        } else {
            base
        };

        let Toolbar {
            navigation,
            address,
            tools,
            downloads,
            menu,
            controls,
        } = toolbar;
        // The window is dragged by the row of the navigation buttons and
        // the window controls, as by the top bar.
        let window_row: Element<'a, Message> =
            enzui_widgets::titlebar_drag_region::titlebar_drag_region(
                row![navigation, Space::new().width(Length::Fill), controls]
                    .spacing(4)
                    .align_y(iced::Alignment::Center),
            )
            .on_gesture(|command| c(Command::Window(command)))
            .into();
        // The page's own tools sit under its address; downloads and the
        // menu stay with the library buttons at the foot.
        let new_tab = enzui_widgets::toolbar_icon::enz_toolbar_icon("plus", c(Command::New))
            .aria_label("New tab")
            .tooltip("New tab")
            .into_element();
        let tools_row = tools
            .into_iter()
            .fold(row![], iced::widget::Row::push)
            .push(Space::new().width(Length::Fill))
            .push(new_tab)
            .spacing(2)
            .align_y(iced::Alignment::Center);
        let head = column![window_row, address, tools_row]
            .spacing(6)
            .padding([2, 4])
            .width(Length::Fill);
        let foot = column![
            iced::widget::rule::horizontal(1),
            row![
                self.tab_library_actions(false),
                Space::new().width(Length::Fill),
                downloads,
                menu
            ]
            .spacing(2)
            .align_y(iced::Alignment::Center),
        ]
        .spacing(4)
        .padding([0, 4])
        .width(Length::Fill);
        let content = column![
            head,
            container(self.tab_strip_for(Layout::Sidebar)).height(Length::Fill),
            foot
        ]
        .spacing(6)
        .width(width)
        .height(Length::Fill);

        let inset = Self::immersive_inset();
        // The glass is tinted with the frame's colour, or with a chosen
        // colour scheme's hue at the frame's lightness: the frame carries
        // only a trace of the scheme's hue, too little to show through glass.
        let palette = &self.look().palette;
        let tint = match self.scheme_accent() {
            Some(accent) if style != ImmersiveStyle::Solid => {
                crate::appearance::glass_tint(palette.background, accent, palette.is_dark())
            }
            _ => palette.background,
        };
        // The glass runs the window's height, so the tabs take what the head
        // and the foot leave and the foot stays at the bottom.
        let glass = enzui_widgets::surface::enz_surface(content)
            .height(Length::Fill)
            .padding(inset)
            // Inside the window's corners by the frame's width, like the page.
            .radius((radius - EDGE).max(0.0))
            .material(style.material())
            .material_tokens(style.tokens())
            .fill(tint)
            .material_tint_opacity(style.tint())
            .elevation(3)
            .clip(true)
            .into_element();
        let away = (width + 2.0 * inset + 2.0 * EDGE) * if right { 1.0 } else { -1.0 };
        // The sidebar shields the page under it: the page is a lower layer
        // of the stack and would take the pointer, presses and scrolls
        // that nothing in the sidebar takes. A layer reporting an
        // interaction lifts the cursor off the layers below, and presses
        // and scrolls on the glass itself are captured here.
        let side = animated(
            mouse_area(glass)
                .interaction(iced::mouse::Interaction::Idle)
                .on_press(command(ImmersiveCommand::Bar { over: true }))
                .on_scroll(move |_| command(ImmersiveCommand::Bar { over: true }))
                .on_enter(command(ImmersiveCommand::Bar { over: true }))
                .on_exit(command(ImmersiveCommand::Bar { over: false })),
        )
        .offset(iced::Vector::new(if shown { 0.0 } else { away }, 0.0))
        .opacity(if shown { 1.0 } else { 0.0 });
        let align = if right {
            iced::alignment::Horizontal::Right
        } else {
            iced::alignment::Horizontal::Left
        };
        let layers: Vec<Element<'a, Message>> = vec![
            base.into(),
            container(side)
                .padding(EDGE)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(align)
                .into(),
            container(
                mouse_area(Space::new().width(REACH).height(Length::Fill))
                    .on_enter(command(ImmersiveCommand::Edge { inside: true }))
                    .on_exit(command(ImmersiveCommand::Edge { inside: false })),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(align)
            .into(),
        ];
        let view = mouse_area(stack(layers)).on_exit(command(ImmersiveCommand::LeftWindow));
        container(view)
            .style(move |_| container::Style {
                text_color: Some(self.look().palette.text),
                ..Default::default()
            })
            .into()
    }
}
