use super::*;
use iced::widget::{Space, column, container, row, text};

pub(crate) struct Cleanup {
    pub(super) id: u64,
    pub(super) host: RefCell<WebViewService>,
    pub(super) started: std::time::Instant,
}

impl App {
    pub(super) fn container_command(&mut self, command: ContainerCommand) -> Task<Message> {
        use ContainerCommand::*;
        let mut task = Task::none();
        match command {
            Menu(event) => {
                let restore = enzui_widgets::menu_button::focus_restore_operation(
                    &event,
                    Some(&iced::widget::Id::new("new-tab-container-menu")),
                )
                .map_or_else(Task::none, iced::advanced::widget::operate);
                if let Some(id) = enzui_widgets::menu_button::apply_event(
                    &mut self.browser.container_menu_model,
                    &mut self.browser.container_menu_state,
                    &event,
                ) {
                    let selected = id
                        .as_str()
                        .strip_prefix("container:")
                        .and_then(|v| v.parse::<u64>().ok());
                    self.browser.new_tab_container = selected
                        .filter(|id| self.browser.saved.containers.iter().any(|c| c.id == *id));
                    self.browser.container_menu_model = container_menu_model(
                        &self.browser.saved.containers,
                        self.browser.new_tab_container,
                        self.look().palette.is_dark(),
                    );
                    return focus("new-address");
                }
                return restore;
            }
            Open => self.open_internal_page(InternalPage::Containers),
            NewTab(id) => {
                if id.is_none()
                    || self
                        .browser
                        .saved
                        .containers
                        .iter()
                        .any(|c| Some(c.id) == id)
                {
                    self.new_tab_in_container(String::new(), id);
                    task = focus("start-search");
                }
            }
            Reopen(key, id) => {
                let tab = self
                    .browser
                    .saved
                    .tabs
                    .iter()
                    .find(|t| t.key == key)
                    .cloned();
                if let Some(tab) = tab
                    && (id.is_none()
                        || self
                            .browser
                            .saved
                            .containers
                            .iter()
                            .any(|c| Some(c.id) == id))
                {
                    self.new_tab_in_container(tab.url, id);
                }
            }
            Edit(id) => {
                self.browser.container_edit = id;
                let entry =
                    id.and_then(|id| self.browser.saved.containers.iter().find(|c| c.id == id));
                self.browser.container_name = entry.map_or_else(String::new, |c| c.name.clone());
                self.browser.container_color = entry.map_or(0, |c| c.color);
                self.browser.container_icon = entry
                    .and_then(|c| {
                        crate::profiles::ICONS
                            .iter()
                            .position(|(name, _)| *name == c.icon_name())
                    })
                    .unwrap_or(0);
                self.browser.container_policy =
                    entry.map_or_else(Default::default, |entry| entry.privacy.clone());
                self.browser.container_proxy_password = Default::default();
                self.browser.container_agent_custom = self
                    .browser
                    .container_policy
                    .user_agent
                    .as_ref()
                    .is_some_and(|value| crate::user_agent::Identity::detect(value).is_none());
                self.browser.container_notice.clear();
                if self.browser.library != Some(InternalPage::Containers) {
                    self.open_library(InternalPage::Containers);
                }
                self.browser.container_editor_open = true;
                self.browser.panel = Panel::None;
                return focus("container-name");
            }
            Back => {
                self.reset_library_scroll();
                self.browser.container_editor_open = false;
                self.browser.container_notice.clear();
                self.browser.panel = Panel::None;
            }
            Override(key, value) => {
                if let Some(value) = value {
                    self.browser
                        .container_policy
                        .controls
                        .insert(key.into(), value);
                } else {
                    self.browser.container_policy.controls.remove(key);
                }
            }
            #[cfg(test)]
            Agent(index) => {
                return self.container_command(Identity(crate::user_agent::Change::Browser(index)));
            }
            #[cfg(test)]
            AgentText(value) => {
                return self.container_command(Identity(crate::user_agent::Change::Custom(value)));
            }
            Identity(change) => {
                let focus_custom = matches!(change, crate::user_agent::Change::Browser(3));
                let inherit = matches!(change, crate::user_agent::Change::Browser(0));
                let (value, custom) = crate::user_agent::apply(
                    self.browser
                        .container_policy
                        .user_agent
                        .as_deref()
                        .unwrap_or_default(),
                    change,
                );
                self.browser.container_agent_custom = custom;
                self.browser.container_policy.user_agent = if inherit { None } else { Some(value) };
                if focus_custom {
                    return focus("container-user-agent-custom");
                }
            }
            Proxy(value) => {
                if value.len() <= 256 && !value.chars().any(char::is_control) {
                    self.browser.container_policy.upstream_proxy = value;
                }
            }
            DenyAgents(deny) => self.browser.container_policy.deny_agents = deny,
            ProxyUsername(value) => {
                if value.len() <= 255 && !value.chars().any(char::is_control) {
                    self.browser.container_policy.proxy_username = value;
                }
            }
            ProxyPassword(value) => {
                if value.expose().len() <= 255 && !value.expose().chars().any(char::is_control) {
                    self.browser.container_proxy_password = value;
                }
            }
            Name(name) => self.browser.container_name = name,
            Color(color) => self.browser.container_color = color % 8,
            Icon(index) => {
                self.browser.container_icon = index.min(crate::profiles::ICONS.len() - 1);
            }
            Save => {
                let parsed = match crate::container_privacy::parse_proxy(
                    &self.browser.container_policy.upstream_proxy,
                ) {
                    Ok(parsed) => parsed,
                    Err(error) => {
                        self.browser.container_notice = error;
                        return Task::none();
                    }
                };
                let username = self.browser.container_policy.proxy_username.trim().to_owned();
                if let Some((kind, _)) = parsed
                    && let Err(error) = crate::container_privacy::check_username(kind, &username)
                {
                    self.browser.container_notice = error;
                    return Task::none();
                }
                let proxy = parsed.map_or_else(String::new, crate::container_privacy::proxy_text);
                self.browser
                    .container_policy
                    .upstream_proxy
                    .clone_from(&proxy);
                // Without a proxy there is nothing to sign in to.
                self.browser.container_policy.proxy_username =
                    if proxy.is_empty() { String::new() } else { username };
                let route = crate::container_privacy::route(&self.browser.container_policy);
                let mut proxy_changed = self
                    .browser
                    .container_edit
                    .and_then(|id| self.browser.container_routes.get(&id))
                    .is_some_and(|applied| applied != &route);
                // The password goes to the desktop keyring, never to the
                // settings. A container being made has no record yet; its
                // password is kept once it has an id, below.
                let password = std::mem::take(&mut self.browser.container_proxy_password);
                if let Some(id) = self.browser.container_edit {
                    match self.keep_proxy_password(id, &password) {
                        Ok(changed) => proxy_changed |= changed,
                        Err(error) => {
                            self.browser.container_notice = error;
                            self.browser.container_proxy_password = password;
                            return Task::none();
                        }
                    }
                }
                let name = self.browser.container_name.trim();
                if name.is_empty() || name.chars().count() > 60 {
                    self.browser.container_notice =
                        "Use a name between 1 and 60 characters.".into();
                } else if let Some(id) = self.browser.container_edit {
                    if let Some(entry) = self
                        .browser
                        .saved
                        .containers
                        .iter_mut()
                        .find(|c| c.id == id)
                    {
                        entry.name = name.into();
                        entry.color = self.browser.container_color;
                        entry.icon = crate::profiles::ICONS[self.browser.container_icon].0.into();
                        entry.privacy = self.browser.container_policy.clone();
                        self.browser.container_editor_open = false;
                        self.browser.panel = Panel::None;
                    }
                } else {
                    match crate::profiles::Container::create(name, self.browser.container_color) {
                        Ok(mut entry) => {
                            entry.icon =
                                crate::profiles::ICONS[self.browser.container_icon].0.into();
                            entry.privacy = self.browser.container_policy.clone();
                            self.browser
                                .container_routes
                                .insert(entry.id, crate::container_privacy::route(&entry.privacy));
                            let id = entry.id;
                            self.browser.saved.containers.push(entry);
                            if let Err(error) = self.keep_proxy_password(id, &password) {
                                self.browser.toasts.error("container-proxy", error);
                            }
                            self.browser.container_editor_open = false;
                            self.browser.panel = Panel::None;
                        }
                        Err(error) => self.browser.container_notice = error,
                    }
                }
                if !self.browser.container_editor_open {
                    self.reset_library_scroll();
                    self.browser.container_notice.clear();
                    if proxy_changed {
                        self.browser.toasts.sticky("Privacy settings saved. Restart the browser to apply the new proxy route to this container.");
                    }
                    self.apply_privacy_to_hosts();
                    let reloads: Vec<_> = self
                        .browser
                        .saved
                        .tabs
                        .iter()
                        .enumerate()
                        .filter(|(_, tab)| {
                            self.browser.container_edit.is_some()
                                && tab.container == self.browser.container_edit
                                && InternalPage::from_url(&tab.url).is_none()
                        })
                        .filter(|(_, tab)| {
                            self.navigation_host(tab.key)
                                .is_some_and(|host| host.borrow().native.is_some())
                        })
                        .map(|(index, tab)| (tab.key, self.effective_privacy(index)))
                        .collect();
                    for (key, policy) in reloads {
                        self.prepare_policy_navigation(key, BrowserAction::Reload, policy);
                    }
                }
            }
            DeleteRequest(id) => {
                if self.browser.saved.containers.iter().any(|c| c.id == id) {
                    self.browser.container_edit = Some(id);
                    self.browser.panel = Panel::ContainerDelete;
                }
            }
            StorageRemoved(id, result) => self.finish_container_removal(id, result),
            DeleteConfirm => {
                if let Some(id) = self.browser.container_edit
                    && self.browser.panel == Panel::ContainerDelete
                    && !self
                        .browser
                        .maintenance
                        .iter()
                        .any(|cleanup| cleanup.id == id)
                {
                    self.clear_page_copies(Some(id));
                    self.retire_container_cookie_host(id);
                    let indices: Vec<_> = self
                        .browser
                        .saved
                        .tabs
                        .iter()
                        .enumerate()
                        .filter(|(_, t)| t.container == Some(id))
                        .map(|(i, _)| i)
                        .collect();
                    for index in indices.into_iter().rev() {
                        self.browser.saved.tabs[index].pinned = false;
                        let _ = self.browser_command(Command::Close(index));
                    }
                    for tab in &mut self.browser.saved.tabs {
                        if tab.container == Some(id) {
                            tab.container = None;
                        }
                    }
                    self.browser.closed.retain(|t| t.container != Some(id));
                    let profile = if self.browser.private {
                        self.browser
                            .private_profiles
                            .entry(Some(id))
                            .or_insert_with(enzui_webkit::web::WebProfile::ephemeral)
                            .clone()
                    } else {
                        let Some(entry) = self.browser.saved.containers.iter().find(|c| c.id == id)
                        else {
                            return Task::none();
                        };
                        enzui_webkit::web::WebProfile::persistent(entry.storage_name())
                    };
                    let profile = match self.routed_profile(Some(id), profile) {
                        Ok(profile) => profile,
                        Err(error) => {
                            self.browser.toasts.error("containers", error);
                            return Task::none();
                        }
                    };
                    let mut host = WebViewService::new();
                    host.profile = profile;
                    host.open_native();
                    host.hide_view();
                    if let (Some(native), Some(view)) = (&host.native, host.live_view)
                        && let Err(error) = native.browse(
                            &host.router,
                            view,
                            BrowserAction::ClearData(enzui_webkit::page::WebsiteData::ALL),
                        )
                    {
                        host.native_error = Some(error);
                    }
                    self.browser.maintenance.push(Cleanup {
                        id,
                        host: RefCell::new(host),
                        started: std::time::Instant::now(),
                    });
                    self.browser.toasts.info(
                        enzui_widgets::toast::ToastTone::Info,
                        "Removing container and its site data…",
                    );
                }
                self.browser.panel = Panel::None;
            }
        }
        if !cfg!(test) {
            self.browser.save();
        }
        task
    }

