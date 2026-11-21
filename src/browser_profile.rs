use enzui_core::activation::ApplicationIntent;
use enzui_core::state_authority::{CustodySnapshot, StateAuthority};
use enzui_widgets::single_instance::{InstanceStart, SingleInstance};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::OnceLock};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Entry {
    pub(crate) id: String,
    pub(crate) name: String,
}
impl Default for Entry {
    fn default() -> Self {
        Self {
            id: "default".into(),
            name: "Default".into(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Paths {
    pub(crate) config: PathBuf,
    pub(crate) data: PathBuf,
    pub(crate) cache: PathBuf,
}
impl Paths {
    fn from_roots(
        config: PathBuf,
        data: PathBuf,
        cache: PathBuf,
        id: &str,
    ) -> Result<Self, String> {
        if !valid_id(id) {
            return Err("Invalid profile identity.".into());
        }
        let scope = |base: PathBuf| {
            let root = base.join("enzim-browser");
            if id == "default" {
                root
            } else {
                root.join("profiles").join(id)
            }
        };
        Ok(Self {
            config: scope(config),
            data: scope(data),
            cache: scope(cache),
        })
    }
    fn for_entry(entry: &Entry) -> Result<Self, String> {
        Self::from_roots(
            home("XDG_CONFIG_HOME", ".config"),
            home("XDG_DATA_HOME", ".local/share"),
            home("XDG_CACHE_HOME", ".cache"),
            &entry.id,
        )
    }
}

fn home(variable: &str, fallback: &str) -> PathBuf {
    std::env::var_os(variable).map_or_else(
        || PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(fallback),
        PathBuf::from,
    )
}
fn valid_id(id: &str) -> bool {
    id == "default"
        || (id.len() == 32
            && id
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
}
struct Active {
    entry: Entry,
    paths: Paths,
    _lock: Option<std::fs::File>,
}
static ACTIVE: OnceLock<Active> = OnceLock::new();
static CHOOSER: OnceLock<bool> = OnceLock::new();
static INSTANCE: std::sync::Mutex<Option<SingleInstance>> = std::sync::Mutex::new(None);
pub(crate) fn take_instance() -> Option<SingleInstance> {
    INSTANCE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
}

fn selected() -> &'static Active {
    ACTIVE.get_or_init(|| Active {
        entry: Entry::default(),
        paths: Paths {
            config: home("XDG_CONFIG_HOME", ".config").join("enzim-browser"),
            data: home("XDG_DATA_HOME", ".local/share").join("enzim-browser"),
            cache: home("XDG_CACHE_HOME", ".cache").join("enzim-browser"),
        },
        _lock: None,
    })
}
pub(crate) fn paths() -> &'static Paths {
    &selected().paths
}
pub(crate) fn name() -> &'static str {
    &selected().entry.name
}
pub(crate) fn chooser_requested() -> bool {
    CHOOSER.get().copied().unwrap_or(false)
}

fn profile_lock(paths: &Paths) -> Result<std::fs::File, String> {
    use rustix::fs::{FlockOperation, Mode, OFlags};
    enzui_security::secure_dir::ensure_private_dir(&paths.data)
        .map_err(|_| "The profile data directory is unsafe or unavailable.".to_owned())?;
    let fd = rustix::fs::open(
        paths.data.join("profile.lock"),
        OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::RUSR | Mode::WUSR,
    )
    .map_err(|_| "Could not open this profile's lock safely.".to_owned())?;
    let stat =
        rustix::fs::fstat(&fd).map_err(|_| "Could not verify this profile's lock.".to_owned())?;
    if stat.st_uid != rustix::process::geteuid().as_raw()
        || stat.st_nlink != 1
        || stat.st_mode & 0o077 != 0
        || rustix::fs::FileType::from_raw_mode(stat.st_mode) != rustix::fs::FileType::RegularFile
    {
        return Err("This profile's lock has unsafe ownership or permissions.".into());
    }
    rustix::fs::flock(&fd, FlockOperation::NonBlockingLockExclusive).map_err(|_| {
        "This profile is already open. Use its existing window or choose another profile."
            .to_owned()
    })?;
    Ok(fd.into())
}
fn remember_active(entry: &Entry) {
    if let Err(error) = Registry::load().and_then(|mut registry| registry.remember(entry)) {
        eprintln!("Could not remember the active browser profile: {error}");
    }
}
pub(crate) fn activate(entry: Entry) -> Result<bool, String> {
    if ACTIVE.get().is_some() {
        return Err("A browser profile is already active in this process.".into());
    }
    let paths = Paths::for_entry(&entry)?;
    use sha2::{Digest, Sha256};
    let identity = format!(
        "org.enzim.Browser.Profile.p{:x}",
        Sha256::digest(paths.data.as_os_str().as_encoded_bytes())
    );
    let instance = match SingleInstance::start_with_intent(
        &identity,
        &launch_intent(&std::env::args().collect::<Vec<_>>()),
    ) {
        InstanceStart::Owner(instance, _) => instance,
        InstanceStart::Forwarded => {
            remember_active(&entry);
            return Ok(true);
        }
    };
    let lock = profile_lock(&paths)?;
    remember_active(&entry);
    ACTIVE
        .set(Active {
            entry,
            paths,
            _lock: Some(lock),
        })
        .map_err(|_| "A profile was already selected.".to_owned())?;
    *INSTANCE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(instance);
    Ok(false)
}

#[derive(Serialize, Deserialize)]
struct Document {
    version: u8,
    profiles: Vec<Entry>,
    #[serde(default)]
    last_profile: Option<String>,
}
pub(crate) struct Registry {
    path: PathBuf,
    authority: Option<StateAuthority>,
    pub(crate) entries: Vec<Entry>,
    last_profile: Option<String>,
}
impl Registry {
    pub(crate) fn load() -> Result<Self, String> {
        Self::at(home("XDG_CONFIG_HOME", ".config").join("enzim-browser/profiles.json"))
    }
    fn at(path: PathBuf) -> Result<Self, String> {
        let mut entries = vec![Entry::default()];
        let mut custody = None;
        let mut last_profile = None;
        match StateAuthority::open_existing(&path) {
            Ok(authority) => {
                let CustodySnapshot::Current {
                    bytes,
                    overflow: false,
                    ..
                } = authority
                    .read_bounded(64 * 1024)
                    .map_err(|_| "Could not read browser profiles.".to_owned())?
                else {
                    return Err("Browser profile metadata is unavailable or too large.".into());
                };
                let document: Document = serde_json::from_slice(&bytes)
                    .map_err(|_| "Browser profile metadata is damaged.".to_owned())?;
                if document.version != 1 || document.profiles.len() > 64 {
                    return Err("Unsupported browser profile metadata.".into());
                }
                last_profile = document.last_profile;
                for entry in document.profiles {
                    if entry.id == "default"
                        || !valid_id(&entry.id)
                        || !valid_name(&entry.name)
                        || entries.iter().any(|other| {
                            other.id == entry.id
                                || other.name.to_lowercase() == entry.name.to_lowercase()
                        })
                    {
                        return Err(
                            "Browser profile metadata contains an invalid or duplicate profile."
                                .into(),
                        );
                    }
                    entries.push(entry);
                }
                custody = Some(authority);
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && std::fs::symlink_metadata(&path)
                        .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound) => {}
            Err(_) => {
                return Err(
                    "Browser profile metadata has unsafe permissions or is unavailable.".into(),
                );
            }
        }
        Ok(Self {
            path,
            authority: custody,
            last_profile: last_profile.filter(|id| entries.iter().any(|entry| &entry.id == id)),
            entries,
        })
    }
    fn last_used(&self) -> Entry {
        self.last_profile
            .as_deref()
            .and_then(|id| self.find(id))
            .unwrap_or_default()
    }
    fn remember(&mut self, entry: &Entry) -> Result<(), String> {
        let current = Self::at(self.path.clone())?;
        let entry = current.find(&entry.id).ok_or("Profile no longer exists.")?;
        if current.last_profile.as_deref() == Some(&entry.id) {
            return Ok(());
        }
        let parent = self.path.parent().ok_or("Invalid profile metadata path.")?;
        enzui_security::secure_dir::ensure_private_dir(parent)
            .map_err(|_| "Could not open browser profile settings safely.".to_owned())?;
        let mut authority = match current.authority {
            Some(authority) => authority,
            None => StateAuthority::create_new(&self.path)
                .map_err(|_| "Could not create browser profile metadata.".to_owned())?,
        };
        let bytes = serde_json::to_vec(&Document {
            version: 1,
            profiles: current.entries.iter().skip(1).cloned().collect(),
            last_profile: Some(entry.id.clone()),
        })
        .map_err(|_| "Could not encode browser profiles.".to_owned())?;
        authority
            .publish(&bytes)
            .map_err(|_| "Profiles changed while remembering the active profile.".to_owned())?;
        self.last_profile = Some(entry.id);
        self.entries = current.entries;
        Ok(())
    }
    pub(crate) fn find(&self, value: &str) -> Option<Entry> {
        self.entries
            .iter()
            .find(|entry| entry.id == value || entry.name.to_lowercase() == value.to_lowercase())
            .cloned()
    }
    pub(crate) fn create(&mut self, name: &str) -> Result<Entry, String> {
        let name = name.trim();
        if !valid_name(name) {
            return Err("Choose a profile name between 1 and 60 characters.".into());
        }
        let mut current = Self::at(self.path.clone())?;
        if current.entries.len() >= 65 {
            return Err("Up to 64 additional browser profiles are supported.".into());
        }
        if current.find(name).is_some() {
            return Err("A profile already uses that name.".into());
        }
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes)
            .map_err(|_| "Could not create a profile identity.".to_owned())?;
        let id: String = bytes
            .into_iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let entry = Entry {
            id,
            name: name.into(),
        };
        let parent = self.path.parent().ok_or("Invalid profile metadata path.")?;
        enzui_security::secure_dir::ensure_private_dir(parent)
            .map_err(|_| "Could not open browser profile settings safely.".to_owned())?;
        let mut authority = match current.authority.take() {
            Some(authority) => authority,
            None => StateAuthority::create_new(&self.path).map_err(|_| {
                "Could not create profile metadata; reopen the chooser and try again.".to_owned()
            })?,
        };
        let last_profile = current.last_profile;
        let mut entries = current.entries;
        entries.push(entry.clone());
        let bytes = serde_json::to_vec(&Document {
            version: 1,
            profiles: entries.iter().skip(1).cloned().collect(),
            last_profile: last_profile.clone(),
        })
        .map_err(|_| "Could not encode browser profiles.".to_owned())?;
        authority.publish(&bytes).map_err(|_| {
            "Profiles changed while saving. Reopen the chooser and try again.".to_owned()
        })?;
        self.entries = entries;
        self.last_profile = last_profile;
        Ok(entry)
    }
}
fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.trim() == value
        && value.chars().count() <= 60
        && !value.chars().any(char::is_control)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ForwardedLaunch {
    pub(crate) url: Option<String>,
    pub(crate) private: bool,
    pub(crate) new_window: bool,
}
pub(crate) fn launch_url(args: &[String]) -> Option<&str> {
    let valid = |value: &&str| {
        !value.is_empty()
            && value.len() <= 4096
            && !value.starts_with('-')
            && !value.chars().any(char::is_control)
    };
    if let Some(index) = args.iter().position(|arg| arg == "--browser")
        && let Some(value) = args.get(index + 1).map(String::as_str).filter(valid)
    {
        return Some(value);
    }
    let mut index = 1;
    while let Some(value) = args.get(index) {
        if matches!(value.as_str(), "-P" | "--profile" | "--create-profile") {
            index += 2;
            continue;
        }
        if valid(&value.as_str())
            && url::Url::parse(value).is_ok_and(|url| {
                matches!(url.scheme(), "http" | "https") && url.host_str().is_some()
            })
        {
            return Some(value);
        }
        index += 1;
    }
    None
}
pub(crate) fn launch_urls(args: &[String]) -> Vec<String> {
    let first = launch_url(args);
    let mut urls = Vec::new();
    let mut index = 1;
    while let Some(value) = args.get(index) {
        if matches!(value.as_str(), "-P" | "--profile" | "--create-profile") {
            index += 2;
            continue;
        }
        if value.len() <= 4096 && !value.chars().any(char::is_control) && !value.starts_with('-') {
            let path = std::path::Path::new(value);
            let target = if path.is_absolute() {
                crate::desktop_files::file_uri(path)
            } else if url::Url::parse(value).is_ok_and(|url| {
                matches!(url.scheme(), "http" | "https")
                    || (url.scheme() == "file" && url.to_file_path().is_ok())
            }) {
                Some(value.clone())
            } else if path.is_file() || value.starts_with("./") || value.starts_with("../") {
                std::env::current_dir()
                    .ok()
                    .and_then(|root| crate::desktop_files::file_uri(&root.join(path)))
            } else if first == Some(value.as_str()) {
                Some(value.clone())
            } else {
                None
            };
            if let Some(target) = target
                && !urls.contains(&target)
            {
                urls.push(target);
            }
        }
        index += 1;
    }
    urls
}

fn launch_intent(args: &[String]) -> ApplicationIntent {
    let private = args.iter().any(|arg| arg == "--private-window");
    let new_window = private || args.iter().any(|arg| arg == "--new-window");
    let urls = launch_urls(args);
    if !new_window && urls.is_empty() {
        return ApplicationIntent::Activate;
    }
    let make = |url: Option<&str>| {
        let mut message = url::Url::parse("enzim://launch").expect("static launch URI");
        if private {
            message.query_pairs_mut().append_pair("private", "1");
        }
        if new_window {
            message.query_pairs_mut().append_pair("new_window", "1");
        }
        if let Some(url) = url {
            message.query_pairs_mut().append_pair("url", url);
        }
        message.to_string()
    };
    ApplicationIntent::Open(if urls.is_empty() {
        vec![make(None)]
    } else {
        urls.into_iter()
            .take(64)
            .map(|url| make(Some(&url)))
            .collect()
    })
}

pub(crate) fn forwarded(intent: ApplicationIntent) -> Vec<ForwardedLaunch> {
    match intent {
        ApplicationIntent::Activate => Vec::new(),
        ApplicationIntent::ActivateAction { name } => match name.as_str() {
            "new-window" => vec![ForwardedLaunch {
                url: None,
                private: false,
                new_window: true,
            }],
            "new-private-window" => vec![ForwardedLaunch {
                url: None,
                private: true,
                new_window: true,
            }],
            _ => Vec::new(),
        },
        ApplicationIntent::Open(uris) => {
            uris.into_iter()
                .filter_map(|uri| {
                    if uri.len() > 16 * 1024 || uri.chars().any(char::is_control) {
                        return None;
                    }
                    let parsed = url::Url::parse(&uri).ok()?;
                    if parsed.scheme() == "enzim" && parsed.host_str() == Some("launch") {
                        let fields: std::collections::BTreeMap<_, _> =
                            parsed.query_pairs().into_owned().collect();
                        let url = fields.get("url").cloned().filter(|url| !url.is_empty());
                        if url.as_ref().is_some_and(|url| {
                            url.len() > 4096 || url.chars().any(char::is_control)
                        }) {
                            return None;
                        }
                        let private = fields.get("private").is_some_and(|value| value == "1");
                        Some(ForwardedLaunch {
                            url,
                            private,
                            new_window: private
                                || fields.get("new_window").is_some_and(|value| value == "1"),
                        })
                    } else if matches!(parsed.scheme(), "http" | "https") {
                        Some(ForwardedLaunch {
                            url: Some(uri),
                            private: false,
                            new_window: false,
                        })
                    } else {
                        None
                    }
                })
                .collect()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Request {
    Default,
    Choose,
    Open(String),
    Create(String),
}
fn parse_request(args: &[String]) -> Result<Request, String> {
    let mut requested = None;
    let mut create = None;
    let mut chooser = false;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "-P" | "--profile" => {
                if args
                    .get(index + 1)
                    .is_some_and(|value| !value.starts_with('-'))
                {
                    index += 1;
                    requested = Some(args[index].clone());
                } else {
                    chooser = true;
                }
            }
            "--create-profile" => {
                index += 1;
                create = Some(
                    args.get(index)
                        .filter(|value| !value.starts_with('-'))
                        .cloned()
                        .ok_or("A profile name is required after --create-profile.")?,
                );
            }
            value if value.starts_with("--profile=") => {
                requested = value.strip_prefix("--profile=").map(str::to_owned);
            }
            _ => {}
        }
        index += 1;
    }
    if (chooser && (requested.is_some() || create.is_some()))
        || (requested.is_some() && create.is_some())
    {
        return Err("Choose either a profile name or the profile chooser.".into());
    }
    Ok(if chooser {
        Request::Choose
    } else if let Some(name) = create {
        Request::Create(name)
    } else if let Some(name) = requested {
        Request::Open(name)
    } else {
        Request::Default
    })
}
pub(crate) fn configure(args: &[String]) -> Result<bool, String> {
    let request = parse_request(args)?;
    let chooser = request == Request::Choose;
    CHOOSER
        .set(chooser)
        .map_err(|_| "Profile arguments were already applied.".to_owned())?;
    let entry = match request {
        Request::Choose => return Ok(false),
        Request::Default => Registry::load()?.last_used(),
        Request::Create(name) => Registry::load()?.create(&name)?,
        Request::Open(name) => Registry::load()?
            .find(&name)
            .ok_or("Profile not found. Use -P to choose or create one.")?,
    };
    activate(entry)
}

pub(crate) fn open_chooser() -> Result<(), String> {
    let executable = std::env::current_exe()
        .map_err(|_| "Could not locate the browser executable.".to_owned())?;
    std::process::Command::new(executable)
        .arg("-P")
        .spawn()
        .map(|_| ())
        .map_err(|_| "Could not open the browser profile chooser.".into())
}

#[derive(Clone, Debug)]
pub(crate) enum Event {
    Name(String),
    Select(String),
    Create,
    Theme(Box<enzui_core::theme_file::integration::ThemeEvent>),
    Window(enzui_widgets::title_bar_gesture::WindowCommand),
}
pub(crate) struct Chooser {
    pub(crate) window: iced::window::Id,
    pub(crate) theme: enzui_core::theme_file::integration::ThemeIntegration,
    registry: Option<Registry>,
    name: String,
    pub(crate) notice: String,
}
impl Chooser {
    pub(crate) fn new(window: iced::window::Id) -> (Self, iced::Task<Event>) {
        let (theme, task) = enzui_core::theme_file::integration::ThemeIntegration::boot();
        let (registry, notice) = match Registry::load() {
            Ok(registry) => (Some(registry), String::new()),
            Err(error) => (None, error),
        };
        (
            Self {
                window,
                theme,
                registry,
                name: String::new(),
                notice,
            },
            task.map(|event| Event::Theme(Box::new(event))),
        )
    }
    pub(crate) fn update(&mut self, event: Event) -> Result<Option<Entry>, String> {
        match event {
            Event::Name(name) => {
                if name.chars().count() <= 60 && !name.chars().any(char::is_control) {
                    self.name = name;
                }
            }
            Event::Select(id) => {
                return self
                    .registry
                    .as_ref()
                    .and_then(|registry| registry.find(&id))
                    .map(Some)
                    .ok_or_else(|| "Profile not found.".into());
            }
            Event::Create => {
                return self
                    .registry
                    .as_mut()
                    .ok_or("Profile metadata is unavailable.")?
                    .create(&self.name)
                    .map(Some);
            }
            Event::Theme(_) | Event::Window(_) => {}
        }
        Ok(None)
    }
    fn content(&self, width: f32) -> iced::Element<'_, Event> {
        use iced::{
            Length,
            widget::{column, container, text},
        };
        let palette = &self.theme.theme().palette;
        let mut profiles = column![].spacing(4);
        if let Some(registry) = &self.registry {
            for entry in &registry.entries {
                profiles = profiles.push(
                    enzui_widgets::preferences_row::enz_preferences_row(entry.name.clone())
                        .icon("user-round")
                        .subtitle(if entry.id == "default" {
                            "Your original browser profile"
                        } else {
                            "Separate tabs, history, passwords and site data"
                        })
                        .action("Open", Event::Select(entry.id.clone()))
                        .into_element(),
                );
            }
        }
        let mut body = column![
            enzui_widgets::icon::enz_icon("users-round")
                .size(36.)
                .tint(palette.accent)
                .into_element(),
            column![
                text("A space for every part of your day").size(28).font(
                    enzui_theme::text_defaults::font(
                        enzui_theme::FontRole::Ui,
                        enzui_theme::WeightRung::Semibold
                    )
                ),
                text("Choose a browser profile, or make a fresh start.")
                    .size(14)
                    .color(palette.text_muted)
            ]
            .spacing(8),
            enzui_widgets::preferences_group::enz_preferences_group(profiles)
                .title("Your profiles")
                .bordered(true)
                .into_element(),
        ]
        .spacing(24)
        .width(Length::Fill);
        if !self.notice.is_empty() {
            body = body.push(text(&self.notice).size(13).color(palette.text_muted));
        }
        let create = column![
            text("Create a profile").size(16),
            enzui_widgets::text_field::enz_text_field("", "Profile name", &self.name, Event::Name)
                .id(iced::advanced::widget::Id::new("profile-name"))
                .on_submit(Event::Create)
                .into_element(),
            enzui_widgets::button::enz_button("Create and open", Event::Create)
                .icon("plus")
                .kind(enzui_widgets::button::ButtonKind::Accent)
                .enabled(self.registry.is_some() && valid_name(self.name.trim()))
                .into_element()
        ]
        .spacing(10);
        let page: iced::Element<'_, Event> = enzui_widgets::scroll_view::enz_scrollable(
            container(container(body).width((width - 48.).clamp(180., 580.)))
                .padding(iced::Padding {
                    top: 24.,
                    right: 24.,
                    bottom: 32.,
                    left: 24.,
                })
                .center_x(Length::Fill),
        )
        .height(Length::Fill)
        .into();
        column![
            page,
            container(container(create).width((width - 48.).clamp(180., 580.)))
                .padding([16, 24])
                .center_x(Length::Fill),
        ]
        .height(Length::Fill)
        .into()
    }
    pub(crate) fn view(&self) -> iced::Element<'_, Event> {
        enzui_widgets::app_shell::enz_app_shell(move || {
            let controls = enzui_widgets::window_controls::enz_window_controls(Event::Window(
                enzui_widgets::title_bar_gesture::WindowCommand::Close,
            ))
            .show_minimize(
                true,
                Event::Window(enzui_widgets::title_bar_gesture::WindowCommand::Minimize),
            )
            .show_maximize(
                true,
                Event::Window(enzui_widgets::title_bar_gesture::WindowCommand::ToggleMaximize),
                false,
            )
            .into_element();
            let header: iced::Element<'_, Event> = iced::widget::row![
                enzui_widgets::icon::enz_icon("compass")
                    .size(18.)
                    .tint(self.theme.theme().palette.accent)
                    .into_element(),
                iced::widget::text("enzim Browser").size(14),
                iced::widget::Space::new().width(iced::Length::Fill),
                controls
            ]
            .align_y(iced::Alignment::Center)
            .spacing(10)
            .padding([6, 12])
            .into();
            iced::widget::column![
                enzui_widgets::titlebar_drag_region::titlebar_drag_region(header)
                    .on_gesture(Event::Window),
                iced::widget::responsive(move |size| self.content(size.width))
            ]
            .height(iced::Length::Fill)
            .into()
        })
        .layout(enzui_widgets::app_shell::ShellLayout::ContentOnly)
        .tokens(
            enzui_widgets::app_shell::ShellTokens::flat_page()
                .radius(self.theme.theme().window_radius)
                .show_close(false),
        )
        .on_window_gesture(Event::Window)
        .into_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    #[test]
    fn provider_review_last_profile_survives_reopen_and_profile_creation() {
        let scratch = crate::credentials::tests::Scratch::new();
        let path = scratch.path().with_file_name("profiles.json");
        let mut registry = Registry::at(path.clone()).expect("registry");
        assert_eq!(registry.last_used(), Entry::default());
        let work = registry.create("Work").expect("work");
        registry.remember(&work).expect("remember work");
        let mut reopened = Registry::at(path.clone()).expect("reopen");
        assert_eq!(reopened.last_used(), work);
        reopened.create("Personal").expect("second profile");
        let mut reopened = Registry::at(path.clone()).expect("reopen after create");
        assert_eq!(reopened.last_used(), work);
        reopened
            .remember(&Entry::default())
            .expect("remember default");
        assert_eq!(
            Registry::at(path).expect("final reopen").last_used(),
            Entry::default()
        );
    }
    #[test]
    fn browser_profiles_lock_refuses_two_owners_and_releases_on_close() {
        let scratch = crate::credentials::tests::Scratch::new();
        let root = scratch
            .path()
            .parent()
            .expect("fixture directory")
            .join("profile");
        let paths = Paths {
            config: root.clone(),
            data: root.clone(),
            cache: root,
        };
        let owner = profile_lock(&paths).expect("first owner");
        assert!(
            profile_lock(&paths).is_err(),
            "two owners cannot write one profile"
        );
        drop(owner);
        drop(profile_lock(&paths).expect("reopened after release"));
    }
    #[test]
    fn browser_profiles_external_launch_does_not_inherit_an_active_container() {
        let mut app = crate::browser::browser_tests::app();
        app.browser.saved.tabs[0].container = Some(app.browser.saved.containers[0].id);
        drop(app.browser_command(crate::browser::Command::ExternalVisit("example.org".into())));
        let tab = &app.browser.saved.tabs[app.browser.saved.active];
        assert_eq!(tab.container, None);
        assert_eq!(tab.url, "https://example.org");
    }
    #[test]
    fn browser_profiles_parse_explicit_names_and_preserve_private_forwarding() {
        let args = |values: &[&str]| {
            std::iter::once("enzim-browser".to_owned())
                .chain(values.iter().map(|value| (*value).to_owned()))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            parse_request(&args(&["-P"])).expect("chooser"),
            Request::Choose
        );
        assert_eq!(
            parse_request(&args(&["-P", "Business"])).expect("named"),
            Request::Open("Business".into())
        );
        assert_eq!(
            parse_request(&args(&["--profile=Personal"])).expect("named"),
            Request::Open("Personal".into())
        );
        assert!(parse_request(&args(&["--create-profile", "--private-window"])).is_err());
        assert!(
            parse_request(&args(&[
                "--profile",
                "Personal",
                "--create-profile",
                "Business"
            ]))
            .is_err()
        );
        let intents = forwarded(launch_intent(&args(&[
            "--profile",
            "Business",
            "--private-window",
            "--browser",
            "https://example.org/a?one=1&two=2",
        ])));
        assert_eq!(
            intents,
            vec![ForwardedLaunch {
                url: Some("https://example.org/a?one=1&two=2".into()),
                private: true,
                new_window: true
            }]
        );
        assert!(
            forwarded(ApplicationIntent::Open(vec![
                "javascript:alert(1)".into(),
                "file:///etc/passwd".into()
            ]))
            .is_empty()
        );
        assert!(
            forwarded(ApplicationIntent::Open(vec![
                "enzim://launch?url=bad%0Avalue".into()
            ]))
            .is_empty()
        );
    }
    #[test]
    fn desktop_open_with_launches_and_forwards_multiple_uris_without_profile_arguments() {
        let values = [
            "browser",
            "--profile",
            "Business",
            "--browser",
            "file:///tmp/one%20report.html",
            "file:///tmp/two.pdf",
            "https://example.test/",
        ];
        let args = values.map(String::from);
        let expected = values[4..]
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        assert_eq!(launch_urls(&args), expected);
        assert_eq!(
            forwarded(launch_intent(&args))
                .into_iter()
                .filter_map(|request| request.url)
                .collect::<Vec<_>>(),
            expected
        );
        let args = [
            "browser".to_owned(),
            "/tmp/missing private document.pdf".to_owned(),
        ];
        assert_eq!(
            launch_urls(&args),
            vec!["file:///tmp/missing%20private%20document.pdf"]
        );
    }
    #[test]
    fn browser_profile_launch_urls_skip_flags_and_profile_values() {
        let args = |values: &[&str]| {
            std::iter::once("browser".to_owned())
                .chain(values.iter().map(|value| (*value).to_owned()))
                .collect::<Vec<_>>()
        };
        for input in [
            vec!["--browser", "--private-window"],
            vec!["--create-profile", "https://profile-name.example"],
            vec!["--profile", "https://profile-name.example"],
        ] {
            assert!(launch_url(&args(&input)).is_none());
        }
        for input in [
            vec!["--profile", "Business", "https://example.org"],
            vec!["--profile=Business", "https://example.org"],
            vec!["--browser", "https://example.org", "--private-window"],
            vec!["--browser", "--new-window", "https://example.org"],
        ] {
            assert_eq!(launch_url(&args(&input)), Some("https://example.org"));
            assert_eq!(
                forwarded(launch_intent(&args(&input)))[0].url.as_deref(),
                Some("https://example.org")
            );
        }
    }
    #[test]
    fn browser_profiles_keep_legacy_paths_and_isolate_all_storage_roots() {
        let roots = || {
            (
                PathBuf::from("/fixture/config"),
                PathBuf::from("/fixture/data"),
                PathBuf::from("/fixture/cache"),
            )
        };
        let (a, b, c) = roots();
        let default = Paths::from_roots(a, b, c, "default").expect("default");
        assert_eq!(default.config, Path::new("/fixture/config/enzim-browser"));
        let (a, b, c) = roots();
        let work = Paths::from_roots(a, b, c, "0123456789abcdef0123456789abcdef").expect("work");
        for (left, right) in [
            (&default.config, &work.config),
            (&default.data, &work.data),
            (&default.cache, &work.cache),
        ] {
            assert_ne!(left, right);
            assert!(right.ends_with("profiles/0123456789abcdef0123456789abcdef"));
        }
        for id in [
            "",
            "../default",
            "DEFAULT",
            "/tmp/profile",
            "0123456789abcdef0123456789abcdeg",
        ] {
            let (a, b, c) = roots();
            assert!(Paths::from_roots(a, b, c, id).is_err());
        }
        assert!(!valid_name("Business\nInjected"));
    }
    #[test]
    fn browser_profiles_registry_persists_distinct_names_and_refuses_substitution() {
        let scratch = crate::credentials::tests::Scratch::new();
        let path = scratch.path().with_file_name("profiles.json");
        let mut registry = Registry::at(path.clone()).expect("initial registry");
        let a = registry.create("Personal").expect("personal");
        let b = registry.create("Business").expect("business");
        assert_ne!(a.id, b.id);
        assert!(valid_id(&a.id));
        assert!(registry.create("personal").is_err());
        let reopened = Registry::at(path.clone()).expect("reopen");
        assert_eq!(reopened.find("Business"), Some(b));
        assert_eq!(reopened.find(&a.id), Some(a));
        assert_eq!(reopened.entries.len(), 3);
        std::fs::remove_file(&path).expect("remove fixture metadata");
        std::os::unix::fs::symlink(scratch.path(), &path).expect("substitute link");
        assert!(Registry::at(path).is_err());
    }
}
