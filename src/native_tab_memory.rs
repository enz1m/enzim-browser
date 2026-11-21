struct RetainedSession {
    live: Option<WebSession>,
    discarded: Option<enzui_webkit::ffi::runtime::DiscardedPage>,
}

impl RetainedSession {
    fn new(session: WebSession) -> Self {
        Self {
            live: Some(session),
            discarded: None,
        }
    }
}

impl std::ops::Deref for RetainedSession {
    type Target = WebSession;
    fn deref(&self) -> &Self::Target {
        // Worker dispatch and readiness exclude discarded sessions until restoration succeeds.
        self.live.as_ref().expect("active worker session")
    }
}
impl std::ops::DerefMut for RetainedSession {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.live.as_mut().expect("active worker session")
    }
}

/// When a hidden tab first releases its caches and tiles. The engine has trimmed the tab's
/// process eight seconds after it was hidden; what this release adds is mostly the tiles, so a
/// tab shown again after it is repainted first (about 50 ms to the first frame instead of 17).
const FIRST_HIDDEN_RELEASE: Duration = Duration::from_secs(15);

fn memory_policy<K>() -> enzui_webkit::tab_memory::TabMemoryPolicy<K> {
    let mut policy = enzui_webkit::tab_memory::TabMemoryPolicy::default();
    configure_memory_policy(&mut policy, super::MemoryMode::Aggressive);
    policy
}

/// Sets the policy's schedule for `mode`; the tabs' release history stays.
fn configure_memory_policy<K>(policy: &mut enzui_webkit::tab_memory::TabMemoryPolicy<K>, mode: super::MemoryMode) {
    use super::MemoryMode;
    let (first, second, recent, recent_after, discard) = match mode {
        MemoryMode::Aggressive => (FIRST_HIDDEN_RELEASE, 5 * 60, 0, 60, 10 * 60),
        MemoryMode::Balanced => (FIRST_HIDDEN_RELEASE, 5 * 60, 5, 60, 10 * 60),
        // Its recent tabs' first release is at five minutes, so the second waits longer.
        MemoryMode::Relaxed => (Duration::from_secs(60), 10 * 60, 10, 5 * 60, 30 * 60),
    };
    policy.release_after = vec![first, Duration::from_secs(second)];
    policy.recent = recent;
    policy.recent_release_after = Duration::from_secs(recent_after);
    policy.moderate_discard_after = Duration::from_secs(discard);
}

#[cfg(test)]
#[test]
fn hidden_tab_first_releases_after_the_trim_and_not_before() {
    use enzui_webkit::tab_memory::{MemoryPressure, TabAction, TabState};
    let hidden = std::time::Instant::now();
    let tabs = [TabState {
        key: 7_u64,
        visible: false,
        last_visible: hidden,
        playing_audio: false,
        protected: false,
        discarded: false,
    }];
    let mut policy = memory_policy::<u64>();
    let early = hidden + FIRST_HIDDEN_RELEASE - Duration::from_millis(1);
    assert!(
        policy.tick(early, MemoryPressure::Normal, &tabs).is_empty(),
        "a tab hidden for less than the first release keeps its tiles"
    );
    assert_eq!(
        policy.tick(hidden + FIRST_HIDDEN_RELEASE, MemoryPressure::Normal, &tabs),
        vec![TabAction::Release(7)]
    );
    assert!(FIRST_HIDDEN_RELEASE > Duration::from_secs(8), "the engine's own trim comes first");
}

std::thread_local! {
    static MEMORY_POLICY: std::cell::RefCell<enzui_webkit::tab_memory::TabMemoryPolicy<u64>> = std::cell::RefCell::new(memory_policy());
    static MEMORY_POLICY_MODE: std::cell::Cell<super::MemoryMode> = const { std::cell::Cell::new(super::MemoryMode::Aggressive) };
    static MEMORY_DUE: std::cell::Cell<Option<std::time::Instant>> = const { std::cell::Cell::new(None) };
}