    pub(super) fn poll_maintenance(&mut self) {
        let mut done = Vec::new();
        for item in std::mem::take(&mut self.browser.maintenance) {
            let mut host = item.host.borrow_mut();
            host.ingest_native();
            let result = std::mem::take(&mut host.page_events)
                .into_iter()
                .find_map(|event| {
                    if let enzui_webkit::page::PageEvent::DataCleared { result, .. } = event {
                        Some(result)
                    } else {
                        None
                    }
                })
                .or_else(|| host.native_error.clone().map(Err))
                .or_else(|| {
                    (item.started.elapsed() > std::time::Duration::from_secs(30))
                        .then(|| Err("Site data cleanup did not finish. Try again.".into()))
                });
            drop(host);
            if let Some(result) = result {
                done.push((item.id, result));
            } else {
                self.browser.maintenance.push(item);
            }
        }
        for (id, result) in done {
            if result.is_ok() && !self.browser.private {
                let Some(name) = self
                    .browser
                    .saved
                    .containers
                    .iter()
                    .find(|container| container.id == id)
                    .map(crate::profiles::Container::storage_name)
                else {
                    continue;
                };
                self.pending_tasks.push(Task::perform(
                    crate::webview_native::remove_profile_storage(name),
                    move |result| {
                        c(Command::Container(ContainerCommand::StorageRemoved(
                            id, result,
                        )))
                    },
                ));
            } else {
                self.finish_container_removal(id, result);
            }
        }
    }

