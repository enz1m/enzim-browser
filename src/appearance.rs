//! The browser's own look: what the Appearance popup edits, kept in the
//! application's theme file (`<config>/enzui/org.enzim.Browser.toml`).
//!
//! Every setting is a token of the shared theme schema, so the file stays one
//! any enzUI theme editor reads, and the tokens it does not set keep coming
//! from the desktop's theme. The browser's chrome has sizes of its own; a
//! sizing token changes one only when a theme file sets it.

use enzui_core::theme_file::registry::{self, TokenValue};
use enzui_core::theme_file::{ModePreference, ThemeDocument};
use enzui_theme::{EnzTheme, EnzThemeMode, MaterialStyle, WindowFrame};
use iced::Color;
use std::path::Path;

/// What the window frame is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Frame {
    /// The theme's background colour.
    Solid,
    /// The background colour under the theme's grain.
    Grain,
    /// A see-through fill the compositor blurs what is behind.
    Glass(Glass),
}
/// How much of what is behind a glass frame shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Glass {
    Clear,
    Regular,
    Frosted,
}
impl Glass {
    pub(crate) const ALL: [Self; 3] = [Self::Clear, Self::Regular, Self::Frosted];
    fn token(self) -> &'static str {
        match self {
            Self::Clear => "clear",
            Self::Regular => "glass",
            Self::Frosted => "frosted",
        }
    }
    /// How opaque the frame's own colour is over what the compositor shows.
    pub(crate) fn tint(self) -> f32 {
        match self {
            Self::Clear => 0.5,
            Self::Regular => 0.68,
            Self::Frosted => 0.84,
        }
    }
    /// How bright the light along the frame's edge is, 0 to 1: clearer glass
    /// has the sharper edge.
    pub(crate) fn edge(self) -> f32 {
        match self {
            Self::Clear => 0.75,
            Self::Regular => 0.6,
            Self::Frosted => 0.45,
        }
    }
    /// How bright the sheen across the frame is, 0 to 1.
    pub(crate) fn shine(self) -> f32 {
        match self {
            Self::Clear => 0.55,
            Self::Regular => 0.45,
            Self::Frosted => 0.3,
        }
    }
    /// The framework's material of this level, where the compositor paints it.
    pub(crate) fn material(self) -> MaterialStyle {
        match self {
            Self::Clear => MaterialStyle::Clear,
            Self::Regular => MaterialStyle::Regular,
            Self::Frosted => MaterialStyle::Frosted,
        }
    }
    fn of(style: MaterialStyle) -> Self {
        match style {
            MaterialStyle::Clear | MaterialStyle::Lens => Self::Clear,
            MaterialStyle::Frosted | MaterialStyle::FlatFrosted | MaterialStyle::Solid => {
                Self::Frosted
            }
            _ => Self::Regular,
        }
    }
}

/// A colour theme of the popup: an accent for each mode, and the frame and
/// the surfaces of the chrome tinted a little towards the accent's hue.
pub(crate) struct Scheme {
    pub(crate) name: &'static str,
    light: Color,
    dark: Color,
}
pub(crate) const SCHEMES: [Scheme; 7] = [
    Scheme::new("Blue", (0x2a, 0x6f, 0xdb), (0x5b, 0x9c, 0xf5)),
    Scheme::new("Teal", (0x0d, 0x8a, 0x82), (0x2f, 0xb5, 0xaa)),
    Scheme::new("Green", (0x2b, 0x8a, 0x45), (0x4c, 0xb8, 0x6a)),
    Scheme::new("Amber", (0xb8, 0x72, 0x0a), (0xe3, 0xa0, 0x2c)),
    Scheme::new("Orange", (0xd2, 0x55, 0x1f), (0xee, 0x7d, 0x47)),
    Scheme::new("Rose", (0xcc, 0x37, 0x66), (0xec, 0x6a, 0x90)),
    Scheme::new("Violet", (0x6f, 0x4f, 0xd0), (0x95, 0x7a, 0xf0)),
];
/// How far a neutral moves from grey towards a scheme's hue, as OKLab
/// chroma: enough to tell two schemes apart by their frames, and little
/// enough that text and page content keep their contrast. In the light
/// mode it also shrinks towards white, which stays white.
const TINT: f32 = 0.016;
const LIGHT_TINT_PER_SHADE: f32 = 0.45;

impl Scheme {
    const fn new(name: &'static str, light: (u8, u8, u8), dark: (u8, u8, u8)) -> Self {
        Self {
            name,
            light: Color::from_rgb8(light.0, light.1, light.2),
            dark: Color::from_rgb8(dark.0, dark.1, dark.2),
        }
    }
    pub(crate) fn accent(&self, mode: EnzThemeMode) -> Color {
        match mode {
            EnzThemeMode::Light => self.light,
            EnzThemeMode::Dark => self.dark,
        }
    }
    /// The palette roles this scheme sets for `mode`, as the theme file
    /// keeps them: eight bits a channel.
    fn roles(&self, mode: EnzThemeMode) -> Vec<(&'static str, Color)> {
        let mut theme = match mode {
            EnzThemeMode::Light => EnzTheme::light(),
            EnzThemeMode::Dark => EnzTheme::dark(),
        };
        let accent = self.accent(mode);
        let [_, a, b] = oklab(accent);
        let hue = b.atan2(a);
        let tokens = |role: &str| registry::token(role).expect("a palette role of the schema");
        for role in TINTED_ROLES {
            let token = tokens(role);
            let TokenValue::Color(neutral) = token.read(&theme) else {
                continue;
            };
            let [lightness, ..] = oklab(neutral);
            let chroma = match mode {
                EnzThemeMode::Dark => TINT,
                EnzThemeMode::Light => TINT.min((1.0 - lightness) * LIGHT_TINT_PER_SHADE),
            };
            let tinted = from_oklab([lightness, chroma * hue.cos(), chroma * hue.sin()]);
            token.write(&mut theme, &TokenValue::Color(tinted));
        }
        // After the tint: the soft accent is mixed from the background.
        theme.palette = theme.palette.with_accent(accent);
        TINTED_ROLES
            .into_iter()
            .chain(ACCENT_ROLES)
            .filter_map(|role| match tokens(role).read(&theme) {
                TokenValue::Color(color) => Some((role, eight_bits(color))),
                _ => None,
            })
            .collect()
    }
}

