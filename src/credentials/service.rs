use super::{Credential, CredentialProvider, Result, Secret, Site, provider::Summary};
use enzui_core::file_tree_watch::{
    BoxHandle, DirectoryChange, EventSink, RawEvent, RawEventKind, WatchToken,
};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};
use tokio::sync::{oneshot, watch};

#[derive(Clone, Debug)]
pub(crate) enum Operation {
    #[cfg(test)]
    Open {
        password: Secret,
        create: bool,
    },
    OpenSource {
        source: String,
        authentication: super::accounts::Authentication,
        create: bool,
    },
    OpenDesktop {
        manual: bool,
    },
    ForgetDesktop,
    Synchronize,
    List(Option<Site>),
    ListSource(String, Option<Site>),
    Read(String, Option<Site>),
    ReadSelected(String),
    /// A submitted login, and the saved login filled into its form, if any:
    /// a login chosen for another website is still the one submitted.
    CheckSubmission(Credential, Option<String>),
    Save(Credential),
    SaveTo(String, Credential),
    Delete(String),
    Import(String, Vec<Credential>),
    Backup {
        current: Secret,
        password: Secret,
    },
    Csv(Secret),
    Rekey {
        current: Secret,
        password: Secret,
    },
}
#[derive(Clone, Debug)]
pub(crate) enum Output {
    Done,
    Opened { desktop: bool },
    DesktopOpened(bool),
    Entries(Vec<Summary>),
    Entry(Credential),
    SubmissionChecked { id: String, known: bool },
    Imported(usize),
    Data(Arc<enzui_security::secret::Secret>),
}
#[derive(Clone, Debug)]
pub(crate) struct Reply {
    pub(crate) generation: u64,
    pub(crate) unlocked: bool,
    pub(crate) authority: Vec<(PathBuf, enzui_core::state_authority::StateIdentity)>,
    pub(crate) result: Result<Output>,
}
enum WorkerEvent {
    Request(Box<Request>),
    Changed,
    Touch(String),
}

struct ChangeSink {
    paths: Vec<PathBuf>,
    settings_path: PathBuf,
    active_process: super::connector::ActiveProcess,
    active_keyring: super::desktop::ActiveOperation,
    sender: mpsc::SyncSender<WorkerEvent>,
    pending: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
}
impl EventSink for ChangeSink {
    fn push(&self, event: RawEvent) {
        let watch_failed = matches!(
            &event.kind,
            RawEventKind::Overflow | RawEventKind::Vanished | RawEventKind::ProbeFailed(_)
        );
        let config_changed = match &event.kind {
            RawEventKind::Change(
                DirectoryChange::Created(path)
                | DirectoryChange::Deleted(path)
                | DirectoryChange::Modified(path),
            ) => path == &self.settings_path,
            RawEventKind::Change(DirectoryChange::Renamed { from, to }) => {
                from == &self.settings_path || to == &self.settings_path
            }
            _ => true,
        };
        if config_changed {
            let process = self
                .active_process
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(active) = &*process
                && (watch_failed
                    || active.settings.as_ref().is_some_and(|(path, identity)| {
                        !enzui_core::state_authority::StateAuthority::open_existing(path)
                            .is_ok_and(|current| current.identity() == Some(*identity))
                    }))
            {
                active
                    .token
                    .cancel(enzui_security::cancellation::CancelCause::Cancelled);
                if let Some(keyring) = self
                    .active_keyring
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .as_ref()
                {
                    keyring.cancel();
                }
            }
        }
        let relevant = match event.kind {
            RawEventKind::Change(
                DirectoryChange::Created(path)
                | DirectoryChange::Deleted(path)
                | DirectoryChange::Modified(path),
            ) => self.paths.contains(&path),
            RawEventKind::Change(DirectoryChange::Renamed { from, to }) => {
                self.paths.contains(&from) || self.paths.contains(&to)
            }
            RawEventKind::Overflow => true,
            RawEventKind::Vanished | RawEventKind::ProbeFailed(_) => {
                self.failed.store(true, Ordering::Release);
                true
            }
        };
        if relevant && !self.pending.swap(true, Ordering::AcqRel) {
            let _ = self.sender.try_send(WorkerEvent::Changed);
        }
    }
}

#[derive(Clone)]
struct ChangeFeed {
    id: u64,
    receiver: watch::Receiver<(u64, bool)>,
}
impl std::hash::Hash for ChangeFeed {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}
fn changes(feed: &ChangeFeed) -> impl iced::futures::Stream<Item = ()> + Send + 'static + use<> {
    iced::futures::stream::unfold(feed.receiver.clone(), |mut receiver| async {
        receiver.changed().await.ok()?;
        let _ = *receiver.borrow_and_update();
        Some(((), receiver))
    })
}

