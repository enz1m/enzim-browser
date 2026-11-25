use crate::{App, Message, webview_native};
use enzui_widgets::title_bar_gesture::WindowCommand;
use iced::{Element, Subscription, Task, window};
use std::collections::BTreeMap;
#[cfg(feature = "center")]
mod agent;
#[cfg(feature = "center")]
mod agent_functions;

/// The browser's functions, which the assistant also offers its model.
#[cfg(feature = "center")]
pub(crate) fn agent_registry()
-> Result<enzui_agent::center::AppFunctionRegistry<agent_functions::Request>, String> {
    agent_functions::registry()
}
#[derive(Clone, Debug)]
#[allow(
    clippy::large_enum_variant,
    reason = "Window-routed input remains by value to avoid allocating for every event."
)]
pub(crate) enum Event {
    Profile(crate::browser_profile::Event),
    Launched,
    App(window::Id, Message),
    Opened,
    Close(window::Id),
    NativeReady,
    NotificationsRetired,
    A11y(enzui_core::app_runtime::AccessibilityWake),
    #[cfg(feature = "center")]
    Agent(agent::AgentEvent),
}
pub(crate) struct Windows {
    retiring_notifications: usize,
    chooser: Option<crate::browser_profile::Chooser>,
    instance: Option<enzui_widgets::single_instance::SingleInstance>,
    apps: BTreeMap<window::Id, App>,
    /// What each window allows its containers to reach locally; profiles
    /// are shared between windows, so each window's sync includes the others'.
    local_claims: BTreeMap<window::Id, Vec<crate::browser::local::Claim>>,
    preferences: serde_json::Value,
    /// The connection to the enzUI agent center, when the center provisioned
    /// the browser.
    #[cfg(feature = "center")]
    agent: Option<agent::Agent>,
}
impl Windows {
    fn share_local_claims(&mut self) {
        for (id, app) in &mut self.apps {
            app.browser.local_elsewhere = self
                .local_claims
                .iter()
                .filter(|(other, _)| *other != id)
                .flat_map(|(_, claims)| claims.iter().cloned())
                .collect();
            app.sync_local_origins();
        }
    }
}
fn open() -> (window::Id, Task<Event>) {
    open_sized(iced::Size::new(1400., 900.))
}
fn open_sized(size: iced::Size) -> (window::Id, Task<Event>) {
    open_with(size, iced::Size::new(420., 360.), None)
}
/// `parent` makes the window a child of another: desktops keep it above that
/// window, and tiling desktops float it at its own size.
fn open_with(
    size: iced::Size,
    min_size: iced::Size,
    parent: Option<window::Id>,
) -> (window::Id, Task<Event>) {
    #[allow(
        clippy::expect_used,
        reason = "The application ID is a valid compile-time desktop basename."
    )]
    let identity =
        enzui_widgets::app_shell::DesktopApplicationIdentity::new(crate::desktop::APP_ID)
            .expect("valid static desktop identity");
    let settings = window::Settings {
        size,
        min_size: Some(min_size),
        parent,
        exit_on_close_request: false,
        decorations: false,
        transparent: true,
        ..enzui_widgets::app_shell::enz_window_settings(
            enzui_widgets::app_shell::ShellLayout::ContentOnly,
        )
    };
    let (id, task) = window::open(identity.apply_to_window_settings(settings));
    (id, task.map(|_| Event::Opened))
}
impl Windows {
    pub(crate) fn boot() -> (Self, Task<Event>) {
        if crate::browser_profile::chooser_requested() {
            let (id, opened) = open_sized(iced::Size::new(720., 600.));
            let (chooser, task) = crate::browser_profile::Chooser::new(id);
            return (
                Self {
                    retiring_notifications: 0,
                    chooser: Some(chooser),
                    instance: None,
                    apps: BTreeMap::new(),
                    local_claims: BTreeMap::new(),
                    preferences: serde_json::Value::Null,
                    #[cfg(feature = "center")]
                    agent: None,
                },
                Task::batch([opened, task.map(Event::Profile)]),
            );
        }
        let (mut app, task) = App::boot();
        let (id, opened) = if app.welcome.is_some() {
            open_sized(iced::Size::new(760., 640.))
        } else {
            open()
        };
        app.window_id = Some(id);
        (
            Self {
                retiring_notifications: 0,
                chooser: None,
                instance: crate::browser_profile::take_instance(),
                preferences: preferences(&app.browser.saved),
                apps: std::iter::once((id, app)).collect(),
                local_claims: BTreeMap::new(),
                #[cfg(feature = "center")]
                agent: agent::Agent::start(),
            },
            Task::batch([opened, task.map(move |message| Event::App(id, message))]),
        )
    }
    pub(crate) fn update(&mut self, event: Event) -> Task<Event> {
        let sync = match &event {
            Event::NativeReady => self
                .apps
                .values()
                .any(|app| !app.browser.maintenance.is_empty()),
            Event::App(_, Message::Chrome(command)) => !matches!(
                command,
                crate::browser::Command::Pointer(_)
                    | crate::browser::Command::Pdf(_)
                    | crate::browser::Command::Zoom(_)
                    | crate::browser::Command::Input(
                        crate::browser_input::InputCommand::Bounds(..)
                            | crate::browser_input::InputCommand::Start { .. }
                            | crate::browser_input::InputCommand::Cancel
                            | crate::browser_input::InputCommand::Click(_)
                            | crate::browser_input::InputCommand::Release
                            | crate::browser_input::InputCommand::Tick(_)
                    )
                    | crate::browser::Command::Viewport(_)
                    | crate::browser::Command::Dev(_)
                    | crate::browser::Command::Inspect(_)
                    | crate::browser::Command::Notifications(
                        crate::website_notifications::Command::Wake
                    )
                    | crate::browser::Command::ChooserWake
            ),
            Event::App(
                _,
                Message::Assistant(
                    crate::assistant::Msg::Set(_)
                    | crate::assistant::Msg::SignInProgress(crate::assistant::chatgpt::Progress::Done(Ok(_)))
                    | crate::assistant::Msg::SignOut
                    | crate::assistant::Msg::Models(_, Ok(_))
                    | crate::assistant::Msg::ServiceSaved(Ok(_))
                    | crate::assistant::Msg::RemoveService(_)
                    | crate::assistant::Msg::Pick(..)
                    | crate::assistant::Msg::SavePrompt
                    | crate::assistant::Msg::Worker(_, crate::assistant::worker::Event::Memory(_))
                    | crate::assistant::Msg::Enrolled(Ok(())),
                ),
            ) => true,
            _ => false,
        };
        if let Event::App(
            id,
            Message::Chrome(crate::browser::Command::Container(
                crate::browser::ContainerCommand::DeleteConfirm,
            )),
        ) = &event
            && let Some(source) = self.apps.get(id)
            && !source.browser.private
            && let Some(container) = source.browser.container_edit
        {
            for (other, app) in &mut self.apps {
                if other != id && !app.browser.private {
                    close_container_tabs(app, container);
                }
            }
        }
        let imported_history = match &event {
            Event::App(
                id,
                Message::Chrome(crate::browser::Command::Import(
                    crate::browser::import_page::ImportCommand::Applied(
                        generation,
                        _,
                        options,
                        Ok(_),
                    ),
                )),
            ) => {
                options.history
                    && self.apps.get(id).is_some_and(|app| {
                        !app.browser.private && app.browser.importing.generation == *generation
                    })
            }
            _ => false,
        };
        let credential_update = match &event {
            Event::App(
                _,
                Message::Chrome(crate::browser::Command::Credential(
                    crate::browser::credentials_ui::CredentialCommand::Configured(
                        _,
                        settings,
                        Ok(publication),
                    ),
                )),
            ) => Some((settings.clone(), publication.identity)),
            _ => None,
        };
        let task = self.handle(event);
        if let Some((settings, identity)) = credential_update {
            for app in self.apps.values_mut() {
                app.credential_preferences_changed(&settings, identity);
            }
        }
        if imported_history {
            for app in self.apps.values_mut().filter(|app| !app.browser.private) {
                if let Ok(history) = app.browser.history_store.query("", 0) {
                    app.browser.saved.history = history;
                }
            }
        }
        if sync {
            self.sync_preferences();
        }
        let mut tasks = vec![task];
        // A page that opened a popup window, from a message or from its
        // worker's report, gets the window now.
        let popups: Vec<_> = self
            .apps
            .iter_mut()
            .filter_map(|(id, app)| {
                app.popup_out
                    .take()
                    .map(|popup| (*id, app.browser.private, popup))
            })
            .collect();
        for (parent, private, popup) in popups {
            tasks.push(self.open_window(parent, private, None, None, Some(popup)));
        }
        for (id, app) in &mut self.apps {
            let id = *id;
            tasks.extend(
                std::mem::take(&mut app.pending_tasks)
                    .into_iter()
                    .map(move |task| task.map(move |message| Event::App(id, message))),
            );
        }
        Task::batch(tasks)
    }
    fn handle(&mut self, event: Event) -> Task<Event> {
        match event {
            Event::Profile(event) => {
                let Some(chooser) = self.chooser.as_mut() else {
                    return Task::none();
                };
                match event {
                    crate::browser_profile::Event::Theme(event) => {
                        return chooser.theme.update(*event).map(|event| {
                            Event::Profile(crate::browser_profile::Event::Theme(Box::new(event)))
                        });
                    }
                    crate::browser_profile::Event::Window(WindowCommand::Close) => {
                        return iced::exit();
                    }
                    crate::browser_profile::Event::Window(command) => {
                        return command.into_task(chooser.window);
                    }
                    event => match chooser.update(event) {
                        Ok(Some(entry)) => {
                            match crate::browser_profile::activate(entry) {
                                Ok(true) => return iced::exit(),
                                Ok(false) => {}
                                Err(error) => {
                                    chooser.notice = error;
                                    return Task::none();
                                }
                            }
                            self.instance = crate::browser_profile::take_instance();
                            let id = chooser.window;
                            let (mut app, task) = App::boot();
                            app.window_id = Some(id);
                            self.preferences = preferences(&app.browser.saved);
                            self.apps.insert(id, app);
                            self.chooser = None;
                            #[cfg(feature = "center")]
                            {
                                self.agent = agent::Agent::start();
                            }
                            return Task::batch([
                                task.map(move |message| Event::App(id, message)),
                                window::size(id).map(move |size| {
                                    Event::App(
                                        id,
                                        Message::Chrome(crate::browser::Command::Viewport(size)),
                                    )
                                }),
                            ]);
                        }
                        Ok(None) => {}
                        Err(error) => chooser.notice = error,
                    },
                }
                Task::none()
            }
            Event::Opened => Task::none(),
            #[cfg(feature = "center")]
            Event::Agent(event) => self.agent_event(event),
            Event::NotificationsRetired => {
                self.retiring_notifications = self.retiring_notifications.saturating_sub(1);
                if self.apps.is_empty() && self.retiring_notifications == 0 {
                    iced::exit()
                } else {
                    Task::none()
                }
            }
            Event::Close(id) => {
                let regular = self.apps.get(&id).is_some_and(|app| !app.browser.private);
                let mut retire = Task::none();
                if let Some(mut app) = self.apps.remove(&id) {
                    app.browser.save();
                    if let Some(shutdown) = app.browser.desktop_notifications.shutdown() {
                        self.retiring_notifications += 1;
                        retire = shutdown.map(|()| Event::NotificationsRetired);
                    }
                    // A popup window shares its opener's private storage,
                    // which goes with the opener's window.
                    if app.browser.private && !app.popup_window {
                        let ids = app
                            .browser
                            .private_profiles
                            .values()
                            .map(enzui_webkit::web::WebProfile::id)
                            .collect();
                        crate::webview_native::release_profiles(ids);
                    }
                }
                if regular
                    && let Some(app) = self.apps.values_mut().find(|app| !app.browser.private)
                {
                    app.browser.save();
                }
                if self.local_claims.remove(&id).is_some() {
                    self.share_local_claims();
                }
                #[cfg(feature = "center")]
                if self.apps.is_empty()
                    && let Some(agent) = self.agent.take()
                {
                    agent.shutdown();
                }
                let close = if self.apps.is_empty() && self.retiring_notifications == 0 {
                    iced::exit()
                } else {
                    let single = self.apps.len() == 1;
                    for app in self.apps.values_mut() {
                        app.allow_quiet = single;
                    }
                    window::close(id)
                };
                Task::batch([close, retire])
            }
            Event::App(
                id,
                Message::Chrome(
                    crate::browser::Command::Exit
                    | crate::browser::Command::Window(WindowCommand::Close),
                ),
            ) => self.update(Event::Close(id)),
            Event::App(id, message) => {
                let completed_welcome =
                    matches!(
                        &message,
                        Message::Chrome(crate::browser::Command::Welcome(
                            crate::welcome::Command::Completed(Ok(()))
                        ))
                    ) && self.apps.get(&id).is_some_and(|app| app.welcome.is_some());
                if completed_welcome {
                    return self.finish_welcome_window(id, message);
                }
                let Some(app) = self.apps.get_mut(&id) else {
                    return Task::none();
                };
                let task = app
                    .update(message)
                    .map(move |message| Event::App(id, message));
                let claims = app.local_claims();
                if self.local_claims.get(&id) != Some(&claims) {
                    self.local_claims.insert(id, claims);
                    self.share_local_claims();
                }
                let Some(app) = self.apps.get_mut(&id) else {
                    return task;
                };
                let calls = std::mem::take(&mut app.assistant.calls);
                let new_window = app.new_window.take();
                let moved = app.moved_out.take();
                let private = app.browser.private;
                let calls = if calls.is_empty() {
                    Task::none()
                } else {
                    self.run_assistant_calls(id, calls)
                };
                if let Some((private, url)) = new_window {
                    return Task::batch([task, calls, self.open_window(id, private, url, None, None)]);
                }
                if let Some(moved) = moved {
                    return Task::batch([task, calls, self.open_window(id, private, None, Some(moved), None)]);
                }
                Task::batch([task, calls])
            }
            Event::Launched => {
                let Some(instance) = self.instance.as_mut() else {
                    return Task::none();
                };
                let (intents, raise) = instance.drain(None);
                let mut tasks = vec![raise];
                let mut visits: BTreeMap<window::Id, Vec<String>> = BTreeMap::new();
                for request in intents
                    .into_iter()
                    .flat_map(crate::browser_profile::forwarded)
                {
                    let target = self
                        .apps
                        .iter()
                        .find(|(_, app)| !app.browser.private)
                        .map(|(id, _)| *id);
                    let parent = target.or_else(|| self.apps.keys().next().copied());
                    let Some(parent) = parent else {
                        continue;
                    };
                    if request.new_window || request.private || target.is_none() {
                        tasks.push(self.open_window(parent, request.private, request.url, None, None));
                    } else if let Some(url) = request.url {
                        visits.entry(parent).or_default().push(url);
                    }
                }
                for (parent, urls) in visits {
                    if let Some(app) = self.apps.get_mut(&parent) {
                        tasks.push(
                            app.open_external_urls(urls)
                                .map(move |message| Event::App(parent, message)),
                        );
                    }
                }
                Task::batch(tasks)
            }
            Event::NativeReady => Task::batch(self.apps.iter_mut().map(|(id, app)| {
                let id = *id;
                app.update(Message::WebViewNativeReady)
                    .map(move |message| Event::App(id, message))
            })),
            Event::A11y(wake) => {
                let actions =
                    enzui_core::app_runtime::drain_accessibility_wake(wake).unwrap_or_default();
                Task::batch(actions.iter().flat_map(|action| {
                    let id = action.window();
                    if self
                        .chooser
                        .as_ref()
                        .is_some_and(|chooser| chooser.window == id)
                    {
                        return vec![
                            enzui_widgets::a11y_actions::apply_accessibility_action(action)
                                .map(Event::Profile),
                        ];
                    }
                    crate::message::a11y_action_tasks(action)
                        .into_iter()
                        .map(move |task| task.map(move |message| Event::App(id, message)))
                        .collect::<Vec<_>>()
                }))
            }
        }
    }
    fn finish_welcome_window(&mut self, previous: window::Id, message: Message) -> Task<Event> {
        let Some(mut app) = self.apps.remove(&previous) else {
            return Task::none();
        };
        let (id, opened) = open();
        // Wayland owns placement; a fresh normal window avoids growing beyond the work area.
        app.window_id = Some(id);
        let task = app
            .update(message)
            .map(move |message| Event::App(id, message));
        let prepare = app
            .browser_command(crate::browser::Command::Viewport(iced::Size::new(
                1400., 900.,
            )))
            .map(move |message| Event::App(id, message));
        let opened = opened.chain(window::size(id).map(move |size| {
            Event::App(id, Message::Chrome(crate::browser::Command::Viewport(size)))
        }));
        self.apps.insert(id, app);
        Task::batch([opened, prepare, task, window::close(previous)])
    }
    fn open_window(
        &mut self,
        parent: window::Id,
        private: bool,
        url: Option<String>,
        moved: Option<crate::browser::tab_move::MovedTab>,
        popup: Option<crate::browser::popups::PopupWindow>,
    ) -> Task<Event> {
        let Some(source) = self.apps.get(&parent) else {
            return Task::none();
        };
        let preferences = source.browser.saved.clone();
        let parent_private = source.browser.private;
        let source_profiles = source.browser.private_profiles.clone();
        let applied_routes = source.browser.container_routes.clone();
        let (theme, theme_task) = enzui_core::theme_file::integration::ThemeIntegration::boot();
        let mut next = App::with_browser(
            theme.theme().clone(),
            crate::browser::Browser::load_with_privacy(private),
        );
        next.theme = theme;
        if !parent_private || private {
            copy_preferences(&preferences, &mut next.browser.saved);
            next.browser.container_routes = applied_routes;
        }
        if private {
            next.browser.saved.site_permissions.clear();
            next.browser.saved.local_sites.clear();
        }
        let url = url
            .map(|url| next.browser.saved.search.resolve(&url))
            .unwrap_or_default();
        next.browser.library = None;
        next.browser.saved.tabs = vec![crate::browser::TabInfo {
            key: crate::browser::next_tab_key(),
            favicon: None,
            title: if url.is_empty() {
                "New tab".into()
            } else {
                url.clone()
            },
            url: url.clone(),
            pinned: false,
            group: None,
            container: None,
        }];
        next.browser.saved.active = 0;
        next.browser.sleeping = vec![None];
        next.browser_mode = true;
        next.address = url;
        next.browser.saved.groups.clear();
        let mut navigation = if next.address.is_empty() {
            crate::browser::focus("start-search")
        } else {
            Task::done(Message::BrowserGo)
        };
        if let Some(moved) = moved {
            // The tab comes with its page: nothing is loaded for it.
            next.adopt_only_tab(moved);
            navigation = Task::none();
        }
        let size = popup
            .as_ref()
            .map(|popup| crate::browser::popups::popup_window_size(popup.size));
        if let Some(popup) = popup {
            // A popup shares its opener's storage, a private one included,
            // or a sign-in would not reach the page that asked for it.
            next.browser
                .private_profiles
                .clone_from(&source_profiles);
            next.take_popup_window(popup);
            navigation = Task::none();
        }
        let (new_id, opened) = match size {
            Some(size) => open_with(size, iced::Size::new(320., 260.), Some(parent)),
            None => open(),
        };
        next.window_id = Some(new_id);
        self.apps.insert(new_id, next);
        for app in self.apps.values_mut() {
            app.allow_quiet = false;
        }
        Task::batch([
            opened,
            theme_task.map(move |event| Event::App(new_id, Message::Theme(event))),
            navigation.map(move |message| Event::App(new_id, message)),
        ])
    }
    fn sync_preferences(&mut self) {
        let changed = self
            .apps
            .values()
            .find(|app| !app.browser.private && preferences(&app.browser.saved) != self.preferences)
            .map(|app| app.browser.saved.clone());
        if let Some(source) = changed {
            self.preferences = preferences(&source);
            for app in self.apps.values_mut().filter(|app| !app.browser.private) {
                let removed: Vec<_> = app
                    .browser
                    .saved
                    .containers
                    .iter()
                    .filter(|container| !source.containers.iter().any(|c| c.id == container.id))
                    .map(|c| c.id)
                    .collect();
                copy_preferences(&source, &mut app.browser.saved);
                if !app.browser.saved.search.suggestions {
                    app.browser.remote_search.clear();
                }
                for container in &source.containers {
                    app.browser
                        .container_routes
                        .entry(container.id)
                        .or_insert_with(|| crate::container_privacy::route(&container.privacy));
                }
                for container in removed {
                    app.browser.container_routes.remove(&container);
                    close_container_tabs(app, container);
                    app.cookie_container_removed(container);
                }
                for (index, host) in std::iter::once((app.browser.saved.active, &app.webview))
                    .chain(
                        app.browser
                            .sleeping
                            .iter()
                            .enumerate()
                            .filter_map(|(index, tab)| tab.as_ref().map(|tab| (index, &tab.host))),
                    )
                {
                    let host = host.borrow();
                    if let (Some(native), Some(view)) = (&host.native, host.live_view) {
                        let _ = native.browse(
                            &host.router,
                            view,
                            crate::webview_native::BrowserAction::Privacy(Box::new(
                                app.effective_privacy(index),
                            )),
                        );
                        let container = app.browser.saved.tabs[index].container;
                        let choices = app
                            .browser
                            .saved
                            .site_permissions
                            .iter()
                            .filter(|c| c.container == container)
                            .cloned()
                            .collect();
                        let _ = native.browse(
                            &host.router,
                            view,
                            crate::webview_native::BrowserAction::PermissionChoices(choices),
                        );
                        let _ = native.browse(
                            &host.router,
                            view,
                            crate::webview_native::BrowserAction::NotificationsEnabled(
                                app.browser.saved.notifications
                                    == crate::website_notifications::Policy::Ask,
                            ),
                        );
                    }
                }
            }
        }
    }
    pub(crate) fn view(&self, id: window::Id) -> Element<'_, Event> {
        if let Some(chooser) = &self.chooser
            && chooser.window == id
        {
            return chooser.view().map(Event::Profile);
        }
        if let Some(app) = self.apps.get(&id) {
            app.view().map(move |message| Event::App(id, message))
        } else {
            iced::widget::Space::new().into()
        }
    }
    pub(crate) fn title(&self, id: window::Id) -> String {
        if self
            .chooser
            .as_ref()
            .is_some_and(|chooser| chooser.window == id)
        {
            return "Choose a profile — enzim Browser".into();
        }
        self.apps.get(&id).map_or_else(String::new, App::title)
    }
    pub(crate) fn theme(&self, id: window::Id) -> iced::Theme {
        if let Some(chooser) = &self.chooser
            && chooser.window == id
        {
            return chooser.theme.iced_theme();
        }
        self.apps
            .get(&id)
            .map_or_else(|| enzui_theme::EnzTheme::dark().iced_theme(), App::theme)
    }
    pub(crate) fn subscription(&self) -> Subscription<Event> {
        Subscription::batch(
            self.apps
                .iter()
                .map(|(id, app)| {
                    app.subscription()
                        .with(*id)
                        .map(|(id, message)| Event::App(id, message))
                })
                .chain([
                    self.chooser
                        .as_ref()
                        .map_or_else(Subscription::none, |chooser| {
                            chooser.theme.subscription().map(|event| {
                                Event::Profile(crate::browser_profile::Event::Theme(Box::new(
                                    event,
                                )))
                            })
                        }),
                    self.instance
                        .as_ref()
                        .map_or_else(Subscription::none, |instance| {
                            instance.subscription(|_| Event::Launched)
                        }),
                    window::close_requests().map(Event::Close),
                    webview_native::notification_subscription().map(|()| Event::NativeReady),
                    webview_native::main_thread_subscription().map(|()| Event::NativeReady),
                    enzui_core::app_runtime::accessibility_wake_subscription(Event::A11y),
                    self.agent_subscription(),
                ]),
        )
    }
}

