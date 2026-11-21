use std::sync::OnceLock;

use enzui_core::font_preferences::{DesktopFontPreferences, FontPreferenceRead};
use enzui_theme::font_role::FontRole;
use iced::futures::{Stream, channel::oneshot, stream};

static DESKTOP_FONT: OnceLock<Option<String>> = OnceLock::new();

pub(crate) fn read_once() -> impl Stream<Item = Option<String>> {
    stream::once(async {
        let (sender, receiver) = oneshot::channel();
        let spawned = std::thread::Builder::new()
            .name("enzim-desktop-web-font".into())
            .spawn(move || {
                let font = DESKTOP_FONT.get_or_init(|| {
                    let read = enzui_core::font_preferences::portal_with_file_fallback().read();
                    match read {
                        FontPreferenceRead::Delivered(preferences) => system_font(&preferences),
                        FontPreferenceRead::Unsupported | FontPreferenceRead::Failed { .. } => None,
                    }
                });
                let _ = sender.send(font.clone());
            });
        if spawned.is_err() {
            return None;
        }
        receiver.await.unwrap_or_default()
    })
}

fn system_font(preferences: &DesktopFontPreferences) -> Option<String> {
    preferences.family(FontRole::Ui)?;
    let family = preferences.parsed(FontRole::Ui)?.family;
    if family.len() > 512 || family.chars().any(char::is_control) {
        return None;
    }
    // WPE expects a trailing size; author CSS still controls size, weight and style.
    Some(format!("{family} 10"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_ui_family_does_not_replace_author_sizes_or_prose_family() {
        let preferences = DesktopFontPreferences::new()
            .with(FontRole::Ui, "Adwaita Sans Bold Italic 22")
            .with(FontRole::Document, "Noto Serif 12");
        assert_eq!(
            system_font(&preferences).as_deref(),
            Some("Adwaita Sans 10")
        );
        assert_eq!(system_font(&DesktopFontPreferences::new()), None);
        assert_eq!(
            system_font(&DesktopFontPreferences::new().with(FontRole::Ui, "bad\0font 11")),
            None
        );
    }
}
