//! The browser's side of a page's passkey request: the prompt, and the steps
//! between the request and its answer.

use super::*;
use crate::credentials::passkey::{Failure, Incoming, Keeper, Operation as Wanted, Request};
use iced::widget::{Space, column, container, row, text};

/// A page's request waiting for the user.
pub(crate) struct Pending {
    request: Request,
    frame: enzui_webkit::page::FrameDocument,
    /// The tab that asked; an answer is only ever given while it is shown.
    tab: u64,
    container: Option<u64>,
    stage: Stage,
}
enum Stage {
    /// Finding the passkeys for the site, or waiting for the store to open.
    Looking,
    /// A new passkey waits for the user to say where it is kept.
    Confirm,
    /// The user chooses among the passkeys of these logins.
    Choose(Vec<credentials::provider::Summary>),
    /// No saved passkey answers the request.
    Missing,
    /// The chosen login is being read, or the new one saved. A new passkey's
    /// answer is kept until its login is saved.
    Working(Option<serde_json::Value>),
}

fn registration_excluded(pending: &Pending, catalog: &[credentials::provider::Summary]) -> bool {
    let Wanted::Create { exclude, .. } = &pending.request.operation else {
        return false;
    };
    catalog.iter().any(|entry| {
        entry.site.container == pending.container
            && entry.passkey.as_ref().is_some_and(|(rp, id)| {
                rp == &pending.request.rp_id
                    && credentials::passkey::credential_id_matches(exclude, id)
            })
    })
}

impl App {
    fn passkey_answer(
        &mut self,
        frame: enzui_webkit::page::FrameDocument,
        id: u64,
        result: Result<serde_json::Value, Failure>,
    ) {
        self.submit_browser_action(BrowserAction::PasskeyReply {
            frame,
            source: credentials::passkey::reply_script(id, result),
        });
    }
    /// Ends the pending request with `result` and closes its prompt.
    fn passkey_finish(&mut self, result: Result<serde_json::Value, Failure>) {
        let Some(pending) = self.browser.credentials.passkey.take() else {
            return;
        };
        super::inline::credential_trace(format_args!(
            "passkey answer: {}",
            if result.is_ok() { "signed" } else { "refused" }
        ));
        if self.browser.panel == Panel::Passkey {
            self.browser.panel = Panel::None;
        }
        if self.browser.saved.tabs[self.browser.saved.active].key == pending.tab {
            self.passkey_answer(pending.frame, pending.request.id, result);
        }
    }
    /// The user closed the prompt, or left the page that asked.
    pub(crate) fn passkey_cancel(&mut self) {
        self.passkey_finish(Err(Failure::NOT_ALLOWED));
    }

    /// A message from the passkey relay of a frame of the shown page, with
    /// the origins the engine recorded for that frame.
    pub(crate) fn passkey_message(
        &mut self,
        frame: enzui_webkit::page::FrameDocument,
        origin: &str,
        top_origin: &str,
        main_frame: bool,
        json: &enzui_security::secret::SecretText,
    ) -> Task<Message> {
        let Some(incoming) = Incoming::parse(json.expose_secret(), origin, top_origin, main_frame)
        else {
            return Task::none();
        };
        let tab = &self.browser.saved.tabs[self.browser.saved.active];
        let (tab, container) = (tab.key, tab.container);
        match incoming {
            Incoming::Refused(id, failure) => self.passkey_answer(frame, id, Err(failure)),
            Incoming::Cancel(id) => {
                if self
                    .browser
                    .credentials
                    .passkey
                    .as_ref()
                    .is_some_and(|pending| pending.request.id == id && pending.frame == frame)
                {
                    // The page gave up: it has already settled its own call.
                    self.browser.credentials.passkey = None;
                    if self.browser.panel == Panel::Passkey {
                        self.browser.panel = Panel::None;
                    }
                }
            }
            Incoming::Request(request) => {
                let state = &self.browser.credentials;
                let creating = matches!(request.operation, Wanted::Create { .. });
                let secure = crate::webview_native::certificate_verified(
                    self.webview.borrow().tls.as_deref(),
                );
                super::inline::credential_trace(format_args!(
                    "passkey request: create={creating} rp={} enabled={} setup_pending={} already_pending={} library_open={} secure={secure} private={}",
                    request.rp_id,
                    state.settings.enabled,
                    state.settings.setup_provider.is_some(),
                    state.passkey.is_some(),
                    self.browser.library.is_some(),
                    self.browser.private
                ));
                if !state.settings.enabled
                    || state.settings.setup_provider.is_some()
                    || state.passkey.is_some()
                    || self.browser.library.is_some()
                    || !secure
                    || (self.browser.private && (creating || !state.settings.private_fill))
                {
                    self.passkey_answer(frame, request.id, Err(Failure::NOT_ALLOWED));
                    return Task::none();
                }
                let state = &mut self.browser.credentials;
                state.destination = credentials::router::default_destination(&state.settings);
                state.notice.clear();
                state.passkey = Some(Pending {
                    request: *request,
                    frame,
                    tab,
                    container,
                    stage: Stage::Looking,
                });
                self.browser.panel = Panel::Passkey;
                self.suspend_webpage_keyboard();
                return self.passkey_advance();
            }
        }
        Task::none()
    }