/// Glass's tint for a colour scheme: `accent`'s hue with far more colour
/// than the frame carries, which glass, diluting its tint over the
/// backdrop, would lose. Light glass keeps `base`'s lightness, up to where
/// it stays light glass with dark ink. Dark glass rescales its colour to a
/// dark ceiling, so a tint as dark as `base` would add no hue over a light
/// page: its tint is mid-light, still below where glass turns light.
pub(crate) fn glass_tint(base: Color, accent: Color, dark: bool) -> Color {
    let [lightness, ..] = oklab(base);
    let [_, a, b] = oklab(accent);
    let hue = b.atan2(a);
    let (lightness, chroma) = if dark {
        (0.6, 0.07)
    } else {
        (lightness.min(0.9), 0.07)
    };
    from_oklab([lightness, chroma * hue.cos(), chroma * hue.sin()])
}

fn eight_bits(color: Color) -> Color {
    let [r, g, b, _] = color.into_rgba8();
    Color::from_rgb8(r, g, b)
}
/// `color` as OKLab lightness and its two opponent axes.
fn oklab(color: Color) -> [f32; 3] {
    let linear = |c: f32| {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let (r, g, b) = (linear(color.r), linear(color.g), linear(color.b));
    let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}
fn from_oklab([lightness, a, b]: [f32; 3]) -> Color {
    let l = (lightness + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let m = (lightness - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
    let s = (lightness - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
    let encoded = |c: f32| {
        let c = c.clamp(0.0, 1.0);
        if c <= 0.003_130_8 {
            12.92 * c
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        }
    };
    Color::from_rgb(
        encoded(4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s),
        encoded(-1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s),
        encoded(-0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s),
    )
}

/// Where the chrome's colours come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Colours {
    /// The desktop's theme.
    Desktop,
    /// `SCHEMES[0]`.
    Scheme(usize),
    /// Palette roles written into the file by hand, or by another build:
    /// shown as they are, and replaced only when a scheme is chosen.
    Custom,
}

/// What the popup tunes of a glass frame beside its level. These are the
/// browser's own, not tokens of the theme schema: the file keeps them in a
/// `[browser.glass]` table, which other readers of the file pass over.
/// `None` is the level's own value.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct GlassLook {
    pub(crate) tint: Option<f32>,
    pub(crate) edge: Option<f32>,
    pub(crate) shine: Option<f32>,
}
/// A glass frame as it is painted: the level with the popup's own values over it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GlassPaint {
    pub(crate) tint: f32,
    pub(crate) edge: f32,
    pub(crate) shine: f32,
}
pub(crate) const GLASS_TINT: (f32, f32) = (0.2, 0.95);
const GLASS_KEYS: [&str; 3] = ["tint", "edge", "shine"];

impl GlassLook {
    pub(crate) fn over(self, glass: Glass) -> GlassPaint {
        GlassPaint {
            tint: self
                .tint
                .unwrap_or_else(|| glass.tint())
                .clamp(GLASS_TINT.0, GLASS_TINT.1),
            edge: self.edge.unwrap_or_else(|| glass.edge()).clamp(0.0, 1.0),
            shine: self.shine.unwrap_or_else(|| glass.shine()).clamp(0.0, 1.0),
        }
    }
    fn values(self) -> [Option<f32>; 3] {
        [self.tint, self.edge, self.shine]
    }
    /// The values the theme file `text` holds; anything that is not a
    /// number from 0 to 1 is as good as absent.
    pub(crate) fn read(text: &str) -> Self {
        let Ok(file) = text.parse::<toml_edit::DocumentMut>() else {
            return Self::default();
        };
        let value = |key: &str| {
            let number = own_number(&file, "glass", key)?;
            (0.0..=1.0)
                .contains(&number)
                .then(|| (number as f32 * 100.0).round() / 100.0)
        };
        Self {
            tint: value("tint"),
            edge: value("edge"),
            shine: value("shine"),
        }
    }
    /// `text` with the values that differ from the ones it holds set or
    /// taken out; the table goes with its last value. (A `[browser]` header
    /// written by hand and left empty is `save`'s to take out.)
    fn written(self, text: &str) -> String {
        let held = Self::read(text);
        if held == self {
            return text.to_owned();
        }
        let Ok(mut file) = text.parse::<toml_edit::DocumentMut>() else {
            return text.to_owned();
        };
        for ((key, value), was) in GLASS_KEYS.into_iter().zip(self.values()).zip(held.values()) {
            if value != was {
                // Two decimals, as the popup shows them.
                let value =
                    value.map(|value| f64::from((value.clamp(0.0, 1.0) * 100.0).round()) / 100.0);
                set_own(&mut file, "glass", key, value.map(toml_edit::Value::from));
            }
        }
        file.to_string()
    }
}

/// A number the browser keeps for itself in the theme file, under
/// `[browser.<table>]`.
fn own_number(file: &toml_edit::DocumentMut, table: &str, key: &str) -> Option<f64> {
    let item = file.get("browser")?.get(table)?.get(key)?;
    item.as_float()
        .or_else(|| item.as_integer().map(|whole| whole as f64))
}
/// Sets `[browser.<table>] key`, or takes it out; the table goes with its
/// last value.
fn set_own(
    file: &mut toml_edit::DocumentMut,
    table: &str,
    key: &str,
    value: Option<toml_edit::Value>,
) {
    match value {
        Some(value) => {
            let browser = file.entry("browser").or_insert_with(|| {
                // Made here for the one table: `[browser.<table>]` alone,
                // with no bare `[browser]` above it.
                let mut table = toml_edit::Table::new();
                table.set_implicit(true);
                toml_edit::Item::Table(table)
            });
            if let Some(browser) = browser.as_table_like_mut() {
                let own = browser.entry(table).or_insert_with(toml_edit::table);
                if let Some(own) = own.as_table_like_mut() {
                    let _ = own.insert(key, toml_edit::Item::Value(value));
                }
            }
        }
        None => {
            let Some(browser) = file
                .get_mut("browser")
                .and_then(toml_edit::Item::as_table_like_mut)
            else {
                return;
            };
            let emptied = browser
                .get_mut(table)
                .and_then(toml_edit::Item::as_table_like_mut)
                .is_some_and(|own| own.remove(key).is_some() && own.is_empty());
            if emptied {
                let _ = browser.remove(table);
            }
        }
    }
}

/// The address bar's size in the top bar. The browser's own, like the glass
/// values: the file keeps it in a `[browser.address]` table. `None` is the
/// bar as it comes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct AddressBar {
    /// Its height in pixels.
    pub(crate) height: Option<f32>,
    /// How much of the room between the buttons on its two sides it takes,
    /// in percent; the rest is left free beside it, half on each side.
    pub(crate) width: Option<f32>,
}
/// The range of the bar's width, and what it is without a choice. At the
/// top of the range the bar takes all the room it has.
pub(crate) const ADDRESS_WIDTH: (f32, f32, f32) = (40.0, 100.0, 95.0);
/// The range of the bar's height at the baseline text size, and what it is
/// without a choice.
const ADDRESS_HEIGHT: (f32, f32, f32) = (30.0, 50.0, 36.0);
/// The height of the framework's large field at the baseline text size. A
/// bar of that height is the field as it comes; at any other the browser
/// draws the bar's body around a field that fits in it.
const ADDRESS_FIELD: f32 = 40.0;
/// The same in this desktop's pixels.
pub(crate) fn address_field_height() -> f32 {
    enzui_theme::text_scale::ui_px(ADDRESS_FIELD).round()
}
/// The same in this desktop's pixels: the field grows with the desktop's
/// text size, and the range with it.
pub(crate) fn address_height() -> (f32, f32, f32) {
    let ui = |px: f32| enzui_theme::text_scale::ui_px(px).round();
    (
        ui(ADDRESS_HEIGHT.0),
        ui(ADDRESS_HEIGHT.1),
        ui(ADDRESS_HEIGHT.2),
    )
}
fn address_keys() -> [(&'static str, (f32, f32, f32)); 2] {
    [("height", address_height()), ("width", ADDRESS_WIDTH)]
}

impl AddressBar {
    pub(crate) fn height(self) -> f32 {
        self.height.unwrap_or_else(|| address_height().2)
    }
    pub(crate) fn width(self) -> f32 {
        self.width.unwrap_or(ADDRESS_WIDTH.2)
    }
    /// A size as the popup's slider gives it: whole, within its range, and
    /// no choice at all where it is the bar's own.
    fn chosen(value: f32, (low, high, own): (f32, f32, f32)) -> Option<f32> {
        let value = value.clamp(low, high).round();
        (value != own).then_some(value)
    }
    pub(crate) fn with_height(self, height: f32) -> Self {
        Self {
            height: Self::chosen(height, address_height()),
            ..self
        }
    }
    pub(crate) fn with_width(self, width: f32) -> Self {
        Self {
            width: Self::chosen(width, ADDRESS_WIDTH),
            ..self
        }
    }
    fn values(self) -> [Option<f32>; 2] {
        [self.height, self.width]
    }
    /// The sizes the theme file `text` holds; a number outside its range is
    /// as good as absent.
    pub(crate) fn read(text: &str) -> Self {
        let Ok(file) = text.parse::<toml_edit::DocumentMut>() else {
            return Self::default();
        };
        let [height, width] = address_keys().map(|(key, (low, high, _))| {
            let number = own_number(&file, "address", key)? as f32;
            (low..=high).contains(&number).then(|| number.round())
        });
        Self { height, width }
    }
    /// `text` with the sizes that differ from the ones it holds set or
    /// taken out.
    fn written(self, text: &str) -> String {
        let held = Self::read(text);
        if held == self {
            return text.to_owned();
        }
        let Ok(mut file) = text.parse::<toml_edit::DocumentMut>() else {
            return text.to_owned();
        };
        for (((key, _), value), was) in address_keys().into_iter().zip(self.values()).zip(held.values())
        {
            if value != was {
                let value = value.map(|value| toml_edit::Value::from(value.round() as i64));
                set_own(&mut file, "address", key, value);
            }
        }
        file.to_string()
    }
}

/// The ranges the popup offers, and the browser's own sizes where no theme
/// file sets one.
pub(crate) const TOOLBAR: (f32, f32, f32) = (50.0, 84.0, 61.0);
pub(crate) const SIDEBAR: (f32, f32, f32) = (180.0, 360.0, 224.0);
pub(crate) const RAIL: (f32, f32, f32) = (44.0, 76.0, 48.0);
pub(crate) const TAB: (f32, f32, f32) = (26.0, 44.0, 32.0);
pub(crate) const RADIUS: (f32, f32) = (0.0, 32.0);
pub(crate) const GRAIN: (f32, f32, f32) = (0.02, 0.10, 0.05);

/// The schema's defaults for the tokens the chrome reads. A resolved theme
/// does not say whether a file set a token, so a value that differs from
/// these is what "a theme set it" means here, and `write` keeps its own
/// values off them.
const SCHEMA_SIZES: [f32; 4] = [48.0, 260.0, 200.0, 34.0];
const SCHEMA_GRAIN: f32 = 0.14;
/// Far enough from a schema default to be told from it, and too little to
/// be seen: sizes are rounded to whole pixels where they are used.
const OFF_DEFAULT: f32 = 0.05;
fn off_default(value: f32, default: f32, by: f32) -> f32 {
    if (value - default).abs() < by {
        default + by
    } else {
        value
    }
}

/// The neutrals a scheme tints: the frame, the surfaces on it and their edges.
const TINTED_ROLES: [&str; 15] = [
    "palette.background",
    "palette.surface",
    "palette.surface_alt",
    "palette.surface_elevated",
    "palette.surface_hover",
    "palette.surface_selected",
    "palette.selected",
    "palette.border",
    "palette.border_strong",
    "palette.field",
    "palette.field_border",
    "palette.field_border_strong",
    "palette.button_primary_bg",
    "palette.button_primary_hover_bg",
    "palette.button_primary_pressed_bg",
];
const ACCENT_ROLES: [&str; 5] = [
    "palette.accent",
    "palette.accent_hover",
    "palette.accent_pressed",
    "palette.accent_soft",
    "palette.focus_ring",
];
const MODES: [EnzThemeMode; 2] = [EnzThemeMode::Light, EnzThemeMode::Dark];

/// The choices of the Appearance popup. `None` leaves a token to the
/// desktop's theme and the browser's defaults.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Appearance {
    /// `None` follows the desktop.
    pub(crate) mode: Option<EnzThemeMode>,
    pub(crate) colours: Colours,
    pub(crate) frame: Frame,
    pub(crate) glass: GlassLook,
    pub(crate) address: AddressBar,
    /// The strength of a grain frame's texture.
    pub(crate) grain: f32,
    pub(crate) radius: Option<f32>,
    pub(crate) toolbar: Option<f32>,
    pub(crate) sidebar: Option<f32>,
    pub(crate) rail: Option<f32>,
    pub(crate) tab: Option<f32>,
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            mode: None,
            colours: Colours::Desktop,
            frame: Frame::Solid,
            glass: GlassLook::default(),
            address: AddressBar::default(),
            grain: GRAIN.2,
            radius: None,
            toolbar: None,
            sidebar: None,
            rail: None,
            tab: None,
        }
    }
}

