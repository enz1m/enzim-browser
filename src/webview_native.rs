//! Native page workers and browser actions.

#[cfg(feature = "benchmark")]
pub(crate) fn trace_navigation(stage: &str) {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ENABLED.get_or_init(|| {
        std::env::var_os("ENZIM_BENCH_NAVIGATION_TRACE").is_some_and(|value| value == "1")
    }) {
        eprintln!(
            "NAVIGATION_TRACE {stage} {}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_micros()
        );
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CredentialFill {
    pub(crate) profile: enzui_webkit::web::WebProfile,
    pub(crate) lease: crate::credentials::service::Lease,
    pub(crate) site: crate::credentials::Site,
    pub(crate) source: enzui_security::secret::SecretText,
    pub(crate) job: crate::native_jobs::Job,
}

/// What pages asked of the windows they opened, by popup id, from the worker
/// that took each window until the tab or window around it is made.
static POPUP_FEATURES: std::sync::Mutex<Vec<(u64, enzui_webkit::page::WindowFeatures)>> =
    std::sync::Mutex::new(Vec::new());

#[cfg_attr(not(feature = "wpe"), allow(dead_code))]
pub(crate) fn remember_popup_features(id: u64, features: enzui_webkit::page::WindowFeatures) {
    let mut kept = POPUP_FEATURES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // Windows nobody took are forgotten with the oldest.
    if kept.len() >= 32 {
        kept.remove(0);
    }
    kept.push((id, features));
}

/// What the page asked of popup `id`'s window, once.
pub(crate) fn take_popup_features(id: u64) -> Option<enzui_webkit::page::WindowFeatures> {
    let mut kept = POPUP_FEATURES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let at = kept.iter().rposition(|(kept, _)| *kept == id)?;
    Some(kept.remove(at).1)
}

/// The first line of a view's certificate details when the person continued
/// past a certificate that failed validation.
pub(crate) const CERTIFICATE_OVERRIDDEN: &str =
    "The certificate could not be verified; you chose to continue";

#[derive(Debug, Clone)]
pub(crate) enum BrowserAction {
    ColorScheme(bool),
    SystemFont(String),
    MemoryProtected(bool),
    Privacy(Box<crate::privacy::Privacy>),
    PauseDownload {
        id: enzui_webkit::page::PageRequestId,
        job: crate::native_jobs::Job,
    },
    ResumeDownload {
        url: String,
        offset: u64,
        validator: String,
        job: crate::native_jobs::Job,
    },
    CancelRangeDownload {
        url: String,
        job: crate::native_jobs::Job,
    },
    CancelDownload {
        id: enzui_webkit::page::PageRequestId,
        job: crate::native_jobs::Job,
    },
    DownloadUrl {
        url: String,
        job: crate::native_jobs::Job,
    },
    PermissionAnswer(enzui_webkit::page::PageRequestId, bool),
    PermissionChoices(Vec<crate::site_permissions::Choice>),
    NotificationsEnabled(bool),
    NotificationClick(u64),
    NotificationClose(u64),
    DeveloperTools(bool),
    /// Replaces the local destinations the view's profile may reach.
    LocalOrigins(Vec<String>),
    Fixture,
    Navigate(String),
    Back,
    Forward,
    Reload,
    Zoom(f64),
    Find(String, bool),
    FindFinish,
    FindNext(bool),
    Files(enzui_webkit::page::PageRequestId, Vec<std::path::PathBuf>),
    Option(enzui_webkit::page::PageRequestId, Option<usize>),
    Dialog(
        enzui_webkit::page::PageRequestId,
        enzui_webkit::page::ScriptDialogAnswer,
    ),
    Authentication(
        enzui_webkit::page::PageRequestId,
        enzui_webkit::page::AuthAnswer,
    ),
    /// Continue to a host whose certificate failed validation, for as long
    /// as this view lives.
    TrustCertificate(String),
    Mute(bool),
    /// Save downloads to this folder from now on.
    DownloadFolder(std::path::PathBuf),
    /// Reload without the cache.
    ReloadFresh,
    /// Let a window a page opened, now a tab of its own with its filters and
    /// scripts in place, load what it was opened for.
    ResumePopup(String),
    /// Show a colour in the page's colour field while it is being chosen.
    ColorPreview(enzui_webkit::page::PageRequestId, [u8; 4]),
    /// Set a colour field, or with `None` leave it as it was.
    ColorAnswer(enzui_webkit::page::PageRequestId, Option<[u8; 4]>),
    /// Replace, accept or learn the misspelled word of the last context menu.
    Spelling(enzui_webkit::page::SpellingFix),
    /// Check spelling in these languages; none turns it off.
    SpellCheck(Vec<String>),
    Script(String),
    IsolatedScript(String),
    Stop,
    ClearData(enzui_webkit::page::WebsiteData),
    ClearDataJob {
        data: enzui_webkit::page::WebsiteData,
        job: crate::native_jobs::Job,
    },
    Cookies {
        job: crate::native_jobs::Job,
    },
    DeleteCookies {
        keys: Vec<enzui_webkit::ffi::runtime::CookieKey>,
        job: crate::native_jobs::Job,
    },
    ContentFilters {
        mode: usize,
        filters: Vec<std::sync::Arc<crate::filters::Source>>,
        job: crate::native_jobs::Job,
    },
    ContentFilter {
        mode: usize,
        json: std::sync::Arc<crate::filters::Source>,
        job: crate::native_jobs::Job,
    },
    ExportHtml {
        job: crate::native_jobs::Job,
    },
    ExportMhtml {
        job: crate::native_jobs::Job,
    },
    ExportScreenshot {
        job: crate::native_jobs::Job,
    },
    ExportPdf {
        settings: enzui_webkit::printing::PdfPrintSettings,
        job: crate::native_jobs::Job,
    },
    Console {
        source: String,
        job: crate::native_jobs::Job,
    },
    CredentialRefresh,
    Credential(Box<CredentialFill>),
    /// Answers a page's passkey request in the frame and document that made it.
    PasskeyReply {
        frame: enzui_webkit::page::FrameDocument,
        source: enzui_security::secret::SecretText,
    },
    Evaluate {
        source: String,
        world: enzui_webkit::page::ScriptWorld,
        job: crate::native_jobs::Job,
    },
}

// CSS top offsets for aimed clicks (text, password, progress region); height
// aims at field middles.
pub(crate) const TEXT_TOP: f32 = 120.0;
pub(crate) const PASSWORD_TOP: f32 = 200.0;
pub(crate) const HOVER_TOP: f32 = 280.0;
pub(crate) const FIELD_HEIGHT: f32 = 44.0;

// A typed refusal to dispatch a native page bootstrap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NativeOpenError {
    // Process-long execution lane unstartable or dead.
    #[cfg(feature = "wpe")]
    DispatchUnavailable(String),
    // Preceding page overran its close budget and still owns the lane.
    #[cfg(feature = "wpe")]
    PreviousPageStillReleasing,
    // No native WPE backend in this build.
    #[cfg(not(feature = "wpe"))]
    BackendUnavailable,
}

impl core::fmt::Display for NativeOpenError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            #[cfg(feature = "wpe")]
            Self::DispatchUnavailable(detail) => {
                write!(f, "native WPE dispatch unavailable: {detail}")
            }
            #[cfg(feature = "wpe")]
            Self::PreviousPageStillReleasing => write!(
                f,
                "native WPE dispatch refused within its bounded handoff: the previous page is still releasing"
            ),
            #[cfg(not(feature = "wpe"))]
            Self::BackendUnavailable => write!(
                f,
                "no native backend: this probe was built without `--features wpe`; no mock was substituted"
            ),
        }
    }
}

impl std::error::Error for NativeOpenError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NativeWorkerError {
    // Display/session/profile bootstrap never completed.
    #[cfg(feature = "wpe")]
    Bootstrap {
        stage: &'static str,
        detail: String,
    },
    // Page-controlled title over the native byte ceiling.
    #[cfg(feature = "wpe")]
    PageTitle {
        // Pre-NUL bytes observed by the bounded reader.
        observed_bytes: usize,
        // Accepted native-byte ceiling.
        max_bytes: usize,
    },
    Http {
        status: u16,
    },
    // The site's certificate failed validation and the load was refused.
    Certificate {
        // The host as the engine keys its exception.
        host: String,
    },
    // The page's own process ended; a reload starts a fresh one.
    Renderer {
        // It was ended for the memory it took, rather than crashing.
        memory: bool,
    },
    // Already-open page's work failed.
    Command {
        operation: &'static str,
        detail: String,
    },
}

impl NativeWorkerError {
    fn load_failure(message: &str) -> Self {
        if let Some(status) = message
            .strip_prefix("HTTP response ")
            .and_then(|value| value.strip_suffix(" could not be displayed."))
            .and_then(|value| value.parse::<u16>().ok())
            .filter(|status| (400..600).contains(status))
        {
            Self::Http { status }
        } else {
            Self::Command {
                operation: "load page",
                detail: message.into(),
            }
        }
    }
}

impl core::fmt::Display for NativeWorkerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            #[cfg(feature = "wpe")]
            Self::Bootstrap { stage, detail } => write!(f, "native bootstrap {stage}: {detail}"),
            #[cfg(feature = "wpe")]
            Self::PageTitle {
                observed_bytes,
                max_bytes,
            } => write!(
                f,
                "native page title exceeds {max_bytes} bytes; observed at least {observed_bytes}"
            ),
            Self::Http { status } => write!(f, "HTTP response {status} could not be displayed."),
            Self::Certificate { host } => {
                write!(f, "The certificate of {host} could not be verified.")
            }
            Self::Renderer { memory: false } => f.write_str("The page's process ended unexpectedly."),
            Self::Renderer { memory: true } => {
                f.write_str("The page's process was ended for the memory it used.")
            }
            Self::Command { operation, detail } => {
                write!(f, "native command {operation}: {detail}")
            }
        }
    }
}

#[cfg(feature = "wpe")]
impl From<enzui_webkit::ffi::runtime::PageTitleError> for NativeWorkerError {
    fn from(error: enzui_webkit::ffi::runtime::PageTitleError) -> Self {
        match error {
            enzui_webkit::ffi::runtime::PageTitleError::TooLong {
                observed_bytes,
                max_bytes,
            } => Self::PageTitle {
                observed_bytes,
                max_bytes,
            },
        }
    }
}

#[cfg_attr(not(feature = "wpe"), allow(dead_code))]
pub(crate) fn add_command_error(
    update: &mut NativeUpdate,
    operation: &'static str,
    detail: String,
) {
    if update.error.is_none() {
        update.error = Some(NativeWorkerError::Command { operation, detail });
    }
}

// Complete frame-independent state; absent fields explicitly clear their value.
#[derive(Debug, Default)]
pub(crate) struct NativeMetadata {
    /// A load is under way in the engine.
    pub(crate) loading: bool,
    pub(crate) can_go_back: bool,
    pub(crate) can_go_forward: bool,
    pub(crate) tls: Option<String>,
    pub(crate) cursor: Option<String>,
    pub(crate) dom_report: Option<String>,
    pub(crate) location: Option<String>,
}

// Coalesced worker notification. No GLib/WebKit work happens while iced holds
// this value.
#[derive(Debug)]
pub(crate) struct NativeUpdate {
    pub(crate) page_events: Vec<enzui_webkit::page::PageEvent>,
    pub(crate) view: enzui_webkit::web::ViewId,
    pub(crate) report: Option<enzui_webkit::ime_bridge::NativeImeReport>,
    pub(crate) metadata: Option<std::sync::Arc<NativeMetadata>>,
    pub(crate) frame: Option<Result<enzui_core::media::Rgba8Frame, String>>,
    pub(crate) gpu_frame: Option<enzui_webview::gpu::GpuFrame>,
    pub(crate) tab_reply: Option<(
        enzui_webkit::web::Generation,
        enzui_webkit::web::focus::TabReply,
    )>,
    pub(crate) clipboard: Option<enzui_webkit::ClipboardOutcome>,
    pub(crate) selection: Option<SelectionText>,
    // Native WPE focus sampled after this update's work.
    pub(crate) view_focused: Option<bool>,
    pub(crate) error: Option<NativeWorkerError>,
}

pub(crate) struct SelectionText {
    pub(crate) text: String,
}
impl std::fmt::Debug for SelectionText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SelectionText([redacted])")
    }
}

// Queued-notification bound: key/motion floods cannot grow memory.
#[cfg_attr(not(feature = "wpe"), allow(dead_code))]
pub(crate) const UPDATE_CAP: usize = 8;

// Terminal-fallback bound: deduplicating, oldest-first evicted — pathological
// floods cannot grow memory.
#[cfg_attr(not(feature = "wpe"), allow(dead_code))]
pub(crate) const PENDING_TERMINAL_CAP: usize = 16;

// Terminal input accepted post-saturation with its monotone `seq`: ordered
// against all other admissions, so both queues interleave without inversion.
#[cfg_attr(not(feature = "wpe"), allow(dead_code))]
#[derive(Debug, Clone)]
pub(crate) struct PendingTerminal {
    pub(crate) seq: u64,
    pub(crate) event: enzui_webkit::input::InputEvent,
}

// Serialized admission for every worker command: bounded channel plus terminal-only
// fallback (lost terminals grab WebKit) — one mutex, one total order by `seq`.
#[cfg_attr(not(feature = "wpe"), allow(dead_code))]
#[derive(Debug, Default)]
pub(crate) struct Admission {
    memory_protected: bool,
    next_seq: u64,
    pending: std::collections::VecDeque<PendingTerminal>,
}

#[cfg_attr(not(feature = "wpe"), allow(dead_code))]
impl Admission {
    // Next monotone admission sequence; call under the lock that will `try_send`
    // or push, so both queues share one total order.
    pub(crate) fn assign_seq(&mut self) -> u64 {
        let seq = self.next_seq;
        self.next_seq = self.next_seq.wrapping_add(1);
        seq
    }

    // Push a terminal fallback entry: dedupes in-flight, evicts oldest. The bool
    // keeps sibling counters consistent with single-`FocusLost` bookkeeping.
    pub(crate) fn push_pending(
        &mut self,
        seq: u64,
        event: enzui_webkit::input::InputEvent,
    ) -> bool {
        if self.pending.iter().any(|p| p.event == event) {
            return false;
        }
        if self.pending.len() == PENDING_TERMINAL_CAP {
            self.pending.pop_front();
        }
        self.pending.push_back(PendingTerminal { seq, event });
        true
    }

    // Take pending terminals with `seq` below `exclusive_seq`, ascending: they
    // entered admission first, so they run before that ordinary.
    pub(crate) fn drain_before(&mut self, exclusive_seq: u64) -> Vec<PendingTerminal> {
        // Same-lock atomic seq plus same-section push: the fallback stays
        // seq-monotone, so a front prefix suffices.
        let mut out = Vec::new();
        while let Some(front) = self.pending.front() {
            if front.seq >= exclusive_seq {
                break;
            }
            out.push(self.pending.pop_front().expect("front just observed"));
        }
        out
    }

    // Take all remaining pending terminals, ascending: only safe when no
    // smaller-seq ordinary can still be in flight.
    pub(crate) fn drain_all(&mut self) -> Vec<PendingTerminal> {
        self.pending.drain(..).collect()
    }
}

// Worker payload tagged with its admission `seq`. Generic so production
// `Command` and test `InputEvent` drive the same arbitrator.
#[cfg_attr(not(feature = "wpe"), allow(dead_code))]
#[derive(Debug)]
pub(crate) struct SeqCmd<T> {
    pub(crate) seq: u64,
    pub(crate) cmd: T,
}

// One nonblocking arbitrator turn: ready work covered, `Idle` hands blocking
// to the caller — no path holds the admission lock across a blocking call.
#[cfg_attr(not(feature = "wpe"), allow(dead_code))]
#[derive(Debug)]
pub(crate) enum ArbitrationStep<T> {
    // Ordinary hot path: terminals in order, then `cmd` (`terminals` may be empty).
    Execute {
        terminals: Vec<PendingTerminal>,
        cmd: SeqCmd<T>,
    },
    // No ordinary pending and terminals have no unrun causal predecessor.
    Drain {
        terminals: Vec<PendingTerminal>,
    },
    // Nothing ready: block, then feed through `drain_before_cmd` so
    // blocked-while terminals still run first.
    Idle,
    // Disconnected: the caller must exit.
    Disconnected,
}

// One arbitration turn: no cross-queue inversion, no empty-race inversion
// (re-poll under the lock), nonblocking throughout.
#[cfg_attr(not(feature = "wpe"), allow(dead_code))]
pub(crate) fn arbitrate_step<T>(
    rx: &std::sync::mpsc::Receiver<SeqCmd<T>>,
    admission: &std::sync::Mutex<Admission>,
) -> ArbitrationStep<T> {
    // Fast path: peek without the mutex — the common ready case saves one
    // acquisition per turn.
    match rx.try_recv() {
        Ok(sc) => {
            let terminals = admission
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .drain_before(sc.seq);
            ArbitrationStep::Execute { terminals, cmd: sc }
        }
        Err(std::sync::mpsc::TryRecvError::Disconnected) => ArbitrationStep::Disconnected,
        Err(std::sync::mpsc::TryRecvError::Empty) => arbitrate_empty_branch(rx, admission),
    }
}

#[cfg_attr(not(feature = "wpe"), allow(dead_code))]
pub(crate) fn arbitrate_empty_branch<T>(
    rx: &std::sync::mpsc::Receiver<SeqCmd<T>>,
    admission: &std::sync::Mutex<Admission>,
) -> ArbitrationStep<T> {
    let mut adm = admission
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match rx.try_recv() {
        Ok(sc) => {
            // In-between submit: its `seq` is authoritative — drain only earlier
            // terminals.
            let terminals = adm.drain_before(sc.seq);
            ArbitrationStep::Execute { terminals, cmd: sc }
        }
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            let terminals = adm.drain_all();
            if terminals.is_empty() {
                ArbitrationStep::Disconnected
            } else {
                ArbitrationStep::Drain { terminals }
            }
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => {
            // Truly empty under the lock: no lower-seq ordinary can exist, so
            // `drain_all` cannot invert.
            let terminals = adm.drain_all();
            if terminals.is_empty() {
                ArbitrationStep::Idle
            } else {
                ArbitrationStep::Drain { terminals }
            }
        }
    }
}

// Drain terminals admitted before `cmd.seq`: blocked-while pushes with lower
// `seq` still run first.
#[cfg_attr(not(feature = "wpe"), allow(dead_code))]
pub(crate) fn drain_before_cmd<T>(
    cmd: &SeqCmd<T>,
    admission: &std::sync::Mutex<Admission>,
) -> Vec<PendingTerminal> {
    admission
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .drain_before(cmd.seq)
}

// What the shared admission helper did. Positive variants keep sibling
// bookkeeping (`blur_requests`) from desyncing with admission state.
#[cfg_attr(not(feature = "wpe"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalFallbackOutcome {
    // Ordinary channel took it; no fallback push.
    ChannelAccepted,
    // Channel full; fallback captured a fresh terminal.
    FallbackAdded,
    // Channel full and an identical terminal already queued (repeated `FocusLost`:
    // historical single-event bookkeeping).
    FallbackDuplicate,
    // The channel is disconnected. The caller must exit or drop.
    Disconnected,
}

#[cfg_attr(not(feature = "wpe"), allow(dead_code))]
pub(crate) fn admit_with_terminal_fallback<T>(
    admission: &std::sync::Mutex<Admission>,
    make_channel_cmd: impl FnOnce(u64) -> T,
    fallback_event: enzui_webkit::input::InputEvent,
    try_send: impl FnOnce(SeqCmd<T>) -> Result<(), std::sync::mpsc::TrySendError<SeqCmd<T>>>,
) -> TerminalFallbackOutcome {
    let mut adm = admission
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let seq = adm.assign_seq();
    let cmd = make_channel_cmd(seq);
    match try_send(SeqCmd { seq, cmd }) {
        Ok(()) => TerminalFallbackOutcome::ChannelAccepted,
        Err(std::sync::mpsc::TrySendError::Full(_)) => {
            if adm.push_pending(seq, fallback_event) {
                TerminalFallbackOutcome::FallbackAdded
            } else {
                TerminalFallbackOutcome::FallbackDuplicate
            }
        }
        Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
            TerminalFallbackOutcome::Disconnected
        }
    }
}

impl NativeUpdate {
    // Command answer, never ordinary traffic: single-query replies and clipboard
    // outcomes (refusals included) never regenerate.
    #[cfg_attr(not(feature = "wpe"), allow(dead_code))]
    pub(crate) fn is_command_answer(&self) -> bool {
        self.tab_reply.is_some()
            || self.clipboard.is_some()
            || self.selection.is_some()
            || !self.page_events.is_empty()
    }
}

#[cfg_attr(not(feature = "wpe"), allow(dead_code))]
pub(crate) fn push_bounded(
    queue: &mut std::collections::VecDeque<NativeUpdate>,
    update: NativeUpdate,
) {
    if queue.len() >= UPDATE_CAP {
        let remove = queue
            .iter()
            .position(|u| !u.is_command_answer())
            .unwrap_or(0);
        queue.remove(remove);
    }
    queue.push_back(update);
}

#[cfg(any(feature = "wpe", test))]
fn wait_for_page_completion(
    completed: &std::sync::atomic::AtomicU8,
    budget: std::time::Duration,
) -> u8 {
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    use enzui_core::monotonic_ms::{Policy, checked_instant_deadline};

    let deadline = checked_instant_deadline(Instant::now(), budget, Policy::Clamp);
    loop {
        let state = completed.load(Ordering::Acquire);
        if state != 0 {
            return state;
        }
        let now = Instant::now();
        if deadline.is_some_and(|d| now >= d) {
            return 0;
        }
        let sleep_for = deadline.map_or(Duration::from_millis(1), |d| d.duration_since(now));
        std::thread::sleep(Duration::from_millis(1).min(sleep_for));
    }
}

#[cfg(feature = "wpe")]
type NativeWakeTx = iced::futures::channel::mpsc::Sender<()>;
#[cfg(feature = "wpe")]
type NativeWakeRx = iced::futures::channel::mpsc::Receiver<()>;
#[cfg(feature = "wpe")]
static NATIVE_WAKE_TX: std::sync::OnceLock<std::sync::Mutex<NativeWakeTx>> =
    std::sync::OnceLock::new();
#[cfg(feature = "wpe")]
static NATIVE_WAKE_RX: std::sync::Mutex<Option<NativeWakeRx>> = std::sync::Mutex::new(None);