    fn finish_container_removal(&mut self, id: u64, result: Result<(), String>) {
        self.browser.chrome_revision = self.browser.chrome_revision.wrapping_add(1);
        match result {
            Ok(()) => {
                if let Some(profile) = self.browser.private_profiles.remove(&Some(id)) {
                    crate::webview_native::release_profiles(vec![profile.id()]);
                }
                self.browser.saved.containers.retain(|c| c.id != id);
                self.browser.container_routes.remove(&id);
                if self.browser.container_edit == Some(id) {
                    self.browser.container_editor_open = false;
                }
                self.cookie_container_removed(id);
                self.browser
                    .saved
                    .site_permissions
                    .retain(|choice| choice.container != Some(id));
                self.browser
                    .saved
                    .local_sites
                    .retain(|site| site.container != id);
                self.clear_site_icons();
                self.browser.toasts.done("Container and site data removed.");
                self.browser.save();
            }
            Err(error) => self.browser.toasts.error(
                "containers",
                format!("Container data could not be removed: {error}"),
            ),
        }
    }

    pub(super) fn address_identity(&self) -> Element<'_, Message> {
        if let Some(page) = self.browser.library {
            return container(glyph(page.icon(), 16.))
                .center_x(28)
                .center_y(24)
                .into();
        }
        let mut identity = row![self.pop("Site information", Panel::Security)]
            .spacing(4)
            .align_y(iced::Alignment::Center);
        if self.browser.private {
            identity = identity.push(enzui_widgets::tooltip::FadeTooltip::new(
                container(glyph("venetian-mask", 16.0))
                    .width(24)
                    .height(24)
                    .center_y(24)
                    .into(),
                "Private browsing",
                iced::widget::tooltip::Position::Bottom,
            ));
        }
        let id = self.browser.saved.tabs[self.browser.saved.active].container;
        if let Some(entry) = self
            .browser
            .saved
            .containers
            .iter()
            .find(|c| Some(c.id) == id)
        {
            let mark = self.container_mark(entry, 14.);
            let badge: Element<'_, Message> = if self.browser.viewport.width < 720.0 {
                container(mark).width(16).center_y(24).into()
            } else {
                row![
                    mark,
                    text(entry.name.clone())
                        .size(11)
                        .wrapping(iced::widget::text::Wrapping::None)
                        .ellipsis(iced::widget::text::Ellipsis::End)
                ]
                .spacing(5)
                .align_y(iced::Alignment::Center)
                .width(Length::Fixed(88.0))
                .into()
            };
            identity = identity.push(enzui_widgets::tooltip::FadeTooltip::new(
                badge,
                format!("{} container", entry.name),
                iced::widget::tooltip::Position::Bottom,
            ));
        }
        identity.into()
    }

    pub(super) fn container_selector(&self) -> Element<'_, Message> {
        let entry = self
            .browser
            .saved
            .containers
            .iter()
            .find(|c| Some(c.id) == self.browser.new_tab_container);
        let kind = enzui_widgets::menu_button::MenuButtonKind::Quiet(
            enzui_widgets::button::ButtonSize::Sm,
        );
        let mut tokens = enzui_widgets::menu_button::MenuButtonTokens::from_palette_kind(
            &self.look().palette,
            kind,
        );
        if let Some(entry) = entry {
            tokens.foreground = group_ink(entry.color, self.look().palette.is_dark());
        }
        enzui_widgets::menu_button::enz_menu_button(
            "",
            &self.browser.container_menu_model,
            &self.browser.container_menu_state,
            |e| c(Command::Container(ContainerCommand::Menu(e))),
        )
        .kind(kind)
        .tokens(tokens)
        .trigger_icon(entry.map_or("container", |c| c.icon_name()))
        .aria_label(format!(
            "Container: {}",
            entry.map_or("Default", |c| c.name.as_str())
        ))
        .menu_id(iced::widget::Id::new("new-tab-container-menu"))
        .side(enzui_core::placement::Side::Top)
        .popup_width(230.)
        .into_element()
    }
    pub(super) fn container_mark(
        &self,
        entry: &crate::profiles::Container,
        size: f32,
    ) -> Element<'static, Message> {
        enzui_widgets::icon::enz_icon(entry.icon_name())
            .size(size)
            .tint(group_ink(entry.color, self.look().palette.is_dark()))
            .into_element()
    }

    pub(super) fn containers_page(&self) -> Element<'_, Message> {
        if self.browser.container_editor_open {
            return self.page_document(
                if self.browser.container_edit.is_some() {
                    "Container settings"
                } else {
                    "New container"
                },
                "container",
                "Changes apply to this container; its open pages reload when saved.",
                self.container_editor(false),
            );
        }
        let mut body = column![
            text("Each container keeps its own cookies, sign-ins and website storage.").size(14),
        ]
        .spacing(6)
        .width(Length::Fill);
        for entry in &self.browser.saved.containers {
            let controls = row![
                icon(
                    "plus",
                    "New tab in this container",
                    c(Command::Container(ContainerCommand::NewTab(Some(entry.id)))),
                ),
                icon(
                    "pencil",
                    "Edit container",
                    c(Command::Container(ContainerCommand::Edit(Some(entry.id)))),
                ),
                icon(
                    "trash-2",
                    "Remove container and site data",
                    c(Command::Container(ContainerCommand::DeleteRequest(
                        entry.id
                    ))),
                ),
            ]
            .spacing(4);
            body = body.push(
                container(
                    enzui_widgets::hover_group::enz_hover_group(
                        row![
                            enzui_widgets::focus_button::focus_button(
                                row![
                                    self.container_mark(entry, 18.),
                                    text(entry.name.clone()).size(14)
                                ]
                                .spacing(10)
                                .align_y(iced::Alignment::Center)
                                .width(Length::Fill),
                                c(Command::Container(ContainerCommand::NewTab(Some(entry.id)))),
                            )
                            .width(Length::Fill)
                            .height(36)
                            .padding([0, 8])
                            .semantics(
                                enzui_core::semantics::Semantics::new(
                                    enzui_core::semantics::Role::Button
                                )
                                .label(format!("New tab in {}", entry.name))
                            )
                            .style_focus(|theme, _| {
                                let p = palette(theme);
                                iced::widget::button::Style {
                                    background: None,
                                    text_color: p.text,
                                    border: iced::Border {
                                        radius: 8.into(),
                                        ..Default::default()
                                    },
                                    ..Default::default()
                                }
                            }),
                            controls
                        ]
                        .spacing(12)
                        .align_y(iced::Alignment::Center),
                    )
                    .width(Length::Fill)
                    .style(
                        |theme, hovered| enzui_widgets::hover_group::HoverGroupStyle {
                            background: if hovered {
                                palette(theme).surface_hover
                            } else {
                                iced::Color::TRANSPARENT
                            },
                            border_color: iced::Color::TRANSPARENT,
                            border_width: 0.,
                            radius: 8.,
                        },
                    )
                    .into_element(),
                )
                .padding([2, 0]),
            );
        }
        self.page_document_with_action(
            "Containers",
            "container",
            if self.browser.private {
                "Temporary isolated spaces in this private window"
            } else {
                "Keep your online lives separate"
            },
            body.into(),
            Some(primary(
                "Create container",
                "plus",
                Command::Container(ContainerCommand::Edit(None)),
            )),
        )
    }

    fn container_icon_picker(&self) -> Element<'_, Message> {
        let mut rows = column![].spacing(6);
        for batch in crate::profiles::ICONS.chunks(6).enumerate() {
            let mut line = row![].spacing(6);
            for (offset, (name, label)) in batch.1.iter().enumerate() {
                let index = batch.0 * 6 + offset;
                line = line.push(
                    enzui_widgets::toolbar_icon::enz_toolbar_icon(
                        *name,
                        c(Command::Container(ContainerCommand::Icon(index))),
                    )
                    .size(36., 18.)
                    .aria_label(*label)
                    .tooltip(*label)
                    .active(index == self.browser.container_icon)
                    .into_element(),
                );
            }
            rows = rows.push(line);
        }
        rows.into()
    }
    pub(super) fn container_editor(&self, delete: bool) -> Element<'_, Message> {
        let body: Element<'_, Message> = if delete {
            let name = self
                .browser
                .container_edit
                .and_then(|id| self.browser.saved.containers.iter().find(|c| c.id == id))
                .map_or("this container", |c| c.name.as_str());
            column![
                heading("trash-2","Remove container",name),
                text("Its open tabs will close and its cookies and website storage will be removed. Web history can be managed separately in History.").size(14),
                row![Space::new().width(Length::Fill),action("Cancel","x",Command::Dismiss),
                    enzui_widgets::button::enz_button("Remove",c(Command::Container(ContainerCommand::DeleteConfirm))).icon("trash-2").kind(enzui_widgets::button::ButtonKind::Destructive).into_element()
                ].spacing(10)
            ].spacing(20).into()
        } else {
            let mut colors = row![].spacing(6);
            for i in 0..8 {
                let color = group_ink(i, self.look().palette.is_dark());
                let selected = i == self.browser.container_color;
                let ink = self.look().palette.text;
                let swatch = container(dot(color, 18.0))
                    .center(Length::Fixed(28.0))
                    .style(move |_| container::Style {
                        border: iced::Border {
                            radius: 14.0.into(),
                            color: if selected {
                                ink
                            } else {
                                iced::Color::TRANSPARENT
                            },
                            width: 2.0,
                        },
                        ..Default::default()
                    });
                colors = colors.push(
                    enzui_widgets::focus_button::focus_button(
                        swatch,
                        c(Command::Container(ContainerCommand::Color(i))),
                    )
                    .width(30)
                    .height(30)
                    .padding(0)
                    .style_focus(|_, _| iced::widget::button::Style::default()),
                );
            }
            let mut body = column![
                action(
                    "All containers",
                    "arrow-left",
                    Command::Container(ContainerCommand::Back)
                ),
                text("Name").size(13),
                field(
                    "Name",
                    &self.browser.container_name,
                    |name| c(Command::Container(ContainerCommand::Name(name))),
                    "container-name"
                ),
                colors,
                self.container_icon_picker(),
                self.container_privacy_controls(),
            ]
            .spacing(16);
            if !self.browser.container_notice.is_empty() {
                body = body.push(text(self.browser.container_notice.clone()).size(13));
            }
            body.push(
                row![
                    Space::new().width(Length::Fill),
                    action("Cancel", "x", Command::Container(ContainerCommand::Back)),
                    primary("Save", "check", Command::Container(ContainerCommand::Save))
                ]
                .spacing(10),
            )
            .into()
        };
        if !delete {
            return body;
        }
        container(
            enzui_widgets::scroll_view::enz_scrollable(container(body).width(Length::Fill))
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .padding(20)
        .height((self.browser.viewport.height - 32.).min(if delete { 260. } else { 360. }))
        .width((self.browser.viewport.width - 32.0).min(440.0))
        .style(|theme| {
            let p = palette(theme);
            container::Style {
                background: Some(p.surface_elevated.into()),
                border: iced::Border {
                    color: p.border,
                    width: 1.0,
                    radius: 14.0.into(),
                },
                ..Default::default()
            }
        })
        .into()
    }
}