fn number(document: &ThemeDocument, path: &str) -> Option<f32> {
    match document.get(path, EnzThemeMode::Dark) {
        Some(TokenValue::Number(value)) => Some(*value),
        _ => None,
    }
}

impl Appearance {
    /// The choices an application theme document holds.
    pub(crate) fn read(document: &ThemeDocument) -> Self {
        let frame = match document.get("window.frame", EnzThemeMode::Dark) {
            Some(TokenValue::Enum(name)) => match WindowFrame::from_name(name) {
                Some(WindowFrame::Glass(style)) => Some(Frame::Glass(Glass::of(style))),
                _ => None,
            },
            _ => None,
        };
        let grain = number(document, "grain.intensity").filter(|strength| *strength > 0.0);
        Self {
            mode: document
                .declares_mode()
                .then(|| match document.mode_preference() {
                    ModePreference::Light => Some(EnzThemeMode::Light),
                    ModePreference::Dark => Some(EnzThemeMode::Dark),
                    ModePreference::System => None,
                })
                .flatten(),
            colours: Self::colours_of(document),
            frame: match (frame, grain) {
                (Some(glass), _) => glass,
                (None, Some(_)) => Frame::Grain,
                (None, None) => Frame::Solid,
            },
            glass: GlassLook::read(&document.to_toml()),
            address: AddressBar::read(&document.to_toml()),
            grain: ((grain.unwrap_or(GRAIN.2) * 100.0).round() / 100.0).clamp(GRAIN.0, GRAIN.1),
            radius: number(document, "window.radius"),
            toolbar: number(document, "sizing.toolbar_height").map(f32::round),
            sidebar: number(document, "sizing.sidebar_width").map(f32::round),
            rail: number(document, "sizing.sidebar_min_width").map(f32::round),
            tab: number(document, "sizing.sidebar_row_height").map(f32::round),
        }
    }

