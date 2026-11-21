#[cfg(feature = "benchmark")]
pub(super) fn run_opening_gate() -> Result<serde_json::Value, String> {
    use enzui_webkit::ffi::runtime::SessionError;
    use std::time::Instant;

    fn wait_ready(
        pending: &enzui_webkit::ffi::runtime::PendingWebSession,
    ) -> Result<WebSession, String> {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            enzui_webkit::ffi::dispatch_pending_native_events()
                .map_err(|error| error.to_string())?;
            if let Some(result) = pending.take_result() {
                let session = result.map_err(|error| error.to_string())?;
                if session.attested_renderer().is_none() || !session.is_private_bootstrap_document()
                {
                    return Err("ready session lacks an attested inert document".into());
                }
                return Ok(session);
            }
            if Instant::now() > deadline {
                return Err("deferred bootstrap timed out".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    Worker::configure_process();
    let OpeningResources { display, profiles } = opening_resources()?;
    let profile = WebProfile::ephemeral();
    let started = Instant::now();
    let pending = WebSession::begin_profiled(&display, &mut profiles.borrow_mut(), &profile)
        .map_err(|error| error.to_string())?;
    let begin_ms = started.elapsed().as_secs_f64() * 1000.0;
    if pending.take_result().is_some() {
        return Err("construction published a session before event dispatch".into());
    }
    if !matches!(
        WebSession::begin_profiled(&display, &mut profiles.borrow_mut(), &profile),
        Err(SessionError::BootstrapBusy)
    ) {
        return Err("concurrent bootstrap bypassed admission".into());
    }
    let session = wait_ready(&pending)?;
    if pending.take_result().is_some() || !pending.is_finished() {
        return Err("ready result must be one-shot and finished".into());
    }
    let renderer = session.attested_renderer();
    session
        .terminate_web_process_verified()
        .map_err(|error| format!("ready cleanup: {error}"))?;
    drop(session);
    drop(pending);

    let mut cancel_ms = Vec::new();
    for pump_before_cancel in [false, true, false, true] {
        let pending = WebSession::begin_profiled(&display, &mut profiles.borrow_mut(), &profile)
            .map_err(|error| error.to_string())?;
        if pump_before_cancel {
            enzui_webkit::ffi::dispatch_pending_native_events()
                .map_err(|error| error.to_string())?;
        }
        let started = Instant::now();
        drop(pending);
        cancel_ms.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    let pending = WebSession::begin_profiled(&display, &mut profiles.borrow_mut(), &profile)
        .map_err(|error| format!("admission after cancellation: {error}"))?;
    let legacy = WebSession::new_profiled(&display, &mut profiles.borrow_mut(), &profile)
        .map_err(|error| format!("legacy construction while pending: {error}"))?;
    let deferred = wait_ready(&pending)?;
    if legacy.attested_renderer().is_none()
        || legacy.attested_renderer() == deferred.attested_renderer()
    {
        return Err("legacy and deferred sessions need distinct attested renderers".into());
    }
    legacy
        .terminate_web_process_verified()
        .map_err(|error| format!("legacy cleanup: {error}"))?;
    deferred
        .terminate_web_process_verified()
        .map_err(|error| format!("deferred cleanup: {error}"))?;
    let reservation_profile = WebProfile::ephemeral();
    let mut reservation_router = ViewRouter::new();
    let reservation_view = reservation_router.create(997);
    let reservation_page = OpenPage {
        adopt: None,
        profile: reservation_profile.clone(),
        retain_profile: false,
        router: reservation_router,
        view: reservation_view,
        scratch: crate::browser_profile::paths()
            .cache
            .join("reservation-gate"),
        initial_size: None,
        initial_generation: MediaGeneration::FIRST,
    };
    let mut reservations = Vec::new();
    for _ in 0..2 {
        let session =
            Worker::new_session(&display, &mut profiles.borrow_mut(), &reservation_profile)?;
        reservations.push(Worker::finish_open(
            reservation_page.clone(),
            OpeningResources {
                display: std::rc::Rc::clone(&display),
                profiles: std::rc::Rc::clone(&profiles),
            },
            session,
        )?);
    }
    let retained =
        || PROFILE_LEASES.with(|leases| leases.borrow().contains_key(&reservation_profile.id()));
    if retained() {
        return Err("inert reservations retained a persistent profile lease".into());
    }
    if reservations
        .iter()
        .any(|worker| !worker.defer_prepared_scripts)
    {
        return Err("cold reservations prepared cosmetic scripts before navigation".into());
    }
    let first = reservations.remove(0);
    first.session.terminate_web_process_verified()?;
    drop(first);
    let second = reservations.pop().expect("second reservation");
    if retained() || second.session.attested_renderer().is_none() {
        return Err("cancelling an overlapping reservation changed the surviving owner".into());
    }
    let (commands, receiver) = mpsc::sync_channel(COMMAND_CAP);
    let mut task = WorkerLoop {
        worker: second,
        rx: PageReceiver {
            commands: receiver,
            stop: Arc::new(StopBool::new(false)),
        },
        updates: Arc::new(Mutex::new(VecDeque::new())),
        admission: Arc::new(Mutex::new(Admission::default())),
        input_frame_due: None,
        _scratch: None,
    };
    commands
        .send(SeqCmd {
            seq: 0,
            cmd: Command::Browse {
                view: task.worker.view,
                action: super::BrowserAction::Navigate("about:blank".into()),
            },
        })
        .map_err(|e| e.to_string())?;
    if !task.step(false) {
        return Err("adopted reservation's worker stopped before navigation".into());
    }
    if !retained() {
        return Err("adopted reservation did not retain its profile".into());
    }
    if task.worker.defer_prepared_scripts {
        return Err("explicit navigation left cosmetic scripts deferred".into());
    }
    task.worker.session.terminate_web_process_verified()?;
    drop(task);
    if !retained() {
        return Err("adopted profile lease vanished on page close".into());
    }
    PROFILE_LEASES.with(|leases| {
        leases.borrow_mut().remove(&reservation_profile.id());
    });
    Ok(serde_json::json!({
        "transient_reservations_do_not_retain_profile": true,
        "overlapping_reservation_cancellation_preserves_owner": true,
        "adoption_promotes_profile_lease": true,
        "cosmetic_preparation_waits_for_explicit_navigation": true,
        "begin_ms": begin_ms,
        "ready_renderer": format!("{renderer:?}"),
        "cancel_ms": cancel_ms,
        "ready_is_inert_and_attested": true,
        "result_is_one_shot": true,
        "concurrent_bootstrap_refused": true,
        "legacy_constructor_waits_for_pending": true,
        "admission_reusable_after_cancellation": true
    }))
}
