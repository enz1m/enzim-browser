//! The Appearance popup: the browser's colours, frame and sizes, shown at
//! once and saved to the application's theme file.

use super::*;
use crate::appearance::{self, Appearance, Colours, Frame, Glass, Metrics, SCHEMES};
use enzui_core::theme_file::ThemeDocument;
use enzui_theme::EnzThemeMode;
use iced::widget::{Space, column, container, row, text};

#[derive(Clone, Debug)]
pub(crate) enum AppearanceCommand {
    Mode(usize),
    /// A scheme of `SCHEMES`, or the desktop's colours.
    Scheme(Option<usize>),
    Frame(usize),
    Glass(usize),
    AddressHeight(f32),
    AddressWidth(f32),
    /// The pointer came over the address bar, or left it.
    AddressHover(bool),
    Tint(f32),
    Edge(f32),
    Shine(f32),
    /// What the compositor offers behind this window's glass frame.
    Backdrop(crate::glass::Backdrop),
    /// The curve the compositor rounds this window along, or that it rounds none.
    Rounding(Option<crate::corners::Rounding>),
    Grain(f32),
    /// The material of the immersive bars, by its place in the list.
    ImmersiveStyle(usize),
    Radius(f32),
    Toolbar(f32),
    Sidebar(f32),
    Rail(f32),
    Tab(f32),
    Reset,
    /// The pause after the change that made revision `0` has passed: save it.
    Flush(u64),
    Saved(u64, Result<(), String>),
    /// Ends the preview of revision `0` if the theme never came to show it.
    Settle(u64),
}

#[derive(Default)]
pub(crate) struct State {
    values: Appearance,
    loaded: bool,
    /// The choices not yet seen in the theme the framework reads from the
    /// file, with the theme they give. `None` when the loaded theme shows
    /// what the file says.
    preview: Option<Preview>,
    /// Counts changes; `saved` is the last one written to the file.
    revision: u64,
    saved: u64,
    saving: bool,
    notice: String,
    /// The glass values in force: the popup's while it holds any, else the
    /// file's as last read.
    glass: appearance::GlassLook,
    /// The address bar's size in force, kept as the glass values are.
    address: appearance::AddressBar,
    /// Whether the pointer is over an address bar that draws its own body.
    address_hover: bool,
    /// What the compositor does behind this window's glass frame.
    backdrop: crate::glass::Backdrop,
    /// The curve the compositor rounds this window along; `None` where the
    /// window's corners are its own to round (see `crate::corners`).
    rounding: Option<crate::corners::Rounding>,
    /// The width of the tab strip when the popup opened. The popup hangs
    /// beside the strip as it was then, so that neither covers the other
    /// and a slider does not move under the pointer that drags it.
    strip: f32,
    #[cfg(test)]
    file: Option<std::path::PathBuf>,
}
struct Preview {
    overlay: ThemeDocument,
    theme: enzui_theme::EnzTheme,
    iced: iced::Theme,
}

fn ac(command: AppearanceCommand) -> Message {
    c(Command::Appearance(command))
}

/// The popup's width beside the tab strip.
const PANEL_WIDTH: f32 = 288.0;
/// The swatch that gives the colours back to the desktop: no hue of its own.
const DESKTOP_SWATCH: iced::Color = iced::Color::from_rgb8(0x8e, 0x8e, 0x93);

const NO_PATH: &str = "The browser's theme file location is unavailable.";

