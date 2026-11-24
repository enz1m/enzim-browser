impl Worker {
    fn credential_settings(&self) -> Option<crate::credentials::Settings> {
        crate::credentials::config::load(&crate::credentials::config::path())
            .ok()
            .filter(|settings| {
                settings.enabled
                    && settings.setup_provider.is_none()
                    && settings.provider == "enzim.builtin"
                    && (!self.credential_private || settings.private_fill)
            })
    }
    fn configure_credential_document(&mut self) {
        let settings = self.credential_settings();
        if settings.is_none() && !self.credential_active {
            return;
        }
        if settings.is_some() {
            if !self.session.register_secret_message_handler(
                crate::credential_bridge::HANDLER,
                crate::credential_bridge::WORLD,
            ) {
                return;
            }
            self.credential_active = true;
        }
        self.credential_active = settings.is_some();
        let source = settings.map_or_else(
            || "globalThis.__enzimCredentials?.disable();undefined".to_owned(),
            |settings| {
                format!(
                    "{};globalThis.__enzimCredentials?.configure({});undefined",
                    crate::credential_bridge::SOURCE,
                    settings.offer_save && !self.credential_private
                )
            },
        );
        let _ = self.session.evaluate_secret_script(
            &enzui_security::secret::SecretText::new(source),
            crate::credential_bridge::WORLD,
        );
    }
    /// Hands a passkey reply to the relay in the frame that asked, while it
    /// still shows the document that asked.
    fn passkey_reply(
        &mut self,
        frame: &enzui_webkit::page::FrameDocument,
        source: &enzui_security::secret::SecretText,
    ) {
        if self.native_operations.len() >= 16 {
            return;
        }
        let Ok(mut evaluation) = self.session.evaluate_secret_script_in_frame(
            frame,
            source,
            crate::credentials::passkey::WORLD,
        ) else {
            return;
        };
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        self.native_operations.push(Box::new(move |_| {
            evaluation.take().is_some() || std::time::Instant::now() >= deadline
        }));
    }
    fn credential_job(
        &mut self,
        lease: crate::credentials::service::Lease,
        site: crate::credentials::Site,
        profile: enzui_webkit::web::WebProfile,
        source: enzui_security::secret::SecretText,
        job: crate::native_jobs::Job,
    ) {
        self.edited = true;
        let permitted = lease.authorize()
            && self.profile.shares_storage(&profile)
            && self.visible
            && !self.session.is_loading()
            && self
                .credential_settings()
                .is_some_and(|settings| settings.autofill)
            && self
                .session
                .connection_tls_info()
                .is_some_and(|tls| tls.errors == 0)
            && !self.certificate_exception_covers(self.session.committed_uri().as_deref())
            && self
                .session
                .current_uri()
                .ok()
                .flatten()
                .and_then(|uri| crate::credentials::Site::parse(&uri, site.container).ok())
                .is_some_and(|current| current.matches_page(&site));
        if !permitted || self.native_operations.len() >= 16 {
            job.complete(Err("Autofill requires the current, fully loaded HTTPS website with a valid certificate and an enabled provider.".into()));
            return;
        }
        let Ok(mut evaluation) = self
            .session
            .evaluate_secret_script(&source, crate::credential_bridge::WORLD)
        else {
            job.complete(Err("The private form operation could not start.".into()));
            return;
        };
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        self.native_operations.push(Box::new(move |worker| {
            if !lease.valid()
                || job.closed()
                || std::time::Instant::now() >= deadline
                || worker
                    .credential_settings()
                    .is_none_or(|settings| !settings.autofill)
            {
                job.complete(Err(
                    "The credential operation expired or was disabled.".into()
                ));
                return true;
            }
            if let Some(result) = evaluation.take() {
                job.complete(
                    result
                        .map(crate::native_jobs::Payload::SecretJson)
                        .map_err(|_| {
                            "The document changed or refused the private form operation.".to_owned()
                        }),
                );
                return true;
            }
            false
        }));
    }
}
