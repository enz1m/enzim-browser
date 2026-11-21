//! The mark on a tab whose page sounds: a small moving wave at the top left
//! corner of its icon, or a crossed speaker once the person silenced it.
//! Under the pointer the mark turns into the button it is, showing what a
//! press does: silence the tab, or let it sound again.

use super::*;
use iced::widget::{Space, container, row};

/// How often the wave moves on.
pub(crate) const BEAT: std::time::Duration = std::time::Duration::from_millis(120);
/// The heights of the wave's three bars, beat after beat, in pixels. Each
/// bar swells and shrinks by two pixels a beat, out of step with its
/// neighbours, about the middle line: it grows up and down alike.
const WAVE: [[f32; 3]; 6] = [
    [4.0, 10.0, 6.0],
    [6.0, 8.0, 8.0],
    [8.0, 6.0, 10.0],
    [10.0, 4.0, 8.0],
    [8.0, 6.0, 6.0],
    [6.0, 8.0, 4.0],
];
/// The tallest a bar gets.
const TALLEST: f32 = 10.0;
/// The side of the mark's box, and how far it reaches out over the corner
/// of the 16 px icon it belongs to.
pub(crate) const MARK: f32 = 16.0;
pub(crate) const REACH: f32 = 6.0;

/// What a tab's icon says about its sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sound {
    Sounding,
    Muted,
}

impl Sound {
    /// The symbol of what a press on the mark does.
    fn press_symbol(self) -> &'static str {
        match self {
            Self::Sounding => "volume-x",
            Self::Muted => "volume-2",
        }
    }
}

/// The wave at `beat`, or where motion is to be spared, at rest.
pub(crate) fn wave(beat: usize, reduced_motion: bool) -> [f32; 3] {
    if reduced_motion {
        WAVE[0]
    } else {
        WAVE[beat % WAVE.len()]
    }
}

impl App {
    pub(crate) fn tab_sound(&self, key: u64) -> Option<Sound> {
        if self.browser.muted.contains(&key) {
            Some(Sound::Muted)
        } else if self.browser.audible.contains(&key) {
            Some(Sound::Sounding)
        } else {
            None
        }
    }

    /// Whether a wave is on show and moving: a tab sounds, is not silenced,
    /// and the desktop does not ask for less motion. Nothing ticks otherwise.
    pub(crate) fn sound_animates(&self) -> bool {
        !self.look().motion.reduced_motion
            && self
                .browser
                .audible
                .iter()
                .any(|key| !self.browser.muted.contains(key))
    }

    pub(super) fn sound_beat(&mut self) {
        self.browser.sound_beat = self.browser.sound_beat.wrapping_add(1);
        self.browser.chrome_revision = self.browser.chrome_revision.wrapping_add(1);
    }