/// Result of the latest memory policy tick, for work that is only worth its
/// memory while there is plenty: `true` until a tick observes pressure.
static MEMORY_PRESSURE_NORMAL: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(true);

pub(crate) fn memory_pressure_normal() -> bool {
    MEMORY_PRESSURE_NORMAL.load(Ordering::Relaxed)
}

fn memory_policy_deadline() -> std::time::Instant {
    MEMORY_DUE.with(|due| due.get().unwrap_or_else(std::time::Instant::now))
}

fn claim_memory_tick(now: std::time::Instant) -> bool {
    MEMORY_DUE.with(|due| {
        if due.get().is_some_and(|deadline| now < deadline) {
            return false;
        }
        due.set(Some(now + Duration::from_secs(1)));
        true
    })
}

#[cfg(test)]
#[test]
fn memory_modes_keep_recent_tabs_ready_longer() {
    use enzui_webkit::tab_memory::{MemoryPressure, TabAction, TabState};
    let now = std::time::Instant::now();
    // Tab 1 was shown last; tab 7 longest ago; all hidden for 30 s.
    let tabs: Vec<_> = (1..=7_u64)
        .map(|key| TabState {
            key,
            visible: false,
            last_visible: now - Duration::from_secs(30) - Duration::from_millis(key),
            playing_audio: false,
            protected: false,
            discarded: false,
        })
        .collect();
    let released = |mode| {
        let mut policy = memory_policy::<u64>();
        configure_memory_policy(&mut policy, mode);
        let mut keys: Vec<u64> = policy
            .tick(now, MemoryPressure::Normal, &tabs)
            .into_iter()
            .filter_map(|action| match action {
                TabAction::Release(key) => Some(key),
                TabAction::Discard(_) => None,
            })
            .collect();
        keys.sort_unstable();
        keys
    };
    assert_eq!(released(super::MemoryMode::Aggressive), [1, 2, 3, 4, 5, 6, 7], "every tab after 15 s");
    assert_eq!(released(super::MemoryMode::Balanced), [6, 7], "the 5 most recent wait a minute");
    assert!(released(super::MemoryMode::Relaxed).is_empty(), "nothing before a minute");
}

#[cfg(test)]
#[test]
fn memory_policy_runs_first_tick_without_a_prior_interest_query() {
    let now = std::time::Instant::now();
    MEMORY_DUE.with(|due| due.set(None));
    assert!(
        claim_memory_tick(now),
        "uninitialized policy must run immediately"
    );
    assert!(
        !claim_memory_tick(now + Duration::from_millis(999)),
        "policy must not poll faster than one second"
    );
    assert!(
        claim_memory_tick(now + Duration::from_secs(1)),
        "deadline must remain anchored to the previous tick"
    );
    MEMORY_DUE.with(|due| due.set(None));
}

/// Time a page stays hidden before the garbage of its last load is collected.
/// Long enough that flipping between two tabs does not collect on every flip.
const IDLE_COLLECTION_DELAY: Duration = Duration::from_secs(4);

/// Whether a hidden page should now get the one collection of its hidden
/// period. The collection keeps compiled code, images and caches, so it comes
/// well before the cache release that the memory policy schedules.
fn idle_collection_due(collected: bool, hidden_for: Duration, busy: bool) -> bool {
    !collected && !busy && hidden_for >= IDLE_COLLECTION_DELAY
}

#[cfg(test)]
#[test]
fn hidden_page_collects_once_after_a_short_grace_and_never_while_busy() {
    let due = IDLE_COLLECTION_DELAY;
    assert!(
        !idle_collection_due(false, due - Duration::from_millis(1), false),
        "a page hidden for a moment keeps its heap untouched"
    );
    assert!(idle_collection_due(false, due, false));
    assert!(
        !idle_collection_due(false, due * 3, true),
        "a loading or playing page is not interrupted"
    );
    assert!(
        !idle_collection_due(true, due * 3, false),
        "one collection per hidden period"
    );
}