#[cfg(feature = "wpe")]
fn init_native_wake() {
    NATIVE_WAKE_TX.get_or_init(|| {
        // Capacity-one coalescing: one pending wake carries no more than the drain
        // already will.
        let (tx, rx) = iced::futures::channel::mpsc::channel(1);
        *NATIVE_WAKE_RX.lock().expect("native wake receiver") = Some(rx);
        std::sync::Mutex::new(tx)
    });
}

#[cfg(feature = "wpe")]
thread_local! {
    static OWNER_DISPATCH: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// A worker published during an owner dispatch and its wake was held back.
    static WAKE_HELD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Updates published by page workers and not yet taken by their windows.
#[cfg(feature = "wpe")]
static QUEUED_UPDATES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// After one window's dispatch: the dispatch stepped every window's pages, and
/// held back their wakes because the window takes its own updates next. What
/// is still queued then is another window's, which has to be woken.
#[cfg(feature = "wpe")]
fn wake_for_other_windows() {
    init_native_wake();
    if let Some(tx) = NATIVE_WAKE_TX.get() {
        wake_for_other_windows_with(tx);
    }
}

#[cfg(feature = "wpe")]
fn wake_for_other_windows_with(tx: &std::sync::Mutex<NativeWakeTx>) {
    if WAKE_HELD.with(|held| held.replace(false))
        && QUEUED_UPDATES.load(std::sync::atomic::Ordering::Acquire) > 0
    {
        send_native_wake(tx);
    }
}

#[cfg(feature = "wpe")]
fn wake_native_worker() {
    init_native_wake();
    if let Some(tx) = NATIVE_WAKE_TX.get() {
        send_native_wake(tx);
    }
}

#[cfg(feature = "wpe")]
fn send_native_wake(tx: &std::sync::Mutex<NativeWakeTx>) {
    if OWNER_DISPATCH.with(std::cell::Cell::get) {
        WAKE_HELD.with(|held| held.set(true));
        return;
    }
    let _ = tx
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .try_send(());
}

// A quiet, bounded subscription woken by the dedicated WPE thread. It polls
// neither frames nor GLib; one wake drains all coalesced worker notifications.
#[cfg(feature = "wpe")]
pub(crate) fn notification_subscription() -> iced::Subscription<()> {
    init_native_wake();
    iced::Subscription::run(|| {
        use iced::futures::StreamExt;
        let rx = NATIVE_WAKE_RX.lock().ok().and_then(|mut slot| slot.take());
        match rx {
            Some(rx) => rx.boxed(),
            None => iced::futures::stream::pending().boxed(),
        }
    })
}

#[cfg(not(feature = "wpe"))]
pub(crate) fn notification_subscription() -> iced::Subscription<()> {
    iced::Subscription::none()
}

#[cfg(feature = "wpe")]
mod real {
    use std::collections::VecDeque;
    use std::path::PathBuf;
    #[cfg(test)]
    use std::sync::atomic::AtomicBool;
    use std::sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool as StopBool, AtomicU8, AtomicUsize, Ordering},
        mpsc,
    };
    use std::time::Duration;

    use enzui_core::media::{Generation as MediaGeneration, Rgba8Frame};
    use enzui_webkit::clipboard_route::{ClipboardOutcome, HostGesture};
    use enzui_webkit::ffi::profile::ProfileStore;
    use enzui_webkit::ffi::runtime::{HeadlessDisplay, WebSession};
    use enzui_webkit::ime_bridge::ImeEffect;
    use enzui_webkit::input::{self, InputEvent, PointerButton, ViewportMapping};
    use enzui_webkit::web::focus::TabReply;
    use enzui_webkit::web::frame::FrameSeq;
    use enzui_webkit::web::{Generation as FocusGeneration, ViewId, ViewRouter, WebProfile};

    use crate::webview_scratch::SecureScratch;

    const COMMAND_CAP: usize = 32;
    #[cfg(feature = "benchmark")]
    fn trace_frame_event(kind: &str, identity: u64) {
        static ENABLED: OnceLock<bool> = OnceLock::new();
        static STARTED: OnceLock<std::time::Instant> = OnceLock::new();
        if *ENABLED.get_or_init(|| {
            std::env::var_os("ENZIM_BENCH_FRAME_TRACE").is_some_and(|value| value == "1")
        }) {
            eprintln!(
                "FRAME_TRACE {kind} {} {identity}",
                STARTED
                    .get_or_init(std::time::Instant::now)
                    .elapsed()
                    .as_micros()
            );
        }
    }

    #[cfg(feature = "benchmark")]
    struct TimedTrace(&'static str, std::time::Instant);
    #[cfg(feature = "benchmark")]
    impl Drop for TimedTrace {
        fn drop(&mut self) {
            let elapsed = self.1.elapsed().as_micros();
            if elapsed >= 8000 {
                trace_frame_event(self.0, u64::try_from(elapsed).unwrap_or(u64::MAX));
            }
        }
    }

    const INPUT_FRAME_INTERVAL: Duration = Duration::from_millis(8);
    // Page-drop join budget. Every worker command is itself bounded, so a live
    // worker exits well inside; overruns report, never block the UI thread.
    const TEARDOWN_BUDGET: Duration = Duration::from_millis(2_500);

    use super::{
        Admission, ArbitrationStep, NativeOpenError, NativeUpdate, NativeWorkerError, SeqCmd,
        TerminalFallbackOutcome, UPDATE_CAP, admit_with_terminal_fallback, arbitrate_step,
        drain_before_cmd, push_bounded,
    };

    // `SeqCmd<Command>`: the production instantiation of the unconditional seam.
    type PageSeqCmd = SeqCmd<Command>;

    enum Command {
        Browse {
            view: ViewId,
            action: super::BrowserAction,
        },
        Input {
            event: InputEvent,
        },
        Resize {
            view: ViewId,
            size: enzui_webkit::ViewportSize,
        },
        Click {
            view: ViewId,
            x: f32,
            y: f32,
        },
        Hover {
            view: ViewId,
            x: f32,
            y: f32,
        },
        Apply {
            view: ViewId,
            effect: ImeEffect,
        },
        // Copy the page's selection out to the host clipboard.
        CopyToHost {
            view: ViewId,
            gesture: HostGesture,
        },
        // Publish host text and paste it into the focused DOM field.
        PasteFromHost {
            view: ViewId,
            gesture: HostGesture,
            text: String,
        },
        Recreate {
            router: ViewRouter,
            view: ViewId,
            size: Option<enzui_webkit::ViewportSize>,
        },
        FocusOut {
            view: ViewId,
        },
        Visibility {
            view: ViewId,
            visible: bool,
        },
    }

    // No in-band `Shutdown` (a full queue discards exactly that): teardown uses
    // the out-of-band stop flag.

    // Process-long owner-free lane for thread-affine GLib: dispatch binds to the
    // first thread, so the lane survives pages while jobs still die on close.
    struct WpeLane {
        jobs: mpsc::SyncSender<Box<dyn FnOnce() + Send>>,
        busy: Arc<StopBool>,
        on_main: bool,
    }

    type MainJob = Box<dyn FnOnce() + Send>;
    static MAIN_JOBS: OnceLock<Mutex<mpsc::Receiver<MainJob>>> = OnceLock::new();
    std::thread_local! {
        static MAIN_TASK: std::cell::RefCell<Vec<MainTask>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    struct MainTask {
        opening: Option<OpeningWorkerLoop>,
        task: Option<WorkerLoop>,
        completed: Arc<AtomicU8>,
        busy: Arc<StopBool>,
        panicked: bool,
    }
    impl Drop for MainTask {
        fn drop(&mut self) {
            self.opening = None;
            self.task = None;
            self.busy.store(false, Ordering::Release);
            self.completed
                .store(if self.panicked { 2 } else { 1 }, Ordering::Release);
        }
    }

    pub(super) fn main_dispatch_needed() -> bool {
        // Native IPC and GPU releases can outlive the last page task.
        WPE_LANE
            .get()
            .and_then(|lane| lane.as_ref().ok())
            .is_some_and(|lane| lane.on_main)
            || enzui_webkit::ffi::presentation::pending_gpu_frame_completions() != 0
                && enzui_webkit::ffi::native_requires_process_main_thread()
    }

    pub(super) fn fallback_dispatch_needed() -> bool {
        WPE_LANE
            .get()
            .and_then(|lane| lane.as_ref().ok())
            .is_some_and(|lane| lane.busy.load(Ordering::Acquire))
            || MAIN_TASK.with(|tasks| !tasks.borrow().is_empty())
            || enzui_webkit::ffi::presentation::pending_gpu_frame_completions() != 0
            || RETIRE_UNTIL.with(|until| {
                until
                    .get()
                    .is_some_and(|until| until > std::time::Instant::now())
            })
    }

    std::thread_local! {
        static RETIRE_UNTIL: std::cell::Cell<Option<std::time::Instant>> = const { std::cell::Cell::new(None) };
    }

    static MAIN_WATCHER: OnceLock<Option<enzui_core::native_loop::NativeLoop>> = OnceLock::new();
    static WATCHER_FAILED: StopBool = StopBool::new(false);

    fn main_interest() -> Option<enzui_core::native_loop::Interest> {
        let native = enzui_webkit::ffi::native_event_interest().ok()?;
        let (input_due, import_due) = MAIN_TASK.with(|slot| {
            let tasks = slot.borrow();
            let now = std::time::Instant::now();
            let input = tasks
                .iter()
                .filter_map(|t| t.task.as_ref())
                .filter(|t| t.worker.session.discarded.is_none())
                .filter_map(|t| {
                    crate::native_interest::worker_deadline(
                        t.input_frame_due,
                        t.worker
                            .copy_poll_due
                            .into_iter()
                            .chain(t.worker.favicons.due)
                            .chain(
                                t.worker
                                    .download_controls
                                    .values()
                                    .map(|(_, start)| *start + Duration::from_secs(10)),
                            )
                            .chain(
                                t.worker
                                    .download_requests
                                    .iter()
                                    .map(|(_, _, start)| *start + Duration::from_secs(30)),
                            )
                            .min(),
                        t.worker.publication_pending(),
                        t.worker.gpu_pending(),
                        now,
                    )
                })
                .min();
            let import = tasks
                .iter()
                .filter_map(|t| t.task.as_ref())
                .filter(|t| t.worker.session.discarded.is_none())
                .filter_map(|t| {
                    crate::native_interest::gpu_import_deadline(
                        t.worker
                            .gpu_last
                            .as_ref()
                            .map(enzui_webview::gpu::GpuFrame::status),
                        t.worker.gpu_negotiating_until,
                        t.worker.visible,
                        now,
                    )
                })
                .min();
            let memory = (!tasks.is_empty()).then(memory_policy_deadline);
            (input.into_iter().chain(memory).min(), import)
        });
        Some(crate::native_interest::compose(
            native,
            input_due,
            import_due,
            std::time::Instant::now(),
        ))
    }

    fn arm_main_watcher() -> bool {
        let Some(Some(watcher)) = MAIN_WATCHER.get() else {
            return false;
        };
        let Some(interest) = main_interest() else {
            return false;
        };
        watcher.arm(interest);
        true
    }

    pub(super) fn native_subscription() -> Option<iced::Subscription<()>> {
        static ENABLED: OnceLock<bool> = OnceLock::new();
        if !*ENABLED.get_or_init(|| {
            std::env::var_os("ENZUI_PROBE_NATIVE_READINESS").is_none_or(|v| v != "0")
        }) || WATCHER_FAILED.load(Ordering::Acquire)
        {
            return None;
        }
        MAIN_WATCHER.get_or_init(|| enzui_core::native_loop::NativeLoop::new().ok());
        if !arm_main_watcher() {
            return None;
        }
        Some(MAIN_WATCHER.get()?.as_ref()?.subscription(|ready| {
            if ready == enzui_core::native_loop::NativeReady::Unavailable {
                WATCHER_FAILED.store(true, Ordering::Release);
            }
        }))
    }

    pub(super) fn dispatcher_available() -> bool {
        static ENABLED: OnceLock<bool> = OnceLock::new();
        *ENABLED.get_or_init(|| {
            std::env::var_os("ENZUI_PROBE_NATIVE_DISPATCH").is_none_or(|value| value != "0")
        }) && !WATCHER_FAILED.load(Ordering::Acquire)
            && MAIN_WATCHER.get().is_some_and(Option::is_some)
    }

    pub(super) fn dispatcher<'a, Message: 'a>(
        dispatch: impl Fn(bool) -> Option<Message> + 'a,
    ) -> Option<iced::Element<'a, Message>> {
        if !dispatcher_available() {
            return None;
        }
        let watcher = MAIN_WATCHER.get()?.as_ref()?;
        Some(
            watcher
                .dispatcher(move |ready| {
                    let unavailable = ready == enzui_core::native_loop::NativeReady::Unavailable;
                    if unavailable {
                        WATCHER_FAILED.store(true, Ordering::Release);
                    }
                    struct Reset(bool);
                    impl Drop for Reset {
                        fn drop(&mut self) {
                            super::OWNER_DISPATCH.with(|active| active.set(self.0));
                        }
                    }
                    let message = {
                        let _reset =
                            Reset(super::OWNER_DISPATCH.with(|active| active.replace(true)));
                        dispatch(unavailable)
                    };
                    super::wake_for_other_windows();
                    message
                })
                .into(),
        )
    }

    pub(super) fn main_dispatch_interval() -> Duration {
        Duration::from_millis(16)
    }

    #[cfg(feature = "benchmark")]
    fn memory_audit_requested() -> bool {
        static CONTROL: OnceLock<Option<PathBuf>> = OnceLock::new();
        std::thread_local! {
            static LAST: std::cell::Cell<(Option<std::time::Instant>, u64)> = const { std::cell::Cell::new((None, 0)) };
        }
        let Some(path) = CONTROL
            .get_or_init(|| std::env::var_os("ENZIM_BENCH_MEMORY_CONTROL").map(PathBuf::from))
        else {
            return false;
        };
        LAST.with(|last| {
            let now = std::time::Instant::now();
            let (due, previous) = last.get();
            if due.is_some_and(|due| now < due) {
                return false;
            }
            let next = std::fs::read_to_string(path)
                .ok()
                .and_then(|value| value.trim().parse::<u64>().ok())
                .unwrap_or(previous);
            last.set((Some(now + Duration::from_secs(1)), next));
            next != previous
        })
    }

    pub(super) fn pump_main_thread() {
        #[cfg(feature = "benchmark")]
        let _timed = TimedTrace("slow_pump", std::time::Instant::now());
        if !main_dispatch_needed() {
            return;
        }
        if enzui_webkit::ffi::dispatch_pending_native_events().is_err() {
            return;
        }
        if let Some(jobs) = MAIN_JOBS.get() {
            let job = jobs.lock().ok().and_then(|jobs| jobs.try_recv().ok());
            if let Some(job) = job {
                job();
            }
        }
        #[cfg(feature = "benchmark")]
        let audit = memory_audit_requested();
        MAIN_TASK.with(|slot| {
            let mut tasks = std::mem::take(&mut *slot.borrow_mut());
            maintain_tab_memory(&mut tasks);
            let mut live = Vec::new();
            for mut active in tasks {
                #[cfg(feature = "benchmark")]
                if audit && let Some(task) = active.task.as_mut() {
                    let worker = &mut task.worker;
                    if worker.session.discarded.is_none()
                        && (worker.visible || worker.session.is_playing_audio())
                    {
                        worker.trace_background_memory("audit-protected", None);
                    } else if worker.session.discarded.is_none() {
                        let released = worker.release_background_memory();
                        worker.trace_background_memory("audit-release", Some(released));
                    }
                }
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if let Some(opening) = active.opening.as_mut() {
                        match opening.poll() {
                            Ok(Some(worker)) => {
                                active.task = Some(worker);
                                active.opening = None;
                            }
                            Ok(None) => return true,
                            Err(error) => {
                                if let Some(error) = error {
                                    opening.report_error(error);
                                }
                                return false;
                            }
                        }
                    }
                    active
                        .task
                        .as_mut()
                        .is_some_and(|worker| worker.step(false))
                })) {
                    Ok(true) => live.push(active),
                    Ok(false) => {}
                    Err(_) => active.panicked = true,
                }
            }
            slot.borrow_mut().extend(live);
        });
        let _ = enzui_webkit::ffi::dispatch_pending_native_events();
        arm_main_watcher();
        #[cfg(feature = "benchmark")]
        if std::env::var_os("ENZIM_BENCH_LIFECYCLE").is_some() {
            std::thread_local! {
                static LAST: std::cell::Cell<Option<enzui_webkit::native_release::NativeReleases>> = const { std::cell::Cell::new(None) };
            }
            let released = enzui_webkit::native_release::released();
            LAST.with(|last| {
                if last.replace(Some(released)) != Some(released) {
                    eprintln!("NATIVE_RELEASES {released:?}");
                }
            });
        }
    }

    static WPE_LANE: OnceLock<Result<WpeLane, NativeOpenError>> = OnceLock::new();

    fn wpe_lane() -> Result<&'static WpeLane, NativeOpenError> {
        WPE_LANE
            .get_or_init(|| {
                let (jobs, rx) = mpsc::sync_channel::<Box<dyn FnOnce() + Send>>(1);
                let busy = Arc::new(StopBool::new(false));
                if enzui_webkit::ffi::native_requires_process_main_thread() {
                    let _ = MAIN_JOBS.set(Mutex::new(rx));
                    return Ok(WpeLane {
                        jobs,
                        busy,
                        on_main: true,
                    });
                }
                std::thread::Builder::new()
                    .name("enzui-wpe-lane".to_owned())
                    .spawn(move || {
                        loop {
                            let job =
                                if enzui_webkit::ffi::presentation::pending_gpu_frame_completions()
                                    == 0
                                {
                                    match rx.recv() {
                                        Ok(job) => Some(job),
                                        Err(_) => break,
                                    }
                                } else {
                                    match rx.recv_timeout(Duration::from_millis(8)) {
                                        Ok(job) => Some(job),
                                        Err(mpsc::RecvTimeoutError::Timeout) => None,
                                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                                    }
                                };
                            if let Some(job) = job {
                                job();
                            }
                            let _ = enzui_webkit::ffi::dispatch_pending_native_events();
                        }
                    })
                    .map_err(|error| {
                        NativeOpenError::DispatchUnavailable(format!(
                            "spawn persistent WPE dispatch lane: {error}"
                        ))
                    })?;
                Ok(WpeLane {
                    jobs,
                    busy,
                    on_main: false,
                })
            })
            .as_ref()
            .map_err(Clone::clone)
    }

    // Ordinary channel receiver plus shared stop flag: stop-flag policy lives
    // here, not in the arbitrator — blocking `recv` still exits on shutdown.
    struct PageReceiver {
        commands: mpsc::Receiver<PageSeqCmd>,
        stop: Arc<StopBool>,
    }

    impl PageReceiver {
        // Stop-flag check before the crank: shutdown short-circuits pre-mutex.
        fn stop_raised(&self) -> bool {
            self.stop.load(Ordering::Acquire)
        }

        // Post-shutdown backlog discard on every exit: saturated pre-stop queues
        // abandon, never execute after teardown.
        fn drain_after_stop(&self) {
            while self.commands.try_recv().is_ok() {}
        }

        // Idle-only blocking wait: `None` on stop or disconnect.
        fn wait(&self) -> Option<PageSeqCmd> {
            if self.stop_raised() {
                self.drain_after_stop();
                return None;
            }
            match self.commands.recv() {
                Ok(command) if !self.stop_raised() => Some(command),
                Ok(_) | Err(_) => {
                    self.drain_after_stop();
                    None
                }
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum PageTeardownOutcome {
        Completed,
        Panicked,
        TimedOut,
        AlreadyTornDown,
    }

    impl PageTeardownOutcome {
        fn completed(self) -> bool {
            matches!(self, Self::Completed | Self::Panicked)
        }
    }

    // Per-page endpoint torn down on close; body runs on the lane, so later pages
    // bootstrap on the same OS thread.
    struct PageWorker {
        commands: Option<mpsc::SyncSender<PageSeqCmd>>,
        stop: Arc<StopBool>,
        // 0 running/queued, 1 completed, 2 panicked — stored only after native
        // state and scratch lease drop.
        completed: Arc<AtomicU8>,
        budget: Duration,
        outcome: Option<PageTeardownOutcome>,
        on_main: bool,
    }

    impl PageWorker {
        fn dispatch(
            budget: Duration,
            body: impl FnOnce(PageReceiver) + Send + 'static,
        ) -> Result<Self, NativeOpenError> {
            Self::dispatch_job(budget, move |receiver, completed, busy| {
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| body(receiver)));
                busy.store(false, Ordering::Release);
                completed.store(if result.is_ok() { 1 } else { 2 }, Ordering::Release);
            })
        }

        fn dispatch_main(
            budget: Duration,
            factory: impl FnOnce(PageReceiver) -> OpeningWorkerLoop + Send + 'static,
        ) -> Result<Self, NativeOpenError> {
            Self::dispatch_job(budget, move |receiver, completed, busy| {
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| factory(receiver))) {
                    Ok(opening) => MAIN_TASK.with(|slot| {
                        slot.borrow_mut().push(MainTask {
                            opening: Some(opening),
                            task: None,
                            completed,
                            busy,
                            panicked: false,
                        });
                    }),
                    result => {
                        busy.store(false, Ordering::Release);
                        completed.store(if result.is_ok() { 1 } else { 2 }, Ordering::Release);
                    }
                }
            })
        }

        fn dispatch_job(
            budget: Duration,
            body: impl FnOnce(PageReceiver, Arc<AtomicU8>, Arc<StopBool>) + Send + 'static,
        ) -> Result<Self, NativeOpenError> {
            let lane = wpe_lane()?;
            if !lane.on_main {
                lane.busy
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                    .map_err(|_| NativeOpenError::PreviousPageStillReleasing)?;
            }
            if lane.on_main {
                lane.busy.store(true, Ordering::Release);
            }
            let lane_busy = Arc::clone(&lane.busy);
            let (commands, rx) = mpsc::sync_channel(COMMAND_CAP);
            let stop = Arc::new(StopBool::new(false));
            let worker_stop = Arc::clone(&stop);
            let completed = Arc::new(AtomicU8::new(0));
            let worker_completed = Arc::clone(&completed);
            let job = Box::new(move || {
                body(
                    PageReceiver {
                        commands: rx,
                        stop: worker_stop,
                    },
                    worker_completed,
                    lane_busy,
                );
            });
            if let Err(error) = lane.jobs.try_send(job) {
                lane.busy.store(false, Ordering::Release);
                return Err(match error {
                    mpsc::TrySendError::Full(_) => NativeOpenError::PreviousPageStillReleasing,
                    mpsc::TrySendError::Disconnected(_) => NativeOpenError::DispatchUnavailable(
                        "persistent WPE dispatch lane terminated".to_owned(),
                    ),
                });
            }
            if lane.on_main {
                super::wake_native_worker();
                enzui_webkit::ffi::wake_native_event_loop();
            }
            Ok(Self {
                commands: Some(commands),
                stop,
                completed,
                budget,
                outcome: None,
                on_main: lane.on_main,
            })
        }

        fn try_send(&self, command: PageSeqCmd) -> Result<(), mpsc::TrySendError<PageSeqCmd>> {
            let result = match &self.commands {
                Some(commands) => commands.try_send(command),
                None => Err(mpsc::TrySendError::Disconnected(command)),
            };
            if self.on_main {
                super::wake_native_worker();
                enzui_webkit::ffi::wake_native_event_loop();
            }
            result
        }

        fn shutdown(&mut self) -> PageTeardownOutcome {
            if self.outcome.is_some() {
                return PageTeardownOutcome::AlreadyTornDown;
            }
            self.stop.store(true, Ordering::Release);
            self.commands = None;
            if self.on_main {
                RETIRE_UNTIL.with(|until| {
                    until.set(Some(std::time::Instant::now() + Duration::from_secs(2)));
                });
                pump_main_thread();
                super::wake_native_worker();
            }
            let completed = super::wait_for_page_completion(&self.completed, self.budget);
            let outcome = match completed {
                1 => {
                    // Frame-completion sources can outlive the session and finish on this same lane.
                    PageTeardownOutcome::Completed
                }
                2 => PageTeardownOutcome::Panicked,
                _ => PageTeardownOutcome::TimedOut,
            };
            self.outcome = Some(outcome);
            outcome
        }
    }

    // UI-side handle: thread-affine objects live only in the page job.
    pub(crate) struct NativePage {
        // Queue-and-stop owner: drop signals, drains, and bounded-waits native
        // release; timed-out detached jobs keep their scratch lease, the lane stays.
        worker: PageWorker,
        updates: Arc<Mutex<VecDeque<NativeUpdate>>>,
        // Per-page admission and fallback: one lock assigns `seq` and pushes;
        // the worker interleaves off it — releases never precede presses.
        admission: Arc<Mutex<Admission>>,
        blur_requests: Arc<AtomicUsize>,
        #[cfg(test)]
        teardown_timed_out: Option<Arc<AtomicBool>>,
    }

    impl core::fmt::Debug for NativePage {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str("NativePage(bounded WPE worker)")
        }
    }

    impl NativePage {
        #[cfg(test)]
        pub(crate) fn with_pending_update(update: NativeUpdate) -> Self {
            Self {
                worker: PageWorker {
                    commands: None,
                    stop: Arc::new(StopBool::new(false)),
                    completed: Arc::new(AtomicU8::new(1)),
                    budget: Duration::ZERO,
                    outcome: None,
                    on_main: true,
                },
                updates: Arc::new(Mutex::new(VecDeque::from([update]))),
                admission: Arc::new(Mutex::new(Admission::default())),
                blur_requests: Arc::new(AtomicUsize::new(0)),
                teardown_timed_out: None,
            }
        }

        pub(crate) fn open_with_profile(
            router: &ViewRouter,
            view: ViewId,
            scratch: Arc<SecureScratch>,
            initial_size: Option<enzui_webkit::ViewportSize>,
            initial_generation: MediaGeneration,
            profile: enzui_webkit::web::WebProfile,
        ) -> Result<Self, NativeOpenError> {
            Self::open_with_teardown_budget(
                router,
                view,
                scratch,
                initial_size,
                initial_generation,
                TEARDOWN_BUDGET,
                profile,
            )
        }

        pub(crate) fn open_reserved_with_profile(
            router: &ViewRouter,
            view: ViewId,
            scratch: Arc<SecureScratch>,
            initial_size: Option<enzui_webkit::ViewportSize>,
            initial_generation: MediaGeneration,
            profile: enzui_webkit::web::WebProfile,
        ) -> Result<Self, NativeOpenError> {
            Self::open_profile_mode(
                router,
                view,
                scratch,
                initial_size,
                initial_generation,
                TEARDOWN_BUDGET,
                (profile, false, None),
            )
        }

        /// Opens a page around a window another page opened, which the
        /// opener's worker keeps under `popup`.
        pub(crate) fn open_adopted_with_profile(
            router: &ViewRouter,
            view: ViewId,
            scratch: Arc<SecureScratch>,
            (initial_size, initial_generation): (Option<enzui_webkit::ViewportSize>, MediaGeneration),
            profile: enzui_webkit::web::WebProfile,
            popup: u64,
        ) -> Result<Self, NativeOpenError> {
            Self::open_profile_mode(
                router,
                view,
                scratch,
                initial_size,
                initial_generation,
                TEARDOWN_BUDGET,
                (profile, true, Some(popup)),
            )
        }

        fn open_with_teardown_budget(
            router: &ViewRouter,
            view: ViewId,
            scratch: Arc<SecureScratch>,
            initial_size: Option<enzui_webkit::ViewportSize>,
            initial_generation: MediaGeneration,
            teardown_budget: Duration,
            profile: enzui_webkit::web::WebProfile,
        ) -> Result<Self, NativeOpenError> {
            Self::open_profile_mode(
                router,
                view,
                scratch,
                initial_size,
                initial_generation,
                teardown_budget,
                (profile, true, None),
            )
        }

        fn open_profile_mode(
            router: &ViewRouter,
            view: ViewId,
            scratch: Arc<SecureScratch>,
            initial_size: Option<enzui_webkit::ViewportSize>,
            initial_generation: MediaGeneration,
            teardown_budget: Duration,
            (profile, retain_profile, adopt): (enzui_webkit::web::WebProfile, bool, Option<u64>),
        ) -> Result<Self, NativeOpenError> {
            let updates = Arc::new(Mutex::new(VecDeque::with_capacity(UPDATE_CAP)));
            let worker_updates = Arc::clone(&updates);
            let admission = Arc::new(Mutex::new(Admission::default()));
            let worker_admission = Arc::clone(&admission);
            let router = router.clone();
            let scratch_path = scratch.path().to_owned();
            let create = move |rx| {
                OpeningWorkerLoop::new(
                    rx,
                    worker_updates,
                    worker_admission,
                    OpenPage {
                        adopt,
                        profile,
                        retain_profile,
                        router,
                        view,
                        scratch: scratch_path,
                        initial_size,
                        initial_generation,
                    },
                    Some(scratch),
                )
            };
            let worker = if enzui_webkit::ffi::native_requires_process_main_thread() {
                PageWorker::dispatch_main(teardown_budget, create)?
            } else {
                PageWorker::dispatch(teardown_budget, move |rx| {
                    if let Some(mut task) = create(rx).finish_synchronously() {
                        while task.step(true) {}
                    }
                })?
            };

            Ok(Self {
                worker,
                updates,
                admission,
                blur_requests: Arc::new(AtomicUsize::new(0)),
                #[cfg(test)]
                teardown_timed_out: None,
            })
        }

        // Real page under a capped teardown wait, plus the timeout witness: fully
        // constructed and released — the detached-still-gets-there claim measurable.

        // Every submission goes through here: seq assignment plus channel push in
        // one section keeps the cross-queue total order authoritative.
        pub(crate) fn set_visibility(&self, router: &ViewRouter, view: ViewId, visible: bool) {
            if router.accepts(view) {
                let _ = self.admit_cmd(Command::Visibility { view, visible });
            }
        }

        fn admit_cmd(&self, cmd: Command) -> Result<(), mpsc::TrySendError<PageSeqCmd>> {
            let mut admission = self
                .admission
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let seq = admission.assign_seq();
            self.worker.try_send(SeqCmd { seq, cmd })
        }

        pub(crate) fn recreate(
            self,
            router: &ViewRouter,
            view: ViewId,
            size: Option<enzui_webkit::ViewportSize>,
        ) -> Result<Self, String> {
            self.admit_cmd(Command::Recreate {
                router: router.clone(),
                view,
                size,
            })
            .map_err(|e| format!("queue recreate: {e}"))?;
            Ok(self)
        }

        // Queue a generation-tagged integer logical viewport for WPE.
        pub(crate) fn resize(
            &self,
            router: &ViewRouter,
            view: ViewId,
            size: enzui_webkit::ViewportSize,
        ) -> Result<(), String> {
            if !router.accepts(view) {
                return Err("view generation is no longer live".to_owned());
            }
            self.admit_cmd(Command::Resize { view, size })
                .map_err(|e| format!("queue viewport resize: {e}"))
        }

        pub(crate) fn browse(
            &self,
            router: &ViewRouter,
            view: ViewId,
            action: super::BrowserAction,
        ) -> Result<(), String> {
            if !router.accepts(view) {
                return Err("view generation is no longer live".into());
            }
            if let super::BrowserAction::MemoryProtected(value) = action {
                // Pressure decisions must see pin changes before draining page commands.
                self.admission
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .memory_protected = value;
                return Ok(());
            }
            self.admit_cmd(Command::Browse { view, action })
                .map_err(|error| format!("queue navigation: {error}"))
        }

        pub(crate) fn click(&self, _router: &ViewRouter, view: ViewId, x: f32, y: f32) {
            let _ = self.admit_cmd(Command::Click { view, x, y });
        }

        pub(crate) fn hover(&self, view: ViewId, x: f32, y: f32) {
            let _ = self.admit_cmd(Command::Hover { view, x, y });
        }

        pub(crate) fn forward_input(&self, router: &ViewRouter, event: &InputEvent) -> bool {
            if !router.accepts(event.view()) {
                return false;
            }
            #[cfg(feature = "benchmark")]
            if matches!(
                event,
                InputEvent::WheelScrolled { .. } | InputEvent::WheelScrolledAt { .. }
            ) {
                trace_frame_event("enqueue", 0);
            }
            #[cfg(feature = "benchmark")]
            if let InputEvent::PointerMoved { at, .. } = event {
                trace_frame_event("pointer_enqueue", u64::from(at.x.to_bits()));
            }
            let is_terminal = event.is_terminal();
            let cmd = Command::Input {
                event: event.clone(),
            };
            // Whole seq+send+fallback under one hold: monotone front, and
            // full-channel terminals precede later ordinaries.
            let mut admission = self
                .admission
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let seq = admission.assign_seq();
            match self.worker.try_send(SeqCmd { seq, cmd }) {
                Ok(()) => true,
                Err(mpsc::TrySendError::Full(seq_cmd)) if is_terminal => {
                    if let Command::Input { event, .. } = seq_cmd.cmd {
                        admission.push_pending(seq_cmd.seq, event);
                    }
                    true
                }
                Err(_) => false,
            }
        }

        pub(crate) fn focus_out(&self, view: ViewId) {
            let outcome = admit_with_terminal_fallback(
                &self.admission,
                |_seq| Command::FocusOut { view },
                InputEvent::FocusLost {
                    view,
                    modifiers: input::Modifiers::NONE,
                },
                |sc| self.worker.try_send(sc),
            );
            match outcome {
                TerminalFallbackOutcome::ChannelAccepted
                | TerminalFallbackOutcome::FallbackAdded => {
                    self.blur_requests.fetch_add(1, Ordering::Relaxed);
                }
                TerminalFallbackOutcome::FallbackDuplicate
                | TerminalFallbackOutcome::Disconnected => {}
            }
        }

        pub(crate) fn apply(
            &self,
            router: &ViewRouter,
            view: ViewId,
            effect: &ImeEffect,
        ) -> Result<(), String> {
            if !router.accepts(view) {
                return Err("view generation is no longer live".to_owned());
            }
            self.admit_cmd(Command::Apply {
                view,
                effect: effect.clone(),
            })
            .map_err(|e| format!("queue IME effect: {e}"))
        }

        pub(crate) fn copy_to_host(
            &self,
            router: &ViewRouter,
            view: ViewId,
            gesture: HostGesture,
        ) -> Result<(), String> {
            if !router.accepts(view) {
                return Err("view generation is no longer live".to_owned());
            }
            self.admit_cmd(Command::CopyToHost { view, gesture })
                .map_err(|e| format!("queue clipboard copy: {e}"))
        }

        // Publish `text` on the host clipboard and paste it into the
        // page's focused editable field.
        pub(crate) fn paste_from_host(
            &self,
            router: &ViewRouter,
            view: ViewId,
            gesture: HostGesture,
            text: &str,
        ) -> Result<(), String> {
            if !router.accepts(view) {
                return Err("view generation is no longer live".to_owned());
            }
            self.admit_cmd(Command::PasteFromHost {
                view,
                gesture,
                text: text.to_owned(),
            })
            .map_err(|e| format!("queue clipboard paste: {e}"))
        }

        // Bounded-notification drain: ordinary coalesces, Tab replies preserved.
        pub(crate) fn take_updates(&self) -> Vec<NativeUpdate> {
            let taken: Vec<_> = self
                .updates
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .drain(..)
                .collect();
            let _ = super::QUEUED_UPDATES.fetch_update(Ordering::AcqRel, Ordering::Acquire, |queued| {
                Some(queued.saturating_sub(taken.len()))
            });
            taken
        }
    }

    impl Drop for NativePage {
        fn drop(&mut self) {
            // What this page queued and nobody took leaves the count with it.
            drop(self.take_updates());
            // Signal, disconnect, drain, wait — all inside budget. The lane survives;
            // session, profiles, display release before `Completed`.
            let outcome = self.worker.shutdown();
            #[cfg(feature = "benchmark")]
            if std::env::var_os("ENZIM_BENCH_LIFECYCLE").is_some() {
                eprintln!(
                    "PAGE_RELEASE {outcome:?} {:?}",
                    enzui_webkit::native_release::released()
                );
            }
            #[cfg(test)]
            if let Some(timed_out) = &self.teardown_timed_out {
                timed_out.store(outcome == PageTeardownOutcome::TimedOut, Ordering::Release);
            }
            if !outcome.completed() {
                // Overrun reports, never waits longer: stop flag stays raised, the
                // detached job still exits alone.
                debug_assert_eq!(
                    outcome,
                    PageTeardownOutcome::TimedOut,
                    "incomplete native teardown must report its timeout"
                );
                eprintln!(
                    "enzui probe: WPE page job did not release within its teardown budget; \
                     detached under the stop flag ({outcome:?})"
                );
            }
        }
    }

    // Native session, profile and display owners, followed by transferable frame holds.
    type NativeOperation = Box<dyn FnMut(&mut Worker) -> bool>;
    include!("native_favicons.rs");

    include!("native_tab_memory.rs");

    struct Worker {
        // A sign-in was just sent. Over plain HTTP the connection the server
        // challenged on can be gone by then, and WebKit fails the load
        // rather than send the credentials it now holds on a new one.
        sign_in_sent: bool,
        // Hosts whose failed certificate the person chose to continue past.
        trusted_hosts: std::rc::Rc<std::cell::RefCell<std::collections::BTreeSet<String>>>,
        // Certificate failures of this view already reported.
        certificate_refusals: u64,
        credential_private: bool,
        defer_prepared_scripts: bool,
        credential_active: bool,
        favicons: NativeFavicons,
        privacy_document: Option<String>,
        pending_copy: Option<enzui_webkit::ffi::runtime::PendingPageCopy>,
        copy_poll_due: Option<std::time::Instant>,
        developer_tools: bool,
        resource_log: Option<enzui_webkit::ffi::runtime::resource_log::ResourceLog>,
        resource_notice: Option<String>,
        native_operations: Vec<NativeOperation>,
        filter_store: Option<std::rc::Rc<enzui_webkit::ffi::content_filters::ContentFilterStore>>,
        filter_cache: PathBuf,
        filter_installation: Option<enzui_webkit::ffi::content_filters::ContentFilterInstallation>,
        pdf_job: Option<(enzui_webkit::printing::PdfPrintId, crate::native_jobs::Job)>,
        jobs: std::collections::HashMap<enzui_webkit::page::PageRequestId, crate::native_jobs::Job>,
        active_downloads: std::collections::HashSet<enzui_webkit::page::PageRequestId>,
        download_controls: std::collections::HashMap<
            enzui_webkit::page::PageRequestId,
            (crate::native_jobs::Job, std::time::Instant),
        >,
        download_requests: Vec<(String, crate::native_jobs::Job, std::time::Instant)>,
        profile: WebProfile,
        privacy: crate::privacy::Privacy,
        default_user_agent: String,
        permission_choices: Vec<crate::site_permissions::Choice>,
        notifications_enabled: bool,
        filter_mode: std::rc::Rc<std::cell::Cell<Option<usize>>>,
        filter_generation: std::rc::Rc<std::cell::Cell<u64>>,
        session: RetainedSession,
        memory_id: u64,
        edited: bool,
        last_visible: std::time::Instant,
        active_filters: (usize, Vec<Arc<crate::filters::Source>>),
        session_size: Option<enzui_webkit::ViewportSize>,
        zoom: f64,
        preferred_dark: Option<bool>,
        preferred_system_font: Option<String>,
        pending_restore: bool,
        profiles: std::rc::Rc<std::cell::RefCell<ProfileStore>>,
        display: std::rc::Rc<HeadlessDisplay>,
        router: ViewRouter,
        view: ViewId,
        seq: u64,
        generation: MediaGeneration,
        gpu_enabled: bool,
        snapshot_ack: Option<u64>,
        gpu_last: Option<enzui_webview::gpu::GpuFrame>,
        gpu_formats_set: bool,
        gpu_negotiating_until: Option<std::time::Instant>,
        visible: bool,
        background_memory: crate::native_interest::BackgroundMemory,
        // Set once the garbage of the last load was collected in this hidden period.
        idle_collected: bool,
        last_activity: std::time::Instant,
        metadata_cache: Option<CachedMetadata>,
        cache_metadata: bool,
        metadata_builds: u64,
    }

    std::thread_local! {
        static PROFILE_LEASES:std::cell::RefCell<std::collections::BTreeMap<enzui_webkit::web::profile::ProfileId,std::rc::Rc<enzui_webkit::ffi::profile::NetworkSession>>>=Default::default();
    }
    pub(super) fn release_profiles(ids: Vec<enzui_webkit::web::profile::ProfileId>) {
        let Ok(lane) = wpe_lane() else {
            return;
        };
        if lane.on_main {
            PROFILE_LEASES.with(|leases| {
                let mut leases = leases.borrow_mut();
                for id in ids {
                    leases.remove(&id);
                }
            });
            return;
        }
        let job: Box<dyn FnOnce() + Send> = Box::new(move || {
            PROFILE_LEASES.with(|leases| {
                let mut leases = leases.borrow_mut();
                for id in ids {
                    leases.remove(&id);
                }
            });
        });
        if lane.jobs.try_send(job).is_ok() {
            super::wake_native_worker();
            enzui_webkit::ffi::wake_native_event_loop();
        }
    }

    /// How long a window a page opened is kept for the tab that takes it.
    const KEPT_POPUP: Duration = Duration::from_secs(30);

    std::thread_local! {
        /// Windows pages opened, taken from their openers and not yet tabs.
        static KEPT_POPUPS: std::cell::RefCell<Vec<(u64, std::time::Instant, WebSession)>> =
            const { std::cell::RefCell::new(Vec::new()) };
    }

    /// Keeps a window a page opened until a tab is made around it. One that
    /// no tab took in [`KEPT_POPUP`] is closed.
    fn keep_popup(id: u64, session: WebSession) {
        KEPT_POPUPS.with(|kept| {
            let mut kept = kept.borrow_mut();
            kept.retain(|(_, since, _)| since.elapsed() < KEPT_POPUP);
            kept.push((id, std::time::Instant::now(), session));
        });
    }

    fn take_kept_popup(id: u64) -> Option<WebSession> {
        KEPT_POPUPS.with(|kept| {
            let mut kept = kept.borrow_mut();
            kept.retain(|(_, since, _)| since.elapsed() < KEPT_POPUP);
            let at = kept.iter().position(|(kept, _, _)| *kept == id)?;
            Some(kept.remove(at).2)
        })
    }

    fn shared_display() -> Result<std::rc::Rc<HeadlessDisplay>, String> {
        std::thread_local! {
            static DISPLAY: std::cell::RefCell<std::rc::Weak<HeadlessDisplay>> = const { std::cell::RefCell::new(std::rc::Weak::new()) };
        }
        DISPLAY.with(|slot| {
            let current = slot.borrow().upgrade();
            if let Some(display) = current {
                return Ok(display);
            }
            let display = std::rc::Rc::new(
                HeadlessDisplay::new().map_err(|error| format!("headless display: {error}"))?,
            );
            *slot.borrow_mut() = std::rc::Rc::downgrade(&display);
            Ok(display)
        })
    }

    struct CachedMetadata {
        revision: enzui_webkit::ffi::runtime::PageMetadataRevision,
        value: Arc<super::NativeMetadata>,
        title_error: Option<enzui_webkit::ffi::runtime::PageTitleError>,
    }

    include!("native_resources.rs");
    include!("native_credentials.rs");
    include!("native_credentials_gate.rs");
    include!("native_passkeys_gate.rs");
    include!("native_scripts.rs");
    include!("native_timing.rs");

    impl Worker {
        fn refresh_document_scripts(&mut self, address: Option<&str>) {
            if self.defer_prepared_scripts {
                return;
            }
            let installed = Self::install_scripts_with_policy(
                &mut self.session,
                self.filter_mode.get().is_some_and(|mode| mode > 0),
                address,
                Some(&self.privacy),
                self.developer_tools,
                self.credential_private,
            );
            self.credential_active |= installed;
        }

        fn configure_permissions(
            &mut self,
            choices: Vec<crate::site_permissions::Choice>,
        ) -> Result<(), enzui_webkit::ffi::runtime::SessionError> {
            if choices != self.permission_choices {
                let devices_changed = self
                    .permission_choices
                    .iter()
                    .filter(|choice| choice.kind != "notifications")
                    .ne(choices
                        .iter()
                        .filter(|choice| choice.kind != "notifications"));
                if devices_changed {
                    self.session.reset_origin_permissions()?;
                } else {
                    self.session.reset_notification_permissions();
                }
                for choice in choices
                    .iter()
                    .filter(|choice| devices_changed || choice.kind == "notifications")
                {
                    if let Some(permission) = choice.native()
                        && !self
                            .session
                            .set_site_permission(permission, choice.allowed)?
                    {
                        return Err(
                            enzui_webkit::ffi::runtime::SessionError::OriginPermissionsUnsupported,
                        );
                    }
                }
                self.permission_choices = choices;
            }
            Ok(())
        }
        fn install_scripts(
            session: &mut WebSession,
            blocking: bool,
            address: Option<&str>,
            credential_private: bool,
        ) -> bool {
            Self::install_scripts_with_policy(
                session,
                blocking,
                address,
                None,
                false,
                credential_private,
            )
        }
        fn install_scripts_with_policy(
            session: &mut WebSession,
            blocking: bool,
            address: Option<&str>,
            privacy: Option<&crate::privacy::Privacy>,
            developer_tools: bool,
            credential_private: bool,
        ) -> bool {
            session.remove_user_scripts();
            #[cfg(feature = "benchmark")]
            Self::install_document_timing(session);
            let edit_world =
                enzui_webkit::page::ScriptWorld::Isolated("enzim-browser-chrome".into());
            let _ = session.register_message_handler("enzimTabEdits", &edit_world);
            let _ = session.register_message_handler("enzimExternalLink", &edit_world);
            let _ = session.register_message_handler("enzimDateField", &edit_world);
            for source in [
                include_str!("tab_edits.js"),
                include_str!("external_links.js"),
                include_str!("date_fields.js"),
            ] {
                let _ = session.add_user_script(&enzui_webkit::page::UserScript {
                    source: source.into(),
                    world: edit_world.clone(),
                    timing: enzui_webkit::page::ScriptTiming::DocumentStart,
                    frames: enzui_webkit::page::ScriptFrames::AllFrames,
                });
            }
            #[cfg(feature = "benchmark")]
            super::trace_navigation("policy-scripts-start");
            let scripts = Self::policy_scripts(blocking, address, privacy, developer_tools);
            #[cfg(feature = "benchmark")]
            super::trace_navigation("policy-scripts-prepared");
            #[cfg(feature = "benchmark")]
            let mut installation_trace = std::env::var_os("ENZIM_BENCH_SCRIPT_INSTALL_TRACE")
                .is_some_and(|value| value == "1")
                .then(Vec::new);
            for script in scripts {
                #[cfg(feature = "benchmark")]
                let started = installation_trace.as_ref().map(|_| std::time::Instant::now());
                let _ = script.install(session);
                #[cfg(feature = "benchmark")]
                if let (Some(trace), Some(started)) = (&mut installation_trace, started) {
                    trace.push((script.script.source.len(), started.elapsed().as_micros()));
                }
            }
            #[cfg(feature = "benchmark")]
            super::trace_navigation("policy-scripts-installed");
            #[cfg(feature = "benchmark")]
            if let Some(trace) = installation_trace {
                eprintln!("SCRIPT_INSTALL_TRACE bytes_and_microseconds={trace:?}");
            }
            if let Ok(settings) =
                crate::credentials::config::load(&crate::credentials::config::path())
                && settings.enabled
                && settings.setup_provider.is_none()
                && settings.provider == "enzim.builtin"
                && (!credential_private || settings.private_fill)
            {
                if !session.register_secret_message_handler(
                    crate::credential_bridge::HANDLER,
                    crate::credential_bridge::WORLD,
                ) {
                    return false;
                }
                let source = format!(
                    "{};globalThis.__enzimCredentials?.configure({});undefined",
                    crate::credential_bridge::SOURCE,
                    settings.offer_save && !credential_private
                );
                let _ = session.add_user_script(&enzui_webkit::page::UserScript {
                    source,
                    world: enzui_webkit::page::ScriptWorld::Isolated(
                        crate::credential_bridge::WORLD.into(),
                    ),
                    timing: enzui_webkit::page::ScriptTiming::DocumentEnd,
                    frames: enzui_webkit::page::ScriptFrames::TopFrame,
                });
                // Pages get the Web Authentication API only while a password
                // manager can answer it, and only on a runtime that reports
                // the calling frame's origin.
                use crate::credentials::passkey;
                if session.register_frame_secret_message_handler(passkey::HANDLER, passkey::WORLD)
                {
                    for (source, world) in [
                        (
                            passkey::BRIDGE_SOURCE,
                            enzui_webkit::page::ScriptWorld::Isolated(passkey::WORLD.into()),
                        ),
                        (passkey::PAGE_SOURCE, enzui_webkit::page::ScriptWorld::Page),
                    ] {
                        let _ = session.add_user_script(&enzui_webkit::page::UserScript {
                            source: source.into(),
                            world,
                            timing: enzui_webkit::page::ScriptTiming::DocumentStart,
                            frames: enzui_webkit::page::ScriptFrames::AllFrames,
                        });
                    }
                }
                return true;
            }
            false
        }
        fn policy_scripts(
            blocking: bool,
            address: Option<&str>,
            privacy: Option<&crate::privacy::Privacy>,
            developer_tools: bool,
        ) -> Vec<DocumentScript> {
            let guard = privacy
                .map(crate::privacy::Privacy::ad_script_guard)
                .unwrap_or_default();
            use enzui_webkit::page::ScriptWorld;
            let chrome = ScriptWorld::Isolated("enzim-browser-chrome".into());
            let mut scripts = vec![DocumentScript::new(
                include_str!("viewport_background.js"),
                chrome.clone(),
            )];
            if developer_tools {
                scripts.push(DocumentScript::new(
                    include_str!("console.js"),
                    ScriptWorld::Page,
                ));
            }
            if blocking {
                scripts.push(DocumentScript::new(
                    format!("(()=>{{{guard}{} }})();", include_str!("cosmetics.js")),
                    chrome,
                ));
                let mut addresses = vec![
                    "https://www.youtube.com/".to_owned(),
                    "https://m.youtube.com/".into(),
                    "https://music.youtube.com/".into(),
                    "https://www.youtube-nocookie.com/".into(),
                ];
                if let Some(address) = address {
                    addresses.push(address.to_owned());
                }
                let mut hosts = std::collections::HashSet::new();
                for address in addresses {
                    let Ok(address) = url::Url::parse(&address) else {
                        continue;
                    };
                    let Some(host) = address.host_str() else {
                        continue;
                    };
                    if !hosts.insert(host.to_owned()) {
                        continue;
                    }
                    let scriptlets = crate::filters::scriptlets(address.as_str());
                    if !scriptlets.is_empty() {
                        // Scripts selected for one host must not execute after a cross-host redirect.
                        let encoded_host = serde_json::to_string(host).unwrap_or_default();
                        scripts.push(DocumentScript::for_host(
                            format!("(()=>{{if(location.hostname!=={encoded_host})return;{guard}const scriptletGlobals=new Map();{scriptlets}}})()"), host,
                        ));
                    }
                }
            }
            scripts
        }
        fn cosmetic_message(&mut self, json: &str) {
            #[derive(serde::Deserialize)]
            struct Survey {
                url: String,
                classes: Vec<String>,
                ids: Vec<String>,
            }
            if self.filter_mode.get().is_none_or(|mode| mode == 0) || json.len() > 300_000 {
                return;
            }
            let Ok(survey) = serde_json::from_str::<Survey>(json) else {
                return;
            };
            if !self.privacy.value_for("ads", &survey.url) {
                return;
            }
            let (metadata, _) = self.metadata_snapshot();
            if metadata.location.as_deref() != Some(&survey.url)
                || survey.classes.len() > 1024
                || survey.ids.len() > 1024
                || survey
                    .classes
                    .iter()
                    .chain(&survey.ids)
                    .any(|name| name.len() > 512)
            {
                return;
            }
            let rules = crate::filters::cosmetic_rules(&survey.url, &survey.classes, &survey.ids);
            if let Ok(args) = serde_json::to_string(&(survey.url, rules)) {
                let source = format!("globalThis.__enzimApplyCosmetics?.(...{args});undefined");
                let _ = self.session.evaluate_script(
                    &source,
                    &enzui_webkit::page::ScriptWorld::Isolated("enzim-browser-chrome".into()),
                );
            }
        }
        fn evaluate_job(
            &mut self,
            source: &str,
            world: &enzui_webkit::page::ScriptWorld,
            job: crate::native_jobs::Job,
            preview: bool,
        ) {
            self.jobs.retain(|_, job| !job.closed());
            if self.jobs.len() >= 16 {
                job.complete(Err(
                    "Too many page operations are pending. Try again shortly.".into(),
                ));
            } else if !job.closed() {
                let result = if preview
                    && source
                        .trim_start()
                        .strip_prefix("await")
                        .is_some_and(|rest| rest.starts_with(char::is_whitespace))
                {
                    self.session
                        .evaluate_async_function_preview(&format!("return {source}"), world)
                } else if preview {
                    self.session.evaluate_script_preview(source, world)
                } else {
                    self.session.evaluate_script(source, world)
                };
                match result {
                    Ok(id) => {
                        self.jobs.insert(id, job);
                    }
                    Err(error) => job.complete(Err(error.to_string())),
                }
            }
        }
        #[cfg(feature = "benchmark")]
        fn start_filter(
            &mut self,
            mode: usize,
            json: Arc<crate::filters::Source>,
            job: crate::native_jobs::Job,
        ) {
            self.start_filters(mode, vec![json], job);
        }
        fn configure_filters(
            &mut self,
            mode: usize,
            filters: Vec<Arc<crate::filters::Source>>,
            job: crate::native_jobs::Job,
        ) {
            #[cfg(feature = "benchmark")]
            super::trace_navigation("filters-dispatched");
            self.active_filters = (mode, filters.clone());
            self.filter_generation
                .set(self.filter_generation.get().wrapping_add(1));
            if mode == 0 {
                self.filter_installation.take();
                self.filter_mode.set(Some(0));
                self.refresh_document_scripts(None);
                job.complete(Ok(crate::native_jobs::Payload::Json(
                    serde_json::Value::Null,
                )));
            } else if self.filter_mode.get() == Some(mode)
                && self.filter_installation.as_ref().is_some_and(|slot| {
                    filters
                        .iter()
                        .map(crate::native_filter_identity::identifier)
                        .collect::<Result<Vec<_>, _>>()
                        .is_ok_and(|ids| slot.identifiers() == ids.as_slice())
                })
            {
                job.complete(Ok(crate::native_jobs::Payload::Json(
                    serde_json::Value::Null,
                )));
            } else {
                self.start_filters(mode, filters, job);
            }
        }
        fn start_filters(
            &mut self,
            mode: usize,
            filters: Vec<Arc<crate::filters::Source>>,
            job: crate::native_jobs::Job,
        ) {
            use enzui_webkit::ffi::content_filters::ContentFilterStore;
            if filters.is_empty() || filters.len() > 8 {
                job.complete(Err(
                    "Protection requires one to eight bounded filter lists".into()
                ));
                return;
            }
            let setup = (|| {
                if self.filter_store.is_none() {
                    let cache = self.filter_cache.clone();
                    std::fs::create_dir_all(cache.parent().ok_or("No filter cache parent")?)
                        .map_err(|e| e.to_string())?;
                    self.filter_store = Some(std::rc::Rc::new(
                        ContentFilterStore::new(cache).map_err(|e| e.to_string())?,
                    ));
                }
                let store =
                    std::rc::Rc::clone(self.filter_store.as_ref().ok_or("No native filter store")?);
                let ids = filters
                    .iter()
                    .map(|json| {
                        crate::native_filter_identity::identifier(json).map_err(|e| e.to_string())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok::<_, String>((store, ids))
            })();
            let (store, ids) = match setup {
                Ok(value) => value,
                Err(error) => {
                    job.complete(Err(error));
                    return;
                }
            };
            let generation = self.filter_generation.get();
            let deadline = std::time::Instant::now() + Duration::from_secs(30);
            let mut index = 0;
            let mut compiled = Vec::new();
            let mut compiling = false;
            let mut task: Option<
                enzui_webkit::ffi::native_task::NativeTask<
                    enzui_webkit::ffi::content_filters::CompiledContentFilter,
                >,
            > = None;
            self.native_operations.push(Box::new(move |worker| {
                if job.closed()
                    || generation != worker.filter_generation.get()
                    || std::time::Instant::now() >= deadline
                {
                    if let Some(task) = &task {
                        task.cancel();
                    }
                    job.complete(Err(
                        "The protection request was superseded or timed out".into()
                    ));
                    return true;
                }
                if task.is_none() {
                    if !compiling {
                        let _ = crate::native_filter_cache::seed(
                            store.cache_path(),
                            ids[index].as_str(),
                            &filters[index],
                        );
                    }
                    let started = if compiling {
                        store.compile(&filters[index], Duration::from_secs(15))
                    } else {
                        store.load(&ids[index], Duration::from_secs(15))
                    };
                    match started {
                        Ok(started) => task = Some(started),
                        Err(enzui_webkit::ffi::native_task::NativeTaskError::Busy) => return false,
                        Err(error) => {
                            job.complete(Err(error.to_string()));
                            return true;
                        }
                    }
                }
                let Some(result) = task
                    .as_ref()
                    .and_then(enzui_webkit::ffi::native_task::NativeTask::take_result)
                else {
                    return false;
                };
                match result {
                    Ok(filter) => {
                        compiled.push(filter);
                        index += 1;
                        task = None;
                        compiling = false;
                        if index < filters.len() {
                            return false;
                        }
                        let borrowed: Vec<_> = compiled.iter().collect();
                        let result = if let Some(slot) = &mut worker.filter_installation {
                            if slot.identifiers().is_empty() {
                                worker.filter_installation.take();
                                worker
                                    .session
                                    .install_content_filters(&borrowed)
                                    .map(|slot| worker.filter_installation = Some(slot))
                            } else {
                                slot.replace_all(&borrowed)
                            }
                        } else {
                            worker
                                .session
                                .install_content_filters(&borrowed)
                                .map(|slot| worker.filter_installation = Some(slot))
                        };
                        if result.is_ok() {
                            worker.filter_mode.set(Some(mode));
                            let address = worker.metadata_snapshot().0.location.clone();
                            worker.refresh_document_scripts(address.as_deref());
                        }
                        job.complete(
                            result
                                .map(|()| {
                                    crate::native_jobs::Payload::Json(serde_json::Value::Null)
                                })
                                .map_err(|e| e.to_string()),
                        );
                        true
                    }
                    Err(_) if !compiling => {
                        compiling = true;
                        task = None;
                        false
                    }
                    Err(error) => {
                        job.complete(Err(error.to_string()));
                        true
                    }
                }
            }));
        }
    }

    impl Worker {
        fn metadata_pending(&self) -> bool {
            self.metadata_cache.as_ref().is_none_or(|cached| {
                cached.revision != self.session.page_metadata_revision()
                    || cached.value.can_go_back != self.session.can_go_back()
                    || cached.value.can_go_forward != self.session.can_go_forward()
            })
        }

        fn publication_pending(&self) -> bool {
            if self.session.discarded.is_some() {
                return false;
            }
            self.copy_poll_due
                .is_some_and(|due| std::time::Instant::now() >= due)
                || self.favicons.pending()
                || !self.native_operations.is_empty()
                || self
                    .resource_log
                    .as_ref()
                    .is_some_and(enzui_webkit::ffi::runtime::resource_log::ResourceLog::has_events)
                || self.gpu_pending()
                || crate::native_interest::snapshot_pending(
                    self.visible,
                    self.gpu_enabled,
                    self.snapshot_ack,
                    self.session.acknowledged_frames(),
                )
                || self.metadata_pending()
                || self.session.has_page_events()
        }

        fn metadata_snapshot(&mut self) -> (Arc<super::NativeMetadata>, Option<NativeWorkerError>) {
            let revision = self.session.page_metadata_revision();
            if self.cache_metadata
                && let Some(cached) = self.metadata_cache.as_ref()
                && cached.revision == revision
                && cached.value.can_go_back == self.session.can_go_back()
                && cached.value.can_go_forward == self.session.can_go_forward()
            {
                return (
                    Arc::clone(&cached.value),
                    cached.title_error.map(Into::into),
                );
            }
            let bootstrap = self.session.is_private_bootstrap_document();
            let (dom_report, title_error) = if bootstrap {
                (None, None)
            } else {
                match self.session.page_title() {
                    Ok(title) => (title, None),
                    Err(error) => (None, Some(error)),
                }
            };
            let value = Arc::new(super::NativeMetadata {
                loading: self.session.is_loading(),
                can_go_back: self.session.can_go_back(),
                can_go_forward: self.session.can_go_forward(),
                tls: self.session.connection_tls_info().map(|info| {
                    format!(
                        "{}\nSubject: {}\nIssuer: {}",
                        if self.trusts_location() {
                            crate::webview_native::CERTIFICATE_OVERRIDDEN.into()
                        } else if info.errors == 0 {
                            "WebKit reported no certificate errors".into()
                        } else {
                            format!("Certificate errors: {}", info.errors)
                        },
                        info.subject.unwrap_or_else(|| "Unavailable".into()),
                        info.issuer.unwrap_or_else(|| "Unavailable".into())
                    )
                }),
                dom_report,
                location: if bootstrap {
                    None
                } else {
                    self.session.current_uri().ok().flatten()
                },
                cursor: self.session.requested_cursor(),
            });
            self.metadata_builds += 1;
            self.metadata_cache = Some(CachedMetadata {
                revision,
                value: Arc::clone(&value),
                title_error,
            });
            (value, title_error.map(Into::into))
        }

        fn configure_process() {
            static GPU_COMPOSITING: OnceLock<()> = OnceLock::new();
            GPU_COMPOSITING.get_or_init(|| {
                // The desktop's portals name the browser by its desktop file,
                // as in the screen-sharing picker.
                enzui_webkit::ffi::runtime::set_application_id("org.enzim.Browser");
                enzui_webkit::ffi::runtime::set_gpu_process_for_compositing(true);
                // A navigation that leaves its process does not wait for a launch.
                if let Err(error) = enzui_webkit::ffi::runtime::set_spare_web_process(true) {
                    eprintln!("spare web process unavailable: {error}");
                }
                if let Err(error) = enzui_webkit::ffi::runtime::set_process_memory_budgets(
                    2 * 1024 * 1024 * 1024,
                    3 * 1024 * 1024 * 1024,
                ) {
                    eprintln!("native memory budgets unavailable: {error}");
                }
            });
        }

        fn new_session(
            display: &HeadlessDisplay,
            profiles: &mut ProfileStore,
            profile: &enzui_webkit::web::WebProfile,
        ) -> Result<WebSession, String> {
            Self::configure_process();
            #[cfg(feature = "benchmark")]
            if std::env::var_os("ENZUI_PROBE_TRUSTED_LOCAL").is_some() {
                return WebSession::new_profiled_trusted_local(
                    display,
                    profiles,
                    &WebProfile::ephemeral(),
                )
                .map_err(|error| format!("trusted local benchmark session: {error}"));
            }
            let session = WebSession::new_profiled(display, profiles, profile)
                .map_err(|error| format!("new_profiled: {error}"))?;
            Ok(session)
        }
        fn retain_profile_session(&self) -> Result<(), enzui_webkit::ffi::runtime::SessionError> {
            let lease = self
                .profiles
                .borrow_mut()
                .session_for(&self.profile)
                .map_err(|_| {
                    enzui_webkit::ffi::runtime::SessionError::BootstrapFailed(
                        "could not retain adopted page profile",
                    )
                })?;
            PROFILE_LEASES.with(|leases| {
                leases
                    .borrow_mut()
                    .entry(self.profile.id())
                    .or_insert(lease);
            });
            Ok(())
        }
        fn settle(&self, millis: u64) {
            if enzui_webkit::ffi::native_requires_process_main_thread() {
                let _ = enzui_webkit::ffi::dispatch_pending_native_events();
            } else {
                self.session.pump(millis);
            }
        }

        fn discard_gpu_frame(&mut self) {
            if let Some(frame) = self.gpu_last.take() {
                frame.discard();
            }
        }

        fn release_background_memory(&mut self) -> bool {
            if self.visible || self.session.release_memory().is_err() {
                return false;
            }
            // Native release invalidates hidden tiles and repaints them when shown.
            self.discard_gpu_frame();
            true
        }

        fn set_visibility(&mut self, view: ViewId, visible: bool) -> NativeUpdate {
            if self
                .session
                .set_view_visibility(&self.router, view, visible)
            {
                self.visible = visible;
                if visible {
                    self.last_visible = std::time::Instant::now();
                }
                if !visible {
                    self.discard_gpu_frame();
                }
                self.background_memory
                    .visibility(visible, std::time::Instant::now());
                self.trace_background_memory(if visible { "shown" } else { "hidden" }, None);
                if visible && self.gpu_negotiating_until.is_some() {
                    self.gpu_negotiating_until =
                        Some(std::time::Instant::now() + Duration::from_secs(2));
                }
            }
            let mut update = self.update(None, visible);
            if visible && update.gpu_frame.is_none() {
                // An unchanged, untrimmed page may not repaint when mapped again.
                update.gpu_frame.clone_from(&self.gpu_last);
            }
            update
        }

        fn maintain_background_memory(&mut self, now: std::time::Instant) {
            if self
                .background_memory
                .deadline()
                .is_none_or(|due| now < due)
            {
                return;
            }
            if self.background_memory.take_due(
                now,
                self.session.is_playing_audio() || self.session.is_loading(),
            ) {
                let released = self.release_background_memory();
                self.trace_background_memory("release", Some(released));
            } else {
                self.trace_background_memory("deferred", None);
            }
        }

        fn trace_background_memory(&self, event: &str, released: Option<bool>) {
            static STARTED: OnceLock<std::time::Instant> = OnceLock::new();
            if !memory_tracing_enabled() {
                return;
            }
            let host = self
                .session
                .current_uri()
                .ok()
                .flatten()
                .and_then(|uri| url::Url::parse(&uri).ok())
                .and_then(|uri| uri.host_str().map(str::to_owned));
            eprintln!(
                "MEMORY_TRACE {}",
                serde_json::json!({
                    "event": event,
                    "ms": STARTED.get_or_init(std::time::Instant::now).elapsed().as_millis(),
                    "view": format!("{:?}", self.view),
                    "renderer_pid": self.session.renderer_process_id().ok().flatten(),
                    "host": host,
                    "visible": self.visible,
                    "loading": self.session.is_loading(),
                    "audio": self.session.is_playing_audio(),
                    "gpu_frame_held": self.gpu_last.is_some(),
                    "released": released,
                })
            );
        }

        fn gpu_pending(&self) -> bool {
            self.gpu_enabled
                && self.visible
                && (self.gpu_negotiation_expired()
                    || self.session.gpu_frame_pending()
                    || self.gpu_last.as_ref().is_some_and(|frame| {
                        matches!(frame.status(), enzui_webview::gpu::GpuStatus::Refused(_))
                    }))
        }

        fn gpu_negotiation_expired(&self) -> bool {
            self.gpu_negotiating_until
                .is_some_and(|deadline| std::time::Instant::now() >= deadline)
                && !self
                    .gpu_last
                    .as_ref()
                    .is_some_and(|frame| crate::native_interest::import_pending(&frame.status()))
        }

        /// Whether the page on show is on a host the person continued to
        /// past a failed certificate.
        fn trusts_location(&self) -> bool {
            let trusted = self.trusted_hosts.borrow();
            !trusted.is_empty()
                && self
                    .session
                    .current_uri()
                    .ok()
                    .flatten()
                    .and_then(|uri| url::Url::parse(&uri).ok())
                    .and_then(|uri| uri.host_str().map(str::to_ascii_lowercase))
                    .is_some_and(|host| {
                        let host = host.trim_matches(['[', ']']).trim_end_matches('.');
                        trusted.contains(host)
                    })
        }

        fn compositor_frame(&mut self) -> Option<enzui_webview::gpu::GpuFrame> {
            if !self.gpu_enabled || !self.visible {
                return None;
            }
            if self
                .gpu_last
                .as_ref()
                .is_some_and(|frame| frame.status() == enzui_webview::gpu::GpuStatus::Imported)
            {
                self.gpu_negotiating_until = None;
            }
            let refused = self.gpu_last.as_ref().is_some_and(|frame| {
                matches!(frame.status(), enzui_webview::gpu::GpuStatus::Refused(_))
            });
            if refused
                && !self.gpu_formats_set
                && let Some(Some(support)) = enzui_webview::gpu::renderer_support()
                && !support.formats.is_empty()
                && self
                    .session
                    .set_gpu_buffer_formats(&self.router, self.view, &support.formats)
            {
                self.gpu_formats_set = true;
                self.gpu_negotiating_until =
                    Some(std::time::Instant::now() + Duration::from_secs(2));
                self.discard_gpu_frame();
                return None;
            }
            // Buffers allocated before the preference notification may still arrive.
            if refused
                && self
                    .gpu_negotiating_until
                    .is_some_and(|deadline| std::time::Instant::now() < deadline)
            {
                self.discard_gpu_frame();
                return None;
            }
            let expired = self.gpu_negotiation_expired();
            if !refused && !expired {
                match self.session.take_shared_gpu_frame(&self.router, self.view) {
                    Ok(Some(frame)) => {
                        #[cfg(feature = "benchmark")]
                        trace_frame_event("frame", frame.content_info().serial());
                        self.last_activity = std::time::Instant::now();
                        let allocation = frame.buffer_id();
                        let frame =
                            enzui_webview::gpu::GpuFrame::from_shared_native(frame, allocation);
                        if let Some(previous) = self.gpu_last.replace(frame.clone()) {
                            previous.discard();
                        }
                        return Some(frame);
                    }
                    Ok(None) => return None,
                    Err(error) => {
                        if cfg!(feature = "benchmark") {
                            eprintln!("GPU_FRAME_REJECTED {error:?}");
                        }
                    }
                }
            }
            if cfg!(feature = "benchmark") {
                eprintln!(
                    "GPU_FALLBACK refused={refused} expired={expired} formats={} pending={} held={:?}",
                    self.gpu_formats_set,
                    self.session.gpu_frame_pending(),
                    self.gpu_last
                        .as_ref()
                        .map(enzui_webview::gpu::GpuFrame::status),
                );
            }
            self.gpu_enabled = false;
            self.gpu_negotiating_until = None;
            self.discard_gpu_frame();
            self.session.disable_gpu_presentation();
            None
        }
        fn open(
            router: ViewRouter,
            view: ViewId,
            scratch: PathBuf,
            initial_size: Option<enzui_webkit::ViewportSize>,
            initial_generation: MediaGeneration,
            profile: enzui_webkit::web::WebProfile,
        ) -> Result<Self, String> {
            let resources = opening_resources()?;
            let session = Self::new_session(
                &resources.display,
                &mut resources.profiles.borrow_mut(),
                &profile,
            )?;
            Self::finish_open(
                OpenPage {
                    adopt: None,
                    router,
                    view,
                    scratch,
                    initial_size,
                    initial_generation,
                    profile,
                    retain_profile: true,
                },
                resources,
                session,
            )
        }

        fn finish_open(
            page: OpenPage,
            resources: OpeningResources,
            mut session: WebSession,
        ) -> Result<Self, String> {
            let OpenPage {
                adopt: _,
                router,
                view,
                scratch,
                initial_size,
                initial_generation,
                profile,
                retain_profile,
            } = page;
            let OpeningResources { display, profiles } = resources;
            let filter_cache = if profile.kind().is_ephemeral() {
                scratch.join("content-filters")
            } else {
                crate::browser_profile::paths()
                    .cache
                    .join("content-filters")
            };
            let defer_prepared_scripts = !retain_profile
                && PROFILE_LEASES.with(|leases| !leases.borrow().contains_key(&profile.id()));
            if retain_profile {
                let lease = profiles
                    .borrow_mut()
                    .session_for(&profile)
                    .map_err(|e| e.to_string())?;
                PROFILE_LEASES.with(|leases| {
                    leases.borrow_mut().entry(profile.id()).or_insert(lease);
                });
            }
            #[cfg(feature = "benchmark")]
            super::trace_navigation("session-created");
            if !retain_profile
                && std::env::var_os("ENZIM_PREPARE_COSMETICS").is_some_and(|value| value == "1")
            {
                #[cfg(feature = "benchmark")]
                super::trace_navigation("cosmetics-prepare-start");
                crate::filters::prepare_cosmetic_engine();
                #[cfg(feature = "benchmark")]
                super::trace_navigation("cosmetics-prepare-end");
            }
            let default_user_agent = session.user_agent();
            session
                .set_font_families("Times New Roman", "Times New Roman", "Arial", "monospace")
                .map_err(|error| format!("browser font defaults: {error}"))?;
            session
                .bind_view(&router, view)
                .map_err(|e| format!("bind_view: {e}"))?;
            if let Some(size) = initial_size {
                session
                    .resize_viewport(&router, view, size)
                    .map_err(|e| format!("initial viewport resize: {e}"))?;
            }
            session
                .enable_input_method()
                .map_err(|e| format!("enable_input_method: {e}"))?;
            let mut policy = enzui_webkit::web::WebPolicy::strict()
                .grant(enzui_webkit::web::policy::PermissionKind::MediaKeySystem);
            policy.allow_popups = true;
            policy.allow_downloads = true;
            // A certificate that fails validation is refused unless the
            // person chose to continue to that very host, in this view.
            policy.allow_insecure_certificates = true;
            let trusted_hosts = std::rc::Rc::new(std::cell::RefCell::new(
                std::collections::BTreeSet::<String>::new(),
            ));
            let trusted = std::rc::Rc::clone(&trusted_hosts);
            session
                .install_native_policy(&router, view, policy, move |request| {
                    match &request.kind {
                        enzui_webkit::web::policy::RequestKind::Certificate(certificate)
                            if !trusted.borrow().contains(&certificate.host) =>
                        {
                            None
                        }
                        _ => Some(request.id),
                    }
                })
                .map_err(|e| format!("browser policy: {e}"))?;
            // A window a page opens is a real view, so that the two can talk
            // (a sign-in window answers its opener). It loads nothing until
            // it is a tab here with this tab's filters, scripts and policy.
            session.set_popups(true);
            session.hold_popup_navigations(true);
            session
                .set_origin_permission_mode(true)
                .map_err(|e| e.to_string())?;
            session.set_download_mime_types(&["application/pdf"]);
            // Call checks need a camera and microphone on machines without
            // them; only a QA run may swap in WebKit's generated devices.
            if std::env::var_os("ENZIM_QA").is_some()
                && std::env::var_os("ENZIM_QA_MOCK_CAPTURE").is_some_and(|value| value == "1")
            {
                session.set_mock_capture_devices(true);
            }
            let downloads = if std::env::var_os("ENZIM_QA").is_some() {
                std::env::var_os("XDG_DATA_HOME")
                    .map_or_else(
                        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("work/overnight/qa/data"),
                        PathBuf::from,
                    )
                    .join("enzim-browser/qa-downloads")
            } else {
                crate::downloads::folder()
            };
            std::fs::create_dir_all(&downloads).map_err(|e| format!("downloads: {e}"))?;
            session
                .set_download_directory(Some(downloads))
                .map_err(|e| format!("downloads: {e}"))?;
            let chrome = enzui_webkit::page::ScriptWorld::Isolated("enzim-browser-chrome".into());
            for name in ["enzimCosmetics", "enzimBackground"] {
                session.register_message_handler(name, &chrome);
            }
            #[cfg(feature = "benchmark")]
            Self::register_document_timing(&mut session);
            let credential_active =
                Self::install_scripts(&mut session, false, None, profile.kind().is_ephemeral());
            session.set_page_prompts(enzui_webkit::page::PagePrompts {
                script_dialogs: true,
                option_menus: true,
                file_chooser: true,
                authentication: true,
                color_chooser: true,
            });
            session.enable_gpu_presentation();
            let gpu_formats_set = enzui_webview::gpu::renderer_support()
                .flatten()
                .filter(|support| !support.formats.is_empty())
                .is_some_and(|support| {
                    session.set_gpu_buffer_formats(&router, view, &support.formats)
                });
            if session.is_private_bootstrap_document() {
                // Bootstrap identity describes the current document, not older queued events.
                let _ = session.take_page_events();
            }
            if let Ok(url) = std::env::var("ENZUI_PROBE_URL") {
                session
                    .load(&url, 20_000)
                    .map_err(|e| format!("load: {e}"))?;
            }
            let favicons = NativeFavicons::new(&session);
            static NEXT_MEMORY_ID: std::sync::atomic::AtomicU64 =
                std::sync::atomic::AtomicU64::new(1);
            Ok(Self {
                memory_id: NEXT_MEMORY_ID.fetch_add(1, Ordering::Relaxed),
                edited: false,
                last_visible: std::time::Instant::now(),
                active_filters: (0, Vec::new()),
                session_size: initial_size,
                zoom: 1.0,
                preferred_dark: None,
                preferred_system_font: None,
                pending_restore: false,
                credential_private: profile.kind().is_ephemeral(),
                defer_prepared_scripts,
                credential_active,
                favicons,
                privacy_document: None,
                developer_tools: false,
                resource_log: None,
                resource_notice: None,
                pending_copy: None,
                copy_poll_due: None,
                native_operations: Vec::new(),
                filter_store: None,
                filter_cache,
                filter_installation: None,
                filter_mode: Default::default(),
                filter_generation: Default::default(),
                privacy: Default::default(),
                default_user_agent,
                permission_choices: Vec::new(),
                notifications_enabled: true,
                pdf_job: None,
                jobs: Default::default(),
                active_downloads: Default::default(),
                download_controls: Default::default(),
                download_requests: Vec::new(),
                profile,
                display,
                profiles,
                session: RetainedSession::new(session),
                router,
                view,
                seq: 0,
                generation: initial_generation,
                gpu_enabled: true,
                sign_in_sent: false,
                trusted_hosts,
                certificate_refusals: 0,
                snapshot_ack: None,
                gpu_last: None,
                gpu_formats_set,
                gpu_negotiating_until: gpu_formats_set
                    .then(|| std::time::Instant::now() + Duration::from_secs(2)),
                visible: true,
                background_memory: Default::default(),
                idle_collected: false,
                last_activity: std::time::Instant::now(),
                metadata_cache: None,
                cache_metadata: std::env::var_os("ENZUI_PROBE_METADATA_CACHE")
                    .is_none_or(|value| value != "0"),
                metadata_builds: 0,
            })
        }

        fn recreate(
            &mut self,
            router: ViewRouter,
            view: ViewId,
            size: Option<enzui_webkit::ViewportSize>,
        ) -> Result<(), String> {
            self.native_operations.clear();
            self.filter_installation.take();
            if let Some((id, job)) = self.pdf_job.take() {
                self.session.cancel_pdf_export(id);
                job.complete(Err("The page was recreated.".into()));
            }
            for job in std::mem::take(&mut self.jobs).into_values() {
                job.complete(Err("The page was recreated.".into()));
            }
            self.session.blur_view();
            self.session.pump(10);
            let mut replacement = Self::open(
                router,
                view,
                PathBuf::new(),
                size,
                self.generation.next(),
                self.profile.clone(),
            )?;
            self.privacy.apply(&replacement.session);
            replacement.privacy = self.privacy.clone();
            replacement
                .configure_permissions(self.permission_choices.clone())
                .map_err(|e| e.to_string())?;
            replacement.notifications_enabled = self.notifications_enabled;
            replacement
                .session
                .set_notifications_enabled(self.notifications_enabled);
            replacement.configure_developer_tools(self.developer_tools);
            replacement.memory_id = self.memory_id;
            replacement.visible = self.visible;
            replacement.background_memory = std::mem::take(&mut self.background_memory);
            replacement.session.set_view_visibility(
                &replacement.router,
                replacement.view,
                self.visible,
            );
            replacement.filter_store.clone_from(&self.filter_store);
            self.discard_gpu_frame();
            *self = replacement;
            Ok(())
        }

        fn snapshot(&mut self) -> Result<Rgba8Frame, String> {
            self.seq += 1;
            let frame = self
                .session
                .snapshot(
                    enzui_webview::convert::engine_generation(self.generation),
                    FrameSeq(self.seq),
                    1_200,
                )
                .map(enzui_webview::convert::media_frame)
                .map_err(|e| format!("snapshot: {e}"));
            // Readback can complete its own compositor work. Consume it even on
            // failure rather than scheduling repeated captures of an unchanged page.
            self.snapshot_ack = Some(self.session.acknowledged_frames());
            frame
        }

        fn resize(&mut self, view: ViewId, size: enzui_webkit::ViewportSize) -> NativeUpdate {
            let outcome = self.session.resize_viewport(&self.router, view, size);
            let changed = matches!(
                outcome,
                Ok(enzui_webkit::ffi::runtime::ViewportResize::Requested)
            );
            if changed {
                self.settle(300);
            }
            let mut update = self.update(None, changed);
            if let Err(error) = outcome {
                super::add_command_error(&mut update, "resize viewport", error.to_string());
            }
            update
        }

        fn update(
            &mut self,
            tab_reply: Option<(FocusGeneration, TabReply)>,
            with_frame: bool,
        ) -> NativeUpdate {
            #[cfg(feature = "benchmark")]
            let _timed = TimedTrace("slow_update", std::time::Instant::now());
            let copied = if self.visible {
                self.pending_copy
                    .as_mut()
                    .and_then(|copy| copy.poll(&self.session, &self.display, &self.router))
            } else {
                None
            };
            if copied.is_some() || !self.visible {
                self.pending_copy = None;
            }
            self.copy_poll_due = self
                .pending_copy
                .as_ref()
                .map(|_| std::time::Instant::now() + Duration::from_millis(8));
            for mut operation in std::mem::take(&mut self.native_operations) {
                if !operation(self) {
                    self.native_operations.push(operation);
                }
            }
            let (metadata, mut error) = self.metadata_snapshot();
            if metadata.location != self.privacy_document {
                self.privacy_document.clone_from(&metadata.location);
                if let Some(address) = &metadata.location {
                    self.privacy.apply_for_site(
                        &self.session,
                        address,
                        self.developer_tools,
                        &self.default_user_agent,
                    );
                }
            }
            let mut page_events = self.session.take_page_events();
            #[cfg(feature = "benchmark")]
            page_events.retain(|event| !self.record_document_timing(event));
            if page_events.iter().any(|event| {
                matches!(
                    event,
                    enzui_webkit::page::PageEvent::Load(enzui_webkit::page::LoadStage::Started)
                )
            }) {
                self.favicons.reset();
            }
            if let Some(icon) = self.favicons.poll(
                &self.session,
                self.view,
                metadata.location.as_deref(),
                &self.privacy,
            ) {
                page_events.push(icon);
            }
            if self.session.is_private_bootstrap_document() {
                page_events
                    .retain(|event| !matches!(event, enzui_webkit::page::PageEvent::Load(_)));
            }
            if page_events.iter().any(|event| {
                matches!(
                    event,
                    enzui_webkit::page::PageEvent::SecretScriptMessage { .. }
                )
            }) && self.credential_settings().is_none()
            {
                page_events.retain(|event| {
                    !matches!(
                        event,
                        enzui_webkit::page::PageEvent::SecretScriptMessage { .. }
                    )
                });
                self.configure_credential_document();
            }
            page_events.retain(|event| {
                if let enzui_webkit::page::PageEvent::ScriptMessage { handler, json } = event
                    && handler == "enzimTabEdits"
                    && json == "true"
                {
                    self.edited = true;
                    false
                } else if let enzui_webkit::page::PageEvent::ScriptMessage { handler, json } = event
                    && handler == "enzimCosmetics"
                {
                    self.cosmetic_message(json);
                    false
                } else {
                    true
                }
            });
            for event in &mut page_events {
                // A window the page opened is taken at once and kept for its
                // tab; one that cannot be taken is opened as an address. One
                // opened with the middle button is let go and opened as an
                // address too: its tab stays behind, and a window's view is
                // only adopted by a tab on show.
                if let enzui_webkit::page::PageEvent::PopupOpened {
                    id,
                    url,
                    user_initiated,
                    background: true,
                } = event
                {
                    self.session.discard_popup(*id);
                    *event = enzui_webkit::page::PageEvent::PopupRequested {
                        url: url.clone(),
                        user_initiated: *user_initiated,
                        background: true,
                    };
                } else if let enzui_webkit::page::PageEvent::PopupOpened {
                    id,
                    url,
                    user_initiated,
                    background: false,
                } = event
                {
                    match self.session.take_popup(*id) {
                        Some(Ok(popup)) => {
                            super::remember_popup_features(id.get(), popup.window_features());
                            keep_popup(id.get(), popup);
                        }
                        _ => {
                            *event = enzui_webkit::page::PageEvent::PopupRequested {
                                url: url.clone(),
                                user_initiated: *user_initiated,
                                background: false,
                            };
                        }
                    }
                }
            }
            for event in &page_events {
                use enzui_webkit::page::PageEvent;
                match event {
                    PageEvent::DownloadStarted { id, .. } => {
                        self.active_downloads.insert(*id);
                    }
                    PageEvent::DownloadFinished { id, .. }
                    | PageEvent::DownloadPaused { id, .. }
                    | PageEvent::DownloadFailed { id, .. }
                    | PageEvent::DownloadInterrupted { id, .. } => {
                        self.active_downloads.remove(id);
                    }
                    _ => {}
                }
                match event {
                    PageEvent::DownloadPaused { id, .. } => {
                        if let Some((job, _)) = self.download_controls.remove(id) {
                            job.complete(Ok(crate::native_jobs::Payload::Json(
                                serde_json::Value::Null,
                            )));
                        }
                    }
                    PageEvent::DownloadFailed { id, message, .. }
                    | PageEvent::DownloadInterrupted { id, message, .. } => {
                        if let Some((job, _)) = self.download_controls.remove(id) {
                            job.complete(Err(message.clone()));
                        }
                    }
                    PageEvent::DownloadRequested { url, .. } => {
                        if let Some(index) = self
                            .download_requests
                            .iter()
                            .position(|(uri, _, _)| uri == url.as_str())
                        {
                            let (_, job, _) = self.download_requests.remove(index);
                            job.complete(Ok(crate::native_jobs::Payload::Json(
                                serde_json::Value::Null,
                            )));
                        }
                    }
                    _ => {}
                }
            }
            self.download_controls.retain(|_, (job, start)| {
                if start.elapsed() >= Duration::from_secs(10) {
                    job.complete(Err(
                        "The native download did not acknowledge the action".into()
                    ));
                    false
                } else {
                    !job.closed()
                }
            });
            self.download_requests.retain(|(uri, job, started)| {
                if started.elapsed() > Duration::from_secs(30) {
                    self.session.cancel_range_download(uri);
                    job.complete(Err(
                        "The server did not start the download within 30 seconds".into(),
                    ));
                    false
                } else {
                    !job.closed()
                }
            });
            self.collect_resource_events(&mut page_events);
            self.jobs.retain(|_, job| !job.closed());
            if let Some((id, job)) = &self.pdf_job
                && job.closed()
            {
                self.session.cancel_pdf_export(*id);
            }
            page_events.retain(|event| {
                if let enzui_webkit::page::PageEvent::DataCleared { id, result } = event
                    && let Some(job) = self.jobs.remove(id)
                {
                    job.complete(
                        result
                            .clone()
                            .map(|()| crate::native_jobs::Payload::Json(serde_json::Value::Null)),
                    );
                    return false;
                }
                if let enzui_webkit::page::PageEvent::PdfExportFinished { id, result } = event
                    && self
                        .pdf_job
                        .as_ref()
                        .is_some_and(|(pending, _)| pending == id)
                {
                    if let Some((_, job)) = self.pdf_job.take() {
                        job.complete(
                            result
                                .clone()
                                .map(crate::native_jobs::Payload::Pdf)
                                .map_err(|e| e.to_string()),
                        );
                    }
                    return false;
                }
                if let enzui_webkit::page::PageEvent::ScriptResult { id, result } = event
                    && let Some(job) = self.jobs.remove(id)
                {
                    let result = match result {
                        Ok(json) => serde_json::from_str(json)
                            .or_else(|_| {
                                Ok::<_, serde_json::Error>(serde_json::Value::String(json.clone()))
                            })
                            .map_err(|e| e.to_string()),
                        Err(enzui_webkit::page::ScriptError::Failed(message)) => {
                            Err(message.clone())
                        }
                        Err(enzui_webkit::page::ScriptError::Unavailable) => Ok(serde_json::json!({
                            "kind":"unavailable",
                            "preview":"Evaluated once. This result type is unavailable. For async values, use .then(value => console.log(value))."
                        })),
                        Err(enzui_webkit::page::ScriptError::Cancelled) => {
                            Err("The page changed before evaluation finished.".into())
                        }
                        Err(enzui_webkit::page::ScriptError::TooLarge) => {
                            Err("The result is too large to display.".into())
                        }
                    };
                    job.complete(result.map(crate::native_jobs::Payload::Json));
                    false
                } else {
                    true
                }
            });
            // A refused certificate ends its load without a word to the
            // page's events: the engine's ledger is where it is told. The
            // address changes as the load is dropped, so that is when to look.
            if page_events
                .iter()
                .any(|event| matches!(event, enzui_webkit::page::PageEvent::UriChanged(_)))
                && let Some(ledger) = self.session.native_certificate_ledger()
                && ledger.callbacks() != self.certificate_refusals
            {
                self.certificate_refusals = ledger.callbacks();
                if let Some(refused) = ledger.events().pop().filter(|event| {
                    event.action != enzui_webkit::ffi::policy::CertificateAction::ContinuedForHost
                }) {
                    error.get_or_insert(NativeWorkerError::Certificate { host: refused.key });
                }
            }
            for event in &page_events {
                match event {
                    enzui_webkit::page::PageEvent::LoadFailed {
                        cancelled: false, ..
                    } if std::mem::take(&mut self.sign_in_sent) => {
                        // Once: the reload carries the credentials, and a
                        // second failure is the page's own.
                        let _ = self.session.reload(false);
                    }
                    enzui_webkit::page::PageEvent::LoadFailed {
                        message,
                        cancelled: false,
                        ..
                    } => {
                        error.get_or_insert_with(|| NativeWorkerError::load_failure(message));
                    }
                    enzui_webkit::page::PageEvent::Load(
                        enzui_webkit::page::LoadStage::Finished,
                    ) => self.sign_in_sent = false,
                    enzui_webkit::page::PageEvent::WebProcessTerminated(reason)
                        if *reason != enzui_webkit::page::TerminationReason::TerminatedByHost =>
                    {
                        error.get_or_insert(NativeWorkerError::Renderer {
                            memory: *reason
                                == enzui_webkit::page::TerminationReason::ExceededMemory,
                        });
                    }
                    _ => {}
                }
            }
            let gpu_frame = self.compositor_frame();
            NativeUpdate {
                page_events,
                view: self.view,
                report: self.session.native_ime_report(),
                metadata: Some(metadata),
                frame: (with_frame && !self.gpu_enabled && self.visible).then(|| self.snapshot()),
                gpu_frame,
                tab_reply,
                view_focused: Some(self.session.view_has_focus()),
                error,
                clipboard: copied
                    .as_ref()
                    .and_then(|result| result.as_ref().err())
                    .cloned()
                    .map(ClipboardOutcome::Refused),
                selection: copied
                    .and_then(Result::ok)
                    .map(|selection| super::SelectionText {
                        text: selection.text,
                    }),
            }
        }

        fn clipboard(
            &mut self,
            view: ViewId,
            gesture: HostGesture,
            text: Option<&str>,
        ) -> NativeUpdate {
            if text.is_none() {
                if gesture == HostGesture::Present
                    && self.visible
                    && view == self.view
                    && self.router.accepts(view)
                {
                    self.session.focus_view();
                }
                let result = if !self.visible {
                    Err(enzui_webkit::clipboard_route::ClipboardRouteError::ViewNotFocused)
                } else if self.pending_copy.is_some() {
                    Err(enzui_webkit::clipboard_route::ClipboardRouteError::Native(
                        "A page copy is already pending.".into(),
                    ))
                } else {
                    self.session
                        .begin_page_copy(&self.display, &self.router, view, gesture)
                };
                let error = match result {
                    Ok(copy) => {
                        self.pending_copy = Some(copy);
                        None
                    }
                    Err(error) => Some(error),
                };
                let mut update = self.update(None, false);
                if let Some(error) = error {
                    update.clipboard = Some(ClipboardOutcome::Refused(error));
                }
                return update;
            }
            self.pending_copy = None;
            self.copy_poll_due = None;
            let outcome = self
                .session
                .paste_host_text_into_page(
                    &self.display,
                    &self.router,
                    view,
                    gesture,
                    text.unwrap_or_default(),
                )
                .map(ClipboardOutcome::Pasted);
            // Pastes reach the DOM pre-snapshot, like any forwarded input.
            if outcome.is_ok() {
                self.settle(300);
            }
            let mut update = self.update(None, outcome.is_ok());
            update.clipboard = Some(outcome.unwrap_or_else(ClipboardOutcome::Refused));
            update
        }
    }

    include!("native_feature_gate.rs");
    include!("native_followup_gate.rs");
    include!("native_latency_gate.rs");
    include!("native_volume_gate.rs");
    include!("native_assets_gate.rs");
    include!("native_privacy_gate.rs");
    include!("native_download_gate.rs");
    include!("native_theme_gate.rs");
    include!("native_youtube_gate.rs");
    include!("native_devtools_gate.rs");
    include!("native_encrypted_media_gate.rs");

    #[cfg(feature = "benchmark")]
    pub(super) fn run_storage_gate(scratch: PathBuf) -> Result<serde_json::Value, String> {
        fn open(scratch: &std::path::Path, profile: WebProfile) -> Result<Worker, String> {
            let mut router = ViewRouter::new();
            let view = router.create(990);
            let mut worker = Worker::open(
                router,
                view,
                scratch.to_path_buf(),
                None,
                MediaGeneration::FIRST,
                profile,
            )?;
            worker.session.disable_gpu_presentation();
            worker.gpu_enabled = false;
            worker.visible = false;
            Ok(worker)
        }
        fn seed(worker: &Worker, value: &str) -> Result<(), String> {
            worker
                .profiles
                .borrow_mut()
                .session_for(&worker.profile)
                .map_err(|e| e.to_string())?
                .add_cookie("example.com", "/", "enzimIsolation", value)
                .map_err(|e| e.to_string())
        }
        fn read(worker: &mut Worker) -> Result<String, String> {
            worker
                .session
                .load_html(
                    "<!doctype html><title>Storage check</title><p>Storage check</p>",
                    "https://example.com/",
                    10000,
                )
                .map_err(|e| e.to_string())?;
            let id = worker
                .session
                .evaluate_script("document.cookie", &enzui_webkit::page::ScriptWorld::Page)
                .map_err(|e| e.to_string())?;
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                worker.session.pump(2);
                for event in worker.session.take_page_events() {
                    if let enzui_webkit::page::PageEvent::ScriptResult { id: found, result } = event
                        && found == id
                    {
                        let json = result.map_err(|e| format!("{e:?}"))?;
                        return serde_json::from_str::<String>(&json).map_err(|e| e.to_string());
                    }
                }
                if std::time::Instant::now() >= deadline {
                    return Err("Cookie read did not finish.".into());
                }
            }
        }
        fn expect(actual: &str, expected: &str, label: &str) -> Result<(), String> {
            if actual == expected {
                Ok(())
            } else {
                Err(format!("{label}: expected {expected:?}, got {actual:?}"))
            }
        }
        let work = WebProfile::persistent("qualification-work");
        let personal = WebProfile::persistent("qualification-personal");
        let mut a = open(&scratch, work.clone())?;
        seed(&a, "work")?;
        expect(
            &read(&mut a)?,
            "enzimIsolation=work",
            "Named profile positive control",
        )?;
        let mut b = open(&scratch, personal)?;
        expect(&read(&mut b)?, "", "Named profile isolation")?;
        seed(&b, "personal")?;
        expect(
            &read(&mut b)?,
            "enzimIsolation=personal",
            "Other named profile positive control",
        )?;
        expect(
            &read(&mut a)?,
            "enzimIsolation=work",
            "Named profile remains separate",
        )?;
        a.recreate(a.router.clone(), a.view, None)?;
        expect(
            &read(&mut a)?,
            "enzimIsolation=work",
            "Recreation retains storage identity",
        )?;
        drop(a);
        let mut reopened = open(&scratch, work)?;
        expect(
            &read(&mut reopened)?,
            "enzimIsolation=work",
            "Persistent cookie survives recreation",
        )?;
        let private = WebProfile::ephemeral();
        let mut p = open(&scratch, private.clone())?;
        let native_ephemeral = p
            .profiles
            .borrow_mut()
            .session_for(&private)
            .map_err(|e| e.to_string())?
            .is_ephemeral();
        if !native_ephemeral {
            return Err("Private view received persistent native storage.".into());
        }
        expect(
            &read(&mut p)?,
            "",
            "Private profile cannot read regular cookies",
        )?;
        seed(&p, "private")?;
        expect(
            &read(&mut p)?,
            "enzimIsolation=private",
            "Private cookie positive control",
        )?;
        let mut same_private = open(&scratch, private)?;
        expect(
            &read(&mut same_private)?,
            "enzimIsolation=private",
            "Tabs in one private window share its ephemeral identity",
        )?;
        drop(p);
        drop(same_private);
        let mut fresh_private = open(&scratch, WebProfile::ephemeral())?;
        expect(
            &read(&mut fresh_private)?,
            "",
            "Closed private session does not leak to a new session",
        )?;
        expect(
            &read(&mut reopened)?,
            "enzimIsolation=work",
            "Private cookies do not overwrite regular cookies",
        )?;
        Ok(
            serde_json::json!({"named_isolation":true,"positive_controls":true,"persistent_reopen":true,"recreation_identity":true,"native_ephemeral":true,"private_tab_sharing":true,"private_reopen_empty":true,"regular_profile_preserved":true}),
        )
    }

    #[cfg(feature = "benchmark")]
    #[allow(
        clippy::unwrap_used,
        clippy::unwrap_in_result,
        clippy::missing_assert_message,
        clippy::print_stdout,
        reason = "Explicit native qualification entry points report metrics and fail on violated invariants."
    )]
    pub(super) fn run_native_title_mapping_gate(scratch: PathBuf) -> Result<(), String> {
        let mut router = ViewRouter::new();
        let view = router.create(2875);
        let mut worker = Worker::open(
            router,
            view,
            scratch,
            None,
            MediaGeneration::FIRST,
            WebProfile::shared(),
        )?;
        worker.session.disable_gpu_presentation();
        worker.gpu_enabled = false;
        worker.visible = false;
        for (markup, expected) in [
            (
                "Cr&egrave;me &#x2615; &#x2014; &#x6771;&#x4EAC;".to_owned(),
                "Crème ☕ — 東京".to_owned(),
            ),
            ("x".repeat(16385), "x".repeat(1000)),
        ] {
            worker
                .session
                .load_html(
                    &format!("<!doctype html><meta charset=utf-8><title>{markup}</title>"),
                    "about:blank",
                    10000,
                )
                .map_err(|e| e.to_string())?;
            assert_eq!(
                worker.session.page_title().unwrap().as_deref(),
                Some(expected.as_str()),
                "native title follows upstream truncation"
            );
            let update = worker.update(None, false);
            assert_eq!(
                update
                    .metadata
                    .as_ref()
                    .and_then(|value| value.dom_report.as_deref()),
                Some(expected.as_str())
            );
            assert!(update.error.is_none());
        }
        let error = enzui_webkit::ffi::runtime::PageTitleError::TooLong {
            observed_bytes: 16385,
            max_bytes: 16384,
        };
        worker.metadata_cache = Some(CachedMetadata {
            revision: worker.session.page_metadata_revision(),
            value: Arc::new(super::NativeMetadata::default()),
            title_error: Some(error),
        });
        let update = worker.update(None, false);
        assert_eq!(
            update.error,
            Some(error.into()),
            "cached typed title refusal survives reuse"
        );
        Ok(())
    }

    #[cfg(feature = "benchmark")]
    fn background_frame_gate(scratch: PathBuf) -> Result<(), String> {
        fn frame(worker: &mut Worker) -> Result<enzui_webview::gpu::GpuFrame, String> {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                worker.session.pump(10);
                if let Some(frame) = worker.compositor_frame() {
                    return Ok(frame);
                }
                if std::time::Instant::now() >= deadline {
                    return Err("native background-frame qualifier received no GPU frame".into());
                }
            }
        }
        let mut router = ViewRouter::new();
        let view = router.create(3290);
        let viewport = enzui_webkit::ViewportSize::from_logical(640.0, 480.0)
            .map_err(|error| error.to_string())?;
        let mut worker = Worker::open(
            router,
            view,
            scratch,
            Some(viewport),
            MediaGeneration::FIRST,
            WebProfile::shared(),
        )?;
        worker.session.load_html("<!doctype html><body style='background:royalblue'><input id=kept value=original><script>window.kept=42</script>", "about:blank", 10_000)
            .map_err(|error| error.to_string())?;
        metadata_eval(
            &worker,
            "document.getElementById('kept').value='edited';true",
        )?;
        let first = frame(&mut worker)?;
        let serial = first.content_info().serial();
        let retained = first.lifetime_probe();
        first.discard();
        drop(first);
        assert!(
            retained(),
            "the worker owns the displayed GPU frame before trimming"
        );
        drop(worker.set_visibility(view, false));
        assert!(
            !retained(),
            "hiding releases the worker's held frame immediately"
        );
        let due = worker
            .background_memory
            .deadline()
            .ok_or("hidden trim has no deadline")?;
        worker.maintain_background_memory(due);
        assert!(
            !retained(),
            "hidden trim releases the worker's native GPU frame lease"
        );
        let next_due = worker
            .background_memory
            .deadline()
            .ok_or("hidden maintenance has no deadline")?;
        assert!(
            next_due > due,
            "maintenance advances the hidden trim deadline"
        );
        drop(worker.set_visibility(view, true));
        let returned = frame(&mut worker)?;
        assert!(
            returned.content_info().serial() > serial,
            "showing a trimmed page exports a new GPU frame"
        );
        drop(worker.set_visibility(view, false));
        drop(worker.set_visibility(view, true));
        let quick = frame(&mut worker)?;
        assert!(
            quick.content_info().serial() > returned.content_info().serial(),
            "a quick hide/show also renders a fresh frame without a memory trim"
        );
        metadata_eval(
            &worker,
            "document.title=(window.kept===42&&document.getElementById('kept').value==='edited')?'gpu state kept':'gpu state lost';true",
        )?;
        assert_eq!(
            worker
                .session
                .page_title()
                .map_err(|error| error.to_string())?
                .as_deref(),
            Some("gpu state kept"),
            "GPU frame release preserves JavaScript and edited input state"
        );
        returned.discard();
        Ok(())
    }

    #[cfg(feature = "benchmark")]
    fn metadata_worker(scratch: PathBuf) -> Result<Worker, String> {
        let mut router = ViewRouter::new();
        let view = router.create(904);
        let mut worker = Worker::open(
            router,
            view,
            scratch,
            None,
            MediaGeneration::FIRST,
            WebProfile::shared(),
        )?;
        worker.session.disable_gpu_presentation();
        worker.gpu_enabled = false;
        worker.discard_gpu_frame();
        worker.visible = false;
        Ok(worker)
    }

    #[cfg(feature = "benchmark")]
    fn metadata_eval(worker: &Worker, script: &str) -> Result<(), String> {
        let id = worker
            .session
            .evaluate_script(script, &enzui_webkit::page::ScriptWorld::Page)
            .map_err(|e| format!("{e:?}"))?;
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            worker.session.pump(2);
            for event in worker.session.take_page_events() {
                if let enzui_webkit::page::PageEvent::ScriptResult { id: found, result } = event
                    && found == id
                {
                    result.map_err(|e| format!("{e:?}"))?;
                    return Ok(());
                }
            }
            if std::time::Instant::now() >= deadline {
                return Err("metadata script did not finish".into());
            }
        }
    }

    #[cfg(feature = "benchmark")]
    #[allow(
        clippy::unwrap_used,
        clippy::unwrap_in_result,
        clippy::missing_assert_message,
        clippy::print_stdout,
        reason = "Explicit native qualification entry points report metrics and fail on violated invariants."
    )]
    pub(super) fn run_metadata_benchmark(scratch: PathBuf) -> Result<(), String> {
        fn cpu_ns() -> u64 {
            std::fs::read_to_string("/proc/thread-self/schedstat")
                .unwrap()
                .split_whitespace()
                .next()
                .unwrap()
                .parse()
                .unwrap()
        }
        let mut worker = metadata_worker(scratch)?;
        let expected_cache =
            std::env::var_os("ENZUI_PROBE_METADATA_CACHE").is_none_or(|value| value != "0");
        for length in [32, 1000] {
            let html = format!("<title>{}</title>", "t".repeat(length));
            worker
                .session
                .load_html(&html, "about:blank", 5000)
                .map_err(|e| format!("metadata fixture: {e}"))?;
            let warm = worker.update(None, false);
            assert_eq!(
                warm.metadata
                    .as_ref()
                    .and_then(|value| value.dom_report.as_ref())
                    .map(String::len),
                Some(length),
                "timed title length matches the native property"
            );
            for round in 0..3 {
                let builds = worker.metadata_builds;
                let cpu = cpu_ns();
                let started = std::time::Instant::now();
                for _ in 0..1000000 {
                    std::hint::black_box(worker.update(None, false));
                }
                let cpu_ms = (cpu_ns() - cpu) as f64 / 1_000_000.0;
                let snapshots = worker.metadata_builds - builds;
                println!(
                    "{{\"metadata_benchmark\":true,\"cache\":{expected_cache},\"title_bytes\":{length},\"round\":{round},\"updates\":1000000,\"snapshots\":{snapshots},\"cpu_ms\":{cpu_ms},\"wall_ms\":{}}}",
                    started.elapsed().as_secs_f64() * 1000.0
                );
                assert_eq!(
                    snapshots,
                    if expected_cache { 0 } else { 1000000 },
                    "unchanged updates reuse the default metadata snapshot"
                );
            }
        }
        Ok(())
    }

    #[cfg(feature = "benchmark")]
    #[allow(
        clippy::unwrap_used,
        clippy::unwrap_in_result,
        clippy::missing_assert_message,
        clippy::print_stdout,
        reason = "Explicit native qualification entry points report metrics and fail on violated invariants."
    )]
    pub(super) fn run_metadata_gate(scratch: PathBuf) -> Result<(), String> {
        let mut worker = metadata_worker(scratch.clone())?;
        worker.session.load_html("<title>initial</title><style>html,body{margin:0;width:100vw;height:100vh}</style><input id=p type=password value='PASSWORD_PRIVATE_CANARY'>","about:blank",5000).map_err(|e|e.to_string())?;
        let initial = worker
            .update(None, false)
            .metadata
            .expect("initial complete metadata");
        let stable = worker
            .update(None, false)
            .metadata
            .expect("unchanged updates carry complete metadata");
        assert!(
            Arc::ptr_eq(&initial, &stable),
            "stable metadata shares its snapshot identity"
        );
        let (commands, rx) = mpsc::sync_channel(COMMAND_CAP);
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let mut task = WorkerLoop {
            worker,
            rx: PageReceiver {
                commands: rx,
                stop: Arc::new(StopBool::new(false)),
            },
            updates: Arc::clone(&queue),
            admission: Arc::new(Mutex::new(Admission::default())),
            input_frame_due: None,
            _scratch: None,
        };
        metadata_eval(&task.worker, "document.title='metadata only';true")?;
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while !task.worker.publication_pending() {
            assert!(
                std::time::Instant::now() < deadline,
                "native title notification arrives"
            );
            task.worker.session.pump(2);
        }
        assert!(!task.worker.gpu_pending());
        assert!(task.step(false));
        let changed = queue
            .lock()
            .unwrap()
            .pop_back()
            .expect("metadata-only change publishes without GPU output");
        assert!(changed.frame.is_none() && changed.gpu_frame.is_none());
        let changed = changed.metadata.expect("metadata-only full snapshot");
        assert_eq!(changed.dom_report.as_deref(), Some("metadata only"));
        assert!(!Arc::ptr_eq(&initial, &changed));
        metadata_eval(
            &task.worker,
            "history.pushState({},'',location.href.split('#')[0]+'#metadata-uri');true",
        )?;
        task.step(false);
        let uri = queue
            .lock()
            .unwrap()
            .pop_back()
            .expect("URI-only change publishes")
            .metadata
            .unwrap();
        assert!(
            uri.location
                .as_deref()
                .is_some_and(|uri| uri.ends_with("#metadata-uri")),
            "current URI follows history without a frame"
        );
        metadata_eval(&task.worker, "document.body.style.cursor='crosshair';true")?;
        let mapping = ViewportMapping::new(0.0, 0.0, 1024.0, 768.0, 1.0).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            let moved = input::pointer_moved_with_modifiers(
                task.worker.view,
                &mapping,
                400.0,
                300.0,
                input::Modifiers::NONE,
            )
            .unwrap();
            assert!(task.worker.session.forward_input(&moved));
            task.worker.session.pump(10);
            if task.worker.session.requested_cursor().as_deref() == Some("crosshair") {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "cursor callback arrives"
            );
        }
        task.step(false);
        let cursor = queue
            .lock()
            .unwrap()
            .pop_back()
            .expect("cursor-only change publishes")
            .metadata
            .unwrap();
        assert_eq!(cursor.cursor.as_deref(), Some("crosshair"));
        metadata_eval(&task.worker, "document.title='';true")?;
        task.step(false);
        let empty = queue
            .lock()
            .unwrap()
            .pop_back()
            .expect("cleared title publishes")
            .metadata
            .unwrap();
        assert_eq!(empty.dom_report.as_deref(), Some(""));
        assert!(task.worker.session.forward_input(&InputEvent::FocusGained {
            view: task.worker.view,
            modifiers: input::Modifiers::NONE
        }));
        metadata_eval(&task.worker, "document.getElementById('p').focus();true")?;
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            let update = task.worker.update(None, false);
            if update.report.is_some_and(|report| {
                report.focused
                    && report.purpose == enzui_webkit::web::ime::WEBKIT_INPUT_PURPOSE_PASSWORD
            }) {
                assert!(!format!("{:?}", update.metadata).contains("PASSWORD_PRIVATE_CANARY"));
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "password purpose remains live"
            );
            task.worker.session.pump(10);
        }
        for _ in 0..UPDATE_CAP * 3 {
            push_update(&queue, task.worker.update(None, false));
        }
        let queued = queue.lock().unwrap();
        assert_eq!(queued.len(), UPDATE_CAP);
        assert!(
            queued.iter().all(|update| update.metadata.is_some()),
            "coalesced updates retain the complete current state"
        );
        drop(queued);
        for (seq, visible) in [(1, true), (2, false)] {
            commands
                .send(SeqCmd {
                    seq,
                    cmd: Command::Visibility {
                        view: task.worker.view,
                        visible,
                    },
                })
                .map_err(|e| e.to_string())?;
            assert!(task.step(false));
        }
        assert!(task.worker.background_memory.deadline().is_some());
        metadata_eval(
            &task.worker,
            "window.kept=42;window.registry=new FinalizationRegistry(()=>document.title='memory collected');window.memoryTarget={payload:new Array(64).fill(1)};registry.register(window.memoryTarget,0);document.title='memory waiting';true",
        )?;
        task.worker.session.pump(3000);
        assert_eq!(
            task.worker.session.page_title().unwrap().as_deref(),
            Some("memory waiting"),
            "reachable registered objects survive garbage collection"
        );
        metadata_eval(&task.worker, "window.memoryTarget=null;true")?;
        let past = std::time::Instant::now() - Duration::from_secs(61);
        task.worker.background_memory.visibility(true, past);
        task.worker.background_memory.visibility(false, past);
        task.worker
            .maintain_background_memory(std::time::Instant::now());
        assert!(task.step(false));
        // Eventual collection alone cannot distinguish a host release from idle GC.
        let deadline = std::time::Instant::now() + Duration::from_millis(500);
        while task.worker.session.page_title().unwrap().as_deref() != Some("memory collected") {
            assert!(
                std::time::Instant::now() < deadline,
                "background release must collect garbage through native release_memory"
            );
            task.worker.session.pump(10);
        }
        let maintenance = task
            .worker
            .background_memory
            .deadline()
            .expect("hidden pages retain a maintenance deadline");
        metadata_eval(
            &task.worker,
            "window.registry2=new FinalizationRegistry(()=>document.title='maintenance collected');window.maintenanceTarget={payload:new Array(64).fill(1)};registry2.register(window.maintenanceTarget,0);document.title='maintenance waiting';true",
        )?;
        task.worker.session.pump(3000);
        assert_eq!(
            task.worker.session.page_title().unwrap().as_deref(),
            Some("maintenance waiting"),
            "new reachable objects survive until their reference is cleared"
        );
        metadata_eval(&task.worker, "window.maintenanceTarget=null;true")?;
        task.worker.maintain_background_memory(maintenance);
        let deadline = std::time::Instant::now() + Duration::from_millis(500);
        while task.worker.session.page_title().unwrap().as_deref() != Some("maintenance collected")
        {
            assert!(
                std::time::Instant::now() < deadline,
                "hidden maintenance collects newly allocated garbage"
            );
            task.worker.session.pump(10);
        }
        metadata_eval(
            &task.worker,
            "document.title=(window.kept===42&&document.getElementById('p').value==='PASSWORD_PRIVATE_CANARY')?'memory state kept':'state lost';true",
        )?;
        assert_eq!(
            task.worker.session.page_title().unwrap().as_deref(),
            Some("memory state kept")
        );
        commands
            .send(SeqCmd {
                seq: 3,
                cmd: Command::Visibility {
                    view: task.worker.view,
                    visible: true,
                },
            })
            .map_err(|e| e.to_string())?;
        assert!(task.step(false));
        assert_eq!(task.worker.background_memory.deadline(), None);
        drop(task);
        drop(commands);
        background_frame_gate(scratch.clone())?;
        run_tab_memory_gate(scratch.clone())?;
        run_native_title_mapping_gate(scratch)?;
        println!(
            "Metadata gate completed: shared snapshots, metadata-only idle publication, title clearing, history URI, cursor-only changes, password purpose, bounded queue, native title clipping, cached typed refusal"
        );
        Ok(())
    }

    fn push_update(queue: &Arc<Mutex<VecDeque<NativeUpdate>>>, update: NativeUpdate) {
        let mut q = queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = q.len();
        push_bounded(&mut q, update);
        super::QUEUED_UPDATES.fetch_add(q.len().saturating_sub(before), Ordering::AcqRel);
        super::wake_native_worker();
    }

    include!("native_opening.rs");
    include!("native_opening_gate.rs");

    #[derive(Clone)]
    struct OpenPage {
        /// The window a page opened that this tab is made around, by the
        /// number it was kept under; `None` for a tab with a view of its own.
        adopt: Option<u64>,
        profile: enzui_webkit::web::WebProfile,
        // Reservations retain their session only through the live page. A real
        // navigation promotes it to the browser's profile lease table.
        retain_profile: bool,
        router: ViewRouter,
        view: ViewId,
        scratch: PathBuf,
        initial_size: Option<enzui_webkit::ViewportSize>,
        initial_generation: MediaGeneration,
    }

    struct WorkerLoop {
        worker: Worker,
        rx: PageReceiver,
        updates: Arc<Mutex<VecDeque<NativeUpdate>>>,
        admission: Arc<Mutex<Admission>>,
        input_frame_due: Option<std::time::Instant>,
        _scratch: Option<Arc<SecureScratch>>,
    }

    impl WorkerLoop {
        fn new(
            rx: PageReceiver,
            updates: Arc<Mutex<VecDeque<NativeUpdate>>>,
            admission: Arc<Mutex<Admission>>,
            page: OpenPage,
            scratch_lease: Option<Arc<SecureScratch>>,
        ) -> Option<Self> {
            if rx.stop_raised() {
                return None;
            }
            let view = page.view;
            let opened = (|| {
                let resources = opening_resources()?;
                let session = match page.adopt {
                    Some(popup) => take_kept_popup(popup)
                        .ok_or_else(|| "The window the page opened is gone.".to_owned())?,
                    None => Worker::new_session(
                        &resources.display,
                        &mut resources.profiles.borrow_mut(),
                        &page.profile,
                    )?,
                };
                Worker::finish_open(page, resources, session)
            })();
            let mut worker = match opened {
                Ok(worker) => worker,
                Err(error) => {
                    push_update(
                        &updates,
                        NativeUpdate {
                            page_events: Vec::new(),
                            view,
                            report: None,
                            metadata: None,
                            frame: None,
                            gpu_frame: None,
                            tab_reply: None,
                            view_focused: None,
                            error: Some(NativeWorkerError::Bootstrap {
                                stage: "display/session/load",
                                detail: error,
                            }),
                            clipboard: None,
                            selection: None,
                        },
                    );
                    return None;
                }
            };
            push_update(&updates, worker.update(None, true));

            Some(Self {
                worker,
                rx,
                updates,
                admission,
                input_frame_due: None,
                _scratch: scratch_lease,
            })
        }
        fn step(&mut self, blocking: bool) -> bool {
            #[cfg(feature = "benchmark")]
            let _timed = TimedTrace("slow_step", std::time::Instant::now());
            let mut resuming = None;
            if self.worker.session.discarded.is_some() {
                if self.rx.stop_raised() {
                    self.rx.drain_after_stop();
                    return false;
                }
                match self.rx.commands.try_recv() {
                    Ok(command) => match command.cmd {
                        Command::Resize { size, .. } => {
                            self.worker.session_size = Some(size);
                            return true;
                        }
                        Command::FocusOut { .. } | Command::Visibility { visible: false, .. } => {
                            return true;
                        }
                        Command::Browse {
                            action: super::BrowserAction::SystemFont(font),
                            ..
                        } => {
                            self.worker.preferred_system_font = Some(font);
                            return true;
                        }
                        Command::Browse {
                            action: super::BrowserAction::ColorScheme(dark),
                            ..
                        } => {
                            self.worker.preferred_dark = Some(dark);
                            return true;
                        }
                        Command::Browse {
                            action: super::BrowserAction::Zoom(zoom),
                            ..
                        } => {
                            self.worker.zoom = zoom;
                            return true;
                        }
                        Command::Browse {
                            action: super::BrowserAction::Privacy(privacy),
                            ..
                        } => {
                            self.worker.privacy = *privacy;
                            return true;
                        }
                        Command::Browse {
                            action: super::BrowserAction::NotificationsEnabled(enabled),
                            ..
                        } => {
                            self.worker.notifications_enabled = enabled;
                            return true;
                        }
                        Command::Browse {
                            action: super::BrowserAction::PermissionChoices(choices),
                            ..
                        } => {
                            self.worker.permission_choices = choices;
                            return true;
                        }
                        Command::Browse {
                            action:
                                super::BrowserAction::CredentialRefresh
                                | super::BrowserAction::FindFinish
                                | super::BrowserAction::NotificationClick(_)
                                | super::BrowserAction::NotificationClose(_),
                            ..
                        } => return true,
                        Command::Browse {
                            action: super::BrowserAction::MemoryProtected(pinned),
                            ..
                        } => {
                            self.admission
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .memory_protected = pinned;
                            return true;
                        }
                        cmd => {
                            resuming = Some(SeqCmd {
                                seq: command.seq,
                                cmd,
                            });
                        }
                    },
                    Err(mpsc::TryRecvError::Empty) => return true,
                    Err(mpsc::TryRecvError::Disconnected) => return false,
                }
                if let Err(error) = self.worker.restore_tab() {
                    eprintln!("Could not restore discarded tab: {error}");
                    return true;
                }
            }
            let Self {
                worker,
                rx,
                updates,
                admission,
                input_frame_due,
                ..
            } = self;
            if rx.stop_raised() {
                rx.drain_after_stop();
                return false;
            }
            let now = std::time::Instant::now();
            if blocking {
                worker.maintain_background_memory(now);
            }
            if worker.gpu_pending() {
                push_update(updates, worker.update(None, false));
            }
            if input_frame_due.is_some_and(|due| std::time::Instant::now() >= due) {
                *input_frame_due = None;
                worker.settle(5);
                push_update(updates, worker.update(None, true));
                push_update(updates, worker.update(None, false));
            }
            let command = if let Some(command) = resuming {
                command
            } else {
                match arbitrate_step::<Command>(&rx.commands, admission) {
                    ArbitrationStep::Execute { terminals, cmd } => {
                        // A release can outlive its input batch and still change pixels.
                        for pt in terminals {
                            let accepted = worker.session.forward_input(&pt.event);
                            if accepted {
                                worker.settle(10);
                                input_frame_due.get_or_insert_with(|| {
                                    std::time::Instant::now() + INPUT_FRAME_INTERVAL
                                });
                            }
                            push_update(updates, worker.update(None, accepted));
                        }
                        cmd
                    }
                    ArbitrationStep::Drain { terminals } => {
                        for pt in terminals {
                            let accepted = worker.session.forward_input(&pt.event);
                            if accepted {
                                worker.settle(10);
                                input_frame_due.get_or_insert_with(|| {
                                    std::time::Instant::now() + INPUT_FRAME_INTERVAL
                                });
                            }
                            push_update(updates, worker.update(None, accepted));
                        }
                        return true;
                    }
                    ArbitrationStep::Idle if input_frame_due.is_some() => {
                        worker.settle(5);
                        return true;
                    }
                    ArbitrationStep::Idle
                        if !blocking
                            || (worker.gpu_enabled && worker.visible)
                            || enzui_webkit::ffi::presentation::pending_gpu_frame_completions()
                                != 0 =>
                    {
                        if blocking {
                            worker.settle(5);
                        }
                        if worker.publication_pending() {
                            push_update(updates, worker.update(None, true));
                        }
                        return true;
                    }
                    ArbitrationStep::Idle => match rx.wait() {
                        Some(cmd) => {
                            // Blocked-while pushes with lower `seq` still run first.
                            for pt in drain_before_cmd(&cmd, admission) {
                                let accepted = worker.session.forward_input(&pt.event);
                                if accepted {
                                    worker.settle(10);
                                    input_frame_due.get_or_insert_with(|| {
                                        std::time::Instant::now() + INPUT_FRAME_INTERVAL
                                    });
                                }
                                push_update(updates, worker.update(None, accepted));
                            }
                            cmd
                        }
                        None => return false,
                    },
                    ArbitrationStep::Disconnected => return false,
                }
            };
            let SeqCmd { seq: _, cmd } = command;
            worker.last_activity = std::time::Instant::now();
            if !blocking {
                input_frame_due
                    .get_or_insert_with(|| std::time::Instant::now() + INPUT_FRAME_INTERVAL);
            }
            let result = match cmd {
                Command::Browse { view, action } => {
                    let result = if view != worker.view || !worker.router.accepts(view) {
                        Err("view generation is no longer live".to_owned())
                    } else {
                        match action {
                            super::BrowserAction::MemoryProtected(_)=>Ok(()),
                            super::BrowserAction::SystemFont(font)=>{let result=worker.session.set_system_font_name(&font);if result.is_ok(){worker.preferred_system_font=Some(font);}result},
                            super::BrowserAction::ColorScheme(dark)=>{worker.preferred_dark=Some(dark);worker.session.set_preferred_dark(dark)},
                            super::BrowserAction::Fixture => {
                                let html = include_str!("../tests/fixtures/browser.html");
                                worker.session.load(
                                    &format!(
                                        "data:text/html,{}",
                                        url::form_urlencoded::byte_serialize(html.as_bytes())
                                            .collect::<String>()
                                            .replace("+", "%20")
                                    ),
                                    20000,
                                )
                            }
                            super::BrowserAction::Navigate(url) => (|| {
                                #[cfg(feature = "benchmark")]
                                super::trace_navigation("navigate-dispatched");
                                // Only an explicit destination promotes a reservation. Its
                                // abort path never enters this strong-reference table.
                                worker.retain_profile_session()?;
                                #[cfg(feature = "benchmark")]
                                super::trace_navigation("profile-promoted");
                                worker.defer_prepared_scripts = false;
                                worker.pending_restore = false;
                                worker.edited = false;
                                worker.privacy.apply_for_site(
                                    &worker.session,
                                    &url,
                                    worker.developer_tools,
                                    &worker.default_user_agent,
                                );
                                #[cfg(feature = "benchmark")]
                                super::trace_navigation("site-privacy-applied");
                                worker.refresh_document_scripts(Some(&url));
                                #[cfg(feature = "benchmark")]
                                super::trace_navigation("load-uri");
                                worker.session.begin_load_uri(&url)
                            })(),
                            super::BrowserAction::ResumePopup(url) => (|| {
                                worker.retain_profile_session()?;
                                worker.defer_prepared_scripts = false;
                                worker.privacy.apply_for_site(
                                    &worker.session,
                                    &url,
                                    worker.developer_tools,
                                    &worker.default_user_agent,
                                );
                                worker.refresh_document_scripts(Some(&url));
                                worker.session.resume_popup();
                                Ok(())
                            })(),
                            super::BrowserAction::ColorPreview(id, color) => {
                                let _ = worker.session.preview_color(id, color);
                                Ok(())
                            }
                            super::BrowserAction::ColorAnswer(id, color) => {
                                let _ = worker.session.answer_color_chooser(id, color);
                                Ok(())
                            }
                            super::BrowserAction::Spelling(fix) => {
                                worker.session.fix_spelling(fix);
                                Ok(())
                            }
                            super::BrowserAction::SpellCheck(languages) => {
                                // One setting for every view: told again
                                // only when it changed.
                                std::thread_local! {
                                    static CHECKED: std::cell::RefCell<Option<Vec<String>>> =
                                        const { std::cell::RefCell::new(None) };
                                }
                                let changed = CHECKED.with(|checked| {
                                    let mut checked = checked.borrow_mut();
                                    let changed = checked.as_ref() != Some(&languages);
                                    if changed {
                                        *checked = Some(languages.clone());
                                    }
                                    changed
                                });
                                if changed {
                                    worker.session.set_spell_checking(&languages);
                                }
                                Ok(())
                            }
                            super::BrowserAction::Back => worker.session.go_back(),
                            super::BrowserAction::Forward => worker.session.go_forward(),
                            super::BrowserAction::Reload => worker.session.reload(false),
                            super::BrowserAction::ReloadFresh => worker.session.reload(true),
                            super::BrowserAction::DownloadFolder(folder) => {
                                // The QA runs keep their own folder. A folder
                                // that cannot be made leaves the one in use.
                                if std::env::var_os("ENZIM_QA").is_none() {
                                    let _ = std::fs::create_dir_all(&folder).and_then(|()| {
                                        worker.session.set_download_directory(Some(folder))
                                    });
                                }
                                Ok(())
                            }
                            super::BrowserAction::Mute(muted) => {
                                worker.session.set_muted(muted);
                                Ok(())
                            }
                            super::BrowserAction::Files(id, paths) => {
                                let _ = worker.session.answer_file_chooser(
                                    id,
                                    if paths.is_empty() { None } else { Some(&paths) },
                                );
                                Ok(())
                            }
                            super::BrowserAction::Option(id, choice) => {
                                let _ = worker.session.answer_option_menu(id, choice);
                                Ok(())
                            }
                            super::BrowserAction::Dialog(id, answer) => {
                                let _ = worker.session.answer_script_dialog(id, &answer);
                                Ok(())
                            }
                            super::BrowserAction::TrustCertificate(host) => {
                                worker.trusted_hosts.borrow_mut().insert(host);
                                Ok(())
                            }
                            super::BrowserAction::Authentication(id, answer) => {
                                worker.sign_in_sent = matches!(
                                    answer,
                                    enzui_webkit::page::AuthAnswer::Credentials { .. }
                                ) && worker
                                    .session
                                    .answer_authentication(id, &answer)
                                    .is_ok();
                                Ok(())
                            }
                            super::BrowserAction::Cookies { job } => {
                                worker.session.cookies(move |result| {
                                    job.complete(result.map(crate::native_jobs::Payload::Cookies));
                                });
                                Ok(())
                            }
                            super::BrowserAction::DeleteCookies { keys, job } => {
                                worker.session.delete_cookies(keys, move |result| {
                                    job.complete(result.map(|count| {
                                        crate::native_jobs::Payload::Json(serde_json::json!(count))
                                    }));
                                });
                                Ok(())
                            }
                            super::BrowserAction::ContentFilters { mode, filters, job } => {
                                worker.configure_filters(mode, filters, job);
                                Ok(())
                            }
                            super::BrowserAction::ContentFilter { mode, json, job } => {
                                worker.configure_filters(mode, vec![json], job);
                                Ok(())
                            }
                            super::BrowserAction::LocalOrigins(origins) => {
                                if let Some(list) = worker.session.local_origins() {
                                    list.clear();
                                    for origin in origins {
                                        let _ = list.allow_origin(&origin);
                                    }
                                }
                                Ok(())
                            }
                            super::BrowserAction::PermissionAnswer(id, allow) => {
                                let _ = worker.session.answer_permission(id, allow);
                                Ok(())
                            }
                            super::BrowserAction::PermissionChoices(choices) => {
                                worker.configure_permissions(choices)
                            }
                            super::BrowserAction::NotificationsEnabled(enabled)=>{
                                worker.notifications_enabled=enabled;
                                worker.session.set_notifications_enabled(enabled);
                                Ok(())
                            }
                            super::BrowserAction::NotificationClick(id)=>{let _=worker.session.notification_clicked(id);Ok(())}
                            super::BrowserAction::NotificationClose(id)=>{let _=worker.session.close_notification(id);Ok(())}
                            super::BrowserAction::DeveloperTools(enabled) => {
                                worker.configure_developer_tools(enabled);
                                Ok(())
                            }
                            super::BrowserAction::Privacy(privacy) => {
                                let address = worker
                                    .metadata_snapshot()
                                    .0
                                    .location
                                    .clone()
                                    .unwrap_or_default();
                                privacy.apply_for_site(
                                    &worker.session,
                                    &address,
                                    worker.developer_tools,
                                    &worker.default_user_agent,
                                );
                                worker.privacy = *privacy;
                                Ok(())
                            }
                            super::BrowserAction::ExportHtml { job } => {
                                worker.export_page_job(
                                    enzui_webkit::page_export::PageExportFormat::MainResourceHtml,
                                    job,
                                );
                                Ok(())
                            }
                            super::BrowserAction::ExportMhtml { job } => {
                                worker.export_page_job(
                                    enzui_webkit::page_export::PageExportFormat::Mhtml,
                                    job,
                                );
                                Ok(())
                            }
                            super::BrowserAction::ExportScreenshot { job } => {
                                worker.export_page_job(
                                    enzui_webkit::page_export::PageExportFormat::FullDocumentPng,
                                    job,
                                );
                                Ok(())
                            }
                            super::BrowserAction::ExportPdf { settings, job } => {
                                match worker.session.export_pdf(&settings) {
                                    Ok(id) => worker.pdf_job = Some((id, job)),
                                    Err(error) => job.complete(Err(error.to_string())),
                                }
                                Ok(())
                            }
                            super::BrowserAction::Console { source, job } => {
                                worker.evaluate_job(
                                    &source,
                                    &enzui_webkit::page::ScriptWorld::Page,
                                    job,
                                    true,
                                );
                                Ok(())
                            }
                            super::BrowserAction::CredentialRefresh => {
                                worker.refresh_document_scripts(worker.session.current_uri().ok().flatten().as_deref());
                                worker.configure_credential_document();
                                Ok(())
                            }
                            super::BrowserAction::Credential(fill) => {
                                let super::CredentialFill { lease, site, profile, source, job } = *fill;
                                worker.credential_job(lease, site, profile, source, job);
                                Ok(())
                            }
                            super::BrowserAction::PasskeyReply { frame, source } => {
                                worker.passkey_reply(&frame, &source);
                                Ok(())
                            }
                            super::BrowserAction::Evaluate { source, world, job } => {
                                worker.evaluate_job(&source, &world, job, false);
                                Ok(())
                            }
                            super::BrowserAction::ClearDataJob { data, job } => {
                                let id = worker.session.clear_website_data(data, None);
                                worker.jobs.insert(id, job);
                                Ok(())
                            }
                            super::BrowserAction::ClearData(data) => {
                                worker.session.clear_website_data(data, None);
                                Ok(())
                            }
                            super::BrowserAction::PauseDownload { id, job } => {
                                if worker.session.pause_download(id) {
                                    worker
                                        .download_controls
                                        .insert(id, (job, std::time::Instant::now()));
                                } else {
                                    job.complete(Err("This source cannot be paused safely".into()));
                                }
                                Ok(())
                            }
                            super::BrowserAction::ResumeDownload {
                                url,
                                offset,
                                validator,
                                job,
                            } => {
                                match worker
                                    .session
                                    .start_range_download(&url, offset, &validator)
                                {
                                    Ok(()) => worker.download_requests.push((
                                        url,
                                        job,
                                        std::time::Instant::now(),
                                    )),
                                    Err(error) => job.complete(Err(error)),
                                }
                                Ok(())
                            }
                            super::BrowserAction::CancelRangeDownload { url,job } => {
                                if worker.session.cancel_range_download(&url) {job.complete(Ok(crate::native_jobs::Payload::Json(serde_json::Value::Null)));}else{job.complete(Err("The transfer has already started or was superseded. Retry or cancel the active transfer.".into()));}
                                Ok(())
                            }
                            super::BrowserAction::CancelDownload { id, job } => {
                                if worker.session.cancel_download(id) {
                                    job.complete(Ok(crate::native_jobs::Payload::Json(
                                        serde_json::Value::Null,
                                    )));
                                } else {
                                    job.complete(Err("The download has already ended".into()));
                                }
                                Ok(())
                            }
                            super::BrowserAction::DownloadUrl { url, job } => {
                                let result = worker.session.start_download(&url).map(|()| {
                                    crate::native_jobs::Payload::Json(serde_json::Value::Null)
                                });
                                job.complete(result);
                                Ok(())
                            }
                            super::BrowserAction::Stop => {
                                worker.session.stop_loading();
                                Ok(())
                            }
                            super::BrowserAction::Zoom(zoom) => {
                                worker.zoom = zoom;
                                worker.session.set_page_zoom(zoom);
                                Ok(())
                            }
                            super::BrowserAction::FindFinish => {
                                worker.session.find_finish();
                                Ok(())
                            }
                            super::BrowserAction::Find(text, case) => {
                                if text.is_empty() {
                                    worker.session.find_finish();
                                } else {
                                    worker.session.find(&text, case, 10000);
                                }
                                Ok(())
                            }
                            super::BrowserAction::FindNext(back) => {
                                worker.session.find_next(back);
                                Ok(())
                            }
                            super::BrowserAction::IsolatedScript(source) => {
                                let _ = worker.session.evaluate_script(
                                    &source,
                                    &enzui_webkit::page::ScriptWorld::Isolated(
                                        "enzim-developer-tools".into(),
                                    ),
                                );
                                Ok(())
                            }
                            super::BrowserAction::Script(source) => {
                                let _ = worker.session.evaluate_script(
                                    &source,
                                    &enzui_webkit::page::ScriptWorld::Page,
                                );
                                Ok(())
                            }
                        }
                        .map_err(|error| error.to_string())
                    };
                    let mut update = worker.update(None, false);
                    if let Err(error) = result {
                        super::add_command_error(&mut update, "navigation", error);
                    }
                    update
                }
                Command::Resize { view, size } => {
                    worker.session_size = Some(size);
                    worker.resize(view, size)
                }
                Command::Recreate { router, view, size } => {
                    match worker.recreate(router, view, size) {
                        Ok(()) => worker.update(None, true),
                        Err(error) => NativeUpdate {
                            page_events: Vec::new(),
                            view,
                            report: None,
                            metadata: None,
                            frame: None,
                            gpu_frame: None,
                            tab_reply: None,
                            view_focused: None,
                            error: Some(NativeWorkerError::Command {
                                operation: "recreate",
                                detail: error,
                            }),
                            clipboard: None,
                            selection: None,
                        },
                    }
                }
                Command::Input { event } => {
                    #[cfg(feature = "benchmark")]
                    if let InputEvent::PointerMoved { at, .. } = &event {
                        trace_frame_event("pointer_dispatch", u64::from(at.x.to_bits()));
                    }
                    #[cfg(feature = "benchmark")]
                    if matches!(
                        event,
                        input::InputEvent::WheelScrolled { .. }
                            | input::InputEvent::WheelScrolledAt { .. }
                    ) {
                        trace_frame_event("wheel", 0);
                    }
                    if worker.session.forward_input(&event) {
                        if matches!(event, InputEvent::FocusGained { .. }) {
                            // Settle native focus before a subsequent DOM click can request IME ownership.
                            worker.settle(300);
                            push_update(updates, worker.update(None, true));
                        } else if event.is_terminal() {
                            worker.settle(10);
                            input_frame_due.get_or_insert_with(|| {
                                std::time::Instant::now() + INPUT_FRAME_INTERVAL
                            });
                            push_update(updates, worker.update(None, true));
                        } else {
                            input_frame_due.get_or_insert_with(|| {
                                std::time::Instant::now() + INPUT_FRAME_INTERVAL
                            });
                            // Native callbacks must progress while input defers frame publication.
                            worker.settle(1);
                        }
                    }
                    return true;
                }
                Command::Click { view, x, y } => {
                    if let Some(mapping) = ViewportMapping::new(0.0, 0.0, 1024.0, 768.0, 1.0) {
                        if let Some(down) = input::pointer_pressed_with_modifiers(
                            view,
                            &mapping,
                            x,
                            y,
                            PointerButton::Primary,
                            input::Modifiers::BUTTON1,
                        ) {
                            let _ = worker.session.forward_input(&down);
                        }
                        if let Some(up) = input::pointer_released_with_modifiers(
                            view,
                            &mapping,
                            x,
                            y,
                            PointerButton::Primary,
                            input::Modifiers::NONE,
                        ) {
                            let _ = worker.session.forward_input(&up);
                        }
                        worker.settle(300);
                    }
                    worker.update(None, true)
                }
                Command::Hover { view, x, y } => {
                    if let Some(mapping) = ViewportMapping::new(0.0, 0.0, 1024.0, 768.0, 1.0)
                        && let Some(mv) = input::pointer_moved_with_modifiers(
                            view,
                            &mapping,
                            x,
                            y,
                            input::Modifiers::NONE,
                        )
                    {
                        let _ = worker.session.forward_input(&mv);
                        worker.settle(300);
                    }
                    worker.update(None, false)
                }
                Command::Apply { view, effect } => {
                    worker.edited = true;
                    let error = worker
                        .session
                        .apply_ime_effect(&worker.router, view, &effect)
                        .err()
                        .map(|e| e.to_string());
                    if error.is_none() {
                        worker.settle(300);
                    }
                    // A refused edit leaves the page as it was: the page may
                    // have moved focus off the field (a form submitting) before
                    // the edit arrived. It is not a page failure, which would
                    // replace the page with the error view.
                    if let Some(detail) = &error {
                        crate::credentials::credential_trace(format_args!(
                            "text input refused: {detail}"
                        ));
                    }
                    worker.update(None, error.is_none())
                }
                Command::CopyToHost { view, gesture } => worker.clipboard(view, gesture, None),
                Command::PasteFromHost {
                    view,
                    gesture,
                    text,
                } => worker.clipboard(view, gesture, Some(&text)),
                Command::Visibility { view, visible } => worker.set_visibility(view, visible),
                Command::FocusOut { view } => {
                    let event = InputEvent::FocusLost {
                        view,
                        modifiers: input::Modifiers::NONE,
                    };
                    let _ = worker.session.forward_input(&event);
                    worker.settle(10);
                    worker.update(None, false)
                }
            };
            push_update(updates, result);
            true
        }
    }
}