    fn colours_of(document: &ThemeDocument) -> Colours {
        let held = |role: &str, mode| match document.get(role, mode) {
            Some(TokenValue::Color(color)) => Some(eight_bits(*color)),
            _ => None,
        };
        let any = MODES.into_iter().any(|mode| {
            TINTED_ROLES
                .into_iter()
                .chain(ACCENT_ROLES)
                .any(|role| held(role, mode).is_some())
        });
        if !any {
            return Colours::Desktop;
        }
        SCHEMES
            .iter()
            .position(|scheme| {
                MODES.into_iter().all(|mode| {
                    scheme
                        .roles(mode)
                        .into_iter()
                        .all(|(role, color)| held(role, mode) == Some(color))
                })
            })
            .map_or(Colours::Custom, Colours::Scheme)
    }

    /// Writes into `document` the choices that differ from the ones it
    /// holds, leaving every other key of it as it is: what a person wrote
    /// by hand stays as written until that very choice is changed. Returns
    /// `false` if the schema refused a value. The glass values are not the
    /// schema's and are written by `save`, into the file's text.
    pub(crate) fn write(&self, document: &mut ThemeDocument) -> bool {
        let held = Self::read(document);
        let mut accepted = true;
        if self.mode != held.mode {
            match self.mode {
                Some(EnzThemeMode::Light) => document.set_mode_preference(ModePreference::Light),
                Some(EnzThemeMode::Dark) => document.set_mode_preference(ModePreference::Dark),
                None => document.clear_mode_preference(),
            }
        }
        if self.colours != held.colours {
            for mode in MODES {
                match self.colours {
                    Colours::Scheme(index) => {
                        let Some(scheme) = SCHEMES.get(index) else {
                            return false;
                        };
                        for (role, color) in scheme.roles(mode) {
                            accepted &= document.set(role, mode, TokenValue::Color(color));
                        }
                    }
                    Colours::Desktop => {
                        for role in TINTED_ROLES.into_iter().chain(ACCENT_ROLES) {
                            let _ = document.clear(role, mode);
                        }
                    }
                    // Only a file gives this; there is nothing of it to write.
                    Colours::Custom => {}
                }
            }
        }
        let common = EnzThemeMode::Dark;
        let frame_held =
            self.frame == held.frame && (self.frame != Frame::Grain || self.grain == held.grain);
        if !frame_held {
            match self.frame {
                Frame::Glass(glass) => {
                    accepted &= document.set(
                        "window.frame",
                        common,
                        TokenValue::Enum(glass.token().to_owned()),
                    );
                }
                Frame::Solid | Frame::Grain => {
                    let _ = document.clear("window.frame", common);
                }
            }
            if self.frame == Frame::Grain {
                accepted &= document.set(
                    "grain.intensity",
                    common,
                    TokenValue::Number(off_default(
                        self.grain.clamp(GRAIN.0, GRAIN.1),
                        SCHEMA_GRAIN,
                        0.002,
                    )),
                );
            } else {
                let _ = document.clear("grain.intensity", common);
            }
        }
        let [toolbar, sidebar, rail, tab] = SCHEMA_SIZES;
        for (path, value, was, (low, high), schema) in [
            ("window.radius", self.radius, held.radius, RADIUS, None),
            (
                "sizing.toolbar_height",
                self.toolbar,
                held.toolbar,
                (TOOLBAR.0, TOOLBAR.1),
                Some(toolbar),
            ),
            (
                "sizing.sidebar_width",
                self.sidebar,
                held.sidebar,
                (SIDEBAR.0, SIDEBAR.1),
                Some(sidebar),
            ),
            (
                "sizing.sidebar_min_width",
                self.rail,
                held.rail,
                (RAIL.0, RAIL.1),
                Some(rail),
            ),
            (
                "sizing.sidebar_row_height",
                self.tab,
                held.tab,
                (TAB.0, TAB.1),
                Some(tab),
            ),
        ] {
            match value {
                _ if value == was => {}
                Some(value) => {
                    let value = value.clamp(low, high);
                    let value =
                        schema.map_or(value, |schema| off_default(value, schema, OFF_DEFAULT));
                    accepted &= document.set(path, common, TokenValue::Number(value));
                }
                None => {
                    let _ = document.clear(path, common);
                }
            }
        }
        accepted
    }
}