    /// Moves the pending request on once the store is open and its logins
    /// are known.
    pub(crate) fn passkey_advance(&mut self) -> Task<Message> {
        let state = &mut self.browser.credentials;
        let Some(pending) = state.passkey.as_mut() else {
            return Task::none();
        };
        if !matches!(pending.stage, Stage::Looking) || state.busy {
            return Task::none();
        }
        if !state.unlocked {
            return self.credential_desktop_unlock(true);
        }
        if !state.catalog_loaded {
            return self.credential_request(Purpose::List, Operation::List(None));
        }
        let held = |entry: &&credentials::provider::Summary| {
            entry.site.container == pending.container
                && entry
                    .passkey
                    .as_ref()
                    .is_some_and(|(rp, _)| *rp == pending.request.rp_id)
        };
        match &pending.request.operation {
            Wanted::Create { .. } => pending.stage = Stage::Confirm,
            Wanted::Get { allow } => {
                let found: Vec<_> = state
                    .catalog
                    .iter()
                    .filter(held)
                    .filter(|entry| {
                        allow.is_empty()
                            || entry
                                .passkey
                                .as_ref()
                                .is_some_and(|(_, id)| allow.iter().any(|allowed| allowed == id))
                    })
                    .cloned()
                    .collect();
                super::inline::credential_trace(format_args!(
                    "passkey get: rp={} allow={} saved_with_passkey={} same_rp_and_container={} found={}",
                    pending.request.rp_id,
                    allow.len(),
                    state.catalog.iter().filter(|entry| entry.passkey.is_some()).count(),
                    state.catalog.iter().filter(held).count(),
                    found.len()
                ));
                pending.stage = if found.is_empty() {
                    Stage::Missing
                } else {
                    Stage::Choose(found)
                };
            }
        }
        Task::none()
    }

    fn passkey_keeper(&self, source: &str) -> Keeper {
        if self
            .browser
            .credentials
            .settings
            .accounts
            .iter()
            .any(|account| account.id == source)
        {
            Keeper::Bitwarden
        } else {
            Keeper::Local
        }
    }

    pub(crate) fn passkey_command(&mut self, action: &CredentialCommand) -> Task<Message> {
        match action {
            CredentialCommand::PasskeyAdvance => self.passkey_advance(),
            CredentialCommand::PasskeyCancel => {
                self.passkey_cancel();
                Task::none()
            }
            CredentialCommand::PasskeyUse(id) => {
                let state = &mut self.browser.credentials;
                let Some(pending) = state.passkey.as_mut() else {
                    return Task::none();
                };
                if !matches!(&pending.stage, Stage::Choose(found) if found.iter().any(|entry| entry.id == *id))
                    || state.busy
                {
                    return Task::none();
                }
                pending.stage = Stage::Working(None);
                self.credential_request(Purpose::PasskeyRead, Operation::ReadSelected(id.clone()))
            }
            CredentialCommand::PasskeyCreate => {
                let destination = self.browser.credentials.destination.clone();
                let keeper = self.passkey_keeper(&destination);
                let state = &mut self.browser.credentials;
                let Some(pending) = state.passkey.as_mut() else {
                    return Task::none();
                };
                if !matches!(pending.stage, Stage::Confirm) || state.busy {
                    return Task::none();
                }
                if registration_excluded(pending, &state.catalog) {
                    self.passkey_finish(Err(Failure::EXCLUDED));
                    self.browser
                        .toasts
                        .done("This account already has a passkey saved here.");
                    return Task::none();
                }
                let made = pending.request.create(keeper).and_then(|(passkey, reply)| {
                    let mut login =
                        Credential::new(Site::parse(&pending.request.origin, pending.container)?)?;
                    login.name = Secret::new(if passkey.rp_name.is_empty() {
                        passkey.rp_id.clone()
                    } else {
                        passkey.rp_name.clone()
                    });
                    login.username = Secret::new(passkey.user_name.clone());
                    login.passkey = Some(Box::new(passkey));
                    Ok((login, reply))
                });
                match made {
                    Ok((login, reply)) => {
                        pending.stage = Stage::Working(Some(reply));
                        self.credential_request(
                            Purpose::PasskeySave,
                            Operation::SaveTo(destination, login),
                        )
                    }
                    Err(error) => {
                        state.notice = error;
                        self.passkey_cancel();
                        Task::none()
                    }
                }
            }
            _ => Task::none(),
        }
    }