pub(crate) fn container_menu_model(
    entries: &[crate::profiles::Container],
    selected: Option<u64>,
    dark: bool,
) -> enzui_core::menu_model::MenuModel {
    use enzui_core::menu_model::{CheckState, MenuEntryModel, MenuModel};
    let mut rows = vec![
        MenuEntryModel::command("default", "Default container")
            .icon("container")
            .radio(true)
            .check(if selected.is_none() {
                CheckState::Checked
            } else {
                CheckState::Unchecked
            }),
    ];
    rows.extend(entries.iter().map(|entry| {
        MenuEntryModel::command(format!("container:{}", entry.id), entry.name.clone())
            .icon(entry.icon_name())
            .icon_tint(group_ink(entry.color, dark))
            .radio(true)
            .check(if Some(entry.id) == selected {
                CheckState::Checked
            } else {
                CheckState::Unchecked
            })
    }));
    MenuModel::new(rows)
}

impl App {
    fn container_privacy_controls(&self) -> Element<'_, Message> {
        let draft = &self.browser.container_policy;
        let mut identity = column![crate::user_agent::picker(
            draft.user_agent.as_deref().unwrap_or_default(),
            self.browser.container_agent_custom,
            true,
            "container-user-agent",
            |change| c(Command::Container(ContainerCommand::Identity(change)))
        )]
        .spacing(8);
        let mut proxy = column![
            text("Proxy").size(13),
            field(
                "Direct connection, 127.0.0.1:8080 or socks5://127.0.0.1:1080",
                &draft.upstream_proxy,
                |value| c(Command::Container(ContainerCommand::Proxy(value))),
                "container-proxy"
            ),
        ]
        .spacing(6);
        if !draft.upstream_proxy.trim().is_empty() {
            // Only a password this session has read or saved is known to
            // be there; the keyring is not asked while the editor draws.
            let saved_password = !draft.proxy_username.is_empty()
                && self.browser.container_proxy_password.is_empty()
                && self
                    .browser
                    .container_edit
                    .is_some_and(|id| self.browser.proxy_passwords.borrow().contains_key(&id));
            proxy = proxy.push(
                row![
                    field(
                        "Username (optional)",
                        &draft.proxy_username,
                        |value| c(Command::Container(ContainerCommand::ProxyUsername(value))),
                        "container-proxy-username"
                    ),
                    enzui_widgets::text_field::enz_text_field(
                        "",
                        if saved_password {
                            "Saved in the keyring"
                        } else {
                            "Password"
                        },
                        self.browser.container_proxy_password.expose(),
                        |value| c(Command::Container(ContainerCommand::ProxyPassword(
                            crate::credentials::Secret::new(value)
                        ))),
                    )
                    .id(iced::advanced::widget::Id::new("container-proxy-password"))
                    .secure(true)
                    .into_element(),
                ]
                .spacing(8),
            );
        }
        proxy = proxy.push(text("An HTTP or SOCKS5 proxy at a numeric address, with a username and password if it asks for them; the password is kept in your desktop keyring, and an empty password field keeps the one saved. DNS is resolved locally; WebRTC is disabled for proxied containers. Restart the browser after changing an existing container’s route.").size(12));
        identity = identity.push(proxy);
        let mut controls = column![
            text("Inherit follows browser defaults. Site choices stay within this container.")
                .size(12)
        ]
        .spacing(4);
        for (key, title, symbol) in super::privacy_settings::CONTROLS.into_iter().chain([
            ("tracking", "Tracking prevention", "scan-eye"),
            ("cookies", "Third-party cookies", "cookie"),
        ]) {
            if key == "drm" && !crate::privacy::encrypted_media_available() {
                controls = controls.push(
                    enzui_widgets::preferences_row::enz_preferences_row(title)
                        .icon(symbol)
                        .subtitle("Encrypted media is unavailable in this runtime.")
                        .enabled(false)
                        .into_element(),
                );
                continue;
            }
            let route_blocked = key == "webrtc"
                && (!draft.upstream_proxy.trim().is_empty()
                    || self.container_uses_proxy(self.browser.container_edit));
            if route_blocked {
                controls = controls.push(
                    enzui_widgets::preferences_row::enz_preferences_row(title)
                        .icon(symbol)
                        .subtitle(
                            "Unavailable with a proxy; real-time socket routing is not supported.",
                        )
                        .enabled(false)
                        .into_element(),
                );
                continue;
            }
            let inherited = self.browser.saved.privacy.value(key);
            let labels = vec![
                format!("Inherit ({})", if inherited { "On" } else { "Off" }),
                "On".into(),
                "Off".into(),
            ];
            let selected = draft
                .controls
                .get(key)
                .map_or(0, |value| if *value { 1 } else { 2 });
            let select = enzui_widgets::combo_box::enz_combo_box(
                "",
                labels.clone(),
                Some(labels[selected].clone()),
                move |_| c(Command::Container(ContainerCommand::Override(key, None))),
            )
            .id_prefix(format!("container-privacy-{key}"))
            .aria_label(format!("Container {title}"))
            .selected_index(Some(selected))
            .on_select_index(move |index| {
                c(Command::Container(ContainerCommand::Override(
                    key,
                    match index {
                        1 => Some(true),
                        2 => Some(false),
                        _ => None,
                    },
                )))
            })
            .width(160)
            .into_element();
            controls = controls.push(
                enzui_widgets::preferences_row::enz_preferences_row(title)
                    .icon(symbol)
                    .control(select)
                    .accessory_width(160.)
                    .into_element(),
            );
        }
        let agents = enzui_widgets::preferences_row::enz_preferences_row("Allow agents")
            .icon("bot")
            .subtitle("Let the assistant and agents connected through the agent center see and use this container’s tabs and open pages in it.")
            .switch(!draft.deny_agents, |allow| {
                c(Command::Container(ContainerCommand::DenyAgents(!allow)))
            })
            .into_element();
        column![
            section("Website identity", identity),
            section("Privacy", controls),
            section("Agents", column![agents]),
        ]
        .spacing(16)
        .into()
    }
}

