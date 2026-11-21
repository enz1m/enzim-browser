#[derive(Default)]
struct NativeFavicons {
    enabled: bool,
    document: Option<String>,
    task: Option<enzui_webkit::ffi::runtime::FaviconTask>,
    due: Option<std::time::Instant>,
    expires: Option<std::time::Instant>,
    attempts: u8,
}
impl NativeFavicons {
    fn new(session: &WebSession) -> Self {
        Self {
            enabled: session.set_favicons_enabled(true).is_ok(),
            ..Self::default()
        }
    }
    fn reset(&mut self) {
        if let Some(task) = self.task.take() {
            task.cancel();
        }
        self.document = None;
        self.due = None;
        self.expires = None;
        self.attempts = 0;
    }
    fn pending(&self) -> bool {
        self.due.is_some_and(|due| std::time::Instant::now() >= due)
            || self
                .task
                .as_ref()
                .is_some_and(enzui_webkit::ffi::runtime::FaviconTask::is_finished)
    }
    fn poll(
        &mut self,
        session: &WebSession,
        view: ViewId,
        address: Option<&str>,
        privacy: &crate::privacy::Privacy,
    ) -> Option<enzui_webkit::page::PageEvent> {
        use base64::Engine;
        let Some(address) = address.filter(|uri| {
            url::Url::parse(uri).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
        }) else {
            self.reset();
            return None;
        };
        if !self.enabled {
            return None;
        }
        let now = std::time::Instant::now();
        if self.document.as_deref() != Some(address) {
            self.reset();
            self.document = Some(address.to_owned());
            self.expires = Some(now + Duration::from_secs(20));
            self.due = Some(now);
        }
        if self.expires.is_some_and(|end| now >= end) {
            if let Some(task) = self.task.take() {
                task.cancel();
            }
            self.due = None;
            self.expires = None;
            return None;
        }
        if let Some(task) = &self.task {
            if let Some(result) = task.take_result() {
                self.task = None;
                if let Ok(icon) = result {
                    self.due = None;
                    self.expires = None;
                    if icon.view != Some(view)
                        || icon.document_uri != address
                        || !crate::filters::allows_image(
                            &icon.source_uri,
                            address,
                            privacy.blocking_for(address),
                        )
                    {
                        return None;
                    }
                    let data = base64::engine::general_purpose::STANDARD.encode(&icon.png);
                    return Some(enzui_webkit::page::PageEvent::ScriptMessage {
                        handler: "enzimIcon".into(),
                        json: serde_json::json!({"url":icon.document_uri,"data":format!("data:image/png;base64,{data}")}).to_string(),
                    });
                }
                self.due = Some(now + Duration::from_millis(250));
            } else {
                self.due = Some(now + Duration::from_millis(50));
                return None;
            }
        }
        if self.due.is_none_or(|due| now < due) {
            return None;
        }
        if self.attempts >= 8 {
            self.due = None;
            self.expires = None;
            return None;
        }
        if session.is_loading() {
            self.due = Some(now + Duration::from_millis(200));
            return None;
        }
        match session.favicon() {
            Ok(task) => {
                self.task = Some(task);
                self.attempts += 1;
                self.due = Some(now + Duration::from_millis(50));
            }
            Err(enzui_webkit::favicon::FaviconError::UnsupportedRuntime) => {
                self.enabled = false;
                self.due = None;
            }
            Err(_) => {
                self.attempts += 1;
                self.due = Some(now + Duration::from_millis(250));
            }
        }
        None
    }
}