    /// The mark as it shows: the wave, with bars `width` wide and their
    /// heights times `scale`, or the crossed speaker. `ground` is the colour
    /// of the tab under it: each bar is edged with it, and the speaker
    /// stands on it, so that the mark is cut out of the icon it crosses
    /// instead of tangling with it.
    fn sound_sign<'a>(
        &self,
        sound: Sound,
        width: f32,
        scale: f32,
        ground: iced::Color,
    ) -> Element<'a, Message> {
        let ink = self.look().palette.text;
        match sound {
            Sound::Muted => container(
                enzui_widgets::icon::enz_icon("volume-x")
                    .size(12.0 * scale)
                    .tint(ink)
                    .into_element(),
            )
            .padding(1)
            .style(move |_| container::Style {
                background: Some(ground.into()),
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into(),
            Sound::Sounding => {
                let bars = wave(self.browser.sound_beat, self.look().motion.reduced_motion);
                // A bar and the edge around it; the edges of neighbours
                // meet, which leaves the bars a bar's width apart.
                let mut wave = row![].spacing((width - 2.0).max(0.0));
                for height in bars {
                    let bar = container(Space::new())
                        .width(width + 2.0)
                        .height(height * scale + 2.0)
                        .style(move |_| container::Style {
                            background: Some(ink.into()),
                            border: iced::Border {
                                radius: (width / 2.0 + 1.0).into(),
                                width: 1.0,
                                color: ground,
                            },
                            ..Default::default()
                        });
                    // Each bar in the middle of the tallest one's room.
                    wave = wave.push(container(bar).center_y(TALLEST * scale + 2.0));
                }
                wave.into()
            }
        }
    }

    /// The mark as a button, [`MARK`] on a side: the wave or the crossed
    /// speaker, and under the pointer the symbol of what a press does, on a
    /// round ground.
    pub(super) fn sound_button(
        &self,
        index: usize,
        sound: Sound,
        ground: iced::Color,
    ) -> Element<'_, Message> {
        let ink = self.look().palette.text;
        let mark = enzui_widgets::hover_group::enz_hover_group_with(move |hovered| {
            let sign: Element<'_, Message> = if hovered {
                enzui_widgets::icon::enz_icon(sound.press_symbol())
                    .size(11.0)
                    .tint(ink)
                    .into_element()
            } else {
                self.sound_sign(sound, 2.0, 1.0, ground)
            };
            container(sign).center_x(MARK).center_y(MARK)
        })
        .width(MARK)
        .height(MARK)
        .style(|theme, hovered| {
            let p = palette(theme);
            enzui_widgets::hover_group::HoverGroupStyle {
                background: if hovered {
                    p.surface_elevated.mix(p.text, 0.16)
                } else {
                    iced::Color::TRANSPARENT
                },
                border_color: iced::Color::TRANSPARENT,
                border_width: 0.0,
                radius: MARK / 2.0,
            }
        })
        .into_element();
        iced::widget::mouse_area(mark)
            .on_press(c(Command::Action(ActionCommand::Mute(index))))
            .interaction(iced::mouse::Interaction::Pointer)
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wave_moves_only_while_an_unsilenced_tab_sounds() {
        let mut app = browser_tests::app();
        app.browser_mode = true;
        let key = app.browser.saved.tabs[0].key;
        assert_eq!(app.tab_sound(key), None);
        assert!(!app.sound_animates());

        app.tab_sounds(key, true);
        assert_eq!(app.tab_sound(key), Some(Sound::Sounding));
        assert!(app.sound_animates());
        let (beat, revision) = (app.browser.sound_beat, app.browser.chrome_revision);
        let _ = app.update(c(Command::SoundBeat));
        assert_eq!(app.browser.sound_beat, beat + 1);
        assert_ne!(app.browser.chrome_revision, revision);

        // Silenced: the crossed speaker stays, and nothing ticks for it.
        let _ = app.update(c(Command::Action(ActionCommand::Mute(0))));
        assert_eq!(app.tab_sound(key), Some(Sound::Muted));
        assert!(!app.sound_animates());
        // It stays marked when its page falls silent, until it sounds again.
        app.tab_sounds(key, false);
        assert_eq!(app.tab_sound(key), Some(Sound::Muted));
        let _ = app.update(c(Command::Action(ActionCommand::Mute(0))));
        assert_eq!(app.tab_sound(key), None);
    }

    #[test]
    fn the_wave_changes_from_beat_to_beat_and_rests_where_motion_is_spared() {
        assert_ne!(wave(0, false), wave(1, false));
        assert_eq!(wave(0, false), wave(WAVE.len(), false));
        assert_eq!(wave(0, true), wave(3, true));
        // Every bar fits the mark, and has a whole pixel above and below
        // the middle line at every beat.
        assert!(WAVE.iter().flatten().all(|height| {
            (2.0..=TALLEST).contains(height) && height % 2.0 == 0.0
        }));
        // A bar never jumps: two pixels a beat, round the loop.
        for (beat, now) in WAVE.iter().enumerate() {
            let next = WAVE[(beat + 1) % WAVE.len()];
            for (bar, (height, after)) in now.iter().zip(next).enumerate() {
                assert_eq!((height - after).abs(), 2.0, "bar {bar} beat {beat}");
            }
        }
    }
}