impl App {
    /// The theme the chrome is drawn with: the loaded one, under the popup's
    /// choices while their file is being written and read back.
    pub(crate) fn look(&self) -> &enzui_theme::EnzTheme {
        match &self.browser.appearance.preview {
            Some(preview) => &preview.theme,
            None => self.theme.theme(),
        }
    }
    pub(crate) fn look_iced(&self) -> iced::Theme {
        match &self.browser.appearance.preview {
            Some(preview) => preview.iced.clone(),
            None => self.theme.iced_theme(),
        }
    }
    /// Publishes the theme defaults the framework's widgets read outside a
    /// view: the look of this window, and never a glass window frame. A
    /// glass frame is the chrome's own surface, the compositor's material or
    /// painted (see `crate::glass`); the shell's own frame under it would be
    /// a second one, opaque wherever the compositor paints none. The
    /// framework publishes the loaded theme whenever it reads it, so this
    /// follows every such read.
    pub(crate) fn publish_look(&self) {
        let mut theme = self.look().clone();
        if matches!(theme.window_frame, enzui_theme::WindowFrame::Glass(_)) {
            theme.window_frame = enzui_theme::WindowFrame::Opaque;
        }
        // Every test thread shares the published defaults.
        if !cfg!(test) {
            let _ = enzui_theme::text_defaults::publish(&theme);
        }
    }
    /// The sizes and frame of the chrome.
    pub(crate) fn chrome_metrics(&self) -> Metrics {
        Metrics::of(self.look())
    }
    /// How a glass frame is painted, when the frame is glass.
    pub(crate) fn glass_paint(&self) -> Option<(Glass, appearance::GlassPaint)> {
        let Frame::Glass(level) = self.chrome_metrics().frame else {
            return None;
        };
        let look = self.browser.appearance.glass;
        let mut paint = look.over(level);
        // Where the compositor paints the material, the tint nobody chose
        // is the material's own, not the painted frame's.
        if self.backdrop().is_material() && look.tint.is_none() {
            paint.tint = self
                .frame_material(level)
                .tint_opacity
                .clamp(appearance::GLASS_TINT.0, appearance::GLASS_TINT.1);
        }
        Some((level, paint))
    }
    /// The framework's material for a glass window frame of `level`, as it
    /// is in this window's mode.
    pub(crate) fn frame_material(&self, level: Glass) -> enzui_theme::MaterialTokens {
        let tokens = enzui_theme::window_frame::frame_tokens(level.material());
        if self.look().palette.is_dark() {
            enzui_theme::window_frame::in_dark(tokens)
        } else {
            // The chrome keeps the theme's dark ink on a light frame, so
            // even the clear level carries enough of the frame's colour for
            // it to read over a dark desktop.
            enzui_theme::MaterialTokens {
                tint_opacity: tokens.tint_opacity.max(0.55),
                ..tokens
            }
        }
    }
    pub(crate) fn backdrop(&self) -> &crate::glass::Backdrop {
        &self.browser.appearance.backdrop
    }
    /// The radius of the curve this window's corners follow: the
    /// compositor's where it cuts them, the theme's otherwise.
    pub(crate) fn corner_radius(&self) -> f32 {
        match self.browser.appearance.rounding {
            Some(rounding) => rounding.radius,
            None => self.look().window_radius,
        }
    }
    /// The radius of what is drawn inside the window to sit with its
    /// corners: theirs, or the theme's where the compositor keeps the
    /// window square.
    pub(crate) fn window_radius(&self) -> f32 {
        match self.desktop_rounds() {
            Some(rounding) => rounding.radius,
            None => self.look().window_radius,
        }
    }
    /// The curve the compositor rounds this window along, where it rounds it.
    fn desktop_rounds(&self) -> Option<crate::corners::Rounding> {
        self.browser
            .appearance
            .rounding
            .filter(|rounding| rounding.radius >= 0.5)
    }
    /// The light on a painted glass frame, along this window's corners.
    pub(crate) fn frame_light(&self, paint: appearance::GlassPaint) -> crate::glass::Light {
        crate::glass::Light {
            radius: self.corner_radius(),
            power: self
                .browser
                .appearance
                .rounding
                .map_or(crate::glass::CIRCLE, |rounding| rounding.power),
            edge: paint.edge,
            shine: paint.shine,
            dark: self.look().palette.is_dark(),
        }
    }
    /// The radius the frame's own paint ends at. Where the compositor cuts
    /// the corner, the paint has to reach under all of its curve: none.
    pub(crate) fn frame_cut(&self) -> f32 {
        // A window that is the whole screen has no corners to round.
        if self.browser.fullscreen.on() {
            return 0.0;
        }
        match self.browser.appearance.rounding {
            Some(_) => 0.0,
            None => self.look().window_radius,
        }
    }
    /// The shape blur is asked for behind a glass frame.
    pub(crate) fn frame_shape(&self) -> crate::glass::Shape {
        let size = self.browser.viewport;
        match self.browser.appearance.rounding {
            Some(rounding) => crate::glass::Shape::under(size, rounding),
            None => crate::glass::Shape::of(size, self.look().window_radius),
        }
    }
    /// Asks the compositor whether it rounds this window. Asked again
    /// whenever the look is read again and when the popup opens: the answer
    /// is a setting of the desktop's, and nothing says when it changes.
    pub(crate) fn appearance_corners(&self) -> Task<Message> {
        // No compositor stands behind a test's window.
        if cfg!(test) {
            return Task::none();
        }
        crate::corners::ask().map(|rounding| ac(AppearanceCommand::Rounding(rounding)))
    }
    /// Asks the compositor, once for this window and when its frame first
    /// is glass, what it can do behind it.
    pub(crate) fn appearance_backdrop(&mut self) -> Task<Message> {
        self.appearance_blur();
        let Some(window) = self.window_id else {
            return Task::none();
        };
        let state = &mut self.browser.appearance;
        if !matches!(state.backdrop, crate::glass::Backdrop::Unasked)
            || self.glass_paint().is_none()
            // No compositor stands behind a test's window.
            || cfg!(test)
        {
            return Task::none();
        }
        self.browser.appearance.backdrop = crate::glass::Backdrop::Asked;
        crate::glass::bind(window).map(|backdrop| ac(AppearanceCommand::Backdrop(backdrop)))
    }
    /// Keeps the blur the compositor was asked for under the frame as it is
    /// now: its size and corners while it is glass, none otherwise.
    fn appearance_blur(&self) {
        if let crate::glass::Backdrop::Blur(blur) = &self.browser.appearance.backdrop {
            blur.set(self.glass_paint().map(|_| self.frame_shape()));
        }
    }
    /// Reads the browser's own values (the glass, the address bar) from the
    /// file, for a window whose popup holds none.
    pub(crate) fn appearance_own_read(&mut self) {
        if let Some(Ok(document)) = self.appearance_file().map(|path| appearance::load(&path)) {
            let text = document.to_toml();
            self.browser.appearance.glass = appearance::GlassLook::read(&text);
            self.browser.appearance.address = appearance::AddressBar::read(&text);
        }
    }
    /// The address bar's size in force.
    pub(crate) fn address_bar(&self) -> appearance::AddressBar {
        self.browser.appearance.address
    }
    /// The height of the top bar as it is laid out: the chosen one, or its
    /// own, and in both cases what the address bar in it needs.
    pub(crate) fn top_bar_height(&self) -> f32 {
        let toolbar = self.chrome_metrics().toolbar;
        let address = self.address_bar().height();
        if toolbar == appearance::TOOLBAR.2 {
            // Its own height is its controls and their padding: it follows
            // an address bar taller or lower than the field it comes as.
            toolbar + address - appearance::address_field_height()
        } else {
            toolbar.max(address + 6.0)
        }
    }
    /// Whether the pointer is over the address bar.
    pub(crate) fn address_hovered(&self) -> bool {
        self.browser.appearance.address_hover
    }

