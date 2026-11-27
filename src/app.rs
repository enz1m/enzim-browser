use std::cell::RefCell;

use enzui_theme::EnzTheme;
use iced::{Element, Subscription, Task};

use crate::{Message, webview, webview_native};

pub(crate) struct App {
    pub(crate) welcome: Option<crate::welcome::State>,
    pub(crate) window_id: Option<iced::window::Id>,
    pub(crate) window_scale: Option<f32>,
    /// A window to open: whether it is private, and the address it opens on.
    pub(crate) new_window: Option<(bool, Option<String>)>,
    /// A tab to open a window around, with its page as it is.
    pub(crate) moved_out: Option<crate::browser::tab_move::MovedTab>,
    /// A window a page opened as a popup, to open as a small window of its own.
    pub(crate) popup_out: Option<crate::browser::popups::PopupWindow>,
    /// This window is a popup a page opened: no tabs, and the address only shown.
    pub(crate) popup_window: bool,
    /// The assistant's panel; its chat exists once the panel first opened.
    pub(crate) assistant: crate::assistant::Panel,
    pub(crate) allow_quiet: bool,
    #[cfg(feature = "benchmark")]
    pub(crate) input_sync_fast: bool,
    pub(crate) webview: RefCell<webview::WebViewService>,
    pub(crate) theme: enzui_core::theme_file::integration::ThemeIntegration,
    pub(crate) pending_tasks: Vec<Task<Message>>,
    pub(crate) local_files: crate::desktop_files::Sources,
    pub(crate) pending_pdf: Option<std::path::PathBuf>,
    pub(crate) browser: crate::browser::Browser,
    pub(crate) download_activity: crate::download_activity::DownloadActivity,
    pub(crate) renderer_ready: bool,
    pub(crate) web_system_font_ready: bool,
    pub(crate) web_system_font: Option<String>,
    pub(crate) pending_native_open: bool,
    pub(crate) prepared_new_tab: Option<crate::prepared_tab::PreparedTab>,
    pub(crate) prepared_new_tab_due: Option<std::time::Instant>,
    pub(crate) address_preparation_until: Option<std::time::Instant>,
    pub(crate) browser_mode: bool,
    pub(crate) address: String,
    pub(crate) address_dirty: bool,
    pub(crate) last_location: Option<String>,
    pub(crate) native_dispatch_publication: std::cell::Cell<bool>,
    pub(crate) pending_native_traversal: std::cell::Cell<Option<enzui_core::focus::Traversal>>,
    pub(crate) pending_navigation: Option<crate::webview_native::BrowserAction>,
}

impl App {
    pub(crate) fn set_window_scale(&mut self, scale: f32) {
        if !scale.is_finite() || scale <= 0.0 || self.window_scale == Some(scale) {
            return;
        }
        self.window_scale = Some(scale);
    }

    pub(crate) fn new(theme: EnzTheme) -> Self {
        Self::with_browser(theme, crate::browser::Browser::load())
    }
    pub(crate) fn with_browser(theme: EnzTheme, browser: crate::browser::Browser) -> Self {
        Self {
            welcome: None,
            window_id: None,
            window_scale: None,
            new_window: None,
            moved_out: None,
            popup_out: None,
            popup_window: false,
            assistant: crate::assistant::Panel::default(),
            allow_quiet: true,
            #[cfg(feature = "benchmark")]
            input_sync_fast: std::env::var_os("ENZIM_BENCH_INPUT_SYNC_FULL").is_none(),
            webview: RefCell::new(webview::WebViewService::new()),
            theme: enzui_core::theme_file::integration::ThemeIntegration::unmanaged_compiled_theme(
                theme,
            )
            .0,
            browser,
            download_activity: crate::download_activity::DownloadActivity::default(),
            pending_tasks: Vec::new(),
            pending_pdf: None,
            local_files: crate::desktop_files::Sources::default(),
            renderer_ready: false,
            web_system_font_ready: false,
            web_system_font: None,
            pending_native_open: false,
            prepared_new_tab: None,
            prepared_new_tab_due: None,
            address_preparation_until: None,
            browser_mode: false,
            address: "https://en.wikipedia.org/wiki/Rust_(programming_language)".into(),
            pending_navigation: None,
            native_dispatch_publication: std::cell::Cell::new(false),
            pending_native_traversal: std::cell::Cell::new(None),
            address_dirty: false,
            last_location: None,
        }
    }

    pub(crate) fn boot() -> (Self, Task<Message>) {
        let (theme, theme_task) = enzui_core::theme_file::integration::ThemeIntegration::boot();
        let mut app = Self::new(theme.theme().clone());
        app.theme = theme;
        app.browser_mode = true;
        app.address
            .clone_from(&app.browser.saved.tabs[app.browser.saved.active].url);
        let args: Vec<String> = std::env::args().collect();
        if args.iter().any(|a| a == "--new-window") {
            app.browser.library = None;
            app.browser.saved.tabs = vec![crate::browser::TabInfo {
                key: crate::browser::next_tab_key(),
                favicon: None,
                page_copy: None,
                url: String::new(),
                title: "New tab".into(),
                pinned: false,
                group: None,
                container: None,
            }];
            app.browser.saved.active = 0;
            app.browser.sleeping = vec![None];
            app.address.clear();
        }
        if !app.browser.private
            && !args.iter().any(|arg| arg == "--skip-welcome")
            && app.browser.saved.welcome_version == Some(0)
        {
            app.welcome = Some(crate::welcome::State::new(
                &app.browser.saved,
                &app.browser.credentials.settings,
            ));
        }
        app.open_launch_urls(crate::browser_profile::launch_urls(&args));
        if let Some(page) = app.browser.library {
            app.address = page.url().into();
            app.webview.borrow_mut().hide_view();
            app.refresh_library_menu();
            if !cfg!(test) {
                app.browser.save();
            }
        }
        let nav = if app.welcome.is_some() || app.browser.library.is_some() {
            Task::none()
        } else if app.address.is_empty() {
            crate::browser::focus("start-search")
        } else {
            Task::done(Message::BrowserGo)
        };
        let credentials = Task::done(Message::Chrome(crate::browser::Command::Credential(
            crate::browser::credentials_ui::CredentialCommand::Initialize,
        )));
        app.publish_look();
        app.appearance_own_read();
        let corners = app.appearance_corners();
        (
            app,
            Task::batch([theme_task.map(Message::Theme), nav, credentials, corners]),
        )
    }