#[cfg(not(feature = "wpe"))]
mod real {
    use super::{NativeOpenError, NativeUpdate};
    use enzui_core::media::Generation as MediaGeneration;
    use enzui_webkit::clipboard_route::HostGesture;
    use enzui_webkit::ime_bridge::ImeEffect;
    use enzui_webkit::web::{Generation as FocusGeneration, ViewId, ViewRouter};
    #[cfg(test)]
    use enzui_webkit::worker::{CommandWorker, TeardownOutcome};

    use crate::webview_scratch::SecureScratch;

    const NO_FEATURE: &str = "no native backend: this probe was built without \
                              `--features wpe`, so `mode::resolve(Native)` is \
                              Err(NativeUnavailable) and there is no real page";

    pub(crate) struct NativePage {
        #[cfg(not(test))]
        _never: core::convert::Infallible,
        #[cfg(test)]
        worker: CommandWorker<()>,
        #[cfg(test)]
        teardown_joined: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
        #[cfg(test)]
        teardown_timed_out: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    }

    impl core::fmt::Debug for NativePage {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str("NativePage(<no wpe feature>)")
        }
    }

    impl NativePage {
        pub(crate) fn browse(
            &self,
            _router: &ViewRouter,
            _view: ViewId,
            _action: super::BrowserAction,
        ) -> Result<(), String> {
            Err(NO_FEATURE.into())
        }

        pub(crate) fn set_visibility(&self, _router: &ViewRouter, _view: ViewId, _visible: bool) {}

        #[cfg(test)]
        pub(crate) fn teardown_probe(
            scratch: std::sync::Arc<SecureScratch>,
        ) -> (
            Self,
            std::sync::Arc<std::sync::atomic::AtomicBool>,
            std::sync::Arc<std::sync::atomic::AtomicBool>,
        ) {
            use std::sync::Arc;
            use std::sync::atomic::{AtomicBool, Ordering};
            use std::time::Duration;

            let joined = Arc::new(AtomicBool::new(false));
            let saw_scratch = Arc::new(AtomicBool::new(false));
            let saw_scratch_in_worker = Arc::clone(&saw_scratch);
            let worker = CommandWorker::spawn(
                "enzui-no-wpe-close-probe",
                1,
                Duration::from_millis(500),
                move |rx| {
                    while rx.next().is_some() {}
                    saw_scratch_in_worker.store(scratch.path().exists(), Ordering::Release);
                },
            )
            .expect("spawn feature-off close probe");
            (
                Self {
                    worker,
                    teardown_joined: Some(Arc::clone(&joined)),
                    teardown_timed_out: None,
                },
                joined,
                saw_scratch,
            )
        }

        #[cfg(test)]
        pub(crate) fn teardown_timeout_probe(
            scratch: std::sync::Arc<SecureScratch>,
        ) -> (
            Self,
            std::sync::mpsc::Sender<()>,
            std::sync::Arc<std::sync::atomic::AtomicBool>,
            std::sync::Arc<std::sync::atomic::AtomicBool>,
        ) {
            use std::sync::Arc;
            use std::sync::atomic::{AtomicBool, Ordering};
            use std::sync::mpsc;
            use std::time::Duration;

            let (release_tx, release_rx) = mpsc::channel();
            let (ready_tx, ready_rx) = mpsc::channel();
            let finished = Arc::new(AtomicBool::new(false));
            let timed_out = Arc::new(AtomicBool::new(false));
            let finished_in_worker = Arc::clone(&finished);
            let worker = CommandWorker::spawn(
                "enzui-no-wpe-close-timeout-probe",
                1,
                Duration::from_millis(5),
                move |_rx| {
                    let _ = ready_tx.send(());
                    let _ = release_rx.recv();
                    debug_assert!(scratch.path().exists());
                    finished_in_worker.store(true, Ordering::Release);
                },
            )
            .expect("spawn feature-off close timeout probe");
            ready_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("feature-off close timeout probe started");
            (
                Self {
                    worker,
                    teardown_joined: None,
                    teardown_timed_out: Some(Arc::clone(&timed_out)),
                },
                release_tx,
                finished,
                timed_out,
            )
        }

        // Always refuses, naming the missing feature.
        pub(crate) fn open(
            _router: &ViewRouter,
            _view: ViewId,
            _scratch: std::sync::Arc<SecureScratch>,
            _initial_size: Option<enzui_webkit::ViewportSize>,
            _initial_generation: MediaGeneration,
        ) -> Result<Self, NativeOpenError> {
            let _ = NO_FEATURE;
            Err(NativeOpenError::BackendUnavailable)
        }

        pub(crate) fn open_with_profile(
            router: &ViewRouter,
            view: ViewId,
            scratch: std::sync::Arc<SecureScratch>,
            initial_size: Option<enzui_webkit::ViewportSize>,
            initial_generation: MediaGeneration,
            _profile: enzui_webkit::web::WebProfile,
        ) -> Result<Self, NativeOpenError> {
            Self::open(router, view, scratch, initial_size, initial_generation)
        }

        pub(crate) fn open_adopted_with_profile(
            router: &ViewRouter,
            view: ViewId,
            scratch: std::sync::Arc<SecureScratch>,
            (initial_size, initial_generation): (Option<enzui_webkit::ViewportSize>, MediaGeneration),
            _profile: enzui_webkit::web::WebProfile,
            _popup: u64,
        ) -> Result<Self, NativeOpenError> {
            Self::open(router, view, scratch, initial_size, initial_generation)
        }

        pub(crate) fn open_reserved_with_profile(
            router: &ViewRouter,
            view: ViewId,
            scratch: std::sync::Arc<SecureScratch>,
            initial_size: Option<enzui_webkit::ViewportSize>,
            initial_generation: MediaGeneration,
            _profile: enzui_webkit::web::WebProfile,
        ) -> Result<Self, NativeOpenError> {
            Self::open(router, view, scratch, initial_size, initial_generation)
        }

        pub(crate) fn recreate(
            self,
            _router: &ViewRouter,
            _view: ViewId,
            _size: Option<enzui_webkit::ViewportSize>,
        ) -> Result<Self, String> {
            Err(NO_FEATURE.to_owned())
        }

        // Typed native-unavailable refusal preserved: no viewport accepted for a
        // mock page.
        pub(crate) fn resize(
            &self,
            _router: &ViewRouter,
            _view: ViewId,
            _size: enzui_webkit::ViewportSize,
        ) -> Result<(), String> {
            Err(NO_FEATURE.to_owned())
        }

        pub(crate) fn take_updates(&self) -> Vec<NativeUpdate> {
            Vec::new()
        }

        // No stub counter: real-page-only, sole caller wpe-gated — a stub would be
        // dead code.

        pub(crate) fn click(&self, _router: &ViewRouter, _view: ViewId, _x: f32, _y: f32) {}

        pub(crate) fn hover(&self, _view: ViewId, _x: f32, _y: f32) {}

        // Always refuses.
        pub(crate) fn forward_input(
            &self,
            _router: &ViewRouter,
            _event: &enzui_webkit::input::InputEvent,
        ) -> bool {
            false
        }

        pub(crate) fn focus_out(&self, _view: ViewId) {}

        // Always refuses: no page to copy from.
        pub(crate) fn copy_to_host(
            &self,
            _router: &ViewRouter,
            _view: ViewId,
            _gesture: HostGesture,
        ) -> Result<(), String> {
            Err(NO_FEATURE.to_owned())
        }

        // Always refuses: no page to paste into.
        pub(crate) fn paste_from_host(
            &self,
            _router: &ViewRouter,
            _view: ViewId,
            _gesture: HostGesture,
            _text: &str,
        ) -> Result<(), String> {
            Err(NO_FEATURE.to_owned())
        }

        pub(crate) fn apply(
            &self,
            _router: &ViewRouter,
            _view: ViewId,
            _effect: &ImeEffect,
        ) -> Result<(), String> {
            Err(NO_FEATURE.to_owned())
        }
    }

    #[cfg(test)]
    impl Drop for NativePage {
        fn drop(&mut self) {
            use std::sync::atomic::Ordering;

            let outcome = self.worker.shutdown();
            if let Some(joined) = &self.teardown_joined {
                joined.store(outcome.is_joined(), Ordering::Release);
            }
            if let Some(timed_out) = &self.teardown_timed_out {
                timed_out.store(outcome == TeardownOutcome::TimedOut, Ordering::Release);
            }
            if !outcome.is_joined() {
                debug_assert_eq!(outcome, TeardownOutcome::TimedOut);
            }
        }
    }
}