fn install_watches(paths: &[PathBuf], sink: Arc<dyn EventSink>) -> Result<Vec<BoxHandle>> {
    let backend = enzui_core::file_tree_watch::native_directory_watch()
        .ok_or("Native credential change notifications are unavailable.")?;
    let mut parents = Vec::new();
    let mut handles = Vec::new();
    for path in paths {
        let parent = path
            .parent()
            .ok_or("Invalid credential storage directory.")?;
        if parents.contains(&parent) {
            continue;
        }
        enzui_security::secure_dir::ensure_private_dir(parent)
            .map_err(|_| "Could not open the private credential directory.".to_owned())?;
        handles.push(
            backend
                .watch(parent, WatchToken::standalone(), Arc::clone(&sink))
                .map_err(|_| "Could not watch credential changes safely.".to_owned())?,
        );
        parents.push(parent);
    }
    Ok(handles)
}

struct Request {
    generation: u64,
    operation: Operation,
    answer: oneshot::Sender<Reply>,
}

#[derive(Clone)]
pub(crate) struct Lease {
    generation: Arc<AtomicU64>,
    expected: u64,
    unlocked: Arc<AtomicBool>,
    bindings: Vec<(PathBuf, enzui_core::state_authority::StateIdentity)>,
    expires: std::time::Instant,
}
impl Lease {
    pub(crate) fn expires_at(&self) -> std::time::Instant {
        self.expires
    }
    pub(crate) fn valid(&self) -> bool {
        self.unlocked.load(Ordering::Acquire)
            && self.generation.load(Ordering::Acquire) == self.expected
            && std::time::Instant::now() < self.expires
    }
}
impl Lease {
    pub(crate) fn authorize(&self) -> bool {
        self.valid()
            && !self.bindings.is_empty()
            && self.bindings.iter().all(|(path, identity)| {
                enzui_core::state_authority::StateAuthority::open_existing(path)
                    .is_ok_and(|authority| authority.identity() == Some(*identity))
            })
    }
}
impl std::fmt::Debug for Lease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CredentialLease([redacted])")
    }
}
struct RevokeOnExit {
    notifications: watch::Sender<(u64, bool)>,
    status: Arc<AtomicBool>,
    epoch: Arc<AtomicU64>,
}
impl Drop for RevokeOnExit {
    fn drop(&mut self) {
        self.status.store(false, Ordering::Release);
        let epoch = self.epoch.fetch_add(1, Ordering::AcqRel) + 1;
        self.notifications.send_replace((epoch, false));
    }
}
#[derive(Clone, Default)]
struct ProviderState {
    #[cfg(any(test, feature = "benchmark"))]
    bindings: Vec<(PathBuf, enzui_core::state_authority::StateIdentity)>,
    sources: Vec<String>,
}
fn publish_status(
    provider: &mut dyn CredentialProvider,
    status: &AtomicBool,
    epoch: &AtomicU64,
    identity: &Mutex<ProviderState>,
    revision: &AtomicU64,
    notifications: &watch::Sender<(u64, bool)>,
    lock_requested: &AtomicBool,
) -> bool {
    let provider_open = provider.unlocked();
    let mut identity = identity
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let open = provider_open && !lock_requested.load(Ordering::Acquire);
    *identity = ProviderState {
        #[cfg(any(test, feature = "benchmark"))]
        bindings: provider.authorization_bindings(),
        sources: provider.open_sources(),
    };
    let previous = status.swap(open, Ordering::AcqRel);
    let changed =
        revision.swap(provider.revocation_epoch(), Ordering::AcqRel) != provider.revocation_epoch();
    if previous && (!open || changed) {
        epoch.fetch_add(1, Ordering::AcqRel);
    }
    if previous != open || (previous && changed) {
        notifications.send_replace((epoch.load(Ordering::Acquire), open));
    }
    open
}
pub(crate) struct Service {
    active_keyring: super::desktop::ActiveOperation,
    active_process: super::connector::ActiveProcess,
    identity: Arc<Mutex<ProviderState>>,
    sender: mpsc::SyncSender<WorkerEvent>,
    stopped: Arc<AtomicBool>,
    lock_requested: Arc<AtomicBool>,
    automatic_unlock_blocked: Arc<AtomicBool>,
    notifications: watch::Sender<(u64, bool)>,
    feed: ChangeFeed,
    #[cfg(test)]
    cycles: Arc<AtomicU64>,
    generation: Arc<AtomicU64>,
    unlocked: Arc<AtomicBool>,
}
type SharedServices = std::collections::BTreeMap<(PathBuf, PathBuf), Weak<Service>>;
impl Service {
    pub(crate) fn shared(path: PathBuf, settings_path: PathBuf) -> Result<Arc<Self>> {
        static SHARED: OnceLock<Mutex<SharedServices>> = OnceLock::new();
        let mut services = SHARED
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        services.retain(|_, service| service.strong_count() > 0);
        let key = (path.clone(), settings_path.clone());
        if let Some(service) = services.get(&key).and_then(Weak::upgrade) {
            return Ok(service);
        }
        let service = Arc::new(Self::start(path, settings_path)?);
        services.insert(key, Arc::downgrade(&service));
        Ok(service)
    }
    pub(crate) fn start(path: PathBuf, settings_path: PathBuf) -> Result<Self> {
        let vault = path.clone();
        let settings = settings_path.clone();
        Self::start_with_factory(path, settings_path, move |active, process| {
            Box::new(super::router::Router::new(vault, settings, active, process))
        })
    }
    #[cfg(test)]
    pub(crate) fn start_with_store(
        path: PathBuf,
        settings_path: PathBuf,
        store: Arc<dyn enzui_services::credential_store::CredentialStore>,
    ) -> Result<Self> {
        let vault = path.clone();
        Self::start_with_factory(path, settings_path, move |active, _process| {
            Box::new(super::Builtin::with_desktop_store_cancellation(
                vault, store, active,
            ))
        })
    }
    fn start_with_factory(
        path: PathBuf,
        settings_path: PathBuf,
        factory: impl FnOnce(
            super::desktop::ActiveOperation,
            super::connector::ActiveProcess,
        ) -> Box<dyn CredentialProvider>
        + Send
        + 'static,
    ) -> Result<Self> {
        let active_keyring = Arc::new(Mutex::new(None));
        let worker_keyring = Arc::clone(&active_keyring);
        let active_process = Arc::new(Mutex::new(None));
        let worker_process = Arc::clone(&active_process);
        let process_requests = Arc::clone(&active_process);
        let (sender, receiver) = mpsc::sync_channel::<WorkerEvent>(8);
        let pending = Arc::new(AtomicBool::new(false));
        let failed = Arc::new(AtomicBool::new(false));
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&stopped);
        let paths = vec![
            super::system::revision_path(&path),
            path,
            settings_path.clone(),
        ];
        let watches = install_watches(
            &paths,
            Arc::new(ChangeSink {
                paths: paths.clone(),
                settings_path: settings_path.clone(),
                active_process: Arc::clone(&active_process),
                active_keyring: Arc::clone(&active_keyring),
                sender: sender.clone(),
                pending: Arc::clone(&pending),
                failed: Arc::clone(&failed),
            }),
        )?;
        let (notifications, notifications_rx) = watch::channel((1, false));
        let status_notifications = notifications.clone();
        let lock_requested = Arc::new(AtomicBool::new(false));
        let worker_lock_requested = Arc::clone(&lock_requested);
        let automatic_unlock_blocked = Arc::new(AtomicBool::new(false));
        let worker_automatic_unlock_blocked = Arc::clone(&automatic_unlock_blocked);
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let feed = ChangeFeed {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            receiver: notifications_rx,
        };
        #[cfg(test)]
        let cycles = Arc::new(AtomicU64::new(0));
        #[cfg(test)]
        let worker_cycles = Arc::clone(&cycles);
        let generation = Arc::new(AtomicU64::new(1));
        let unlocked = Arc::new(AtomicBool::new(false));
        let identity = Arc::new(Mutex::new(ProviderState::default()));
        let current_identity = Arc::clone(&identity);
        let revision = AtomicU64::new(0);
        let epoch = Arc::clone(&generation);
        let status = Arc::clone(&unlocked);
        std::thread::Builder::new()
            .name("credential-vault".into())
            .spawn(move || {
                let _watches = watches;
                let mut provider = factory(worker_keyring, worker_process);
                let _revoke = RevokeOnExit {
                    notifications: notifications.clone(),
                    status: Arc::clone(&status),
                    epoch: Arc::clone(&epoch),
                };
                loop {
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                    if worker_lock_requested.swap(false, Ordering::AcqRel) {
                        provider.lock();
                        status.store(false, Ordering::Release);
                    }
                    pending.store(false, Ordering::Release);
                    #[cfg(test)]
                    worker_cycles.fetch_add(1, Ordering::Relaxed);
                    if !failed.load(Ordering::Acquire)
                        && let Ok(settings) = super::config::load(&settings_path)
                        && settings.enabled
                        && settings.provider == "enzim.builtin"
                    {
                        provider.timeout(settings.lock_minutes);
                        provider.desktop_enabled(settings.desktop_unlock);
                    } else {
                        provider.lock();
                    }
                    let open = publish_status(
                        provider.as_mut(), &status, &epoch, &current_identity, &revision, &notifications, &worker_lock_requested,
                    );
                    let deadline = open.then(|| provider.idle_deadline()).flatten();
                    let event = if let Some(deadline) = deadline {
                        match receiver.recv_timeout(
                            deadline.saturating_duration_since(std::time::Instant::now()),
                        ) {
                            Ok(event) => event,
                            Err(mpsc::RecvTimeoutError::Timeout) => continue,
                            Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        }
                    } else {
                        match receiver.recv() {
                            Ok(event) => event,
                            Err(_) => break,
                        }
                    };
                    let request = match event {
                        WorkerEvent::Request(request) => request,
                        WorkerEvent::Touch(source) => {
                            let _ = provider.touch_source(&source);
                            continue;
                        }
                        WorkerEvent::Changed => continue,
                    };
                    let request = *request;
                    if worker_lock_requested.swap(false, Ordering::AcqRel) {
                        provider.lock();
                        status.store(false, Ordering::Release);
                    }
                    if failed.load(Ordering::Acquire) {
                        provider.lock();
                        publish_status(provider.as_mut(), &status, &epoch, &current_identity, &revision, &notifications, &worker_lock_requested);
                        let _ = request.answer.send(Reply { generation: request.generation, unlocked: false, authority: Vec::new(), result: Err("Credential monitoring stopped. Turn the password manager off and on to unlock again.".into()) });
                        continue;
                    }
                    if request.generation != epoch.load(Ordering::Acquire) {
                        provider.lock();
                        status.store(false, Ordering::Release);
                        let _ = request.answer.send(Reply {
                            generation: request.generation,
                            unlocked: false,
                            authority: Vec::new(),
                            result: Err("The credential request was cancelled.".into()),
                        });
                        continue;
                    }
                    if !matches!(request.operation, Operation::ForgetDesktop)
                        && !super::config::load(&settings_path).is_ok_and(|settings| {
                            settings.enabled && settings.provider == "enzim.builtin"
                        })
                    {
                        provider.lock();
                        status.store(false, Ordering::Release);
                        let _ = request.answer.send(Reply {
                            generation: request.generation,
                            unlocked: false,
                            authority: Vec::new(),
                            result: Err("The built-in credential provider is disabled.".into()),
                        });
                        continue;
                    }
                    let process_token = enzui_security::cancellation::CancelToken::new();
                    let authority = enzui_core::state_authority::StateAuthority::open_existing(&settings_path)
                        .ok().and_then(|authority| authority.identity().map(|identity| (settings_path.clone(), identity)));
                    *process_requests.lock().unwrap_or_else(std::sync::PoisonError::into_inner) =
                        Some(super::connector::ActiveRequest {
                            token: process_token.share(),
                            deadline: std::time::Instant::now() + Duration::from_secs(90),
                            settings: authority,
                        });
                    if request.generation != epoch.load(Ordering::Acquire) {
                        process_token.cancel(enzui_security::cancellation::CancelCause::Cancelled);
                    }
                    let automatic_unlock = matches!(request.operation, Operation::OpenDesktop { manual: false });
                    let mut result = if process_token.cause().is_some() {
                        Err("The credential request was cancelled.".into())
                    } else if automatic_unlock && worker_automatic_unlock_blocked.load(Ordering::Acquire) {
                        Ok(Output::DesktopOpened(false))
                    } else {
                        execute(provider.as_mut(), request.operation)
                    };
                    process_requests.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
                    if request.generation != epoch.load(Ordering::Acquire)
                        || !super::config::load(&settings_path).is_ok_and(|settings| {
                            settings.enabled && settings.provider == "enzim.builtin"
                        })
                    {
                        provider.lock();
                        result = Err(
                            "The credential request was cancelled or its provider was disabled."
                                .into(),
                        );
                    }
                    if matches!(&result, Ok(Output::Opened { .. } | Output::DesktopOpened(true))) {
                        let _identity = current_identity.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                        if request.generation == epoch.load(Ordering::Acquire) {
                            worker_automatic_unlock_blocked.store(false, Ordering::Release);
                        }
                    }
                    let open = publish_status(
                        provider.as_mut(), &status, &epoch, &current_identity, &revision, &notifications, &worker_lock_requested,
                    );
                    let secret_reply = matches!(&result, Ok(Output::Entries(_) | Output::Entry(_) | Output::Data(_) | Output::SubmissionChecked { .. }));
                    if secret_reply && (!open || request.generation != epoch.load(Ordering::Acquire)) {
                        result = Err("The credential reply was revoked before delivery.".into());
                    }
                    let _ = request.answer.send(Reply {
                        generation: if secret_reply { request.generation } else { epoch.load(Ordering::Acquire) },
                        unlocked: status.load(Ordering::Acquire),
                        authority: provider.authorization_bindings(),
                        result,
                    });
                }
                provider.lock();
                status.store(false, Ordering::Release);
            })
            .map_err(|_| "The credential worker could not start.".to_owned())?;
        Ok(Self {
            active_keyring,
            active_process,
            identity,
            sender,
            stopped,
            lock_requested,
            automatic_unlock_blocked,
            notifications: status_notifications,
            feed,
            #[cfg(test)]
            cycles,
            generation,
            unlocked,
        })
    }
    pub(crate) fn subscription(&self) -> iced::Subscription<()> {
        iced::Subscription::run_with(self.feed.clone(), changes)
    }
    #[cfg(test)]
    pub(super) fn test_cycles(&self) -> u64 {
        self.cycles.load(Ordering::Relaxed)
    }
    #[cfg(test)]
    pub(super) fn test_changes(&self) -> watch::Receiver<(u64, bool)> {
        self.feed.receiver.clone()
    }
    #[cfg(any(test, feature = "benchmark"))]
    pub(crate) fn lease(&self) -> Lease {
        self.lease_for(Duration::from_secs(10))
    }
    #[cfg(any(test, feature = "benchmark"))]
    fn lease_for(&self, lifetime: Duration) -> Lease {
        let identity = self
            .identity
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        self.lease_bound(self.generation(), identity.bindings, lifetime)
    }
    pub(crate) fn reply_lease(&self, reply: &Reply, lifetime: Duration) -> Lease {
        self.lease_bound(reply.generation, reply.authority.clone(), lifetime)
    }
    fn lease_bound(
        &self,
        generation: u64,
        bindings: Vec<(PathBuf, enzui_core::state_authority::StateIdentity)>,
        lifetime: Duration,
    ) -> Lease {
        Lease {
            generation: Arc::clone(&self.generation),
            expected: generation,
            unlocked: Arc::clone(&self.unlocked),
            bindings,
            expires: std::time::Instant::now() + lifetime,
        }
    }
    pub(crate) fn source_unlocked(&self, source: &str) -> bool {
        self.unlocked()
            && self
                .identity
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .sources
                .iter()
                .any(|id| id == source)
    }
    pub(crate) fn touch(&self, source: String) {
        let _ = self.sender.try_send(WorkerEvent::Touch(source));
    }
    pub(crate) fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }
    pub(crate) fn unlocked(&self) -> bool {
        self.unlocked.load(Ordering::Acquire)
    }
    pub(crate) fn lock(&self) {
        self.lock_with_policy(false);
    }
    pub(crate) fn lock_manually(&self) {
        self.lock_with_policy(true);
    }
    fn lock_with_policy(&self, manual: bool) {
        // Cancellation can complete on another thread, so revoke grants before waking it.
        {
            let _identity = self
                .identity
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
            self.lock_requested.store(true, Ordering::Release);
            if manual {
                self.automatic_unlock_blocked.store(true, Ordering::Release);
            }
            self.unlocked.store(false, Ordering::Release);
            self.notifications.send_replace((generation, false));
        }
        let process = self
            .active_process
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(active) = process {
            active
                .token
                .cancel(enzui_security::cancellation::CancelCause::Cancelled);
        }
        let active = self
            .active_keyring
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(active) = active {
            active.cancel();
        }
        let _ = self.sender.try_send(WorkerEvent::Changed);
    }
    pub(crate) fn request(&self, operation: Operation) -> Result<oneshot::Receiver<Reply>> {
        let (answer, receive) = oneshot::channel();
        self.sender
            .try_send(WorkerEvent::Request(Box::new(Request {
                generation: self.generation(),
                operation,
                answer,
            })))
            .map_err(|_| "The credential manager is busy. Try again shortly.".to_owned())?;
        Ok(receive)
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.lock();
        self.stopped.store(true, Ordering::Release);
        let _ = self.sender.try_send(WorkerEvent::Changed);
    }
}

