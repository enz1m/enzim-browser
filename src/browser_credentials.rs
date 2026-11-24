use super::*;
use crate::credentials::{
    self, Credential, Secret, Settings, Site,
    service::{Operation, Output, Reply, Service},
};
use enzui_widgets::toast::ToastTone;
use iced::widget::{Space, column, row, text};
#[path = "browser_credential_accounts.rs"]
mod accounts;
#[path = "browser_credential_inline.rs"]
mod inline;
#[path = "browser_passkeys.rs"]
mod passkeys;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Purpose {
    Unlock,
    DesktopUnlock,
    ForgetDesktop,
    InlineList(u64),
    Refresh,
    List,
    Edit,
    Save,
    Delete,
    Backup,
    Csv,
    Rekey,
    Import,
    Fill,
    Copy(bool),
    CheckOffer,
    OfferSave,
    UpdateOffer,
    PasskeyRead,
    PasskeySave,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TransferMode {
    None,
    Import,
    Backup,
    Csv,
    Rekey,
    Settings,
}
#[derive(Clone, Debug)]
pub(crate) enum CredentialCommand {
    PasskeyAdvance,
    PasskeyUse(String),
    PasskeyCreate,
    PasskeyCancel,
    Initialize,
    CheckOffer,
    Open,
    OpenSite,
    OpenDestination,
    FillInline(u64, String, bool),
    DismissInline,
    Fill(String, bool),
    FillSelected(String),
    Copy(String, bool),
    /// Copies the open login's current authenticator code.
    CopyCode,
    View(String),
    /// Shows one login of the popup's list in the popup, or the list again.
    Show(Option<String>),
    /// Copies a login's website address, which is not a secret.
    CopySite(String),
    ClipboardWritten(u64, bool),
    FillReady(u64, u64, Result<crate::native_jobs::Payload, String>),
    SaveOffer,
    DismissOffer,
    Configured(
        u64,
        Settings,
        Result<credentials::config::Publication, String>,
    ),
    Enable(bool),
    Storage(usize),
    Destination(usize),
    DefaultDestination(usize),
    SourceSelect(usize),
    ProviderLogins(usize),
    Providers,
    ProviderPreferences,
    ProviderDefault(String),
    ProviderRemove(String),
    AddLocal,
    ConnectionTimeout(usize),
    ConnectionNew(credentials::accounts::Kind),
    ConnectionEdit(String),
    ConnectionField(&'static str, String),
    ConnectionToggle(&'static str, bool),
    ConnectionSave,
    ConnectionDelete,
    ConnectionCancel,
    Setting(&'static str, bool),
    Timeout(usize),
    Master(Secret),
    AuthenticatorCode(Secret),
    ApiKeyMode(bool),
    ApiClientId(Secret),
    ApiClientSecret(Secret),
    Confirmation(Secret),
    Current(Secret),
    Unlock,
    UnlockDesktop,
    ForgetDesktop,
    Lock,
    Tick,
    StatusChanged,
    Session(enzui_core::portal::session_monitor::SessionEvent),
    Query(Secret),
    Refresh,
    New,
    Edit(String),
    Field(&'static str, Secret),
    Container(usize),
    Reveal,
    Generate,
    Save,
    Back,
    DeleteRequest,
    DeleteConfirm,
    Transfer(TransferMode),
    Export,
    ImportChoose,
    ImportConfirm,
    FileChosen(u64, Vec<std::path::PathBuf>, bool),
    FileRead(u64, Result<Vec<Credential>, String>),
    FileWritten(u64, Result<(), String>),
    Ready(u64, Purpose, Reply),
}
impl CredentialCommand {
    pub(super) fn is_background_update(&self) -> bool {
        matches!(
            self,
            Self::Initialize
                | Self::CheckOffer
                | Self::Tick
                | Self::StatusChanged
                | Self::Session(_)
                | Self::Ready(..)
                | Self::Configured(..)
                | Self::FillReady(..)
                | Self::FileRead(..)
                | Self::FileWritten(..)
                | Self::ClipboardWritten(..)
        )
    }
}
struct Editor {
    login: Credential,
    origin: Secret,
    totp: Secret,
    reveal: bool,
    confirm_delete: bool,
}
impl Editor {
    fn authenticator(&self) -> Option<credentials::Totp> {
        credentials::Totp::parse(self.totp.expose()).ok()
    }
}
#[derive(Clone)]
struct FormDocument {
    profile: enzui_webkit::web::WebProfile,
    tab: u64,
    site: Site,
    document: crate::credential_bridge::Document,
    password: bool,
    otp: bool,
}
struct Candidate {
    login: Credential,
    /// The saved login the browser filled into the submitted form.
    filled: Option<String>,
    born: std::time::Instant,
    checked: bool,
    presented: bool,
}
pub(crate) struct State {
    pub(crate) settings: Settings,
    vault_exists: bool,
    document: Option<FormDocument>,
    inline: Option<inline::Offer>,
    inline_serial: u64,
    candidate: Option<Candidate>,
    /// Sites whose save offer the person turned down with "Not now"; not
    /// offered again until the browser restarts.
    declined: Vec<String>,
    fill_target: Option<(FormDocument, bool, Option<Site>)>,
    pub(crate) service: Option<std::sync::Arc<Service>>,
    config: std::path::PathBuf,
    config_identity: Option<enzui_core::state_authority::StateIdentity>,
    master: Secret,
    authenticator_code: Secret,
    api_key_mode: bool,
    api_client_id: Secret,
    api_client_secret: Secret,
    confirmation: Secret,
    current: Secret,
    query: Secret,
    entries: Vec<credentials::provider::Summary>,
    catalog: Vec<credentials::provider::Summary>,
    catalog_loaded: bool,
    editor: Option<Editor>,
    /// The login the popup shows instead of its list.
    shown: Option<String>,
    transfer: TransferMode,
    plaintext_confirmed: bool,
    review: Vec<Credential>,
    pending_export: Option<(
        std::sync::Arc<enzui_security::secret::Secret>,
        credentials::service::Lease,
    )>,
    busy: bool,
    pending_purpose: Option<Purpose>,
    unlocked: bool,
    desktop_attempted: bool,
    initialized: bool,
    sequence: u64,
    notice: String,
    connection: Option<accounts::Editor>,
    browse_logins: bool,
    provider_preferences: bool,
    removing_provider: Option<String>,
    destination: String,
    unlock_source: String,
    known_generation: u64,
    last_editor_touch: Option<std::time::Instant>,
    unlocking: bool,
    /// A page's passkey request waiting for the user.
    passkey: Option<passkeys::Pending>,
    /// The login the user was warned belongs to another website than the
    /// page, and may confirm filling.
    fill_mismatch: Option<String>,
    /// The page document a login was last filled into, and that login.
    filled: Option<(String, String)>,
}
impl Default for State {
    fn default() -> Self {
        let config = credentials::config::path();
        #[cfg(not(test))]
        let loaded = credentials::config::load_authorized(&config);
        #[cfg(test)]
        let loaded: Result<
            (Settings, Option<enzui_core::state_authority::StateIdentity>),
            String,
        > = Ok((Settings::default(), None));
        let (mut settings, config_identity, mut notice) = match loaded {
            Ok((settings, identity)) => (settings, identity, String::new()),
            Err(error) => (Settings::default(), None, error),
        };
        if settings.enabled
            && let Err(error) = credentials::protect_process()
        {
            settings.enabled = false;
            notice = error;
        }
        #[cfg(not(test))]
        let vault_exists = match std::fs::symlink_metadata(credentials::Vault::default_path()) {
            Ok(_) => true,
            Err(error) => error.kind() != std::io::ErrorKind::NotFound,
        };
        #[cfg(test)]
        let vault_exists = false;
        Self {
            vault_exists,
            destination: credentials::router::default_destination(&settings),
            unlock_source: credentials::router::default_unlock_source(&settings),
            connection: None,
            browse_logins: false,
            provider_preferences: false,
            removing_provider: None,
            known_generation: 0,
            last_editor_touch: None,
            unlocking: false,
            passkey: None,
            fill_mismatch: None,
            filled: None,
            settings,
            document: None,
            inline: None,
            inline_serial: 0,
            candidate: None,
            declined: Vec::new(),
            fill_target: None,
            service: None,
            config,
            config_identity,
            master: Secret::default(),
            authenticator_code: Secret::default(),
            api_key_mode: false,
            api_client_id: Secret::default(),
            api_client_secret: Secret::default(),
            confirmation: Secret::default(),
            current: Secret::default(),
            query: Secret::default(),
            entries: Vec::new(),
            catalog: Vec::new(),
            catalog_loaded: false,
            editor: None,
            shown: None,
            transfer: TransferMode::None,
            plaintext_confirmed: false,
            review: Vec::new(),
            pending_export: None,
            busy: false,
            pending_purpose: None,
            unlocked: false,
            desktop_attempted: false,
            initialized: false,
            sequence: 0,
            notice,
        }
    }
}
impl State {
    /// Whether a page's passkey request is waiting for the user.
    pub(crate) fn passkey_pending(&self) -> bool {
        self.passkey.is_some()
    }
    fn has_destination(&self) -> bool {
        self.vault_exists
            || self.settings.system_store
            || self.settings.storage == credentials::accounts::Storage::System
            || self.settings.accounts.iter().any(|account| account.enabled)
    }
    fn desktop_available(&self) -> bool {
        self.settings.desktop_unlock && self.vault_exists
            || self.settings.system_store
            || self.settings.storage == credentials::accounts::Storage::System
            || self.settings.accounts.iter().any(|account| {
                account.enabled && account.remember
            })
    }
    fn expiry_deadline(&self) -> Option<std::time::Instant> {
        self.candidate
            .as_ref()
            .map(|candidate| candidate.born + std::time::Duration::from_secs(60))
            .into_iter()
            .chain(
                self.pending_export
                    .as_ref()
                    .map(|(_, lease)| lease.expires_at()),
            )
            .min()
    }
    fn has_visible_totp(&self, manager_visible: bool) -> bool {
        manager_visible
            && self.unlocked
            && self
                .editor
                .as_ref()
                .is_some_and(|editor| editor.authenticator().is_some())
    }
    fn initialization_pending(&self, allowed: bool) -> bool {
        allowed
            && !self.busy
            && !self.initialized
            && self.settings.enabled
            && self.settings.provider == "enzim.builtin"
            && self.settings.setup_provider.is_none()
            && self.has_destination()
    }
    pub(crate) fn subscription(
        &self,
        manager_visible: bool,
        startup_allowed: bool,
    ) -> iced::Subscription<Message> {
        let startup = if self.initialization_pending(startup_allowed) {
            iced::Subscription::run(initialize_once)
                .map(|()| message(CredentialCommand::Initialize))
        } else {
            iced::Subscription::none()
        };
        let totp = if self.has_visible_totp(manager_visible) {
            iced::time::every(std::time::Duration::from_secs(1))
                .map(|_| message(CredentialCommand::Tick))
        } else {
            iced::Subscription::none()
        };
        let expiry = self
            .expiry_deadline()
            .map_or_else(iced::Subscription::none, |deadline| {
                iced::Subscription::run_with(deadline, expiry_wake)
                    .map(|()| message(CredentialCommand::Tick))
            });
        let status = self
            .service
            .as_ref()
            .map_or_else(iced::Subscription::none, |service| {
                service
                    .subscription()
                    .map(|()| message(CredentialCommand::StatusChanged))
            });
        iced::Subscription::batch([startup, totp, expiry, status])
    }
    pub(super) fn dismiss(&mut self) {
        self.dismiss_inline();
        self.master = Secret::default();
        self.clear_connection_auth();
        self.confirmation = Secret::default();
        self.current = Secret::default();
        self.query = Secret::default();
        self.entries.clear();
        self.candidate = None;
        self.fill_target = None;
        self.fill_mismatch = None;
        self.editor = None;
        self.shown = None;
        self.last_editor_touch = None;
        self.review.clear();
        self.pending_export = None;
        self.transfer = TransferMode::None;
        self.plaintext_confirmed = false;
        if !matches!(
            self.pending_purpose,
            Some(Purpose::DesktopUnlock | Purpose::List)
        ) {
            self.busy = false;
            self.pending_purpose = None;
            self.sequence = self.sequence.wrapping_add(1);
        }
    }
    fn clear_secrets(&mut self) {
        let settings_visible = self.transfer == TransferMode::Settings;
        self.dismiss_inline();
        self.candidate = None;
        self.fill_target = None;
        self.master = Secret::default();
        self.clear_connection_auth();
        self.confirmation = Secret::default();
        self.current = Secret::default();
        self.query = Secret::default();
        self.entries.clear();
        self.catalog.clear();
        self.catalog_loaded = false;
        self.editor = None;
        self.shown = None;
        self.filled = None;
        self.last_editor_touch = None;
        self.review.clear();
        self.pending_export = None;
        self.transfer = if settings_visible {
            TransferMode::Settings
        } else {
            TransferMode::None
        };
        self.plaintext_confirmed = false;
        self.unlocked = false;
        self.busy = false;
        self.pending_purpose = None;
        self.sequence = self.sequence.wrapping_add(1);
    }
    fn clear_connection_auth(&mut self) {
        self.authenticator_code = Secret::default();
        self.api_client_id = Secret::default();
        self.api_client_secret = Secret::default();
    }
    pub(crate) fn lock(&mut self) {
        self.lock_with_policy(false);
    }
    fn lock_with_policy(&mut self, manual: bool) {
        self.desktop_attempted = true;
        if let Some(service) = &self.service {
            if manual {
                service.lock_manually();
            } else {
                service.lock();
            }
        }
        self.clear_secrets();
    }
}
fn initialize_once() -> impl iced::futures::Stream<Item = ()> {
    iced::futures::stream::once(async {})
}
fn expiry_wake(
    deadline: &std::time::Instant,
) -> impl iced::futures::Stream<Item = ()> + Send + 'static + use<> {
    let deadline = *deadline;
    iced::futures::stream::once(async move {
        tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
    })
}
fn message(command: CredentialCommand) -> Message {
    c(Command::Credential(command))
}
fn command(command: CredentialCommand) -> Command {
    Command::Credential(command)
}
fn secret_field<'a>(
    label: &'a str,
    placeholder: &'a str,
    secret: &'a Secret,
    secure: bool,
    id: &'static str,
    on_input: impl Fn(Secret) -> CredentialCommand + 'a,
) -> Element<'a, Message> {
    enzui_widgets::text_field::enz_text_field(label, placeholder, secret.expose(), move |value| {
        message(on_input(Secret::new(value)))
    })
    .id(iced::widget::Id::new(id))
    .secure(secure)
    .on_submit_with(move || {
        message(match id {
            "credentials-search" => CredentialCommand::Query(secret.clone()),
            "vault-master"
            | "vault-confirmation"
            | "credential-popup-master"
            | "vault-authenticator"
            | "vault-api-id"
            | "vault-api-secret" => CredentialCommand::Unlock,
            "credential-name"
            | "credential-origin"
            | "credential-username"
            | "credential-password"
            | "credential-totp"
            | "credential-notes" => CredentialCommand::Save,
            "credential-import-password" => CredentialCommand::ImportChoose,
            _ => CredentialCommand::Export,
        })
    })
    .into_element()
}
fn password_section<'a>(
    title: &str,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    column![
        text(title.to_owned()).size(16),
        iced::widget::rule::horizontal(1),
        content.into()
    ]
    .spacing(12)
    .width(Length::Fill)
    .into()
}
impl App {
    pub(crate) fn credential_preferences_changed(
        &mut self,
        settings: &Settings,
        identity: enzui_core::state_authority::StateIdentity,
    ) {
        let state = &mut self.browser.credentials;
        if state.config_identity == Some(identity) {
            return;
        }
        state.lock();
        state.settings = settings.clone();
        state.config_identity = Some(identity);
        state.destination = credentials::router::default_destination(settings);
        state.unlock_source = credentials::router::default_unlock_source(settings);
        state.connection = None;
        state.desktop_attempted = false;
        state.initialized = false;
        if !settings.enabled {
            state.service = None;
        }
        self.refresh_credential_documents();
    }
    fn current_credential_document(&self) -> Option<&FormDocument> {
        let document = self.browser.credentials.document.as_ref()?;
        let current = &self.browser.saved.tabs[self.browser.saved.active];
        let host = self.webview.borrow();
        (self.browser.library.is_none()
            && host.is_visible()
            && host.native_error.is_none()
            && document.tab == current.key
            && !host.loading
            && document.profile.shares_storage(&host.profile)
            && host
                .location
                .as_deref()
                .and_then(|uri| Site::parse(uri, current.container).ok())
                .is_some_and(|site| site.matches_page(&document.site))
            && crate::webview_native::certificate_verified(host.tls.as_deref()))
        .then_some(document)
    }
    fn credential_save_settings(&mut self, settings: Settings) -> Task<Message> {
        let state = &mut self.browser.credentials;
        state.busy = true;
        state.sequence = state.sequence.wrapping_add(1);
        let sequence = state.sequence;
        let path = state.config.clone();
        let identity = state.config_identity;
        if !settings.enabled {
            state.settings.enabled = false;
        }
        self.refresh_credential_documents();
        let updated = settings.clone();
        super::import_page::background(
            "credential-settings",
            move || credentials::config::save_with_session_cleanup(&path, identity, &settings),
            move |result| {
                message(CredentialCommand::Configured(
                    sequence,
                    updated.clone(),
                    result,
                ))
            },
        )
    }
    fn credential_request(&mut self, purpose: Purpose, operation: Operation) -> Task<Message> {
        let state = &mut self.browser.credentials;
        if state.busy {
            return Task::none();
        }
        if state.service.is_none() {
            match Service::shared(credentials::Vault::default_path(), state.config.clone()) {
                Ok(service) => state.service = Some(service),
                Err(error) => {
                    state.notice = error;
                    return Task::none();
                }
            }
        }
        let Some(service) = state.service.as_ref() else {
            return Task::none();
        };
        let generation = service.generation();
        match service.request(operation) {
            Ok(receive) => {
                state.busy = true;
                state.pending_purpose = Some(purpose);
                state.notice.clear();
                state.sequence = state.sequence.wrapping_add(1);
                let sequence = state.sequence;
                Task::perform(
                    async move {
                        receive.await.unwrap_or_else(|_| Reply {
                            generation,
                            unlocked: false,
                            authority: Vec::new(),
                            result: Err("The credential operation was interrupted.".into()),
                        })
                    },
                    move |reply| message(CredentialCommand::Ready(sequence, purpose, reply)),
                )
            }
            Err(error) => {
                state.notice = error;
                Task::none()
            }
        }
    }
    fn credential_desktop_unlock(&mut self, manual: bool) -> Task<Message> {
        let state = &mut self.browser.credentials;
        if !state.settings.enabled
            || state.settings.setup_provider.is_some()
            || !state.desktop_available()
            || state.unlocked
            || state.busy
            || (self.browser.private && !state.settings.private_fill)
        {
            return Task::none();
        }
        state.desktop_attempted = true;
        self.credential_request(Purpose::DesktopUnlock, Operation::OpenDesktop { manual })
    }
    pub(crate) fn credential_command(&mut self, action: CredentialCommand) -> Task<Message> {
        use CredentialCommand::*;
        if matches!(
            action,
            PasskeyAdvance | PasskeyUse(_) | PasskeyCreate | PasskeyCancel
        ) {
            return self.passkey_command(&action);
        }
        if matches!(action, Open | OpenSite | OpenDestination)
            && !self.browser.private
            && self.browser.credentials.settings.enabled
            && let Some(kind) = self.browser.credentials.settings.setup_provider
        {
            self.browser.panel = Panel::None;
            self.open_library(InternalPage::Passwords);
            return if self.browser.credentials.connection.is_none() {
                self.credential_account_command(ConnectionNew(kind))
            } else {
                Task::none()
            };
        }
        if matches!(action, OpenDestination) {
            self.browser.credentials.unlock_source = self.browser.credentials.destination.clone();
            self.browser.credentials.unlocking = true;
            return self.credential_command(Open);
        }
        if matches!(action, Open) {
            self.browser.credentials.browse_logins = false;
            let candidate = self.browser.credentials.candidate.take();
            self.open_library(InternalPage::Passwords);
            self.browser.credentials.candidate = candidate;
        }
        match action {
            Initialize => {
                if !self.browser.credentials.initialization_pending(
                    self.welcome.is_none()
                        && (!self.browser.private
                            || self.browser.credentials.settings.private_fill),
                ) {
                    return Task::none();
                }
                self.browser.credentials.initialized = true;
                self.browser.credentials.desktop_attempted = true;
                return self.credential_request(
                    Purpose::DesktopUnlock,
                    Operation::OpenDesktop { manual: false },
                );
            }
            CheckOffer => {
                let state = &self.browser.credentials;
                if state.busy || !state.unlocked {
                    return Task::none();
                }
                let Some(candidate) = state
                    .candidate
                    .as_ref()
                    .filter(|candidate| !candidate.checked)
                else {
                    return Task::none();
                };
                return self.credential_request(
                    Purpose::CheckOffer,
                    Operation::CheckSubmission(candidate.login.clone(), candidate.filled.clone()),
                );
            }
            DismissInline => {
                self.browser.credentials.dismiss_inline();
                return Task::none();
            }
            FillInline(serial, id, code) => {
                if !self.browser.credentials.unlocked || !self.browser.credentials.settings.autofill
                {
                    return Task::none();
                }
                let Some(target) = self.take_inline_offer(serial) else {
                    return Task::none();
                };
                if !code {
                    self.browser.credentials.filled =
                        Some((target.document.document.clone(), id.clone()));
                }
                self.browser.credentials.fill_target = Some((target.clone(), code, None));
                return self
                    .credential_request(Purpose::Fill, Operation::Read(id, Some(target.site)));
            }
            ClipboardWritten(sequence, succeeded) => {
                let state = &mut self.browser.credentials;
                if state.sequence == sequence {
                    state.busy = false;
                    state.notice.clear();
                    if succeeded {
                        self.browser.toasts.done("Copied for 30 seconds.");
                    } else {
                        self.browser.toasts.error(
                            "credentials-copy",
                            "Secure clipboard access was not confirmed. Try again, or use Fill login.",
                        );
                    }
                }
                return Task::none();
            }
            OpenSite => {
                if self.browser.panel == Panel::Passwords {
                    self.browser.credentials.dismiss();
                    self.browser.panel = Panel::None;
                    return Task::none();
                }
                self.browser.panel = Panel::Passwords;
                self.submit_browser_action(BrowserAction::CredentialRefresh);
                self.suspend_webpage_keyboard();
                let focus_task =
                    iced::advanced::widget::operate(enzui_widgets::popover::focus_operation(
                        iced::advanced::widget::Id::new("browser-popover-Passwords"),
                    ));
                self.browser.credentials.query = Secret::default();
                if self.browser.credentials.unlocked {
                    if self.browser.credentials.catalog_loaded || self.browser.credentials.busy {
                        return focus_task;
                    }
                    let list_task = self.credential_request(Purpose::List, Operation::List(None));
                    return Task::batch([focus_task, list_task]);
                }
                let unlock = self.credential_desktop_unlock(true);
                return Task::batch([focus_task, unlock]);
            }
            FillReady(tab, sequence, result) => {
                if sequence != self.browser.credentials.sequence {
                    return Task::none();
                }
                self.browser.credentials.busy = false;
                if tab != self.browser.saved.tabs[self.browser.saved.active].key {
                    return Task::none();
                }
                let filled = matches!(result, Ok(crate::native_jobs::Payload::SecretJson(ref json)) if json.expose_secret() == "true");
                inline::credential_trace(format_args!("fill: filled={filled}"));
                if filled {
                    self.browser.panel = Panel::None;
                    self.browser.credentials.notice.clear();
                } else {
                    self.browser.credentials.notice =
                        "Select an empty login field on this website, then try again.".into();
                    self.browser.panel = Panel::Passwords;
                    self.suspend_webpage_keyboard();
                }
                return Task::none();
            }
            Configured(sequence, settings, result) => {
                let state = &mut self.browser.credentials;
                if sequence != state.sequence {
                    return Task::none();
                }
                state.busy = false;
                let forget_desktop = state.settings.desktop_unlock && !settings.desktop_unlock;
                let settings_saved = result.is_ok();
                match result {
                    Ok(publication) => {
                        state.config_identity = Some(publication.identity);
                        state.notice = publication.warning.unwrap_or_default();
                        state.destination = credentials::router::default_destination(&settings);
                        state.unlock_source = credentials::router::default_unlock_source(&settings);
                        state.settings = settings;
                        state.connection = None;
                        state.desktop_attempted = false;
                        state.initialized = false;
                        if !state.settings.enabled {
                            state.service = None;
                        }
                    }
                    Err(error) => {
                        state.lock();
                        match credentials::config::load_authorized(&state.config) {
                            Ok((settings, identity)) => {
                                state.settings = settings;
                                state.config_identity = identity;
                            }
                            Err(_) => state.settings.enabled = false,
                        }
                        state.notice = error;
                    }
                }
                self.refresh_credential_documents();
                if forget_desktop && settings_saved {
                    return self
                        .credential_request(Purpose::ForgetDesktop, Operation::ForgetDesktop);
                }
                return Task::none();
            }
            Lock => {
                self.browser.credentials.lock_with_policy(true);
                return Task::none();
            }
            Tick | StatusChanged => {
                let state = &mut self.browser.credentials;
                if state
                    .pending_export
                    .as_ref()
                    .is_some_and(|(_, lease)| !lease.valid())
                {
                    state.pending_export = None;
                    state.notice = "The export expired. Confirm your vault password again.".into();
                }
                if state.candidate.as_ref().is_some_and(|candidate| {
                    candidate.born.elapsed() >= std::time::Duration::from_secs(60)
                }) {
                    state.candidate = None;
                }
                if state.unlocked {
                    let current = state.service.as_ref().map(|service| {
                        (
                            service.unlocked(),
                            service.generation(),
                            service.source_unlocked(&state.destination),
                        )
                    });
                    match current {
                        Some((true, generation, true)) if generation != state.known_generation => {
                            state.dismiss_inline();
                            state.fill_target = None;
                            state.pending_export = None;
                            state.entries.clear();
                            state.known_generation = generation;
                        }
                        None | Some((false, _, _)) => {
                            state.clear_secrets();
                            state.desktop_attempted = false;
                        }
                        Some((true, generation, false)) if generation != state.known_generation => {
                            state.clear_secrets();
                            state.desktop_attempted = false;
                        }
                        _ => {}
                    }
                }
                return Task::none();
            }
            Session(event) => {
                if event.screensaver_active
                    || event.state != enzui_core::portal::session_monitor::SessionState::Running
                {
                    self.browser.credentials.lock();
                } else {
                    self.browser.credentials.desktop_attempted = false;
                }
                return Task::none();
            }
            Ready(sequence, purpose, reply) => {
                let state = &mut self.browser.credentials;
                if sequence != state.sequence {
                    return Task::none();
                }
                if state
                    .service
                    .as_ref()
                    .is_none_or(|service| service.generation() != reply.generation)
                {
                    state.clear_secrets();
                    state.desktop_attempted = false;
                    state.notice =
                        "Password access changed. Open the manager again to continue.".into();
                    return Task::none();
                }
                state.busy = false;
                state.pending_purpose = None;
                state.unlocked = reply.unlocked;
                state.known_generation = reply.generation;
                if matches!(purpose, Purpose::PasskeyRead | Purpose::PasskeySave) {
                    return self.passkey_ready(purpose, reply.result);
                }
                if purpose != Purpose::CheckOffer
                    && state.unlocked
                    && state
                        .candidate
                        .as_ref()
                        .is_some_and(|candidate| !candidate.checked)
                {
                    self.pending_tasks.push(Task::done(message(CheckOffer)));
                }
                let reply_lease = state.service.as_ref().map(|service| {
                    service.reply_lease(
                        &reply,
                        std::time::Duration::from_secs(
                            if matches!(purpose, Purpose::Backup | Purpose::Csv) {
                                300
                            } else {
                                10
                            },
                        ),
                    )
                });
                let output = match reply.result {
                    Ok(output) => output,
                    Err(error) => {
                        if !reply.unlocked {
                            let candidate = if matches!(
                                purpose,
                                Purpose::Unlock
                                    | Purpose::OfferSave
                                    | Purpose::UpdateOffer
                                    | Purpose::Save
                            ) {
                                state.candidate.take()
                            } else {
                                None
                            };
                            state.clear_secrets();
                            state.candidate = candidate;
                        }
                        if purpose == Purpose::CheckOffer {
                            state.candidate = None;
                        }
                        state.notice = error;
                        return Task::none();
                    }
                };
                if let Purpose::InlineList(serial) = purpose
                    && state
                        .inline
                        .as_ref()
                        .is_none_or(|offer| offer.serial != serial)
                {
                    let next = state
                        .inline
                        .as_ref()
                        .filter(|_| state.unlocked)
                        .map(|offer| (offer.serial, offer.target.site.clone()));
                    return next.map_or_else(Task::none, |(serial, site)| {
                        self.credential_request(
                            Purpose::InlineList(serial),
                            Operation::List(Some(site)),
                        )
                    });
                }
                let output = match output {
                    Output::Opened { desktop } => {
                        state.unlocking = false;
                        if state.unlock_source == "browser"
                            && state.settings.desktop_unlock
                            && !desktop
                        {
                            self.browser.toasts.info(ToastTone::Warning, "Vault unlocked. The desktop keyring could not remember it; password unlock is still available.");
                        }
                        Output::Done
                    }
                    other => other,
                };
                match output {
                    Output::SubmissionChecked { id, known } => {
                        inline::credential_trace(format_args!(
                            "submission check: the submitted password is {}saved",
                            if known { "" } else { "not " }
                        ));
                        if state
                            .candidate
                            .as_ref()
                            .is_none_or(|candidate| candidate.login.id != id)
                        {
                            if state
                                .candidate
                                .as_ref()
                                .is_some_and(|candidate| !candidate.checked)
                            {
                                self.pending_tasks.push(Task::done(message(CheckOffer)));
                            }
                            return Task::none();
                        }
                        if known {
                            let presented = state
                                .candidate
                                .as_ref()
                                .is_some_and(|candidate| candidate.presented);
                            state.candidate = None;
                            if presented && self.browser.panel == Panel::Passwords {
                                self.browser.panel = Panel::None;
                            }
                        } else {
                            if let Some(candidate) = state.candidate.as_mut() {
                                candidate.checked = true;
                            }
                            self.show_credential_save_offer();
                        }
                    }
                    Output::DesktopOpened(opened) => {
                        if !opened {
                            if state.candidate.is_some() {
                                self.show_credential_save_offer();
                                return Task::none();
                            }
                            if self.browser.panel == Panel::Passwords
                                || self.browser.library == Some(InternalPage::Passwords)
                            {
                                state.notice = "Use your vault password, or unlock your desktop keyring and try again.".into();
                            }
                            return Task::none();
                        }
                        state.master = Secret::default();
                        state.confirmation = Secret::default();
                        return self.credential_request(Purpose::List, Operation::List(None));
                    }
                    Output::Done if purpose == Purpose::Refresh => {
                        return self.credential_request(Purpose::List, Operation::List(None));
                    }
                    Output::Done if purpose == Purpose::ForgetDesktop => {
                        self.browser
                            .toasts
                            .done("Remembered desktop unlock removed.");
                    }
                    Output::Entries(mut entries) => {
                        if purpose == Purpose::List {
                            state.catalog.clone_from(&entries);
                            state.catalog_loaded = true;
                            if state.passkey.is_some() {
                                self.pending_tasks
                                    .push(Task::done(message(PasskeyAdvance)));
                            }
                            if let Some(offer) = &mut state.inline {
                                let listed = entries.len();
                                entries.retain(|entry| entry.matches_site(&offer.target.site));
                                inline::credential_trace(format_args!(
                                    "list: {listed} logins for the page, {} match {}",
                                    entries.len(),
                                    offer.target.site.origin
                                ));
                                for entry in &mut entries {
                                    entry.site = offer.target.site.clone();
                                }
                                offer.loaded = true;
                            }
                            self.browser.chrome_revision =
                                self.browser.chrome_revision.wrapping_add(1);
                        }
                        state.entries = entries;
                        if matches!(purpose, Purpose::InlineList(_)) {
                            inline::credential_trace(format_args!(
                                "list: {} logins match the field's page",
                                state.entries.len()
                            ));
                            self.browser.chrome_revision =
                                self.browser.chrome_revision.wrapping_add(1);
                            if state.entries.is_empty() {
                                state.dismiss_inline();
                            } else if let Some(offer) = &mut state.inline {
                                offer.loaded = true;
                            }
                        }
                        if purpose == Purpose::OfferSave
                            && let Some(candidate) = state.candidate.as_ref()
                        {
                            if let Some(existing) = state.entries.iter().find(|entry| {
                                entry.site == candidate.login.site
                                    && entry.username == candidate.login.username
                            }) {
                                let id = existing.id.clone();
                                let site = candidate.login.site.clone();
                                return self.credential_request(
                                    Purpose::UpdateOffer,
                                    Operation::Read(id, Some(site)),
                                );
                            }
                            if let Some(candidate) = state.candidate.as_ref() {
                                let destination = state.destination.clone();
                                let login = candidate.login.clone();
                                return self.credential_request(
                                    Purpose::Save,
                                    Operation::SaveTo(destination, login),
                                );
                            }
                        }
                    }
                    Output::Entry(mut existing) if purpose == Purpose::UpdateOffer => {
                        let Some(candidate) = state.candidate.as_ref() else {
                            return Task::none();
                        };
                        if existing.password == candidate.login.password {
                            state.candidate = None;
                            self.browser.panel = Panel::None;
                            return Task::none();
                        }
                        if let Err(error) = existing.update_password(&candidate.login) {
                            state.notice = error;
                            return Task::none();
                        }
                        return self.credential_request(Purpose::Save, Operation::Save(existing));
                    }
                    Output::Entry(login) if matches!(purpose, Purpose::Copy(_)) => {
                        let Some(lease) =
                            reply_lease.filter(credentials::service::Lease::authorize)
                        else {
                            state.notice = "Password access expired. Try again.".into();
                            return Task::none();
                        };
                        let secret = if purpose == Purpose::Copy(true) {
                            &login.password
                        } else {
                            &login.username
                        };
                        if secret.is_empty() {
                            state.notice = "This login does not have that field saved.".into();
                            return Task::none();
                        }
                        let text =
                            enzui_security::secret::SecretText::new(secret.expose().to_owned());
                        state.busy = true;
                        state.notice = "Copying…".into();
                        return Task::perform(
                            async move {
                                if !lease.authorize() {
                                    return false;
                                }
                                matches!(
                                    enzui_clipboard_runtime::write_sensitive_text_async(
                                        text,
                                        std::time::Duration::from_secs(30),
                                        std::time::Duration::from_secs(2)
                                    )
                                    .await,
                                    Some(Ok(()))
                                )
                            },
                            move |succeeded| message(ClipboardWritten(sequence, succeeded)),
                        );
                    }
                    Output::Entry(login) if purpose == Purpose::Fill => {
                        let Some((target, code, selected)) = state.fill_target.take() else {
                            return Task::none();
                        };
                        let tab = self.browser.saved.tabs[self.browser.saved.active].key;
                        if target.tab != tab {
                            return Task::none();
                        }
                        let build_script = if selected.is_some() {
                            crate::credential_bridge::fill_selected_script
                        } else {
                            crate::credential_bridge::fill_script
                        };
                        if selected
                            .as_ref()
                            .is_some_and(|site| !login.site.matches(site))
                        {
                            state.notice = "The selected login changed. Choose it again.".into();
                            return Task::none();
                        }
                        let source =
                            match build_script(&target.document, &target.site, &login, code) {
                                Ok(source) => source,
                                Err(error) => {
                                    state.notice = error;
                                    self.browser.panel = Panel::Passwords;
                                    self.suspend_webpage_keyboard();
                                    return Task::none();
                                }
                            };
                        let (job, receive) = crate::native_jobs::Job::new();
                        state.busy = true;
                        let lease =
                            reply_lease.expect("the credential reply is from a live provider");
                        self.submit_browser_action(BrowserAction::Credential(Box::new(
                            crate::webview_native::CredentialFill {
                                profile: target.profile,
                                lease,
                                site: target.site,
                                source,
                                job,
                            },
                        )));
                        return Task::perform(
                            crate::native_jobs::receive(receive),
                            move |result| message(FillReady(tab, sequence, result)),
                        );
                    }
                    Output::Entry(login) => {
                        if let Ok((source, _)) = credentials::router::split_reference(&login.id) {
                            state.destination = source;
                        }
                        state.editor = Some(Editor {
                            origin: login.website(),
                            totp: login
                                .totp
                                .as_ref()
                                .map(credentials::Totp::uri_secret)
                                .unwrap_or_default(),
                            login,
                            reveal: false,
                            confirm_delete: false,
                        });
                    }
                    Output::Imported(count) => {
                        state.review.clear();
                        state.transfer = TransferMode::None;
                        let task = self.credential_request(Purpose::List, Operation::List(None));
                        self.browser.toasts.done(format!(
                            "Imported {count} new logins. Identical logins were skipped."
                        ));
                        return task;
                    }
                    Output::Data(bytes) => {
                        state.pending_export = reply_lease.map(|lease| (bytes, lease));
                        return self.credential_choose_export(purpose == Purpose::Csv);
                    }
                    Output::Done if purpose == Purpose::Rekey => {
                        state.clear_secrets();
                        state.notice =
                            "Vault password changed. Unlock with your new password.".into();
                    }
                    Output::Done
                        if matches!(purpose, Purpose::Unlock | Purpose::Save | Purpose::Delete) =>
                    {
                        if state.unlock_source == "browser" {
                            state.vault_exists = true;
                        }
                        state.master = Secret::default();
                        state.confirmation = Secret::default();
                        state.current = Secret::default();
                        state.editor = None;
                        if purpose == Purpose::Save {
                            state.candidate = None;
                        }
                        if purpose == Purpose::Unlock
                            && self.browser.library == Some(InternalPage::Passwords)
                            && let Some(candidate) = state.candidate.take()
                        {
                            let login = candidate.login;
                            state.editor = Some(Editor {
                                origin: login.website(),
                                login,
                                totp: Secret::default(),
                                reveal: false,
                                confirm_delete: false,
                            });
                            state.notice =
                                "Review this submitted login, then choose Save login.".into();
                            return Task::none();
                        }
                        return self.credential_request(Purpose::List, Operation::List(None));
                    }
                    Output::Done | Output::Opened { .. } => {}
                }
                return Task::none();
            }
            _ => {}
        }
        if self.browser.private
            && !matches!(
                action,
                Open | Master(_)
                    | AuthenticatorCode(_)
                    | ApiKeyMode(_)
                    | ApiClientId(_)
                    | ApiClientSecret(_)
                    | Confirmation(_)
                    | Unlock
                    | UnlockDesktop
                    | Query(_)
                    | Back
                    | Edit(_)
                    | Reveal
                    | Fill(..)
                    | FillSelected(..)
                    | Copy(..)
                    | View(..)
                    | Show(_)
                    | CopySite(_)
            )
        {
            self.browser.credentials.notice =
                "Manage saved passwords from a regular window.".into();
            return Task::none();
        }
        if accounts::handles(&action) {
            return self.credential_account_command(action);
        }
        if matches!(action, New | Edit(_) | Back | Transfer(_) | OpenDestination) {
            self.reset_library_scroll();
        }
        if matches!(action, Field(..) | Container(_) | Reveal | Generate) {
            let state = &mut self.browser.credentials;
            if state.editor.is_some()
                && state
                    .last_editor_touch
                    .is_none_or(|last| last.elapsed() >= std::time::Duration::from_secs(30))
            {
                if let Some(service) = &state.service {
                    service.touch(state.destination.clone());
                }
                state.last_editor_touch = Some(std::time::Instant::now());
            }
        }
        if let Query(value) = action {
            self.browser.credentials.query = value;
            return Task::none();
        }
        if let Show(id) = action {
            self.browser.credentials.shown = id;
            return Task::none();
        }
        if let CopySite(site) = action {
            self.browser.toasts.done("Website address copied.");
            return iced::clipboard::write(site).discard();
        }
        let refresh = matches!(action, Enable(_) | Setting(_, _) | Timeout(_));
        let active_document = self.current_credential_document().cloned();
        let state = &mut self.browser.credentials;
        if state.busy && !matches!(action, FileRead(..) | FileWritten(..)) {
            return Task::none();
        }
        if matches!(action, Transfer(TransferMode::Settings)) {
            state.transfer = TransferMode::Settings;
            state.editor = None;
            return Task::none();
        }
        if matches!(
            action,
            New | Edit(_)
                | FillSelected(_)
                | Copy(..)
                | View(..)
                | Field(..)
                | Container(_)
                | Reveal
                | Generate
                | Save
                | DeleteRequest
                | DeleteConfirm
                | Transfer(_)
                | Export
                | ImportChoose
                | ImportConfirm
        ) && (!state.unlocked
            || state
                .service
                .as_ref()
                .is_none_or(|service| !service.unlocked()))
        {
            state.lock();
            return Task::none();
        }
        match action {
            PasskeyAdvance | PasskeyUse(_) | PasskeyCreate | PasskeyCancel => {}
            View(id) => {
                self.open_library(InternalPage::Passwords);
                return self.credential_request(Purpose::Edit, Operation::ReadSelected(id));
            }
            Copy(id, password) => {
                let selected = state
                    .catalog
                    .iter()
                    .chain(&state.entries)
                    .find(|entry| entry.id == id);
                let Some(_) = selected else {
                    return Task::none();
                };
                return self
                    .credential_request(Purpose::Copy(password), Operation::ReadSelected(id));
            }
            CopyCode => {
                let Some(code) = state
                    .editor
                    .as_ref()
                    .and_then(Editor::authenticator)
                    .and_then(|totp| totp.code(chrono::Utc::now().timestamp().max(0) as u64).ok())
                else {
                    return Task::none();
                };
                let text = enzui_security::secret::SecretText::new(code.0.expose().to_owned());
                state.busy = true;
                state.notice = "Copying…".into();
                state.sequence = state.sequence.wrapping_add(1);
                let sequence = state.sequence;
                return Task::perform(
                    async move {
                        matches!(
                            enzui_clipboard_runtime::write_sensitive_text_async(
                                text,
                                std::time::Duration::from_secs(30),
                                std::time::Duration::from_secs(2)
                            )
                            .await,
                            Some(Ok(()))
                        )
                    },
                    move |succeeded| message(ClipboardWritten(sequence, succeeded)),
                );
            }
            FillSelected(id) => {
                if !state.settings.autofill || !state.unlocked {
                    return Task::none();
                }
                let Some(target) = active_document else {
                    state.notice = "Focus a login field on a secure website first.".into();
                    return Task::none();
                };
                let Some(selected) = state.catalog.iter().find(|entry| entry.id == id) else {
                    return Task::none();
                };
                if selected.site.container != target.site.container {
                    state.notice = "This login belongs to a different container.".into();
                    return Task::none();
                }
                // A login saved for another website is filled only after the
                // user has been told both addresses and asked again.
                if !selected.matches_site(&target.site)
                    && state.fill_mismatch.as_deref() != Some(id.as_str())
                {
                    let host = |origin: &str| {
                        url::Url::parse(origin)
                            .ok()
                            .and_then(|url| url.host_str().map(str::to_owned))
                            .unwrap_or_else(|| "no website".into())
                    };
                    state.notice = format!(
                        "This login is saved for {}, and this page is {}. Fill it here only if you trust this page: choose Fill again to confirm.",
                        host(&selected.site.origin),
                        host(&target.site.origin)
                    );
                    state.fill_mismatch = Some(id);
                    return Task::none();
                }
                state.fill_mismatch = None;
                state.filled = Some((target.document.document.clone(), id.clone()));
                let source_site = selected.site.clone();
                state.fill_target = Some((target, false, Some(source_site)));
                return self.credential_request(Purpose::Fill, Operation::ReadSelected(id));
            }
            Fill(id, code) => {
                if !state.settings.autofill {
                    state.notice = "Autofill is turned off.".into();
                    return Task::none();
                }

                let Some(target) = active_document else {
                    state.notice = "Focus a login field on a secure website first.".into();
                    return Task::none();
                };
                state.fill_target = Some((target.clone(), code, None));
                return self
                    .credential_request(Purpose::Fill, Operation::Read(id, Some(target.site)));
            }
            SaveOffer => {
                let Some(candidate) = state.candidate.as_ref() else {
                    return Task::none();
                };
                if !state.unlocked {
                    state.notice = "Unlock your vault to save this login.".into();
                    return Task::none();
                }
                let site = candidate.login.site.clone();
                let destination = state.destination.clone();
                return self.credential_request(
                    Purpose::OfferSave,
                    Operation::ListSource(destination, Some(site)),
                );
            }
            DismissOffer => {
                if let Some(candidate) = state.candidate.take()
                    && !state.declined.contains(&candidate.login.site.origin)
                {
                    state.declined.push(candidate.login.site.origin);
                }
                self.browser.panel = Panel::None;
            }
            Refresh => {
                return self.credential_request(Purpose::Refresh, Operation::Synchronize);
            }
            Open => {
                if state.unlocking {
                    return Task::none();
                }
                if state.unlocked {
                    return self.credential_request(Purpose::List, Operation::List(None));
                }
                return self.credential_desktop_unlock(true);
            }
            Enable(enabled) => {
                if enabled && let Err(error) = credentials::protect_process() {
                    state.notice = error;
                    return Task::none();
                }
                state.lock();
                state.notice.clear();
                let mut settings = state.settings.clone();
                settings.enabled = enabled;
                if enabled {
                    settings.provider = "enzim.builtin".into();
                }
                return self.credential_save_settings(settings);
            }
            Setting(key, value) => {
                if key == "plaintext" {
                    state.plaintext_confirmed = value;
                    return Task::none();
                }
                let mut settings = state.settings.clone();
                match key {
                    "save" => settings.offer_save = value,
                    "fill" => settings.autofill = value,
                    "private" => settings.private_fill = value,
                    "desktop" => settings.desktop_unlock = value,
                    _ => return Task::none(),
                }
                state.lock();
                return self.credential_save_settings(settings);
            }
            Timeout(index) => {
                let mut settings = state.settings.clone();
                settings.lock_minutes = [1, 5, 15, 30, 60][index.min(4)];
                state.lock();
                return self.credential_save_settings(settings);
            }
            Master(value) => state.master = value,
            AuthenticatorCode(value) => state.authenticator_code = value,
            ApiKeyMode(value) => {
                state.api_key_mode = value;
                state.clear_connection_auth();
            }
            ApiClientId(value) => state.api_client_id = value,
            ApiClientSecret(value) => state.api_client_secret = value,
            Confirmation(value) => state.confirmation = value,
            Current(value) => state.current = value,
            Unlock => {
                if !state.settings.enabled
                    || state.settings.provider != "enzim.builtin"
                    || (self.browser.private && !state.settings.private_fill)
                {
                    return Task::none();
                }
                let source = state.unlock_source.clone();
                let create = source == "browser" && !state.vault_exists;
                if create && self.browser.private {
                    state.notice = "Create your vault in a regular window.".into();
                    return Task::none();
                }
                if create && state.master != state.confirmation {
                    state.notice = "The vault passwords do not match.".into();
                    return Task::none();
                }
                let password = std::mem::take(&mut state.master);
                let bitwarden = state.settings.accounts.iter().any(|account| {
                    account.id == source && account.kind == credentials::accounts::Kind::Bitwarden
                });
                let client_id = if bitwarden && state.api_key_mode {
                    state.api_client_id.expose().trim().to_owned()
                } else {
                    String::new()
                };
                let client_secret = if bitwarden && state.api_key_mode {
                    std::mem::take(&mut state.api_client_secret)
                } else {
                    Secret::default()
                };
                let second_factor = if bitwarden && !state.api_key_mode {
                    std::mem::take(&mut state.authenticator_code)
                } else {
                    Secret::default()
                };
                state.clear_connection_auth();
                state.confirmation = Secret::default();
                let settings_identity = state.config_identity;
                return self.credential_request(
                    Purpose::Unlock,
                    Operation::OpenSource {
                        source,
                        authentication: credentials::accounts::Authentication {
                            password,
                            settings_identity,
                            client_id,
                            client_secret,
                            second_factor,
                        },
                        create,
                    },
                );
            }
            UnlockDesktop => return self.credential_desktop_unlock(true),
            ForgetDesktop => {
                state.lock();
                return self.credential_request(Purpose::ForgetDesktop, Operation::ForgetDesktop);
            }
            Query(value) => state.query = value,
            Show(_) | CopySite(_) => {}
            New => {
                let tab = &self.browser.saved.tabs[self.browser.saved.active];
                let site = Site::parse(&tab.url, tab.container).unwrap_or(Site {
                    origin: String::new(),
                    container: tab.container,
                    url: None,
                });
                match Credential::new(site) {
                    Ok(mut login) => {
                        login.name = Secret::default();
                        state.editor = Some(Editor {
                            origin: login.website(),
                            login,
                            totp: Secret::default(),
                            reveal: false,
                            confirm_delete: false,
                        });
                    }
                    Err(error) => state.notice = error,
                }
            }
            Edit(id) => {
                state.browse_logins = true;
                // A login listed without a usable website is still one the
                // user chose: it opens, and a failed strict read would lock
                // its whole account.
                return self.credential_request(Purpose::Edit, Operation::ReadSelected(id));
            }
            Field(key, value) => {
                if let Some(editor) = &mut state.editor {
                    match key {
                        "name" => editor.login.name = value,
                        "origin" => editor.origin = value,
                        "username" => editor.login.username = value,
                        "password" => editor.login.password = value,
                        "totp" => editor.totp = value,
                        "notes" => editor.login.notes = value,
                        _ => {}
                    }
                }
            }
            Container(index) => {
                if let Some(editor) = &mut state.editor {
                    if index == 0 {
                        editor.login.site.container = None;
                    } else if let Some(container) = self.browser.saved.containers.get(index - 1) {
                        editor.login.site.container = Some(container.id);
                    }
                }
            }
            Reveal => {
                if let Some(editor) = &mut state.editor {
                    editor.reveal = !editor.reveal;
                }
            }
            Generate => {
                if let Some(editor) = &mut state.editor {
                    match credentials::generate(24) {
                        Ok(password) => editor.login.password = password,
                        Err(error) => state.notice = error,
                    }
                }
            }
            Save => {
                let Some(editor) = &mut state.editor else {
                    return Task::none();
                };
                let result = (|| {
                    editor.login.set_website(editor.origin.expose())?;
                    editor.login.totp = if editor.totp.is_empty() {
                        None
                    } else {
                        Some(credentials::Totp::parse(editor.totp.expose())?)
                    };
                    if editor.login.name.is_empty() {
                        editor.login.name = Secret::new(editor.login.site.origin.clone());
                    }
                    if editor.login.password.is_empty() && editor.login.totp.is_none() {
                        return Err("Add a password or an authenticator secret.".to_owned());
                    }
                    let mut validation = editor.login.clone();
                    if validation.id.starts_with("ref:") {
                        validation.id = credentials::router::split_reference(&validation.id)?.1;
                    }
                    validation.validate()?;
                    Ok(editor.login.clone())
                })();
                match result {
                    Ok(entry) => {
                        let destination = state.destination.clone();
                        return self.credential_request(
                            Purpose::Save,
                            Operation::SaveTo(destination, entry),
                        );
                    }
                    Err(error) => state.notice = error,
                }
            }
            Back => {
                state.editor = None;
                state.transfer = TransferMode::None;
                state.review.clear();
                state.master = Secret::default();
                state.current = Secret::default();
                state.confirmation = Secret::default();
                state.notice.clear();
            }
            DeleteRequest => {
                if let Some(editor) = &mut state.editor {
                    editor.confirm_delete = true;
                }
            }
            DeleteConfirm => {
                if let Some(editor) = &state.editor {
                    let id = editor.login.id.clone();
                    return self.credential_request(Purpose::Delete, Operation::Delete(id));
                }
            }
            Transfer(mode) => {
                state.editor = None;
                state.transfer = mode;
                state.master = Secret::default();
                state.current = Secret::default();
                state.confirmation = Secret::default();
                state.plaintext_confirmed = false;
                state.review.clear();
            }
            Export => {
                let operation = match state.transfer {
                    TransferMode::Csv if state.plaintext_confirmed => {
                        Operation::Csv(std::mem::take(&mut state.current))
                    }
                    TransferMode::Backup | TransferMode::Rekey => {
                        if state.master != state.confirmation {
                            state.notice = "The new passwords do not match.".into();
                            return Task::none();
                        }
                        let current = std::mem::take(&mut state.current);
                        let password = std::mem::take(&mut state.master);
                        state.confirmation = Secret::default();
                        if state.transfer == TransferMode::Rekey {
                            Operation::Rekey { current, password }
                        } else {
                            Operation::Backup { current, password }
                        }
                    }
                    _ => return Task::none(),
                };
                let purpose = match state.transfer {
                    TransferMode::Csv => Purpose::Csv,
                    TransferMode::Rekey => Purpose::Rekey,
                    _ => Purpose::Backup,
                };
                return self.credential_request(purpose, operation);
            }
            ImportChoose => return self.credential_choose_import(),
            ImportConfirm => {
                let entries = state.review.clone();
                let destination = state.destination.clone();
                return self
                    .credential_request(Purpose::Import, Operation::Import(destination, entries));
            }
            FileChosen(sequence, paths, save) => {
                if sequence != state.sequence {
                    return Task::none();
                }
                return self.credential_file_chosen(paths, save);
            }
            FileRead(sequence, result) => {
                if sequence == state.sequence {
                    state.busy = false;
                    match result {
                        Ok(entries) => {
                            state.review = entries;
                            state.notice =
                                "Review the accounts, then import. Existing logins are preserved."
                                    .into();
                        }
                        Err(error) => state.notice = error,
                    }
                }
            }
            FileWritten(sequence, result) => {
                if sequence == state.sequence {
                    state.busy = false;
                    match result {
                        Ok(()) => self.browser.toasts.done("Export saved."),
                        Err(error) => self.browser.toasts.error("credentials-export", error),
                    }
                }
            }
            OpenDestination
            | Providers
            | ProviderPreferences
            | ProviderDefault(_)
            | ProviderRemove(_)
            | AddLocal
            | ConnectionTimeout(_)
            | Storage(_)
            | Destination(_)
            | DefaultDestination(_)
            | SourceSelect(_)
            | ProviderLogins(_)
            | ConnectionNew(_)
            | ConnectionEdit(_)
            | ConnectionField(..)
            | ConnectionToggle(..)
            | ConnectionSave
            | ConnectionDelete
            | ConnectionCancel => {}
            Initialize | CheckOffer | FillInline(..) | DismissInline | Lock | Tick
            | StatusChanged | Session(_) | Ready(..) | Configured(..) | OpenSite
            | FillReady(..) | ClipboardWritten(..) => {}
        }
        if refresh {
            self.refresh_credential_documents();
        }
        Task::none()
    }
    pub(super) fn credentials_page(&self) -> Element<'_, Message> {
        let state = &self.browser.credentials;
        let mut body = column![].spacing(16).width(Length::Fill);
        if !state.notice.is_empty() {
            body = body.push(text(&state.notice).size(13));
        }
        if self.browser.private {
            return self.page_document("Passwords", "key-round", "Your logins, wherever you keep them.", body.push(pref("Private window", "venetian-mask", "Manage passwords in a regular window. When private autofill is allowed, use the key button in the address bar to fill a saved login." )).into());
        }
        if state.connection.is_some() {
            return self.page_document(
                "Provider settings",
                "key-round",
                "Manage this account and its lock policy.",
                self.credential_connection_editor(),
            );
        }
        if state.transfer == TransferMode::Settings
            || !state.browse_logins
                && state.editor.is_none()
                && state.transfer == TransferMode::None
        {
            return self.page_document(
                "Passwords",
                "key-round",
                "Manage your providers and choose where new logins are saved.",
                self.credential_settings(),
            );
        }
        if let Some(kind) = state.settings.setup_provider {
            return self.page_document(
                "Passwords",
                "key-round",
                "Finish connecting your password manager.",
                column![
                    text(format!(
                        "Connect your {} account to start saving and filling passwords.",
                        kind.label()
                    ))
                    .size(14),
                    primary(
                        &format!("Connect {}", kind.label()),
                        "key-round",
                        command(CredentialCommand::ConnectionNew(kind))
                    ),
                    action(
                        "Password settings",
                        "settings",
                        command(CredentialCommand::Transfer(TransferMode::Settings))
                    ),
                ]
                .spacing(16)
                .into(),
            );
        }
        if state.settings.provider != "enzim.builtin" {
            body = body.push(pref(
                "Another password provider is selected",
                "key-round",
                "The built-in vault is disabled while another provider manages credentials.",
            ));
        } else if !state.settings.enabled {
            body = body.push(password_section("Your passwords, under your control", column![
                pref("Choose your storage", "lock-keyhole", "Use an encrypted browser vault, the system keyring, or a connected password manager."),
                pref("You choose when to fill", "mouse-pointer-2", "Logins are matched to the exact website and container. Forms are never submitted automatically."),
                pref("Optional", "toggle-left", "Turn saving and autofill off at any time. Stored logins are preserved."),
                primary(if state.vault_exists { "Enable password manager" } else { "Set up password manager" }, "key-round", command(CredentialCommand::Enable(true))),
                action("Choose storage or connect an account", "settings", command(CredentialCommand::Transfer(TransferMode::Settings)))
            ].spacing(12)));
        } else if !state.unlocked || state.unlocking {
            body = body.push(self.credential_unlock_view());
            body = body.push(action(
                "Password settings and connections",
                "settings",
                command(CredentialCommand::Transfer(TransferMode::Settings)),
            ));
        } else if let Some(editor) = &state.editor {
            let mut containers: Vec<String> = std::iter::once("Default session".into())
                .chain(
                    self.browser
                        .saved
                        .containers
                        .iter()
                        .map(|container| container.name.clone()),
                )
                .collect();
            let selected = match editor.login.site.container {
                None => 0,
                Some(id) => self
                    .browser
                    .saved
                    .containers
                    .iter()
                    .position(|container| container.id == id)
                    .map_or_else(
                        || {
                            containers.push("Unavailable container".into());
                            containers.len() - 1
                        },
                        |index| index + 1,
                    ),
            };
            let container_choice = enzui_widgets::combo_box::enz_combo_box(
                "Container",
                containers.clone(),
                Some(containers[selected].clone()),
                |_| message(CredentialCommand::Container(0)),
            )
            .id_prefix("credential-container")
            .selected_index(Some(selected))
            .on_select_index(|index| message(CredentialCommand::Container(index)))
            .into_element();
            let mut form = column![
                if editor.login.id.starts_with("ref:") {
                    Space::new().into()
                } else {
                    self.credential_destination_picker(false)
                },
                action("All logins", "arrow-left", command(CredentialCommand::Back)),
                secret_field(
                    "Name",
                    "Login name",
                    &editor.login.name,
                    false,
                    "credential-name",
                    |v| CredentialCommand::Field("name", v)
                ),
                secret_field(
                    "Website",
                    "https://example.com",
                    &editor.origin,
                    false,
                    "credential-origin",
                    |v| CredentialCommand::Field("origin", v),
                ),
                secret_field(
                    "Username",
                    "Username or email address",
                    &editor.login.username,
                    false,
                    "credential-username",
                    |v| CredentialCommand::Field("username", v)
                ),
                secret_field(
                    "Password",
                    "Password",
                    &editor.login.password,
                    !editor.reveal,
                    "credential-password",
                    |v| CredentialCommand::Field("password", v)
                ),
                row![
                    action(
                        if editor.reveal { "Hide" } else { "Show" },
                        if editor.reveal { "eye-off" } else { "eye" },
                        command(CredentialCommand::Reveal)
                    ),
                    action(
                        "Generate password",
                        "sparkles",
                        command(CredentialCommand::Generate)
                    )
                ]
                .spacing(8),
                secret_field(
                    "Authenticator",
                    "Base32 secret or otpauth:// URI",
                    &editor.totp,
                    true,
                    "credential-totp",
                    |v| CredentialCommand::Field("totp", v)
                ),
                container_choice,
                secret_field(
                    "Notes",
                    "Optional note",
                    &editor.login.notes,
                    false,
                    "credential-notes",
                    |v| CredentialCommand::Field("notes", v)
                ),
            ]
            .spacing(14);
            if let Some(passkey) = &editor.login.passkey {
                form = form.push(pref(
                    format!("Passkey for {}", passkey.rp_id),
                    "key-round",
                    "This login signs in to the website without a password. Deleting the login deletes its passkey.",
                ));
            }
            if let Some(totp) = editor.authenticator()
                && let Ok((code, remaining)) =
                    totp.code(chrono::Utc::now().timestamp().max(0) as u64)
            {
                form = form.push(
                    row![
                        text(code.expose().to_owned())
                            .size(26)
                            .font(iced::Font::MONOSPACE),
                        text(format!("Changes in {remaining}s")).size(12),
                        Space::new().width(Length::Fill),
                        action("Copy code", "copy", command(CredentialCommand::CopyCode)),
                    ]
                    .spacing(14)
                    .align_y(iced::Alignment::Center),
                );
            }
            form = form.push(
                row![
                    primary(
                        if state.busy {
                            "Saving…"
                        } else {
                            "Save login"
                        },
                        "check",
                        command(CredentialCommand::Save),
                    ),
                    Space::new().width(Length::Fill),
                    action(
                        "Delete",
                        "trash-2",
                        command(CredentialCommand::DeleteRequest)
                    )
                ]
                .spacing(8),
            );
            if editor.confirm_delete {
                form = form.push(password_section(
                    "Delete this saved login?",
                    column![
                        text("This removes it from the encrypted vault.").size(13),
                        action(
                            "Delete login",
                            "trash-2",
                            command(CredentialCommand::DeleteConfirm)
                        )
                    ]
                    .spacing(10),
                ));
            }
            body = body.push(password_section("Login details", form));
        } else if state.transfer == TransferMode::Settings {
            body = body.push(action(
                "All logins",
                "arrow-left",
                command(CredentialCommand::Back),
            ));
            body = body.push(self.credential_settings());
        } else if state.transfer != TransferMode::None {
            body = body.push(self.credential_transfer_view());
        } else {
            body = body.push(
                row![
                    primary("Add login", "plus", command(CredentialCommand::New)),
                    Space::new().width(Length::Fill),
                    action(
                        "Providers",
                        "arrow-left",
                        command(CredentialCommand::Providers)
                    ),
                    action(
                        "Lock passwords",
                        "lock-keyhole",
                        command(CredentialCommand::Lock)
                    )
                ]
                .spacing(10),
            );
            body = body.push(
                row![
                    secret_field(
                        "",
                        "Search logins",
                        &state.query,
                        false,
                        "credentials-search",
                        CredentialCommand::Query,
                    ),
                    enzui_widgets::toolbar_icon::enz_toolbar_icon(
                        "refresh-cw",
                        message(CredentialCommand::Refresh),
                    )
                    .aria_label("Refresh connected accounts")
                    .tooltip("Refresh connected accounts")
                    .busy(state.busy)
                    .enabled(!state.busy)
                    .into_element(),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
            );
            let query = state.query.expose().to_lowercase();
            let mut entries = column![].spacing(2);
            let mut count = 0;
            for entry in &state.entries {
                if credentials::router::split_reference(&entry.id)
                    .is_ok_and(|(source, _)| source != state.unlock_source)
                {
                    continue;
                }
                if !query.is_empty()
                    && !entry.name.expose().to_lowercase().contains(&query)
                    && !entry.username.expose().to_lowercase().contains(&query)
                    && !entry.site.origin.to_lowercase().contains(&query)
                {
                    continue;
                }
                count += 1;
                let line = row![
                    glyph("key-round", 18.),
                    column![
                        text(entry.name.expose()).size(15),
                        text(format!(
                            "{} · {} · {}",
                            entry.source,
                            entry.username.expose(),
                            match entry.site.container.and_then(|id| self
                                .browser
                                .saved
                                .containers
                                .iter()
                                .find(|container| container.id == id))
                            {
                                Some(container) =>
                                    format!("{} · {}", entry.site.origin, container.name),
                                None => entry.site.origin.clone(),
                            }
                        ))
                        .size(12)
                    ]
                    .spacing(4)
                    .width(Length::Fill),
                    glyph(
                        if entry.passkey.is_some() {
                            "key-round"
                        } else if entry.has_totp {
                            "timer"
                        } else {
                            "chevron-right"
                        },
                        16.
                    )
                ]
                .spacing(14)
                .align_y(iced::Alignment::Center);
                entries = entries.push(
                    enzui_widgets::list_row_surface::enz_list_row_surface(
                        line,
                        message(CredentialCommand::Edit(entry.id.clone())),
                    )
                    .label(format!(
                        "{}, {}, {}",
                        entry.name.expose(),
                        entry.username.expose(),
                        entry.source
                    ))
                    .id(format!("credential-row-{}", entry.id).into())
                    .row_height(64.)
                    .content_padding(12.)
                    .outer_gutter(0.)
                    .into_element(),
                );
                if count >= 200 {
                    entries = entries.push(text("Refine your search to see more logins.").size(12));
                    break;
                }
            }
            if count == 0 {
                entries = entries.push(pref(
                    "No saved logins",
                    "key-round",
                    "Add one manually or import an existing password export.",
                ));
            }
            body = body.push(password_section("Saved logins", entries));

            let mut buttons = column![].spacing(6);
            if matches!(state.destination.as_str(), "browser" | "system") {
                buttons = buttons.push(action(
                    "Import passwords…",
                    "folder-input",
                    command(CredentialCommand::Transfer(TransferMode::Import)),
                ));
            }
            if state.vault_exists {
                buttons = buttons
                    .push(action(
                        "Encrypted browser-vault backup…",
                        "file-lock",
                        command(CredentialCommand::Transfer(TransferMode::Backup)),
                    ))
                    .push(action(
                        "Export browser vault as CSV…",
                        "file-output",
                        command(CredentialCommand::Transfer(TransferMode::Csv)),
                    ))
                    .push(action(
                        "Change browser-vault password…",
                        "key-round",
                        command(CredentialCommand::Transfer(TransferMode::Rekey)),
                    ));
            }
            if state.vault_exists || matches!(state.destination.as_str(), "browser" | "system") {
                body = body.push(password_section("Password tools", buttons));
            }
        }
        self.page_document(
            "Passwords",
            "key-round",
            "Your logins, wherever you keep them.",
            body.into(),
        )
    }
    pub(super) fn credential_settings(&self) -> Element<'_, Message> {
        let state = &self.browser.credentials;
        if self.browser.library != Some(InternalPage::Passwords) {
            return password_section(
                "Passwords",
                enzui_widgets::preferences_row::enz_preferences_row("Passwords and authenticators")
                    .icon("key-round")
                    .subtitle("Manage saved logins, connected accounts and autofill preferences.")
                    .action("Manage", message(CredentialCommand::Open))
                    .into_element(),
            );
        }
        if !state.provider_preferences {
            return column![
                enzui_widgets::preferences_row::enz_preferences_row("Password manager")
                    .switch(
                        state.settings.enabled && state.settings.provider == "enzim.builtin",
                        |value| message(CredentialCommand::Enable(value))
                    )
                    .into_element(),
                self.credential_sources_view(),
                action(
                    "Autofill and local vault settings",
                    "sliders-horizontal",
                    command(CredentialCommand::ProviderPreferences)
                ),
            ]
            .spacing(16)
            .into();
        }
        let mut settings = column![
            action(
                "Providers",
                "arrow-left",
                command(CredentialCommand::Providers)
            ),
            text("Autofill and local vault settings").size(16),
            iced::widget::rule::horizontal(1),
            enzui_widgets::preferences_row::enz_preferences_row("Password manager")
                .icon("key-round")
                .subtitle("Choose local storage or connect your password manager")
                .switch(
                    state.settings.enabled && state.settings.provider == "enzim.builtin",
                    |value| message(CredentialCommand::Enable(value))
                )
                .into_element()
        ]
        .spacing(4);