pub(crate) use real::NativePage;

#[cfg(feature = "benchmark")]
pub(crate) fn run_storage_gate(scratch: std::path::PathBuf) -> Result<serde_json::Value, String> {
    real::run_storage_gate(scratch)
}

pub(crate) fn main_thread_subscription() -> iced::Subscription<()> {
    #[cfg(feature = "wpe")]
    if real::main_dispatch_needed() {
        if let Some(subscription) = real::native_subscription() {
            return subscription;
        }
        if real::fallback_dispatch_needed() {
            return iced::time::every(real::main_dispatch_interval()).map(|_| ());
        }
    }
    iced::Subscription::none()
}

pub(crate) fn pump_main_thread() {
    #[cfg(feature = "wpe")]
    real::pump_main_thread();
}

/// How readily hidden tabs give back memory (Settings → Memory).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum MemoryMode {
    /// Every hidden tab releases its caches after 15 seconds.
    #[default]
    Aggressive,
    /// The 5 most recently shown hidden tabs wait a minute, others 15 seconds.
    Balanced,
    /// The 10 most recently shown wait 5 minutes, others a minute; under
    /// memory pressure tabs are discarded after 30 minutes rather than 10.
    Relaxed,
}

impl MemoryMode {
    pub(crate) const ALL: [Self; 3] = [Self::Aggressive, Self::Balanced, Self::Relaxed];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Aggressive => "Aggressive",
            Self::Balanced => "Balanced",
            Self::Relaxed => "Relaxed",
        }
    }
}

