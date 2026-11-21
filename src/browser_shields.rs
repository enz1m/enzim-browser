use super::*;
use iced::widget::{Space, column, row, text};
#[derive(Clone, Debug)]
pub(crate) enum ShieldCommand {
    Mode(usize),
    Prepared(
        u64,
        BrowserAction,
        Result<(usize, Vec<std::sync::Arc<crate::filters::Source>>), String>,
    ),
    Ready(
        u64,
        BrowserAction,
        Result<crate::native_jobs::Payload, String>,
    ),
}
impl App {
    pub(super) fn shield_command(&mut self, command: ShieldCommand) -> Task<Message> {
        match command {
            ShieldCommand::Prepared(key, action, result) => {
                #[cfg(feature = "benchmark")]
                crate::webview_native::trace_navigation("rules-prepared");
                let Some(host) = self.navigation_host(key) else {
                    return Task::none();
                };
                let (mode, json) = match result {
                    Ok(value) => value,
                    Err(error) => {
                        host.borrow_mut().native_error = Some(error);
                        return Task::none();
                    }
                };
                let (job, receiver) = crate::native_jobs::Job::new();
                let service = host.borrow();
                if let (Some(native), Some(view)) = (&service.native, service.live_view) {
                    let filter_action = if json.len() == 1 {
                        BrowserAction::ContentFilter {
                            mode,
                            json: std::sync::Arc::clone(&json[0]),
                            job,
                        }
                    } else {
                        BrowserAction::ContentFilters {
                            mode,
                            filters: json,
                            job,
                        }
                    };
                    let _ = native.browse(&service.router, view, filter_action);
                    return Task::perform(crate::native_jobs::receive(receiver), move |result| {
                        c(Command::Shields(ShieldCommand::Ready(key, action, result)))
                    });
                }
            }
            ShieldCommand::Ready(key, action, result) => {
                #[cfg(feature = "benchmark")]
                crate::webview_native::trace_navigation("filters-ready");
                let Some(host) = self.navigation_host(key) else {
                    return Task::none();
                };
                match result {
                    Ok(_) => {
                        let mut service = host.borrow_mut();
                        if let (Some(native), Some(view)) = (&service.native, service.live_view) {
                            service.native_error =
                                native.browse(&service.router, view, action).err();
                        }
                    }
                    Err(error) => {
                        host.borrow_mut().native_error = Some(format!(
                            "Privacy protection could not be prepared: {error}. Try again, or change Privacy settings."
                        ));
                    }
                }
            }
            ShieldCommand::Mode(mode) => {
                self.browser.saved.privacy.blocking = mode.min(2);
                self.browser.save();
                if InternalPage::from_url(&self.browser.saved.tabs[self.browser.saved.active].url)
                    .is_none()
                {
                    self.submit_browser_action(BrowserAction::Reload);
                }
            }
        }
        Task::none()
    }
    pub(super) fn navigation_host(&self, key: u64) -> Option<&RefCell<WebViewService>> {
        let index = self
            .browser
            .saved
            .tabs
            .iter()
            .position(|tab| tab.key == key)?;
        if index == self.browser.saved.active {
            Some(&self.webview)
        } else {
            self.browser
                .sleeping
                .get(index)?
                .as_ref()
                .map(|tab| &tab.host)
        }
    }
    pub(crate) fn prepare_navigation(&mut self, action: BrowserAction) {
        let key = self.browser.saved.tabs[self.browser.saved.active].key;
        let privacy = self.effective_privacy(self.browser.saved.active);
        self.prepare_policy_navigation(key, action, privacy);
    }
    pub(super) fn prepare_policy_navigation(
        &mut self,
        key: u64,
        action: BrowserAction,
        privacy: crate::privacy::Privacy,
    ) {
        #[cfg(feature = "benchmark")]
        crate::webview_native::trace_navigation("prepare-rules");
        self.pending_tasks.push(Task::perform(
            async move {
                let (send, receive) = iced::futures::channel::oneshot::channel();
                std::thread::Builder::new()
                    .name("privacy-rules".into())
                    .spawn(move || {
                        let _ = send.send(crate::filters::for_privacy(&privacy));
                    })
                    .map_err(|e| e.to_string())?;
                receive
                    .await
                    .map_err(|_| "Protection preparation was interrupted".to_owned())?
            },
            move |result| {
                c(Command::Shields(ShieldCommand::Prepared(
                    key, action, result,
                )))
            },
        ));
    }
    pub(super) fn shields_settings(&self, scope: &str) -> Element<'_, Message> {
        let mode = self.browser.saved.privacy.blocking.min(2);
        let labels = vec!["Off".into(), "Balanced".into(), "Strict".into()];
        let select = enzui_widgets::combo_box::enz_combo_box(
            "",
            labels.clone(),
            Some(labels[mode].clone()),
            |_| c(Command::Shields(ShieldCommand::Mode(1))),
        )
        .id_prefix(format!("{scope}-blocking-mode"))
        .aria_label("Ad and tracker blocking")
        .selected_index(Some(mode))
        .on_select_index(|i| c(Command::Shields(ShieldCommand::Mode(i))))
        .width(180)
        .into_element();
        let body = column![
            enzui_widgets::preferences_row::enz_preferences_row("Protection level")
                .icon("shield-check")
                .subtitle(
                    "Balanced for everyday browsing; Strict also blocks first-party requests."
                )
                .control(select)
                .accessory_width(180.)
                .into_element()
        ]
        .spacing(12);
        enzui_widgets::preferences_group::enz_preferences_group(body)
            .title("Privacy")
            .id(iced::advanced::widget::Id::from(format!(
                "{scope}-privacy-group"
            )))
            .bordered(true)
            .into_element()
    }
    pub(super) fn shields_panel(&self) -> Element<'_, Message> {
        let url = &self.browser.saved.tabs[self.browser.saved.active].url;
        let has_origin = crate::privacy::Privacy::origin(url).is_some();
        let mut header = row![
            glyph("shield-check", 17.),
            text("Privacy").size(16),
            Space::new().width(Length::Fill)
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);
        if has_origin {
            header = header.push(
                enzui_widgets::toolbar_icon::enz_toolbar_icon(
                    "rotate-ccw",
                    c(Command::Privacy(PrivacyCommand::ResetSite)),
                )
                .aria_label("Reset site choices")
                .tooltip("Reset site choices")
                .size(28., 16.)
                .into_element(),
            );
        }
        let mut body = column![
            header,
            text(site_name(url))
                .size(12)
                .style(|theme| iced::widget::text::Style {
                    color: Some(palette(theme).text_muted)
                })
        ]
        .spacing(4);
        if !has_origin {
            return body
                .push(text("Open a website to adjust its privacy controls.").size(13))
                .into();
        }

        let tokens = enzui_widgets::preferences_row::PreferencesRowTokens {
            row_height: 32.,
            row_height_with_subtitle: 42.,
            wrap_vertical_padding: 2.,
            ..popup_row_tokens()
        };
        for (key, title, symbol) in super::privacy_settings::CONTROLS {
            let route_blocked = key == "webrtc"
                && self.container_uses_proxy(
                    self.browser.saved.tabs[self.browser.saved.active].container,
                );
            let unavailable = key == "drm" && !crate::privacy::encrypted_media_available();
            let value = self
                .effective_privacy(self.browser.saved.active)
                .value_for(key, url);
            body = body.push(
                enzui_widgets::preferences_row::enz_preferences_row(title)
                    .tokens(tokens)
                    .switch_hover(false)
                    .icon(symbol)
                    .subtitle(if unavailable {
                        "Encrypted media is unavailable in this runtime."
                    } else if route_blocked {
                        "Unavailable in proxy containers to prevent direct real-time connections."
                    } else {
                        super::privacy_settings::control_help(key)
                    })
                    .enabled(!route_blocked && !unavailable)
                    .switch(value, move |_| {
                        c(Command::Privacy(PrivacyCommand::Site(key)))
                    })
                    .into_element(),
            );
        }
        body.into()
    }
}