    pub(crate) fn open_launch_urls(&mut self, urls: Vec<String>) {
        if let Some(welcome) = self.welcome.as_mut() {
            welcome.has_launch_urls |= !urls.is_empty();
        }
        for url in urls {
            if let Some(page) = crate::browser::InternalPage::from_url(&url) {
                self.browser.library = Some(page);
            } else {
                crate::startup::open_launch(&mut self.browser.saved, &url);
                self.browser
                    .sleeping
                    .resize_with(self.browser.saved.tabs.len(), || None);
                self.browser.library = None;
            }
            self.address = url;
        }
    }

    pub(crate) fn theme(&self) -> iced::Theme {
        self.look_iced()
    }

    pub(crate) fn subscription(&self) -> Subscription<Message> {
        let shortcuts = if self.browser_mode {
            iced::event::listen_with(|event, status, window| {
                if status == iced::event::Status::Ignored
                    && let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                        key,
                        modifiers,
                        ..
                    }) = &event
                    && !modifiers.alt()
                    && !modifiers.control()
                {
                    let action = match key.as_ref() {
                        iced::keyboard::Key::Named(
                            iced::keyboard::key::Named::PageDown
                            | iced::keyboard::key::Named::ArrowRight,
                        ) => Some(crate::browser_pdf::PdfCommand::Step(1)),
                        iced::keyboard::Key::Named(
                            iced::keyboard::key::Named::PageUp
                            | iced::keyboard::key::Named::ArrowLeft,
                        ) => Some(crate::browser_pdf::PdfCommand::Step(-1)),
                        iced::keyboard::Key::Named(iced::keyboard::key::Named::Home) => {
                            Some(crate::browser_pdf::PdfCommand::First)
                        }
                        iced::keyboard::Key::Named(iced::keyboard::key::Named::End) => {
                            Some(crate::browser_pdf::PdfCommand::Last)
                        }
                        _ => None,
                    };
                    if let Some(action) = action {
                        return Some((
                            window,
                            Message::Chrome(crate::browser::Command::Pdf(action)),
                        ));
                    }
                    // For a page's list of choices; dropped below unless
                    // one is open.
                    use crate::browser::prompts::PromptCommand as P;
                    let list = match key.as_ref() {
                        iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowDown) => {
                            Some(P::ListMove(1))
                        }
                        iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowUp) => {
                            Some(P::ListMove(-1))
                        }
                        iced::keyboard::Key::Character(text) => {
                            let mut letters = text.chars();
                            letters
                                .next()
                                .filter(|_| letters.next().is_none())
                                .map(P::ListType)
                        }
                        _ => None,
                    };
                    if let Some(list) = list {
                        return Some((
                            window,
                            Message::Chrome(crate::browser::Command::Prompt(list)),
                        ));
                    }
                }