/// The mode the native memory policy follows; set from the UI thread, read on
/// its next tick.
static MEMORY_MODE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

pub(crate) fn set_memory_mode(mode: MemoryMode) {
    let index = MemoryMode::ALL.iter().position(|m| *m == mode).unwrap_or(0);
    MEMORY_MODE.store(u8::try_from(index).unwrap_or(0), std::sync::atomic::Ordering::Relaxed);
}

fn memory_mode() -> MemoryMode {
    MemoryMode::ALL
        .get(usize::from(MEMORY_MODE.load(std::sync::atomic::Ordering::Relaxed)))
        .copied()
        .unwrap_or_default()
}

#[cfg(test)]
thread_local! {
    /// Lets one test observe memory pressure without changing it for the
    /// tests running beside it.
    pub(crate) static TEST_MEMORY_PRESSURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether the latest memory check found no pressure. Without a native engine
/// there is nothing to measure, so the answer is always yes.
pub(crate) fn memory_pressure_normal() -> bool {
    #[cfg(test)]
    if TEST_MEMORY_PRESSURE.with(std::cell::Cell::get) {
        return false;
    }
    #[cfg(feature = "wpe")]
    {
        real::memory_pressure_normal()
    }
    #[cfg(not(feature = "wpe"))]
    {
        true
    }
}

#[cfg(feature = "benchmark")]
pub(crate) fn run_metadata_benchmark(scratch: std::path::PathBuf) -> Result<(), String> {
    real::run_metadata_benchmark(scratch)
}

#[cfg(feature = "benchmark")]
pub(crate) fn run_metadata_gate(scratch: std::path::PathBuf) -> Result<(), String> {
    real::run_metadata_gate(scratch)
}

pub(crate) fn main_thread_dispatcher<'a, Message: 'a>(
    dispatch: impl Fn(bool) -> Option<Message> + 'a,
) -> Option<iced::Element<'a, Message>> {
    #[cfg(feature = "wpe")]
    return real::dispatcher(dispatch);
    #[cfg(not(feature = "wpe"))]
    {
        let _ = dispatch;
        None
    }
}

