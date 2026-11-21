//! Operation results and failures, as toasts in the window's bottom-right corner.
//!
//! Failures stay until they are dismissed; results and status dismiss
//! themselves. Every toast has a close button.

use enzui_widgets::toast::ToastTone;
use enzui_widgets::toast_host::{ToastEvent, ToastExpirySchedule, ToastHost, ToastSpec};

#[derive(Debug)]
pub(crate) struct Toasts {
    host: ToastHost,
    schedule: ToastExpirySchedule,
    /// Bumped on every change, so an update that only changed toasts still
    /// redraws.
    revision: u64,
    #[cfg(test)]
    log: Vec<(ToastTone, String)>,
}

impl Default for Toasts {
    fn default() -> Self {
        Self {
            host: ToastHost::new().id_prefix("browser-toast"),
            schedule: ToastExpirySchedule::default(),
            revision: 0,
            #[cfg(test)]
            log: Vec::new(),
        }
    }
}

impl Toasts {
    /// A failure; it stays until dismissed. A repeat of the same `key` (the
    /// same operation failing again) replaces the earlier toast.
    pub(crate) fn error(&mut self, key: &str, text: impl Into<String>) {
        self.push(
            ToastSpec::new(text)
                .tone(ToastTone::Error)
                .coalesce_key(key),
        );
    }

    /// A finished operation; it dismisses itself.
    pub(crate) fn done(&mut self, text: impl Into<String>) {
        self.push(ToastSpec::new(text).tone(ToastTone::Success));
    }

    /// A status line with the given tone; it dismisses itself.
    pub(crate) fn info(&mut self, tone: ToastTone, text: impl Into<String>) {
        self.push(ToastSpec::new(text).tone(tone));
    }

    /// Status the user has to act on, such as a restart; it stays until
    /// dismissed.
    pub(crate) fn sticky(&mut self, text: impl Into<String>) {
        self.push(ToastSpec::new(text).sticky(true));
    }

    fn push(&mut self, spec: ToastSpec) {
        if spec.text.trim().is_empty() {
            return;
        }
        #[cfg(test)]
        self.log.push((spec.tone, spec.text.clone()));
        self.host.push(spec);
        self.revision = self.revision.wrapping_add(1);
    }

    pub(crate) fn update(&mut self, event: ToastEvent) {
        let dismissed = matches!(event, ToastEvent::Dismiss(_) | ToastEvent::Action(..));
        let _ = self.host.update(event);
        if dismissed {
            // A dismissal exposes waiting toasts without starting their dwell.
            let _ = self.host.tick_at(std::time::Instant::now());
        }
        self.revision = self.revision.wrapping_add(1);
    }

    /// Re-arm the expiry wake after an update.
    pub(crate) fn sync(&mut self) {
        let _ = self
            .host
            .sync_expiry(&mut self.schedule, std::time::Instant::now());
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    /// Every toast pushed so far, oldest first.
    #[cfg(test)]
    pub(crate) fn texts(&self) -> &[(ToastTone, String)] {
        &self.log
    }

    /// Wakes for the earliest toast deadline, or nothing while none expires.
    pub(crate) fn subscription(&self) -> iced::Subscription<ToastEvent> {
        match self.schedule.armed() {
            Some((deadline, wait)) => iced::time::every(wait)
                .with(deadline)
                .map(|(_, now)| ToastEvent::Tick(now)),
            None => iced::Subscription::none(),
        }
    }

    /// The toasts, or `None` while there are none.
    pub(crate) fn view(&self) -> Option<iced::Element<'_, ToastEvent>> {
        (!self.host.is_empty()).then(|| self.host.view())
    }
}
