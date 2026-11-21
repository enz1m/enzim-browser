#[derive(Clone)]
struct OpeningResources {
    display: std::rc::Rc<HeadlessDisplay>,
    profiles: std::rc::Rc<std::cell::RefCell<ProfileStore>>,
}

fn opening_resources() -> Result<OpeningResources, String> {
    std::fs::create_dir_all(&crate::browser_profile::paths().data)
        .map_err(|error| format!("browser data directory: {error}"))?;
    let display = shared_display()?;
    std::thread_local! {
        static PROFILES: std::rc::Rc<std::cell::RefCell<ProfileStore>> = {
            let root = crate::browser_profile::paths().data.clone();
            std::rc::Rc::new(std::cell::RefCell::new(ProfileStore::new(root)))
        };
    }
    Ok(OpeningResources {
        display,
        profiles: PROFILES.with(std::rc::Rc::clone),
    })
}

struct OpeningWorkerLoop {
    rx: Option<PageReceiver>,
    updates: Arc<Mutex<VecDeque<NativeUpdate>>>,
    admission: Arc<Mutex<Admission>>,
    page: Option<OpenPage>,
    scratch: Option<Arc<SecureScratch>>,
    resources: Option<OpeningResources>,
    pending: Option<enzui_webkit::ffi::runtime::PendingWebSession>,
}

impl OpeningWorkerLoop {
    fn new(
        rx: PageReceiver,
        updates: Arc<Mutex<VecDeque<NativeUpdate>>>,
        admission: Arc<Mutex<Admission>>,
        page: OpenPage,
        scratch: Option<Arc<SecureScratch>>,
    ) -> Self {
        Self {
            rx: Some(rx),
            updates,
            admission,
            page: Some(page),
            scratch,
            resources: None,
            pending: None,
        }
    }

    fn report_error(&self, error: String) {
        push_update(
            &self.updates,
            NativeUpdate {
                page_events: Vec::new(),
                view: self.page.as_ref().expect("opening page").view,
                report: None,
                metadata: None,
                frame: None,
                gpu_frame: None,
                tab_reply: None,
                view_focused: None,
                error: Some(NativeWorkerError::Bootstrap {
                    stage: "asynchronous display/session/load",
                    detail: error,
                }),
                clipboard: None,
                selection: None,
            },
        );
    }

    fn finish_synchronously(mut self) -> Option<WorkerLoop> {
        WorkerLoop::new(
            self.rx.take()?,
            Arc::clone(&self.updates),
            Arc::clone(&self.admission),
            self.page.take()?,
            self.scratch.take(),
        )
    }

    fn poll(&mut self) -> Result<Option<WorkerLoop>, Option<String>> {
        if self.rx.as_ref().expect("opening receiver").stop_raised() {
            self.pending.take();
            return Err(None);
        }
        if self.resources.is_none() {
            self.resources = Some(opening_resources().map_err(Some)?);
        }
        let resources = self.resources.as_ref().expect("opening resources");
        // A tab made around a window a page opened takes that window's
        // view, which its opener's worker keeps, instead of starting one.
        let adopted = self.page.as_ref().expect("opening page").adopt;
        let session = if let Some(popup) = adopted {
            take_kept_popup(popup)
                .ok_or_else(|| Some("The window the page opened is gone.".to_owned()))?
        } else {
            if self.pending.is_none() {
                Worker::configure_process();
                let page = self.page.as_ref().expect("opening page");
                match WebSession::begin_profiled(
                    &resources.display,
                    &mut resources.profiles.borrow_mut(),
                    &page.profile,
                ) {
                    Ok(pending) => self.pending = Some(pending),
                    Err(enzui_webkit::ffi::runtime::SessionError::BootstrapBusy) => {
                        return Ok(None);
                    }
                    Err(error) => return Err(Some(format!("begin_profiled: {error}"))),
                }
            }
            let Some(result) = self
                .pending
                .as_ref()
                .expect("pending session")
                .take_result()
            else {
                return Ok(None);
            };
            result.map_err(|error| Some(format!("pending session: {error}")))?
        };
        let page = self.page.as_ref().expect("opening page");
        let worker = Worker::finish_open(page.clone(), resources.clone(), session).map_err(Some)?;
        self.page.take();
        self.pending.take();
        let mut task = WorkerLoop {
            worker,
            rx: self.rx.take().expect("opening receiver"),
            updates: Arc::clone(&self.updates),
            admission: Arc::clone(&self.admission),
            input_frame_due: None,
            _scratch: self.scratch.take(),
        };
        push_update(&task.updates, task.worker.update(None, true));
        Ok(Some(task))
    }
}
impl Drop for OpeningWorkerLoop {
    fn drop(&mut self) {
        // Renderer cancellation must finish before its private scratch directory is released.
        self.pending.take();
    }
}
