#![cfg(feature = "wpe")]

use enzui_core::native_loop::Interest;
use enzui_webkit::ffi::NativeInterest;
use std::time::{Duration, Instant};

use enzui_webview::gpu::GpuStatus;

#[derive(Default)]
pub(crate) struct BackgroundMemory {
    hidden: bool,
    due: Option<Instant>,
}

impl BackgroundMemory {
    pub(crate) fn visibility(&mut self, visible: bool, now: Instant) {
        let hidden = !visible;
        if self.hidden != hidden {
            self.hidden = hidden;
            self.due = self.hidden.then_some(now + Duration::from_secs(60));
        }
    }

    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.due
    }

    pub(crate) fn take_due(&mut self, now: Instant, busy: bool) -> bool {
        if !self.hidden || self.due.is_none_or(|due| now < due) {
            return false;
        }
        self.due = Some(now + Duration::from_secs(if busy { 60 } else { 120 }));
        !busy
    }
}

// The headless presenter still completes compositor frames after GPU import is
// unavailable. Those completions must refresh the retained software image too.
pub(crate) fn snapshot_pending(
    visible: bool,
    gpu: bool,
    sampled: Option<u64>,
    acknowledged: u64,
) -> bool {
    visible && !gpu && sampled != Some(acknowledged)
}

pub(crate) fn import_pending(status: &GpuStatus) -> bool {
    matches!(status, GpuStatus::Pending | GpuStatus::Writing)
}

pub(crate) fn publication_deadline(
    deferred: Option<Instant>,
    pending: bool,
    frame_ready: bool,
    now: Instant,
) -> Option<Instant> {
    if frame_ready {
        return Some(now);
    }
    #[cfg(feature = "benchmark")]
    {
        static EAGER: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if *EAGER.get_or_init(|| {
            std::env::var_os("ENZUI_BENCH_EAGER_PUBLICATION").is_some_and(|value| value == "1")
        }) {
            return if pending { Some(now) } else { deferred };
        }
    }
    deferred.or_else(|| pending.then_some(now))
}

/// When a page task needs the owner thread: its publication (held back only by
/// an input batch, see [`publication_deadline`]) or its own timers (favicons,
/// clipboard polling, download checks), whichever is first. A timer is a wake
/// of its own: it must not hold back a page event that is waiting, such as a
/// window asking to close.
pub(crate) fn worker_deadline(
    input_due: Option<Instant>,
    timers: Option<Instant>,
    pending: bool,
    frame_ready: bool,
    now: Instant,
) -> Option<Instant> {
    publication_deadline(input_due, pending, frame_ready, now)
        .into_iter()
        .chain(timers)
        .min()
}

pub(crate) fn gpu_import_deadline(
    status: Option<GpuStatus>,
    negotiating_until: Option<Instant>,
    visible: bool,
    now: Instant,
) -> Option<Instant> {
    if !visible {
        None
    } else if status.as_ref().is_some_and(import_pending) {
        Some(negotiating_until.unwrap_or(now + Duration::from_millis(50)))
    } else if status == Some(GpuStatus::Imported) {
        None
    } else {
        negotiating_until
    }
}

pub(crate) fn compose(
    native: NativeInterest,
    input_due: Option<Instant>,
    import_due: Option<Instant>,
    now: Instant,
) -> Interest {
    let mut interest = Interest::new();
    for (fd, events) in native.descriptors {
        interest = interest.descriptor(fd, events);
    }
    if let Some(timeout) = native.timeout {
        interest = interest.deadline(now + timeout);
    }
    if let Some(due) = input_due {
        interest = interest.deadline(due);
    }
    if let Some(due) = import_due {
        // Renderer import status can change without a native descriptor event.
        let poll_due = now + Duration::from_millis(50);
        let due = if due <= now {
            poll_due
        } else {
            due.min(poll_due)
        };
        interest = interest.deadline(due);
    }
    interest
}

#[cfg(test)]
mod tests {
    use super::*;
    use enzui_core::native_loop::{NativeLoop, NativeReady};
    use iced::futures::StreamExt;
    use std::task::{Context, Poll, Waker};

    #[test]
    fn background_memory_maintains_hidden_pages_after_grace_and_defers_busy_pages() {
        let start = Instant::now();
        let mut memory = BackgroundMemory::default();
        assert!(!memory.take_due(start + Duration::from_secs(120), false));
        memory.visibility(false, start);
        memory.visibility(false, start + Duration::from_secs(30));
        assert_eq!(memory.deadline(), Some(start + Duration::from_secs(60)));
        assert!(!memory.take_due(start + Duration::from_secs(59), false));
        assert!(!memory.take_due(start + Duration::from_secs(60), true));
        assert_eq!(memory.deadline(), Some(start + Duration::from_secs(120)));
        assert!(memory.take_due(start + Duration::from_secs(120), false));
        assert!(!memory.take_due(start + Duration::from_secs(180), false));
        memory.visibility(false, start + Duration::from_secs(180));
        assert_eq!(memory.deadline(), Some(start + Duration::from_secs(240)));
        assert!(memory.take_due(start + Duration::from_secs(240), false));
        assert_eq!(memory.deadline(), Some(start + Duration::from_secs(360)));
        memory.visibility(true, start + Duration::from_secs(241));
        memory.visibility(false, start + Duration::from_secs(242));
        memory.visibility(true, start + Duration::from_secs(243));
        assert_eq!(memory.deadline(), None);
        assert!(!memory.take_due(start + Duration::from_secs(300), false));
    }