        if state.settings.enabled {
            for (title, detail, key, enabled) in [
                (
                    "Remember browser-vault unlock",
                    "Use GNOME Keyring or oo7 for this browser vault. Keep its password for recovery.",
                    "desktop",
                    state.settings.desktop_unlock,
                ),
                (
                    "Offer to save logins",
                    "Ask before saving a submitted login",
                    "save",
                    state.settings.offer_save,
                ),
                (
                    "Offer autofill",
                    "Choose a matching login before filling; never submit it automatically",
                    "fill",
                    state.settings.autofill,
                ),
                (
                    "Allow in private windows",
                    "Allow access to saved logins without saving new private-session logins",
                    "private",
                    state.settings.private_fill,
                ),
            ] {
                if key == "desktop"
                    && state.settings.storage != credentials::accounts::Storage::Browser
                    && !state.vault_exists
                {
                    continue;
                }
                settings = settings.push(
                    enzui_widgets::preferences_row::enz_preferences_row(title)
                        .subtitle(detail)
                        .switch(enabled, move |value| {
                            message(CredentialCommand::Setting(key, value))
                        })
                        .into_element(),
                );
            }
            let options = vec![
                "After 1 minute".into(),
                "After 5 minutes".into(),
                "After 15 minutes".into(),
                "After 30 minutes".into(),
                "After 1 hour".into(),
            ];
            let index = [1, 5, 15, 30, 60]
                .iter()
                .position(|m| *m == state.settings.lock_minutes)
                .unwrap_or(1);
            let timeout = enzui_widgets::combo_box::enz_combo_box(
                "",
                options.clone(),
                Some(options[index].clone()),
                |_| message(CredentialCommand::Timeout(1)),
            )
            .id_prefix("credential-timeout")
            .selected_index(Some(index))
            .on_select_index(|i| message(CredentialCommand::Timeout(i)))
            .into_element();
            settings = settings.push(
                column![
                    text("Lock local storage after inactivity").size(14),
                    timeout,
                ]
                .spacing(8),
            );
            if state.vault_exists {
                settings = settings.push(
                    enzui_widgets::preferences_row::enz_preferences_row(
                        "Remembered desktop unlock",
                    )
                    .icon("key-round")
                    .subtitle("Remove the saved unlock key from this desktop keyring")
                    .action("Forget", message(CredentialCommand::ForgetDesktop))
                    .into_element(),
                );
            }
        }
        if !state.notice.is_empty() {
            settings = settings.push(
                enzui_widgets::preferences_row::enz_preferences_row("Vault status")
                    .icon("info")
                    .subtitle(&state.notice)
                    .into_element(),
            );
        }
        settings.into()
    }
    fn credential_transfer_view(&self) -> Element<'_, Message> {
        let state = &self.browser.credentials;
        let mut body = column![action(
            "All logins",
            "arrow-left",
            command(CredentialCommand::Back)
        )]
        .spacing(14);
        if state.transfer == TransferMode::Import {
            body = body.push(self.credential_destination_picker(false));
            body = body.push(text("Import a Firefox/Chrome-compatible CSV, Bitwarden JSON or enzim encrypted backup.").size(13));
            body = body.push(secret_field(
                "Backup password",
                "Only for an enzim encrypted backup",
                &state.master,
                true,
                "credential-import-password",
                CredentialCommand::Master,
            ));
            body = body.push(action(
                "Choose password export…",
                "folder-input",
                command(CredentialCommand::ImportChoose),
            ));
            if !state.review.is_empty() {
                body = body.push(text("Review the websites and accounts before importing. Passwords remain hidden.").size(13));
                for entry in state.review.iter().take(20) {
                    body = body.push(
                        row![
                            glyph("key-round", 16.),
                            column![
                                text(entry.name.expose()).size(14),
                                text(format!(
                                    "{} · {}",
                                    entry.username.expose(),
                                    entry.site.origin
                                ))
                                .size(12)
                            ]
                            .spacing(3)
                        ]
                        .spacing(10)
                        .align_y(iced::Alignment::Center),
                    );
                }
                if state.review.len() > 20 {
                    body = body.push(
                        text(format!("And {} more logins.", state.review.len() - 20)).size(12),
                    );
                }
                body = body.push(primary(
                    &format!("Import {} logins", state.review.len()),
                    "download",
                    command(CredentialCommand::ImportConfirm),
                ));
            }
            return password_section("Import logins", body);
        }
        body = body.push(secret_field(
            "Current vault password",
            "Confirm your vault password",
            &state.current,
            true,
            "credential-current-master",
            CredentialCommand::Current,
        ));
        if state.transfer == TransferMode::Csv {
            body = body.push(enzui_widgets::preferences_row::enz_preferences_row("Export unencrypted secrets").icon("triangle-alert").subtitle("CSV contains readable passwords and authenticator secrets. Store it carefully and remove it after migration.").switch(state.plaintext_confirmed, |value| message(CredentialCommand::Setting("plaintext", value))).into_element());
        } else {
            body = body.push(secret_field(
                if state.transfer == TransferMode::Rekey {
                    "New vault password"
                } else {
                    "Backup password"
                },
                "At least 12 characters",
                &state.master,
                true,
                "credential-new-master",
                CredentialCommand::Master,
            ));
            body = body.push(secret_field(
                if state.transfer == TransferMode::Rekey {
                    "Confirm new vault password"
                } else {
                    "Confirm backup password"
                },
                "Enter it again",
                &state.confirmation,
                true,
                "credential-new-confirmation",
                CredentialCommand::Confirmation,
            ));
        }
        body = body.push(primary(
            if state.transfer == TransferMode::Rekey {
                "Change password"
            } else {
                "Choose where to save…"
            },
            "check",
            command(CredentialCommand::Export),
        ));
        password_section(
            match state.transfer {
                TransferMode::Csv => "Export CSV",
                TransferMode::Rekey => "Change vault password",
                _ => "Encrypted backup",
            },
            body,
        )
    }
}

