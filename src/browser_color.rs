//! The colour picker for a page's colour field (`<input type=color>`). The
//! engine draws none; the page sees each colour as it is tried, and keeps
//! the one chosen.

use super::*;
use enzui_webkit::page::ColorChooserRequest;
use enzui_widgets::color_picker::Hsva;
use iced::widget::{Space, column, row};

#[derive(Debug, Clone)]
pub(crate) enum ColorCommand {
    /// The picker moved.
    Pick(Hsva),
    /// The hex field was typed in.
    Hex(String),
    /// Keep the colour on show.
    Choose,
}

/// A colour field being answered.
#[derive(Debug, Clone)]
pub(crate) struct Choosing {
    /// The tab whose page asked.
    tab: u64,
    request: ColorChooserRequest,
    value: Hsva,
    /// What is typed in the hex field, which need not be a colour yet.
    hex: String,
}

fn to_hsva(color: [u8; 4]) -> Hsva {
    let part = |byte: u8| f32::from(byte) / 255.0;
    Hsva::from_color(iced::Color {
        r: part(color[0]),
        g: part(color[1]),
        b: part(color[2]),
        a: part(color[3]),
    })
}

fn to_bytes(value: Hsva) -> [u8; 4] {
    let color = value.to_color();
    let byte = |part: f32| (part.clamp(0.0, 1.0) * 255.0).round() as u8;
    [byte(color.r), byte(color.g), byte(color.b), byte(color.a)]
}

/// A colour typed as `#rgb`, `#rrggbb` or, where the field takes an alpha,
/// `#rrggbbaa`; the `#` may be left out.
pub(crate) fn typed_color(text: &str, alpha: bool) -> Option<[u8; 4]> {
    let digits = text.trim().trim_start_matches('#');
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let pair = |at: usize| u8::from_str_radix(digits.get(at..at + 2)?, 16).ok();
    match digits.len() {
        3 => {
            let mut color = [0, 0, 0, 255];
            for (part, digit) in color.iter_mut().zip(digits.chars()) {
                // One digit stands for the same digit twice: `f` is `ff`.
                *part = digit.to_digit(16)? as u8 * 17;
            }
            Some(color)
        }
        6 => Some([pair(0)?, pair(2)?, pair(4)?, 255]),
        8 if alpha => Some([pair(0)?, pair(2)?, pair(4)?, pair(6)?]),
        _ => None,
    }
}

impl App {
    /// A page's colour field was opened.
    pub(super) fn color_requested(&mut self, tab: u64, request: ColorChooserRequest) {
        // One at a time: a second field cannot be opened behind the dialog,
        // and a request that arrives anyway leaves its field as it was.
        if self.browser.color.is_some() {
            self.submit_browser_action(BrowserAction::ColorAnswer(request.id, None));
            return;
        }
        let value = to_hsva(request.color);
        self.browser.color = Some(Choosing {
            tab,
            request,
            hex: value.to_hex(),
            value,
        });
        self.browser.panel = Panel::Color;
    }

    /// Gives up a colour dialog that is no longer the one on show: closed,
    /// or its tab left or sent elsewhere (a tab takes a new key with each
    /// document it is sent to). Runs after every message.
    pub(crate) fn settle_color(&mut self) {
        let Some(choosing) = &self.browser.color else {
            return;
        };
        let active = self.browser.saved.tabs[self.browser.saved.active].key;
        if self.browser.panel == Panel::Color
            && choosing.tab == active
            && self.browser.library.is_none()
        {
            return;
        }
        let (tab, id) = (choosing.tab, choosing.request.id);
        self.browser.color = None;
        if self.browser.panel == Panel::Color {
            self.browser.panel = Panel::None;
        }
        // The field's own colour comes back. Its page may be gone, which
        // the engine takes as nothing to answer.
        if let Some(index) = self.browser.saved.tabs.iter().position(|t| t.key == tab) {
            self.tab_action(index, BrowserAction::ColorAnswer(id, None));
        }
    }

    pub(super) fn color_command(&mut self, command: ColorCommand) -> Task<Message> {
        let Some(choosing) = &mut self.browser.color else {
            return Task::none();
        };
        let id = choosing.request.id;
        match command {
            ColorCommand::Pick(value) => {
                choosing.value = if choosing.request.alpha {
                    value
                } else {
                    Hsva { alpha: 1.0, ..value }
                };
                choosing.hex = choosing.value.to_hex();
                let color = to_bytes(choosing.value);
                self.submit_browser_action(BrowserAction::ColorPreview(id, color));
            }
            ColorCommand::Hex(text) => {
                let typed = typed_color(&text, choosing.request.alpha);
                choosing.hex = text;
                if let Some(color) = typed {
                    choosing.value = to_hsva(color);
                    self.submit_browser_action(BrowserAction::ColorPreview(id, color));
                }
            }
            ColorCommand::Choose => {
                let color = to_bytes(choosing.value);
                self.browser.color = None;
                self.browser.panel = Panel::None;
                self.submit_browser_action(BrowserAction::ColorAnswer(id, Some(color)));
            }
        }
        self.browser.chrome_revision = self.browser.chrome_revision.wrapping_add(1);
        Task::none()
    }