/// The sizes of the browser's chrome under `theme`, and what its frame is
/// made of.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Metrics {
    /// The height of the top bar, with its padding.
    pub(crate) toolbar: f32,
    pub(crate) sidebar: f32,
    pub(crate) rail: f32,
    /// The height of a tab's row in the sidebar; the top strip's is 4 less.
    pub(crate) tab: f32,
    pub(crate) frame: Frame,
}
impl Metrics {
    pub(crate) fn of(theme: &EnzTheme) -> Self {
        // A sizing token moves the chrome only once a theme file has set it:
        // the schema's defaults describe the framework's shell, not this one.
        let chosen = |value: f32, schema: f32, (low, high, own): (f32, f32, f32)| {
            if (value - schema).abs() < OFF_DEFAULT / 2.0 {
                own
            } else {
                value.clamp(low, high).round()
            }
        };
        let sizing = &theme.sizing;
        let [toolbar, sidebar, rail, tab] = SCHEMA_SIZES;
        let frame = match theme.window_frame {
            WindowFrame::Glass(style) => Frame::Glass(Glass::of(style)),
            _ if (theme.grain.intensity - SCHEMA_GRAIN).abs() > 0.001
                && theme.grain.intensity > 0.0 =>
            {
                Frame::Grain
            }
            _ => Frame::Solid,
        };
        Self {
            toolbar: chosen(sizing.toolbar_height, toolbar, TOOLBAR),
            sidebar: chosen(sizing.sidebar_width, sidebar, SIDEBAR),
            rail: chosen(sizing.sidebar_min_width, rail, RAIL),
            tab: chosen(sizing.sidebar_row_height, tab, TAB),
            frame,
        }
    }
}