fn memory_tracing_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED
        .get_or_init(|| std::env::var_os("ENZIM_MEMORY_TRACE").is_some_and(|value| value == "1"))
}

fn maintain_tab_memory(tasks: &mut [MainTask]) {
    use enzui_webkit::tab_memory::{MemorySnapshot, PressureThresholds};
    let now = std::time::Instant::now();
    if !claim_memory_tick(now) {
        return;
    }
    let Ok(snapshot) = MemorySnapshot::read() else {
        return;
    };
    let pressure = snapshot.pressure(&PressureThresholds::default());
    MEMORY_PRESSURE_NORMAL.store(
        pressure == enzui_webkit::tab_memory::MemoryPressure::Normal,
        Ordering::Relaxed,
    );
    if memory_tracing_enabled() {
        eprintln!(
            "MEMORY_PRESSURE {}",
            serde_json::json!({
                "total_bytes": snapshot.total_bytes,
                "available_bytes": snapshot.available_bytes,
                "stall_percent": snapshot.stall_percent,
                "pressure": format!("{pressure:?}"),
            })
        );
    }
    apply_tab_memory_policy(tasks, now, pressure);
}

fn apply_tab_memory_policy(
    tasks: &mut [MainTask],
    now: std::time::Instant,
    pressure: enzui_webkit::tab_memory::MemoryPressure,
) {
    use enzui_webkit::tab_memory::{TabAction, TabState};
    let tabs: Vec<_> = tasks
        .iter_mut()
        .filter_map(|task| task.task.as_mut())
        .map(|task| {
            let w = &mut task.worker;
            if w.visible {
                w.last_visible = now;
                w.idle_collected = false;
            }
            let discarded = w.session.discarded.is_some();
            if !w.visible
                && !discarded
                && idle_collection_due(
                    w.idle_collected,
                    now.saturating_duration_since(w.last_visible),
                    w.session.is_loading() || w.session.is_playing_audio(),
                )
            {
                // One attempt per hidden period, also on a runtime without the request.
                w.idle_collected = true;
                let collected = w.session.release_idle_memory().is_ok();
                w.trace_background_memory("idle-collect", Some(collected));
            }
            if !discarded {
                w.trace_background_memory("policy-observe", None);
            }
            TabState {
                key: w.memory_id,
                visible: w.visible,
                last_visible: w.last_visible,
                playing_audio: !discarded && w.session.is_playing_audio(),
                protected: task
                    .admission
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .memory_protected
                    || w.edited
                    || (!discarded && (w.session.is_loading() || w.session.has_page_events()))
                    || !w.jobs.is_empty()
                    || w.pdf_job.is_some()
                    || !w.download_requests.is_empty()
                    || !w.active_downloads.is_empty()
                    || !w.download_controls.is_empty()
                    || !w.native_operations.is_empty()
                    || w.developer_tools,
                discarded,
            }
        })
        .collect();
    let mode = super::memory_mode();
    let actions = MEMORY_POLICY.with(|policy| {
        let mut policy = policy.borrow_mut();
        if MEMORY_POLICY_MODE.with(|current| current.replace(mode)) != mode {
            configure_memory_policy(&mut policy, mode);
        }
        policy.tick(now, pressure, &tabs)
    });
    for action in actions {
        let key = match &action {
            TabAction::Release(key) | TabAction::Discard(key) => key,
        };
        let Some(task) = tasks
            .iter_mut()
            .filter_map(|task| task.task.as_mut())
            .find(|task| task.worker.memory_id == *key)
        else {
            continue;
        };
        match action {
            TabAction::Release(_) => {
                let released = task.worker.release_background_memory();
                task.worker
                    .trace_background_memory("policy-release", Some(released));
            }
            TabAction::Discard(_) => {
                eprintln!("MEMORY_DISCARD");
                task.worker.trace_background_memory("policy-discard", None);
                if let Some(update) = task.worker.discard_tab() {
                    push_update(&task.updates, update);
                }
                task.input_frame_due = None;
            }
        }
    }
}