                browser_shortcut(event).map(|message| (window, message))
            })
            .with((
                self.window_id,
                self.browser.chooser.is_fallback_open(),
                self.browser.container_menu_state.is_open()
                    || self.browser.menu_state.is_open()
                    || self.browser.zoom_menu_state.is_open()
                    || self.browser.credentials.inline_open(),
                self.browser.address_suggestions_open,
                self.welcome.is_some(),
                self.browser.options.is_some(),
            ))
            .filter_map(
                |((target, modal, menu, address_suggestions, welcome, list), (window, message))| {
                    (target == Some(window)
                        && (list
                            || !matches!(
                                message,
                                Message::Chrome(crate::browser::Command::Prompt(
                                    crate::browser::prompts::PromptCommand::ListMove(_)
                                        | crate::browser::prompts::PromptCommand::ListType(_)
                                ))
                            ))
                        && (!welcome
                            || matches!(
                                message,
                                Message::Chrome(crate::browser::Command::Viewport(_))
                            ))
                        && !((menu || address_suggestions)
                            && matches!(message, Message::Chrome(crate::browser::Command::Escape)))
                        && (!modal
                            || matches!(
                                message,
                                Message::Chrome(
                                    crate::browser::Command::Viewport(_)
                                        | crate::browser::Command::Pointer(_)
                                )
                            )))
                    .then_some(message)
                },
            )
        } else {
            Subscription::none()
        };
        let sound = if self.browser_mode && self.sound_animates() {
            iced::time::every(crate::browser::sound::BEAT)
                .map(|_| Message::Chrome(crate::browser::Command::SoundBeat))
        } else {
            Subscription::none()
        };
        let maintenance = if self.browser.maintenance.is_empty() {
            Subscription::none()
        } else {
            iced::time::every(std::time::Duration::from_millis(200))
                .map(|_| Message::Chrome(crate::browser::Command::MaintenancePoll))
        };
        let autoscroll = if self.browser.input.anchor.is_some() {
            Subscription::batch([
                iced::time::every(std::time::Duration::from_millis(16)).map(|time| {
                    Message::Chrome(crate::browser::Command::Input(
                        crate::browser_input::InputCommand::Tick(time),
                    ))
                }),
                iced::event::listen_with(|event, _, _| {
                    let command = match event {
                        iced::Event::Mouse(iced::mouse::Event::ButtonPressed(button)) => {
                            crate::browser_input::InputCommand::Click(button)
                        }
                        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { .. })
                        | iced::Event::Window(iced::window::Event::Unfocused) => {
                            crate::browser_input::InputCommand::Cancel
                        }
                        _ => return None,
                    };
                    Some(Message::Chrome(crate::browser::Command::Input(command)))
                }),
            ])
        } else {
            Subscription::none()
        };
        let devtools_resize = if self.browser.devtools.resizing {
            iced::event::listen_with(|event, _, window| match event {
                iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
                    iced::mouse::Button::Left,
                ))
                | iced::Event::Window(iced::window::Event::Unfocused) => Some((
                    window,
                    Message::Chrome(crate::browser::Command::Dev(
                        crate::browser::devtools::DevCommand::ResizeEnd,
                    )),
                )),
                _ => None,
            })
            .with(self.window_id)
            .filter_map(|(target, (window, message))| (target == Some(window)).then_some(message))
        } else {
            Subscription::none()
        };
        let web_font = if self.web_system_font_ready {
            Subscription::none()
        } else {
            Subscription::run(crate::web_fonts::read_once)
                .with(self.window_id)
                .map(|(_, font)| Message::WebSystemFont(font))
        };
        let credentials = if self.browser.credentials.settings.enabled
            || self.browser.credentials.service.is_some()
        {
            Subscription::batch([
                self.browser.credentials.subscription(
                    self.browser.library == Some(crate::browser::InternalPage::Passwords)
                        || crate::browser::InternalPage::from_url(
                            &self.browser.saved.tabs[self.browser.saved.active].url,
                        ) == Some(crate::browser::InternalPage::Passwords),
                    self.welcome.is_none()
                        && (!self.browser.private
                            || self.browser.credentials.settings.private_fill),
                ),
                enzui_core::portal::session_monitor::subscription().map(|event| {
                    Message::Chrome(crate::browser::Command::Credential(
                        crate::browser::credentials_ui::CredentialCommand::Session(event),
                    ))
                }),
            ])
        } else {
            Subscription::none()
        };
        Subscription::batch([
            shortcuts,
            credentials,
            self.browser.desktop_notifications.subscription(),
            self.browser
                .toasts
                .subscription()
                .map(|event| Message::Chrome(crate::browser::Command::Toast(event))),
            devtools_resize,
            self.assistant_subscription(),
            autoscroll,
            maintenance,
            sound,
            self.theme.subscription().map(Message::Theme),
            web_font,
            self.browser
                .chooser
                .subscription()
                .map(|_| Message::Chrome(crate::browser::Command::ChooserWake)),
        ])
    }

    pub(crate) fn title(&self) -> String {
        if self.browser_mode {
            let mut suffix = if self.browser.private {
                "enzim Browser (Private)".to_owned()
            } else {
                "enzim Browser".to_owned()
            };
            if crate::browser_profile::name() != "Default" {
                suffix.push_str(" · ");
                suffix.push_str(crate::browser_profile::name());
            }
            if let Some(page) = self.browser.library {
                return format!("{} - {suffix}", page.title());
            }
            if let Some(pdf) = &self.browser.pdf {
                return format!(
                    "{} - {suffix}",
                    pdf.path.file_name().unwrap_or_default().to_string_lossy()
                );
            }
            if let Some(page) = crate::browser::InternalPage::from_url(
                &self.browser.saved.tabs[self.browser.saved.active].url,
            ) {
                return format!("{} - {suffix}", page.title());
            }
            let service = self.webview.borrow();
            match service.dom_report.as_deref() {
                Some(title) if !title.is_empty() => format!("{title} - {suffix}"),
                _ => suffix,
            }
        } else {
            "enzUI web view probe".into()
        }
    }

    pub(crate) fn submit_browser_action(&mut self, action: crate::webview_native::BrowserAction) {
        let action = if let crate::webview_native::BrowserAction::Navigate(uri) = &action
            && uri.starts_with("file:")
        {
            self.close_library();
            match self.local_files.open(uri) {
                Ok(crate::desktop_files::Opened::Pdf(path)) => {
                    let old = std::mem::replace(
                        &mut self.webview,
                        RefCell::new(webview::WebViewService::new()),
                    );
                    self.retain_download_host(old);
                    self.pending_navigation = None;
                    self.pending_native_open = false;
                    self.address =
                        crate::desktop_files::file_uri(&path).unwrap_or_else(|| uri.clone());
                    self.browser.saved.tabs[self.browser.saved.active]
                        .url
                        .clone_from(&self.address);
                    let task = self.open_pdf(path, None);
                    self.pending_tasks.push(task);
                    return;
                }
                Ok(crate::desktop_files::Opened::Web { uri, file_uri }) => {
                    self.address.clone_from(&file_uri);
                    self.browser.saved.tabs[self.browser.saved.active].url = file_uri;
                    crate::webview_native::BrowserAction::Navigate(uri)
                }
                Err(error) => {
                    self.webview.borrow_mut().native_error =
                        Some(format!("Could not open the selected file: {error}"));
                    return;
                }
            }
        } else {
            action
        };
        if matches!(
            action,
            crate::webview_native::BrowserAction::Navigate(_)
                | crate::webview_native::BrowserAction::Fixture
        ) {
            self.close_library();
        }
        if matches!(
            action,
            crate::webview_native::BrowserAction::Navigate(_)
                | crate::webview_native::BrowserAction::Reload
                | crate::webview_native::BrowserAction::Back
                | crate::webview_native::BrowserAction::Forward
        ) {
            self.webview.borrow_mut().cancel_pending_paste();
            self.browser.input.anchor = None;
            self.browser.input.hovered_link = None;
            let key = self.browser.saved.tabs[self.browser.saved.active].key;
            self.browser.pdf_requests.remove(&key);
            if self
                .browser
                .pdf_prefetch
                .as_ref()
                .is_some_and(|(owner, _)| *owner == key)
            {
                self.browser.pdf_prefetch = None;
            }
        }
        let action = if matches!(&action,crate::webview_native::BrowserAction::Navigate(url) if url=="enzim://qa"&&std::env::var_os("ENZIM_QA").is_some())
        {
            crate::webview_native::BrowserAction::Fixture
        } else {
            action
        };
        if let crate::webview_native::BrowserAction::Navigate(url) = &action
            && let Some(page) = crate::browser::InternalPage::from_url(url)
        {
            self.show_internal_page(page);
            return;
        }
        if matches!(action, crate::webview_native::BrowserAction::Navigate(_)) {
            self.adopt_blank_prepared_tab();
        }
        if self.webview.borrow().native.is_none()
            && !matches!(
                action,
                crate::webview_native::BrowserAction::Navigate(_)
                    | crate::webview_native::BrowserAction::ResumePopup(_)
                    | crate::webview_native::BrowserAction::Fixture
            )
        {
            match action {
                crate::webview_native::BrowserAction::Credential(fill) => {
                    fill.job.complete(Err("Open a web page first.".into()));
                }
                crate::webview_native::BrowserAction::Evaluate { job, .. }
                | crate::webview_native::BrowserAction::Console { job, .. }
                | crate::webview_native::BrowserAction::ExportPdf { job, .. }
                | crate::webview_native::BrowserAction::ExportScreenshot { job }
                | crate::webview_native::BrowserAction::ExportHtml { job }
                | crate::webview_native::BrowserAction::ExportMhtml { job } => {
                    job.complete(Err("Open a web page first.".into()));
                }
                _ => {}
            }
            return;
        }
        if self.webview.borrow().native.is_none() {
            let container = self.browser.saved.tabs[self.browser.saved.active].container;
            let profile = match self.profile_for_container(container) {
                Ok(profile) => profile,
                Err(error) => {
                    if container.is_some_and(|id| {
                        !self
                            .browser
                            .saved
                            .containers
                            .iter()
                            .any(|entry| entry.id == id)
                    }) {
                        self.pending_navigation = None;
                        self.pending_native_open = false;
                        self.browser.pdf = None;
                        self.pending_pdf = None;
                    }
                    self.webview.borrow_mut().native_error = Some(error);
                    return;
                }
            };
            self.webview.borrow_mut().profile = profile;
        }
        if matches!(
            action,
            crate::webview_native::BrowserAction::Navigate(_)
                | crate::webview_native::BrowserAction::ResumePopup(_)
                | crate::webview_native::BrowserAction::Back
                | crate::webview_native::BrowserAction::Forward
                | crate::webview_native::BrowserAction::Reload
                | crate::webview_native::BrowserAction::Fixture
        ) {
            self.clear_devtools_document();
            self.webview.borrow_mut().loading = true;
            self.webview
                .borrow_mut()
                .initial_paint
                .load(enzui_webkit::page::LoadStage::Started);
            self.browser.pdf = None;
            self.pending_pdf = None;
            // Invalidate late PDF results when the tab starts another document.
            let active = self.browser.saved.active;
            self.browser
                .replace_tab_key(active, crate::browser::next_tab_key());
        }
        self.pending_navigation = Some(action);
        #[cfg(feature = "benchmark")]
        crate::webview_native::trace_navigation("navigation-submitted");
        if self.webview.borrow().native.is_none() {
            if self.renderer_ready {
                self.webview.borrow_mut().open_native();
            } else {
                self.pending_native_open = true;
            }
        }
        self.flush_browser_navigation();
    }

    pub(crate) fn sync_web_theme(&self) {
        let dark = self.look().palette.is_dark();
        for host in std::iter::once(&self.webview)
            .chain(self.browser.sleeping.iter().flatten().map(|tab| &tab.host))
            .chain(self.browser.download_hosts.iter())
            .chain(self.prepared_new_tab.iter().map(|prepared| &prepared.host))
        {
            let mut service = host.borrow_mut();
            if service.preferred_system_font != self.web_system_font
                && let Some(font) = &self.web_system_font
                && let (Some(native), Some(view)) = (&service.native, service.live_view)
                && native
                    .browse(
                        &service.router,
                        view,
                        crate::webview_native::BrowserAction::SystemFont(font.clone()),
                    )
                    .is_ok()
            {
                service
                    .preferred_system_font
                    .clone_from(&self.web_system_font);
            }
            if service.preferred_dark == Some(dark) {
                continue;
            }
            if let (Some(native), Some(view)) = (&service.native, service.live_view)
                && native
                    .browse(
                        &service.router,
                        view,
                        crate::webview_native::BrowserAction::ColorScheme(dark),
                    )
                    .is_ok()
            {
                service.preferred_dark = Some(dark);
            }
        }
    }
    pub(crate) fn flush_browser_navigation(&mut self) {
        self.sync_web_theme();
        if !self.web_system_font_ready || self.webview.borrow().native.is_none() {
            return;
        }
        if let Some(action) = self.pending_navigation.take() {
            if matches!(
                action,
                crate::webview_native::BrowserAction::Navigate(_)
                    | crate::webview_native::BrowserAction::ResumePopup(_)
                    | crate::webview_native::BrowserAction::Fixture
                    | crate::webview_native::BrowserAction::Reload
                    | crate::webview_native::BrowserAction::Back
                    | crate::webview_native::BrowserAction::Forward
            ) {
                {
                    let service = self.webview.borrow();
                    if let (Some(native), Some(view)) = (&service.native, service.live_view) {
                        let _ = native.browse(
                            &service.router,
                            view,
                            crate::webview_native::BrowserAction::Privacy(Box::new(
                                self.effective_privacy(self.browser.saved.active),
                            )),
                        );
                    }
                }
                {
                    let service = self.webview.borrow();
                    if let (Some(native), Some(view)) = (&service.native, service.live_view) {
                        let container =
                            self.browser.saved.tabs[self.browser.saved.active].container;
                        let choices = self
                            .browser
                            .saved
                            .site_permissions
                            .iter()
                            .filter(|choice| choice.container == container)
                            .cloned()
                            .collect();
                        let _ = native.browse(
                            &service.router,
                            view,
                            crate::webview_native::BrowserAction::PermissionChoices(choices),
                        );
                        let _ = native.browse(
                            &service.router,
                            view,
                            crate::webview_native::BrowserAction::SpellCheck(
                                self.spelling_languages(),
                            ),
                        );
                        let _ = native.browse(
                            &service.router,
                            view,
                            crate::webview_native::BrowserAction::NotificationsEnabled(
                                !self.browser.private
                                    && self.browser.saved.notifications
                                        == crate::website_notifications::Policy::Ask,
                            ),
                        );
                    }
                }
                {
                    let mut origins =
                        self.local_origins(self.browser.saved.tabs[self.browser.saved.active].container);
                    let service = self.webview.borrow();
                    if let (Some(native), Some(view)) = (&service.native, service.live_view) {
                        let document_uri = match &action {
                            crate::webview_native::BrowserAction::Navigate(uri)
                            | crate::webview_native::BrowserAction::ResumePopup(uri) => {
                                Some(uri.as_str())
                            }
                            _ => service.location.as_deref(),
                        };
                        if let Some(uri) = document_uri
                            && self.local_files.display_uri(uri).is_some()
                            && let Ok(uri) = url::Url::parse(uri)
                        {
                            origins.push(uri.origin().ascii_serialization());
                        }
                        let _ = native.browse(
                            &service.router,
                            view,
                            crate::webview_native::BrowserAction::LocalOrigins(origins),
                        );
                    }
                }
                {
                    let service = self.webview.borrow();
                    if let (Some(native), Some(view)) = (&service.native, service.live_view) {
                        let _ = native.browse(
                            &service.router,
                            view,
                            crate::webview_native::BrowserAction::DeveloperTools(
                                self.browser.devtools.open,
                            ),
                        );
                    }
                }
                {
                    let service = self.webview.borrow();
                    if let (Some(native), Some(view)) = (&service.native, service.live_view) {
                        let _ = native.browse(
                            &service.router,
                            view,
                            crate::webview_native::BrowserAction::Zoom(
                                f64::from(service.zoom_percent) / 100.,
                            ),
                        );
                    }
                }
                self.prepare_navigation(action);
            } else {
                // An error page stays until the page is left or loaded again:
                // a zoom or a theme change sent to the view is no reason to
                // take it down and show the nothing behind it.
                let leaves = matches!(
                    action,
                    crate::webview_native::BrowserAction::Reload
                        | crate::webview_native::BrowserAction::Back
                        | crate::webview_native::BrowserAction::Forward
                );
                let mut service = self.webview.borrow_mut();
                if let (Some(native), Some(view)) = (&service.native, service.live_view) {
                    match native.browse(&service.router, view, action) {
                        Err(error) => service.native_error = Some(error),
                        Ok(()) if leaves => service.native_error = None,
                        Ok(()) => {}
                    }
                }
            }
        }
    }

    pub(crate) fn view(&self) -> Element<'_, Message> {
        let content = if self.browser_mode {
            let shell = enzui_widgets::app_shell::enz_app_shell(|| {
                self.welcome.as_ref().map_or_else(
                    || self.chrome_view(),
                    |welcome| welcome.view(self.look()),
                )
            })
            .layout(enzui_widgets::app_shell::ShellLayout::ContentOnly)
            .tokens({
                let mut tokens = enzui_widgets::app_shell::ShellTokens::flat_page()
                    .gap(0.0)
                    .radius(self.frame_cut())
                    .show_close(false);
                // The chrome paints a glass frame itself, see-through; the
                // shell's own fill under it would close it.
                if self.welcome.is_none()
                    && matches!(
                        self.chrome_metrics().frame,
                        crate::appearance::Frame::Glass(_)
                    )
                {
                    tokens.shell_tone = enzui_widgets::surface::SurfaceTone::Transparent;
                }
                tokens
            })
            .on_window_gesture(|command| Message::Chrome(crate::browser::Command::Window(command)))
            .into_element();
            enzui_widgets::toast_host::enz_toast_layer(
                shell,
                self.browser.toasts.view().map(|toasts| {
                    toasts.map(|event| Message::Chrome(crate::browser::Command::Toast(event)))
                }),
                self.toast_placement(),
            )
        } else {
            enzui_widgets::scroll_view::enz_scrollable(
                iced::widget::container(self.view_webview())
                    .width(iced::Length::Fill)
                    .padding(iced::Padding::new(40.0)),
            )
            .height(iced::Length::Fill)
            .into()
        };
        // Where the compositor paints the frame's glass, this hands it the
        // frame each time the window is drawn. Always there, so a frame that
        // turns to glass keeps its tree.
        let content =
            enzui_widgets::desktop_glass::enz_desktop_glass(&self.backdrop().window_glass(), content)
                .into_element();
        let mut layers = iced::widget::stack![
            content,
            enzui_webview::gpu::renderer_probe().on_support(|_| Message::WebViewRendererReady)
        ];
        if let Some(dispatcher) = webview_native::main_thread_dispatcher(|unavailable| {
            self.dispatch_native_ready(unavailable)
        }) {
            layers = layers.push(dispatcher);
        }
        layers.into()
    }

    /// Toasts sit in the bottom-right corner, above the welcome's action bar
    /// (16 px padding around a 32 px button) and above the find bar while
    /// either is showing.
    fn toast_placement(&self) -> enzui_widgets::toast_host::ToastOverlayPlacement {
        let placement = enzui_widgets::toast_host::ToastOverlayPlacement::default();
        if self.welcome.is_some() {
            placement.bottom_inset(64.0)
        } else if self.browser.panel == crate::browser::Panel::Find && !self.browser.devtools.open {
            placement.bottom_inset(self.find_bar_clearance())
        } else {
            placement
        }
    }

    fn dispatch_native_ready(&self, unavailable: bool) -> Option<Message> {
        if unavailable {
            self.webview.borrow_mut().detach_gpu_source();
        }
        let previous = self.webview.borrow().native_visual_revision;
        let discarded = self.webview.borrow_mut().discard_refused_gpu_frame();
        webview_native::pump_main_thread();
        let traversal = self.webview.borrow_mut().ingest_native();
        if let Some(prepared) = &self.prepared_new_tab {
            prepared.host.borrow_mut().ingest_native();
        }
        let mut background_changed = false;
        for tab in self.browser.sleeping.iter().flatten() {
            let old = tab.host.borrow().native_visual_revision;
            tab.host.borrow_mut().ingest_native();
            background_changed |= old != tab.host.borrow().native_visual_revision;
        }
        for host in &self.browser.download_hosts {
            let old = host.borrow().native_visual_revision;
            host.borrow_mut().ingest_native();
            background_changed |= old != host.borrow().native_visual_revision;
        }
        if let Some(direction) = traversal {
            self.pending_native_traversal.set(Some(direction));
        }
        let publish = unavailable
            || background_changed
            || discarded
            || traversal.is_some()
            || self.webview.borrow().native_visual_revision != previous;
        if publish {
            self.native_dispatch_publication.set(true);
        }
        publish.then_some(Message::WebViewNativeReady)
    }
}