    pub(super) fn color_view(&self) -> Option<Element<'_, Message>> {
        let choosing = self.browser.color.as_ref()?;
        let command = |command| c(Command::Color(command));
        let mut picker = enzui_widgets::color_picker::enz_color_picker(choosing.value, move |value| {
            command(ColorCommand::Pick(value))
        })
        .label("Colour")
        .alpha(choosing.request.alpha)
        .hex_text(choosing.hex.clone())
        .on_hex_input(move |text| command(ColorCommand::Hex(text)))
        // A short window keeps the buttons under the picker in reach.
        .plane_height((self.browser.viewport.height - 300.0).clamp(80.0, 180.0))
        .id_prefix("page-color");
        if typed_color(&choosing.hex, choosing.request.alpha).is_none() {
            picker = picker.hex_error(if choosing.request.alpha {
                "Six or eight hex digits, like #1a2b3c"
            } else {
                "Six hex digits, like #1a2b3c"
            });
        }
        // In a low window the picker scrolls between the heading and the
        // buttons, which stay in reach.
        let picker: Element<'_, Message> = if self.browser.viewport.height < 560.0 {
            enzui_widgets::scroll_view::enz_scrollable(picker.into_element())
                .height((self.browser.viewport.height - 232.0).max(100.0))
                .into()
        } else {
            picker.into_element()
        };
        Some(
            column![
                heading(
                    "palette",
                    "Choose a colour",
                    site_name(&self.browser.saved.tabs[self.browser.saved.active].url),
                ),
                picker,
                row![
                    Space::new().width(Length::Fill),
                    action("Cancel", "x", Command::Dismiss),
                    primary("Choose", "check", Command::Color(ColorCommand::Choose)),
                ]
                .spacing(8),
            ]
            .spacing(10)
            .into(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use enzui_webkit::page::{PageRequestId, ViewRect};

    fn request(alpha: bool) -> ColorChooserRequest {
        ColorChooserRequest {
            id: PageRequestId::new(7),
            color: [255, 0, 0, 255],
            alpha,
            area: ViewRect::default(),
        }
    }

    #[test]
    fn a_typed_colour_is_three_six_or_with_alpha_eight_hex_digits() {
        assert_eq!(typed_color("#1a2b3c", false), Some([0x1a, 0x2b, 0x3c, 255]));
        assert_eq!(typed_color(" 1A2B3C ", false), Some([0x1a, 0x2b, 0x3c, 255]));
        assert_eq!(typed_color("#f80", false), Some([255, 136, 0, 255]));
        assert_eq!(typed_color("#1a2b3c80", true), Some([0x1a, 0x2b, 0x3c, 0x80]));
        // A field without alpha takes none, and half a colour is none.
        assert_eq!(typed_color("#1a2b3c80", false), None);
        for bad in ["", "#", "#12", "#1a2b3", "#1a2b3g", "red", "#1a2b3c8"] {
            assert_eq!(typed_color(bad, true), None, "{bad}");
        }
    }

    #[test]
    fn a_colour_is_shown_as_it_is_tried_and_kept_when_chosen() {
        let mut app = browser_tests::app();
        app.browser_mode = true;
        let tab = app.browser.saved.tabs[0].key;
        app.color_requested(tab, request(false));
        assert_eq!(app.browser.panel, Panel::Color);
        assert!(app.color_view().is_some());
        let command = |command| c(Command::Color(command));
        // Typing half a colour changes nothing but the text.
        let _ = app.update(command(ColorCommand::Hex("#00f".into())));
        let _ = app.update(command(ColorCommand::Hex("#00ff".into())));
        let choosing = app.browser.color.as_ref().expect("choosing");
        assert_eq!(choosing.hex, "#00ff");
        assert_eq!(to_bytes(choosing.value), [0, 0, 255, 255]);
        // A field without alpha stays opaque whatever the picker sends.
        let _ = app.update(command(ColorCommand::Pick(Hsva {
            alpha: 0.2,
            ..to_hsva([0, 255, 0, 255])
        })));
        let choosing = app.browser.color.as_ref().expect("choosing");
        assert_eq!(to_bytes(choosing.value), [0, 255, 0, 255]);
        assert_eq!(choosing.hex, "#00ff00");
        let _ = app.update(command(ColorCommand::Choose));
        assert!(app.browser.color.is_none());
        assert_eq!(app.browser.panel, Panel::None);
    }

    #[test]
    fn a_colour_dialog_goes_with_its_page() {
        let mut app = browser_tests::app();
        app.browser_mode = true;
        let tab = app.browser.saved.tabs[0].key;
        // Closed with Escape or Cancel.
        app.color_requested(tab, request(true));
        let _ = app.update(c(Command::Dismiss));
        assert!(app.browser.color.is_none());
        // The tab was sent to another document.
        app.color_requested(tab, request(true));
        app.browser.replace_tab_key(0, next_tab_key());
        app.settle_color();
        assert!(app.browser.color.is_none());
        assert_eq!(app.browser.panel, Panel::None);
        // A second request behind an open dialog is not shown.
        app.color_requested(app.browser.saved.tabs[0].key, request(true));
        let mut second = request(true);
        second.id = PageRequestId::new(8);
        app.color_requested(app.browser.saved.tabs[0].key, second);
        assert_eq!(
            app.browser.color.as_ref().expect("choosing").request.id,
            PageRequestId::new(7)
        );
    }
}