impl Windows {
    #[cfg(feature = "center")]
    fn agent_subscription(&self) -> Subscription<Event> {
        self.agent
            .as_ref()
            .map_or_else(Subscription::none, agent::Agent::subscription)
    }
    #[cfg(not(feature = "center"))]
    #[allow(clippy::unused_self, reason = "the same call with the agent center left out")]
    fn agent_subscription(&self) -> Subscription<Event> {
        Subscription::none()
    }
    #[cfg(feature = "center")]
    fn run_assistant_calls(
        &mut self,
        id: window::Id,
        calls: Vec<(String, String, serde_json::Value)>,
    ) -> Task<Event> {
        self.assistant_calls(id, calls)
    }
    /// Without the browser's functions every call fails.
    #[cfg(not(feature = "center"))]
    #[allow(clippy::unused_self, reason = "the same call with the agent center left out")]
    fn run_assistant_calls(
        &mut self,
        id: window::Id,
        calls: Vec<(String, String, serde_json::Value)>,
    ) -> Task<Event> {
        Task::batch(calls.into_iter().map(move |(call, _, _)| {
            Task::done(Event::App(
                id,
                Message::Assistant(crate::assistant::Msg::ToolResult(
                    call,
                    Err("This build has no page functions.".into()),
                )),
            ))
        }))
    }
}