#[cfg(all(test, feature = "wpe"))]
#[test]
fn native_dispatch_suppresses_only_its_owner_worker_wake() {
    let (tx, mut receiver) = iced::futures::channel::mpsc::channel(1);
    let sender = std::sync::Arc::new(std::sync::Mutex::new(tx));
    OWNER_DISPATCH.with(|active| active.set(true));
    send_native_wake(&sender);
    let own_wake = receiver.try_recv();
    OWNER_DISPATCH.with(|active| active.set(false));
    assert!(own_wake.is_err());
    OWNER_DISPATCH.with(|active| active.set(true));
    let other = std::sync::Arc::clone(&sender);
    std::thread::spawn(move || send_native_wake(&other))
        .join()
        .unwrap();
    OWNER_DISPATCH.with(|active| active.set(false));
    receiver.try_recv().expect("worker wake must be delivered");
    send_native_wake(&sender);
    receiver.try_recv().expect("worker wake must be delivered");
}

#[cfg(all(test, feature = "wpe"))]
#[test]
fn a_held_wake_reaches_the_window_whose_update_is_still_queued() {
    use std::sync::atomic::Ordering;
    let (tx, mut receiver) = iced::futures::channel::mpsc::channel(1);
    let sender = std::sync::Arc::new(std::sync::Mutex::new(tx));
    // One window's dispatch steps another window's page, which publishes.
    OWNER_DISPATCH.with(|active| active.set(true));
    QUEUED_UPDATES.fetch_add(1, Ordering::AcqRel);
    send_native_wake(&sender);
    OWNER_DISPATCH.with(|active| active.set(false));
    assert!(receiver.try_recv().is_err(), "the owner's own wake is held");
    wake_for_other_windows_with(&sender);
    receiver
        .try_recv()
        .expect("the other window must be woken for its queued update");
    // Once taken, nothing is queued and nothing more wakes.
    QUEUED_UPDATES.fetch_sub(1, Ordering::AcqRel);
    OWNER_DISPATCH.with(|active| active.set(true));
    send_native_wake(&sender);
    OWNER_DISPATCH.with(|active| active.set(false));
    wake_for_other_windows_with(&sender);
    assert!(receiver.try_recv().is_err(), "no update waits, no wake");
}