fn execute(provider: &mut dyn CredentialProvider, operation: Operation) -> Result<Output> {
    match operation {
        #[cfg(test)]
        Operation::Open { password, create } => {
            provider.open(password.expose(), create)?;
            Ok(Output::Opened {
                desktop: provider.desktop_remembered(),
            })
        }
        Operation::OpenSource {
            source,
            authentication,
            create,
        } => {
            provider.open_source(&source, &authentication, create)?;
            Ok(Output::Opened {
                desktop: provider.desktop_remembered(),
            })
        }
        Operation::ListSource(source, site) => provider
            .list_source(&source, site.as_ref())
            .map(Output::Entries),
        Operation::SaveTo(source, entry) => provider.save_to(&source, entry).map(|()| Output::Done),
        Operation::OpenDesktop { .. } => provider.desktop_unlock().map(Output::DesktopOpened),
        Operation::ForgetDesktop => provider.forget_desktop().map(|()| Output::Done),
        Operation::Synchronize => provider.synchronize().map(|()| Output::Done),
        Operation::List(site) => provider.list(site.as_ref()).map(Output::Entries),
        Operation::CheckSubmission(candidate, filled) => {
            if let Some(id) = filled
                && let Ok(saved) = provider.read_selected(&id)
                && saved.site.container == candidate.site.container
                && saved.password == candidate.password
                && (candidate.username.is_empty() || saved.username == candidate.username)
            {
                return Ok(Output::SubmissionChecked {
                    id: candidate.id,
                    known: true,
                });
            }
            let entries = provider.list(Some(&candidate.site))?;
            let mut failed = None;
            for entry in entries.into_iter().filter(|entry| {
                candidate.username.is_empty() || entry.username == candidate.username
            }) {
                match provider.read(&entry.id, Some(&candidate.site)) {
                    Ok(saved)
                        if saved.site == candidate.site && saved.password == candidate.password =>
                    {
                        return Ok(Output::SubmissionChecked {
                            id: candidate.id,
                            known: true,
                        });
                    }
                    Ok(_) => {}
                    Err(error) => failed = Some(error),
                }
            }
            if let Some(error) = failed {
                return Err(error);
            }
            Ok(Output::SubmissionChecked {
                id: candidate.id,
                known: false,
            })
        }
        Operation::Read(id, site) => provider.read(&id, site.as_ref()).map(Output::Entry),
        Operation::ReadSelected(id) => provider.read_selected(&id).map(Output::Entry),
        Operation::Save(entry) => provider.save(entry).map(|()| Output::Done),
        Operation::Delete(id) => provider.delete(&id).map(|()| Output::Done),
        Operation::Import(source, entries) => {
            provider.import_to(&source, entries).map(Output::Imported)
        }
        Operation::Backup { current, password } => provider
            .backup(current.expose(), password.expose())
            .map(|bytes| Output::Data(Arc::new(enzui_security::secret::Secret::new(bytes)))),
        Operation::Csv(current) => {
            let mut bytes = provider.csv(current.expose())?;
            Ok(Output::Data(Arc::new(enzui_security::secret::Secret::new(
                std::mem::take(&mut *bytes),
            ))))
        }
        Operation::Rekey { current, password } => {
            let result = provider.rekey(current.expose(), password.expose());
            provider.lock();
            result.map(|()| Output::Done)
        }
    }
}

