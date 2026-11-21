use crate::Message;
use enzui_core::semantics::{Role, Semantics};
use enzui_theme::EnzTheme;
use enzui_webkit::page::LoadStage;
use enzui_widgets::frame_clock::{enz_frame_clock, frame_clock_phase};
use enzui_widgets::semantics_container::SemanticsContainer;
use enzui_widgets::skeleton::pulse_alpha;
use iced::widget::container;
use iced::{Element, Length};

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) enum InitialPaint {
    #[default]
    AwaitingCommit,
    AwaitingFrame,
    Presented,
}

impl InitialPaint {
    pub(crate) fn load(&mut self, stage: LoadStage) {
        if *self != Self::Presented {
            match stage {
                LoadStage::Started => *self = Self::AwaitingCommit,
                LoadStage::Committed => *self = Self::AwaitingFrame,
                _ => {}
            }
        }
    }

    pub(crate) fn frame_received(&mut self) -> bool {
        if *self == Self::AwaitingFrame {
            *self = Self::Presented;
            true
        } else {
            false
        }
    }

    pub(crate) fn waiting(&self) -> bool {
        *self != Self::Presented
    }
}

pub(crate) fn placeholder(theme: &EnzTheme) -> Element<'static, Message> {
    let palette = theme.palette.clone();
    let phase = frame_clock_phase();
    let paint_phase = std::rc::Rc::clone(&phase);
    let reduced = theme.motion.reduced_motion;
    let ink = palette.text;
    let pill = container(iced::widget::Space::new().width(64).height(4)).style(move |_| {
        container::Style {
            background: Some(
                iced::Color {
                    a: 0.24 * pulse_alpha(paint_phase.get(), 0.3, 0.6, reduced),
                    ..ink
                }
                .into(),
            ),
            border: iced::Border {
                radius: 2.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    });
    let pulse = enz_frame_clock(pill, phase)
        .active(!reduced)
        .reduced_motion(reduced)
        .cadence(std::time::Duration::from_millis(50))
        .into_element();
    let pulse = SemanticsContainer::leaf(
        pulse,
        Semantics::new(Role::ProgressIndicator).label("Loading page"),
    );
    container(container(pulse).width(64).height(4))
        .center(Length::Fill)
        .style(move |_| container::Style {
            background: Some(palette.surface.into()),
            ..Default::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_paint_ignores_bootstrap_frames_and_stays_finished_during_reload() {
        let mut paint = InitialPaint::default();
        paint.load(LoadStage::Started);
        assert!(
            !paint.frame_received(),
            "bootstrap frames do not end loading"
        );
        assert!(paint.waiting());
        paint.load(LoadStage::Committed);
        assert!(
            paint.waiting(),
            "commit alone does not contain a page frame"
        );
        assert!(paint.frame_received());
        assert!(
            !paint.waiting(),
            "the first frame ends loading before load completion"
        );
        paint.load(LoadStage::Started);
        paint.load(LoadStage::Committed);
        assert!(
            !paint.waiting(),
            "retained content stays visible during reload"
        );
        assert!(
            !paint.frame_received(),
            "later frames do not rebuild chrome"
        );

        let mut redirected = InitialPaint::default();
        redirected.load(LoadStage::Committed);
        redirected.load(LoadStage::Started);
        assert!(
            !redirected.frame_received(),
            "superseded loads need a fresh commit"
        );
        redirected.load(LoadStage::Redirected);
        assert!(redirected.waiting());
    }
}