pub(crate) fn live_gpu_source_enabled() -> bool {
    match std::env::var_os("ENZUI_PROBE_LIVE_GPU") {
        Some(value) => value == "1",
        None => {
            #[cfg(feature = "wpe")]
            return real::dispatcher_available();
            #[cfg(not(feature = "wpe"))]
            false
        }
    }
}

#[cfg(feature = "benchmark")]
pub(crate) fn run_browser_feature_gate(
    scratch: std::path::PathBuf,
) -> Result<serde_json::Value, String> {
    real::run_browser_feature_gate(scratch)
}

#[cfg(feature = "wpe")]
pub(crate) fn release_profiles(ids: Vec<enzui_webkit::web::profile::ProfileId>) {
    real::release_profiles(ids);
}

#[cfg(feature = "benchmark")]
pub(crate) fn run_followup_gate(scratch: std::path::PathBuf) -> Result<serde_json::Value, String> {
    real::run_followup_gate(scratch)
}

#[cfg(feature = "benchmark")]
pub(crate) fn run_latency_gate(scratch: std::path::PathBuf) -> Result<serde_json::Value, String> {
    real::run_latency_gate(scratch)
}

#[cfg(feature = "benchmark")]
pub(crate) fn run_volume_gate(scratch: std::path::PathBuf) -> Result<serde_json::Value, String> {
    real::run_volume_gate(scratch)
}

