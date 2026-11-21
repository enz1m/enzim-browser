use super::*;
use credentials::{
    accounts::{Connection, Kind, Storage as LocalStorage},
    router::{default_destination, destinations},
};
use iced::widget::{column, row};

pub(super) struct Editor {
    connection: Connection,
    advanced: bool,
    confirm_delete: bool,
}
pub(super) fn handles(command: &CredentialCommand) -> bool {
    matches!(
        command,
        CredentialCommand::Providers
            | CredentialCommand::ProviderPreferences
            | CredentialCommand::ProviderDefault(_)
            | CredentialCommand::ProviderRemove(_)
            | CredentialCommand::AddLocal
            | CredentialCommand::ConnectionTimeout(_)
            | CredentialCommand::Storage(_)
            | CredentialCommand::Destination(_)
            | CredentialCommand::DefaultDestination(_)
            | CredentialCommand::SourceSelect(_)
            | CredentialCommand::ProviderLogins(_)
            | CredentialCommand::ConnectionNew(_)
            | CredentialCommand::ConnectionEdit(_)
            | CredentialCommand::ConnectionField(..)
            | CredentialCommand::ConnectionToggle(..)
            | CredentialCommand::ConnectionSave
            | CredentialCommand::ConnectionDelete
            | CredentialCommand::ConnectionCancel
    )
}
impl App {
    pub(super) fn credential_account_command(
        &mut self,
        action: CredentialCommand,
    ) -> Task<Message> {
        use CredentialCommand::*;
        if self.browser.credentials.busy || self.browser.private {
            return Task::none();
        }
        if matches!(
            action,
            ConnectionNew(_)
                | ConnectionEdit(_)
                | ConnectionCancel
                | SourceSelect(_)
                | ProviderLogins(_)
        ) {
            self.reset_library_scroll();
        }
        let state = &mut self.browser.credentials;
        match action {
            Providers => {
                state.dismiss();
                state.connection = None;
                state.browse_logins = false;
                state.provider_preferences = false;
                state.removing_provider = None;
                state.unlocking = false;
                self.reset_library_scroll();
            }
            ProviderPreferences => {
                state.provider_preferences = true;
                state.browse_logins = false;
                state.transfer = TransferMode::Settings;
                self.reset_library_scroll();
            }
            ProviderDefault(id) => {
                let Some(source) = destinations(&state.settings, state.vault_exists)
                    .into_iter()
                    .find(|source| source.id == id && source.writable)
                else {
                    return Task::none();
                };
                let mut settings = state.settings.clone();
                settings.save_to.clone_from(&source.id);
                settings.preferred_source = source.id;
                state.lock();
                return self.credential_save_settings(settings);
            }
            ProviderRemove(id) => {
                if state.removing_provider.as_deref() != Some(&id) {
                    state.removing_provider = Some(id);
                    return Task::none();
                }
                let mut settings = state.settings.clone();
                match id.as_str() {
                    "browser" => settings.browser_store = false,
                    "system" => {
                        settings.system_store = false;
                        settings.storage = LocalStorage::Browser;
                    }
                    _ => settings.accounts.retain(|account| account.id != id),
                }
                if settings.save_to == id {
                    settings.save_to.clear();
                }
                if settings.preferred_source == id {
                    settings.preferred_source.clear();
                }
                state.removing_provider = None;
                state.lock();
                return self.credential_save_settings(settings);
            }
            AddLocal => {
                let mut settings = state.settings.clone();
                settings.browser_store = true;
                settings.storage = LocalStorage::Browser;
                settings.enabled = true;
                settings.setup_provider = None;
                state.lock();
                return self.credential_save_settings(settings);
            }
            ConnectionTimeout(index) => {
                if let Some(editor) = &mut state.connection {
                    editor.connection.remember = index == 6;
                    editor.connection.lock_minutes = [
                        Some(1),
                        Some(5),
                        Some(15),
                        Some(30),
                        Some(60),
                        Some(0),
                        None,
                    ][index.min(6)];
                }
            }
            Storage(index) => {
                let mut settings = state.settings.clone();
                settings.storage = if index == 1 {
                    LocalStorage::System
                } else {
                    LocalStorage::Browser
                };
                settings.system_store = settings.storage == LocalStorage::System;
                if settings.storage == LocalStorage::Browser {
                    settings.browser_store = true;
                }
                settings.setup_provider = None;
                settings.preferred_source.clear();
                if settings.save_to == "browser" || settings.save_to == "system" {
                    settings.save_to.clear();
                }
                state.lock();
                return self.credential_save_settings(settings);
            }
            Destination(index) | DefaultDestination(index) => {
                let choices = destinations(&state.settings, state.vault_exists)
                    .into_iter()
                    .filter(|source| source.writable)
                    .collect::<Vec<_>>();
                if let Some(choice) = choices.get(index) {
                    state.clear_connection_auth();
                    state.api_key_mode = false;
                    state.destination.clone_from(&choice.id);
                    state.unlock_source.clone_from(&choice.id);
                    if matches!(action, DefaultDestination(_)) {
                        let mut settings = state.settings.clone();
                        settings.save_to.clone_from(&choice.id);
                        settings.preferred_source.clone_from(&choice.id);
                        settings.setup_provider = None;
                        state.lock();
                        return self.credential_save_settings(settings);
                    }
                }
            }
            SourceSelect(index) | ProviderLogins(index) => {
                if let Some(source) = destinations(&state.settings, state.vault_exists).get(index) {
                    state.unlock_source.clone_from(&source.id);
                    state.browse_logins = true;
                    state.unlocking = !matches!(action, ProviderLogins(_))
                        || !state
                            .service
                            .as_ref()
                            .is_some_and(|service| service.source_unlocked(&source.id));
                    state.transfer = TransferMode::None;
                    if source.writable {
                        state.destination.clone_from(&source.id);
                    }
                    state.master = Secret::default();
                    state.clear_connection_auth();
                    state.api_key_mode = false;
                    state.confirmation = Secret::default();
                    state.notice.clear();
                    if !state.unlocking {
                        return self.credential_request(Purpose::List, Operation::List(None));
                    }
                }
            }
            ConnectionNew(kind) => {
                let base = format!("{} account", kind.label());
                let mut name = base.clone();
                let mut index = 2;
                while state
                    .settings
                    .accounts
                    .iter()
                    .any(|account| account.name.eq_ignore_ascii_case(&name))
                {
                    name = format!("{base} {index}");
                    index += 1;
                }
                match Connection::new(kind, &name) {
                    Ok(mut connection) => {
                        connection.remember = false;
                        connection.lock_minutes = Some(5);
                        state.notice.clear();
                        state.connection = Some(Editor {
                            connection,
                            advanced: false,
                            confirm_delete: false,
                        });
                    }
                    Err(error) => state.notice = error,
                }
            }
            ConnectionEdit(id) => {
                if let Some(connection) = state
                    .settings
                    .accounts
                    .iter()
                    .find(|account| account.id == id)
                {
                    state.connection = Some(Editor {
                        connection: connection.clone(),
                        advanced: false,
                        confirm_delete: false,
                    });
                }
            }
            ConnectionField(field, value) => {
                if let Some(editor) = &mut state.connection {
                    match field {
                        "name" => editor.connection.name = value,
                        "account" => editor.connection.account = value,
                        "server" => editor.connection.server = value,
                        "executable" => editor.connection.executable = value,
                        "directory" => {
                            editor.connection.directory = (!value.trim().is_empty())
                                .then(|| std::path::PathBuf::from(value.trim()));
                        }
                        _ => {}
                    }
                }
            }
            ConnectionToggle(field, value) => {
                if let Some(editor) = &mut state.connection {
                    match field {
                        "remember" => {
                            editor.connection.remember = value;
                            editor.connection.lock_minutes = None;
                        }
                        "enabled" => editor.connection.enabled = value,
                        "advanced" => editor.advanced = value,
                        _ => {}
                    }
                }
            }
            ConnectionSave => {
                let Some(editor) = &state.connection else {
                    return Task::none();
                };
                if let Err(error) = editor.connection.validate() {
                    state.notice = error;
                    return Task::none();
                }
                let mut settings = state.settings.clone();
                let connection = editor.connection.clone();
                if connection.enabled {
                    settings.enabled = true;
                }
                if !connection.enabled && settings.save_to == connection.id {
                    settings.save_to.clear();
                }
                if !connection.enabled && settings.preferred_source == connection.id {
                    settings.preferred_source.clear();
                }
                if connection.enabled && settings.setup_provider == Some(connection.kind) {
                    settings.preferred_source.clone_from(&connection.id);
                    settings.setup_provider = None;
                    settings.save_to.clone_from(&connection.id);
                }
                if settings.accounts.is_empty()
                    && !state.vault_exists
                    && settings.storage == LocalStorage::Browser
                    && connection.enabled
                {
                    settings.save_to.clone_from(&connection.id);
                }
                if let Some(existing) = settings
                    .accounts
                    .iter_mut()
                    .find(|account| account.id == connection.id)
                {
                    *existing = connection;
                } else {
                    settings.accounts.push(connection);
                }
                if let Err(error) = credentials::accounts::validate_connections(
                    &settings.accounts,
                    &settings.save_to,
                ) {
                    state.notice = error;
                    return Task::none();
                }
                state.lock();
                return self.credential_save_settings(settings);
            }
            ConnectionDelete => {
                let Some(editor) = &mut state.connection else {
                    return Task::none();
                };
                if !editor.confirm_delete {
                    editor.confirm_delete = true;
                    return Task::none();
                }
                let id = editor.connection.id.clone();
                let mut settings = state.settings.clone();
                settings.accounts.retain(|account| account.id != id);
                if settings.save_to == id {
                    settings.save_to.clear();
                }
                if settings.preferred_source == id {
                    settings.preferred_source.clear();
                }
                state.lock();
                return self.credential_save_settings(settings);
            }
            ConnectionCancel => {
                state.connection = None;
                state.notice.clear();
            }
            _ => {}
        }
        Task::none()
    }
    pub(super) fn credential_destination_picker(&self, default: bool) -> Element<'_, Message> {
        let state = &self.browser.credentials;
        let choices = destinations(&state.settings, state.vault_exists)
            .into_iter()
            .filter(|source| source.writable)
            .collect::<Vec<_>>();
        if choices.len() < 2 && !default {
            return Space::new().into();
        }
        let current = if default {
            default_destination(&state.settings)
        } else {
            state.destination.clone()
        };
        let selected = choices
            .iter()
            .position(|source| source.id == current)
            .unwrap_or(0);
        let labels = choices
            .into_iter()
            .map(|source| source.name)
            .collect::<Vec<_>>();
        let value = labels.get(selected).cloned();
        enzui_widgets::combo_box::enz_combo_box(
            if default {
                "Default save destination"
            } else {
                "Save to"
            },
            labels,
            value,
            move |_| {
                message(if default {
                    CredentialCommand::DefaultDestination(0)
                } else {
                    CredentialCommand::Destination(0)
                })
            },
        )
        .id_prefix(if default {
            "credential-default-destination"
        } else {
            "credential-save-destination"
        })
        .selected_index(Some(selected))
        .on_select_index(move |index| {
            message(if default {
                CredentialCommand::DefaultDestination(index)
            } else {
                CredentialCommand::Destination(index)
            })
        })
        .into_element()
    }
    pub(super) fn credential_unlock_view(&self) -> Element<'_, Message> {
        let state = &self.browser.credentials;
        if state.busy {
            return password_section(
                "Connecting",
                column![
                    row![
                        enzui_widgets::spinner::enz_spinner().into_element(),
                        text("Connecting to your password manager…").size(14)
                    ]
                    .spacing(10)
                    .align_y(iced::Alignment::Center)
                ],
            );
        }
        let sources = destinations(&state.settings, state.vault_exists);
        let selected = sources
            .iter()
            .position(|source| source.id == state.unlock_source)
            .unwrap_or(0);
        let kind = state
            .settings
            .accounts
            .iter()
            .find(|account| account.id == state.unlock_source)
            .map(|account| account.kind);
        let browser = state.unlock_source == "browser";
        let system = state.unlock_source == "system";
        let create = browser && !state.vault_exists;
        let options = sources
            .into_iter()
            .map(|source| source.name)
            .collect::<Vec<_>>();
        let picker = enzui_widgets::combo_box::enz_combo_box(
            "Password destination",
            options.clone(),
            options.get(selected).cloned(),
            |_| message(CredentialCommand::SourceSelect(0)),
        )
        .id_prefix("credential-unlock-source")
        .selected_index(Some(selected))
        .on_select_index(|index| message(CredentialCommand::SourceSelect(index)))
        .into_element();
        let mut body = column![picker].spacing(12);
        let detail = if system {
            "Your desktop keyring stores the logins. GNOME Keyring and oo7 authorize access; no browser vault is created."
        } else if browser {
            if create {
                "Choose a unique vault password of at least 12 characters. Keep it for recovery."
            } else {
                "Unlock your browser vault, or use your desktop keyring."
            }
        } else {
            match kind {
                Some(Kind::Bitwarden) => {
                    "Use this account's master password and, if enabled, its authenticator code. Personal API key sign-in is available for other verification methods or authentication challenges."
                }
                None => "Authorize access in the connected password manager.",
            }
        };
        body = body.push(pref(
            if create {
                "Create your browser vault"
            } else {
                "Unlock a password destination"
            },
            "lock-keyhole",
            detail,
        ));
        if browser && state.vault_exists && state.settings.desktop_unlock {
            body = body.push(action(
                "Unlock with desktop keyring",
                "key-round",
                command(CredentialCommand::UnlockDesktop),
            ));
        }
        if browser || kind == Some(Kind::Bitwarden) {
            body = body.push(secret_field(
                if browser {
                    "Vault password"
                } else {
                    "Master password"
                },
                "Password",
                &state.master,
                true,
                "vault-master",
                CredentialCommand::Master,
            ));
        }
        if kind == Some(Kind::Bitwarden) {
            let options = vec!["Master password".to_owned(), "Personal API key".to_owned()];
            let selected = usize::from(state.api_key_mode);
            body = body.push(
                enzui_widgets::combo_box::enz_combo_box(
                    "Sign-in method",
                    options.clone(),
                    options.get(selected).cloned(),
                    |_| message(CredentialCommand::ApiKeyMode(false)),
                )
                .id_prefix("credential-bitwarden-auth-method")
                .selected_index(Some(selected))
                .on_select_index(|index| message(CredentialCommand::ApiKeyMode(index == 1)))
                .into_element(),
            );
            if state.api_key_mode {
                body = body.push(pref("Personal API key", "key-round", "Find your client_id and client_secret in Bitwarden's Account settings → Security → Keys. They authorize this connector; your master password decrypts the vault. These fields are not saved."))
                    .push(secret_field("Client ID", "user.…", &state.api_client_id, false, "vault-api-id", CredentialCommand::ApiClientId))
                    .push(secret_field("Client secret", "client_secret", &state.api_client_secret, true, "vault-api-secret", CredentialCommand::ApiClientSecret));
            } else {
                body = body.push(secret_field(
                    "Authenticator code (if required)",
                    "6-digit code",
                    &state.authenticator_code,
                    true,
                    "vault-authenticator",
                    CredentialCommand::AuthenticatorCode,
                ));
            }
        }
        if create {
            body = body.push(secret_field(
                "Confirm vault password",
                "Enter it again",
                &state.confirmation,
                true,
                "vault-confirmation",
                CredentialCommand::Confirmation,
            ));
        }
        body = body.push(primary(
            if state.busy {
                "Connecting…"
            } else if create {
                "Create vault"
            } else if system {
                "Unlock system keyring"
            } else {
                "Unlock / connect"
            },
            "lock-keyhole-open",
            command(CredentialCommand::Unlock),
        ));
        password_section("Access", body)
    }
    pub(super) fn credential_sources_view(&self) -> Element<'_, Message> {
        let state = &self.browser.credentials;
        let sources = destinations(&state.settings, state.vault_exists);
        let default = default_destination(&state.settings);
        let mut body = column![].spacing(10);
        for source in &sources {
            let account = state
                .settings
                .accounts
                .iter()
                .find(|account| account.id == source.id);
            let name = if source.id == "browser" {
                "Local vault"
            } else {
                &source.name
            };
            let subtitle = account.map_or_else(
                || {
                    if source.id == "browser" {
                        "Stored on this device".to_owned()
                    } else {
                        "Desktop keyring".to_owned()
                    }
                },
                |account| {
                    format!(
                        "{} · {}",
                        account.kind.label(),
                        if account.account.is_empty() {
                            "Not signed in"
                        } else {
                            &account.account
                        }
                    )
                },
            );
            let mut title = row![
                glyph(
                    if account.is_some() {
                        "key-round"
                    } else {
                        "hard-drive"
                    },
                    18.
                ),
                text(name.to_owned()).size(15)
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center);
            if source.id == default {
                title = title.push(
                    text("Default")
                        .size(12)
                        .color(self.look().palette.accent),
                );
            }
            let mut controls = row![].spacing(6);
            let index = sources
                .iter()
                .position(|entry| entry.id == source.id)
                .expect("source in list");
            controls = controls.push(action(
                "Logins",
                "key-round",
                command(CredentialCommand::ProviderLogins(index)),
            ));
            if source.writable && source.id != default {
                controls = controls.push(action(
                    "Make default",
                    "check",
                    command(CredentialCommand::ProviderDefault(source.id.clone())),
                ));
            }
            if account.is_none() {
                controls = controls.push(action(
                    "Settings",
                    "settings-2",
                    command(CredentialCommand::ProviderPreferences),
                ));
            }
            if let Some(account) = account {
                controls = controls.push(action(
                    "Settings",
                    "settings-2",
                    command(CredentialCommand::ConnectionEdit(account.id.clone())),
                ));
            }
            controls = controls.push(action(
                "Remove",
                "trash-2",
                command(CredentialCommand::ProviderRemove(source.id.clone())),
            ));
            let details = column![
                title,
                text(subtitle)
                    .size(12)
                    .color(self.look().palette.text_muted)
            ]
            .spacing(4)
            .width(Length::Fill);
            let entry: Element<'_, Message> = if self.content_width() >= 680. {
                row![details, controls]
                    .spacing(12)
                    .align_y(iced::Alignment::Center)
                    .into()
            } else {
                column![details, controls.wrap()].spacing(4).into()
            };
            body = body.push(entry);
            if state.removing_provider.as_deref() == Some(&source.id) {
                body = body.push(column![
                    text("Remove this provider from the browser? Saved passwords are kept in its vault.").size(13),
                    row![
                        action("Remove provider", "trash-2", command(CredentialCommand::ProviderRemove(source.id.clone()))),
                        action("Cancel", "x", command(CredentialCommand::Providers)),
                    ].spacing(8).wrap(),
                ].spacing(8));
            }
            body = body.push(iced::widget::rule::horizontal(1));
        }
        // Disabled connections remain editable and removable instead of disappearing.
        for account in state
            .settings
            .accounts
            .iter()
            .filter(|account| !account.enabled)
        {
            body = body.push(
                column![
                    text(format!("{} · Disabled", account.name)).size(15),
                    action(
                        "Settings / remove",
                        "settings-2",
                        command(CredentialCommand::ConnectionEdit(account.id.clone()))
                    ),
                    iced::widget::rule::horizontal(1),
                ]
                .spacing(6),
            );
        }
        let mut add = row![].spacing(6);
        if !state.settings.browser_store {
            add = add.push(action(
                "Add local vault",
                "hard-drive",
                command(CredentialCommand::AddLocal),
            ));
        }
        if state.settings.storage != LocalStorage::System && !state.settings.system_store {
            add = add.push(action(
                "Add system keyring",
                "hard-drive",
                command(CredentialCommand::Storage(1)),
            ));
        }
        for kind in Kind::ALL {
            add = add.push(action(
                format!("Add {}", kind.label()),
                "plus",
                command(CredentialCommand::ConnectionNew(kind)),
            ));
        }
        body = body.push(add.wrap());
        password_section("Providers", body)
    }
    pub(super) fn credential_connection_editor(&self) -> Element<'_, Message> {
        let state = &self.browser.credentials;
        let Some(editor) = &state.connection else {
            return Space::new().into();
        };
        let connection = &editor.connection;
        let field = |label: &'static str,
                     placeholder: &'static str,
                     value: String,
                     id: &'static str,
                     key: &'static str| {
            enzui_widgets::text_field::enz_text_field_owned(
                label,
                placeholder,
                value,
                move |value| message(CredentialCommand::ConnectionField(key, value)),
            )
            .id(iced::advanced::widget::Id::new(id))
            .into_element()
        };
        let mut body = column![
            row![
                action(
                    "Providers",
                    "arrow-left",
                    command(CredentialCommand::ConnectionCancel)
                ),
                primary(
                    "Save provider",
                    "check",
                    command(CredentialCommand::ConnectionSave)
                ),
            ]
            .spacing(8)
            .wrap(),
        ]
        .spacing(12);
        if connection.kind == Kind::Bitwarden {
            let options: Vec<String> = [
                "After 1 minute",
                "After 5 minutes",
                "After 15 minutes",
                "After 30 minutes",
                "After 1 hour",
                "On browser restart",
                "Remember with desktop keyring",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect();
            let selected = if connection.remember {
                6
            } else {
                match connection
                    .lock_minutes
                    .unwrap_or(state.settings.lock_minutes)
                {
                    1 => 0,
                    15 => 2,
                    30 => 3,
                    60 => 4,
                    0 => 5,
                    _ => 1,
                }
            };
            body = body.push(password_section("Locking", column![
                enzui_widgets::combo_box::enz_combo_box("Lock Bitwarden", options.clone(), Some(options[selected].clone()), |_| message(CredentialCommand::ConnectionTimeout(1)))
                    .id_prefix("bitwarden-lock-policy").selected_index(Some(selected))
                    .on_select_index(|index| message(CredentialCommand::ConnectionTimeout(index)))
                    .into_element(),
                text("Timed locking uses inactivity. Timed and restart locking require your master password to unlock again, including after browser restart.").size(13),
            ].spacing(8)));
        }
        body = body.push(password_section(
            "Account",
            column![
                field(
                    "Provider name",
                    "Personal or work",
                    connection.name.clone(),
                    "credential-connection-name",
                    "name"
                ),
                field(
                    "Account email",
                    "Account",
                    connection.account.clone(),
                    "credential-connection-account",
                    "account"
                ),
            ]
            .spacing(12),
        ));
        body = body.push(field(
            "Server (optional)",
            "https://vault.bitwarden.com",
            connection.server.clone(),
            "credential-connection-server",
            "server",
        ));
        for (label, key, value) in [
            ("Enable this connection", "enabled", connection.enabled),
            ("Advanced connector settings", "advanced", editor.advanced),
        ] {
            body = body.push(
                enzui_widgets::preferences_row::enz_preferences_row(label)
                    .switch(value, move |value| {
                        message(CredentialCommand::ConnectionToggle(key, value))
                    })
                    .into_element(),
            );
        }
        if editor.advanced {
            body = body.push(field(
                "Connector executable (optional)",
                "Bundled with enzim Browser",
                connection.executable.clone(),
                "credential-connector-executable",
                "executable",
            ));
            body = body.push(field(
                "Existing CLI profile directory (optional)",
                "Use an absolute path",
                connection
                    .directory
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                "credential-connector-directory",
                "directory",
            ));
            body = body.push(
                enzui_widgets::text_field::enz_text_field_owned(
                    "Connection CLI profile",
                    "",
                    connection.state_directory().to_string_lossy().into_owned(),
                    |value| {
                        message(CredentialCommand::ConnectionField(
                            "readonly-profile",
                            value,
                        ))
                    },
                )
                .read_only(true)
                .id(iced::advanced::widget::Id::new(
                    "credential-connection-profile",
                ))
                .into_element(),
            );
        }
        if !state.notice.is_empty() {
            body = body.push(text(&state.notice).size(13));
        }
        let mut actions = row![primary(
            "Save provider",
            "check",
            command(CredentialCommand::ConnectionSave)
        )]
        .spacing(8);
        if state
            .settings
            .accounts
            .iter()
            .any(|account| account.id == connection.id)
        {
            actions = actions.push(action(
                if editor.confirm_delete {
                    "Confirm removal"
                } else {
                    "Remove provider"
                },
                "unlink",
                command(CredentialCommand::ConnectionDelete),
            ));
        }
        body = body.push(actions.wrap());
        body = body.push(action(
            "Connector setup guide",
            "external-link",
            Command::Visit(connection.kind.documentation().into()),
        ));
        body.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_manager_survives_restart_and_opens_its_connection_without_a_vault() {
        for kind in Kind::ALL {
            let scratch = credentials::tests::Scratch::new();
            let settings = credentials::Settings {
                enabled: true,
                setup_provider: Some(kind),
                ..Default::default()
            };
            credentials::config::save(&scratch.path(), &settings).expect("persist pending setup");
            let restored =
                credentials::config::load(&scratch.path()).expect("reopen configuration");
            let mut app = crate::browser::browser_tests::app();
            app.browser.credentials.settings = restored;
            let _ = app.credential_command(CredentialCommand::OpenSite);
            assert_eq!(app.browser.library, Some(InternalPage::Passwords));
            let editor = app
                .browser
                .credentials
                .connection
                .as_ref()
                .expect("provider setup");
            assert_eq!(editor.connection.kind, kind);
            assert!(
                !app.browser.credentials.vault_exists,
                "no local vault is created"
            );
            assert!(
                app.browser.credentials.service.is_none(),
                "setup starts no unlock worker"
            );
        }
    }

    #[test]
    fn preferred_read_source_survives_restart_separately_from_the_save_destination() {
        let scratch = credentials::tests::Scratch::new();
        let connection = Connection::new(Kind::Bitwarden, "Read account").expect("connection");
        let settings = credentials::Settings {
            enabled: true,
            preferred_source: connection.id.clone(),
            save_to: "browser".into(),
            accounts: vec![connection.clone()],
            ..Default::default()
        };
        credentials::config::save(&scratch.path(), &settings).expect("persist destinations");
        let restored = credentials::config::load(&scratch.path()).expect("reopen destinations");
        assert_eq!(
            credentials::router::default_unlock_source(&restored),
            connection.id
        );
        assert_eq!(
            credentials::router::default_destination(&restored),
            "browser"
        );
        let mut invalid = restored;
        invalid.preferred_source = "missing-account".into();
        assert!(
            credentials::config::save(&scratch.path(), &invalid).is_err(),
            "unknown read destinations must be rejected"
        );
    }
}
