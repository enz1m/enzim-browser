fn resource_value<T>(field: enzui_webkit::resource::ResourceField<T>) -> Option<T> {
    match field {
        enzui_webkit::resource::ResourceField::Available(value) => Some(value),
        enzui_webkit::resource::ResourceField::Unavailable(_) => None,
    }
}
fn resource_headers(
    field: enzui_webkit::resource::ResourceField<Vec<enzui_webkit::resource::ResourceHeader>>,
) -> serde_json::Value {
    resource_value(field).map_or(serde_json::Value::Null, |headers| {
        serde_json::json!(
            headers
                .into_iter()
                .map(|header| (header.name, header.value))
                .collect::<std::collections::BTreeMap<_, _>>()
        )
    })
}
fn resource_event_json(event: enzui_webkit::resource::ResourceEvent) -> serde_json::Value {
    use enzui_webkit::resource::ResourceEventKind;
    let mut value = serde_json::json!({"log":event.log_id,"navigation":event.navigation,"id":event.resource_id,"elapsed":event.elapsed.as_secs_f64()*1000.});
    let fields = match event.kind {
        ResourceEventKind::Started(request) => {
            serde_json::json!({"event":"started","url":resource_value(request.url),"method":resource_value(request.method),"request_headers":resource_headers(request.headers)})
        }
        ResourceEventKind::Sent {
            request,
            redirect_status,
        } => {
            serde_json::json!({"event":"sent","url":resource_value(request.url),"method":resource_value(request.method),"request_headers":resource_headers(request.headers),"redirect_status":redirect_status})
        }
        ResourceEventKind::Response(response) => {
            serde_json::json!({"event":"response","status":resource_value(response.status),"mime":resource_value(response.mime_type),"length":resource_value(response.content_length),"response_headers":resource_headers(response.headers)})
        }
        ResourceEventKind::Failed { domain, code } => {
            serde_json::json!({"event":"failed","failed":true,"failure":format!("Native error {domain}:{code}")})
        }
        ResourceEventKind::TlsFailed { flags } => {
            serde_json::json!({"event":"failed","failed":true,"failure":format!("TLS error flags {flags:#x}")})
        }
        ResourceEventKind::Finished { failed } => {
            serde_json::json!({"event":"finished","failed":failed})
        }
    };
    if let (Some(target), Some(fields)) = (value.as_object_mut(), fields.as_object()) {
        target.extend(
            fields
                .iter()
                .map(|(key, value)| (key.clone(), value.clone())),
        );
    }
    value
}
impl Worker {
    fn configure_developer_tools(&mut self, enabled: bool) {
        if self.developer_tools == enabled {
            return;
        }
        let page = enzui_webkit::page::ScriptWorld::Page;
        let inspector = enzui_webkit::page::ScriptWorld::Isolated("enzim-developer-tools".into());
        if enabled
            && (!self.session.register_message_handler("enzimConsole", &page)
                || !self
                    .session
                    .register_message_handler("enzimInspect", &inspector))
        {
            self.session
                .unregister_message_handler("enzimConsole", &page);
            self.session
                .unregister_message_handler("enzimInspect", &inspector);
            self.resource_notice = Some("Could not attach developer tools to this page".into());
            return;
        }
        self.developer_tools = enabled;
        let address = self.metadata_snapshot().0.location.clone();
        self.privacy.apply_for_site(
            &self.session,
            address.as_deref().unwrap_or_default(),
            enabled,
            &self.default_user_agent,
        );
        if enabled && self.resource_log.is_none() {
            match self.session.open_resource_log() {
                Ok(log) => self.resource_log = Some(log),
                Err(error) => self.resource_notice = Some(error.to_string()),
            }
        } else if !enabled {
            self.resource_log = None;
        }
        self.refresh_document_scripts(address.as_deref());
        if !enabled {
            let _ = self
                .session
                .evaluate_script("globalThis.__enzimDisposeInspector?.()", &inspector);
        }
        let source = if enabled {
            include_str!("console.js")
        } else {
            "globalThis[Symbol.for('enzim.devtools.console')]?.stop()"
        };
        let _ = self.session.evaluate_script(source, &page);
        if !enabled {
            self.session
                .unregister_message_handler("enzimConsole", &page);
            self.session
                .unregister_message_handler("enzimInspect", &inspector);
        }
    }
    fn collect_resource_events(&mut self, page_events: &mut Vec<enzui_webkit::page::PageEvent>) {
        if let Some(error) = self.resource_notice.take() {
            page_events.push(enzui_webkit::page::PageEvent::ScriptMessage {
                handler: "enzimNativeResources".into(),
                json: serde_json::json!({"error":error}).to_string(),
            });
        }
        if let Some(log) = &self.resource_log {
            let events = log.take_events();
            if !events.is_empty() {
                let json = serde_json::json!({"events":events.into_iter().map(resource_event_json).collect::<Vec<_>>(),"dropped":log.dropped()}).to_string();
                page_events.push(enzui_webkit::page::PageEvent::ScriptMessage {
                    handler: "enzimNativeResources".into(),
                    json,
                });
            }
        }
    }
    fn export_page_job(
        &mut self,
        format: enzui_webkit::page_export::PageExportFormat,
        job: crate::native_jobs::Job,
    ) {
        let task = match self.session.export_page(
            format,
            &enzui_webkit::page_export::PageExportOptions {
                max_bytes: 16 * 1024 * 1024,
                ..Default::default()
            },
        ) {
            Ok(task) => task,
            Err(error) => {
                job.complete(Err(error.to_string()));
                return;
            }
        };
        self.native_operations.push(Box::new(move |_| {
            if job.closed() {
                task.cancel();
                return true;
            }
            if let Some(result) = task.take_result() {
                job.complete(
                    result
                        .map(|artifact| {
                            crate::native_jobs::Payload::Bytes(Arc::new(
                                artifact.as_bytes().to_vec(),
                            ))
                        })
                        .map_err(|error| error.to_string()),
                );
                true
            } else {
                false
            }
        }));
    }
}