    fn appearance_file(&self) -> Option<std::path::PathBuf> {
        #[cfg(test)]
        if let Some(file) = &self.browser.appearance.file {
            return Some(file.clone());
        }
        appearance::path()
    }
    fn appearance_preview(&mut self) {
        let state = &mut self.browser.appearance;
        let mut overlay = ThemeDocument::empty();
        let _ = state.values.write(&mut overlay);
        let mut theme = self.theme.theme().clone();
        let mode = theme.mode();
        overlay.apply(&mut theme, mode);
        let iced = theme.iced_theme();
        state.preview = Some(Preview {
            overlay,
            theme,
            iced,
        });
        self.publish_look();
    }
    fn appearance_settle(&mut self) {
        if self.browser.appearance.preview.take().is_some() {
            let _ = self.theme.set_mode_override(None);
        }
        self.publish_look();
    }

    fn appearance_load(&mut self) {
        if self.browser.appearance.loaded || self.browser.appearance.preview.is_some() {
            return;
        }
        let file = self.appearance_file();
        let state = &mut self.browser.appearance;
        match file.map(|path| appearance::load(&path)) {
            Some(Ok(document)) => {
                state.values = Appearance::read(&document);
                state.glass = state.values.glass;
                state.address = state.values.address;
                state.loaded = true;
                state.notice.clear();
            }
            Some(Err(error)) => state.notice = error,
            None => state.notice = NO_PATH.into(),
        }
    }
    /// The popup is about to be shown.
    pub(crate) fn appearance_open(&mut self) {
        let metrics = self.chrome_metrics();
        let strip = match self.effective_tab_layout() {
            Some(Layout::Icons) if !self.immersive() => metrics.rail,
            _ => metrics.sidebar,
        };
        let state = &mut self.browser.appearance;
        state.strip = strip;
        state.loaded &= state.preview.is_some();
        self.appearance_load();
    }

    /// The framework read the theme again. A preview is rebuilt over it, and
    /// let go once the saved file is what was read.
    pub(crate) fn appearance_theme_changed(&mut self) {
        let state = &mut self.browser.appearance;
        let Some(preview) = &state.preview else {
            // Another window, or a hand edit, changed the file.
            state.loaded = false;
            if self.browser.panel == Panel::Theme {
                self.appearance_load();
            } else {
                self.appearance_own_read();
            }
            self.publish_look();
            return;
        };
        if !state.saving && state.saved == state.revision {
            let loaded = self.theme.theme();
            let mut shown = loaded.clone();
            let mode = shown.mode();
            preview.overlay.apply(&mut shown, mode);
            // The theme has no equality of its own; its debug form names
            // every token.
            if format!("{shown:?}") == format!("{loaded:?}") {
                self.appearance_settle();
                return;
            }
        }
        self.appearance_preview();
    }