#[cfg(test)]
mod authority_watch_tests {
    use super::*;
    #[test]
    fn submitted_passwords_compare_all_matching_accounts_without_writes() {
        let scratch = super::super::tests::Scratch::new();
        let path = scratch.path().with_file_name("submission-vault");
        let mut provider =
            super::super::Builtin::with_cancellation(path.clone(), Arc::new(Mutex::new(None)));
        provider
            .open("a distinct synthetic master password", true)
            .expect("create vault");
        let site = Site::parse("https://example.com/login", None).expect("site");
        let mut saved = Credential::new(site.clone()).expect("entry");
        saved.username = Secret::new("person@example.invalid".into());
        saved.password = Secret::new("current synthetic login password".into());
        let mut older = Credential::new(site).expect("another entry");
        older.username = saved.username.clone();
        older.password = Secret::new("older synthetic password".into());
        provider.save(older).expect("old entry");
        let saved_id = saved.id.clone();
        provider.save(saved).expect("current entry");
        let before = std::fs::read(&path).expect("encrypted bytes");
        for (username, password, origin, container, expected) in [
            (
                "person@example.invalid",
                "current synthetic login password",
                "https://example.com/login",
                None,
                true,
            ),
            (
                "person@example.invalid",
                "changed synthetic login password",
                "https://example.com/login",
                None,
                false,
            ),
            (
                "",
                "current synthetic login password",
                "https://example.com/password-step",
                None,
                true,
            ),
            (
                "different@example.invalid",
                "current synthetic login password",
                "https://example.com/login",
                None,
                false,
            ),
            (
                "person@example.invalid",
                "current synthetic login password",
                "https://elsewhere.example/login",
                None,
                false,
            ),
            (
                "person@example.invalid",
                "current synthetic login password",
                "https://example.com/login",
                Some(3),
                false,
            ),
        ] {
            let mut candidate =
                Credential::new(Site::parse(origin, container).expect("candidate site"))
                    .expect("candidate");
            candidate.username = Secret::new(username.into());
            candidate.password = Secret::new(password.into());
            let id = candidate.id.clone();
            let output =
                execute(&mut provider, Operation::CheckSubmission(candidate, None)).expect("compare");
            assert!(
                matches!(output, Output::SubmissionChecked { id: returned, known } if returned == id && known == expected),
                "submission comparison must bind site, account, password, container and candidate"
            );
        }
        // A login chosen for another website and filled there is the one
        // submitted: its own password makes it known, another does not.
        for (password, container, expected) in [
            ("current synthetic login password", None, true),
            ("changed synthetic login password", None, false),
            ("current synthetic login password", Some(3), false),
        ] {
            let mut candidate =
                Credential::new(Site::parse("https://elsewhere.example/login", container).expect("site"))
                    .expect("candidate");
            candidate.username = Secret::new("person@example.invalid".into());
            candidate.password = Secret::new(password.into());
            let output = execute(
                &mut provider,
                Operation::CheckSubmission(candidate, Some(saved_id.clone())),
            )
            .expect("compare");
            assert!(
                matches!(output, Output::SubmissionChecked { known, .. } if known == expected),
                "a filled login is known only with its own password and container"
            );
        }
        assert_eq!(
            std::fs::read(path).expect("encrypted vault"),
            before,
            "checking a submission never rewrites a vault"
        );
    }
    #[test]
    fn pending_shared_lock_cannot_publish_an_old_open_store() {
        let scratch = super::super::tests::Scratch::new();
        let mut provider =
            super::super::provider::Builtin::with_cancellation(scratch.path(), Default::default());
        provider
            .open("a distinct fixture master password", true)
            .expect("open provider");
        let status = AtomicBool::new(false);
        let epoch = AtomicU64::new(2);
        let identity = Mutex::new(ProviderState::default());
        let revision = AtomicU64::new(0);
        let (notifications, _) = watch::channel((2, false));
        assert!(!publish_status(
            &mut provider,
            &status,
            &epoch,
            &identity,
            &revision,
            &notifications,
            &AtomicBool::new(true)
        ));
        assert!(!status.load(Ordering::Acquire));
        assert_eq!(epoch.load(Ordering::Acquire), 2);
    }
    #[test]
    fn shared_lock_notifies_every_window_before_the_worker_handles_it() {
        let scratch = super::super::tests::Scratch::new();
        let path = scratch.path().with_file_name("settings.json");
        super::super::config::save(&path, &super::super::Settings::enabled_defaults())
            .expect("settings");
        let service = Service::shared(scratch.path(), path.clone()).expect("first window");
        let second = Service::shared(scratch.path(), path).expect("second window");
        let mut changes = second.test_changes();
        let _ = changes.borrow_and_update();
        let generation = service.generation();
        service.lock();
        assert_eq!(second.generation(), generation + 1);
        assert!(!second.unlocked());
        assert!(changes.has_changed().expect("shared channel"));
        assert_eq!(*changes.borrow_and_update(), (generation + 1, false));
    }
    #[test]
    fn reply_authorization_remains_bound_to_its_store_after_another_window_reads() {
        use enzui_core::state_authority::StateAuthority;
        let scratch = super::super::tests::Scratch::new();
        let mut origin = StateAuthority::open_or_create(&scratch.path()).expect("origin authority");
        origin.publish(b"origin revision").expect("origin revision");
        let other_path = scratch.path().with_file_name("other-store");
        let mut other = StateAuthority::open_or_create(&other_path).expect("other authority");
        other.publish(b"other revision").expect("other revision");
        let (sender, _receive) = mpsc::sync_channel(8);
        let (notify, receiver) = watch::channel((1, true));
        let service = Service {
            active_keyring: Default::default(),
            active_process: Default::default(),
            identity: Arc::new(Mutex::new(ProviderState {
                bindings: vec![(other_path, other.identity().expect("other identity"))],
                sources: vec!["other".into()],
            })),
            sender,
            stopped: Arc::new(AtomicBool::new(false)),
            lock_requested: Arc::new(AtomicBool::new(false)),
            automatic_unlock_blocked: Arc::new(AtomicBool::new(false)),
            notifications: notify,
            feed: ChangeFeed {
                id: 909090,
                receiver,
            },
            cycles: Default::default(),
            generation: Arc::new(AtomicU64::new(1)),
            unlocked: Arc::new(AtomicBool::new(true)),
        };
        let reply = Reply {
            generation: 1,
            unlocked: true,
            authority: vec![(scratch.path(), origin.identity().expect("origin identity"))],
            result: Ok(Output::Done),
        };
        let lease = service.reply_lease(&reply, Duration::from_secs(10));
        assert!(lease.authorize());
        origin
            .publish(b"revoked origin revision")
            .expect("revoke only origin");
        assert!(
            service.lease().authorize(),
            "the other window's store remains authorized"
        );
        assert!(
            !lease.authorize(),
            "the reply must not inherit another window's authorization"
        );
    }
    #[test]
    fn pending_fill_checks_config_custody_even_before_the_worker_processes_a_change() {
        let scratch = super::super::tests::Scratch::new();
        let settings_path = scratch.path().with_file_name("settings.json");
        let settings = super::super::Settings::enabled_defaults();
        super::super::config::save(&settings_path, &settings).expect("settings");
        let mut router = super::super::router::Router::new(
            scratch.path(),
            settings_path.clone(),
            Default::default(),
            Default::default(),
        );
        router
            .open("a distinct fixture master password", true)
            .expect("vault");
        let entry =
            Credential::new(Site::parse("https://authority.example.test", None).expect("site"))
                .expect("login");
        let id = super::super::router::reference("browser", &entry.id);
        router.save(entry).expect("save");
        router.read(&id, None).expect("read source");
        let lease = Lease {
            generation: Arc::new(AtomicU64::new(1)),
            expected: 1,
            unlocked: Arc::new(AtomicBool::new(true)),
            bindings: router.authorization_bindings(),
            expires: std::time::Instant::now() + Duration::from_secs(10),
        };
        assert!(lease.authorize());
        super::super::config::save(&settings_path, &settings)
            .expect("same settings under a new identity");
        assert!(lease.valid());
        assert!(!lease.authorize());
    }
    #[test]
    fn changed_configuration_cancels_helpers_but_late_current_and_vault_events_do_not() {
        use enzui_core::state_authority::StateAuthority;
        let scratch = super::super::tests::Scratch::new();
        let settings_path = scratch.path().with_file_name("settings.json");
        let settings = super::super::Settings::enabled_defaults();
        super::super::config::save(&settings_path, &settings).expect("settings");
        let identity = StateAuthority::open_existing(&settings_path)
            .expect("authority")
            .identity()
            .expect("identity");
        let token = enzui_security::cancellation::CancelToken::new();
        let active = Arc::new(Mutex::new(Some(super::super::connector::ActiveRequest {
            token: token.share(),
            deadline: std::time::Instant::now() + Duration::from_secs(90),
            settings: Some((settings_path.clone(), identity)),
        })));
        let (sender, _receive) = mpsc::sync_channel(8);
        let sink = ChangeSink {
            paths: vec![settings_path.clone(), scratch.path()],
            settings_path: settings_path.clone(),
            active_process: active,
            active_keyring: Default::default(),
            sender,
            pending: Default::default(),
            failed: Default::default(),
        };
        sink.push(RawEvent::change(
            WatchToken::standalone(),
            DirectoryChange::Modified(settings_path.clone()),
        ));
        sink.push(RawEvent::change(
            WatchToken::standalone(),
            DirectoryChange::Modified(scratch.path()),
        ));
        assert!(token.cause().is_none());
        super::super::config::save(&settings_path, &settings).expect("new config publication");
        sink.push(RawEvent::change(
            WatchToken::standalone(),
            DirectoryChange::Renamed {
                from: settings_path.with_extension("tmp"),
                to: settings_path,
            },
        ));
        assert!(token.cause().is_some());
    }
}