/// Reads the application theme file at `path`; a missing file is an empty
/// document.
pub(crate) fn load(path: &Path) -> Result<ThemeDocument, String> {
    use std::io::Read;
    let limit = enzui_core::theme_file::MAX_THEME_FILE_BYTES;
    match enzui_core::atomic_state::open_state_file_nofollow(path) {
        Ok(file) => {
            let mut text = String::new();
            file.take(limit + 1)
                .read_to_string(&mut text)
                .map_err(|_| "The browser's theme file could not be read.".to_owned())?;
            if text.len() as u64 > limit {
                return Err("The browser's theme file is too large. It was left as it is.".into());
            }
            ThemeDocument::parse(&text)
                .map_err(|_| "The browser's theme file is invalid. It was left as it is.".into())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(ThemeDocument::empty()),
        Err(_) => Err("The browser's theme file could not be opened.".into()),
    }
}

/// Saves `appearance` into the application theme file at `path`, as an edit
/// of what the file holds now: keys the popup does not manage, comments and
/// keys this build does not know stay.
pub(crate) fn save(path: &Path, appearance: &Appearance) -> Result<(), String> {
    let mut document = load(path)?;
    if !appearance.write(&mut document) {
        return Err("A theme value was out of range. Nothing was saved.".into());
    }
    let text = appearance.glass.written(&document.to_toml());
    let text = appearance.address.written(&text);
    // A file made here starts with the framework's header for the shared
    // theme, which this file is not.
    let text = match text.split_once("\nversion = ") {
        Some((header, rest))
            if !header.is_empty()
                && header
                    .lines()
                    .all(|line| line.starts_with("# enzUI") || line.starts_with("# Omitted")) =>
        {
            format!("{HEADER}version = {rest}")
        }
        _ => text,
    };
    let text = without_empty_tables(&text);
    enzui_core::theme_file::save_theme_atomic(path, &text)
        .map_err(|_| "The browser's theme file could not be saved.".into())
}
/// `text` without the tables a cleared choice left holding nothing. A table
/// header followed by a comment stays, since the comment may be about it.
fn without_empty_tables(text: &str) -> String {
    let header = |line: &str| {
        let line = line.trim();
        line.starts_with('[') && line.ends_with(']')
    };
    let lines: Vec<&str> = text.lines().collect();
    let mut kept = String::with_capacity(text.len());
    for (index, line) in lines.iter().enumerate() {
        let next = lines[index + 1..]
            .iter()
            .find(|line| !line.trim().is_empty());
        if header(line) && next.is_none_or(|next| header(next)) {
            continue;
        }
        kept.push_str(line);
        kept.push('\n');
    }
    // What is written must still be the theme that was meant.
    if ThemeDocument::parse(&kept).is_ok() {
        kept
    } else {
        text.to_owned()
    }
}

const HEADER: &str = "# enzim Browser appearance. Only this browser reads this file.\n\
# Omitted values come from the desktop's theme (theme.toml beside it).\n";

/// Where the browser's theme file is, once the application has an identity.
pub(crate) fn path() -> Option<std::path::PathBuf> {
    enzui_core::app_identity::declared()
        .and_then(|app| enzui_core::theme_file::ThemePaths::from_env().app_user_path(&app))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Glass is light glass, with dark ink, above this linear luminance of
    /// its tint, and dark glass below the lower one (the material shader's
    /// `light_glass` ramp).
    const LIGHT_GLASS: (f32, f32) = (0.35, 0.65);

    fn linear_luminance(color: Color) -> f32 {
        let linear = |c: f32| {
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
    }

    #[test]
    fn a_glass_tint_carries_the_scheme_and_keeps_its_glass_light_or_dark() {
        let rose = &SCHEMES[5];
        let light = glass_tint(
            EnzTheme::light().palette.background,
            rose.accent(EnzThemeMode::Light),
            false,
        );
        let dark = glass_tint(
            EnzTheme::dark().palette.background,
            rose.accent(EnzThemeMode::Dark),
            true,
        );
        for tint in [light, dark] {
            assert!(tint.r > tint.g + 0.05 && tint.r > tint.b, "rose, not grey: {tint:?}");
        }
        assert!(linear_luminance(light) > LIGHT_GLASS.1, "{light:?}");
        assert!(linear_luminance(dark) < LIGHT_GLASS.0, "{dark:?}");
    }

    fn chosen() -> Appearance {
        Appearance {
            mode: Some(EnzThemeMode::Light),
            colours: Colours::Scheme(5),
            frame: Frame::Glass(Glass::Clear),
            glass: GlassLook {
                tint: Some(0.35),
                edge: None,
                shine: Some(1.0),
            },
            address: AddressBar {
                height: Some(46.0),
                width: None,
            },
            // A strength is kept only with a grain frame.
            grain: GRAIN.2,
            radius: Some(18.0),
            toolbar: Some(70.0),
            sidebar: Some(300.0),
            rail: Some(56.0),
            tab: Some(38.0),
        }
    }

    #[test]
    fn the_popups_choices_survive_the_theme_file_and_leave_other_keys_alone() {
        let scratch = crate::credentials::tests::Scratch::new();
        let path = scratch.path().with_file_name("org.enzim.Browser.toml");
        // A file someone edited by hand, with a key the popup does not manage.
        std::fs::write(
            &path,
            "version = 1\n# my own corner shapes\n[shape]\nbutton = \"lg\"\n",
        )
        .expect("hand-written file");
        save(&path, &chosen()).expect("save");
        let text = std::fs::read_to_string(&path).expect("saved file");
        assert!(
            text.contains("# my own corner shapes"),
            "comments stay: {text}"
        );
        assert!(
            text.contains("button = \"lg\""),
            "unmanaged keys stay: {text}"
        );
        let document = load(&path).expect("reload");
        assert_eq!(Appearance::read(&document), chosen());
        // The resolved theme carries the choices.
        let theme = document.resolve(EnzThemeMode::Light);
        let plain = EnzTheme::light().palette;
        assert_eq!(theme.palette.accent, SCHEMES[5].accent(EnzThemeMode::Light));
        assert_ne!(theme.palette.accent_hover, plain.accent_hover);
        assert_ne!(theme.palette.background, plain.background);
        assert_eq!(theme.palette.text, plain.text);
        assert_eq!(theme.window_radius, 18.0);
        let metrics = Metrics::of(&theme);
        assert_eq!(
            (metrics.toolbar, metrics.sidebar, metrics.rail, metrics.tab),
            (70.0, 300.0, 56.0, 38.0)
        );
        assert_eq!(metrics.frame, Frame::Glass(Glass::Clear));
        // The glass values are the browser's own table; the theme passes over it.
        assert!(
            text.contains("[browser.glass]\ntint = 0.35\nshine = 1.0\n"),
            "{text}"
        );
        assert_eq!(
            chosen().glass.over(Glass::Clear),
            GlassPaint {
                tint: 0.35,
                edge: Glass::Clear.edge(),
                shine: 1.0
            }
        );
        // So is the address bar's size.
        assert!(text.contains("[browser.address]\nheight = 46\n"), "{text}");

        // Back to the defaults: the managed keys go, the others stay.
        save(&path, &Appearance::default()).expect("reset");
        let text = std::fs::read_to_string(&path).expect("reset file");
        assert!(text.contains("button = \"lg\""));
        for gone in [
            "accent",
            "background",
            "palette",
            "browser",
            "tint",
            "toolbar_height",
            "frame",
            "mode",
            "radius",
            "[sizing]",
        ] {
            assert!(!text.contains(gone), "{gone} is removed: {text}");
        }
        assert!(text.contains("# my own corner shapes\n[shape]"), "{text}");
        assert_eq!(
            Appearance::read(&load(&path).expect("reload")),
            Appearance::default()
        );

        // A file made here says whose it is; a hand-written one keeps its top.
        assert!(!text.starts_with(HEADER));
        let fresh = path.with_file_name("fresh.toml");
        save(&fresh, &chosen()).expect("first save");
        let text = std::fs::read_to_string(&fresh).expect("fresh file");
        assert!(text.starts_with(HEADER), "{text}");
        assert!(!text.contains("shared theme"), "{text}");
        assert_eq!(Appearance::read(&load(&fresh).expect("reload")), chosen());
    }

    #[test]
    fn what_was_written_by_hand_stays_until_that_choice_is_changed() {
        let scratch = crate::credentials::tests::Scratch::new();
        let path = scratch.path().with_file_name("by-hand.toml");
        let by_hand = "version = 1\n\n[light.palette]\naccent = \"#0f9d94\"\n\n\
                       [sizing]\nsidebar_width = 300.4\n\n[grain]\nintensity = 0.9\n";
        std::fs::write(&path, by_hand).expect("hand-written file");
        let mut shown = Appearance::read(&load(&path).expect("load"));
        assert_eq!(shown.colours, Colours::Custom, "an accent no scheme wrote");
        assert_eq!(
            (shown.sidebar, shown.frame, shown.grain),
            (Some(300.0), Frame::Grain, GRAIN.1)
        );

        // Saving with nothing changed changes nothing.
        save(&path, &shown).expect("save");
        assert_eq!(std::fs::read_to_string(&path).expect("file"), by_hand);

        // One choice changes one key.
        shown.rail = Some(60.0);
        save(&path, &shown).expect("save");
        let text = std::fs::read_to_string(&path).expect("file");
        assert_eq!(
            text,
            by_hand.replace("300.4\n", "300.4\nsidebar_min_width = 60\n"),
            "{text}"
        );

        // Choosing a scheme writes all of it, for both modes.
        shown.colours = Colours::Scheme(6);
        save(&path, &shown).expect("save");
        let document = load(&path).expect("load");
        for mode in MODES {
            assert_eq!(
                document.resolve(mode).palette.accent,
                SCHEMES[6].accent(mode)
            );
        }
        assert_eq!(Appearance::read(&document), shown);

        // Giving the colours back to the desktop removes every one of them.
        shown.colours = Colours::Desktop;
        save(&path, &shown).expect("save");
        let text = std::fs::read_to_string(&path).expect("file");
        assert!(!text.contains("palette"), "{text}");
        assert!(text.contains("sidebar_width = 300.4"), "{text}");
    }

    /// WCAG contrast of two opaque colours.
    fn contrast(a: Color, b: Color) -> f32 {
        let luminance = |color: Color| {
            let linear = |c: f32| {
                if c <= 0.04045 {
                    c / 12.92
                } else {
                    ((c + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
        };
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn every_scheme_tints_the_frame_a_little_and_keeps_its_text_legible() {
        let mut frames = Vec::new();
        for (index, scheme) in SCHEMES.iter().enumerate() {
            let chosen = Appearance {
                colours: Colours::Scheme(index),
                ..Appearance::default()
            };
            let mut document = ThemeDocument::empty();
            assert!(chosen.write(&mut document), "{}", scheme.name);
            assert_eq!(Appearance::read(&document), chosen, "{}", scheme.name);
            for mode in MODES {
                let plain = match mode {
                    EnzThemeMode::Light => EnzTheme::light(),
                    EnzThemeMode::Dark => EnzTheme::dark(),
                }
                .palette;
                let palette = document.resolve(mode).palette;
                let name = format!("{} {mode:?}", scheme.name);
                // The frame takes the accent's hue, and stays close to the grey it was.
                let [_, a, b] = oklab(palette.background);
                let [_, accent_a, accent_b] = oklab(scheme.accent(mode));
                let chroma = a.hypot(b);
                assert!((0.008..=0.02).contains(&chroma), "{name}: chroma {chroma}");
                let turn = (b.atan2(a) - accent_b.atan2(accent_a)).abs();
                let turn = turn.min(std::f32::consts::TAU - turn);
                assert!(turn < 0.2, "{name}: hue off by {turn}");
                let [lightness, ..] = oklab(palette.background);
                let [plain_lightness, ..] = oklab(plain.background);
                assert!((lightness - plain_lightness).abs() < 0.01, "{name}");
                // White stays white: a light surface is not tinted.
                if mode == EnzThemeMode::Light {
                    assert_eq!(palette.surface, plain.surface, "{name}");
                }
                // Text keeps the contrast it has on every surface it is drawn on.
                for (surface, was) in [
                    (palette.background, plain.background),
                    (palette.surface_alt, plain.surface_alt),
                    (palette.surface_selected, plain.surface_selected),
                    (palette.surface_elevated, plain.surface_elevated),
                ] {
                    for ink in [palette.text, palette.text_muted] {
                        let (now, before) = (contrast(ink, surface), contrast(ink, was));
                        assert!(now > before - 0.2, "{name}: {now} from {before}");
                    }
                }
                // The accent is seen against the frame, and carries a label.
                assert!(contrast(palette.accent, palette.background) >= 3.0, "{name}");
                assert!(contrast(palette.on_accent(), palette.accent) >= 3.0, "{name}");
                frames.push(eight_bits(palette.background));
            }
        }
        // No two schemes share a frame colour.
        let mut distinct = frames.clone();
        distinct.sort_by_key(|color| color.into_rgba8());
        distinct.dedup();
        assert_eq!(distinct.len(), frames.len());
    }

    #[test]
    fn the_glass_values_keep_to_their_own_table_of_the_file() {
        let by_hand = "version = 1\n\n[browser.glass]\n# mine\ntint = 7\nedge = 0.5\n\n\
                       [browser.other]\nkept = true\n";
        let held = GlassLook::read(by_hand);
        assert_eq!(
            held,
            GlassLook {
                // Out of range: as good as absent, and left as written.
                tint: None,
                edge: Some(0.5),
                shine: None
            }
        );
        assert_eq!(held.written(by_hand), by_hand);
        let changed = GlassLook {
            shine: Some(0.2),
            ..held
        }
        .written(by_hand);
        assert_eq!(
            changed,
            by_hand.replace("edge = 0.5\n", "edge = 0.5\nshine = 0.2\n")
        );
        // Giving the values back takes out what was set here, nothing else.
        let cleared = GlassLook::default().written(&changed);
        assert!(!cleared.contains("edge") && !cleared.contains("shine"), "{cleared}");
        assert!(cleared.contains("tint = 7") && cleared.contains("kept = true"), "{cleared}");
        // A file without the table gets it, and loses it again whole.
        let fresh = GlassLook {
            tint: Some(0.456),
            ..GlassLook::default()
        }
        .written("version = 1\n");
        assert_eq!(fresh, "version = 1\n\n[browser.glass]\ntint = 0.46\n");
        assert_eq!(GlassLook::default().written(&fresh), "version = 1\n");
        // The theme passes over the table and still reads the rest.
        let document = ThemeDocument::parse(&format!("{fresh}\n[window]\nradius = 9\n"))
            .expect("a theme with the browser's table");
        assert_eq!(document.resolve(EnzThemeMode::Dark).window_radius, 9.0);
        assert_eq!(Appearance::read(&document).glass.tint, Some(0.46));
        // The level's own values stand where the popup set none.
        assert_eq!(
            GlassLook::default().over(Glass::Frosted),
            GlassPaint {
                tint: Glass::Frosted.tint(),
                edge: Glass::Frosted.edge(),
                shine: Glass::Frosted.shine()
            }
        );
    }

    #[test]
    fn the_address_bars_size_keeps_to_its_own_table_of_the_file() {
        // The bar as it comes is no choice, and is not written.
        let own = AddressBar::default();
        assert_eq!((own.height(), own.width()), (36.0, 95.0));
        assert_eq!(
            own.with_height(36.4).with_width(95.0),
            own,
            "the bar's own size is no choice"
        );
        assert_eq!(own.written("version = 1\n"), "version = 1\n");
        // A slider's value is kept whole and within its range.
        let chosen = own.with_height(47.6).with_width(12.0);
        assert_eq!(
            chosen,
            AddressBar {
                height: Some(48.0),
                width: Some(40.0)
            }
        );
        let text = chosen.written("version = 1\n\n[browser.glass]\ntint = 0.4\n");
        assert_eq!(
            text,
            "version = 1\n\n[browser.glass]\ntint = 0.4\n\n[browser.address]\nheight = 48\nwidth = 40\n"
        );
        assert_eq!(AddressBar::read(&text), chosen);
        // Giving one back takes out that key; giving both back, the table,
        // and the glass values beside it stay.
        let narrower = chosen.with_height(36.0).written(&text);
        assert!(!narrower.contains("height") && narrower.contains("width = 40"), "{narrower}");
        let cleared = own.written(&narrower);
        assert_eq!(cleared, "version = 1\n\n[browser.glass]\ntint = 0.4\n");
        // A size written by hand that the bar cannot take is as good as
        // absent, and stays as written until that size is chosen here.
        let by_hand = "version = 1\n\n[browser.address]\nheight = 500\nwidth = 62.4\n";
        let held = AddressBar::read(by_hand);
        assert_eq!(
            held,
            AddressBar {
                height: None,
                width: Some(62.0)
            }
        );
        assert_eq!(held.written(by_hand), by_hand);
        // The theme passes over the table and still reads the rest.
        let document = ThemeDocument::parse(&format!("{text}\n[window]\nradius = 9\n"))
            .expect("a theme with the browser's tables");
        assert_eq!(document.resolve(EnzThemeMode::Dark).window_radius, 9.0);
        assert_eq!(Appearance::read(&document).address, chosen);
    }

    #[test]
    fn the_chrome_keeps_its_own_sizes_until_a_theme_sets_one() {
        let schema = EnzTheme::dark();
        assert_eq!(
            [
                schema.sizing.toolbar_height,
                schema.sizing.sidebar_width,
                schema.sizing.sidebar_min_width,
                schema.sizing.sidebar_row_height,
            ],
            SCHEMA_SIZES
        );
        assert_eq!(schema.grain.intensity, SCHEMA_GRAIN);
        let own = Metrics::of(&schema);
        assert_eq!(
            (own.toolbar, own.sidebar, own.rail, own.tab, own.frame),
            (TOOLBAR.2, SIDEBAR.2, RAIL.2, TAB.2, Frame::Solid)
        );
        // Out-of-range values from a hand-written file are brought into range.
        let mut theme = EnzTheme::dark();
        theme.sizing.sidebar_width = 4000.0;
        theme.sizing.sidebar_min_width = 1.0;
        let metrics = Metrics::of(&theme);
        assert_eq!((metrics.sidebar, metrics.rail), (SIDEBAR.1, RAIL.0));
        // Grain shows once a theme asks for another strength than the default.
        let grained = Appearance {
            frame: Frame::Grain,
            grain: 0.08,
            ..Appearance::default()
        };
        let mut document = ThemeDocument::empty();
        assert!(grained.write(&mut document));
        assert_eq!(
            Metrics::of(&document.resolve(EnzThemeMode::Dark)).frame,
            Frame::Grain
        );
        assert_eq!(Appearance::read(&document), grained);
        // A choice that happens to be the schema's default is still a choice.
        // (The schema's grain is stronger than any the popup offers.)
        let same = Appearance {
            frame: Frame::Grain,
            grain: GRAIN.1,
            sidebar: Some(SCHEMA_SIZES[1]),
            tab: Some(SCHEMA_SIZES[3]),
            ..Appearance::default()
        };
        let mut document = ThemeDocument::empty();
        assert!(same.write(&mut document));
        let metrics = Metrics::of(&document.resolve(EnzThemeMode::Light));
        assert_eq!(
            (metrics.sidebar, metrics.tab, metrics.frame),
            (SCHEMA_SIZES[1], SCHEMA_SIZES[3], Frame::Grain)
        );
        assert_eq!(Appearance::read(&document), same);
    }

    #[test]
    fn a_file_that_cannot_be_read_as_a_theme_is_not_overwritten() {
        let scratch = crate::credentials::tests::Scratch::new();
        let path = scratch.path().with_file_name("broken.toml");
        std::fs::write(&path, "version = 1\n[light.palette\naccent = ").expect("broken file");
        assert!(save(&path, &chosen()).is_err());
        assert_eq!(
            std::fs::read_to_string(&path).expect("file"),
            "version = 1\n[light.palette\naccent = "
        );
    }
}