    pub(crate) fn appearance_command(&mut self, command: AppearanceCommand) -> Task<Message> {
        use AppearanceCommand::*;
        // The pointer passing over the bar is no choice: nothing to load or save.
        if let AddressHover(over) = command {
            self.browser.appearance.address_hover = over;
            return Task::none();
        }
        // A browser setting, kept with the others rather than in the theme.
        if let ImmersiveStyle(index) = command {
            let style = crate::browser::immersive::ImmersiveStyle::ALL
                .get(index)
                .copied()
                .unwrap_or_default();
            return self.immersive_command(crate::browser::immersive::ImmersiveCommand::Style(style));
        }
        self.appearance_load();
        let file = self.appearance_file();
        let state = &mut self.browser.appearance;
        match command {
            Flush(revision) => {
                if revision != state.revision || state.saving {
                    return Task::none();
                }
                let Some(path) = file else {
                    state.notice = NO_PATH.into();
                    return Task::none();
                };
                state.saving = true;
                let values = state.values.clone();
                return super::import_page::background(
                    "appearance",
                    move || appearance::save(&path, &values),
                    move |result| ac(Saved(revision, result)),
                );
            }
            Saved(revision, result) => {
                state.saving = false;
                match result {
                    Ok(()) => {
                        state.saved = revision;
                        state.notice.clear();
                    }
                    Err(error) => state.notice = error,
                }
                if state.revision != revision {
                    return Task::done(ac(Flush(state.revision)));
                }
                // The framework reads the file back within its debounce. Should
                // that never show these values, the preview still ends.
                return Task::perform(
                    async move {
                        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                    },
                    move |()| ac(Settle(revision)),
                );
            }
            Settle(revision) => {
                if revision == state.revision && !state.saving {
                    self.appearance_settle();
                }
                return Task::none();
            }
            Mode(index) => {
                state.values.mode = [None, Some(EnzThemeMode::Light), Some(EnzThemeMode::Dark)]
                    .get(index)
                    .copied()
                    .flatten();
                let _ = self.theme.set_mode_override(state.values.mode);
            }
            Scheme(scheme) => {
                state.values.colours = match scheme {
                    Some(index) if index < SCHEMES.len() => Colours::Scheme(index),
                    _ => Colours::Desktop,
                };
            }
            Frame(index) => {
                state.values.frame = match index {
                    1 => appearance::Frame::Grain,
                    2 => match state.values.frame {
                        glass @ appearance::Frame::Glass(_) => glass,
                        _ => appearance::Frame::Glass(appearance::Glass::Regular),
                    },
                    _ => appearance::Frame::Solid,
                };
            }
            Glass(index) => {
                state.values.frame = appearance::Frame::Glass(
                    *appearance::Glass::ALL
                        .get(index)
                        .unwrap_or(&appearance::Glass::Regular),
                );
                // A level is a set of values: choosing one takes all of it.
                state.values.glass = appearance::GlassLook::default();
            }
            AddressHeight(height) => {
                state.values.address = state.values.address.with_height(height);
            }
            AddressWidth(width) => state.values.address = state.values.address.with_width(width),
            AddressHover(_) | ImmersiveStyle(_) => return Task::none(),
            Tint(tint) => state.values.glass.tint = Some((tint * 100.0).round() / 100.0),
            Edge(edge) => state.values.glass.edge = Some((edge * 100.0).round() / 100.0),
            Shine(shine) => state.values.glass.shine = Some((shine * 100.0).round() / 100.0),
            Backdrop(backdrop) => {
                state.backdrop = backdrop;
                self.appearance_blur();
                return Task::none();
            }
            Rounding(rounding) => {
                state.rounding = rounding;
                self.appearance_blur();
                return Task::none();
            }
            Grain(strength) => state.values.grain = strength,
            Radius(radius) => state.values.radius = Some(radius.round()),
            Toolbar(height) => state.values.toolbar = Some(height.round()),
            Sidebar(width) => state.values.sidebar = Some(width.round()),
            Rail(width) => state.values.rail = Some(width.round()),
            Tab(height) => state.values.tab = Some(height.round()),
            Reset => {
                state.values = Appearance::default();
                let _ = self.theme.set_mode_override(None);
            }
        }
        self.browser.appearance.glass = self.browser.appearance.values.glass;
        self.browser.appearance.address = self.browser.appearance.values.address;
        self.appearance_preview();
        let backdrop = self.appearance_backdrop();
        let state = &mut self.browser.appearance;
        state.revision = state.revision.wrapping_add(1);
        let revision = state.revision;
        // Saved once the control has been still for a moment: a slider
        // dragged across its range is one write, not one per pixel.
        Task::batch([
            backdrop,
            Task::perform(
                async move {
                    tokio::time::sleep(std::time::Duration::from_millis(160)).await;
                },
                move |()| ac(Flush(revision)),
            ),
        ])
    }