impl App {
    fn credential_choose_import(&mut self) -> Task<Message> {
        self.browser.credentials.sequence = self.browser.credentials.sequence.wrapping_add(1);
        let result = self.browser.chooser.start(
            (
                u64::MAX - 4,
                enzui_webkit::page::PageRequestId::new(self.browser.credentials.sequence),
            ),
            enzui_core::portal::backend::PortalOperation::OpenFile,
            enzui_core::portal::chooser::ChooserOptions::new().title("Import password export"),
        );
        if result.is_err() {
            self.browser.toasts.error(
                "credentials-import",
                "Could not open the password import chooser.",
            );
        }
        Task::none()
    }
    fn credential_choose_export(&mut self, csv: bool) -> Task<Message> {
        let result = self.browser.chooser.start(
            (
                u64::MAX - 5,
                enzui_webkit::page::PageRequestId::new(self.browser.credentials.sequence),
            ),
            enzui_core::portal::backend::PortalOperation::SaveFile,
            enzui_core::portal::chooser::ChooserOptions::new()
                .title(if csv {
                    "Export unencrypted password CSV"
                } else {
                    "Save encrypted vault backup"
                })
                .current_name(if csv {
                    "enzim-passwords.csv"
                } else {
                    "enzim-vault-backup.enzim"
                }),
        );
        if result.is_err() {
            self.browser.credentials.pending_export = None;
            self.browser.toasts.error(
                "credentials-export",
                "Could not open the password export chooser.",
            );
        }
        Task::none()
    }
    fn credential_file_chosen(
        &mut self,
        paths: Vec<std::path::PathBuf>,
        save: bool,
    ) -> Task<Message> {
        let state = &mut self.browser.credentials;
        let Some(path) = paths.into_iter().next() else {
            state.pending_export = None;
            state.notice = "Cancelled.".into();
            return Task::none();
        };
        if !state.unlocked
            || state
                .service
                .as_ref()
                .is_none_or(|service| !service.unlocked())
        {
            state.lock();
            return Task::none();
        }
        state.busy = true;
        state.sequence = state.sequence.wrapping_add(1);
        let sequence = state.sequence;
        if save {
            let Some((bytes, lease)) = state.pending_export.take() else {
                state.busy = false;
                return Task::none();
            };
            return super::import_page::background(
                "credential-export",
                move || {
                    use enzui_core::state_authority::StateAuthority;
                    if !lease.authorize() {
                        return Err(
                            "The export expired or the vault was locked. Start the export again."
                                .into(),
                        );
                    }
                    let mut destination = StateAuthority::open_or_create(&path).map_err(|_| {
                        "Choose a private file you own, or a new filename.".to_owned()
                    })?;
                    let source = StateAuthority::open_existing(&credentials::Vault::default_path())
                        .map_err(|_| {
                            "The source vault is unavailable. Unlock it again before exporting."
                                .to_owned()
                        })?;
                    if destination.identity().is_some()
                        && destination.identity() == source.identity()
                    {
                        return Err("A password export cannot overwrite the active vault. Choose another filename.".into());
                    }
                    if !lease.authorize() {
                        return Err("The export expired before it could be saved.".into());
                    }
                    destination.publish(bytes.as_bytes()).map_err(|_| "The export could not be confirmed. Check the chosen file before retrying.".to_owned())?;
                    Ok(())
                },
                move |result| message(CredentialCommand::FileWritten(sequence, result)),
            );
        }
        let password = std::mem::take(&mut state.master);
        let container = self.browser.saved.tabs[self.browser.saved.active].container;
        super::import_page::background(
            "credential-import",
            move || {
                use std::io::Read;
                let file =
                    enzui_core::atomic_state::open_state_file_nofollow(&path).map_err(|_| {
                        "The password export is not a readable regular file.".to_owned()
                    })?;
                let mut bytes = zeroize::Zeroizing::new(Vec::new());
                file.take((credentials::MAX_BYTES + 1) as u64)
                    .read_to_end(&mut bytes)
                    .map_err(|_| "The password export could not be read.".to_owned())?;
                if bytes.len() > credentials::MAX_BYTES {
                    return Err("Choose a password export smaller than 16 MiB.".into());
                }
                if bytes.starts_with(b"ENZVAULT") {
                    credentials::Vault::read_backup(password.expose(), &bytes)
                } else {
                    credentials::exchange::import(&bytes, container)
                }
            },
            move |result| message(CredentialCommand::FileRead(sequence, result)),
        )
    }
}