    /// A store operation started for the pending request has ended.
    pub(crate) fn passkey_ready(
        &mut self,
        purpose: Purpose,
        result: Result<Output, String>,
    ) -> Task<Message> {
        let state = &mut self.browser.credentials;
        let Some(pending) = state.passkey.as_mut() else {
            return Task::none();
        };
        let stage = std::mem::replace(&mut pending.stage, Stage::Looking);
        match (purpose, result, stage) {
            (Purpose::PasskeyRead, Ok(Output::Entry(login)), Stage::Working(None)) => {
                let (source, _) = credentials::router::split_reference(&login.id)
                    .unwrap_or_else(|_| ("browser".into(), String::new()));
                let request = pending.request.clone();
                let answer = login
                    .passkey
                    .as_ref()
                    .filter(|_| login.site.container == pending.container)
                    .ok_or_else(|| "This login no longer has a passkey.".to_owned())
                    .and_then(|passkey| {
                        super::inline::credential_trace(format_args!(
                            "passkey sign: rp={} counter={} source={source}",
                            passkey.rp_id, passkey.counter
                        ));
                        request.assert(passkey, self.passkey_keeper(&source))
                    });
                match answer {
                    Ok(reply) => self.passkey_finish(Ok(reply)),
                    Err(error) => {
                        self.passkey_cancel();
                        self.browser.toasts.error("passkey", error);
                    }
                }
            }
            (Purpose::PasskeySave, Ok(_), Stage::Working(Some(reply))) => {
                state.catalog_loaded = false;
                self.passkey_finish(Ok(reply));
                self.browser.toasts.done("Passkey saved.");
            }
            (_, Err(error), _) => {
                self.passkey_cancel();
                self.browser.toasts.error("passkey", error);
            }
            _ => self.passkey_cancel(),
        }
        Task::none()
    }

    pub(crate) fn passkey_dialog(&self) -> Element<'_, Message> {
        let state = &self.browser.credentials;
        let Some(pending) = &state.passkey else {
            return Space::new().into();
        };
        let cancel = || {
            enzui_widgets::button::enz_button(
                "Cancel",
                c(command(CredentialCommand::PasskeyCancel)),
            )
                .kind(enzui_widgets::button::ButtonKind::Quiet)
                .size(enzui_widgets::button::ButtonSize::Md)
                .into_element()
        };
        let creating = matches!(pending.request.operation, Wanted::Create { .. });
        let mut body = column![heading(
            "key-round",
            if creating {
                "Create a passkey"
            } else {
                "Sign in with a passkey"
            },
            pending.request.rp_id.clone(),
        )]
        .spacing(16);
        let mut actions = row![Space::new().width(Length::Fill)].spacing(10);
        match &pending.stage {
            Stage::Looking if !state.unlocked && !state.busy => {
                body = body.push(
                    text("Unlock your passwords to continue.")
                        .size(14)
                        .color(palette(&self.theme()).text_muted),
                );
                body = body.push(self.credential_unlock_view());
                actions = actions.push(cancel());
            }
            Stage::Looking | Stage::Working(_) => {
                body = body.push(text("One moment…").size(14));
                actions = actions.push(cancel());
            }
            Stage::Missing => {
                body = body.push(
                    text("No passkey for this website is saved in your password manager.").size(14),
                );
                actions = actions.push(action(
                    "Close",
                    "x",
                    command(CredentialCommand::PasskeyCancel),
                ));
            }
            Stage::Confirm => {
                if let Wanted::Create {
                    user_name,
                    user_display_name,
                    ..
                } = &pending.request.operation
                {
                    let account = if user_name.is_empty() {
                        user_display_name
                    } else {
                        user_name
                    };
                    body = body.push(pref(
                        account.clone(),
                        "user",
                        "A passkey signs you in to this website without a password. It is kept with your saved logins.",
                    ));
                }
                body = body.push(self.credential_destination_picker(false));
                actions = actions.push(cancel()).push(primary(
                    "Create passkey",
                    "key-round",
                    command(CredentialCommand::PasskeyCreate),
                ));
            }
            Stage::Choose(found) => {
                let mut choices = column![].spacing(6);
                for entry in found.iter().take(12) {
                    let name = if entry.username.is_empty() {
                        entry.name.expose().to_owned()
                    } else {
                        entry.username.expose().to_owned()
                    };
                    choices = choices.push(
                        enzui_widgets::preferences_row::enz_preferences_row(name)
                            .icon("key-round")
                            .subtitle(entry.source.clone())
                            .action(
                                "Use",
                                message(CredentialCommand::PasskeyUse(entry.id.clone())),
                            )
                            .into_element(),
                    );
                }
                body = body.push(choices);
                actions = actions.push(cancel());
            }
        }
        if !state.notice.is_empty() {
            body = body.push(text(&state.notice).size(13));
        }
        container(body.push(actions))
            .padding(20)
            .width((self.browser.viewport.width - 32.0).min(460.0))
            .style(|theme| {
                let p = palette(theme);
                container::Style {
                    background: Some(p.surface_elevated.into()),
                    border: iced::Border {
                        radius: 14.0.into(),
                        color: p.border,
                        width: 1.0,
                    },
                    ..Default::default()
                }
            })
            .into()
    }
}