fn preferences(saved: &crate::browser::Saved) -> serde_json::Value {
    serde_json::json!({"startup":saved.startup,"startup_page":saved.startup_page,"middle_autoscroll":saved.middle_autoscroll,"layout":saved.layout,"right":saved.right,"bookmarks":saved.bookmarks,"bookmark_folders":saved.bookmark_folders,"containers":saved.containers,"search":saved.search,"privacy":saved.privacy,"permissions":saved.site_permissions,"notifications":saved.notifications,"local_sites":saved.local_sites,"site_zoom":saved.site_zoom,"download_folder":saved.download_folder,"ask_download_place":saved.ask_download_place,"bookmarks_bar":saved.bookmarks_bar,"immersive":saved.immersive,"immersive_style":saved.immersive_style,"spell_check":saved.spell_check,"spell_languages":saved.spell_languages,"assistant":saved.assistant,"memory_mode":saved.memory_mode})
}
fn copy_preferences(source: &crate::browser::Saved, target: &mut crate::browser::Saved) {
    target.local_sites.clone_from(&source.local_sites);
    target.middle_autoscroll = source.middle_autoscroll;
    target.notifications = source.notifications;
    target.startup = source.startup;
    target.startup_page.clone_from(&source.startup_page);
    target.layout = source.layout;
    target.right = source.right;
    target.bookmarks.clone_from(&source.bookmarks);
    target.bookmark_folders.clone_from(&source.bookmark_folders);
    target.containers.clone_from(&source.containers);
    target.search.clone_from(&source.search);
    target.privacy.clone_from(&source.privacy);
    target.site_permissions.clone_from(&source.site_permissions);
    target.site_zoom.clone_from(&source.site_zoom);
    target.download_folder.clone_from(&source.download_folder);
    target.ask_download_place = source.ask_download_place;
    target.bookmarks_bar = source.bookmarks_bar;
    target.immersive = source.immersive;
    target.assistant.clone_from(&source.assistant);
    target.immersive_style = source.immersive_style;
    target.spell_check = source.spell_check;
    target.memory_mode = source.memory_mode;
    target.spell_languages.clone_from(&source.spell_languages);
}