impl App {
    pub(super) fn credential_bridge_message(&mut self, json: enzui_security::secret::SecretText) {
        let current = &self.browser.saved.tabs[self.browser.saved.active];
        let state = &mut self.browser.credentials;
        if !state.settings.enabled
            || state.settings.provider != "enzim.builtin"
            || self.browser.library.is_some()
        {
            return;
        }
        if self.browser.private && !state.settings.private_fill {
            return;
        }
        let Ok(event) = crate::credential_bridge::Event::parse(&json) else {
            return;
        };
        match event {
            crate::credential_bridge::Event::Ready {
                document,
                origin,
                action,
                target,
                password,
                otp,
                username_only,
                focus,
            } => {
                let host = self.webview.borrow();
                let valid_tls = crate::webview_native::certificate_verified(host.tls.as_deref());
                let Some(uri) = host.location.as_deref() else {
                    return;
                };
                let Ok(site) = Site::parse(uri, current.container) else {
                    return;
                };
                let mut document = crate::credential_bridge::Document {
                    document,
                    origin,
                    target,
                    username_only,
                };
                inline::credential_trace(format_args!(
                    "ready: page={} valid_tls={} same_document_origin={} action_same_origin={} password={password} otp={otp} username_only={} focus={}",
                    site.origin,
                    valid_tls,
                    document.valid_for(&site),
                    action == document.origin,
                    document.username_only,
                    focus.is_some()
                ));
                if valid_tls && document.valid_for(&site) {
                    let action_allowed = action == document.origin;
                    document.username_only &= action_allowed;
                    if state.fill_target.as_ref().is_some_and(|(pending, _, _)| {
                        pending.document.document != document.document
                            || pending.document.target != document.target
                    }) {
                        state.fill_target = None;
                    }
                    state.document = Some(FormDocument {
                        profile: host.profile.clone(),
                        tab: current.key,
                        site,
                        document,
                        password: password && action_allowed,
                        otp: otp && action_allowed,
                    });
                }
                drop(host);
                self.credential_field_focus(focus);
            }
            crate::credential_bridge::Event::Dismiss { document, origin } => {
                if state.document.as_ref().is_some_and(|known| {
                    known.document.document == document && known.site.origin == origin
                }) {
                    if state.inline.is_some() {
                        self.browser.chrome_revision = self.browser.chrome_revision.wrapping_add(1);
                    }
                    state.dismiss_inline();
                }
            }
            crate::credential_bridge::Event::Submitted {
                document,
                origin,
                action,
                username,
                password,
            } => {
                if self.browser.private
                    || !state.settings.offer_save
                    || state.candidate.is_some()
                    || state.declined.contains(&origin)
                    || username.expose().len() > 4096
                    || password.expose().len() > 4096
                    || password.is_empty()
                {
                    return;
                }
                inline::credential_trace(format_args!("submitted: page={origin}"));
                let Some(known) = &state.document else {
                    return;
                };
                if known.tab != current.key
                    || known.document.document != document
                    || known.site.origin != origin
                    || action != origin
                {
                    return;
                }
                if let Ok(mut login) = Credential::new(known.site.clone()) {
                    login.username = username;
                    login.password = password;
                    let filled = state
                        .filled
                        .as_ref()
                        .filter(|(page, _)| *page == document)
                        .map(|(_, id)| id.clone());
                    state.candidate = Some(Candidate {
                        login,
                        filled,
                        born: std::time::Instant::now(),
                        checked: false,
                        presented: false,
                    });
                    if state.unlocked {
                        self.pending_tasks
                            .push(Task::done(message(CredentialCommand::CheckOffer)));
                    } else if !state.busy {
                        self.show_credential_save_offer();
                    }
                }
            }
        }
    }
    fn show_credential_save_offer(&mut self) {
        if let Some(candidate) = &mut self.browser.credentials.candidate {
            candidate.presented = true;
        }
        self.browser.panel = Panel::Passwords;
        self.suspend_webpage_keyboard();
        self.pending_tasks.push(iced::advanced::widget::operate(
            iced::advanced::widget::operation::focusable::unfocus(),
        ));
    }
    pub(super) fn credential_popup(&self) -> Element<'_, Message> {
        let state = &self.browser.credentials;
        let mut body = column![].spacing(8);
        if self.browser.private && !state.settings.private_fill {
            return pref(
                "Private autofill is off",
                "venetian-mask",
                "Allow access to saved logins from password settings in a regular window.",
            );
        }
        if !state.settings.enabled || state.settings.provider != "enzim.builtin" {
            return column![
                pref(
                    "Built-in password manager is off",
                    "key-round",
                    "Set it up or choose a password provider to use saved logins."
                ),
                action(
                    "Password settings",
                    "settings",
                    command(CredentialCommand::Open)
                )
            ]
            .spacing(12)
            .into();
        }
        if let Some(kind) = state.settings.setup_provider {
            return column![
                text(format!("Connect {}", kind.label())).size(16),
                text("Finish connecting this account to save and fill passwords.").size(13),
                primary(
                    "Continue setup",
                    "arrow-right",
                    command(CredentialCommand::Open)
                ),
            ]
            .spacing(12)
            .into();
        }
        if state.candidate.is_none()
            && state.unlocked
            && let Some(entry) = state.shown.as_ref().and_then(|id| {
                state
                    .catalog
                    .iter()
                    .chain(&state.entries)
                    .find(|entry| &entry.id == id)
            })
        {
            return self.credential_popup_login(entry);
        }
        if !state.notice.is_empty() {
            body = body.push(text(&state.notice).size(12));
        }
        if let Some(candidate) = &state.candidate {
            let updating = state.entries.iter().any(|entry| {
                entry.site == candidate.login.site
                    && entry.username == candidate.login.username
                    && credentials::router::split_reference(&entry.id)
                        .is_ok_and(|(source, _)| source == state.destination)
            });
            body = body.push(heading(
                "key-round",
                if updating {
                    "Update this password?"
                } else {
                    "Save this login?"
                },
                candidate.login.site.origin.clone(),
            ));
            body = body.push(text(candidate.login.username.expose()).size(14));
            body = body.push(self.credential_destination_picker(false));
            if state.unlocked {
                body = body.push(
                    row![
                        primary(
                            if state.busy {
                                "Saving…"
                            } else if updating {
                                "Update password"
                            } else {
                                "Save login"
                            },
                            "check",
                            command(CredentialCommand::SaveOffer)
                        ),
                        action("Not now", "x", command(CredentialCommand::DismissOffer))
                    ]
                    .spacing(8),
                );
                body = body.push(action(
                    "Unlock destination…",
                    "lock-keyhole-open",
                    command(CredentialCommand::OpenDestination),
                ));
                return body
                    .push(action(
                        "Manage passwords",
                        "settings",
                        command(CredentialCommand::Open),
                    ))
                    .into();
            }
        } else {
            body = body.push(
                row![
                    enzui_widgets::icon::enz_icon("key-round").size(17.),
                    text("Passwords").size(16),
                    iced::widget::Space::new().width(Length::Fill),
                    enzui_widgets::toolbar_icon::enz_toolbar_icon(
                        "settings",
                        message(CredentialCommand::Open),
                    )
                    .aria_label("Manage passwords")
                    .tooltip("Manage passwords")
                    .size(28., 16.)
                    .into_element()
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
            );
        }
        if !state.unlocked {
            body = body.push(self.credential_unlock_view());
        } else {
            body = body.push(secret_field(
                "",
                "Search",
                &state.query,
                false,
                "credentials-search",
                CredentialCommand::Query,
            ));
            let document = self.current_credential_document();
            let current = &self.browser.saved.tabs[self.browser.saved.active];
            let site = document
                .map(|doc| doc.site.clone())
                .or_else(|| Site::parse(&current.url, current.container).ok());
            let query = state.query.expose().trim().to_lowercase();
            let palette = &self.look().palette;
            let entries = if state.catalog_loaded {
                &state.catalog
            } else {
                &state.entries
            };
            let matches: Vec<_> = entries
                .iter()
                .filter(|entry| popup_matches(entry, site.as_ref(), &query))
                .collect();
            if matches.is_empty() {
                body = body.push(
                    text(if !state.catalog_loaded && state.busy {
                        "Loading your logins…"
                    } else if query.is_empty() {
                        "No website matches. Search all logins to find an account."
                    } else {
                        "No logins match your search."
                    })
                    .size(13)
                    .color(palette.text_muted),
                );
            }
            for entry in matches {
                let can_fill = document.is_some_and(|doc| {
                    state.settings.autofill
                        && entry.site.container == doc.site.container
                        && (doc.password && entry.has_password
                            || doc.document.username_only && !entry.username.is_empty())
                });
                let control = |symbol: &'static str,
                               label: &'static str,
                               action: CredentialCommand,
                               enabled: bool| {
                    enzui_widgets::toolbar_icon::enz_toolbar_icon(symbol, message(action))
                        .aria_label(label)
                        .tooltip(label)
                        .enabled(enabled && !state.busy)
                        .size(28., 15.)
                        .into_element()
                };
                let controls = column![
                    row![
                        control(
                            "arrow-down-to-line",
                            "Fill login",
                            CredentialCommand::FillSelected(entry.id.clone()),
                            can_fill
                        ),
                        control(
                            "square-pen",
                            "Edit login",
                            CredentialCommand::View(entry.id.clone()),
                            true
                        ),
                    ]
                    .spacing(2),
                    row![
                        control(
                            "user-round",
                            "Copy username",
                            CredentialCommand::Copy(entry.id.clone(), false),
                            !entry.username.is_empty()
                        ),
                        control(
                            "copy",
                            "Copy password",
                            CredentialCommand::Copy(entry.id.clone(), true),
                            entry.has_password
                        ),
                    ]
                    .spacing(2),
                ]
                .spacing(2);
                let mut details = column![
                    text(if entry.username.is_empty() {
                        "No username"
                    } else {
                        entry.username.expose()
                    })
                    .size(13)
                    .color(palette.text_muted)
                    .width(Length::Fill),
                    text(entry.site_label())
                        .size(11)
                        .color(palette.text_subtle)
                        .width(Length::Fill),
                ]
                .spacing(4)
                .width(Length::Fill);
                if entry.site.container != current.container {
                    details = details.push(
                        text("Different container")
                            .size(11)
                            .color(palette.text_subtle),
                    );
                }
                let (rest, hover, ink) = (palette.surface_alt, palette.surface_hover, palette.text);
                let card = iced::widget::button(
                    row![details, controls]
                        .spacing(12)
                        .align_y(iced::Alignment::Center),
                )
                .on_press(message(CredentialCommand::Show(Some(entry.id.clone()))))
                .padding(8)
                .width(Length::Fill)
                .style(move |_, status| iced::widget::button::Style {
                    background: Some(
                        if matches!(
                            status,
                            iced::widget::button::Status::Hovered
                                | iced::widget::button::Status::Pressed
                        ) {
                            hover
                        } else {
                            rest
                        }
                        .into(),
                    ),
                    text_color: ink,
                    border: iced::Border {
                        radius: 10.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                });
                body = body.push(card);
                if entry.has_totp && document.is_some_and(|doc| doc.otp && entry.matches_site(&doc.site)) {
                    body = body.push(action(
                        "Fill authenticator code",
                        "timer",
                        command(CredentialCommand::Fill(entry.id.clone(), true)),
                    ));
                }
            }
        }
        body.into()
    }
}

impl App {
    /// One login in the popup: its website, username and password, each with
    /// a copy button. The password itself stays in the store until copied.
    fn credential_popup_login<'a>(
        &'a self,
        entry: &'a credentials::provider::Summary,
    ) -> Element<'a, Message> {
        let state = &self.browser.credentials;
        let palette = &self.look().palette;
        let document = self.current_credential_document();
        let current = &self.browser.saved.tabs[self.browser.saved.active];
        let site = entry.site_label();
        let host = url::Url::parse(&site)
            .ok()
            .and_then(|url| url.host_str().map(|host| host.trim_start_matches("www.").to_owned()))
            .unwrap_or_else(|| site.to_string());
        let title = if entry.name.expose().trim().is_empty() {
            host.clone()
        } else {
            entry.name.expose().to_owned()
        };
        let button = |symbol: &'static str, label: &'static str, action: CredentialCommand, enabled: bool| {
            enzui_widgets::toolbar_icon::enz_toolbar_icon(symbol, message(action))
                .aria_label(label)
                .tooltip(label)
                .enabled(enabled && !state.busy)
                .size(30., 16.)
                .into_element()
        };
        let initial = host
            .chars()
            .find(|c| c.is_alphanumeric())
            .map_or_else(|| "?".to_owned(), |c| c.to_uppercase().to_string());
        let (soft, accent) = (palette.accent_soft, palette.accent);
        let badge = iced::widget::container(
            text(initial)
                .size(18)
                .color(accent)
                .font(enzui_theme::text_defaults::font(
                    enzui_theme::FontRole::Ui,
                    enzui_theme::WeightRung::Semibold,
                )),
        )
        .center(40)
        .style(move |_| iced::widget::container::Style {
            background: Some(soft.into()),
            border: iced::Border {
                radius: 12.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });
        let header = row![
            button("arrow-left", "Back", CredentialCommand::Show(None), true),
            Space::new().width(Length::Fill),
            button(
                "square-pen",
                "Edit login",
                CredentialCommand::View(entry.id.clone()),
                true
            ),
        ]
        .align_y(iced::Alignment::Center);
        let mut identity = column![
            text(title)
                .size(16)
                .font(enzui_theme::text_defaults::font(
                    enzui_theme::FontRole::Ui,
                    enzui_theme::WeightRung::Semibold,
                ))
                .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
            text(host).size(12).color(palette.text_muted),
        ]
        .spacing(2)
        .width(Length::Fill);
        if entry.site.container != current.container {
            identity = identity.push(text("Different container").size(11).color(palette.text_subtle));
        }
        let hero = row![badge, identity]
            .spacing(12)
            .align_y(iced::Alignment::Center);
        let field = |label: &'static str, value: Element<'a, Message>, copy: Element<'a, Message>| -> Element<'a, Message> {
            row![
                column![text(label).size(11).color(palette.text_subtle), value]
                    .spacing(2)
                    .width(Length::Fill),
                copy,
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center)
            .padding(iced::Padding::default().top(8).bottom(8).left(12).right(6))
            .into()
        };
        let value = |value: String, color: iced::Color| -> Element<'a, Message> {
            text(value)
                .size(13)
                .color(color)
                .wrapping(iced::widget::text::Wrapping::Glyph)
                .into()
        };
        let mut fields = column![
            field(
                "Website",
                value(site.to_string(), palette.text),
                button(
                    "copy",
                    "Copy website",
                    CredentialCommand::CopySite(site.to_string()),
                    !entry.site.origin.is_empty() || entry.uri_rules.is_some()
                ),
            ),
            iced::widget::rule::horizontal(1),
            field(
                "Username",
                if entry.username.is_empty() {
                    value("No username".into(), palette.text_muted)
                } else {
                    value(entry.username.expose().to_owned(), palette.text)
                },
                button(
                    "copy",
                    "Copy username",
                    CredentialCommand::Copy(entry.id.clone(), false),
                    !entry.username.is_empty()
                ),
            ),
            iced::widget::rule::horizontal(1),
            field(
                "Password",
                if entry.has_password {
                    value("••••••••••••".into(), palette.text)
                } else {
                    value("No password".into(), palette.text_muted)
                },
                button(
                    "copy",
                    "Copy password",
                    CredentialCommand::Copy(entry.id.clone(), true),
                    entry.has_password
                ),
            ),
        ];
        if entry.has_totp {
            fields = fields.push(iced::widget::rule::horizontal(1)).push(field(
                "Authenticator",
                value("Saved with this login".into(), palette.text_muted),
                Space::new().width(30).into(),
            ));
        }
        let ground = palette.surface_alt;
        let card = iced::widget::container(fields)
            .width(Length::Fill)
            .clip(true)
            .style(move |_| iced::widget::container::Style {
                background: Some(ground.into()),
                border: iced::Border {
                    radius: 12.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            });
        let mut body = column![header, hero, card].spacing(14);
        if !state.notice.is_empty() {
            body = body.push(text(&state.notice).size(12).color(palette.text_muted));
        }
        let can_fill = document.is_some_and(|doc| {
            state.settings.autofill
                && entry.site.container == doc.site.container
                && (doc.password && entry.has_password
                    || doc.document.username_only && !entry.username.is_empty())
        });
        if can_fill {
            body = body.push(primary(
                "Fill login",
                "arrow-down-to-line",
                command(CredentialCommand::FillSelected(entry.id.clone())),
            ));
        }
        if entry.has_totp && document.is_some_and(|doc| doc.otp && entry.matches_site(&doc.site)) {
            body = body.push(action(
                "Fill authenticator code",
                "timer",
                command(CredentialCommand::Fill(entry.id.clone(), true)),
            ));
        }
        body.into()
    }
}