impl App {
    /// Keep a container's proxy `password` in the desktop keyring, when one
    /// was typed, or forget the one kept when the container no longer signs
    /// in. Whether this changes a password already in use this session,
    /// which then applies after a restart.
    fn keep_proxy_password(
        &mut self,
        id: u64,
        password: &crate::credentials::Secret,
    ) -> Result<bool, String> {
        let Some(entry) = self.browser.saved.containers.iter().find(|c| c.id == id) else {
            return Ok(false);
        };
        let mut record = crate::credentials::ProxyPassword::for_container(&entry.storage_name());
        let signs_in = !self.browser.container_policy.proxy_username.is_empty();
        if !signs_in {
            // Best effort: a password nothing uses any more is removed.
            let _ = record.forget();
            return Ok(false);
        }
        if password.is_empty() {
            return Ok(false);
        }
        if !record.store(password) {
            return Err("The desktop keyring did not keep the proxy password. Unlock it and save again.".into());
        }
        let mut cache = self.browser.proxy_passwords.borrow_mut();
        match cache.get(&id) {
            Some(applied) => Ok(applied != password),
            None => {
                cache.insert(id, password.clone());
                Ok(false)
            }
        }
    }

    pub(crate) fn routed_profile(
        &self,
        container: Option<u64>,
        profile: enzui_webkit::web::WebProfile,
    ) -> Result<enzui_webkit::web::WebProfile, String> {
        let route=match container {
            None=>"",
            Some(id)=>self.browser.container_routes.get(&id).map(String::as_str).ok_or("This container has no applied network route. Restart the browser before opening it.")?,
        };
        use enzui_webkit::destination_guard::{ProxyKind, UpstreamProxy};
        let Some(((kind, address), username)) = crate::container_privacy::parse_route(route)?
        else {
            return Ok(profile);
        };
        let proxy = match kind {
            ProxyKind::HttpConnect => UpstreamProxy::http_connect(address),
            ProxyKind::Socks5 => UpstreamProxy::socks5(address),
        };
        let (Some(id), Some(username)) = (container, username) else {
            return Ok(profile.with_upstream_proxy(proxy));
        };
        // The password is read from the desktop keyring once a session and
        // kept, so every tab of the container signs in alike.
        let mut cache = self.browser.proxy_passwords.borrow_mut();
        let password = match cache.entry(id) {
            std::collections::btree_map::Entry::Occupied(known) => known.into_mut(),
            std::collections::btree_map::Entry::Vacant(unknown) => {
                let storage = self
                    .browser
                    .saved
                    .containers
                    .iter()
                    .find(|c| c.id == id)
                    .map(crate::profiles::Container::storage_name)
                    .ok_or("This container no longer exists.")?;
                let password = crate::credentials::ProxyPassword::for_container(&storage)
                    .read()?
                    .ok_or("The proxy password is not in the desktop keyring. Enter it again in the container's settings.")?;
                unknown.insert(password)
            }
        };
        let proxy = proxy
            .with_credentials(username, password.expose())
            .map_err(|error| format!("The proxy sign-in cannot be used: {error}."))?;
        Ok(profile.with_upstream_proxy(proxy))
    }
}