impl Worker {
    fn discard_tab(&mut self) -> Option<NativeUpdate> {
        if self.visible || self.session.discarded.is_some() {
            return None;
        }
        self.session.set_notifications_enabled(false);
        let retirement = self.update(None, false);
        self.session
            .set_view_visibility(&self.router, self.view, false);
        self.discard_gpu_frame();
        self.pending_copy = None;
        self.copy_poll_due = None;
        self.resource_log = None;
        self.filter_installation = None;
        self.favicons.reset();
        if let Some(session) = self.session.live.take() {
            self.session.discarded = Some(session.discard());
        }
        Some(retirement)
    }

    fn restore_tab(&mut self) -> Result<(), String> {
        let Some(page) = self.session.discarded.as_ref() else {
            return Ok(());
        };
        let size = self.session_size;
        let mut replacement = Self::open(
            self.router.clone(),
            self.view,
            self.filter_cache
                .parent()
                .unwrap_or_else(|| std::path::Path::new(""))
                .to_path_buf(),
            size,
            self.generation,
            self.profile.clone(),
        )?;
        replacement.privacy = self.privacy.clone();
        replacement.privacy.apply_for_site(
            &replacement.session,
            page.uri().unwrap_or_default(),
            false,
            &replacement.default_user_agent,
        );
        replacement
            .configure_permissions(self.permission_choices.clone())
            .map_err(|e| e.to_string())?;
        replacement.notifications_enabled = self.notifications_enabled;
        replacement
            .session
            .set_notifications_enabled(self.notifications_enabled);
        replacement.configure_developer_tools(self.developer_tools);
        replacement.session.set_page_zoom(self.zoom);
        replacement.zoom = self.zoom;
        if let Some(font) = &self.preferred_system_font {
            replacement
                .session
                .set_system_font_name(font)
                .map_err(|e| e.to_string())?;
        }
        replacement
            .preferred_system_font
            .clone_from(&self.preferred_system_font);
        replacement.preferred_dark = self.preferred_dark;
        if let Some(dark) = self.preferred_dark {
            replacement
                .session
                .set_preferred_dark(dark)
                .map_err(|e| e.to_string())?;
        }
        replacement.pending_restore = true;
        replacement.memory_id = self.memory_id;
        replacement.last_visible = self.last_visible;
        replacement.background_memory = std::mem::take(&mut self.background_memory);
        replacement.visible = false;
        replacement
            .session
            .set_view_visibility(&replacement.router, replacement.view, false);
        replacement.refresh_document_scripts(page.uri());
        let (mode, filters) = self.active_filters.clone();
        let page = page.clone();
        let (job, mut reply) = crate::native_jobs::Job::new();
        replacement.configure_filters(mode, filters, job);
        replacement.native_operations.push(Box::new(move |worker| {
            if !worker.pending_restore {
                return true;
            }
            match reply.try_recv() {
                Ok(Ok(_)) => {
                    worker.pending_restore = false;
                    worker.privacy.apply_for_site(
                        &worker.session,
                        page.uri().unwrap_or_default(),
                        worker.developer_tools,
                        &worker.default_user_agent,
                    );
                    let _ = worker.session.restore_discarded(&page);
                    true
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => false,
                _ => {
                    worker.pending_restore = false;
                    true
                }
            }
        }));
        *self = replacement;
        Ok(())
    }
}

#[cfg(feature = "benchmark")]
fn run_tab_memory_gate(scratch: PathBuf) -> Result<(), String> {
    use enzui_webkit::tab_memory::{MemoryPressure, TabAction, TabMemoryPolicy, TabState};
    let now = std::time::Instant::now();
    let hidden = now - Duration::from_secs(601);
    let mut policy = TabMemoryPolicy::<u8>::default();
    let mut tabs = vec![TabState {
        key: 1,
        visible: false,
        last_visible: hidden,
        playing_audio: false,
        protected: false,
        discarded: false,
    }];
    let actions = policy.tick(now, MemoryPressure::Moderate, &tabs);
    if !actions.contains(&TabAction::Discard(1)) {
        return Err("Moderate pressure did not discard old background tab".into());
    }
    for (visible, audio, protected, discarded) in [
        (true, false, false, false),
        (false, true, false, false),
        (false, false, true, false),
        (false, false, false, true),
    ] {
        tabs[0].visible = visible;
        tabs[0].playing_audio = audio;
        tabs[0].protected = protected;
        tabs[0].discarded = discarded;
        if policy
            .tick(now, MemoryPressure::Critical, &tabs)
            .iter()
            .any(|a| matches!(a, TabAction::Discard(_)))
        {
            return Err("Protected tab selected for discard".into());
        }
    }
    let mut worker = metadata_worker(scratch)?;
    worker
        .session
        .load("https://example.com/", 20000)
        .map_err(|e| e.to_string())?;
    worker.set_visibility(worker.view, true);
    metadata_eval(
        &worker,
        "document.body.insertAdjacentHTML('afterbegin','<input id=memoryEdit>');memoryEdit.focus();setTimeout(()=>memoryEdit.dispatchEvent(new Event('input',{bubbles:true})),30);true",
    )?;
    let synthetic_deadline = std::time::Instant::now() + Duration::from_millis(250);
    while std::time::Instant::now() < synthetic_deadline {
        worker.session.pump(20);
        worker.update(None, false);
    }
    if worker.edited {
        return Err("Synthetic form input protected the page".into());
    }
    let _ = worker.session.forward_input(&InputEvent::FocusGained {
        view: worker.view,
        modifiers: input::Modifiers::NONE,
    });
    metadata_eval(&worker, "memoryEdit.focus();true")?;
    for event in [
        InputEvent::KeyPressed {
            view: worker.view,
            key: input::KeyCode::Character("x".into()),
            modifiers: input::Modifiers::NONE,
        },
        InputEvent::KeyReleased {
            view: worker.view,
            key: input::KeyCode::Character("x".into()),
            modifiers: input::Modifiers::NONE,
        },
    ] {
        let _ = worker.session.forward_input(&event);
    }
    let edited_deadline = std::time::Instant::now() + Duration::from_secs(3);
    while !worker.edited {
        worker.session.pump(20);
        worker.update(None, false);
        if std::time::Instant::now() > edited_deadline {
            return Err("Trusted form edit did not protect the page".into());
        }
    }
    let origin = enzui_webkit::site_permissions::PermissionOrigin::parse("https://example.com")
        .ok_or("notification fixture origin")?;
    worker
        .session
        .set_site_permission(
            enzui_webkit::site_permissions::SitePermission {
                requesting_origin: origin.clone(),
                top_origin: origin,
                kind: enzui_webkit::web::policy::PermissionKind::Notifications,
            },
            true,
        )
        .map_err(|error| error.to_string())?;
    let _ = worker.session.evaluate_script(
        "Notification.requestPermission().then(p=>{if(p==='granted')window.discardNotice=new Notification('Discard lifecycle fixture')});true",
        &enzui_webkit::page::ScriptWorld::Page,
    ).map_err(|error| format!("{error:?}"))?;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let notification = loop {
        worker.session.pump(10);
        if let Some(id) = worker
            .session
            .take_page_events()
            .into_iter()
            .find_map(|event| {
                if let enzui_webkit::page::PageEvent::Notification(request) = event {
                    Some(request.id)
                } else {
                    None
                }
            })
        {
            break id;
        }
        if std::time::Instant::now() >= deadline {
            return Err("Discard fixture notification was not shown".into());
        }
    };
    let uri = worker.session.current_uri().map_err(|e| format!("{e:?}"))?;
    let title = worker.session.page_title().map_err(|e| e.to_string())?;
    let old_pid = worker.session.attested_renderer().map(|id| id.pid);
    worker.set_visibility(worker.view, false);
    worker.session.pump(100);
    let retirement = worker
        .discard_tab()
        .ok_or("hidden page was not discarded")?;
    if !retirement.page_events.iter().any(|event| {
        matches!(event,
        enzui_webkit::page::PageEvent::NotificationClosed { id } if *id == notification)
    }) {
        return Err("Discard lost the notification close relay".into());
    }
    if worker.session.live.is_some()
        || worker.session.discarded.is_none()
        || worker.gpu_last.is_some()
        || worker.publication_pending()
    {
        return Err("Discard retained a session/frame or publishes against a closed page".into());
    }
    let page = worker
        .session
        .discarded
        .as_ref()
        .ok_or("No discarded page")?;
    if page.uri() != uri.as_deref() || page.title() != title.as_deref() {
        return Err("Discard lost tab strip metadata".into());
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while old_pid.is_some_and(|pid| std::path::Path::new(&format!("/proc/{pid}")).exists()) {
        if std::time::Instant::now() > deadline {
            return Err("Discarded renderer remains alive".into());
        }
        enzui_webkit::ffi::dispatch_pending_native_events().map_err(|e| e.to_string())?;
        std::thread::sleep(Duration::from_millis(20));
    }
    worker.notifications_enabled = false;
    worker.restore_tab()?;
    worker.set_visibility(worker.view, true);
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        worker.session.pump(20);
        worker.update(None, false);
        if worker.session.current_uri().ok().flatten() == uri
            && !worker.session.is_loading()
            && worker.native_operations.is_empty()
        {
            break;
        }
        if std::time::Instant::now() > deadline {
            return Err("Discarded page did not restore".into());
        }
    }
    if worker.profile != WebProfile::shared()
        || worker.session.attested_renderer().map(|id| id.pid) == old_pid
    {
        return Err("Restore reused the discarded renderer or changed profile".into());
    }
    if worker.notifications_enabled {
        return Err("Restore lost the global notification block".into());
    }
    metadata_eval(
        &worker,
        "document.title='Restored permission before request: '+Notification.permission;true",
    )?;
    eprintln!(
        "{}",
        worker
            .session
            .page_title()
            .map_err(|error| error.to_string())?
            .unwrap_or_default()
    );
    metadata_eval(
        &worker,
        "document.body.innerHTML='<button id=notificationPolicyCheck style=\"position:absolute;left:20px;top:20px;width:200px;height:60px\">Request</button>';document.getElementById('notificationPolicyCheck').onclick=()=>Notification.requestPermission().then(p=>{document.title='Restored request: '+p;new Notification('Blocked restored notification')});true",
    )?;
    worker.session.pump(100);
    let _ = worker.session.forward_input(&InputEvent::FocusGained {
        view: worker.view,
        modifiers: input::Modifiers::NONE,
    });
    let mapping = input::ViewportMapping::new(0., 0., 1024., 768., 1.)
        .ok_or("invalid notification fixture viewport")?;
    for event in [
        input::pointer_moved(worker.view, &mapping, 50., 40.),
        input::pointer_pressed(
            worker.view,
            &mapping,
            50.,
            40.,
            input::PointerButton::Primary,
        ),
        input::pointer_released(
            worker.view,
            &mapping,
            50.,
            40.,
            input::PointerButton::Primary,
        ),
    ] {
        if !worker
            .session
            .forward_input(&event.ok_or("notification gesture outside viewport")?)
        {
            return Err("Restored notification gesture was not delivered".into());
        }
        worker.session.pump(10);
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        worker.session.pump(10);
        if worker.session.take_page_events().iter().any(|event| {
            matches!(
                event,
                enzui_webkit::page::PageEvent::PermissionRequested(_)
                    | enzui_webkit::page::PageEvent::Notification(_)
            )
        }) {
            return Err("Restored globally blocked page prompted or posted a notification".into());
        }
        if worker
            .session
            .page_title()
            .map_err(|error| error.to_string())?
            .as_deref()
            == Some("Restored request: denied")
        {
            break;
        }
        if std::time::Instant::now() >= deadline {
            return Err("Restored notification request was not denied".into());
        }
    }
    worker.session.pump(100);
    if worker.session.take_page_events().iter().any(|event| {
        matches!(
            event,
            enzui_webkit::page::PageEvent::PermissionRequested(_)
                | enzui_webkit::page::PageEvent::Notification(_)
        )
    }) {
        return Err("Restored globally blocked page delivered a late notification".into());
    }
    run_worker_discard_policy_gate(worker)?;
    Ok(())
}

#[cfg(feature = "benchmark")]
fn run_worker_discard_policy_gate(mut worker: Worker) -> Result<(), String> {
    use enzui_webkit::tab_memory::MemoryPressure;
    let (commands, receiver) = mpsc::sync_channel(COMMAND_CAP);
    worker.set_visibility(worker.view, false);
    worker.last_visible = std::time::Instant::now() - Duration::from_secs(700);
    let mut tasks = vec![MainTask {
        opening: None,
        task: Some(WorkerLoop {
            worker,
            rx: PageReceiver {
                commands: receiver,
                stop: Arc::new(StopBool::new(false)),
            },
            updates: Default::default(),
            admission: Default::default(),
            input_frame_due: None,
            _scratch: None,
        }),
        completed: Arc::new(AtomicU8::new(0)),
        busy: Arc::new(StopBool::new(true)),
        panicked: false,
    }];
    for (pinned, edited) in [(true, false), (false, true)] {
        let task = tasks[0].task.as_mut().ok_or("Missing worker")?;
        task.admission
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .memory_protected = pinned;
        task.worker.edited = edited;
        apply_tab_memory_policy(
            &mut tasks,
            std::time::Instant::now(),
            MemoryPressure::Critical,
        );
        if tasks[0]
            .task
            .as_ref()
            .ok_or("Missing worker")?
            .worker
            .session
            .live
            .is_none()
        {
            return Err("Policy discarded a pinned or edited page".into());
        }
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let task = tasks[0].task.as_mut().ok_or("Missing worker")?;
        task.admission
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .memory_protected = false;
        task.worker.edited = false;
        task.worker.session.pump(20);
        task.worker.update(None, false);
        apply_tab_memory_policy(
            &mut tasks,
            std::time::Instant::now(),
            MemoryPressure::Critical,
        );
        if tasks[0]
            .task
            .as_ref()
            .ok_or("Missing worker")?
            .worker
            .session
            .discarded
            .is_some()
        {
            break;
        }
        if std::time::Instant::now() > deadline {
            return Err("Critical policy failed to discard background worker".into());
        }
    }
    let task = tasks[0].task.as_mut().ok_or("Missing worker")?;
    if !task.step(false) {
        return Err("Idle discarded worker stopped".into());
    }
    let view = task.worker.view;
    commands
        .send(SeqCmd {
            seq: 1,
            cmd: Command::Resize {
                view,
                size: enzui_webkit::ViewportSize::from_logical(800.0, 600.0)
                    .map_err(|e| format!("{e:?}"))?,
            },
        })
        .map_err(|e| e.to_string())?;
    if !task.step(false) || task.worker.session.live.is_some() {
        return Err("Background resize revived discarded worker".into());
    }
    for action in [
        super::BrowserAction::ColorScheme(false),
        super::BrowserAction::SystemFont("Arial 10".into()),
        super::BrowserAction::Zoom(1.25),
        super::BrowserAction::CredentialRefresh,
    ] {
        commands
            .send(SeqCmd {
                seq: 2,
                cmd: Command::Browse { view, action },
            })
            .map_err(|e| e.to_string())?;
        if !task.step(false) || task.worker.session.live.is_some() {
            return Err("Passive settings revived discarded worker".into());
        }
    }
    commands
        .send(SeqCmd {
            seq: 2,
            cmd: Command::Visibility {
                view,
                visible: true,
            },
        })
        .map_err(|e| e.to_string())?;
    if !task.step(false) || task.worker.session.live.is_none() || !task.worker.visible {
        return Err("Activating tab failed to restore its worker".into());
    }
    Ok(())
}