    #[test]
    fn software_snapshot_tracks_compositor_completion_without_input_or_idle_polling() {
        assert!(snapshot_pending(true, false, None, 0));
        assert!(!snapshot_pending(true, false, Some(17), 17));
        assert!(snapshot_pending(true, false, Some(17), 18));
        assert!(snapshot_pending(true, false, Some(u64::MAX), 0));
        assert!(!snapshot_pending(false, false, Some(17), 18));
        assert!(!snapshot_pending(true, true, Some(17), 18));
        let now = Instant::now();
        let due = publication_deadline(
            None,
            snapshot_pending(true, false, Some(17), 18),
            false,
            now,
        );
        assert!(
            wait(compose(
                NativeInterest {
                    descriptors: Vec::new(),
                    timeout: None
                },
                due,
                None,
                now
            )) < Duration::from_millis(30)
        );
        assert_eq!(
            publication_deadline(
                None,
                snapshot_pending(true, false, Some(18), 18),
                false,
                now
            ),
            None
        );
    }

    fn wait(interest: Interest) -> Duration {
        let watcher = NativeLoop::new().unwrap();
        watcher.arm(interest);
        let mut recipes = iced::advanced::subscription::into_recipes(
            watcher.subscription(std::convert::identity),
        );
        let mut stream = recipes
            .pop()
            .unwrap()
            .stream(Box::pin(iced::futures::stream::pending()));
        let started = Instant::now();
        let mut context = Context::from_waker(Waker::noop());
        while started.elapsed() < Duration::from_millis(300) {
            if let Poll::Ready(ready) = stream.poll_next_unpin(&mut context) {
                assert_eq!(ready, Some(NativeReady::Deadline));
                return started.elapsed();
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("host deadline did not wake the native owner");
    }

    #[test]
    fn non_frame_publication_respects_input_batch_deadline() {
        let now = Instant::now();
        let deferred = now + Duration::from_millis(40);
        let due = publication_deadline(Some(deferred), true, false, now);
        let elapsed = wait(compose(
            NativeInterest {
                descriptors: Vec::new(),
                timeout: None,
            },
            due,
            None,
            now,
        ));
        assert!(
            elapsed >= Duration::from_millis(30),
            "non-frame publication bypassed input coalescing: {elapsed:?}"
        );
        assert_eq!(publication_deadline(None, true, false, now), Some(now));
        assert_eq!(publication_deadline(None, false, false, now), None);
        assert_eq!(
            publication_deadline(Some(deferred), false, false, now),
            Some(deferred)
        );
    }

    #[test]
    fn a_waiting_page_event_is_not_held_back_by_a_timer() {
        let now = Instant::now();
        let favicon = now + Duration::from_secs(30);
        assert_eq!(worker_deadline(None, Some(favicon), true, false, now), Some(now));
        assert_eq!(worker_deadline(None, Some(favicon), false, false, now), Some(favicon));
        // An input batch still holds it back, and the earlier of the two wins.
        let batch = now + Duration::from_millis(16);
        assert_eq!(worker_deadline(Some(batch), Some(favicon), true, false, now), Some(batch));
        assert_eq!(worker_deadline(None, None, false, false, now), None);
    }

    #[test]
    fn ready_native_frame_wakes_before_input_batch_deadline() {
        let now = Instant::now();
        let due = publication_deadline(Some(now + Duration::from_secs(1)), true, true, now);
        let elapsed = wait(compose(
            NativeInterest {
                descriptors: Vec::new(),
                timeout: None,
            },
            due,
            None,
            now,
        ));
        assert!(
            elapsed < Duration::from_millis(30),
            "ready frame was held behind input: {elapsed:?}"
        );
    }

    #[test]
    fn writing_import_keeps_native_status_feedback_armed() {
        let now = Instant::now();
        let native = NativeInterest {
            descriptors: Vec::new(),
            timeout: None,
        };
        let due = gpu_import_deadline(Some(GpuStatus::Writing), None, true, now);
        let elapsed = wait(compose(native, None, due, now));
        assert!(elapsed < Duration::from_millis(250));
        assert!(now.elapsed() >= Duration::from_millis(30));
    }

    #[test]
    fn native_readiness_deadlines_include_host_work_and_avoid_expired_import_spins() {
        for source in [
            "input",
            "native",
            "import",
            "expired-import",
            "pending-import",
            "ready-frame",
        ] {
            let now = Instant::now();
            let soon = now + Duration::from_millis(20);
            let later = now + Duration::from_secs(1);
            let native = NativeInterest {
                descriptors: Vec::new(),
                timeout: (source == "native").then_some(Duration::from_millis(20)),
            };
            let input = Some(match source {
                "input" => soon,
                "ready-frame" => now,
                _ => later,
            });
            let import = match source {
                "import" => Some(soon),
                "expired-import" => Some(now - Duration::from_secs(1)),
                "pending-import" => Some(later),
                _ => None,
            };
            eprintln!("native_readiness_case={source}");
            let elapsed = wait(compose(native, input, import, now));
            assert!(
                elapsed < Duration::from_millis(250),
                "{source}: {elapsed:?}"
            );
            if matches!(source, "expired-import" | "pending-import") {
                assert!(
                    now.elapsed() >= Duration::from_millis(30),
                    "expired import armed an immediate wake storm"
                );
            }
        }
    }
}