fn browser_shortcut(event: iced::Event) -> Option<Message> {
    use crate::browser::actions::ActionCommand as A;
    let action = |command| Message::Chrome(crate::browser::Command::Action(command));
    if let iced::Event::Window(iced::window::Event::Resized(size)) = event {
        return Some(Message::Chrome(crate::browser::Command::Viewport(size)));
    }
    if let iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) = event {
        return Some(Message::Chrome(crate::browser::Command::Pointer(position)));
    }
    // The side buttons of a mouse.
    match event {
        iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Back)) => {
            return Some(Message::BrowserBack);
        }
        iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Forward)) => {
            return Some(Message::BrowserForward);
        }
        _ => {}
    }
    if let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. }) = &event
    {
        if matches!(
            key.as_ref(),
            iced::keyboard::Key::Named(iced::keyboard::key::Named::F6)
        ) {
            return Some(Message::BrowserFocusAddress);
        }
        if matches!(
            key.as_ref(),
            iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape)
        ) {
            return Some(Message::Chrome(crate::browser::Command::Escape));
        }
        if matches!(
            key.as_ref(),
            iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter)
        ) && modifiers.is_empty()
        {
            return Some(Message::Chrome(crate::browser::Command::Prompt(
                crate::browser::prompts::PromptCommand::Confirm,
            )));
        }
        if matches!(
            key.as_ref(),
            iced::keyboard::Key::Named(iced::keyboard::key::Named::F11)
        ) {
            return Some(Message::Chrome(crate::browser::Command::Fullscreen));
        }
        if matches!(
            key.as_ref(),
            iced::keyboard::Key::Named(iced::keyboard::key::Named::F12)
        ) {
            return Some(Message::Chrome(crate::browser::Command::Dev(
                crate::browser::devtools::DevCommand::Toggle,
            )));
        }
        if matches!(
            key.as_ref(),
            iced::keyboard::Key::Named(iced::keyboard::key::Named::F5)
        ) {
            return Some(if modifiers.control() {
                action(A::HardReload)
            } else {
                Message::BrowserReload
            });
        }
        if matches!(
            key.as_ref(),
            iced::keyboard::Key::Named(iced::keyboard::key::Named::F3)
        ) {
            return Some(Message::Chrome(crate::browser::Command::FindNext(
                modifiers.shift(),
            )));
        }
        if modifiers.alt() {
            match key.as_ref() {
                iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowLeft) => {
                    return Some(Message::BrowserBack);
                }
                iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowRight) => {
                    return Some(Message::BrowserForward);
                }
                iced::keyboard::Key::Character("d" | "D") => {
                    return Some(Message::BrowserFocusAddress);
                }
                _ => {}
            }
        }
    }
    if let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. }) = event
        && modifiers.control()
    {
        use crate::browser::{Command as C, Panel};
        match key.as_ref() {
            iced::keyboard::Key::Character("+" | "=") => return Some(Message::Chrome(C::Zoom(10))),
            iced::keyboard::Key::Character("-" | "_") => {
                return Some(Message::Chrome(C::Zoom(-10)));
            }
            iced::keyboard::Key::Character("0") => return Some(Message::Chrome(C::Zoom(0))),
            iced::keyboard::Key::Character("j" | "J") if modifiers.shift() => {
                return Some(Message::Chrome(C::Dev(
                    crate::browser::devtools::DevCommand::Console,
                )));
            }
            iced::keyboard::Key::Character("i" | "I") if modifiers.shift() => {
                return Some(Message::Chrome(C::Dev(
                    crate::browser::devtools::DevCommand::Toggle,
                )));
            }
            iced::keyboard::Key::Character("s" | "S") => {
                return Some(Message::Chrome(C::Export(
                    crate::browser::export::ExportCommand::Open,
                )));
            }
            iced::keyboard::Key::Character("p" | "P") => {
                return Some(Message::Chrome(C::Print));
            }
            iced::keyboard::Key::Character("o" | "O") if modifiers.shift() => {
                return Some(Message::Chrome(C::Panel(Panel::Bookmarks)));
            }
            iced::keyboard::Key::Character("o" | "O") => {
                return Some(Message::Chrome(C::OpenPdf));
            }
            iced::keyboard::Key::Character("w" | "W") => {
                return Some(Message::Chrome(C::CloseActive));
            }
            iced::keyboard::Key::Named(iced::keyboard::key::Named::Tab) => {
                return Some(Message::Chrome(C::Cycle(modifiers.shift())));
            }
            iced::keyboard::Key::Named(iced::keyboard::key::Named::PageDown) => {
                return Some(Message::Chrome(C::Cycle(false)));
            }
            iced::keyboard::Key::Named(iced::keyboard::key::Named::PageUp) => {
                return Some(Message::Chrome(C::Cycle(true)));
            }
            iced::keyboard::Key::Named(iced::keyboard::key::Named::Delete)
                if modifiers.shift() =>
            {
                return Some(Message::Chrome(C::Panel(Panel::History)));
            }
            iced::keyboard::Key::Character(digit @ ("1" | "2" | "3" | "4" | "5" | "6" | "7" | "8")) => {
                let nth = usize::from(digit.as_bytes()[0] - b'1');
                return Some(action(A::SelectNth(nth)));
            }
            iced::keyboard::Key::Character("9") => {
                return Some(action(A::SelectNth(crate::browser::actions::LAST_TAB)));
            }
            iced::keyboard::Key::Character("h" | "H") => {
                return Some(Message::Chrome(C::Panel(Panel::History)));
            }
            iced::keyboard::Key::Character("j" | "J") if !modifiers.shift() => {
                return Some(Message::Chrome(C::Panel(Panel::Downloads)));
            }
            iced::keyboard::Key::Character("b" | "B") if modifiers.shift() => {
                return Some(Message::Chrome(C::Bookmarks(
                    crate::browser::bookmarks::BookmarkCommand::Bar,
                )));
            }
            iced::keyboard::Key::Character("k" | "K") => {
                return Some(Message::BrowserFocusAddress);
            }
            iced::keyboard::Key::Character("u" | "U") => {
                return Some(action(A::ViewSource));
            }
            iced::keyboard::Key::Character("r" | "R") if modifiers.shift() => {
                return Some(action(A::HardReload));
            }
            iced::keyboard::Key::Character("t" | "T") => {
                return Some(Message::Chrome(if modifiers.shift() {
                    C::Reopen
                } else {
                    C::New
                }));
            }
            iced::keyboard::Key::Character("n" | "N") => {
                return Some(Message::Chrome(if modifiers.shift() {
                    C::NewPrivateWindow
                } else {
                    C::NewWindow
                }));
            }
            iced::keyboard::Key::Character("g" | "G") => {
                return Some(Message::Chrome(C::FindNext(modifiers.shift())));
            }
            iced::keyboard::Key::Character("f" | "F") => {
                return Some(Message::Chrome(C::Panel(Panel::Find)));
            }
            iced::keyboard::Key::Character("a" | "A") if modifiers.shift() => {
                return Some(Message::Assistant(crate::assistant::Msg::Toggle));
            }
            iced::keyboard::Key::Character("e" | "E") if modifiers.shift() => {
                return Some(Message::Chrome(C::Immersive(
                    crate::browser::immersive::ImmersiveCommand::Toggle,
                )));
            }
            iced::keyboard::Key::Character("d" | "D") => {
                return Some(Message::Chrome(C::Bookmark));
            }
            iced::keyboard::Key::Character("l" | "L") if modifiers.shift() => {
                return Some(Message::Chrome(C::Credential(
                    crate::browser::credentials_ui::CredentialCommand::OpenSite,
                )));
            }
            iced::keyboard::Key::Character("l" | "L") => {
                return Some(Message::BrowserFocusAddress);
            }
            iced::keyboard::Key::Character("r" | "R") => {
                return Some(Message::BrowserReload);
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod native_dispatch_tests {
    use super::*;
    use iced_test::runtime::{Action, task::into_stream};

    fn actions(task: Task<Message>) -> Vec<Action<Message>> {
        use iced::futures::{FutureExt, StreamExt};
        let Some(mut stream) = into_stream(task) else {
            return Vec::new();
        };
        let mut ready = Vec::new();
        while let Some(Some(action)) = stream.next().now_or_never() {
            ready.push(action);
        }
        ready
    }

    #[test]
    fn native_dispatch_publication_rebuilds_and_survives_other_messages() {
        let mut app = App::new(EnzTheme::dark());
        let view = app.webview.borrow().live_view.unwrap();
        app.webview.borrow_mut().gpu_source = Some(enzui_webview::gpu::GpuFrameSource::new(view));
        assert!(matches!(
            app.dispatch_native_ready(true),
            Some(Message::WebViewNativeReady)
        ));
        assert!(app.webview.borrow().gpu_source.is_none());
        assert!(app.native_dispatch_publication.get());
        let _ = app.update(Message::BrowserAddress("https://example.com".into()));
        assert!(app.native_dispatch_publication.get());
        let published = actions(app.update(Message::WebViewNativeReady));
        assert!(
            !published
                .iter()
                .any(|action| matches!(action, Action::Invalidate(_)))
        );
        assert!(!app.native_dispatch_publication.get());
        let quiet = actions(app.update(Message::WebViewNativeReady));
        assert!(
            quiet
                .iter()
                .any(|action| matches!(action, Action::Invalidate(_)))
        );
    }

    #[test]
    fn forwarded_scroll_and_pointer_preserve_cached_scene_until_publication() {
        let mut app = App::new(EnzTheme::dark());
        app.browser_mode = true;
        let view = app.webview.borrow().live_view.unwrap();
        let wheel = || {
            Message::WebViewInput(enzui_webkit::input::InputEvent::WheelScrolled {
                view,
                delta_x: 0.,
                delta_y: -30.,
                modifiers: enzui_webkit::input::Modifiers::NONE,
            })
        };
        let quiet = |task| {
            actions(task)
                .iter()
                .any(|action| matches!(action, Action::Invalidate(_)))
        };
        assert!(quiet(app.update(wheel())));
        let at = iced::Point::new(85., 120.);
        assert!(quiet(
            app.update(Message::Chrome(crate::browser::Command::Pointer(at)))
        ));
        assert_eq!(app.browser.pointer, at);
        assert!(!quiet(
            app.update(Message::BrowserAddress("https://example.com".into()))
        ));
        app.native_dispatch_publication.set(true);
        assert!(!quiet(app.update(wheel())));
        assert!(app.native_dispatch_publication.get());
        app.native_dispatch_publication.set(false);
        app.allow_quiet = false;
        assert!(!quiet(app.update(wheel())));
    }

    #[cfg(feature = "wpe")]
    #[test]
    fn first_page_frame_ends_loading_placeholder_before_network_completion() {
        use enzui_core::media::{Generation, Rgba8Frame};
        use enzui_webkit::page::{LoadStage, PageEvent};
        let mut host = crate::webview::WebViewService::new();
        let view = host.live_view.unwrap();
        let frame = |seq| {
            Rgba8Frame::new(
                Generation::FIRST,
                enzui_core::frame_plan::FrameSeq(seq),
                2,
                2,
                vec![255; 16],
            )
            .unwrap()
        };
        let notify = |host: &mut crate::webview::WebViewService,
                      stages: &[LoadStage],
                      frame: Option<Rgba8Frame>| {
            host.native = Some(crate::webview_native::NativePage::with_pending_update(
                crate::webview_native::NativeUpdate {
                    view,
                    page_events: stages.iter().copied().map(PageEvent::Load).collect(),
                    frame: frame.map(|frame| {
                        Ok(crate::webview_native::NativeSnapshot {
                            frame,
                            pixel_scale: 1.5,
                        })
                    }),
                    gpu_frame: None,
                    metadata: None,
                    report: None,
                    tab_reply: None,
                    clipboard: None,
                    selection: None,
                    view_focused: None,
                    error: None,
                },
            ));
            host.ingest_native();
        };
        notify(&mut host, &[LoadStage::Started], Some(frame(1)));
        assert!(
            host.initial_paint.waiting(),
            "bootstrap frames stay behind the placeholder"
        );
        notify(&mut host, &[LoadStage::Committed], None);
        assert!(
            host.initial_paint.waiting(),
            "response headers alone do not paint content"
        );
        let revision = host.native_visual_revision;
        notify(&mut host, &[], Some(frame(2)));
        let displayed = host.displayed_frame().expect("presented snapshot");
        assert_eq!(host.frame_pixel_scale(displayed), 1.5);
        assert!(!host.initial_paint.waiting());
        assert!(host.loading, "subresources may still be loading");
        assert!(
            host.native_visual_revision > revision,
            "first paint publishes the new scene"
        );
        notify(&mut host, &[LoadStage::Started], None);
        assert!(
            !host.initial_paint.waiting(),
            "reload keeps existing page content visible"
        );
    }

    #[cfg(feature = "wpe")]
    #[test]
    fn background_tab_publication_during_pointer_update_rebuilds_chrome() {
        let mut app = App::new(EnzTheme::dark());
        app.browser_mode = true;
        app.browser.private = true;
        app.browser.saved.active = 0;
        app.browser.saved.tabs.truncate(1);
        app.browser.history_store = crate::history::Store::open(None);
        let mut host = crate::webview::WebViewService::new();
        let view = host.live_view.unwrap();
        host.native = Some(crate::webview_native::NativePage::with_pending_update(
            crate::webview_native::NativeUpdate {
                view,
                metadata: Some(std::sync::Arc::new(crate::webview_native::NativeMetadata {
                    location: Some("https://example.org/".into()),
                    committed_location: Some("https://example.org/".into()),
                    dom_report: Some("Updated background title".into()),
                    ..Default::default()
                })),
                page_events: Vec::new(),
                report: None,
                frame: None,
                gpu_frame: None,
                tab_reply: None,
                clipboard: None,
                selection: None,
                view_focused: Some(false),
                error: None,
            },
        ));
        app.browser.saved.tabs.push(crate::browser::TabInfo {
            key: crate::browser::next_tab_key(),
            url: "https://example.org/".into(),
            title: "Old title".into(),
            favicon: None,
            page_copy: None,
            pinned: false,
            group: None,
            container: None,
        });
        app.browser.sleeping = vec![
            None,
            Some(crate::browser::SleepingTab {
                pdf: None,
                host: std::cell::RefCell::new(host),
            }),
        ];
        let updated = actions(app.update(Message::Chrome(crate::browser::Command::Pointer(
            iced::Point::new(30., 40.),
        ))));
        assert_eq!(app.browser.saved.tabs[1].title, "Updated background title");
        assert!(
            !updated
                .iter()
                .any(|action| matches!(action, Action::Invalidate(_))),
            "background publication must redraw chrome"
        );
        let idle = actions(app.update(Message::Chrome(crate::browser::Command::Pointer(
            iced::Point::new(31., 40.),
        ))));
        assert!(
            idle.iter()
                .any(|action| matches!(action, Action::Invalidate(_))),
            "an unchanged background tab must not force rebuilding"
        );
    }

    #[test]
    fn native_dispatch_pending_focus_traverses_once() {
        let mut app = App::new(EnzTheme::dark());
        app.pending_native_traversal
            .set(Some(enzui_core::focus::Traversal::Next));
        let first = actions(app.update(Message::WebViewNativeReady));
        assert_eq!(
            first
                .iter()
                .filter(|action| matches!(action, Action::Widget(_)))
                .count(),
            1
        );
        let second = actions(app.update(Message::WebViewNativeReady));
        assert!(
            !second
                .iter()
                .any(|action| matches!(action, Action::Widget(_)))
        );
    }
}

#[cfg(test)]
mod input_sync_checks {
    use super::*;
    use crate::browser::{Command, c};
    #[test]
    fn stable_input_skips_metadata_work_but_drains_events_and_publishes_changed_state() {
        let mut app = crate::browser::browser_tests::app();
        app.browser_mode = true;
        #[cfg(feature = "benchmark")]
        {
            app.input_sync_fast = true;
        }
        app.browser.saved.tabs[0].url = "https://example.org/document".into();
        app.webview.borrow_mut().dom_report = Some("Settled title".into());
        app.sync_browser();
        app.webview.borrow_mut().dom_report = Some("Changed title".into());
        let _ = app.update(c(Command::Pointer(iced::Point::new(20., 30.))));
        assert_eq!(app.browser.saved.tabs[0].title, "Settled title");
        app.webview
            .borrow_mut()
            .page_events
            .push(enzui_webkit::page::PageEvent::FindMatches(Some(7)));
        let _ = app.update(c(Command::Pointer(iced::Point::new(21., 30.))));
        assert_eq!(app.browser.saved.tabs[0].title, "Changed title");
        assert!(app.browser.find_status.contains('7'));
        assert!(app.webview.borrow().page_events.is_empty());
        app.webview.borrow_mut().dom_report = Some("Published metadata".into());
        app.native_dispatch_publication.set(true);
        let _ = app.update(c(Command::Pointer(iced::Point::new(22., 30.))));
        assert_eq!(app.browser.saved.tabs[0].title, "Published metadata");
        assert!(app.native_dispatch_publication.get());
        let _ = app.update(Message::WebViewNativeReady);
        assert!(!app.native_dispatch_publication.get());
    }
}