#[cfg(feature = "benchmark")]
pub(crate) fn run_assets_gate(scratch: std::path::PathBuf) -> Result<serde_json::Value, String> {
    real::run_assets_gate(scratch)
}

#[cfg(feature = "benchmark")]
pub(crate) fn run_privacy_gate(scratch: std::path::PathBuf) -> Result<serde_json::Value, String> {
    real::run_privacy_gate(scratch)
}

#[cfg(feature = "benchmark")]
pub(crate) fn run_download_gate(scratch: std::path::PathBuf) -> Result<serde_json::Value, String> {
    real::run_download_gate(scratch)
}

#[cfg(feature = "benchmark")]
pub(crate) fn run_theme_gate(scratch: std::path::PathBuf) -> Result<serde_json::Value, String> {
    real::run_theme_gate(scratch)
}

#[cfg(all(feature = "wpe", feature = "benchmark"))]
pub(crate) fn run_credentials_gate(
    scratch: std::path::PathBuf,
) -> Result<serde_json::Value, String> {
    real::run_credentials_gate(scratch)
}

#[cfg(all(feature = "wpe", feature = "benchmark"))]
pub(crate) fn run_passkeys_gate(scratch: std::path::PathBuf) -> Result<serde_json::Value, String> {
    real::run_passkeys_gate(scratch)
}

#[cfg(feature = "benchmark")]
pub(crate) fn run_devtools_gate(scratch: std::path::PathBuf) -> Result<serde_json::Value, String> {
    real::run_devtools_gate(scratch)
}
#[cfg(feature = "benchmark")]
pub(crate) fn run_youtube_gate(scratch: std::path::PathBuf) -> Result<serde_json::Value, String> {
    real::run_youtube_gate(scratch)
}

#[cfg(feature = "benchmark")]
pub(crate) fn run_encrypted_media_gate(
    scratch: std::path::PathBuf,
) -> Result<serde_json::Value, String> {
    real::run_encrypted_media_gate(scratch)
}

#[cfg(all(feature = "benchmark", feature = "wpe"))]
pub(crate) fn run_opening_gate() -> Result<serde_json::Value, String> {
    real::run_opening_gate()
}