fn close_container_tabs(app: &mut App, container: u64) {
    let indices: Vec<_> = app
        .browser
        .saved
        .tabs
        .iter()
        .enumerate()
        .filter(|(_, tab)| tab.container == Some(container))
        .map(|(index, _)| index)
        .collect();
    for index in indices.into_iter().rev() {
        app.browser.saved.tabs[index].pinned = false;
        drop(app.browser_command(crate::browser::Command::Close(index)));
    }
    if app.browser.saved.tabs.len() == 1 && app.browser.saved.tabs[0].url.is_empty() {
        app.browser.saved.tabs[0].container = None;
    }
    app.browser
        .closed
        .retain(|tab| tab.container != Some(container));
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn windows() -> (Windows, window::Id, window::Id) {
        let a = window::Id::unique();
        let b = window::Id::unique();
        let first = crate::browser::browser_tests::app();
        let mut second = crate::browser::browser_tests::app();
        second.browser.saved = first.browser.saved.clone();
        second
            .browser
            .container_routes
            .clone_from(&first.browser.container_routes);
        let prefs = preferences(&first.browser.saved);
        let mut apps = BTreeMap::new();
        apps.insert(a, first);
        apps.insert(b, second);
        (
            Windows {
                retiring_notifications: 0,
                chooser: None,
                instance: None,
                apps,
                local_claims: BTreeMap::new(),
                preferences: prefs,
                #[cfg(feature = "center")]
                agent: None,
            },
            a,
            b,
        )
    }
    #[test]
    fn welcome_success_replaces_only_its_window_and_keeps_preferences() {
        let (mut windows, a, b) = windows();
        let app = windows.apps.get_mut(&a).expect("welcome window");
        app.window_id = Some(a);
        app.browser.saved.search.selected = "google".into();
        app.welcome = Some(crate::welcome::State::new(
            &app.browser.saved,
            &app.browser.credentials.settings,
        ));
        let _ = windows.update(Event::App(
            a,
            Message::Chrome(crate::browser::Command::Welcome(
                crate::welcome::Command::Completed(Ok(())),
            )),
        ));
        assert!(
            !windows.apps.contains_key(&a),
            "the welcome native window is retired"
        );
        assert!(windows.apps.contains_key(&b), "other windows stay open");
        assert_eq!(windows.apps.len(), 2);
        let (id, app) = windows
            .apps
            .iter()
            .find(|(id, _)| **id != b)
            .expect("normal browser window");
        assert_eq!(app.window_id, Some(*id));
        assert!(app.welcome.is_none());
        assert_eq!(app.address, "https://browser.enzim.dev/");
        assert_eq!(app.browser.saved.search.selected, "google");
    }
    #[test]
    fn titlebar_close_removes_window_state_and_last_window_exits() {
        use crate::browser::{Command as C, c};
        let (mut windows, a, b) = windows();
        let urls = windows.apps[&b]
            .browser
            .saved
            .tabs
            .iter()
            .map(|t| t.url.clone())
            .collect::<Vec<_>>();
        let _ = windows.update(Event::App(a, c(C::Window(WindowCommand::Close))));
        assert!(!windows.apps.contains_key(&a));
        assert!(windows.apps[&b].allow_quiet);
        assert_eq!(
            windows.apps[&b]
                .browser
                .saved
                .tabs
                .iter()
                .map(|t| t.url.clone())
                .collect::<Vec<_>>(),
            urls
        );
        let _ = windows.update(Event::App(b, c(C::Window(WindowCommand::Close))));
        assert!(windows.apps.is_empty());
    }
    #[test]
    fn imported_history_and_bookmark_paths_reach_regular_windows_but_not_private_windows() {
        use crate::{
            browser::{Command, import_page::ImportCommand},
            imports::{Bookmark, Family, Options, Preview, Source},
        };
        use std::sync::Arc;
        let root = std::env::temp_dir().join(format!(
            "enzim-import-windows-{}-{}",
            std::process::id(),
            crate::browser::next_tab_key()
        ));
        enzui_security::secure_dir::ensure_private_dir(&root).expect("private fixture");
        let path = root.join("history.sqlite");
        let (mut windows, first, second) = windows();
        for app in windows.apps.values_mut() {
            app.browser.history_store = crate::history::Store::open(Some(&path));
        }
        let private = window::Id::unique();
        let mut app = crate::browser::browser_tests::app();
        app.browser.private = true;
        windows.apps.insert(private, app);
        let visits = vec![crate::history::Visit {
            id: 0,
            url: "https://example.org/imported".into(),
            title: "Imported page".into(),
            visited_at: 1_700_000_000_123,
            container: None,
        }];
        assert_eq!(
            crate::history::Store::open(Some(&path))
                .import_visits(&visits)
                .expect("history worker"),
            1
        );
        let data = Arc::new(Preview {
            source: Source {
                family: Family::Firefox,
                browser: "Firefox".into(),
                name: "Work".into(),
                path: "/unused".into(),
            },
            bookmarks: vec![Bookmark {
                url: "https://example.org/imported".into(),
                title: "Imported bookmark".into(),
                folders: vec!["Research".into()],
            }],
            history: visits,
            engines: Vec::new(),
            notes: Vec::new(),
        });
        windows
            .apps
            .get_mut(&first)
            .expect("source")
            .browser
            .importing
            .generation = 3;
        let _ = windows.update(Event::App(
            first,
            Message::Chrome(Command::Import(ImportCommand::Applied(
                3,
                data,
                Options::default(),
                Ok(1),
            ))),
        ));
        for id in [first, second] {
            let app = &windows.apps[&id];
            assert_eq!(app.browser.saved.history.len(), 1);
            assert_eq!(app.browser.saved.bookmarks.len(), 1);
            assert_eq!(
                app.browser.saved.bookmark_folders["https://example.org/imported"],
                ["Imported from Firefox · Work", "Research"]
            );
        }
        assert!(windows.apps[&private].browser.saved.history.is_empty());
        assert!(windows.apps[&private].browser.saved.bookmarks.is_empty());
        drop(windows);
        std::fs::remove_dir_all(root).expect("cleanup");
    }
    #[test]
    fn shared_preferences_keep_applied_routes_and_import_routes_for_new_containers() {
        let (mut windows, a, b) = windows();
        let id = windows.apps[&a].browser.saved.containers[0].id;
        for app in windows.apps.values_mut() {
            app.browser
                .container_routes
                .insert(id, "127.0.0.1:8080".into());
        }
        windows
            .apps
            .get_mut(&a)
            .expect("source")
            .browser
            .saved
            .containers[0]
            .privacy
            .upstream_proxy = "127.0.0.1:9090".into();
        windows.sync_preferences();
        assert_eq!(
            windows.apps[&b].browser.saved.containers[0]
                .privacy
                .upstream_proxy,
            "127.0.0.1:9090"
        );
        assert_eq!(
            windows.apps[&b].browser.container_routes[&id],
            "127.0.0.1:8080"
        );
        let mut added = crate::profiles::Container::create("Proxied", 1).expect("container");
        added.privacy.upstream_proxy = "127.0.0.1:8181".into();
        let added_id = added.id;
        windows
            .apps
            .get_mut(&a)
            .expect("source")
            .browser
            .saved
            .containers
            .push(added);
        windows.sync_preferences();
        assert_eq!(
            windows.apps[&b].browser.container_routes[&added_id],
            "127.0.0.1:8181"
        );
    }
}