    /// The button that opens the popup, for the sidebar's row or rail.
    pub(crate) fn appearance_button(&self) -> Element<'_, Message> {
        let open = self.browser.panel == Panel::Theme;
        enzui_widgets::popover::enz_popover(
            enzui_widgets::toolbar_icon::enz_toolbar_icon(
                "palette",
                c(Command::Panel(Panel::Theme)),
            )
            .aria_label("Appearance")
            .tooltip("Appearance")
            .size(28., 16.)
            .active(open)
            .into_element(),
            open.then(|| self.appearance_panel()),
            || c(Command::Dismiss),
        )
        .side(if self.browser.saved.right {
            enzui_core::placement::Side::Left
        } else {
            enzui_core::placement::Side::Right
        })
        // At the foot of immersive view's sidebar it rises from its button.
        .alignment(if self.immersive() {
            enzui_core::placement::Alignment::End
        } else {
            enzui_core::placement::Alignment::Start
        })
        .anchor(self.appearance_anchor())
        .width(PANEL_WIDTH)
        .padding(0.0)
        .label("Appearance")
        .scope(iced::advanced::widget::Id::new("browser-popover-Theme"))
        .into_element()
    }
    /// The tab strip's place in the window as it was when the popup opened:
    /// its header row in the sidebar, the foot of the icon rail.
    fn appearance_anchor(&self) -> iced::Rectangle {
        // The chrome's own margins: 10 around the content, 4 inside the strip.
        const MARGIN: f32 = 10.0;
        const BUTTON: f32 = 28.0;
        let viewport = self.browser.viewport;
        // In immersive view its button is at the foot of the sidebar, as
        // wide as when the popup opened: the popup does not move while its
        // own slider changes the sidebar's width under the pointer.
        if self.immersive() {
            let (x, width) = self.immersive_sidebar_span(self.browser.appearance.strip);
            return iced::Rectangle {
                x,
                y: viewport.height - MARGIN - 4.0 - BUTTON,
                width,
                height: BUTTON,
            };
        }
        let strip = self.browser.appearance.strip;
        let x = if self.browser.saved.right {
            viewport.width - MARGIN - strip
        } else {
            MARGIN
        };
        let y = if self.effective_tab_layout() == Some(Layout::Icons) {
            viewport.height - MARGIN - 4.0 - BUTTON
        } else {
            self.top_bar_height() + 4.0
        };
        iced::Rectangle {
            x,
            y,
            width: strip,
            height: BUTTON,
        }
    }

    /// The chosen colour scheme's accent, `None` with the desktop's colours.
    pub(crate) fn scheme_accent(&self) -> Option<iced::Color> {
        match self.browser.appearance.values.colours {
            Colours::Scheme(_) => Some(self.look().palette.accent),
            Colours::Desktop | Colours::Custom => None,
        }
    }

    pub(crate) fn appearance_panel(&self) -> Element<'_, Message> {
        use enzui_widgets::inline_view_switcher::{
            InlineSwitcherPage as Page, enz_inline_view_switcher,
        };
        use enzui_widgets::view_switcher::{SwitcherPolicy, SwitcherTokens};
        let state = &self.browser.appearance;
        let values = &state.values;
        let muted = self.look().palette.text_muted;
        let caption = move |label: &'static str| text(label).size(11).color(muted);
        // A pill of choices: a rounded track with the chosen cell raised on it.
        let pills = SwitcherPolicy {
            label_weight: enzui_widgets::tab_pill::TabLabelWeight::AlwaysRegular,
            compact_fallback: false,
            width_floors: false,
            tokens: SwitcherTokens {
                strip_height: 28.0,
                tab_height: 24.0,
                strip_radius: 14.0,
                tab_radius: 12.0,
                padding: 2.0,
                tab_padding_x: 8.0,
                label_size: 12.0,
                tab_min_width: 0.0,
                auto_floor: 0.0,
                ..SwitcherTokens::PILL_STRIP
            },
            ..SwitcherPolicy::PILL_STRIP
        };
        let pill = move |pages: Vec<Page>,
                         selected: usize,
                         label: &'static str,
                         choose: fn(usize) -> AppearanceCommand| {
            enz_inline_view_switcher(pages, selected, move |index| ac(choose(index)))
                .policy(pills)
                .label_size(12.0)
                .tab_id_prefix(format!("appearance-{label}"))
                .aria_label(label)
                .stretch(true)
                .into_element()
        };
        // A white thumb on a white popup needs an edge of its own.
        let slider_tokens = enzui_widgets::slider::SliderTokens {
            thumb_border: self.look().palette.border_strong,
            thumb_border_width: Some(1.0),
            ..enzui_widgets::slider::SliderTokens::from_palette(&self.look().palette)
        };
        let slider = |label: &'static str,
                      value: f32,
                      (low, high): (f32, f32),
                      step: f32,
                      shown: String,
                      change: fn(f32) -> AppearanceCommand| {
            row![
                text(label).size(12).width(62),
                enzui_widgets::slider::enz_slider(low..=high, value, move |value| {
                    ac(change(value))
                })
                .step(step)
                .step_marks(false)
                .track_height(4.0)
                .thumb_width(16.0)
                .thumb_height(16.0)
                .tokens(slider_tokens)
                .aria_label(label)
                .into_element(),
                text(shown)
                    .size(11)
                    .color(muted)
                    .width(38)
                    .align_x(iced::alignment::Horizontal::Right),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center)
        };

        // Colours.
        let mode = pill(
            vec![
                Page::new("system", "Auto").with_icon("sun-moon"),
                Page::new("light", "Light").with_icon("sun"),
                Page::new("dark", "Dark").with_icon("moon"),
            ],
            match values.mode {
                None => 0,
                Some(EnzThemeMode::Light) => 1,
                Some(EnzThemeMode::Dark) => 2,
            },
            "Mode",
            AppearanceCommand::Mode,
        );
        // A circle for each scheme; the chosen one wears a ring of its colour.
        let shown_mode = self.look().mode();
        let (ink, focus_ring) = (self.look().palette.text, self.look().palette.focus_ring);
        let swatch = |name: &'static str, colour: iced::Color, scheme: Option<usize>| {
            let selected = match (scheme, values.colours) {
                (None, Colours::Desktop) => true,
                (Some(index), Colours::Scheme(chosen)) => index == chosen,
                _ => false,
            };
            container(
                enzui_widgets::color_swatch::enz_color_swatch(
                    colour,
                    ac(AppearanceCommand::Scheme(scheme)),
                )
                .aria_label(name)
                .selected(selected)
                // The ring around it says which is chosen; its own edge only
                // answers the pointer.
                .tokens(enzui_widgets::color_swatch::ColorSwatchTokens {
                    normal_border: colour,
                    selected_border: colour,
                    hover_border: ink,
                    focus_ring,
                    swatch_size: 18.0,
                    swatch_radius: 9.0,
                    normal_border_width: 1.0,
                    selected_border_width: 1.0,
                    disabled_alpha: 0.55,
                })
                .id_prefix(format!("appearance-scheme-{name}"))
                .into_element(),
            )
            .padding(4)
            .style(move |_| container::Style {
                border: iced::Border {
                    radius: 13.0.into(),
                    width: 2.0,
                    color: if selected {
                        colour
                    } else {
                        iced::Color::TRANSPARENT
                    },
                },
                ..Default::default()
            })
        };
        let mut swatches = row![swatch("The desktop's colours", DESKTOP_SWATCH, None)]
            .spacing(8)
            .align_y(iced::Alignment::Center);
        for (index, scheme) in SCHEMES.iter().enumerate() {
            swatches = swatches.push(swatch(scheme.name, scheme.accent(shown_mode), Some(index)));
        }
        let colours = column![mode, swatches].spacing(10);

        // The window frame.
        let frame = pill(
            vec![
                Page::new("solid", "Solid").with_icon("square"),
                Page::new("grain", "Grain").with_icon("sparkles"),
                Page::new("glass", "Glass").with_icon("blend"),
            ],
            match values.frame {
                Frame::Solid => 0,
                Frame::Grain => 1,
                Frame::Glass(_) => 2,
            },
            "Frame",
            AppearanceCommand::Frame,
        );
        let mut window = column![caption("Frame"), frame].spacing(6);
        match values.frame {
            Frame::Grain => {
                window = window.push(slider(
                    "Texture",
                    values.grain,
                    (appearance::GRAIN.0, appearance::GRAIN.1),
                    0.01,
                    format!("{:.0}%", values.grain * 100.0),
                    AppearanceCommand::Grain,
                ));
            }
            Frame::Glass(glass) => {
                let paint = self
                    .glass_paint()
                    .map_or_else(|| values.glass.over(glass), |(_, paint)| paint);
                let material = state.backdrop.is_material();
                let percent = |value: f32| format!("{:.0}%", value * 100.0);
                // What the frame can be here depends on the desktop.
                let desktop = match &state.backdrop {
                    crate::glass::Backdrop::Material(_) => {
                        "This desktop paints the glass and its light."
                    }
                    crate::glass::Backdrop::Blur(_) => {
                        "This desktop blurs what is behind; the light is painted here."
                    }
                    crate::glass::Backdrop::Plain => {
                        "This desktop does not blur behind windows: the frame is see-through."
                    }
                    crate::glass::Backdrop::Unasked | crate::glass::Backdrop::Asked => "",
                };
                window = window
                    .push(pill(
                        vec![
                            Page::new("clear", "Clear"),
                            Page::new("regular", "Regular"),
                            Page::new("frosted", "Frosted"),
                        ],
                        Glass::ALL.iter().position(|level| *level == glass).unwrap_or(1),
                        "Glass",
                        AppearanceCommand::Glass,
                    ))
                    .push(slider(
                        "Tint",
                        paint.tint,
                        appearance::GLASS_TINT,
                        0.01,
                        percent(paint.tint),
                        AppearanceCommand::Tint,
                    ))
                    .push(slider(
                        "Edge light",
                        paint.edge,
                        (0.0, 1.0),
                        0.01,
                        percent(paint.edge),
                        AppearanceCommand::Edge,
                    ));
                // The compositor's material has its own light across it.
                if !material {
                    window = window.push(slider(
                        "Shine",
                        paint.shine,
                        (0.0, 1.0),
                        0.01,
                        percent(paint.shine),
                        AppearanceCommand::Shine,
                    ));
                }
                if !desktop.is_empty() {
                    window = window.push(text(desktop).size(11).color(muted));
                }
            }
            Frame::Solid => {}
        }
        window = window.push(match self.desktop_rounds() {
            // The compositor rounds the window: there is nothing here to
            // choose.
            Some(rounding) => row![
                text("Corners").size(12).width(62),
                text(format!("Set by the desktop: {:.0} px", rounding.radius))
                    .size(11)
                    .color(muted),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
            None => {
                let radius = self.look().window_radius;
                slider(
                    "Corners",
                    radius,
                    appearance::RADIUS,
                    1.0,
                    format!("{radius:.0} px"),
                    AppearanceCommand::Radius,
                )
            }
        });

        // Sizes.
        let metrics = self.chrome_metrics();
        let address = self.address_bar();
        let sizes = column![
            caption("Sizes"),
            slider(
                "Top bar",
                metrics.toolbar,
                (appearance::TOOLBAR.0, appearance::TOOLBAR.1),
                1.0,
                format!("{:.0} px", metrics.toolbar),
                AppearanceCommand::Toolbar,
            ),
            slider(
                "URL bar",
                address.height(),
                {
                    let (low, high, _) = appearance::address_height();
                    (low, high)
                },
                1.0,
                format!("{:.0} px", address.height()),
                AppearanceCommand::AddressHeight,
            ),
            slider(
                "URL width",
                address.width(),
                (appearance::ADDRESS_WIDTH.0, appearance::ADDRESS_WIDTH.1),
                1.0,
                format!("{:.0}%", address.width()),
                AppearanceCommand::AddressWidth,
            ),
            slider(
                "Sidebar",
                metrics.sidebar,
                (appearance::SIDEBAR.0, appearance::SIDEBAR.1),
                2.0,
                format!("{:.0} px", metrics.sidebar),
                AppearanceCommand::Sidebar,
            ),
            slider(
                "Icon rail",
                metrics.rail,
                (appearance::RAIL.0, appearance::RAIL.1),
                1.0,
                format!("{:.0} px", metrics.rail),
                AppearanceCommand::Rail,
            ),
            slider(
                "Tabs",
                metrics.tab,
                (appearance::TAB.0, appearance::TAB.1),
                1.0,
                format!("{:.0} px", metrics.tab),
                AppearanceCommand::Tab,
            ),
        ]
        .spacing(6);

        let header = row![
            glyph("palette", 15.),
            text("Appearance").size(14),
            Space::new().width(Length::Fill),
            enzui_widgets::toolbar_icon::enz_toolbar_icon(
                "rotate-ccw",
                ac(AppearanceCommand::Reset)
            )
            .aria_label("Reset appearance")
            .tooltip("Reset to the desktop's theme")
            .size(24., 14.)
            .enabled(*values != Appearance::default())
            .into_element(),
        ]
        .spacing(7)
        .align_y(iced::Alignment::Center);
        let rule = || iced::widget::rule::horizontal(1);
        let styles = crate::browser::immersive::ImmersiveStyle::ALL;
        let immersive = column![
            caption("Immersive sidebar"),
            pill(
                styles.iter().map(|style| Page::new(style.label(), style.label())).collect(),
                styles
                    .iter()
                    .position(|style| *style == self.browser.saved.immersive_style)
                    .unwrap_or(1),
                "Immersive sidebar",
                AppearanceCommand::ImmersiveStyle,
            ),
        ]
        .spacing(6);
        // The style of immersive view's sidebar is offered only in it.
        let mut body = column![header, colours, rule(), window, rule()].spacing(10);
        if self.browser.saved.immersive {
            body = body.push(immersive).push(rule());
        }
        let mut body = body.push(sizes);
        if !state.notice.is_empty() {
            body = body.push(text(state.notice.clone()).size(11).color(muted));
        }
        enzui_widgets::scroll_view::enz_scrollable(container(body).padding(12).width(Length::Fill))
            .height(Length::Shrink.max((self.browser.viewport.height - 96.).clamp(160., 780.)))
            .into()
    }

    /// The popup as a dialog, where the sidebar that holds its button is not shown.
    pub(crate) fn appearance_dialog(&self) -> Element<'_, Message> {
        let radius = self.window_radius().min(18.0);
        container(self.appearance_panel())
            .width((self.browser.viewport.width - 24.).clamp(240., PANEL_WIDTH + 12.))
            .style(move |theme| {
                let p = palette(theme);
                container::Style {
                    background: Some(p.surface_elevated.into()),
                    border: iced::Border {
                        radius: radius.max(10.0).into(),
                        color: p.border,
                        width: 1.0,
                    },
                    ..Default::default()
                }
            })
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::AppearanceCommand::*;
    use super::*;

    fn send(app: &mut App, command: AppearanceCommand) {
        let _ = app.browser_command(Command::Appearance(command));
    }

    #[test]
    fn a_window_the_compositor_rounds_is_painted_square_under_its_curve() {
        use crate::corners::Rounding as Curve;
        use crate::glass::{Corner, Shape};
        let mut app = crate::browser::browser_tests::app();
        app.browser.viewport = iced::Size::new(900.0, 600.0);
        let own = app.look().window_radius;
        assert!(own >= 4.0, "the theme rounds its windows");
        let paint = appearance::GlassLook::default().over(appearance::Glass::Clear);

        // Left to itself the window rounds its own frame, paint and blur alike.
        let itself = |app: &App| {
            assert_eq!(
                (app.corner_radius(), app.window_radius(), app.frame_cut()),
                (own, own, own)
            );
            assert_eq!(app.frame_shape(), Shape::of(app.browser.viewport, own));
            assert_eq!(app.frame_light(paint).radius, own);
        };
        itself(&app);

        // A compositor that rounds windows cuts this one: the paint reaches
        // the square corners, and what follows the curve follows the
        // compositor's.
        let curve = Curve {
            radius: 10.0,
            power: 4.0,
        };
        send(&mut app, Rounding(Some(curve)));
        assert_eq!(
            (app.corner_radius(), app.window_radius(), app.frame_cut()),
            (10.0, 10.0, 0.0)
        );
        assert_eq!(
            app.frame_shape(),
            Shape {
                width: 900,
                height: 600,
                radius: 10,
                corner: Corner::Cut(400),
            }
        );
        let light = app.frame_light(paint);
        assert_eq!((light.radius, light.power), (10.0, 4.0));

        // One that keeps windows square has this one square too; only what
        // is drawn inside it keeps the theme's radius.
        send(
            &mut app,
            Rounding(Some(Curve {
                radius: 0.0,
                power: 2.0,
            })),
        );
        assert_eq!(
            (app.corner_radius(), app.window_radius(), app.frame_cut()),
            (0.0, own, 0.0)
        );
        assert_eq!(app.frame_shape().rectangles(), vec![(0, 0, 900, 600)]);

        send(&mut app, Rounding(None));
        itself(&app);
    }

    #[test]
    fn the_address_bars_size_shows_at_once_and_the_top_bar_holds_it() {
        let scratch = crate::credentials::tests::Scratch::new();
        let file = scratch.path().with_file_name("org.enzim.Browser.toml");
        let mut app = crate::browser::browser_tests::app();
        app.browser.appearance.file = Some(file.clone());
        let (_, _, field) = appearance::address_height();
        assert_eq!(
            (app.address_bar().height(), app.address_bar().width()),
            (field, 95.0)
        );
        // The top bar's own height was made for the framework's field; it
        // is lower by as much as the bar is.
        let own = app.top_bar_height();
        assert_eq!(
            own,
            appearance::TOOLBAR.2 + field - appearance::address_field_height()
        );

        send(&mut app, AddressHeight(field + 12.3));
        send(&mut app, AddressWidth(70.0));
        assert_eq!(
            (app.address_bar().height(), app.address_bar().width()),
            (field + 12.0, 70.0)
        );
        // The top bar's own height follows the bar in it; a chosen height
        // too low for the bar gives way to it.
        assert_eq!(app.top_bar_height(), own + 12.0);
        send(&mut app, Toolbar(50.0));
        assert_eq!(app.chrome_metrics().toolbar, 50.0);
        assert_eq!(app.top_bar_height(), field + 12.0 + 6.0);
        send(&mut app, Toolbar(80.0));
        assert_eq!(app.top_bar_height(), 80.0);

        // The pointer over the bar is no choice: nothing to save.
        let revision = app.browser.appearance.revision;
        send(&mut app, AddressHover(true));
        assert!(app.address_hovered());
        assert_eq!(app.browser.appearance.revision, revision);

        // A window whose popup was never opened reads the size from the file.
        appearance::save(&file, &app.browser.appearance.values).expect("save");
        let mut other = crate::browser::browser_tests::app();
        other.browser.appearance.file = Some(file);
        assert_eq!(other.address_bar(), appearance::AddressBar::default());
        other.appearance_own_read();
        assert_eq!(other.address_bar(), app.address_bar());

        // The bar's own size is no choice.
        send(&mut app, AddressHeight(field));
        send(&mut app, AddressWidth(appearance::ADDRESS_WIDTH.2));
        assert_eq!(app.address_bar(), appearance::AddressBar::default());
    }

    #[test]
    fn a_choice_in_the_popup_shows_at_once_and_stays_once_the_file_is_read_back() {
        let scratch = crate::credentials::tests::Scratch::new();
        let file = scratch.path().with_file_name("org.enzim.Browser.toml");
        let mut app = crate::browser::browser_tests::app();
        app.browser.appearance.file = Some(file.clone());
        let own = app.chrome_metrics();
        let _ = app.browser_command(Command::Panel(Panel::Theme));
        assert_eq!(app.browser.panel, Panel::Theme);

        send(&mut app, Sidebar(301.4));
        send(&mut app, Scheme(Some(2)));
        send(&mut app, Frame(2));
        // Shown before anything is written.
        assert!(!file.exists());
        let shown = app.chrome_metrics();
        assert_eq!(shown.sidebar, 301.0);
        assert_eq!(
            shown.frame,
            appearance::Frame::Glass(appearance::Glass::Regular)
        );
        assert_eq!(
            (shown.toolbar, shown.rail, shown.tab),
            (own.toolbar, own.rail, own.tab)
        );
        let accent = SCHEMES[2].accent(app.look().mode());
        assert_eq!(app.look().palette.accent, accent);
        assert_ne!(
            app.look().palette.background,
            app.theme.theme().palette.background,
            "the frame is tinted with the accent"
        );
        assert_eq!(
            enzui_theme::EnzTheme::palette_from_iced(&app.theme()).map(|palette| palette.accent),
            Some(accent)
        );

        // The glass values show at once, and choosing a level takes that
        // level's own values again.
        send(&mut app, Tint(0.314));
        send(&mut app, Edge(0.0));
        assert_eq!(
            app.glass_paint().map(|(_, paint)| (paint.tint, paint.edge)),
            Some((0.31, 0.0))
        );
        send(&mut app, Glass(0));
        assert_eq!(
            app.glass_paint(),
            Some((
                appearance::Glass::Clear,
                appearance::GlassLook::default().over(appearance::Glass::Clear)
            ))
        );
        send(&mut app, Glass(1));
        send(&mut app, Shine(0.9));

        // Only the pause after the last change saves.
        let revision = app.browser.appearance.revision;
        send(&mut app, Flush(revision - 1));
        assert!(!app.browser.appearance.saving);
        send(&mut app, Flush(revision));
        assert!(app.browser.appearance.saving);
        appearance::save(&file, &app.browser.appearance.values).expect("save");

        // The theme read while the save is under way does not end the preview.
        app.appearance_theme_changed();
        assert!(app.browser.appearance.preview.is_some());
        send(&mut app, Saved(revision, Ok(())));
        assert!(app.browser.appearance.preview.is_some());
        assert!(
            std::fs::read_to_string(&file)
                .expect("saved file")
                .contains("[browser.glass]\nshine = 0.9\n"),
            "the glass values are kept in the file's own table"
        );
        // The framework reads the file back: the loaded theme now says the same.
        let loaded = appearance::load(&file)
            .expect("saved file")
            .resolve(enzui_theme::EnzThemeMode::Dark);
        app.theme =
            enzui_core::theme_file::integration::ThemeIntegration::unmanaged_compiled_theme(loaded)
                .0;
        app.appearance_theme_changed();
        assert!(app.browser.appearance.preview.is_none());
        assert_eq!(app.chrome_metrics(), shown);
        assert_eq!(app.look().palette.accent, accent);

        // A save that fails is said, and the next change tries again.
        send(&mut app, Rail(60.0));
        let revision = app.browser.appearance.revision;
        send(&mut app, Flush(revision));
        send(
            &mut app,
            Saved(
                revision,
                Err("The browser's theme file could not be saved.".into()),
            ),
        );
        assert!(!app.browser.appearance.notice.is_empty());
        assert_eq!(app.browser.appearance.saved, revision - 1);
        assert_eq!(app.chrome_metrics().rail, 60.0);

        // Reset gives every choice back to the desktop's theme.
        send(&mut app, Reset);
        assert_eq!(app.browser.appearance.values, Appearance::default());
        appearance::save(&file, &app.browser.appearance.values).expect("reset");
        assert_eq!(
            Appearance::read(&appearance::load(&file).expect("reset file")),
            Appearance::default()
        );
    }
}