fn popup_matches(entry: &credentials::provider::Summary, site: Option<&Site>, query: &str) -> bool {
    if query.is_empty() {
        return site.is_some_and(|site| entry.matches_site(site));
    }
    entry.matches_query(query)
}

impl App {
    pub(super) fn refresh_credential_documents(&self) {
        for index in 0..self.browser.saved.tabs.len() {
            let host = if index == self.browser.saved.active {
                Some(&self.webview)
            } else {
                self.browser.sleeping[index].as_ref().map(|tab| &tab.host)
            };
            let Some(host) = host else { continue };
            let service = host.borrow();
            if let (Some(native), Some(view)) = (&service.native, service.live_view) {
                let _ = native.browse(&service.router, view, BrowserAction::CredentialRefresh);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn review_login(origin: &str, username: &str) -> credentials::provider::Summary {
        let mut login = Credential::new(Site::parse(origin, None).expect("site")).expect("login");
        login.name = Secret::new("Personal account".into());
        login.username = Secret::new(username.into());
        login.password = Secret::new("not-a-searchable-password".into());
        credentials::provider::Summary::from(&login)
    }
    #[test]
    fn password_review_search_finds_accounts_beyond_url_matching_without_searching_secrets() {
        let entry = review_login("https://accounts.example.test", "review@example.test");
        let current = Site::parse("https://login.example.test", None).expect("site");
        assert!(!popup_matches(&entry, Some(&current), ""));
        assert!(popup_matches(&entry, Some(&current), "review@"));
        assert!(popup_matches(&entry, None, "accounts.example"));
        assert!(!popup_matches(&entry, None, "not-a-searchable-password"));
    }
    #[test]
    fn password_review_cached_bitwarden_matches_keep_secondary_urls_and_uri_rules() {
        let mut entry = review_login("https://primary.example.test", "review");
        entry.uri_rules = Some(vec![
            (
                Secret::new("https://secondary.example.test/login".into()),
                Some(2),
            ),
            (Secret::new("https://never.example.test".into()), Some(5)),
        ]);
        let login = Site::parse("https://secondary.example.test/login/step", None).expect("login");
        let other_path = Site::parse("https://secondary.example.test/other", None).expect("path");
        assert!(popup_matches(&entry, Some(&login), ""));
        assert!(!popup_matches(&entry, Some(&other_path), ""));
        assert!(!popup_matches(
            &entry,
            Some(&Site::parse("https://never.example.test", None).expect("site")),
            ""
        ));
        assert!(popup_matches(&entry, None, "secondary.example"));
        assert!(!popup_matches(
            &entry,
            Some(&Site::parse("https://secondary.example.test/login", Some(7)).expect("container")),
            ""
        ));
    }
    #[test]
    fn password_review_opening_a_warm_popup_does_not_start_a_connector_request() {
        let mut app = crate::browser::browser_tests::app();
        app.browser.credentials.settings = Settings::enabled_defaults();
        app.browser.credentials.unlocked = true;
        app.browser.credentials.catalog_loaded = true;
        app.browser
            .credentials
            .catalog
            .push(review_login("https://example.test", "review"));
        app.browser.credentials.dismiss();
        assert!(
            app.browser.credentials.catalog_loaded,
            "closing UI keeps warmed metadata"
        );
        let _ = app.browser_command(Command::Credential(CredentialCommand::OpenSite));
        assert_eq!(app.browser.panel, Panel::Passwords);
        assert!(!app.browser.credentials.busy);
        assert!(
            app.browser.credentials.service.is_none(),
            "warm popup must not reconnect or reload"
        );
        app.browser.credentials.busy = true;
        let _ = app.credential_command(CredentialCommand::Query(Secret::new("review".into())));
        assert_eq!(
            app.browser.credentials.query.expose(),
            "review",
            "cached search stays live during asynchronous work"
        );
    }
    #[test]
    fn password_review_closing_ui_keeps_background_initialization_but_lock_revokes_it() {
        let mut state = State {
            busy: true,
            pending_purpose: Some(Purpose::List),
            sequence: 42,
            ..Default::default()
        };
        state.dismiss();
        assert!(state.busy, "closing UI must not cancel metadata preloading");
        assert_eq!(state.sequence, 42);
        state.lock();
        assert!(!state.busy);
        assert!(state.pending_purpose.is_none());
        assert_ne!(state.sequence, 42);
    }
    #[test]
    fn password_review_lock_clears_cached_metadata_without_restarting_initialization() {
        let mut state = State {
            initialized: true,
            catalog_loaded: true,
            ..Default::default()
        };
        state
            .catalog
            .push(review_login("https://example.test", "review"));
        state.lock();
        assert!(state.catalog.is_empty());
        assert!(!state.catalog_loaded);
        assert!(state.initialized);
    }
    #[test]
    #[ignore = "rendered review snapshots; set ENZIM_VISUAL_OUTPUT to an isolated capture directory"]
    fn password_review_popup_snapshots_both_themes_and_small_sizes() {
        let output = std::path::PathBuf::from(
            std::env::var_os("ENZIM_VISUAL_OUTPUT").expect("capture directory"),
        );
        for (name, theme) in [
            ("dark", enzui_theme::EnzTheme::dark()),
            ("light", enzui_theme::EnzTheme::light()),
        ] {
            let mut app = crate::browser::browser_tests::app();
            app.theme =
                enzui_core::theme_file::integration::ThemeIntegration::unmanaged_compiled_theme(
                    theme,
                )
                .0;
            app.browser.credentials.settings = Settings::enabled_defaults();
            app.browser.credentials.unlocked = true;
            app.browser.credentials.catalog_loaded = true;
            app.browser.credentials.catalog = (0..30)
                .map(|index| {
                    review_login("https://example.test", &format!("alex{index}@example.test"))
                })
                .collect();
            app.browser.credentials.query = Secret::new("example".into());
            for (width, height) in [(400., 540.), (396., 280.)] {
                app.browser.viewport = iced::Size::new(width + 24., height + 80.);
                let content = app.panel_body(Panel::Passwords);
                let mut ui = iced_test::Simulator::with_size(
                    Default::default(),
                    iced::Size::new(width, height),
                    content,
                );
                let snapshot = ui
                    .snapshot(&app.theme.iced_theme())
                    .expect("rendered password popup");
                let path = output.join(format!(
                    "password-popup-{name}-{}x{}",
                    width as u32, height as u32
                ));
                assert!(snapshot.matches_image(path).expect("PNG capture"));
            }
            let first = app.browser.credentials.catalog[0].id.clone();
            let _ = app.credential_command(CredentialCommand::Show(Some(first)));
            for (width, height) in [(400., 540.), (300., 400.)] {
                app.browser.viewport = iced::Size::new(width + 24., height + 80.);
                let content = app.panel_body(Panel::Passwords);
                let mut ui = iced_test::Simulator::with_size(
                    Default::default(),
                    iced::Size::new(width, height),
                    content,
                );
                let snapshot = ui
                    .snapshot(&app.theme.iced_theme())
                    .expect("rendered password login");
                let path = output.join(format!(
                    "password-login-{name}-{}x{}",
                    width as u32, height as u32
                ));
                assert!(snapshot.matches_image(path).expect("PNG capture"));
            }
        }
    }
    #[test]
    fn the_popup_shows_one_login_until_back_or_dismissed() {
        let mut app = crate::browser::browser_tests::app();
        app.browser.credentials.settings = Settings::enabled_defaults();
        app.browser.credentials.unlocked = true;
        app.browser.credentials.catalog_loaded = true;
        app.browser.credentials.catalog = vec![review_login("https://example.test", "alex@example.test")];
        let id = app.browser.credentials.catalog[0].id.clone();
        let _ = app.credential_command(CredentialCommand::Show(Some(id.clone())));
        assert_eq!(app.browser.credentials.shown.as_deref(), Some(id.as_str()));
        let _ = app.credential_command(CredentialCommand::Show(None));
        assert!(app.browser.credentials.shown.is_none(), "back returns to the list");
        let _ = app.credential_command(CredentialCommand::Show(Some(id)));
        app.browser.credentials.dismiss();
        assert!(app.browser.credentials.shown.is_none(), "closing the popup forgets the login");
    }
    #[test]
    #[ignore = "rendered provider review; set ENZIM_VISUAL_OUTPUT to a disposable capture directory"]
    fn provider_review_page_snapshots_both_themes_and_small_sizes() {
        let output = std::path::PathBuf::from(
            std::env::var_os("ENZIM_VISUAL_OUTPUT").expect("capture directory"),
        );
        for (name, theme) in [
            ("dark", enzui_theme::EnzTheme::dark()),
            ("light", enzui_theme::EnzTheme::light()),
        ] {
            let mut app = crate::browser::browser_tests::app();
            app.theme =
                enzui_core::theme_file::integration::ThemeIntegration::unmanaged_compiled_theme(
                    theme,
                )
                .0;
            app.browser.private = false;
            app.browser.library = Some(InternalPage::Passwords);
            app.browser.saved.layout = Layout::Top;
            app.browser.credentials.settings = Settings::enabled_defaults();
            app.browser.credentials.vault_exists = true;
            let mut connection = credentials::accounts::Connection::new(
                credentials::accounts::Kind::Bitwarden,
                "Personal Bitwarden",
            )
            .expect("connection");
            connection.account = "alex@example.test".into();
            connection.remember = false;
            connection.lock_minutes = Some(5);
            app.browser.credentials.settings.save_to = connection.id.clone();
            app.browser
                .credentials
                .settings
                .accounts
                .push(connection.clone());
            for (view, preferences, editor) in [
                ("providers", false, false),
                ("bitwarden", false, true),
                ("preferences", true, false),
            ] {
                app.browser.credentials.provider_preferences = preferences;
                if editor {
                    let _ = app.credential_account_command(CredentialCommand::ConnectionEdit(
                        connection.id.clone(),
                    ));
                } else {
                    app.browser.credentials.connection = None;
                }
                for (width, height) in [(760., 640.), (396., 280.)] {
                    app.browser.viewport = iced::Size::new(width + 20., height);
                    let mut ui = iced_test::Simulator::with_size(
                        Default::default(),
                        iced::Size::new(width, height),
                        app.credentials_page(),
                    );
                    let snapshot = ui
                        .snapshot(&app.theme.iced_theme())
                        .expect("rendered provider page");
                    assert!(
                        snapshot
                            .matches_image(output.join(format!(
                                "password-{view}-{name}-{}x{}",
                                width as u32, height as u32
                            )))
                            .expect("PNG capture")
                    );
                }
            }
        }
    }
    #[test]
    fn credential_startup_is_once_per_window_and_never_reopens_an_idle_locked_vault() {
        let mut state = State {
            settings: Settings::enabled_defaults(),
            ..Default::default()
        };
        for kind in credentials::accounts::Kind::ALL {
            state.settings.accounts = vec![
                credentials::accounts::Connection::new(kind, "Startup fixture")
                    .expect("connection"),
            ];
            assert!(
                state.initialization_pending(true),
                "every configured provider gets startup initialization"
            );
            assert!(
                !state.initialization_pending(false),
                "welcome and disallowed private windows do not initialize credentials"
            );
        }
        state.busy = true;
        assert!(
            !state.initialization_pending(true),
            "startup waits for an existing operation"
        );
        state.busy = false;
        state.initialized = true;
        state.clear_secrets();
        assert!(
            !state.initialization_pending(true),
            "idle lock cannot restart remembered sessions in the background"
        );
        state.initialized = false;
        state.settings.enabled = false;
        assert!(
            !state.initialization_pending(true),
            "disabled password managers stay dormant"
        );
        state.settings.enabled = true;
        state.settings.provider = "external.fixture".into();
        assert!(
            !state.initialization_pending(true),
            "replacement providers retain ownership"
        );
    }
    #[test]
    fn secure_copy_completion_does_not_release_another_operation() {
        let mut app = crate::browser::browser_tests::app();
        app.browser.credentials.sequence = 9;
        app.browser.credentials.busy = true;
        let _ = app.credential_command(CredentialCommand::ClipboardWritten(8, true));
        assert!(
            app.browser.credentials.busy,
            "stale copy cannot release a new request"
        );
        let _ = app.credential_command(CredentialCommand::ClipboardWritten(9, false));
        assert!(
            !app.browser.credentials.busy,
            "current refusal releases the copy control"
        );
        assert!(
            app.browser.toasts.texts().iter().any(|(tone, text)| *tone
                == enzui_widgets::toast::ToastTone::Error
                && text.contains("not confirmed")),
            "refused clipboard never reports a successful copy"
        );
    }
    #[test]
    fn fill_completion_from_a_previous_tab_releases_only_its_own_busy_state() {
        let mut app = crate::browser::browser_tests::app();
        let current = app.browser.saved.tabs[app.browser.saved.active].key;
        app.browser.credentials.sequence = 7;
        app.browser.credentials.busy = true;
        let _ = app.credential_command(CredentialCommand::FillReady(
            current.wrapping_add(1),
            6,
            Err("obsolete".into()),
        ));
        assert!(
            app.browser.credentials.busy,
            "an older operation cannot release a newer request"
        );
        let _ = app.credential_command(CredentialCommand::FillReady(
            current.wrapping_add(1),
            7,
            Err("retired tab".into()),
        ));
        assert!(
            !app.browser.credentials.busy,
            "the completed operation cannot leave the manager stuck on another tab"
        );
        assert_eq!(
            app.browser.panel,
            Panel::None,
            "a retired tab does not open an error popup on the current page"
        );
    }
    #[test]
    fn connection_authentication_input_is_cleared_on_dismiss_lock_and_destination_change() {
        for operation in ["dismiss", "lock", "destination"] {
            let mut app = crate::browser::browser_tests::app();
            let state = &mut app.browser.credentials;
            state.authenticator_code = Secret::new("123456".into());
            state.api_client_id = Secret::new("user.fixture".into());
            state.api_client_secret = Secret::new("private-fixture-secret".into());
            match operation {
                "dismiss" => state.dismiss(),
                "lock" => state.lock(),
                _ => {
                    let _ = app.credential_account_command(CredentialCommand::SourceSelect(0));
                }
            }
            let state = &app.browser.credentials;
            assert!(state.authenticator_code.is_empty(), "{operation}");
            assert!(state.api_client_id.is_empty(), "{operation}");
            assert!(state.api_client_secret.is_empty(), "{operation}");
        }
    }
    #[test]
    fn dismissing_one_window_does_not_cancel_another_windows_shared_store() {
        let scratch = credentials::tests::Scratch::new();
        let config = scratch.path().with_file_name("settings.json");
        credentials::config::save(&config, &Settings::enabled_defaults()).expect("settings");
        let first = Service::shared(scratch.path(), config.clone()).expect("first window");
        let second = Service::shared(scratch.path(), config).expect("second window");
        let reply = first
            .request(Operation::Open {
                password: Secret::new("a distinct fixture master password".into()),
                create: true,
            })
            .expect("unlock")
            .blocking_recv()
            .expect("reply");
        assert!(reply.result.is_ok() && second.unlocked());
        let generation = second.generation();
        let mut state = State {
            service: Some(first),
            busy: true,
            unlocked: true,
            master: Secret::new("window-local input".into()),
            ..Default::default()
        };
        state.dismiss();
        assert!(second.unlocked());
        assert_eq!(second.generation(), generation);
        assert!(!state.busy);
        assert!(state.master.is_empty());
    }
    #[test]
    fn a_revoked_reply_clears_busy_state_and_sensitive_editor_data() {
        let scratch = credentials::tests::Scratch::new();
        let config = scratch.path().with_file_name("settings.json");
        credentials::config::save(&config, &Settings::enabled_defaults()).expect("settings");
        let service = Service::shared(scratch.path(), config).expect("service");
        let mut app = App::new(enzui_theme::EnzTheme::dark());
        app.browser.credentials.service = Some(service);
        app.browser.credentials.busy = true;
        app.browser.credentials.master = Secret::new("must be cleared".into());
        app.browser.credentials.sequence = 7;
        let _ = app.credential_command(CredentialCommand::Ready(
            7,
            Purpose::List,
            Reply {
                generation: 0,
                unlocked: true,
                authority: Vec::new(),
                result: Ok(Output::Entries(Vec::new())),
            },
        ));
        assert!(!app.browser.credentials.busy);
        assert!(app.browser.credentials.master.is_empty());
        assert!(!app.browser.credentials.unlocked);
    }
    #[test]
    fn system_and_connected_fill_sources_do_not_require_a_browser_vault_or_spurious_keyring_reads()
    {
        let mut state = State::default();
        assert!(!state.has_destination());
        assert!(!state.desktop_available());
        state.settings.storage = credentials::accounts::Storage::System;
        assert!(state.has_destination());
        assert!(state.desktop_available());
        state.settings.storage = credentials::accounts::Storage::Browser;
        let mut account = credentials::accounts::Connection::new(
            credentials::accounts::Kind::Bitwarden,
            "Personal",
        )
        .expect("connection");
        account.remember = false;
        state.settings.accounts.push(account);
        assert!(state.has_destination());
        assert!(!state.desktop_available());
        state.settings.accounts[0].remember = true;
        assert!(state.desktop_available());
        state.settings.accounts[0].enabled = false;
        assert!(!state.has_destination());
    }
    #[test]
    fn password_connection_settings_do_not_require_an_unlocked_browser_vault() {
        let mut app = App::new(enzui_theme::EnzTheme::dark());
        assert!(!app.browser.credentials.unlocked);
        let _ = app.credential_command(CredentialCommand::Transfer(TransferMode::Settings));
        assert_eq!(app.browser.credentials.transfer, TransferMode::Settings);
        assert!(app.browser.credentials.service.is_none());
    }
    fn fixture() -> App {
        let mut app = App::new(enzui_theme::EnzTheme::dark());
        app.browser_mode = true;
        app.browser.credentials.settings = Settings::enabled_defaults();
        app.browser.saved.tabs[app.browser.saved.active].url = "https://example.test/login".into();
        app.webview.borrow_mut().location = Some("https://example.test/login".into());
        app
    }
    fn ready(origin: &str) -> enzui_security::secret::SecretText {
        enzui_security::secret::SecretText::new(serde_json::json!({"kind":"ready","document":"0123456789abcdef0123456789abcdef","origin":origin,"target":"1","action":origin,"password":true,"otp":false}).to_string())
    }
    fn submitted(document: &str) -> enzui_security::secret::SecretText {
        enzui_security::secret::SecretText::new(serde_json::json!({"kind":"submitted","document":document,"origin":"https://example.test","action":"https://example.test","username":"private-username-sentinel","password":"private-password-sentinel"}).to_string())
    }
    #[test]
    fn credential_ui_requires_tls_and_current_origin_and_never_saves_private_submissions() {
        let mut app = fixture();
        app.credential_bridge_message(ready("https://example.test"));
        assert!(
            app.browser.credentials.document.is_none(),
            "no trusted TLS state means no credential document"
        );
        app.webview.borrow_mut().tls = Some("WebKit reported no certificate errors".into());
        app.credential_bridge_message(ready("https://evil.test"));
        assert!(
            app.browser.credentials.document.is_none(),
            "page-reported origin must match the native location"
        );
        app.credential_bridge_message(ready("https://example.test"));
        assert!(
            app.browser.credentials.document.is_some(),
            "authenticated current document accepted"
        );
        assert!(
            app.current_credential_document().is_some(),
            "current secure context can offer a login"
        );
        let profile = app.webview.borrow().profile.clone();
        app.webview.borrow_mut().profile = enzui_webkit::web::WebProfile::ephemeral();
        assert!(
            app.current_credential_document().is_none(),
            "another storage profile cannot reuse cached form identity"
        );
        app.webview.borrow_mut().profile = profile;
        app.webview.borrow_mut().location = Some("http://example.test/".into());
        assert!(
            app.current_credential_document().is_none(),
            "navigation cannot offer a previous secure page's logins"
        );
        app.webview.borrow_mut().location = Some("https://example.test/login".into());
        app.webview.borrow_mut().loading = true;
        assert!(
            app.current_credential_document().is_none(),
            "loading documents offer no cached login"
        );
        app.webview.borrow_mut().loading = false;
        app.credential_bridge_message(submitted("stale-document"));
        assert!(
            app.browser.credentials.candidate.is_none(),
            "old document cannot create a save offer"
        );
        app.browser.private = true;
        app.browser.credentials.settings.private_fill = true;
        app.credential_bridge_message(submitted("0123456789abcdef0123456789abcdef"));
        assert!(
            app.browser.credentials.candidate.is_none(),
            "private fill permission never permits saving"
        );
        app.browser.private = false;
        app.credential_bridge_message(submitted("0123456789abcdef0123456789abcdef"));
        assert!(
            app.browser.credentials.candidate.is_some(),
            "regular trusted submission offers explicit save"
        );
        let session = serde_json::to_string(&app.browser.saved).expect("session serialization");
        assert!(
            !session.contains("private-username-sentinel")
                && !session.contains("private-password-sentinel"),
            "save offers cannot enter the browser session"
        );
        let _ = app.credential_command(CredentialCommand::Open);
        assert!(
            app.browser.credentials.candidate.is_some(),
            "explicit management transition preserves the submission for review"
        );
        app.close_library();
        assert!(
            app.browser.credentials.candidate.is_none(),
            "closing the manager releases the submission"
        );
        app.browser.credentials.settings.enabled = false;
        app.credential_bridge_message(submitted("0123456789abcdef0123456789abcdef"));
        assert!(
            app.browser.credentials.candidate.is_none(),
            "disabled manager accepts no submission"
        );
    }
    #[test]
    fn credential_ui_file_chooser_reply_cannot_follow_a_new_generation() {
        let mut app = fixture();
        app.browser.credentials.sequence = 41;
        app.browser.credentials.master = Secret::new("new-export-fixture-sentinel".into());
        let _ = app.credential_command(CredentialCommand::FileChosen(
            40,
            vec!["/never-read-stale-credential-fixture".into()],
            true,
        ));
        assert_eq!(
            app.browser.credentials.sequence, 41,
            "old chooser cannot start a new credential operation"
        );
        assert!(
            !app.browser.credentials.master.is_empty(),
            "old chooser cannot alter a newer credential draft"
        );
    }
    #[test]
    fn credential_ui_session_lock_clears_inputs_and_rejects_stale_fill_replies() {
        let mut app = fixture();
        app.browser.credentials.master = Secret::new("fixture-master-sentinel".into());
        app.browser.credentials.current = Secret::new("fixture-current-sentinel".into());
        app.browser.credentials.unlocked = true;
        let tab = app.browser.saved.tabs[app.browser.saved.active].key;
        let sequence = app.browser.credentials.sequence;
        let _ = app.credential_command(CredentialCommand::Session(
            enzui_core::portal::session_monitor::SessionEvent {
                state: enzui_core::portal::session_monitor::SessionState::Running,
                screensaver_active: true,
                ack: None,
            },
        ));
        assert!(
            app.browser.credentials.master.is_empty()
                && app.browser.credentials.current.is_empty()
                && !app.browser.credentials.unlocked,
            "desktop locking releases credential inputs and display state"
        );
        app.browser.panel = Panel::Passwords;
        let _ = app.credential_command(CredentialCommand::FillReady(
            tab,
            sequence,
            Ok(crate::native_jobs::Payload::SecretJson(
                enzui_security::secret::SecretText::new("true".into()),
            )),
        ));
        assert_eq!(
            app.browser.panel,
            Panel::Passwords,
            "an old native reply cannot change the locked UI"
        );
    }
    #[test]
    fn credential_ui_timers_require_a_visible_valid_authenticator_or_exact_expiry() {
        let mut state = State {
            unlocked: true,
            busy: true,
            ..State::default()
        };
        assert!(!state.has_visible_totp(true));
        assert!(
            state.expiry_deadline().is_none(),
            "unlocked and busy status do not create polling deadlines"
        );
        let login =
            Credential::new(Site::parse("https://example.test", None).expect("fixture site"))
                .expect("fixture login");
        state.editor = Some(Editor {
            origin: login.website(),
            login,
            totp: Secret::default(),
            reveal: false,
            confirm_delete: false,
        });
        assert!(
            !state.has_visible_totp(true),
            "a password without an authenticator has no timer"
        );
        state.editor.as_mut().expect("editor").totp = Secret::new("invalid seed!".into());
        assert!(!state.has_visible_totp(true));
        state.editor.as_mut().expect("editor").totp = Secret::new("JBSWY3DPEHPK3PXP".into());
        assert!(
            !state.has_visible_totp(false),
            "normal websites do not animate manager codes"
        );
        assert!(
            state.has_visible_totp(true),
            "visible valid code gets a countdown"
        );
        state.unlocked = false;
        assert!(!state.has_visible_totp(true));
        let born = std::time::Instant::now();
        state.candidate = Some(Candidate {
            login: state.editor.as_ref().expect("editor").login.clone(),
            born,
            filled: None,
            checked: false,
            presented: false,
        });
        assert_eq!(
            state.expiry_deadline(),
            Some(born + std::time::Duration::from_secs(60)),
            "save offers schedule their exact secret expiry rather than a polling tick"
        );
    }
}
