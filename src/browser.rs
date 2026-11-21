use crate::{App, Message, webview::WebViewService, webview_native::BrowserAction};
use enzui_widgets::scroll_view::enz_scrollable as scrollable;
use iced::{Element, Length, Task};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
#[path = "browser_containers.rs"]
pub(crate) mod containers;
#[path = "browser_export.rs"]
pub(crate) mod export;
use export::ExportCommand;
#[path = "browser_import.rs"]
pub(crate) mod import_page;
use import_page::ImportCommand;
#[path = "browser_devtools.rs"]
pub(crate) mod devtools;
use devtools::DevCommand;
#[path = "browser_inspector.rs"]
mod inspector;
#[path = "browser_network.rs"]
mod network;
use inspector::InspectCommand;
use network::NetworkCommand;
#[path = "browser_local.rs"]
pub(crate) mod local;
#[path = "browser_permissions.rs"]
mod permissions;
use permissions::PermissionCommand;
#[path = "browser_find.rs"]
mod find_bar;
#[path = "browser_shields.rs"]
mod shields;
#[path = "browser_toasts.rs"]
pub(crate) mod toasts;
use shields::ShieldCommand;
#[path = "browser_cookies.rs"]
mod cookies;
use cookies::CookieCommand;
#[path = "browser_error.rs"]
mod error_page;
#[path = "browser_history.rs"]
mod history_page;
#[path = "browser_pages.rs"]
mod pages;
#[path = "browser_privacy.rs"]
mod privacy_settings;
#[path = "browser_search.rs"]
mod search_settings;
use privacy_settings::PrivacyCommand;
#[path = "browser_downloads.rs"]
pub(crate) mod download_panel;
#[path = "browser_tabs.rs"]
mod tabs;
#[path = "browser_zoom.rs"]
mod zoom;
use download_panel::DownloadCommand;
#[path = "browser_credentials.rs"]
pub(crate) mod credentials_ui;
use credentials_ui::CredentialCommand;
#[path = "browser_appearance.rs"]
pub(crate) mod appearance_ui;
use appearance_ui::AppearanceCommand;
#[path = "browser_actions.rs"]
pub(crate) mod actions;
use actions::ActionCommand;
#[path = "browser_bookmarks.rs"]
pub(crate) mod bookmarks;
use bookmarks::BookmarkCommand;
#[path = "browser_bookmark_bar.rs"]
pub(crate) mod bookmark_bar;
#[path = "browser_popups.rs"]
pub(crate) mod popups;
#[path = "browser_color.rs"]
pub(crate) mod color;
use color::ColorCommand;
#[path = "browser_spelling.rs"]
pub(crate) mod spelling;
#[path = "browser_tab_move.rs"]
pub(crate) mod tab_move;
#[path = "browser_page_menu.rs"]
mod page_menu;
#[path = "browser_sound.rs"]
pub(crate) mod sound;
#[path = "browser_dates.rs"]
pub(crate) mod dates;
use dates::DateCommand;
#[path = "browser_save_place.rs"]
pub(crate) mod save_place;
#[path = "browser_fullscreen.rs"]
pub(crate) mod fullscreen;
#[path = "browser_immersive.rs"]
pub(crate) mod immersive;
#[path = "browser_preferences.rs"]
pub(crate) mod preferences;
use preferences::PreferenceCommand;
#[path = "browser_print.rs"]
pub(crate) mod print;
use print::PrintCommand;
#[path = "browser_prompts.rs"]
pub(crate) mod prompts;
use prompts::PromptCommand;

#[derive(Debug, Clone)]
pub(crate) enum ContainerCommand {
    Open,
    NewTab(Option<u64>),
    Reopen(u64, Option<u64>),
    Edit(Option<u64>),
    Back,
    Override(&'static str, Option<bool>),
    #[cfg(test)]
    Agent(usize),
    #[cfg(test)]
    AgentText(String),
    Identity(crate::user_agent::Change),
    Proxy(String),
    DenyAgents(bool),
    ProxyUsername(String),
    ProxyPassword(crate::credentials::Secret),
    Name(String),
    Color(usize),
    Icon(usize),
    Menu(enzui_widgets::menu_button::MenuButtonEvent),
    Save,
    DeleteRequest(u64),
    DeleteConfirm,
}
#[derive(Debug, Clone)]
pub(crate) enum Command {
    Appearance(AppearanceCommand),
    Prompt(PromptCommand),
    Action(ActionCommand),
    Preference(PreferenceCommand),
    PrintJob(PrintCommand),
    Bookmarks(BookmarkCommand),
    /// Continue to the page whose certificate failed validation.
    TrustCertificate,
    /// F11: the window takes the screen, or gives it back.
    Fullscreen,
    Credential(CredentialCommand),
    Download(DownloadCommand),
    Export(ExportCommand),
    Dev(DevCommand),
    Inspect(InspectCommand),
    Network(NetworkCommand),
    Permission(PermissionCommand),
    Notifications(crate::website_notifications::Command),
    Welcome(crate::welcome::Command),
    Shields(ShieldCommand),
    Cookies(CookieCommand),
    Privacy(PrivacyCommand),
    Container(ContainerCommand),
    Import(ImportCommand),
    OpenLocal,
    ChooseProfile,
    ExternalVisit(String),
    /// An address opened from outside in a new tab of this container, or of
    /// none.
    ExternalVisitIn(String, Option<u64>),
    MaintenancePoll,
    Toast(enzui_widgets::toast_host::ToastEvent),
    Window(enzui_widgets::title_bar_gesture::WindowCommand),
    ChooserWake,
    ChooserEvent(enzui_widgets::file_chooser::FallbackEvent),
    FilesChosen(
        (u64, enzui_webkit::page::PageRequestId),
        enzui_core::portal::PortalOutcome<enzui_core::portal::op::FileResult>,
    ),
    TabContext(usize, iced::Point),
    ChromeContext(iced::Point),
    GroupContext(usize, iced::Point),
    Context(enzui_widgets::context_menu_host::ContextMenuEvent),
    Menu(enzui_widgets::menu_button::MenuButtonEvent),
    Pointer(iced::Point),
    Viewport(iced::Size),
    Reload,
    Stop,
    CopyText(String),
    CopySelection,
    Paste,
    Edit(&'static str),
    AnswerOption(usize),
    AnswerDialog(bool),
    PromptText(String),
    New,
    CloseActive,
    Cycle(bool),
    ToggleTabs,
    Immersive(immersive::ImmersiveCommand),
    DropTabs(enzui_core::drag::DropRequest),
    DropVisibleTabs(enzui_core::drag::DropRequest, Vec<u64>),
    TopTabsMenu,
    StartGo,
    Startup(usize),
    StartupPage(String),
    Navigate(String),
    BookmarkSearch(String),
    BookmarkMore,
    RemoveBookmark(String),
    Modal(enzui_widgets::modal_host::ModalHostEvent),
    Dismiss,
    Escape,
    Submit,
    Query(String),
    SuggestionMove(bool),
    AddressSuggestionMove(bool),
    AddressSuggestionsDismiss,
    Select(usize),
    Close(usize),
    Pin(usize),
    Panel(Panel),
    Layout(Layout),
    Right(bool),
    Bookmark,
    Visit(String),
    /// The wave on a sounding tab moves on.
    SoundBeat,
    /// The colour dialog of a page's colour field.
    Color(ColorCommand),
    /// The calendar or list of times of a page's date or time field.
    Date(DateCommand),
    /// Spelling: the setting, and the corrections in the page menu.
    Spelling(spelling::SpellingCommand),
    NewWindow,
    NewPrivateWindow,
    Minimize,
    Maximize,
    Exit,
    Find(String),
    FindCase,
    FindNext(bool),
    Input(crate::browser_input::InputCommand),
    Pdf(crate::browser_pdf::PdfCommand),
    Zoom(i32),
    ZoomTo(i32),
    MenuZoom(i32),
    MenuZoomHover,
    ZoomMenu(enzui_widgets::menu_button::MenuButtonEvent),
    AssignGroup(usize, usize),
    Group(usize),
    NewGroup,
    GroupDrop(enzui_core::drag::DropRequest, u64),
    GroupName(String),
    GroupColor(usize),
    Ungroup(usize),
    /// Ends a group; its tabs stay where it was, in no group.
    RemoveGroup(usize),
    /// Closes a group's tabs and ends the group.
    CloseGroup(usize),
    Print,
    OpenPdf,
    PdfOpen(std::path::PathBuf),
    PdfReady(u64, Result<crate::pdf::Page, String>),
    PdfClose,
    Reopen,
    Duplicate(usize),
    CopyUrl,
    HistoryFilter(String),
    HistoryRange(crate::history::Range),
    HistoryDelete(i64),
    HistoryPage(i32),
    HistoryClearRequest,
    HistoryClearConfirm,
    SelectEngine(usize),
    SearchSuggestionsEnabled(bool),
    SearchSuggestionsReady(u64, Vec<String>),
    EngineName(String),
    EngineTemplate(String),
    AddEngine,
    RemoveEngine(String),
    ToggleEngineEditor,
    ExternalPdfs(bool),
    MemoryMode(crate::webview_native::MemoryMode),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Panel {
    None,
    New,
    Options,
    Script,
    /// A server's request to sign in (HTTP authentication).
    SignIn,
    /// A bookmark's name and folder.
    BookmarkEdit,
    /// The colour for a page's colour field.
    Color,
    Downloads,
    Security,
    Passwords,
    Permissions,
    Shields,
    Settings,
    Print,
    Export,
    History,
    Bookmarks,
    Find,
    Group(usize),
    HistoryClear,
    ContainerDelete,
    CookieClear,
    /// Cookies and site data, opened from Settings.
    Cookies,
    PermissionSettings,
    /// A page's passkey request.
    Passkey,
    /// The Appearance popup.
    Theme,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(crate) enum Layout {
    Top,
    #[default]
    Sidebar,
    Icons,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InternalPage {
    New,
    Settings,
    Bookmarks,
    History,
    Containers,
    Cookies,
    Passwords,
    Import,
}
impl InternalPage {
    pub(crate) fn from_url(url: &str) -> Option<Self> {
        match url.trim_end_matches('/') {
            "" | "enzim://newtab" | "about:newtab" => Some(Self::New),
            "enzim://settings" => Some(Self::Settings),
            "enzim://bookmarks" => Some(Self::Bookmarks),
            "enzim://history" => Some(Self::History),
            "enzim://containers" => Some(Self::Containers),
            "enzim://cookies" => Some(Self::Cookies),
            "enzim://passwords" => Some(Self::Passwords),
            "enzim://import" => Some(Self::Import),
            "enzim://qa" => None,
            unknown if unknown.starts_with("enzim://") => Some(Self::New),
            _ => None,
        }
    }
    pub(crate) fn is_library(self) -> bool {
        matches!(
            self,
            Self::Bookmarks | Self::History | Self::Containers | Self::Passwords
        )
    }
    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::New => "New tab",
            Self::Settings => "Settings",
            Self::Bookmarks => "Bookmarks",
            Self::History => "History",
            Self::Containers => "Containers",
            Self::Cookies => "Cookies and site data",
            Self::Passwords => "Passwords",
            Self::Import => "Import browser data",
        }
    }
    fn icon(self) -> &'static str {
        match self {
            Self::New => "compass",
            Self::Settings => "settings",
            Self::Bookmarks => "bookmark",
            Self::History => "history",
            Self::Containers => "container",
            Self::Cookies => "cookie",
            Self::Passwords => "key-round",
            Self::Import => "folder-input",
        }
    }
    pub(crate) fn url(self) -> &'static str {
        match self {
            Self::New => "",
            Self::Settings => "enzim://settings",
            Self::Bookmarks => "enzim://bookmarks",
            Self::History => "enzim://history",
            Self::Containers => "enzim://containers",
            Self::Cookies => "enzim://cookies",
            Self::Passwords => "enzim://passwords",
            Self::Import => "enzim://import",
        }
    }
}
static NEXT_TAB_KEY: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
pub(crate) fn next_tab_key() -> u64 {
    NEXT_TAB_KEY.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct TabInfo {
    #[serde(default = "next_tab_key")]
    pub(crate) key: u64,
    #[serde(skip)]
    pub(crate) favicon: Option<iced::widget::image::Handle>,
    pub(crate) url: String,
    pub(crate) title: String,
    pub(crate) pinned: bool,
    pub(crate) group: Option<usize>,
    #[serde(default)]
    pub(crate) container: Option<u64>,
}
pub(crate) struct SleepingTab {
    pub(crate) pdf: Option<crate::pdf::Page>,
    pub(crate) host: RefCell<WebViewService>,
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Group {
    pub(crate) name: String,
    pub(crate) color: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum TabStripEntry {
    Tab(u64),
    Group(usize),
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Saved {
    pub(crate) tabs: Vec<TabInfo>,
    pub(crate) active: usize,
    pub(crate) layout: Layout,
    pub(crate) right: bool,
    pub(crate) bookmarks: Vec<TabInfo>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub(crate) bookmark_folders: std::collections::BTreeMap<String, Vec<String>>,
    #[serde(skip_serializing)]
    pub(crate) history: Vec<crate::history::Visit>,
    pub(crate) groups: Vec<Group>,
    pub(crate) tab_order: Vec<TabStripEntry>,
    pub(crate) downloads: Vec<crate::downloads::Archive>,
    pub(crate) containers: Vec<crate::profiles::Container>,
    pub(crate) search: crate::search::Search,
    pub(crate) local_sites: Vec<local::LocalSite>,
    pub(crate) site_permissions: Vec<crate::site_permissions::Choice>,
    pub(crate) middle_autoscroll: bool,
    pub(crate) external_pdfs: bool,
    pub(crate) startup: crate::startup::Mode,
    pub(crate) startup_page: String,
    pub(crate) notifications: crate::website_notifications::Policy,
    pub(crate) welcome_version: Option<u32>,
    pub(crate) privacy: crate::privacy::Privacy,
    /// The zoom chosen for a site, by host and port; 100 is not kept.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub(crate) site_zoom: std::collections::BTreeMap<String, i32>,
    /// The folder downloads are saved to; `None` for the default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) download_folder: Option<String>,
    /// Whether each download asks where it is saved.
    pub(crate) ask_download_place: bool,
    /// Whether the bookmarks bar is shown under the toolbar.
    pub(crate) bookmarks_bar: bool,
    /// Immersive view: the page fills the window inside a thin frame, and
    /// the top bar and the tab sidebar slide over it when the pointer reaches
    /// their edge.
    pub(crate) immersive: bool,
    /// What the immersive bars are made of.
    pub(crate) immersive_style: immersive::ImmersiveStyle,
    /// Whether spelling is checked in what is typed into pages.
    pub(crate) spell_check: bool,
    /// The languages to check in; empty for the desktop's own.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) spell_languages: Vec<String>,
    pub(crate) assistant: crate::assistant::Settings,
    /// How readily hidden tabs give back memory.
    #[serde(default)]
    pub(crate) memory_mode: crate::webview_native::MemoryMode,
}
impl Default for Saved {
    fn default() -> Self {
        Self {
            tabs: Vec::new(),
            active: 0,
            layout: Layout::default(),
            right: false,
            bookmarks: Vec::new(),
            bookmark_folders: Default::default(),
            history: Vec::new(),
            groups: Vec::new(),
            tab_order: Vec::new(),
            downloads: Vec::new(),
            containers: crate::profiles::defaults(),
            search: crate::search::Search::default(),
            local_sites: Vec::new(),
            site_permissions: Vec::new(),
            middle_autoscroll: false,
            external_pdfs: false,
            startup: Default::default(),
            startup_page: String::new(),
            notifications: Default::default(),
            welcome_version: None,
            privacy: Default::default(),
            site_zoom: Default::default(),
            download_folder: None,
            ask_download_place: false,
            bookmarks_bar: false,
            immersive: false,
            immersive_style: immersive::ImmersiveStyle::default(),
            spell_check: true,
            spell_languages: Vec::new(),
            assistant: Default::default(),
            memory_mode: Default::default(),
        }
    }
}
pub(crate) struct Browser {
    pub(crate) remote_search: crate::search_suggestions::State,
    pub(crate) credentials: credentials_ui::State,
    pub(crate) appearance: appearance_ui::State,
    pub(crate) desktop_notifications: crate::website_notifications::State,
    pub(crate) importing: crate::imports::State,
    pub(crate) container_routes: std::collections::BTreeMap<u64, String>,
    /// Each proxied container's proxy password, as first read from the
    /// desktop keyring (or saved) this session; the route it signs in on
    /// stays as it was until the browser restarts.
    pub(crate) proxy_passwords:
        std::cell::RefCell<std::collections::BTreeMap<u64, crate::credentials::Secret>>,
    pub(crate) input: crate::browser_input::State,
    pub(crate) pdf_cache: std::sync::Arc<crate::pdf::Cache>,
    pub(crate) pdf_prefetch: Option<(u64, crate::browser_pdf::Request)>,
    pub(crate) pdf_external_error: Option<(u64, String)>,
    pub(crate) pdf_requests: std::collections::BTreeMap<u64, crate::browser_pdf::Request>,
    pub(crate) favicons: crate::favicons::Cache,
    pub(crate) maintenance: Vec<containers::Cleanup>,
    pub(crate) container_edit: Option<u64>,
    pub(crate) container_editor_open: bool,
    pub(crate) container_policy: crate::container_privacy::Overrides,
    /// A proxy password typed into the container editor and not yet saved.
    pub(crate) container_proxy_password: crate::credentials::Secret,
    pub(crate) container_agent_custom: bool,
    pub(crate) container_name: String,
    pub(crate) container_color: usize,
    pub(crate) container_icon: usize,
    pub(crate) container_menu_model: enzui_core::menu_model::MenuModel,
    pub(crate) container_menu_state: enzui_core::menu_model::MenuState,
    pub(crate) suggestion_highlight: Option<usize>,
    pub(crate) address_suggestions_open: bool,
    pub(crate) start_suggestions_open: bool,
    pub(crate) address_suggestion_highlight: Option<usize>,
    pub(crate) local_grants: Vec<(Option<u64>, String)>,
    pub(crate) container_notice: String,
    pub(crate) new_tab_container: Option<u64>,
    pub(crate) history_store: crate::history::Store,
    pub(crate) history_ready: bool,
    pub(crate) history_filter: String,
    pub(crate) history_range: crate::history::Range,
    pub(crate) history_page: usize,
    pub(crate) history_total: usize,
    pub(crate) history_matches: Vec<crate::history::Visit>,
    pub(crate) history_cleanup: Option<(String, i64, usize)>,
    pub(crate) export: export::ExportState,
    pub(crate) devtools: devtools::DevTools,
    pub(crate) cookies: cookies::CookieState,
    pub(crate) engine_name: String,
    pub(crate) engine_template: String,
    pub(crate) engine_edit: bool,
    pub(crate) engine_error: String,
    pub(crate) permission: Option<enzui_webkit::site_permissions::SitePermissionRequest>,
    /// Requests the person blocked or dismissed in this session; the same
    /// request is refused without asking until the browser restarts or the
    /// site's permissions change.
    pub(crate) permission_denied: Vec<crate::browser::permissions::Denial>,
    pub(crate) private: bool,
    pub(crate) private_profiles:
        std::collections::BTreeMap<Option<u64>, enzui_webkit::web::WebProfile>,
    pub(crate) chooser:
        enzui_widgets::file_chooser::ChooserSession<(u64, enzui_webkit::page::PageRequestId)>,
    pub(crate) context: enzui_widgets::context_menu_host::ContextMenu<usize, Command>,
    pub(crate) menu_model: enzui_core::menu_model::MenuModel,
    pub(crate) menu_state: enzui_core::menu_model::MenuState,
    pub(crate) zoom_menu_model: enzui_core::menu_model::MenuModel,
    pub(crate) zoom_menu_state: enzui_core::menu_model::MenuState,
    pub(crate) zoom_menu_key: (i32, bool),
    pub(crate) pointer: iced::Point,
    pub(crate) viewport: iced::Size,
    pub(crate) saved: Saved,
    pub(crate) sleeping: Vec<Option<SleepingTab>>,
    pub(crate) panel: Panel,
    pub(crate) library: Option<InternalPage>,
    pub(crate) query: String,
    pub(crate) bookmark_query: String,
    pub(crate) bookmarks_limit: usize,
    pub(crate) tabs_open: bool,
    /// The immersive bars: whether each is out and where the pointer is.
    pub(crate) immersive: immersive::ImmersiveState,
    /// Tabs just opened, by key, with when: their row makes an entrance.
    pub(crate) tab_entrances: std::collections::HashMap<u64, std::time::Instant>,
    pub(crate) closed: Vec<TabInfo>,
    pub(crate) zoom: i32,
    /// Operation results and failures, shown bottom-right.
    pub(crate) toasts: toasts::Toasts,
    pub(crate) options: Option<enzui_webkit::page::OptionMenuRequest>,
    pub(crate) dialog: Option<enzui_webkit::page::ScriptDialogRequest>,
    pub(crate) prompt: String,
    pub(crate) sign_in: Option<prompts::SignIn>,
    /// The choice of the list on show that the keyboard is on.
    pub(crate) option_highlight: Option<usize>,
    pub(crate) fullscreen: fullscreen::Fullscreen,
    pub(crate) print: print::PrintState,
    pub(crate) bookmark_edit: Option<bookmarks::BookmarkEdit>,
    /// The folder of the bookmarks bar whose list is open.
    pub(crate) bar_open: Option<String>,
    /// The download whose place is being asked for.
    pub(crate) download_ask: Option<u64>,
    /// The tabs made around windows pages opened, each with the tab that
    /// opened it.
    pub(crate) popup_openers: std::collections::BTreeMap<u64, u64>,
    /// The colour field being answered, and the colour chosen so far.
    pub(crate) color: Option<color::Choosing>,
    /// The date or time field whose picker is open.
    pub(crate) date: Option<dates::DateChoosing>,
    /// Where the wave on a sounding tab is in its movement.
    pub(crate) sound_beat: usize,
    /// The tabs whose pages are sounding, and those the person silenced.
    pub(crate) audible: std::collections::BTreeSet<u64>,
    pub(crate) muted: std::collections::BTreeSet<u64>,
    /// The tab whose page raised the dialog, list or sign-in on show.
    pub(crate) prompt_tab: Option<u64>,
    pub(crate) pdf: Option<crate::pdf::Page>,
    pub(crate) downloads: crate::downloads::Store,
    pub(crate) download_hosts: Vec<RefCell<WebViewService>>,
    pub(crate) find: String,
    pub(crate) find_case: bool,
    pub(crate) find_status: String,
    pub(crate) chrome_revision: u64,
}
#[cfg(test)]
pub(crate) fn state_path() -> std::path::PathBuf {
    std::env::var_os("ENZIM_BROWSER_TEST_PROFILE")
        .map_or_else(
            || std::env::temp_dir().join(format!("enzim-browser-unit-{}", std::process::id())),
            std::path::PathBuf::from,
        )
        .join("session.json")
}
#[cfg(not(test))]
pub(crate) fn state_path() -> std::path::PathBuf {
    crate::browser_profile::paths().config.join("session.json")
}
#[cfg(test)]
fn resolve(input: &str) -> String {
    crate::search::Search::default().resolve(input)
}
impl Browser {
    pub(crate) fn load() -> Self {
        Self::load_with_privacy(std::env::args().any(|arg| arg == "--private-window"))
    }
    pub(crate) fn load_with_privacy(private: bool) -> Self {
        #[cfg(test)]
        let mut saved = Saved::default();
        #[cfg(not(test))]
        let mut saved: Saved = std::fs::read(state_path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        if saved.welcome_version.is_none() {
            saved.welcome_version = Some(if state_path().exists() { 1 } else { 0 });
        }
        #[cfg(not(test))]
        crate::downloads::set_folder(saved.download_folder.as_ref().map(std::path::PathBuf::from));
        crate::startup::apply(&mut saved, private);
        saved.assistant.migrate();
        crate::webview_native::set_memory_mode(saved.memory_mode);
        if private {
            saved.site_permissions.clear();
            saved.local_sites.clear();
            saved.tabs.clear();
            saved.history.clear();
            saved.groups.clear();
            saved.tab_order.clear();
            saved.downloads.clear();
            saved.active = 0;
        }
        if saved.tabs.is_empty() {
            saved.tabs.push(TabInfo {
                key: next_tab_key(),
                favicon: None,
                url: String::new(),
                title: "New tab".into(),
                pinned: false,
                group: None,
                container: None,
            });
        }
        saved.active = saved.active.min(saved.tabs.len() - 1);
        let library = saved.migrate_library_tabs();
        NEXT_TAB_KEY.fetch_max(
            saved.tabs.iter().map(|t| t.key).max().unwrap_or(0) + 1,
            std::sync::atomic::Ordering::Relaxed,
        );
        let sleeping = (0..saved.tabs.len()).map(|_| None).collect();
        let history_path = state_path().with_file_name("history.sqlite");
        let mut history_store = crate::history::Store::open(if private || cfg!(test) {
            None
        } else {
            Some(&history_path)
        });
        let migration = history_store.migrate(&saved.history);
        let history_ready = migration.is_ok();
        let mut toasts = toasts::Toasts::default();
        if let Err(e) = migration {
            toasts.error("history", format!("History could not be opened: {e}"));
        }
        if history_ready {
            saved.history = history_store.query("", 0).unwrap_or_default();
        }
        let downloads = crate::downloads::Store::from_archive(&saved.downloads);
        Self {
            remote_search: Default::default(),
            credentials: Default::default(),
            appearance: Default::default(),
            desktop_notifications: Default::default(),
            importing: Default::default(),
            container_routes: saved
                .containers
                .iter()
                .map(|entry| (entry.id, crate::container_privacy::route(&entry.privacy)))
                .collect(),
            proxy_passwords: Default::default(),
            input: Default::default(),
            pdf_requests: Default::default(),
            pdf_cache: Default::default(),
            pdf_prefetch: None,
            pdf_external_error: None,
            history_store,
            favicons: crate::favicons::Cache::new(private),
            history_ready,
            maintenance: Vec::new(),
            container_edit: None,
            container_editor_open: false,
            container_policy: Default::default(),
            container_proxy_password: Default::default(),
            container_agent_custom: false,
            container_name: String::new(),
            container_color: 0,
            container_icon: 0,
            container_menu_model: containers::container_menu_model(&saved.containers, None, true),
            container_menu_state: enzui_core::menu_model::MenuState::closed(),
            suggestion_highlight: None,
            address_suggestions_open: false,
            start_suggestions_open: false,
            address_suggestion_highlight: None,
            local_grants: Vec::new(),
            container_notice: String::new(),
            new_tab_container: None,
            history_filter: String::new(),
            history_range: crate::history::Range::All,
            history_page: 0,
            history_total: 0,
            history_matches: Vec::new(),
            history_cleanup: None,
            export: Default::default(),
            devtools: Default::default(),
            cookies: Default::default(),
            engine_name: String::new(),
            engine_template: String::new(),
            engine_edit: false,
            engine_error: String::new(),
            permission: None,
            permission_denied: Vec::new(),
            private,
            private_profiles: Default::default(),
            chooser: enzui_widgets::file_chooser::ChooserSession::new().with_fallback(),
            context: Default::default(),
            menu_model: menu_model(&saved.containers),
            menu_state: enzui_core::menu_model::MenuState::closed(),
            zoom_menu_model: zoom::preset_model(100, false),
            zoom_menu_state: enzui_core::menu_model::MenuState::closed(),
            zoom_menu_key: (100, false),
            pointer: iced::Point::ORIGIN,
            viewport: iced::Size::new(1400.0, 900.0),
            saved,
            sleeping,
            panel: Panel::None,
            library,
            query: String::new(),
            bookmark_query: String::new(),
            bookmarks_limit: 200,
            tabs_open: false,
            immersive: immersive::ImmersiveState::default(),
            tab_entrances: std::collections::HashMap::new(),
            closed: vec![],
            zoom: 100,
            toasts,
            downloads,
            download_hosts: Vec::new(),
            pdf: None,
            options: None,
            dialog: None,
            prompt: String::new(),
            sign_in: None,
            option_highlight: None,
            prompt_tab: None,
            fullscreen: fullscreen::Fullscreen::default(),
            print: print::PrintState::default(),
            bookmark_edit: None,
            bar_open: None,
            download_ask: None,
            popup_openers: std::collections::BTreeMap::new(),
            color: None,
            date: None,
            sound_beat: 0,
            audible: std::collections::BTreeSet::new(),
            muted: std::collections::BTreeSet::new(),
            find: String::new(),
            find_case: false,
            find_status: String::new(),
            chrome_revision: 0,
        }
    }
    pub(crate) fn save(&mut self) {
        if cfg!(test) {
            return;
        }
        self.normalize_tab_order();
        self.saved.downloads = self.downloads.archive();
        self.save_to(&state_path());
    }
    fn save_to(&mut self, path: &std::path::Path) {
        if self.private || !self.history_ready {
            return;
        }
        let result = (|| -> std::io::Result<()> {
            let parent = path
                .parent()
                .ok_or_else(|| std::io::Error::other("The session path has no parent."))?;
            std::fs::create_dir_all(parent)?;
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
            let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
            std::fs::write(&tmp, serde_json::to_vec_pretty(&self.saved)?)?;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
            std::fs::rename(tmp, path)
        })();
        if let Err(e) = result {
            self.toasts
                .error("session", format!("Could not save session: {e}"));
        }
    }
}
impl App {
    pub(crate) fn browser_command(&mut self, cmd: Command) -> Task<Message> {
        use Command::*;
        if self.browser.library.is_some()
            && matches!(
                &cmd,
                Print
                    | Export(_)
                    | Dev(_)
                    | Zoom(_)
                    | ZoomTo(_)
                    | MenuZoom(_)
                    | Bookmark
                    | CopySelection
                    | Panel(crate::browser::Panel::Find)
                    | Find(_)
                    | FindCase
                    | FindNext(_)
            )
        {
            return Task::none();
        }
        if matches!(&cmd, Escape) {
            let close = enzui_widgets::menu_button::MenuButtonEvent::Close(
                enzui_core::menu_model::CloseCause::Escape,
            );
            if self.browser.menu_state.is_open() {
                return self.browser_command(Menu(close));
            }
            if self.browser.zoom_menu_state.is_open() {
                return self.browser_command(ZoomMenu(close));
            }
            if self.browser.credentials.inline_open() {
                return self.browser_command(Credential(CredentialCommand::DismissInline));
            }
        }
        let order_changed = matches!(
            &cmd,
            Pin(_) | Group(_) | NewGroup | Ungroup(_) | AssignGroup(_, _) | RemoveGroup(_)
        );
        // Immersive view's pointer tracking, its sidebar's edge and hover,
        // is not something the person did to a menu.
        let background_update = matches!(
            &cmd,
            MaintenancePoll
                | SearchSuggestionsReady(..)
                | Notifications(crate::website_notifications::Command::Wake)
        ) || matches!(&cmd, Credential(command) if command.is_background_update())
            || matches!(&cmd, Immersive(command) if command.is_pointer_tracking());
        if !background_update
            && !matches!(
                cmd,
                Menu(_)
                    | MenuZoom(_)
                    | MenuZoomHover
                    | Pointer(_)
                    | Viewport(_)
                    | Context(_)
                    | Appearance(AppearanceCommand::AddressHover(_))
            )
        {
            self.browser
                .menu_state
                .close(enzui_core::menu_model::CloseCause::Escape);
        }
        if !background_update
            && !matches!(
                cmd,
                ZoomMenu(_)
                    | Pointer(_)
                    | Viewport(_)
                    | Appearance(AppearanceCommand::AddressHover(_))
            )
        {
            self.browser
                .zoom_menu_state
                .close(enzui_core::menu_model::CloseCause::Escape);
        }
        let mut task = Task::none();
        match cmd {
            ZoomMenu(event) => return self.zoom_menu_event(event),
            ZoomTo(percent) => return self.apply_page_zoom(percent),
            MenuZoomHover => {
                self.clear_browser_menu_hover();
                return Task::none();
            }
            MenuZoom(percent) => {
                self.clear_browser_menu_hover();
                return self.apply_page_zoom(percent);
            }
            Input(command) => return self.input_command(command),
            Pdf(command) => return self.pdf_command(command),
            Download(command) => return self.download_command(command),
            Export(command) => return self.export_command(command),
            Dev(command) => return self.dev_command(command),
            Network(command) => return self.network_command(command),
            Inspect(command) => return self.inspector_command(command),
            OpenLocal => return self.open_local_site(),
            Credential(command) => return self.credential_command(command),
            Prompt(command) => return self.prompt_command(command),
            Action(command) => return self.action_command(command),
            Preference(command) => return self.preference_command(command),
            Color(command) => return self.color_command(command),
            Date(command) => return self.date_command(command),
            Spelling(command) => return self.spelling_command(command),
            PrintJob(command) => return self.print_command(command),
            Bookmarks(command) => return self.bookmark_command(command),
            Fullscreen => return self.fullscreen_toggle(),
            TrustCertificate => {
                let host = match &self.webview.borrow().native_worker_error {
                    Some(crate::webview_native::NativeWorkerError::Certificate { host }) => {
                        Some(host.clone())
                    }
                    _ => None,
                };
                if let Some(host) = host {
                    self.submit_browser_action(BrowserAction::TrustCertificate(host));
                    return Task::done(Message::BrowserGo);
                }
            }
            Appearance(command) => return self.appearance_command(command),
            Permission(command) => return self.permission_command(command),
            Notifications(command) => return self.notification_command(command),
            Welcome(command) => return self.welcome_command(command),
            Shields(command) => return self.shield_command(command),
            Cookies(command) => return self.cookie_command(command),
            Privacy(command) => return self.privacy_command(command),
            Container(command) => return self.container_command(command),
            Import(command) => return self.import_command(command),
            ChooseProfile => {
                if let Err(error) = crate::browser_profile::open_chooser() {
                    self.browser.toasts.error("profiles", error);
                }
            }
            MaintenancePoll => return Task::none(),
            SoundBeat => {
                self.sound_beat();
                return Task::none();
            }
            Toast(event) => {
                self.browser.toasts.update(event);
                return Task::none();
            }
            Window(command) => {
                use enzui_widgets::title_bar_gesture::WindowCommand as W;
                if self.immersive() && matches!(command, W::Drag | W::Resize(_)) {
                    self.browser.immersive.hold_for_move();
                }
                task = if let Some(id) = self.window_id {
                    command.into_task(id)
                } else {
                    iced::window::latest().and_then(move |id| command.into_task(id))
                };
            }
            ChooserWake => {
                return self
                    .browser
                    .chooser
                    .pump(|tag, outcome| c(Command::FilesChosen(tag, outcome)));
            }
            ChooserEvent(event) => {
                return self
                    .browser
                    .chooser
                    .handle(event, |tag, outcome| c(Command::FilesChosen(tag, outcome)));
            }
            FilesChosen((slot, id), outcome) => {
                let paths = if let enzui_core::portal::PortalOutcome::Completed(result) = outcome {
                    result.local_paths()
                } else {
                    vec![]
                };
                if slot == u64::MAX - 4 || slot == u64::MAX - 5 {
                    return self.credential_command(CredentialCommand::FileChosen(
                        id.get(),
                        paths,
                        slot == u64::MAX - 5,
                    ));
                }
                if slot == u64::MAX - 3 {
                    return self.import_folder_chosen(paths);
                }
                if slot == crate::assistant::PICTURES_TAG {
                    return self.assistant_update(crate::assistant::Msg::PicturesChosen(paths));
                }
                if slot == bookmarks::EXPORT_TAG {
                    return self.bookmarks_export_chosen(paths);
                }
                if slot == preferences::FOLDER_TAG {
                    self.download_folder_chosen(paths);
                    return task;
                }
                if slot == save_place::PLACE_TAG {
                    return self.download_place_chosen(id.get(), paths);
                }
                if slot == u64::MAX - 1 {
                    return self.export_file_chosen(id.get(), paths);
                }
                if slot == u64::MAX {
                    if let Some(path) = paths.first() {
                        return self.browser_command(PdfOpen(path.clone()));
                    }
                    return task;
                }
                let action = BrowserAction::Files(id, paths);
                if self.browser.saved.tabs[self.browser.saved.active].key == slot {
                    self.submit_browser_action(action);
                } else {
                    if let Some(index) = self.browser.saved.tabs.iter().position(|t| t.key == slot)
                        && let Some(tab) = &self.browser.sleeping[index]
                    {
                        let host = tab.host.borrow();
                        if let (Some(view), Some(native)) = (host.live_view, &host.native) {
                            let _ = native.browse(&host.router, view, action);
                        }
                    }
                }
            }
            Viewport(size) => {
                if self.browser.viewport != size {
                    self.browser.tabs_open = false;
                }
                self.browser.viewport = size;
                // A glass frame's blur follows the window's shape.
                return Task::batch([task, self.appearance_backdrop()]);
            }
            Pointer(point) => {
                self.browser.pointer = point;
                self.assistant_pointer(point);
                if self.browser.devtools.resizing {
                    self.browser.devtools.height = Some(
                        (self.browser.viewport.height - point.y - 10.)
                            .clamp(190., (self.browser.viewport.height - 140.).max(190.)),
                    );
                    self.browser.chrome_revision = self.browser.chrome_revision.wrapping_add(1);
                }
                return task;
            }
            Stop => {
                self.pending_navigation = None;
                self.pending_native_open = false;
                if let Some(location) = self.webview.borrow().location.clone() {
                    self.address.clone_from(&location);
                    self.browser.saved.tabs[self.browser.saved.active].url = location;
                    self.address_dirty = false;
                }
                self.browser
                    .replace_tab_key(self.browser.saved.active, next_tab_key());
                self.webview.borrow_mut().loading = false;
                self.submit_browser_action(BrowserAction::Stop);
            }
            Reload => self.submit_browser_action(BrowserAction::Reload),
            CopyText(text) => task = iced::clipboard::write(text).map(|_| c(Command::Dismiss)),
            CopySelection => return self.update(Message::WebViewNativeCopy),
            Paste => {
                task = self.request_web_paste(false);
            }
            Edit(command) => self.submit_browser_action(BrowserAction::Script(format!(
                "document.execCommand('{command}')"
            ))),
            Menu(event) => {
                if matches!(event, enzui_widgets::menu_button::MenuButtonEvent::Open(_)) {
                    self.browser.menu_model = menu_model(&self.browser.saved.containers);
                    self.refresh_library_menu();
                    self.browser.panel = crate::browser::Panel::None;
                }
                if let Some(operation) = enzui_widgets::menu_button::focus_restore_operation(
                    &event,
                    Some(&iced::advanced::widget::Id::new("browser-menu")),
                ) {
                    task = iced::advanced::widget::operate(operation);
                }
                if let Some(id) = enzui_widgets::menu_button::apply_event(
                    &mut self.browser.menu_model,
                    &mut self.browser.menu_state,
                    &event,
                ) {
                    let command = if id.as_str() == "containers-manage" {
                        Some(Command::Container(ContainerCommand::Open))
                    } else if let Some(value) = id.as_str().strip_prefix("container-new:") {
                        value
                            .parse()
                            .ok()
                            .map(|id| Command::Container(ContainerCommand::NewTab(Some(id))))
                    } else {
                        menu_commands()
                            .into_iter()
                            .find(|(key, ..)| *key == id.as_str())
                            .map(|(_, _, _, _, command)| command)
                    };
                    if let Some(command) = command {
                        return Task::batch([task, self.browser_command(command)]);
                    }
                }
                return task;
            }
            Context(event) => {
                let (choice, focus) = self.browser.context.update(&event);
                if let Some((_, command)) = choice {
                    return Task::batch([focus, self.browser_command(command)]);
                }
                return focus;
            }
            TabContext(index, point) => {
                let items =
                    tab_context_items(&self.browser, index, self.look().palette.is_dark());
                self.open_context(index, point, items);
            }
            AssignGroup(index, group) => {
                self.browser.saved.tabs[index].group = Some(group);
                self.browser.saved.tabs[index].pinned = false;
            }
            GroupContext(group, point) => {
                if group < self.browser.saved.groups.len() {
                    use enzui_widgets::context_menu_host::ContextMenuItem as I;
                    self.open_context(
                        self.browser.saved.active,
                        point,
                        vec![
                            I::command(
                                Panel(crate::browser::Panel::Group(group)),
                                "Tab group settings",
                            ),
                            I::command(RemoveGroup(group), "Ungroup"),
                            I::command(CloseGroup(group), "Close group"),
                            I::separator(),
                            I::command(New, "New tab"),
                            I::command(NewGroup, "New group"),
                        ],
                    );
                }
            }
            ChromeContext(point) => {
                // A website answers a right-click in it itself: with the
                // page's menu here, or with a menu of the site's own. The
                // browser's menu is for the bars and its own pages.
                if self.over_website(point) {
                    return Task::none();
                }
                let items = chrome_context_items(
                    &self.browser.saved.containers,
                    self.look().palette.is_dark(),
                );
                self.open_context(self.browser.saved.active, point, items);
            }
            AnswerOption(index) => {
                if let Some(options) = self.browser.options.take() {
                    self.submit_browser_action(BrowserAction::Option(options.id, Some(index)));
                }
                self.browser.prompt_tab = None;
                self.browser.panel = crate::browser::Panel::None;
            }
            PromptText(text) => self.browser.prompt = text,
            AnswerDialog(accept) => {
                if let Some(dialog) = self.browser.dialog.take() {
                    let answer = if !accept {
                        enzui_webkit::page::ScriptDialogAnswer::Dismiss
                    } else if matches!(
                        dialog.kind,
                        enzui_webkit::page::ScriptDialogKind::Prompt { .. }
                    ) {
                        enzui_webkit::page::ScriptDialogAnswer::Text(self.browser.prompt.clone())
                    } else {
                        enzui_webkit::page::ScriptDialogAnswer::Accept
                    };
                    self.submit_browser_action(BrowserAction::Dialog(dialog.id, answer));
                }
                self.browser.prompt_tab = None;
                self.browser.panel = crate::browser::Panel::None;
            }
            CloseActive => return self.browser_command(Close(self.browser.saved.active)),
            Cycle(back) => {
                let n = self.browser.saved.tabs.len();
                let i = (self.browser.saved.active + if back { n - 1 } else { 1 }) % n;
                self.select_tab(i);
            }
            ToggleTabs => self.browser.tabs_open = !self.browser.tabs_open,
            Immersive(command) => return self.immersive_command(command),
            DropTabs(request) => self.apply_tab_drop(request),
            DropVisibleTabs(request, keys) => self.visible_tab_drop(request, &keys),
            TopTabsMenu => {
                use enzui_widgets::context_menu_host::ContextMenuItem as I;
                let items = self
                    .browser
                    .saved
                    .tabs
                    .iter()
                    .enumerate()
                    .map(|(i, tab)| {
                        let label = tab.group.map_or_else(
                            || tab.title.clone(),
                            |g| format!("{} · {}", self.browser.saved.groups[g].name, tab.title),
                        );
                        I::command(Select(i), label).icon("globe")
                    })
                    .collect();
                self.open_context(
                    self.browser.saved.active,
                    iced::Point::new(self.browser.viewport.width - 76., 86.),
                    items,
                );
            }
            BookmarkSearch(query) => {
                self.browser.bookmark_query = query;
                self.browser.bookmarks_limit = 200;
            }
            BookmarkMore => {
                self.browser.bookmarks_limit = self.browser.bookmarks_limit.saturating_add(200);
            }
            RemoveBookmark(url) => {
                self.browser.saved.bookmarks.retain(|t| t.url != url);
                self.browser.saved.bookmark_folders.remove(&url);
            }
            Navigate(url) => {
                self.browser.address_suggestions_open = false;
                self.browser.address_suggestion_highlight = None;
                self.address = url;
                return self.update(Message::BrowserGo);
            }
            StartGo => {
                if let Some(url) = self.selected_new_tab_target() {
                    self.browser.suggestion_highlight = None;
                    return self.browser_command(Navigate(url));
                }
            }
            Startup(index) => {
                self.browser.saved.startup = match index {
                    1 => crate::startup::Mode::Empty,
                    2 => crate::startup::Mode::Page,
                    _ => crate::startup::Mode::Restore,
                }
            }
            StartupPage(value) => self.browser.saved.startup_page = value,
            Modal(event) => {
                if let Some(op) = enzui_widgets::modal_host::scoped_focus_operation(
                    event,
                    iced::advanced::widget::Id::new("browser-modal"),
                ) {
                    task = iced::advanced::widget::operate(op);
                } else {
                    return self.browser_command(Dismiss);
                }
            }
            New => {
                self.browser.address_suggestions_open = false;
                self.browser.address_suggestion_highlight = None;
                self.browser.new_tab_container =
                    self.browser.saved.tabs[self.browser.saved.active].container;
                self.browser.tabs_open = false;
                self.browser.panel = crate::browser::Panel::New;
                self.browser.query.clear();
                self.browser.suggestion_highlight = None;
                self.browser.container_menu_state = enzui_core::menu_model::MenuState::closed();
                self.browser.container_menu_model = containers::container_menu_model(
                    &self.browser.saved.containers,
                    self.browser.new_tab_container,
                    self.look().palette.is_dark(),
                );
                task = focus("new-address");
            }
            Escape => {
                if self.browser.date.take().is_some() {
                    self.browser.chrome_revision = self.browser.chrome_revision.wrapping_add(1);
                    return Task::none();
                }
                self.address_preparation_until = None;
                if self.browser.address_suggestions_open {
                    return self.browser_command(AddressSuggestionsDismiss);
                }
                if self.browser.start_suggestions_open
                    && self.browser.panel == crate::browser::Panel::None
                {
                    self.browser.start_suggestions_open = false;
                    self.browser.suggestion_highlight = None;
                    return focus("start-search");
                }
                if self.browser.library.is_some()
                    && self.browser.panel == crate::browser::Panel::None
                {
                    self.close_library();
                    self.resume_selected_document();
                    return Task::none();
                }
                if self.browser.input.anchor.take().is_some() {
                    self.browser.chrome_revision = self.browser.chrome_revision.wrapping_add(1);
                    return Task::none();
                }
                if self.browser.devtools.inspector.picking {
                    self.cancel_inspector_picker();
                    return Task::none();
                }
                if self.browser.panel == crate::browser::Panel::None
                    && let Some(task) = self.fullscreen_escape()
                {
                    return task;
                }
                if self.browser.panel == crate::browser::Panel::None
                    && self.webview.borrow().loading
                {
                    return self.browser_command(Stop);
                }
                return self.browser_command(Dismiss);
            }
            AddressSuggestionsDismiss => {
                self.browser.address_suggestions_open = false;
                self.browser.address_suggestion_highlight = None;
            }
            Dismiss => {
                self.browser.date = None;
                self.address_preparation_until = None;
                if self.assistant.draft.take().is_some() || self.assistant.prompt.take().is_some() {
                    return Task::none();
                }
                if self.browser.engine_edit {
                    self.browser.engine_edit = false;
                    self.browser.engine_error.clear();
                    return Task::none();
                }
                if self.browser.panel == crate::browser::Panel::Passwords {
                    self.browser.credentials.dismiss();
                }
                if self.browser.panel == crate::browser::Panel::Passkey {
                    self.passkey_cancel();
                }
                if self.browser.panel == crate::browser::Panel::Cookies {
                    self.browser.cookies.dialog = false;
                }
                if self.browser.panel == crate::browser::Panel::CookieClear
                    && self.browser.cookies.dialog
                {
                    self.browser.panel = crate::browser::Panel::Cookies;
                    return Task::none();
                }
                if self.browser.panel == crate::browser::Panel::New {
                    self.browser.query.clear();
                    self.browser.start_suggestions_open = false;
                    self.browser.suggestion_highlight = None;
                }
                self.browser.address_suggestions_open = false;
                if self.browser.panel == crate::browser::Panel::Permissions
                    && let Some(request) = self.browser.permission.take()
                {
                    self.deny_permission(&request);
                    self.submit_browser_action(BrowserAction::PermissionAnswer(request.id, false));
                }
                if self.browser.panel == crate::browser::Panel::Find {
                    self.submit_browser_action(BrowserAction::FindFinish);
                }
                self.browser.tabs_open = false;
                if self.browser.panel == crate::browser::Panel::Script {
                    return self.browser_command(AnswerDialog(false));
                }
                if self.browser.panel == crate::browser::Panel::SignIn {
                    return self.prompt_command(PromptCommand::SignIn(false));
                }
                if self.browser.panel == crate::browser::Panel::Options
                    && let Some(options) = self.browser.options.take()
                {
                    self.browser.prompt_tab = None;
                    self.submit_browser_action(BrowserAction::Option(options.id, None));
                }
                self.browser.panel = crate::browser::Panel::None;
            }
            Query(q) => {
                self.browser.start_suggestions_open =
                    self.browser.panel != crate::browser::Panel::New;
                self.browser.query = q;
                self.browser.suggestion_highlight = None;
            }
            AddressSuggestionMove(back) => {
                let n = self.suggestions_for(&self.address).len();
                self.browser.address_suggestion_highlight =
                    pages::next_suggestion(self.browser.address_suggestion_highlight, n, back);
                if let Some(i) = self.browser.address_suggestion_highlight {
                    task = iced::widget::operation::snap_to(
                        iced::widget::Id::new("address-suggestions"),
                        iced::widget::scrollable::RelativeOffset {
                            x: 0.,
                            y: i as f32 / n.saturating_sub(1).max(1) as f32,
                        },
                        iced::widget::operation::Animation::Instant,
                    );
                }
            }
            SuggestionMove(back) => {
                self.browser.start_suggestions_open =
                    self.browser.panel != crate::browser::Panel::New;
                let n = self.new_tab_suggestions().len();
                self.browser.suggestion_highlight =
                    pages::next_suggestion(self.browser.suggestion_highlight, n, back);
                if let Some(i) = self.browser.suggestion_highlight {
                    task = iced::widget::operation::snap_to(
                        iced::widget::Id::new(
                            if self.browser.panel == crate::browser::Panel::New {
                                "new-tab-suggestions"
                            } else {
                                "start-page-suggestions"
                            },
                        ),
                        iced::widget::scrollable::RelativeOffset {
                            x: 0.,
                            y: i as f32 / n.saturating_sub(1).max(1) as f32,
                        },
                        iced::widget::operation::Animation::Instant,
                    );
                }
            }
            Submit => {
                if let Some(url) = self.selected_new_tab_target() {
                    self.new_tab_in_container(url, self.browser.new_tab_container);
                    self.browser.panel = crate::browser::Panel::None;
                }
            }
            ExternalVisit(url) => {
                let url = self.browser.saved.search.resolve(&url);
                if self.welcome.is_some() {
                    self.address.clone_from(&url);
                    self.browser.saved.tabs[self.browser.saved.active].url = url;
                    return Task::none();
                }
                self.new_tab_in_container(url, None);
            }
            ExternalVisitIn(url, container) => {
                let url = self.browser.saved.search.resolve(&url);
                self.new_tab_in_container(url, container);
            }
            Visit(url) => {
                if self.browser.panel == crate::browser::Panel::New {
                    self.new_tab_in_container(url, self.browser.new_tab_container);
                } else {
                    self.new_tab(url);
                }
                self.browser.panel = crate::browser::Panel::None;
            }
            Select(i) => self.select_tab(i),
            Close(i) => {
                if i < self.browser.saved.tabs.len() {
                    if self.browser.saved.tabs[i].pinned {
                        return task;
                    }
                    self.browser
                        .desktop_notifications
                        .tab_closed(self.browser.saved.tabs[i].key);
                    if self
                        .browser
                        .pdf_prefetch
                        .as_ref()
                        .is_some_and(|(key, _)| *key == self.browser.saved.tabs[i].key)
                    {
                        self.browser.pdf_prefetch = None;
                    }
                    self.browser
                        .pdf_requests
                        .remove(&self.browser.saved.tabs[i].key);
                    self.browser.input.anchor = None;
                    if self.browser.saved.tabs.len() == 1 {
                        self.browser.closed.push(self.browser.saved.tabs[i].clone());
                        self.address.clear();
                        self.browser.query.clear();
                        self.pending_navigation = None;
                        self.pending_native_open = false;
                        self.browser.replace_tab_key(i, next_tab_key());
                        self.pending_pdf = None;
                        self.browser.saved.tabs[i].url.clear();
                        self.browser.saved.tabs[i].title = "New tab".into();
                        self.browser.pdf = None;
                        self.browser.saved.tabs[i].favicon = None;
                        self.browser.saved.tabs[i].group = None;
                        let old = std::mem::replace(
                            &mut self.webview,
                            RefCell::new(WebViewService::new()),
                        );
                        self.retain_download_host(old);
                    } else {
                        if i == self.browser.saved.active {
                            self.select_tab(if i == 0 { 1 } else { i - 1 });
                        }
                        self.browser.closed.push(self.browser.saved.tabs.remove(i));
                        if let Some(old) = self.browser.sleeping.remove(i) {
                            self.retain_download_host(old.host);
                        }
                        if self.browser.saved.active > i {
                            self.browser.saved.active -= 1;
                        }
                    }
                }
                self.browser.panel = crate::browser::Panel::None;
            }
            Pin(i) => {
                if let Some(t) = self.browser.saved.tabs.get_mut(i) {
                    t.pinned = !t.pinned;
                    if t.pinned {
                        t.group = None;
                    }
                }
                self.browser.panel = crate::browser::Panel::None;
            }
            Layout(l) => self.browser.saved.layout = l,
            Right(r) => self.browser.saved.right = r,
            Panel(crate::browser::Panel::Passwords) => {
                return self.credential_command(CredentialCommand::OpenSite);
            }
            Panel(p) => {
                if self.browser.panel == crate::browser::Panel::Passwords {
                    self.browser.credentials.dismiss();
                }
                if self.browser.panel == crate::browser::Panel::Passkey {
                    self.passkey_cancel();
                }
                self.browser.address_suggestions_open = false;
                self.browser.address_suggestion_highlight = None;
                self.browser.tabs_open = false;
                if let Some(page) = match p {
                    crate::browser::Panel::Settings => Some(InternalPage::Settings),
                    crate::browser::Panel::Bookmarks => Some(InternalPage::Bookmarks),
                    crate::browser::Panel::History => Some(InternalPage::History),
                    _ => None,
                } {
                    if page == InternalPage::History {
                        self.refresh_history();
                    }
                    self.open_internal_page(page);
                    return task;
                }
                self.browser.panel = if self.browser.panel == p && p != crate::browser::Panel::Find
                {
                    crate::browser::Panel::None
                } else {
                    p
                };
                if self.browser.panel == crate::browser::Panel::Theme {
                    self.appearance_open();
                }
                if self.browser.panel != crate::browser::Panel::None {
                    self.suspend_webpage_keyboard();
                    task = iced::advanced::widget::operate(
                        iced::advanced::widget::operation::focusable::unfocus(),
                    );
                }
                if self.browser.panel == crate::browser::Panel::Find {
                    task = focus("page-find");
                }
                if self.browser.panel == crate::browser::Panel::Theme {
                    task = Task::batch([task, self.appearance_corners()]);
                }
            }
            Bookmark => {
                let tab = self.browser.saved.tabs[self.browser.saved.active].clone();
                if let Some(i) = self
                    .browser
                    .saved
                    .bookmarks
                    .iter()
                    .position(|t| t.url == tab.url)
                {
                    self.browser.saved.bookmarks.remove(i);
                    self.browser.saved.bookmark_folders.remove(&tab.url);
                } else if !tab.url.is_empty() {
                    self.browser.saved.bookmarks.push(tab);
                }
            }
            NewWindow | NewPrivateWindow => {
                self.new_window = Some((matches!(cmd, NewPrivateWindow), None));
                self.browser.panel = crate::browser::Panel::None;
            }
            Minimize => {
                task = if let Some(id) = self.window_id {
                    iced::window::minimize(id, true)
                } else {
                    iced::window::latest().and_then(|id| iced::window::minimize(id, true))
                }
            }
            Maximize => {
                task = if let Some(id) = self.window_id {
                    iced::window::toggle_maximize(id)
                } else {
                    iced::window::latest().and_then(iced::window::toggle_maximize)
                }
            }
            Exit => {
                self.browser.save();
                task = iced::exit();
            }
            Find(q) => {
                self.browser.find.clone_from(&q);
                self.browser.find_status.clear();
                self.submit_browser_action(BrowserAction::Find(q, self.browser.find_case));
            }
            FindCase => {
                self.browser.find_case = !self.browser.find_case;
                self.submit_browser_action(BrowserAction::Find(
                    self.browser.find.clone(),
                    self.browser.find_case,
                ));
            }
            FindNext(back) => {
                if self.browser.panel != crate::browser::Panel::Find
                    && !self.browser.find.is_empty()
                {
                    self.browser.panel = crate::browser::Panel::Find;
                    self.submit_browser_action(BrowserAction::Find(
                        self.browser.find.clone(),
                        self.browser.find_case,
                    ));
                    task = focus("page-find");
                } else {
                    self.submit_browser_action(BrowserAction::FindNext(back));
                }
            }
            Zoom(delta) => {
                self.browser.input.anchor = None;
                if self.browser.pdf.is_some() {
                    return self.pdf_command(crate::browser_pdf::PdfCommand::Zoom(delta));
                }
                let percent = if delta == 0 {
                    100
                } else {
                    (self.browser.zoom + delta).clamp(25, 300)
                };
                return self.apply_page_zoom(percent);
            }
            GroupDrop(request, target) => {
                self.group_tab_drop(request, target);
            }
            NewGroup => {
                let g = self.browser.saved.groups.len();
                self.browser.saved.groups.push(crate::browser::Group {
                    name: "New group".into(),
                    color: g % 8,
                });
                self.browser.normalize_tab_order();
                self.browser.panel = crate::browser::Panel::Group(g);
                task = focus("group-name");
            }
            Group(i) => {
                let mut order = self.browser.strip_order();
                let tab = &self.browser.saved.tabs[i];
                let anchor = TabStripEntry::Tab(tab.key);
                let at = order
                    .iter()
                    .position(|e| *e == anchor)
                    .or_else(|| {
                        tab.group.and_then(|g| {
                            order
                                .iter()
                                .position(|e| *e == TabStripEntry::Group(g))
                                .map(|i| i + 1)
                        })
                    })
                    .unwrap_or(order.len());
                order.retain(|e| *e != anchor);
                self.browser.saved.groups.push(crate::browser::Group {
                    name: "New group".into(),
                    color: 0,
                });
                let g = self.browser.saved.groups.len() - 1;
                self.browser.saved.tabs[i].group = Some(g);
                self.browser.saved.tabs[i].pinned = false;
                order.insert(at.min(order.len()), TabStripEntry::Group(g));
                self.browser.saved.tab_order = order;
                self.browser.panel = crate::browser::Panel::Group(g);
                task = focus("group-name");
            }
            GroupName(name) => {
                if let crate::browser::Panel::Group(g) = self.browser.panel {
                    self.browser.saved.groups[g].name = name;
                }
            }
            GroupColor(color) => {
                if let crate::browser::Panel::Group(g) = self.browser.panel {
                    self.browser.saved.groups[g].color = color % 8;
                }
            }
            RemoveGroup(g) => {
                self.browser.remove_group(g);
                self.browser.panel = crate::browser::Panel::None;
            }
            CloseGroup(g) => {
                if g < self.browser.saved.groups.len() {
                    self.browser.panel = crate::browser::Panel::None;
                    let doomed: Vec<bool> = self
                        .browser
                        .saved
                        .tabs
                        .iter()
                        .map(|tab| !tab.pinned && tab.group == Some(g))
                        .collect();
                    let task = self.close_tabs(move |i| doomed.get(i).copied().unwrap_or(false));
                    self.browser.remove_group(g);
                    self.browser.commit_tab_order();
                    if !cfg!(test) {
                        self.browser.save();
                    }
                    return task;
                }
            }
            Ungroup(i) => {
                let mut order = self.browser.strip_order();
                if let Some(group) = self.browser.saved.tabs[i].group {
                    let at = order
                        .iter()
                        .position(|e| *e == TabStripEntry::Group(group))
                        .map_or(order.len(), |i| i + 1);
                    order.insert(at, TabStripEntry::Tab(self.browser.saved.tabs[i].key));
                }
                self.browser.saved.tab_order = order;
                self.browser.saved.tabs[i].group = None;
                self.browser.panel = crate::browser::Panel::None;
            }
            Reopen => {
                if let Some(tab) = self.browser.closed.pop() {
                    self.new_tab(tab.url);
                }
            }
            Duplicate(i) => {
                let url = self.browser.saved.tabs[i].url.clone();
                self.new_tab(url);
                self.browser.panel = crate::browser::Panel::None;
            }
            CopyUrl => {
                if let Some(page) = self.browser.library {
                    return iced::clipboard::write(page.url().to_owned())
                        .map(|_| c(Command::MaintenancePoll));
                }
                task = iced::clipboard::write(
                    self.effective_privacy(self.browser.saved.active)
                        .clean_url(&self.browser.saved.tabs[self.browser.saved.active].url),
                )
                .map(|_| c(Command::Dismiss));
            }
            Print => return self.print_command(PrintCommand::Open),
            OpenPdf => {
                self.browser.panel = crate::browser::Panel::None;
                let _ = self.browser.chooser.start(
                    (u64::MAX, enzui_webkit::page::PageRequestId::new(0)),
                    enzui_core::portal::backend::PortalOperation::OpenFile,
                    enzui_core::portal::chooser::ChooserOptions::new()
                        .title("Open PDF")
                        .filter(
                            enzui_core::portal::chooser::FileFilter::new("PDF documents")
                                .glob("*.pdf")
                                .glob("*.PDF"),
                        )
                        .current_folder(if std::env::var_os("ENZIM_QA").is_some() {
                            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                                .join("tests/fixtures")
                        } else {
                            std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                                .join("Downloads")
                        }),
                );
            }
            PdfOpen(path) => {
                self.close_library();
                if !self.browser.saved.tabs[self.browser.saved.active]
                    .url
                    .is_empty()
                    || self.browser.pdf.is_some()
                {
                    self.new_tab(String::new());
                }
                return self.open_pdf(path, None);
            }
            PdfReady(key, result) => match result {
                Ok(page) => {
                    if self.browser.saved.tabs[self.browser.saved.active].key == key {
                        self.browser.saved.tabs[self.browser.saved.active].title = page
                            .path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned();
                        self.browser.pdf = Some(page);
                    } else if let Some(i) =
                        self.browser.saved.tabs.iter().position(|t| t.key == key)
                        && let Some(tab) = self.browser.sleeping[i].as_mut()
                    {
                        tab.pdf = Some(page);
                    }
                }
                Err(error) => self.browser.toasts.error("pdf", format!("PDF: {error}")),
            },
            PdfClose => {
                self.browser.pdf_prefetch = None;
                self.browser.pdf_external_error = None;
                self.browser
                    .pdf_requests
                    .remove(&self.browser.saved.tabs[self.browser.saved.active].key);
                self.browser.input.anchor = None;
                self.browser.pdf = None;
                if let Some(page) =
                    InternalPage::from_url(&self.browser.saved.tabs[self.browser.saved.active].url)
                {
                    self.browser.saved.tabs[self.browser.saved.active].title = page.title().into();
                }
            }
            HistoryFilter(query) => {
                self.browser.history_filter = query;
                self.browser.history_page = 0;
                self.refresh_history();
            }
            HistoryRange(range) => {
                self.browser.history_range = range;
                self.browser.history_page = 0;
                self.refresh_history();
            }
            HistoryPage(step) => {
                self.browser.history_page = self
                    .browser
                    .history_page
                    .saturating_add_signed(step as isize)
                    .min(self.browser.history_total.saturating_sub(1) / 100);
                self.refresh_history();
            }
            HistoryDelete(id) => {
                if let Err(e) = self.browser.history_store.delete(id) {
                    self.browser.toasts.error("history", e);
                }
                self.refresh_history();
            }
            HistoryClearRequest => {
                self.browser.history_cleanup = Some((
                    self.browser.history_filter.clone(),
                    self.browser
                        .history_range
                        .start(chrono::Utc::now().timestamp_millis()),
                    self.browser.history_total,
                ));
                self.browser.panel = crate::browser::Panel::HistoryClear;
            }
            HistoryClearConfirm => {
                if let Some((filter, start, _)) = self.browser.history_cleanup.take() {
                    match self.browser.history_store.clear(&filter, start) {
                        Ok(n) => self.browser.toasts.done(format!("Removed {n} visits")),
                        Err(e) => self.browser.toasts.error("history", e),
                    }
                }
                self.browser.panel = crate::browser::Panel::None;
                self.refresh_history();
            }
            SearchSuggestionsEnabled(value) => self.browser.saved.search.suggestions = value,
            SearchSuggestionsReady(generation, results) => {
                let selecting = self.browser.address_suggestion_highlight.is_some()
                    || self.browser.suggestion_highlight.is_some();
                self.browser
                    .remote_search
                    .accept(generation, results, selecting);
                return Task::none();
            }
            SelectEngine(index) => {
                if let Some(engine) = self.browser.saved.search.engines().get(index) {
                    self.browser.saved.search.selected.clone_from(&engine.id);
                }
            }
            ExternalPdfs(value) => self.browser.saved.external_pdfs = value,
            MemoryMode(mode) => {
                self.browser.saved.memory_mode = mode;
                crate::webview_native::set_memory_mode(mode);
            }
            EngineName(name) => self.browser.engine_name = name,
            EngineTemplate(template) => self.browser.engine_template = template,
            ToggleEngineEditor => {
                self.browser.engine_edit = !self.browser.engine_edit;
                self.browser.engine_error.clear();
                if self.browser.engine_edit {
                    task = focus("engine-name");
                }
            }
            AddEngine => match self
                .browser
                .saved
                .search
                .add_custom(&self.browser.engine_name, &self.browser.engine_template)
            {
                Ok(()) => {
                    self.browser.engine_name.clear();
                    self.browser.engine_template.clear();
                    self.browser.toasts.done("Search engine added");
                    self.browser.engine_edit = false;
                    self.browser.engine_error.clear();
                }
                Err(e) => self.browser.engine_error = e,
            },
            RemoveEngine(id) => {
                self.browser.saved.search.custom.retain(|e| e.id != id);
                if self.browser.saved.search.selected == id {
                    self.browser.saved.search.selected = "duckduckgo".into();
                }
            }
        }
        if order_changed {
            self.browser.commit_tab_order();
        }
        if !cfg!(test) {
            self.browser.save();
        }
        task
    }
    fn open_context(
        &mut self,
        index: usize,
        point: iced::Point,
        items: Vec<enzui_widgets::context_menu_host::ContextMenuItem<Command>>,
    ) {
        self.browser.context.open(
            index,
            enzui_core::context_menu::InvocationKind::Pointer(enzui_core::context_menu::Point {
                x: point.x,
                y: point.y,
            }),
            items.into_iter().map(decorate_context).collect(),
        );
    }
    pub(crate) fn open_external_urls(&mut self, urls: Vec<String>) -> Task<Message> {
        let Some((last, background)) = urls.split_last() else {
            return Task::none();
        };
        // A desktop URI list must not open and immediately retire native pages.
        // Background operands load when selected, just like restored session tabs.
        for url in background {
            let url = self.browser.saved.search.resolve(url);
            self.browser.saved.tabs.push(TabInfo {
                key: next_tab_key(),
                favicon: None,
                title: site_name(&url),
                url,
                pinned: false,
                group: None,
                container: None,
            });
            self.browser.sleeping.push(None);
        }
        self.browser_command(Command::ExternalVisit(last.clone()))
    }

    fn new_tab(&mut self, url: String) {
        let container = self.browser.saved.tabs[self.browser.saved.active].container;
        self.new_tab_in_container(url, container);
    }
    fn new_tab_in_container(&mut self, url: String, container: Option<u64>) {
        let i = self.browser.saved.tabs.len();
        let key = next_tab_key();
        self.browser.mark_tab_entrance(key);
        self.browser.saved.tabs.push(TabInfo {
            key,
            favicon: None,
            title: InternalPage::from_url(&url)
                .map_or_else(|| site_name(&url), |p| p.title().to_owned()),
            url: url.clone(),
            pinned: false,
            group: None,
            container,
        });
        let prepared = if !url.is_empty() && InternalPage::from_url(&url).is_none() {
            self.take_prepared_tab(container)
        } else {
            self.discard_tab_reservation();
            None
        };
        let navigate_prepared = prepared
            .as_ref()
            .is_some_and(|host| host.borrow().native.is_some());
        self.browser
            .sleeping
            .push(prepared.map(|host| SleepingTab { host, pdf: None }));
        self.select_tab(i);
        if navigate_prepared {
            self.submit_browser_action(BrowserAction::Navigate(url));
        }
    }

    fn select_tab(&mut self, i: usize) {
        if self.browser.credentials.passkey_pending() {
            self.passkey_cancel();
        }
        self.address_preparation_until = None;
        self.browser.address_suggestions_open = false;
        self.browser.address_suggestion_highlight = None;
        self.browser.input.anchor = None;
        self.browser.input.hovered_link = None;
        self.cancel_inspector_picker();
        if i >= self.browser.saved.tabs.len() {
            return;
        }
        let was_library = self.browser.library.is_some();
        self.close_library();
        if i == self.browser.saved.active {
            if was_library {
                self.resume_selected_document();
            }
            return;
        }
        self.reset_devtools_tab();
        self.pending_navigation = None;
        self.pending_native_open = false;
        let old = self.browser.saved.active;
        self.submit_browser_action(BrowserAction::DeveloperTools(false));
        self.webview.borrow_mut().hide_view();
        if let Some(request) = self.browser.permission.take() {
            self.submit_browser_action(BrowserAction::PermissionAnswer(request.id, false));
            if self.browser.panel == Panel::Permissions {
                self.browser.panel = Panel::None;
            }
        }
        let next = self.browser.sleeping[i].take();
        let (host, pdf) = next.map_or_else(
            || (RefCell::new(WebViewService::new()), None),
            |t| (t.host, t.pdf),
        );
        let previous = std::mem::replace(&mut self.webview, host);
        self.browser.sleeping[old] = Some(SleepingTab {
            pdf: self.browser.pdf.take(),
            host: previous,
        });
        self.browser.pdf = pdf;
        self.browser.zoom = self.webview.borrow().zoom_percent;
        self.sync_zoom_model();
        self.browser
            .zoom_menu_state
            .close(enzui_core::menu_model::CloseCause::Escape);
        self.browser.saved.active = i;
        self.browser.tabs_open = false;
        self.browser.query.clear();
        self.browser.suggestion_highlight = None;
        self.browser.start_suggestions_open = false;
        self.webview.borrow_mut().show_view();
        self.address = self.browser.saved.tabs[i].url.clone();
        self.address_dirty = false;
        self.last_location = None;
        self.submit_browser_action(BrowserAction::DeveloperTools(self.browser.devtools.open));
        // A tab made around a window a page opened is not sent anywhere: the
        // window loads what it was opened for.
        if self.webview.borrow().native.is_none()
            && self.webview.borrow().adopt.is_none()
            && !self.address.is_empty()
            && InternalPage::from_url(&self.address).is_none()
            && self.browser.pdf.is_none()
        {
            self.submit_browser_action(BrowserAction::Navigate(self.address.clone()));
        }
    }
    pub(crate) fn sync_browser(&mut self) {
        self.sync_browser_state();
        self.sync_browser_metadata();
    }
    pub(crate) fn sync_browser_state(&mut self) {
        {
            let host = self.webview.borrow();
            if let (Some(page), Some(view)) = (&host.native, host.live_view) {
                let _ = page.browse(
                    &host.router,
                    view,
                    crate::webview_native::BrowserAction::MemoryProtected(
                        self.browser.saved.tabs[self.browser.saved.active].pinned,
                    ),
                );
            }
        }
        self.poll_maintenance();
        self.sync_downloads();
    }
    pub(crate) fn browser_events_pending(&self) -> bool {
        std::iter::once(&self.webview)
            .chain(self.browser.sleeping.iter().flatten().map(|tab| &tab.host))
            .any(|host| !host.borrow().page_events.is_empty())
    }
    pub(crate) fn sync_browser_metadata(&mut self) {
        let mut background_visits = Vec::new();
        let mut history_titles_changed = false;
        let mut notification_relays = Vec::new();
        let mut sounds = Vec::new();
        let mut background_popups = Vec::new();
        let mut background_closing = Vec::new();
        for (i, tab) in self.browser.sleeping.iter_mut().enumerate() {
            let Some(tab) = tab else { continue };
            let mut host = tab.host.borrow_mut();
            if let (Some(page), Some(view)) = (&host.native, host.live_view) {
                let _ = page.browse(
                    &host.router,
                    view,
                    crate::webview_native::BrowserAction::MemoryProtected(
                        self.browser.saved.tabs[i].pinned,
                    ),
                );
            }
            if let Some(pdf) = &tab.pdf {
                self.browser.saved.tabs[i].title = pdf
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
            } else if let Some(title) = &host.dom_report
                && title != "enzim Browser"
                && !(title.is_empty() && host.page_stopped())
            {
                self.browser.saved.tabs[i].title.clone_from(title);
            }
            if let Some(url) = &host.location
                && !url.is_empty()
                && !url.starts_with("data:")
            {
                self.browser.saved.tabs[i].url = self
                    .local_files
                    .display_uri(url)
                    .unwrap_or_else(|| url.clone());
            }
            if tab.pdf.is_none()
                && InternalPage::from_url(&self.browser.saved.tabs[i].url).is_none()
                && host.dom_report.as_deref().is_none_or(str::is_empty)
                && !host.page_stopped()
            {
                self.browser.saved.tabs[i].title = site_name(&self.browser.saved.tabs[i].url);
            }
            let current = &self.browser.saved.tabs[i];
            match self.browser.history_store.refresh_title(
                &mut host.history_visit,
                &current.url,
                &current.title,
            ) {
                Ok(changed) => history_titles_changed |= changed,
                Err(error) => self.browser.toasts.error("history", error),
            }
            let mut pending = Vec::new();
            for event in std::mem::take(&mut host.page_events) {
                use enzui_webkit::page::PageEvent;
                match event {
                    PageEvent::Load(enzui_webkit::page::LoadStage::Finished)
                        if !self.browser.private =>
                    {
                        let tab = &self.browser.saved.tabs[i];
                        if url::Url::parse(&tab.url)
                            .is_ok_and(|u| matches!(u.scheme(), "http" | "https"))
                        {
                            background_visits.push((
                                i,
                                crate::history::Visit {
                                    id: 0,
                                    url: tab.url.clone(),
                                    title: tab.title.clone(),
                                    visited_at: chrono::Utc::now().timestamp_millis(),
                                    container: tab.container,
                                },
                            ));
                        }
                    }
                    PageEvent::Notification(request) => {
                        if let Some(view) = host.live_view {
                            let target = crate::website_notifications::Target {
                                tab: self.browser.saved.tabs[i].key,
                                view,
                                profile: host.profile.clone(),
                                native_id: request.id,
                            };
                            let (relays, error) = self.browser.desktop_notifications.show(
                                target,
                                request,
                                self.browser.saved.notifications,
                                self.browser.private,
                            );
                            notification_relays.extend(relays);
                            if let Some(error) = error {
                                self.browser.toasts.error("notifications", error);
                            }
                        }
                    }
                    PageEvent::NotificationClosed { id } => {
                        if let Some(view) = host.live_view {
                            self.browser.desktop_notifications.native_closed(view, id);
                        }
                    }
                    PageEvent::PermissionRequested(_)
                    | PageEvent::PermissionCancelled { .. }
                    | PageEvent::ScriptDialog(_)
                    | PageEvent::Authentication(_)
                    | PageEvent::FileChooser(_)
                    | PageEvent::ColorChooser(_)
                    | PageEvent::OptionMenu(_)
                    | PageEvent::OptionMenuClosed { .. } => pending.push(event),
                    PageEvent::PlayingAudio(sounding) => {
                        sounds.push((self.browser.saved.tabs[i].key, sounding));
                    }
                    PageEvent::PopupOpened { id, url, .. } => background_popups.push((
                        self.browser.saved.tabs[i].key,
                        id.get(),
                        url.as_str().to_owned(),
                    )),
                    PageEvent::CloseRequested => {
                        background_closing.push(self.browser.saved.tabs[i].key);
                    }
                    _ => {}
                }
            }
            host.page_events = pending;
        }
        for (key, sounding) in sounds.drain(..) {
            self.tab_sounds(key, sounding);
        }
        for key in background_closing {
            let task = self.popup_close_requested(key);
            self.pending_tasks.push(task);
        }
        // A window opened from a tab behind comes to the front like any
        // other; the tab on show is then another, and is read next time.
        if !background_popups.is_empty() {
            for (opener, popup, url) in background_popups {
                self.popup_opened(opener, popup, url);
            }
            return;
        }
        if !background_visits.is_empty() || history_titles_changed {
            for (index, visit) in background_visits {
                match self.browser.history_store.record(&visit) {
                    Ok(recorded) => {
                        if let Some(tab) = &self.browser.sleeping[index] {
                            tab.host.borrow_mut().history_visit = Some(recorded);
                        }
                    }
                    Err(error) => self.browser.toasts.error("history", error),
                }
            }
            self.refresh_history();
        }
        self.relay_notifications(std::mem::take(&mut notification_relays));
        history_titles_changed = false;
        if InternalPage::from_url(&self.browser.saved.tabs[self.browser.saved.active].url).is_some()
        {
            return;
        }
        let mut qa_answers = Vec::new();
        let mut service = self.webview.borrow_mut();
        let current = &mut self.browser.saved.tabs[self.browser.saved.active];
        if let Some(url) = service.location.as_ref()
            && !url.is_empty()
            && !url.starts_with("data:")
        {
            if url::Url::parse(&current.url).ok().map(|u| u.origin())
                != url::Url::parse(url).ok().map(|u| u.origin())
            {
                current.favicon = None;
                current.title = site_name(url);
            }
            current.url = self
                .local_files
                .display_uri(url)
                .unwrap_or_else(|| url.clone());
        }
        if let Some(pdf) = &self.browser.pdf {
            current.title = pdf
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
        } else if let Some(title) = service.dom_report.as_ref()
            && !title.is_empty()
        {
            current.title.clone_from(title);
        } else if !service.page_stopped() {
            current.title = site_name(&current.url);
        }
        let mut save = false;
        let mut refresh_find = false;
        let mut resources = Vec::new();
        let mut console_messages = Vec::new();
        let mut credential_messages = Vec::new();
        let mut passkey_messages = Vec::new();
        let mut passkey_left = false;
        let mut popup = None;
        let mut background_requests = Vec::new();
        let mut opened_popups = Vec::new();
        let mut closing_popups = Vec::new();
        let mut colors = Vec::new();
        let mut date_messages = Vec::new();
        let mut prompt_field = None;
        let mut fullscreen_changes = Vec::new();
        let mut external_link = None;
        let events = std::mem::take(&mut service.page_events);
        for ev in events {
            use enzui_webkit::page::PageEvent;
            // A page that starts loading something else has left the
            // document whose passkey request is waiting.
            passkey_left |= matches!(
                ev,
                PageEvent::Load(enzui_webkit::page::LoadStage::Started)
            );
            if (self.browser.library.is_some()
                && matches!(
                    ev,
                    PageEvent::PermissionRequested(_)
                        | PageEvent::PermissionCancelled { .. }
                        | PageEvent::ScriptDialog(_)
                        | PageEvent::Authentication(_)
                        | PageEvent::FileChooser(_)
                        | PageEvent::OptionMenu(_)
                        | PageEvent::OptionMenuClosed { .. }
                ))
                || prompts::waits(
                    self.browser.prompt_tab.is_some(),
                    self.browser.options.is_some(),
                    &ev,
                )
            {
                service.page_events.push(ev);
                continue;
            }
            if let Some(id) = prompts::first_field(&ev) {
                prompt_field = Some(id);
            }
            match ev {
                PageEvent::PopupRequested {
                    url,
                    user_initiated: true,
                    background: false,
                } => popup = Some(url.as_str().to_owned()),
                PageEvent::PopupRequested {
                    url,
                    user_initiated: true,
                    background: true,
                } => background_requests.push(url.as_str().to_owned()),
                PageEvent::PopupOpened { id, url, .. } => {
                    opened_popups.push((current.key, id.get(), url.as_str().to_owned()));
                }
                PageEvent::CloseRequested => closing_popups.push(current.key),
                PageEvent::ColorChooser(request) => colors.push((current.key, request)),
                PageEvent::FileChooser(request) => {
                    if service.live_view.is_some() {
                        let operation = if request.multiple {
                            enzui_core::portal::backend::PortalOperation::OpenMultiple
                        } else {
                            enzui_core::portal::backend::PortalOperation::OpenFile
                        };
                        if let Err(error) = self.browser.chooser.start(
                            (current.key, request.id),
                            operation,
                            enzui_core::portal::chooser::ChooserOptions::new()
                                .title("Choose files to upload"),
                        ) {
                            self.browser
                                .toasts
                                .error("file-chooser", format!("File chooser: {error:?}"));
                        }
                    }
                }
                PageEvent::ScriptMessage { handler, json } if handler == "enzimIcon" => {
                    if let Some(icon) = self.browser.favicons.receive(&current.url, &json) {
                        current.favicon = Some(icon);
                    }
                }
                PageEvent::OptionMenu(options) => {
                    self.browser.option_highlight = prompts::list_start(&options);
                    self.browser.options = Some(options);
                    self.browser.panel = Panel::Options;
                    self.browser.prompt_tab = Some(current.key);
                }
                PageEvent::OptionMenuClosed { .. } => {
                    if self.browser.options.take().is_some() {
                        self.browser.prompt_tab = None;
                    }
                    if self.browser.panel == Panel::Options {
                        self.browser.panel = Panel::None;
                    }
                }
                PageEvent::Fullscreen(entered) => fullscreen_changes.push(entered),
                PageEvent::PlayingAudio(sounding) => sounds.push((current.key, sounding)),
                PageEvent::ScriptMessage { handler, json } if handler == "enzimDateField" => {
                    date_messages.push((current.key, json));
                }
                PageEvent::ScriptMessage { handler, json } if handler == "enzimExternalLink" => {
                    external_link = actions::external_link(&json);
                }
                PageEvent::Authentication(challenge) => {
                    self.browser.sign_in = Some(prompts::SignIn::new(challenge));
                    self.browser.panel = Panel::SignIn;
                    self.browser.prompt_tab = Some(current.key);
                }
                PageEvent::ScriptDialog(dialog) => {
                    self.browser.prompt =
                        if let enzui_webkit::page::ScriptDialogKind::Prompt { default_text } =
                            &dialog.kind
                        {
                            default_text.clone()
                        } else {
                            String::new()
                        };
                    self.browser.dialog = Some(dialog);
                    self.browser.panel = Panel::Script;
                    self.browser.prompt_tab = Some(current.key);
                }
                PageEvent::ScriptMessage { handler, json } if handler == "enzimNativeResources" => {
                    resources.push(json);
                }
                PageEvent::SecretScriptMessage { handler, json }
                    if handler == crate::credential_bridge::HANDLER =>
                {
                    credential_messages.push(json);
                }
                PageEvent::FrameSecretMessage {
                    handler,
                    frame,
                    origin,
                    top_origin,
                    main_frame,
                    json,
                } if handler == crate::credentials::passkey::HANDLER => {
                    passkey_messages.push((frame, origin, top_origin, main_frame, json));
                }
                PageEvent::ScriptMessage { handler, json } if handler == "enzimConsole" => {
                    console_messages.push(json);
                }
                PageEvent::ScriptMessage { handler, json } if handler == "enzimInspect" => {
                    if self.browser.devtools.open
                        && self.browser.devtools.inspector.picking
                        && let Ok(value) = serde_json::from_str(&json)
                    {
                        self.browser.devtools.inspector.picking = false;
                        self.browser.devtools.inspector.element = Some(value);
                    }
                }
                PageEvent::Notification(request) => {
                    if let Some(view) = service.live_view {
                        let target = crate::website_notifications::Target {
                            tab: current.key,
                            view,
                            profile: service.profile.clone(),
                            native_id: request.id,
                        };
                        let (relays, error) = self.browser.desktop_notifications.show(
                            target,
                            request,
                            self.browser.saved.notifications,
                            self.browser.private,
                        );
                        notification_relays.extend(relays);
                        if let Some(error) = error {
                            self.browser.toasts.error("notifications", error);
                        }
                    }
                }
                PageEvent::NotificationClosed { id } => {
                    if let Some(view) = service.live_view {
                        self.browser.desktop_notifications.native_closed(view, id);
                    }
                }
                PageEvent::PermissionRequested(request) => {
                    // Call checks answer prompts the way a person would, without
                    // driving the panel; only a QA run may.
                    let qa_answer = std::env::var_os("ENZIM_QA")
                        .and_then(|_| std::env::var("ENZIM_QA_PERMISSION_ANSWER").ok())
                        .and_then(|answer| match answer.as_str() {
                            "allow" => Some(true),
                            "block" => Some(false),
                            _ => None,
                        });
                    if let Some(allow) = qa_answer {
                        qa_answers.push(BrowserAction::PermissionAnswer(request.id, allow));
                    } else if self
                        .browser
                        .permission_denied
                        .contains(&permissions::Denial::new(current.container, &request))
                    {
                        qa_answers.push(BrowserAction::PermissionAnswer(request.id, false));
                    } else {
                        self.browser.permission = Some(request);
                        self.browser.panel = Panel::Permissions;
                    }
                }
                PageEvent::PermissionCancelled { id } => {
                    if self
                        .browser
                        .permission
                        .as_ref()
                        .is_some_and(|request| request.id == id)
                    {
                        self.browser.permission = None;
                        if self.browser.panel == Panel::Permissions {
                            self.browser.panel = Panel::None;
                        }
                    }
                }
                PageEvent::FindMatches(n) => {
                    self.browser.find_status = format!(
                        "{} matches",
                        n.map_or_else(|| "Many".into(), |n| n.to_string())
                    );
                }
                PageEvent::HoveredLink(link) => {
                    let target = link.as_ref().map(|url| url.as_str().to_owned());
                    if target != self.browser.input.hovered_link {
                        self.browser.chrome_revision = self.browser.chrome_revision.wrapping_add(1);
                    }
                    self.browser.input.hovered_link = target;
                }
                PageEvent::FindFailed => self.browser.find_status = "No matches".into(),
                PageEvent::Load(enzui_webkit::page::LoadStage::Started)
                    if self.browser.panel == Panel::Find =>
                {
                    self.browser.find_status = "Searching…".into();
                }
                PageEvent::Load(enzui_webkit::page::LoadStage::Finished) => {
                    refresh_find =
                        self.browser.panel == Panel::Find && !self.browser.find.is_empty();
                    save = true;
                    if !self.browser.private
                        && url::Url::parse(&current.url)
                            .is_ok_and(|u| matches!(u.scheme(), "http" | "https"))
                    {
                        let visit = crate::history::Visit {
                            url: current.url.clone(),
                            title: current.title.clone(),
                            visited_at: chrono::Utc::now().timestamp_millis(),
                            container: current.container,
                            id: 0,
                        };
                        match self.browser.history_store.record(&visit) {
                            Ok(recorded) => service.history_visit = Some(recorded),
                            Err(error) => self.browser.toasts.error("history", error),
                        }
                        self.browser.saved.history =
                            self.browser.history_store.query("", 0).unwrap_or_default();
                    }
                }
                PageEvent::ContextMenu(target) => {
                    let items = page_menu::page_menu(&target, self.browser.private);
                    self.browser.context.open(
                        self.browser.saved.active,
                        enzui_core::context_menu::InvocationKind::Pointer(
                            enzui_core::context_menu::Point {
                                x: self.browser.pointer.x,
                                y: self.browser.pointer.y,
                            },
                        ),
                        items.into_iter().map(decorate_context).collect(),
                    );
                }
                _ => {}
            }
        }
        match self.browser.history_store.refresh_title(
            &mut service.history_visit,
            &current.url,
            &current.title,
        ) {
            Ok(changed) => history_titles_changed |= changed,
            Err(error) => self.browser.toasts.error("history", error),
        }
        drop(service);
        self.relay_notifications(notification_relays);
        for answer in qa_answers {
            self.submit_browser_action(answer);
        }
        for message in credential_messages {
            self.credential_bridge_message(message);
        }
        if passkey_left && self.browser.credentials.passkey_pending() {
            self.passkey_cancel();
        }
        for (frame, origin, top_origin, main_frame, json) in passkey_messages {
            let task =
                self.passkey_message(frame, origin.as_str(), top_origin.as_str(), main_frame, &json);
            self.pending_tasks.push(task);
        }
        if history_titles_changed {
            self.refresh_history();
        }
        if refresh_find {
            self.submit_browser_action(BrowserAction::Find(
                self.browser.find.clone(),
                self.browser.find_case,
            ));
        }
        for json in resources {
            self.native_resource_events(&json);
        }
        for json in console_messages {
            self.console_message(&json);
        }
        if let Some(url) = popup {
            self.new_tab(url);
        }
        // A link the page saw opened with the middle button (one the
        // browser had not seen hovered) opens behind, like any other.
        for url in background_requests {
            let task = self.action_command(ActionCommand::VisitBackground(url));
            self.pending_tasks.push(task);
        }
        for (tab, request) in colors {
            self.color_requested(tab, request);
        }
        for (tab, json) in date_messages {
            self.date_field_message(tab, &json);
        }
        for key in closing_popups {
            let task = self.popup_close_requested(key);
            self.pending_tasks.push(task);
        }
        for (opener, popup, url) in opened_popups {
            self.popup_opened(opener, popup, url);
        }
        for entered in fullscreen_changes {
            self.page_fullscreen(entered);
        }
        for (key, sounding) in sounds {
            self.tab_sounds(key, sounding);
        }
        self.follow_site_zoom();
        if let Some(link) = external_link {
            let task = self.action_command(ActionCommand::External(link));
            self.pending_tasks.push(task);
        }
        if let Some(id) = prompt_field {
            // What the page suggests is replaced by what is typed.
            self.pending_tasks.push(Task::batch([
                focus(id),
                iced::advanced::widget::operate(enzui_widgets::text_field::select_all(
                    iced::advanced::widget::Id::new(id),
                )),
            ]));
        }
        if save && !cfg!(test) {
            self.browser.save();
        }
    }
}
pub(crate) fn focus(id: &'static str) -> Task<Message> {
    iced::advanced::widget::operate(iced::advanced::widget::operation::focusable::focus(
        iced::advanced::widget::Id::new(id),
    ))
}
pub(crate) fn site_name(url: &str) -> String {
    if let Some(path) = crate::desktop_files::file_path(url) {
        return path.file_name().map_or_else(
            || "Local document".into(),
            |name| name.to_string_lossy().into_owned(),
        );
    }
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_else(|| "New tab".into())
}
pub(crate) fn c(cmd: Command) -> Message {
    Message::Chrome(cmd)
}
fn icon<'a>(name: &str, label: &str, msg: Message) -> Element<'a, Message> {
    enzui_widgets::toolbar_icon::enz_toolbar_icon(name, msg)
        .aria_label(label)
        .tooltip(label)
        .into_element()
}
fn field<'a>(
    placeholder: &'a str,
    value: &'a str,
    cb: impl Fn(String) -> Message + 'a,
    id: &'static str,
) -> Element<'a, Message> {
    enzui_widgets::text_field::enz_text_field("", placeholder, value, cb)
        .id(iced::advanced::widget::Id::new(id))
        .into_element()
}
pub(crate) fn palette(theme: &iced::Theme) -> enzui_theme::Palette {
    enzui_theme::EnzTheme::palette_from_iced(theme)
        .unwrap_or_else(|| enzui_theme::EnzTheme::dark().palette)
}
pub(crate) fn ink_icon<'a>(
    name: &str,
    label: &str,
    message: Message,
    control: f32,
    glyph: f32,
) -> Element<'a, Message> {
    enzui_widgets::toolbar_icon::enz_toolbar_icon(name, message)
        .aria_label(label)
        .variant(enzui_widgets::toolbar_icon::ToolbarIconVariant::InkOnly)
        .size(control, glyph)
        .into_element()
}
fn dot<'a>(color: iced::Color, size: f32) -> Element<'a, Message> {
    iced::widget::container(iced::widget::Space::new())
        .width(size)
        .height(size)
        .style(move |_| iced::widget::container::Style {
            background: Some(color.into()),
            border: iced::Border {
                radius: (size / 2.0).into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}
pub(crate) fn glyph<'a>(name: &str, size: f32) -> Element<'a, Message> {
    enzui_widgets::icon::enz_icon(name)
        .size(size)
        .into_element()
}
fn action<'a>(label: impl Into<String>, name: &str, command: Command) -> Element<'a, Message> {
    enzui_widgets::button::enz_button(label.into(), c(command))
        .icon(name)
        .kind(enzui_widgets::button::ButtonKind::Quiet)
        .size(enzui_widgets::button::ButtonSize::Md)
        .into_element()
}
fn primary<'a>(label: &str, name: &str, command: Command) -> Element<'a, Message> {
    enzui_widgets::button::enz_button(label.to_owned(), c(command))
        .icon(name)
        .kind(enzui_widgets::button::ButtonKind::Accent)
        .size(enzui_widgets::button::ButtonSize::Md)
        .into_element()
}
fn section<'a>(title: &str, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    enzui_widgets::preferences_group::enz_preferences_group(content)
        .title(title)
        .bordered(true)
        .into_element()
}
fn pref<'a>(
    name: impl Into<std::borrow::Cow<'a, str>>,
    symbol: &'a str,
    detail: impl Into<std::borrow::Cow<'a, str>>,
) -> Element<'a, Message> {
    enzui_widgets::preferences_row::enz_preferences_row(name)
        .icon(symbol)
        .subtitle(detail)
        .into_element()
}
fn heading<'a>(
    name: &str,
    title: impl Into<String>,
    subtitle: impl Into<String>,
) -> Element<'a, Message> {
    use iced::widget::{column, row, text};
    let subtitle = subtitle.into();
    let mut body = column![
        row![glyph(name, 17.), text(title.into()).size(16)]
            .spacing(8)
            .align_y(iced::Alignment::Center)
    ]
    .spacing(6);
    if !subtitle.is_empty() {
        body = body.push(
            text(subtitle)
                .size(12)
                .style(|theme| iced::widget::text::Style {
                    color: Some(palette(theme).text_muted),
                }),
        );
    }
    body.width(Length::Fill).into()
}
fn popup_section<'a>(
    title: &str,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    iced::widget::column![
        iced::widget::text(title.to_owned()).size(12),
        iced::widget::rule::horizontal(1),
        content.into()
    ]
    .spacing(8)
    .width(Length::Fill)
    .into()
}
fn popup_row_tokens() -> enzui_widgets::preferences_row::PreferencesRowTokens {
    enzui_widgets::preferences_row::PreferencesRowTokens {
        row_height: 36.,
        row_height_with_subtitle: 48.,
        padding_x: 2.,
        gap: 8.,
        wrap_vertical_padding: 6.,
        icon_size: 16.,
        icon_slot: 18.,
        title_size: 13.,
        subtitle_size: 11.,
        switch_width: 32.,
        switch_height: 18.,
        switch_thumb: 14.,
        ..Default::default()
    }
}
fn popup_info<'a>(
    title: impl Into<std::borrow::Cow<'a, str>>,
    symbol: &'a str,
    detail: impl Into<std::borrow::Cow<'a, str>>,
) -> Element<'a, Message> {
    enzui_widgets::preferences_row::enz_preferences_row(title)
        .icon(symbol)
        .subtitle(detail)
        .tokens(popup_row_tokens())
        .into_element()
}
fn menu_commands() -> Vec<(
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    Command,
)> {
    use Command::*;
    vec![
        ("new", "New tab", "plus", "Ctrl+T", New),
        ("window", "New window", "app-window", "Ctrl+N", NewWindow),
        (
            "private-window",
            "New private window",
            "venetian-mask",
            "Ctrl+Shift+N",
            NewPrivateWindow,
        ),
        (
            "other-profile",
            "Open other Profile",
            "users-round",
            "",
            ChooseProfile,
        ),
        (
            "reopen",
            "Reopen closed tab",
            "rotate-ccw",
            "Ctrl+Shift+T",
            Reopen,
        ),
        (
            "bookmarks",
            "Bookmarks",
            "bookmark",
            "",
            Panel(crate::browser::Panel::Bookmarks),
        ),
        (
            "history",
            "History",
            "history",
            "",
            Panel(crate::browser::Panel::History),
        ),
        (
            "downloads",
            "Downloads",
            "download",
            "",
            Panel(crate::browser::Panel::Downloads),
        ),
        (
            "immersive",
            "Immersive view",
            "maximize",
            "Ctrl+Shift+E",
            Immersive(immersive::ImmersiveCommand::Toggle),
        ),
        (
            "find",
            "Find in page",
            "search",
            "Ctrl+F",
            Panel(crate::browser::Panel::Find),
        ),
        (
            "save",
            "Save page as…",
            "file-down",
            "Ctrl+S",
            Export(ExportCommand::Open),
        ),
        (
            "import",
            "Import browser data…",
            "download",
            "",
            Import(ImportCommand::Open),
        ),
        (
            "passwords",
            "Passwords",
            "key-round",
            "",
            Credential(CredentialCommand::Open),
        ),
        ("print", "Print…", "printer", "Ctrl+P", Print),
        (
            "screenshot",
            "Full-page screenshot…",
            "camera",
            "",
            Export(ExportCommand::Screenshot),
        ),
        (
            "devtools",
            "Developer tools",
            "code",
            "Ctrl+Shift+I",
            Dev(DevCommand::Toggle),
        ),
        (
            "containers",
            "Containers",
            "container",
            "",
            Container(ContainerCommand::Open),
        ),
        (
            "appearance",
            "Appearance",
            "palette",
            "",
            Panel(crate::browser::Panel::Theme),
        ),
        (
            "settings",
            "Settings",
            "settings",
            "",
            Panel(crate::browser::Panel::Settings),
        ),
    ]
}
fn tab_context_items(
    browser: &Browser,
    index: usize,
    dark: bool,
) -> Vec<enzui_widgets::context_menu_host::ContextMenuItem<Command>> {
    use enzui_widgets::context_menu_host::ContextMenuItem as I;
    let tab = &browser.saved.tabs[index];
    let mut items = vec![
        I::command(
            Command::Pin(index),
            if tab.pinned { "Unpin tab" } else { "Pin tab" },
        ),
        I::command(Command::Duplicate(index), "Duplicate tab"),
        I::command(Command::Action(ActionCommand::ReloadTab(index)), "Reload tab"),
        I::command(
            Command::Action(ActionCommand::Mute(index)),
            if browser.muted.contains(&tab.key) {
                "Unmute tab"
            } else {
                "Mute tab"
            },
        ),
        I::command(Command::CopyText(tab.url.clone()), "Copy address"),
        I::command(
            Command::Action(ActionCommand::MoveToWindow(index)),
            "Move to new window",
        )
        .enabled(browser.saved.tabs.len() > 1 && !tab.pinned),
        I::separator(),
        I::command(Command::Group(index), "New group").icon("folder-plus"),
    ];
    if let Some(group) = tab.group {
        items.push(I::command(Command::Ungroup(index), "Remove from group"));
        items.push(I::command(
            Command::Panel(Panel::Group(group)),
            "Tab group settings",
        ));
    }
    for (g, group) in browser
        .saved
        .groups
        .iter()
        .enumerate()
        .filter(|(g, _)| Some(*g) != tab.group)
    {
        items.push(I::command(
            Command::AssignGroup(index, g),
            format!("Move to {}", group.name),
        ));
    }
    let mut containers = vec![
        I::command(
            Command::Container(ContainerCommand::Reopen(tab.key, None)),
            "Default",
        )
        .icon("container"),
    ];
    containers.extend(browser.saved.containers.iter().map(|entry| {
        I::command(
            Command::Container(ContainerCommand::Reopen(tab.key, Some(entry.id))),
            entry.name.clone(),
        )
        .icon(entry.icon_name())
        .icon_tint(group_ink(entry.color, dark))
    }));
    items.extend([
        I::separator(),
        I::submenu("Open in container", containers).icon("container"),
        I::separator(),
        I::command(Command::Close(index), "Close tab").enabled(!tab.pinned),
        I::command(
            Command::Action(ActionCommand::CloseOthers(index)),
            "Close other tabs",
        )
        .enabled(browser.saved.tabs.len() > 1),
        I::command(
            Command::Action(ActionCommand::CloseAfter(index)),
            "Close tabs after this",
        )
        .enabled(index + 1 < browser.saved.tabs.len()),
    ]);
    items
}
fn chrome_context_items(
    containers: &[crate::profiles::Container],
    dark: bool,
) -> Vec<enzui_widgets::context_menu_host::ContextMenuItem<Command>> {
    use enzui_widgets::context_menu_host::ContextMenuItem as I;
    let mut choices = containers
        .iter()
        .map(|entry| {
            I::command(
                Command::Container(ContainerCommand::NewTab(Some(entry.id))),
                entry.name.clone(),
            )
            .icon(entry.icon_name())
            .icon_tint(group_ink(entry.color, dark))
        })
        .collect::<Vec<_>>();
    choices.push(I::separator());
    choices.push(
        I::command(
            Command::Container(ContainerCommand::Open),
            "Manage containers",
        )
        .icon("settings"),
    );
    vec![
        I::command(Command::New, "New tab"),
        I::command(Command::NewGroup, "New group").icon("folder-plus"),
        I::command(Command::NewWindow, "New window"),
        I::command(Command::NewPrivateWindow, "New private window"),
        I::separator(),
        I::submenu("Containers", choices).icon("container"),
        I::submenu(
            "Tabs location",
            vec![
                I::command(Command::Layout(Layout::Top), "Top").icon("panel-top"),
                I::command(Command::Layout(Layout::Sidebar), "Sidebar").icon("panel-left"),
                I::command(Command::Layout(Layout::Icons), "Icons").icon("panel-left-close"),
            ],
        )
        .icon("panels-top-left"),
        I::separator(),
        I::command(Command::Panel(Panel::Settings), "Settings"),
    ]
}
fn menu_model(containers: &[crate::profiles::Container]) -> enzui_core::menu_model::MenuModel {
    use enzui_core::menu_model::{MenuEntryModel, MenuModel};
    let mut entries = vec![];
    for (id, label, symbol, shortcut, _) in menu_commands() {
        if matches!(id, "bookmarks" | "find" | "settings") {
            entries.push(MenuEntryModel::separator(format!("separator-{id}")));
        }
        let mut entry = if id == "containers" {
            MenuEntryModel::submenu(id, label).icon(symbol)
        } else {
            MenuEntryModel::command(id, label).icon(symbol)
        };
        if !shortcut.is_empty() {
            entry = entry.shortcut(shortcut);
        }
        entries.push(entry);
    }
    let mut children = containers
        .iter()
        .map(|entry| {
            MenuEntryModel::command(format!("container-new:{}", entry.id), entry.name.clone())
                .icon(entry.icon_name())
        })
        .collect::<Vec<_>>();
    children.push(MenuEntryModel::separator("containers-manage-divider"));
    children
        .push(MenuEntryModel::command("containers-manage", "Manage containers").icon("settings"));
    MenuModel::new(entries).with_submenu("containers", MenuModel::new(children))
}
fn command_icon(command: &Command) -> &'static str {
    use Command::*;
    match command {
        New => "plus",
        NewWindow => "app-window",
        NewPrivateWindow => "venetian-mask",
        Container(_) => "container",
        Reopen => "rotate-ccw",
        Action(ActionCommand::ReloadTab(_)) => "rotate-cw",
        Action(ActionCommand::Mute(_)) => "volume-x",
        Action(ActionCommand::MoveToWindow(_) | ActionCommand::OpenWindow(_, false)) => "app-window",
        Action(ActionCommand::OpenWindow(_, true)) => "venetian-mask",
        Action(ActionCommand::CloseOthers(_) | ActionCommand::CloseAfter(_)) => "x",
        Action(ActionCommand::Save(_)) | Export(_) => "download",
        Action(ActionCommand::SearchSelection) => "search",
        Action(ActionCommand::Cut) => "scissors",
        Action(ActionCommand::Translate) => "languages",
        Action(ActionCommand::ViewSource) => "code",
        Action(ActionCommand::Reader) => "book-open",
        Close(_) | CloseActive => "x",
        Pin(_) => "pin",
        Duplicate(_) => "copy",
        Group(_) | AssignGroup(_, _) | NewGroup => "folder-plus",
        Ungroup(_) => "folder-minus",
        RemoveGroup(_) => "ungroup",
        CloseGroup(_) => "folder-x",
        Reload => "rotate-cw",
        Visit(_) => "external-link",
        Spelling(spelling::SpellingCommand::Fix(fix)) => match fix {
            enzui_webkit::page::SpellingFix::Guess(_) => "spell-check",
            enzui_webkit::page::SpellingFix::Learn => "book-plus",
            enzui_webkit::page::SpellingFix::Ignore => "eye-off",
        },
        CopyText(_) | CopyUrl | CopySelection => "copy",
        Paste => "clipboard-paste",
        Edit("undo") => "undo",
        Edit("redo") => "redo",
        Edit(_) => "scan-text",
        Bookmark => "bookmark",
        Layout(crate::browser::Layout::Top) => "panel-top",
        Layout(crate::browser::Layout::Sidebar) => "panel-left",
        Layout(crate::browser::Layout::Icons) => "panel-left-close",
        Panel(crate::browser::Panel::Find) => "search",
        Inspect(_) => "mouse-pointer-click",
        Panel(crate::browser::Panel::Settings | crate::browser::Panel::Group(_)) => "settings",
        Panel(crate::browser::Panel::Bookmarks) => "bookmark",
        Panel(crate::browser::Panel::History) => "history",
        Panel(crate::browser::Panel::Downloads) => "download",
        Print => "printer",
        OpenPdf => "file-text",
        _ => "circle",
    }
}
fn group_ink(index: usize, dark: bool) -> iced::Color {
    let colors = if dark {
        [
            [152, 174, 255],
            [126, 204, 162],
            [232, 171, 111],
            [217, 143, 198],
            [114, 193, 246],
            [91, 206, 193],
            [245, 143, 105],
            [172, 181, 198],
        ]
    } else {
        [
            [92, 75, 155],
            [39, 112, 72],
            [142, 85, 24],
            [145, 54, 118],
            [25, 104, 164],
            [20, 115, 104],
            [171, 72, 30],
            [85, 96, 117],
        ]
    };
    let [r, g, b] = colors[index % 8];
    iced::Color::from_rgb8(r, g, b)
}
fn decorate_context(
    mut item: enzui_widgets::context_menu_host::ContextMenuItem<Command>,
) -> enzui_widgets::context_menu_host::ContextMenuItem<Command> {
    if let enzui_widgets::context_menu_host::ContextMenuItem::Command { command, icon, .. } =
        &mut item
        && icon.is_none()
    {
        *icon = Some(command_icon(command).into());
    }
    item
}

impl App {
    fn browser_menu(&self) -> Element<'_, Message> {
        use enzui_widgets::menu_button::{MenuButtonEvent as E, dismiss_on_outside_press};
        let open = self.browser.menu_state.is_open();
        let popup = open.then(|| {
            enzui_widgets::swallow_press::SwallowPress::new(
                self.browser_menu_level(&self.browser.menu_model, true),
            )
            .into()
        });
        let trigger = enzui_widgets::toolbar_icon::enz_toolbar_icon(
            "ellipsis-vertical",
            c(Command::Menu(if open {
                E::Close(enzui_core::menu_model::CloseCause::TriggerToggle)
            } else {
                E::Open(enzui_core::menu_model::OpenCause::Pointer)
            })),
        )
        .id(iced::widget::Id::new("browser-menu"))
        .aria_label("Browser menu")
        .expanded(open)
        .into_element();
        let anchored: Element<'_, Message> =
            enzui_widgets::anchored_overlay::anchored_overlay(trigger, popup)
                .placement(
                    enzui_widgets::menu_button::menu_placement(
                        enzui_core::placement::Side::Bottom,
                        enzui_core::placement::ambient_direction(),
                    )
                    .alignment(enzui_core::placement::Alignment::End),
                )
                .into();
        dismiss_on_outside_press(
            anchored,
            &self.browser.menu_model,
            &self.browser.menu_state,
            open,
            |event| c(Command::Menu(event)),
        )
    }
    fn browser_menu_level<'a>(
        &'a self,
        model: &'a enzui_core::menu_model::MenuModel,
        root: bool,
    ) -> Element<'a, Message> {
        use enzui_widgets::{
            menu::{EnzMenuItem, enz_menu},
            menu_button::MenuButtonEvent as E,
        };
        let mut menu = enz_menu()
            .label(if root { "Browser menu" } else { "Containers" })
            .id_prefix(if root {
                "browser-menu-row"
            } else {
                "browser-container-menu-row"
            })
            .width(if root { 300. } else { 260. })
            .active_maybe(model.active().cloned())
            .on_hover(|id| c(Command::Menu(E::Highlight(id))));
        for entry in model.entries() {
            if root && entry.id().as_str() == "separator-settings" {
                menu = menu.push(self.browser_zoom_row());
            }
            let event = if entry.kind() == enzui_core::menu_model::EntryKind::Submenu {
                E::SubmenuOpened(entry.id().clone())
            } else {
                E::Activated {
                    id: entry.id().clone(),
                    check: entry.check_state(),
                }
            };
            if root && entry.kind() == enzui_core::menu_model::EntryKind::Submenu {
                let trigger = enzui_widgets::menu_row::enz_menu_row(
                    entry.label().to_owned(),
                    c(Command::Menu(event)),
                )
                .icon("container")
                .submenu_indicator(true)
                .highlighted(model.active() == Some(entry.id()))
                .id(iced::widget::Id::new("browser-containers-submenu"))
                .into_element();
                let trigger = iced::widget::mouse_area(trigger)
                    .on_enter(c(Command::Menu(E::Highlight(entry.id().clone()))));
                let child = self
                    .browser
                    .menu_state
                    .open_submenu()
                    .filter(|open| *open == entry.id())
                    .and_then(|_| model.submenu(entry.id()))
                    .map(|child| {
                        enzui_widgets::swallow_press::SwallowPress::new(
                            self.browser_menu_level(child, false),
                        )
                        .into()
                    });
                let cascade: Element<'_, Message> =
                    enzui_widgets::anchored_overlay::anchored_overlay(trigger, child)
                        .placement(
                            enzui_core::placement::Placement::new(
                                enzui_core::placement::Side::Right,
                            )
                            .alignment(enzui_core::placement::Alignment::Start)
                            .direction(enzui_core::placement::ambient_direction()),
                        )
                        .into();
                menu = menu.push(cascade);
            } else {
                let mut item =
                    EnzMenuItem::from_entry(entry.clone(), Some(c(Command::Menu(event))));
                if let Some(container) = entry
                    .id()
                    .as_str()
                    .strip_prefix("container-new:")
                    .and_then(|id| id.parse::<u64>().ok())
                    .and_then(|id| self.browser.saved.containers.iter().find(|c| c.id == id))
                {
                    item = item.icon_tint(group_ink(
                        container.color,
                        self.look().palette.is_dark(),
                    ));
                }
                menu = menu.push_item(item);
            }
        }
        menu.into_element()
    }
    fn panel_body(&self, panel: Panel) -> Element<'_, Message> {
        use iced::widget::{Space, column, container, row, text};
        let mut body = column![]
            .spacing(if panel == Panel::Settings { 16 } else { 10 })
            .width(Length::Fill);
        match panel {
            Panel::Security => {
                let service = self.webview.borrow();
                let overridden = service.tls.as_deref().is_some_and(|tls| {
                    tls.starts_with(crate::webview_native::CERTIFICATE_OVERRIDDEN)
                });
                let mixed = service.mixed_content;
                let secure = !service.loading
                    && service.tls.is_some()
                    && !overridden
                    && !mixed
                    && service.native_error.is_none();
                body = body.push(heading(
                    if secure {
                        "shield-check"
                    } else if overridden {
                        "shield-alert"
                    } else {
                        "info"
                    },
                    if secure {
                        "Connection secured"
                    } else if overridden {
                        "Site not verified"
                    } else if mixed && service.tls.is_some() {
                        "Partly secure"
                    } else {
                        "Site information"
                    },
                    site_name(&self.browser.saved.tabs[self.browser.saved.active].url),
                ));
                if service.loading {
                    body = body.push(popup_info(
                        "Connecting",
                        "loader",
                        "Certificate details appear after this connection is established.",
                    ));
                } else if let Some(error) = &service.native_error {
                    body = body.push(popup_info(
                        "Connection unavailable",
                        "shield-alert",
                        error.clone(),
                    ));
                } else if let Some(tls) = &service.tls {
                    body = body.push(
                        text(if overridden {
                            "The connection is encrypted, but nothing proves who is at the other end. Do not enter passwords or payment details here."
                        } else if mixed {
                            "The page itself came over an encrypted connection, but parts of it (images, scripts) did not and could have been read or changed on the way."
                        } else {
                            "Your connection to this site is encrypted."
                        })
                        .size(13),
                    );
                    let mut cert = column![].spacing(2);
                    for line in tls.lines() {
                        let (title, symbol, value) = if let Some(v) = line.strip_prefix("Subject: ")
                        {
                            ("Issued to", "badge-check", v)
                        } else if let Some(v) = line.strip_prefix("Issuer: ") {
                            ("Issued by", "building-2", v)
                        } else if overridden {
                            ("Validation", "shield-alert", line)
                        } else {
                            ("Validation", "shield-check", line)
                        };
                        cert = cert.push(popup_info(title, symbol, value.to_owned()));
                    }
                    body = body.push(popup_section("Certificate", cert));
                } else {
                    body = body.push(popup_info(
                        "No secure connection",
                        "shield-off",
                        "Certificate information is available after an HTTPS page connects.",
                    ));
                }
            }
            Panel::Passwords => {
                return container(
                    scrollable(self.credential_popup())
                        .width(Length::Fill)
                        .height(Length::Shrink),
                )
                .width(Length::Fill)
                .padding(12)
                .height(Length::Shrink.max(self.popup_max_height()))
                .into();
            }
            Panel::Permissions => {
                body = body.push(self.site_permissions_panel());
            }
            Panel::Settings => {
                if self.browser.private {
                    body = body.push(pref(
                        "Private window",
                        "venetian-mask",
                        "Settings and bookmark changes in this window are temporary.",
                    ));
                }
                let layout = self.browser.saved.layout;
                use enzui_widgets::inline_view_switcher::{
                    InlineSwitcherPage as Page, enz_inline_view_switcher,
                };
                let placement = enz_inline_view_switcher(
                    vec![
                        Page::new("top", "Top").with_icon("panel-top"),
                        Page::new("sidebar", "Sidebar").with_icon("panel-left"),
                        Page::new("icons", "Icons").with_icon("panel-left-close"),
                    ],
                    match layout {
                        Layout::Top => 0,
                        Layout::Sidebar => 1,
                        Layout::Icons => 2,
                    },
                    |i| {
                        c(Command::Layout(match i {
                            0 => Layout::Top,
                            1 => Layout::Sidebar,
                            _ => Layout::Icons,
                        }))
                    },
                )
                .tab_id_prefix("settings-layout")
                .into_element();
                let side = enz_inline_view_switcher(
                    vec![
                        Page::new("left", "Left").with_icon("panel-left"),
                        Page::new("right", "Right").with_icon("panel-right"),
                    ],
                    usize::from(self.browser.saved.right),
                    |i| c(Command::Right(i == 1)),
                )
                .tab_id_prefix("settings-side")
                .into_element();
                body = body.push(section(
                    "Appearance",
                    column![
                        enzui_widgets::preferences_row::enz_preferences_row("Tab layout")
                            .icon("panels-top-left")
                            .control(placement)
                            .accessory_width(300.0)
                            .label_min_width(130.0)
                            .into_element(),
                        enzui_widgets::preferences_row::enz_preferences_row("Sidebar position")
                            .icon("panel-left-right-dashed")
                            .control(side)
                            .accessory_width(190.0)
                            .into_element(),
                        settings_divider(),
                        enzui_widgets::preferences_row::enz_preferences_row("Colours, frame and sizes")
                            .icon("palette")
                            .subtitle("Follow your enzUI theme until you change them here.")
                            .action("Customise…", c(Command::Panel(Panel::Theme)))
                            .into_element()
                    ]
                    .spacing(4),
                ));
                body = body.push(self.shields_settings("settings"));
                body = body.push(self.privacy_settings());
                body = body.push(self.notification_settings());
                body = body.push(self.credential_settings());
                body = body.push(self.assistant_settings_section());
                body = body.push(section(
                    "Browser profiles",
                    enzui_widgets::preferences_row::enz_preferences_row(
                        crate::browser_profile::name(),
                    )
                    .icon("users-round")
                    .subtitle("Separate spaces for your tabs, passwords, history and site data.")
                    .action("Choose profile…", c(Command::ChooseProfile))
                    .into_element(),
                ));
                body = body.push(section(
                    "Bring your data",
                    enzui_widgets::preferences_row::enz_preferences_row("Import browser data")
                        .icon("download")
                        .subtitle("Firefox, Chrome, Zen, Brave and other compatible profiles")
                        .action("Import…", c(Command::Import(ImportCommand::Open)))
                        .into_element(),
                ));
                body = body.push(section(
                    "Website data",
                    enzui_widgets::preferences_row::enz_preferences_row("Cookies and site data")
                        .icon("cookie")
                        .action("Manage", c(Command::Cookies(CookieCommand::Open)))
                        .into_element(),
                ));
                body = body.push(section(
                    "Browsing",
                    column![
                        self.search_settings(),
                        settings_divider(),
                        enzui_widgets::preferences_row::enz_preferences_row("Middle-button autoscroll")
                            .icon("move-vertical")
                            .subtitle("Click the middle mouse button, then move up or down to scroll. Press Escape or click to stop.")
                            .switch(self.browser.saved.middle_autoscroll,|value|c(Command::Input(crate::browser_input::InputCommand::Enabled(value))))
                            .into_element(),
                        settings_divider(),
                        self.startup_settings(),
                        settings_divider(),
                        self.download_settings(),
                        settings_divider(),
                        self.download_place_settings(),
                        settings_divider(),
                        self.spelling_settings(),
                        settings_divider(),
                        enzui_widgets::preferences_row::enz_preferences_row("Bookmarks bar")
                            .icon("bookmark")
                            .subtitle("A row under the toolbar with the bookmarks kept in no folder, and the folders. Ctrl+Shift+B shows and hides it.")
                            .switch(self.browser.saved.bookmarks_bar, |_| c(Command::Bookmarks(BookmarkCommand::Bar)))
                            .into_element()
                    ]
                    .spacing(4),
                ));
                body = body.push(section("Memory", self.memory_settings()));
                body = body.push(section(
                    "Documents & privacy",
                    column![
                        enzui_widgets::preferences_row::enz_preferences_row(
                            "Open PDFs in the browser"
                        )
                        .icon("file-text")
                        .subtitle("When off, downloaded PDFs open with your default system app.")
                        .switch(!self.browser.saved.external_pdfs, |value| c(
                            Command::ExternalPdfs(!value)
                        ))
                        .into_element(),
                        settings_divider(),
                        enzui_widgets::preferences_row::enz_preferences_row("Browsing history")
                            .icon("history")
                            .action("Manage", c(Command::Panel(Panel::History)))
                            .into_element(),
                        settings_divider(),
                        enzui_widgets::preferences_row::enz_preferences_row("Site permissions")
                            .icon("shield")
                            .subtitle("Review and reset remembered website permissions.")
                            .action("Manage", c(Command::Panel(Panel::PermissionSettings)))
                            .into_element()
                    ]
                    .spacing(4),
                ));
            }
            Panel::Downloads => {
                return self.downloads_panel();
            }
            Panel::Shields => {
                body = body.push(self.shields_panel());
            }
            Panel::Export => return self.export_panel(),
            Panel::Print => body = body.push(self.print_view()),
            Panel::Find => {
                body = body
                    .push(heading(
                        "search",
                        "Find in page",
                        format!("{}% zoom", self.browser.zoom),
                    ))
                    .push(field(
                        "Search this page",
                        &self.browser.find,
                        |s| c(Command::Find(s)),
                        "page-find",
                    ));
                body = body.push(
                    row![
                        text(self.browser.find_status.clone()).size(12),
                        Space::new().width(Length::Fill),
                        icon("chevron-up", "Previous match", c(Command::FindNext(true))),
                        icon("chevron-down", "Next match", c(Command::FindNext(false)))
                    ]
                    .align_y(iced::Alignment::Center)
                    .spacing(8),
                );
            }
            Panel::Group(g) => {
                let group = &self.browser.saved.groups[g];
                body = body
                    .push(heading("folder", "Tab group", "Organize related pages"))
                    .push(field(
                        "Group name",
                        &group.name,
                        |s| c(Command::GroupName(s)),
                        "group-name",
                    ));
                let colors = [
                    ("Lavender", iced::Color::from_rgb8(152, 174, 255)),
                    ("Sage", iced::Color::from_rgb8(126, 204, 162)),
                    ("Amber", iced::Color::from_rgb8(232, 171, 111)),
                    ("Rose", iced::Color::from_rgb8(217, 143, 198)),
                ];
                let mut swatches = row![].spacing(8);
                for (i, (name, _)) in colors.into_iter().enumerate() {
                    let color = group_ink(i, self.look().palette.is_dark());
                    swatches = swatches.push(
                        enzui_widgets::focus_button::focus_button(
                            row![
                                enzui_widgets::icon::enz_icon(if group.color % 4 == i {
                                    "circle-check"
                                } else {
                                    "circle"
                                })
                                .size(18.0)
                                .tint(color)
                                .into_element(),
                                text(name).size(12)
                            ]
                            .spacing(6),
                            c(Command::GroupColor(i)),
                        )
                        .padding([8, 10]),
                    );
                }
                body = body.push(text("Color").size(12)).push(swatches);
            }
            Panel::Options => {
                if let Some(options) = &self.browser.options {
                    for (i, item) in options.items.iter().enumerate() {
                        if item.enabled && !item.group_label {
                            body = body.push(action(
                                item.label.clone(),
                                "chevron-right",
                                Command::AnswerOption(i),
                            ));
                        } else {
                            body = body.push(text(item.label.clone()).size(12));
                        }
                    }
                }
            }
            Panel::Script => {
                if let Some(view) = self.script_dialog_view() {
                    body = body.push(view);
                }
            }
            Panel::SignIn => {
                if let Some(view) = self.sign_in_view() {
                    body = body.push(view);
                }
            }
            Panel::BookmarkEdit => {
                if let Some(view) = self.bookmark_edit_view() {
                    body = body.push(view);
                }
            }
            Panel::Color => {
                if let Some(view) = self.color_view() {
                    body = body.push(view);
                }
            }
            _ => {}
        }
        if panel == Panel::Settings {
            return body.into();
        }
        container(scrollable(body).width(Length::Fill).height(Length::Shrink))
            .width(Length::Fill)
            .padding(12)
            .height(Length::Shrink.max(self.popup_max_height()))
            .into()
    }
    fn popup_max_height(&self) -> f32 {
        (self.browser.viewport.height - 100.).clamp(100., 520.)
    }
    fn pop(&self, label: &str, panel: Panel) -> Element<'_, Message> {
        if self.browser.library.is_some() && panel != Panel::Downloads {
            return enzui_widgets::toolbar_icon::enz_toolbar_icon(
                match panel {
                    Panel::Passwords => "key-round",
                    Panel::Shields => "shield-check",
                    Panel::Permissions => "sliders-horizontal",
                    _ => "info",
                },
                c(Command::Panel(panel)),
            )
            .aria_label(label)
            .enabled(false)
            .into_element();
        }
        enzui_widgets::popover::enz_popover(
            icon(
                match panel {
                    Panel::Security => {
                        let service = self.webview.borrow();
                        if service.loading || service.native_error.is_some() {
                            "info"
                        } else {
                            match service.tls.as_deref() {
                                Some(tls)
                                    if tls.starts_with(
                                        crate::webview_native::CERTIFICATE_OVERRIDDEN,
                                    ) =>
                                {
                                    "shield-alert"
                                }
                                Some(_) if service.mixed_content => "shield-alert",
                                Some(_) => "lock-keyhole",
                                None => "info",
                            }
                        }
                    }
                    Panel::Passwords => "key-round",
                    Panel::Shields => "shield-check",
                    Panel::Permissions => "sliders-horizontal",
                    Panel::Downloads => "download",
                    _ => "ellipsis-vertical",
                },
                match panel {
                    Panel::Security => "Site information",
                    Panel::Passwords => "Passwords",
                    Panel::Shields => "Privacy",
                    Panel::Permissions => "Site permissions",
                    Panel::Downloads => "Downloads",
                    _ => "Browser menu",
                },
                c(Command::Panel(panel)),
            ),
            (self.browser.panel == panel).then(|| self.panel_body(panel)),
            || c(Command::Dismiss),
        )
        .width(
            (self.browser.viewport.width
                - if panel == Panel::Downloads {
                    96.0
                } else {
                    24.0
                })
            .min(
                if matches!(
                    panel,
                    Panel::Security
                        | Panel::Downloads
                        | Panel::Passwords
                        | Panel::Shields
                        | Panel::Permissions
                ) {
                    420.0
                } else {
                    380.0
                },
            ),
        )
        .padding(0.0)
        .alignment(if panel == Panel::Security {
            enzui_core::placement::Alignment::Start
        } else {
            enzui_core::placement::Alignment::End
        })
        .label(label)
        .scope(iced::advanced::widget::Id::from(format!(
            "browser-popover-{panel:?}"
        )))
        .into_element()
    }
    pub(crate) fn chrome_view(&self) -> Element<'_, Message> {
        use iced::widget::{column, container, row};
        // In immersive view the sidebar carries the toolbar: the address
        // field keeps only the bookmark star, and the site's tools get a
        // row of their own.
        let immersive = self.immersive();
        let bookmark = self
            .browser
            .saved
            .bookmarks
            .iter()
            .any(|t| t.url == self.address);
        // The framework's field comes in three heights. At a height of its
        // own the address bar's body is drawn here, around the tallest field
        // that fits in it, whose own body and edges are then left unpainted.
        let bar = self.address_bar();
        let bar_height = bar.height();
        let own_body = bar_height != crate::appearance::address_field_height();
        // The field, and what its text starts further in than a large
        // field's: the body keeps the large field's inset at every height.
        let (field_size, inset) = {
            use enzui_widgets::text_field::TextFieldSize::{Lg, Md, Sm};
            let ui = enzui_theme::text_scale::ui_px;
            // How tall the framework makes a smaller field around a large
            // field's text: its own padding above and below the taller line.
            let holding = |height: f32, text: f32| (ui(height) + 1.3 * (ui(15.0) - ui(text))).ceil();
            if !own_body || bar_height >= ui(40.0) {
                (Lg, 0.0)
            } else if bar_height >= holding(32.0, 13.0) {
                (Md, 4.0)
            } else {
                (Sm, 6.0)
            }
        };
        let site_tools = || -> Vec<Element<'_, Message>> {
            vec![
                enzui_widgets::toolbar_icon::enz_toolbar_icon("copy", c(Command::CopyUrl))
                    .aria_label("Copy page URL")
                    .tooltip("Copy page URL")
                    .enabled(
                        self.browser.library.is_some()
                            || !self.browser.saved.tabs[self.browser.saved.active]
                                .url
                                .is_empty()
                    )
                    .into_element(),
                if self.browser.credentials.settings.enabled {
                    self.pop("Passwords", Panel::Passwords)
                } else {
                    iced::widget::Space::new().into()
                },
                self.pop("Privacy", Panel::Shields),
                self.pop("Site permissions", Panel::Permissions),
            ]
        };
        let address = enzui_widgets::text_field::enz_text_field(
            "",
            "Search or enter address",
            &self.address,
            Message::BrowserAddress,
        )
        .aria_label("Search or enter address")
        .id(iced::advanced::widget::Id::new("browser-address"))
        .size(field_size)
        .radius(10.0)
        .quiet(true)
        .subtle_focus(true)
        .select_all_on_focus(true)
        .leading(self.address_identity())
        .trailing({
            let star: Element<'_, Message> = enzui_widgets::toolbar_icon::enz_toolbar_icon(
                if bookmark { "bookmark-check" } else { "star" },
                c(Command::Bookmark),
            )
            .aria_label(if bookmark {
                "Remove bookmark"
            } else {
                "Bookmark page"
            })
            .tooltip(if bookmark {
                "Remove bookmark"
            } else {
                "Bookmark page"
            })
            .enabled(
                self.browser.library.is_none()
                    && !self.browser.saved.tabs[self.browser.saved.active]
                        .url
                        .is_empty(),
            )
            .into_element();
            let trailing = row![self.address_zoom_button(), star]
                .spacing(2)
                .align_y(iced::Alignment::Center);
            if immersive {
                trailing
            } else {
                site_tools().into_iter().fold(trailing, iced::widget::Row::push)
            }
        })
        .on_submit(Message::BrowserGo);
        // In the narrow sidebar the site-information button's own margin is
        // space enough before it.
        let address = if immersive {
            address.padding_x(4.0)
        } else {
            address
        };
        // On immersive view's glass the field lets some of it through; on
        // the solid style, as outside immersive view, it is opaque.
        let field_alpha = if immersive
            && self.browser.saved.immersive_style != immersive::ImmersiveStyle::Solid
        {
            immersive::FIELD_ALPHA
        } else {
            1.0
        };
        let address: Element<'_, Message> = if own_body {
            let look = &self.look().palette;
            let (fill, edge) = (look.field.scale_alpha(field_alpha), look.field_border_strong);
            let hovered = self.address_hovered();
            let field = address
                // A large field's text, whichever field holds it.
                .text_size(15.0)
                .palette(enzui_theme::Palette {
                    field: iced::Color::TRANSPARENT,
                    field_border: iced::Color::TRANSPARENT,
                    field_border_strong: iced::Color::TRANSPARENT,
                    ..look.clone()
                })
                .into_element();
            iced::widget::mouse_area(
                container(field)
                    .height(bar_height)
                    .padding([0.0, inset])
                    .align_y(iced::Alignment::Center)
                    .style(move |_| container::Style {
                        background: Some(fill.into()),
                        border: iced::Border {
                            radius: 10.0.into(),
                            width: if hovered { 1.0 } else { 0.0 },
                            color: edge,
                        },
                        ..Default::default()
                    }),
            )
            .on_enter(c(Command::Appearance(AppearanceCommand::AddressHover(true))))
            .on_exit(c(Command::Appearance(AppearanceCommand::AddressHover(false))))
            // The body above and below the field belongs to the bar.
            .on_press(Message::BrowserFocusAddress)
            .interaction(iced::mouse::Interaction::Text)
            .into()
        } else if field_alpha < 1.0 {
            let look = &self.look().palette;
            address
                .palette(enzui_theme::Palette {
                    field: look.field.scale_alpha(field_alpha),
                    ..look.clone()
                })
                .into_element()
        } else {
            address.into_element()
        };
        // Only a bar beside the buttons is narrowed; on a row of its own, in
        // a narrow window, it keeps the row.
        let share = if self.browser.viewport.width < 640.0 {
            crate::appearance::ADDRESS_WIDTH.1
        } else {
            bar.width()
        };
        let choices = if self.browser.address_suggestions_open {
            self.suggestions_for(&self.address)
        } else {
            Vec::new()
        };
        let open = !choices.is_empty();
        let address = enzui_widgets::keyboard_capture::keyboard_capture_any_focus(
            address,
            move |key, modifiers| {
                open.then(|| pages::suggestion_key(key, modifiers, true))
                    .flatten()
            },
        );
        let address = enzui_widgets::popover::enz_popover(
            address,
            open.then(|| self.address_suggestions(&choices)),
            || c(Command::AddressSuggestionsDismiss),
        )
        .width(if immersive {
            // Hanging from the sidebar's field, the list reaches over the page.
            (self.browser.viewport.width - 24.0).clamp(200.0, 520.0)
        } else {
            let room = (self.browser.viewport.width
                - if self.browser.viewport.width < 720. {
                    20.
                } else {
                    240.
                })
            .max(200.);
            // Under a narrowed bar the list is narrowed with it, as far as
            // its rows stay readable.
            (room * share / crate::appearance::ADDRESS_WIDTH.1).max(room.min(420.))
        })
        .padding(6.)
        .label("Address suggestions")
        .into_element();
        let web = self.browser.library.is_none()
            && InternalPage::from_url(&self.browser.saved.tabs[self.browser.saved.active].url)
                .is_none();
        let navigation = row![
            navigation_button(
                "arrow-left",
                "Back",
                Message::BrowserBack,
                self.browser.library.is_some() || (web && self.webview.borrow().can_go_back)
            ),
            navigation_button(
                "arrow-right",
                "Forward",
                Message::BrowserForward,
                web && self.webview.borrow().can_go_forward
            ),
            enzui_widgets::toolbar_icon::enz_toolbar_icon(
                if self.webview.borrow().loading {
                    "x"
                } else {
                    "rotate-cw"
                },
                if self.webview.borrow().loading {
                    c(Command::Stop)
                } else {
                    Message::BrowserReload
                }
            )
            .aria_label(if self.webview.borrow().loading {
                "Stop loading"
            } else {
                "Reload"
            })
            .enabled(web)
            .into_element()
        ]
        .spacing(2)
        .align_y(iced::Alignment::Center);
        let switcher: Element<'_, Message> = if self.effective_tab_layout().is_none() {
            enzui_widgets::popover::enz_popover(
                icon("panel-left", "Show tabs", c(Command::ToggleTabs)),
                self.browser.tabs_open.then(|| {
                    container(self.tab_strip_for(Layout::Sidebar))
                        .height((self.browser.viewport.height - 140.0).clamp(160.0, 520.0))
                        .into()
                }),
                || c(Command::ToggleTabs),
            )
            .width(232.0)
            .padding(4.0)
            .label("Tabs")
            .into_element()
        } else {
            iced::widget::Space::new().width(0).into()
        };
        let downloads: Element<'_, Message> = row![
            self.assistant_button(),
            self.pop("Downloads", Panel::Downloads)
        ]
        .spacing(3)
        .align_y(iced::Alignment::Center)
        .into();
        let menu = self.browser_menu();
        let controls = enzui_widgets::window_controls::enz_window_controls(c(Command::Exit))
            .show_minimize(true, c(Command::Minimize))
            .show_maximize(true, c(Command::Maximize), false)
            .into_element();
        let mut immersive_parts = None;
        let toolbar: Element<'_, Message> = if self.popup_window {
            self.popup_bar()
        } else if immersive {
            immersive_parts = Some(immersive::Toolbar {
                navigation: navigation.into(),
                address,
                tools: site_tools(),
                downloads,
                menu,
                controls,
            });
            iced::widget::Space::new().into()
        } else if self.browser.viewport.width < 640.0 {
            column![
                row![
                    switcher,
                    navigation,
                    iced::widget::Space::new().width(Length::Fill),
                    row![downloads, menu, controls].spacing(3).align_y(iced::Alignment::Center)
                ]
                .align_y(iced::Alignment::Center)
                .spacing(4),
                address
            ]
            .spacing(6)
            .into()
        } else {
            // A narrowed bar stands in the middle of its room; what is left
            // free beside it is the top bar's, to drag the window by.
            let address: Element<'_, Message> = if share < crate::appearance::ADDRESS_WIDTH.1 {
                let side = Length::FillPortion((crate::appearance::ADDRESS_WIDTH.1 - share) as u16);
                row![
                    iced::widget::Space::new().width(side),
                    container(address).width(Length::FillPortion(2 * share as u16)),
                    iced::widget::Space::new().width(side),
                ]
                .padding([0, 6])
                .into()
            } else {
                container(address)
                    .width(Length::Fill)
                    .padding([0, 6])
                    .into()
            };
            let actions = row![downloads, menu, controls]
                .spacing(3)
                .align_y(iced::Alignment::Center);
            row![switcher, navigation, address, actions]
                .spacing(4)
                .align_y(iced::Alignment::Center)
                .into()
        };
        let p = &self.look().palette;
        let radius = self.window_radius();
        // In immersive view the page sits a frame's width inside the
        // window's corners; its curve follows theirs at that distance, so
        // the frame is as wide around a corner as along a side.
        let page_radius = if immersive {
            (radius - immersive::EDGE).max(0.0)
        } else {
            radius
        };
        // Where the compositor cuts the corners, the frame's paint reaches
        // under all of its curve, and only what follows the curve is rounded.
        let (corner, cut) = (self.corner_radius(), self.frame_cut());
        let metrics = self.chrome_metrics();
        let surface: Element<'_, Message> = if let Some(page) = self.browser.library {
            self.internal_page_view(page)
        } else if self.browser.pdf.is_some()
            || self
                .browser
                .pdf_requests
                .contains_key(&self.browser.saved.tabs[self.browser.saved.active].key)
        {
            self.pdf_view()
        } else if let Some(page) =
            InternalPage::from_url(&self.browser.saved.tabs[self.browser.saved.active].url)
        {
            self.internal_page_view(page)
        } else if let Some(error) = self.webview.borrow().native_error.clone() {
            self.navigation_error_view(&error)
        } else {
            let page = self.webview.borrow().live_view.map_or_else(
                || self.webview_surface(),
                |view| {
                    self.surface_input(
                        self.webview_surface(),
                        crate::browser_input::Surface::Web(view),
                    )
                },
            );
            self.load_progress_over(page)
        };
        // In fullscreen a website is shown alone, across the window.
        let bare = self.fullscreen_bare();
        let page: Element<'_, Message> = if bare {
            container(surface)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_| container::Style {
                    background: Some(iced::Color::BLACK.into()),
                    ..Default::default()
                })
                .into()
        } else {
            let surface: Element<'_, Message> = if self.browser.panel == Panel::Find {
                column![container(surface).height(Length::Fill), self.find_bar()].into()
            } else {
                surface
            };
            let surface: Element<'_, Message> = if self.browser.devtools.open {
                column![
                    container(surface).height(Length::Fill),
                    self.devtools_panel()
                ]
                .into()
            } else {
                surface
            };
            let page_background = if self.browser.library.is_none()
                && self.browser.pdf.is_none()
                && crate::privacy::Privacy::origin(
                    &self.browser.saved.tabs[self.browser.saved.active].url,
                )
                .is_some()
                // An error is the browser's own page, in the theme's colours;
                // the colour of the page that failed is not its ground.
                && self.webview.borrow().native_error.is_none()
            {
                self.webview.borrow().page_background
            } else {
                None
            };
            let frame = enzui_widgets::surface::enz_surface(surface)
                .fill_size()
                .radius(page_radius)
                .fill(page_background.unwrap_or(p.surface))
                .border_color(p.border)
                .border_width(1.0)
                .elevation(2)
                .clip(true)
                .into_element();
            let frame = self.with_assistant(frame);
            if let Some(toolbar) = immersive_parts {
                self.immersive_view(frame, toolbar, radius)
            } else {
                let top_bar: Element<'_, Message> =
                    enzui_widgets::titlebar_drag_region::titlebar_drag_region(
                        // The bar's own height is its controls and their
                        // padding, a fraction of a pixel off a whole number;
                        // only a height a theme chose is laid out as given.
                        if self.popup_window {
                            container(toolbar)
                                .padding([0, 10])
                                .height(popups::POPUP_BAR_HEIGHT)
                                .align_y(iced::Alignment::Center)
                        } else if metrics.toolbar == crate::appearance::TOOLBAR.2 {
                            container(toolbar).padding([8, 10])
                        } else {
                            container(toolbar)
                                .padding([0, 10])
                                // Never lower than the address bar in it.
                                .height(self.top_bar_height())
                                .align_y(iced::Alignment::Center)
                        }
                    )
                    .on_gesture(|command| c(Command::Window(command)))
                    .into();
                let content: Element<'_, Message> = match self.effective_tab_layout() {
                    Some(Layout::Top) => column![self.tab_strip_for(Layout::Top), frame]
                        .spacing(2)
                        .into(),
                    Some(layout) if self.browser.saved.right => {
                        row![frame, self.tab_strip_for(layout)].spacing(6).into()
                    }
                    Some(layout) => row![self.tab_strip_for(layout), frame].spacing(6).into(),
                    None => frame,
                };
                let page: Element<'_, Message> = container(
                    column![
                        top_bar,
                        (!self.popup_window)
                            .then(|| self.bookmark_bar())
                            .flatten()
                            .unwrap_or_else(|| iced::widget::Space::new().height(0).into()),
                        container(content)
                            .padding(iced::Padding {
                                top: 0.0,
                                right: 10.0,
                                bottom: 10.0,
                                left: 10.0
                            })
                            .height(Length::Fill),
                    ]
                    .height(Length::Fill),
                )
                .style(move |_| container::Style {
                    text_color: Some(p.text),
                    ..Default::default()
                })
                .into();
                page
            }
        };
        let modal = if self.assistant.draft.is_some() {
            self.assistant_service_dialog()
        } else if self.assistant.prompt.is_some() {
            self.assistant_prompt_dialog()
        } else if self.browser.engine_edit {
            Some(self.search_engine_dialog())
        } else {
            match self.browser.panel {
                Panel::PermissionSettings => Some(self.permission_settings_dialog()),
                Panel::Passkey => Some(self.passkey_dialog()),
                // The popup hangs from its button in the sidebar; without a
                // sidebar it is a dialog.
                Panel::Theme
                    if !matches!(
                        self.effective_tab_layout(),
                        Some(Layout::Sidebar | Layout::Icons)
                    ) =>
                {
                    Some(self.appearance_dialog())
                }
                Panel::New => Some(self.new_tab_dialog()),
                Panel::Group(g) => Some(self.group_editor(g)),
                Panel::HistoryClear => Some(self.history_clear_dialog()),
                Panel::CookieClear => Some(self.cookie_clear_dialog()),
                Panel::Cookies => Some(self.cookies_dialog()),
                Panel::ContainerDelete => Some(self.container_editor(true)),
                Panel::Script
                | Panel::SignIn
                | Panel::BookmarkEdit
                | Panel::Color
                | Panel::Print
                | Panel::Export
                | Panel::History => {
                    Some(
                        container(self.panel_body(self.browser.panel))
                            .padding(0)
                            .width((self.browser.viewport.width - 24.).clamp(180., 520.))
                            .style(move |_| container::Style {
                                background: Some(p.surface_elevated.into()),
                                border: iced::Border {
                                    radius: radius.into(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            })
                            .into(),
                    )
                }
                _ => None,
            }
        };
        // The frame is the one layer between the desktop and the chrome: a
        // glass frame lets the compositor's picture of what is behind show
        // through it, so nothing opaque may lie under or over it.
        let page: Element<'_, Message> = if bare {
            page
        } else {
            match self.glass_paint() {
                // The compositor paints the framework's material behind the
                // window: the frame is that glass, tinted with its own colour.
                Some((level, paint)) if self.backdrop().is_material() => {
                    let style = level.material();
                    let mut tokens = self.frame_material(level);
                    // The popup's values, as far as the material has them: the
                    // tint, and the edge's light relative to the level's own.
                    tokens.tint_opacity = paint.tint;
                    tokens.highlight *= paint.edge / level.edge();
                    enzui_widgets::surface::enz_surface(page)
                        .fill_size()
                        .radius(corner)
                        .material(style)
                        .material_tokens(tokens)
                        .material_ink(enzui_theme::MaterialInk::Strong)
                        // The chrome's fields and rows are drawn in the theme's
                        // colours; ink that followed the backdrop would leave
                        // light text on their light fills.
                        .glass_ink(enzui_theme::GlassInk::Theme)
                        .fill(p.background)
                        .backdrop(enzui_widgets::desktop_glass::Backdrop::Window)
                        .clip(true)
                }
                // Elsewhere the compositor shows what is behind, blurred or not,
                // and the frame is painted: its colour as a tint, and the light
                // glass catches on its edge and across it.
                Some((_, paint)) => {
                    let light = iced::widget::canvas::Canvas::new(self.frame_light(paint))
                        .width(Length::Fill)
                    .height(Length::Fill);
                    enzui_widgets::surface::enz_surface(iced::widget::stack![light, page])
                        .fill_size()
                        .radius(cut)
                        .fill(p.background.scale_alpha(paint.tint))
                        .clip(true)
                }
                None => {
                    let page = enzui_widgets::surface::enz_surface(page)
                        .fill_size()
                        .radius(cut)
                        .fill(p.background)
                        .clip(true);
                    if metrics.frame == crate::appearance::Frame::Grain {
                        page.grain(enzui_theme::GrainSpec::Theme)
                    } else {
                        page
                    }
                }
            }
            .into_element()
        };
        let page = self
            .browser
            .context
            .host(
                enzui_widgets::context_target::enz_context_target(page, |p| {
                    c(Command::ChromeContext(p))
                })
                .into_element(),
                |e| c(Command::Context(e)),
            )
            .into_element();
        let page = self.option_list_over(page);
        let page = self.date_picker_over(page);
        let page = self.credential_inline_host(page);
        let page = self
            .browser
            .chooser
            .view(page, |e| c(Command::ChooserEvent(e)));
        let placement = if self.browser.panel == Panel::New && !self.browser.engine_edit {
            enzui_widgets::modal_host::SurfacePlacement::TopInset(self.new_tab_top_inset())
        } else {
            enzui_widgets::modal_host::SurfacePlacement::Centred
        };
        enzui_widgets::modal_host::enz_modal_host(page, modal, |e| c(Command::Modal(e)))
            .placement(placement)
            .scope(iced::advanced::widget::Id::new("browser-modal"))
            .label("Browser dialog")
            .into_element()
    }
}

fn navigation_button<'a>(
    symbol: &str,
    label: &str,
    message: Message,
    enabled: bool,
) -> Element<'a, Message> {
    enzui_widgets::focus_button::focus_button_maybe(
        enzui_widgets::icon::enz_icon(symbol)
            .size(16.)
            .tint_resolved(|theme| palette(theme).text)
            .opacity(if enabled { 1. } else { 0.22 })
            .into_element(),
        enabled.then_some(message),
    )
    .width(28)
    .height(28)
    .padding(6)
    .semantics(
        enzui_core::semantics::Semantics::new(enzui_core::semantics::Role::Button).label(label),
    )
    .style_focus(move |theme, status| {
        let p = palette(theme);
        iced::widget::button::Style {
            text_color: if enabled {
                p.text
            } else {
                p.text.scale_alpha(0.22)
            },
            background: if enabled
                && matches!(
                    status,
                    enzui_widgets::focus_button::FocusStatus::Hovered
                        | enzui_widgets::focus_button::FocusStatus::Pressed
                ) {
                Some(p.surface_hover.into())
            } else {
                None
            },
            border: iced::Border {
                radius: 14.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    })
    .into()
}

#[cfg(test)]
pub(crate) mod browser_tests {
    use super::*;
    pub(crate) fn app() -> App {
        let mut app = App::new(enzui_theme::EnzTheme::dark());
        app.browser.saved = Saved::default();
        app.browser.container_routes = app
            .browser
            .saved
            .containers
            .iter()
            .map(|entry| (entry.id, crate::container_privacy::route(&entry.privacy)))
            .collect();
        app.browser.library = None;
        app.browser.saved.tabs.push(TabInfo {
            key: next_tab_key(),
            favicon: None,
            url: String::new(),
            title: "New tab".into(),
            pinned: false,
            group: None,
            container: None,
        });
        app.browser.sleeping = vec![None];
        app
    }
    #[test]
    fn browser_tests_download_controls_ignore_superseded_acknowledgements() {
        use enzui_webkit::page::{PageEvent, PageRequestId};
        let mut a = app();
        a.browser.private = true;
        a.browser.downloads = crate::downloads::Store::default();
        let owner = a.webview.borrow().live_view.unwrap();
        a.browser.downloads.accept(
            owner,
            &PageEvent::DownloadRequested {
                id: PageRequestId::new(14),
                url: enzui_webkit::web::policy::PolicyUrl::new("https://example.com/file"),
            },
        );
        let e = &mut a.browser.downloads.entries[0];
        let id = e.model.id.0;
        e.pending = true;
        e.retrying = true;
        e.control = 2;
        let _ = a.download_command(DownloadCommand::Ready(
            id,
            1,
            Err("stale acknowledgement".into()),
        ));
        let e = &a.browser.downloads.entries[0];
        assert!(e.pending);
        assert!(e.error.is_empty());
        assert_eq!(e.model.phase, enzui_core::download::Phase::Queued);
        let _ = a.download_command(DownloadCommand::Ready(
            id,
            2,
            Err("current acknowledgement".into()),
        ));
        let e = &a.browser.downloads.entries[0];
        assert!(!e.pending);
        assert_eq!(e.model.phase, enzui_core::download::Phase::Failed);
        let e = &mut a.browser.downloads.entries[0];
        e.model.retry();
        e.control = 3;
        e.pending = true;
        e.retrying = true;
        let _ = a.download_command(DownloadCommand::CancelledRequest(
            id,
            3,
            Ok(crate::native_jobs::Payload::Json(serde_json::Value::Null)),
        ));
        let e = &a.browser.downloads.entries[0];
        assert!(!e.pending && !e.retrying);
        assert_eq!(e.model.phase, enzui_core::download::Phase::Cancelled);
    }
    #[test]
    fn browser_tests_downloads_survive_tab_closure_and_release_their_owner_on_completion() {
        use enzui_webkit::page::{PageEvent, PageRequestId};
        let mut a = app();
        a.browser.private = true;
        a.browser.downloads = crate::downloads::Store::default();
        let owner = a.webview.borrow().live_view.unwrap();
        let id = PageRequestId::new(14);
        a.webview
            .borrow_mut()
            .page_events
            .push(PageEvent::DownloadRequested {
                id,
                url: enzui_webkit::web::policy::PolicyUrl::new("https://example.com/file"),
            });
        let _ = a.update(c(Command::Close(0)));
        assert_eq!(a.browser.download_hosts.len(), 1);
        assert!(a.browser.downloads.owns_active(owner));
        a.browser.download_hosts[0]
            .borrow_mut()
            .page_events
            .extend([
                PageEvent::DownloadProgress {
                    id,
                    received: 123,
                    expected: Some(123),
                },
                PageEvent::DownloadFinished {
                    id,
                    path: "/tmp/retained-download".into(),
                },
            ]);
        a.sync_downloads();
        assert!(a.browser.download_hosts.is_empty());
        assert_eq!(
            a.browser.downloads.entries.last().unwrap().model.phase,
            enzui_core::download::Phase::Complete
        );
        assert_eq!(a.browser.downloads.archive()[0].bytes, 123);
    }
    #[test]
    fn browser_tests_accessibility_ids_are_unique_across_pages_and_popups() {
        use iced::advanced::{renderer::Headless, widget::Operation};
        use iced_test::runtime::user_interface::{Cache, UserInterface};
        let mut renderer = iced_test::futures::futures::executor::block_on(
            <iced_test::renderer::Renderer as Headless>::new(
                iced::advanced::renderer::Settings::default(),
                None,
            ),
        )
        .expect("headless renderer");
        let mut app = app();
        for page in [
            InternalPage::New,
            InternalPage::Settings,
            InternalPage::History,
            InternalPage::Cookies,
            InternalPage::Containers,
            InternalPage::Bookmarks,
            InternalPage::Import,
            InternalPage::Passwords,
        ] {
            app.show_internal_page(page);
            for panel in [
                Panel::None,
                Panel::New,
                Panel::Shields,
                Panel::Permissions,
                Panel::Export,
                Panel::Passwords,
            ] {
                app.browser.panel = panel;
                let mut ui = UserInterface::build(
                    app.chrome_view(),
                    iced::Size::new(1400., 900.),
                    Cache::default(),
                    &mut renderer,
                );
                let mut collector = enzui_core::collect::SemanticsCollector::new();
                ui.operate(&renderer, &mut collector as &mut dyn Operation);
                let (tree, ids) = collector.to_tree_update_with_ids();
                assert!(
                    !tree.nodes.is_empty() && !ids.is_empty(),
                    "Missing accessibility tree for {page:?}/{panel:?}"
                );
            }
        }
    }
    #[test]
    fn browser_tests_private_session_never_rewrites_regular_state() {
        let mut a = app();
        let root = std::env::temp_dir().join(format!(
            "enzim-private-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_micros()
        ));
        let path = root.join("session.json");
        a.browser.save_to(&path);
        let regular = std::fs::read(&path).expect("regular session positive control");
        a.browser.private = true;
        a.browser.saved.tabs[0].url = "https://private.example/secret".into();
        a.browser.save_to(&path);
        assert_eq!(std::fs::read(&path).expect("regular session"), regular);
        assert!(!String::from_utf8_lossy(&regular).contains("private.example"));
        std::fs::remove_dir_all(root).expect("cleanup");
    }
    #[test]
    fn browser_tests_selected_search_engine_routes_all_omnibox_entry_points() {
        for command in [0, 1, 2] {
            let mut a = app();
            a.browser_mode = true;
            let _ = a.update(c(Command::SelectEngine(1)));
            match command {
                0 => {
                    let _ = a.update(Message::BrowserAddress("rust &web".into()));
                    let _ = a.update(Message::BrowserGo);
                }
                1 => {
                    let _ = a.update(c(Command::Query("rust &web".into())));
                    let _ = a.update(c(Command::StartGo));
                }
                _ => {
                    let _ = a.update(c(Command::Query("rust &web".into())));
                    let _ = a.update(c(Command::Submit));
                }
            }
            let parsed = url::Url::parse(&a.address).expect("resolved address");
            assert_eq!(parsed.host_str(), Some("www.google.com"));
            assert_eq!(
                parsed
                    .query_pairs()
                    .find(|(k, _)| k == "q")
                    .expect("query")
                    .1,
                "rust &web"
            );
        }
    }
    #[test]
    fn browser_tests_private_and_container_profiles_reach_pending_native_open() {
        let mut a = app();
        a.browser_mode = true;
        a.browser.private = true;
        a.submit_browser_action(BrowserAction::Navigate("https://example.com".into()));
        let private = a.webview.borrow().profile.clone();
        assert!(private.kind().is_ephemeral());
        a.new_tab("https://rust-lang.org".into());
        assert!(private.shares_storage(&a.webview.borrow().profile));
        a.browser.private = false;
        a.new_tab(String::new());
        let id = a.browser.saved.containers[0].id;
        a.browser.saved.tabs[a.browser.saved.active].container = Some(id);
        a.submit_browser_action(BrowserAction::Navigate("https://example.com".into()));
        assert_eq!(
            a.webview.borrow().profile.id(),
            enzui_webkit::web::WebProfile::persistent(a.browser.saved.containers[0].storage_name())
                .id()
        );
        assert!(!private.shares_storage(&a.webview.borrow().profile));
    }
    #[test]
    fn browser_tests_start_page_suggestions_select_existing_tabs_and_container_name_opens_new_tab()
    {
        let mut a = app();
        a.browser_mode = true;
        a.browser.saved.history = vec![crate::history::Visit {
            id: 1,
            url: "https://www.youtube.com/watch?v=demo".into(),
            title: "A film - YouTube".into(),
            visited_at: 1,
            container: None,
        }];
        let mut bookmark = a.browser.saved.tabs[0].clone();
        bookmark.url = "https://example.org/handbook".into();
        bookmark.title = "Handbook".into();
        a.browser.saved.bookmarks.push(bookmark);
        for query in ["you", "handbook"] {
            a.show_internal_page(InternalPage::New);
            let count = a.browser.saved.tabs.len();
            let _ = a.update(c(Command::Query(query.into())));
            let choices = a.suggestions_for(query);
            assert_eq!(a.new_tab_suggestions()[0].url, choices[0].url);
            let _ = a.update(c(Command::SuggestionMove(false)));
            let _ = a.update(c(Command::StartGo));
            assert_eq!(a.browser.saved.tabs.len(), count);
            assert_eq!(
                a.browser.saved.tabs[a.browser.saved.active].url,
                choices[0].url
            );
        }
        a.show_internal_page(InternalPage::New);
        let _ = a.update(c(Command::Query("you".into())));
        assert!(a.browser.start_suggestions_open);
        let _ = a.update(Message::BrowserAddress("handbook".into()));
        let _ = a.update(c(Command::Escape));
        assert!(!a.browser.address_suggestions_open);
        assert!(a.browser.start_suggestions_open);
        let _ = a.update(c(Command::Escape));
        assert!(!a.browser.start_suggestions_open);
        assert_eq!(a.browser.query, "you");
        let _ = a.update(c(Command::SuggestionMove(false)));
        assert!(a.browser.start_suggestions_open);
        let count = a.browser.saved.tabs.len();
        let id = a.browser.saved.containers[0].id;
        a.open_library(InternalPage::Containers);
        let _ = a.update(c(Command::Container(ContainerCommand::NewTab(Some(id)))));
        assert_eq!(a.browser.saved.tabs.len(), count + 1);
        assert_eq!(
            a.browser.saved.tabs[a.browser.saved.active].container,
            Some(id)
        );
        assert!(a.browser.library.is_none());
        assert!(a.selected_new_tab_target().is_none());
        let _ = a.update(c(Command::New));
        let _ = a.update(c(Command::Query("handbook".into())));
        assert!(!a.browser.start_suggestions_open);
        let _ = a.update(c(Command::Dismiss));
        assert!(a.browser.query.is_empty());
        assert!(!a.browser.start_suggestions_open);
    }
    #[test]
    fn browser_tests_address_suggestions_share_ranking_and_submit_in_the_same_tab() {
        let mut a = app();
        a.browser_mode = true;
        a.browser.saved.history = vec![crate::history::Visit {
            id: 1,
            url: "https://www.youtube.com/watch?v=demo".into(),
            title: "A film - YouTube".into(),
            visited_at: 1,
            container: None,
        }];
        let mut bookmark = a.browser.saved.tabs[0].clone();
        bookmark.url = "https://example.org/handbook".into();
        bookmark.title = "Handbook".into();
        a.browser.saved.bookmarks.push(bookmark);
        let _ = a.update(c(Command::Query("you".into())));
        let new = a.new_tab_suggestions();
        let _ = a.update(Message::BrowserAddress("you".into()));
        let address = a.suggestions_for(&a.address);
        assert_eq!(
            new.iter().map(|s| (&s.url, &s.title)).collect::<Vec<_>>(),
            address
                .iter()
                .map(|s| (&s.url, &s.title))
                .collect::<Vec<_>>()
        );
        assert!(a.browser.address_suggestions_open);
        let count = a.browser.saved.tabs.len();
        let _ = a.update(c(Command::AddressSuggestionMove(false)));
        assert_eq!(
            a.selected_address_suggestion().as_deref(),
            Some("https://www.youtube.com/")
        );
        let _ = a.update(Message::BrowserGo);
        assert_eq!(a.browser.saved.tabs.len(), count);
        assert_eq!(
            a.browser.saved.tabs[a.browser.saved.active].url,
            "https://www.youtube.com/"
        );
        assert!(!a.browser.address_suggestions_open);
        let _ = a.update(Message::BrowserAddress(String::new()));
        assert!(a.browser.address_suggestions_open);
        let _ = a.update(c(Command::AddressSuggestionMove(false)));
        let empty_choice = a.selected_address_suggestion().expect("frequent site");
        let _ = a.update(Message::BrowserGo);
        assert_eq!(
            a.browser.saved.tabs[a.browser.saved.active].url,
            empty_choice
        );
        assert_eq!(a.browser.saved.tabs.len(), count);
        let _ = a.update(Message::BrowserAddress("handbook".into()));
        let _ = a.update(c(Command::AddressSuggestionMove(true)));
        assert_eq!(
            a.selected_address_suggestion().as_deref(),
            Some("https://example.org/handbook")
        );
        let _ = a.update(c(Command::Escape));
        assert!(!a.browser.address_suggestions_open);
        assert!(a.selected_address_suggestion().is_none());
        let _ = a.update(Message::BrowserAddress("you".into()));
        let _ = a.update(c(Command::AddressSuggestionMove(false)));
        let _ = a.update(c(Command::Navigate("https://example.org/handbook".into())));
        assert_eq!(
            a.browser.saved.tabs[a.browser.saved.active].url,
            "https://example.org/handbook"
        );
        assert_eq!(a.browser.saved.tabs.len(), count);
    }
    #[test]
    fn browser_tests_local_access_keeps_the_selected_tab_and_profile() {
        for private in [false, true] {
            let mut a = app();
            a.browser_mode = true;
            a.browser.private = private;
            a.submit_browser_action(BrowserAction::Navigate("https://example.org/".into()));
            a.address = "http://192.168.50.1:8080/project".into();
            let original = a.webview.borrow().profile.clone();
            let count = a.browser.saved.tabs.len();
            let containers = a.browser.saved.containers.len();
            let _ = a.update(c(Command::OpenLocal));
            assert!(original.shares_storage(&a.webview.borrow().profile));
            assert_eq!(a.browser.saved.tabs.len(), count);
            assert_eq!(a.browser.saved.containers.len(), containers);
            assert_eq!(a.browser.saved.tabs[a.browser.saved.active].container, None);
            assert_eq!(
                a.browser.local_grants,
                vec![(None, "http://192.168.50.1:8080".into())]
            );
            let _ = a.update(c(Command::OpenLocal));
            assert_eq!(a.browser.local_grants.len(), 1);
            a.address = "file:///etc/passwd".into();
            let _ = a.update(c(Command::OpenLocal));
            assert_eq!(a.browser.local_grants.len(), 1);
        }
    }
    #[test]
    fn browser_tests_background_credential_updates_preserve_open_menus() {
        use enzui_core::menu_model::OpenCause;
        use enzui_widgets::menu_button::MenuButtonEvent;
        let mut a = app();
        a.browser_mode = true;
        a.open_library(InternalPage::Passwords);
        for zoom in [false, true] {
            let open = MenuButtonEvent::Open(OpenCause::Pointer);
            let _ = a.update(c(if zoom {
                Command::ZoomMenu(open)
            } else {
                Command::Menu(open)
            }));
            for _ in 0..4 {
                let _ = a.update(c(Command::Credential(CredentialCommand::Tick)));
                let _ = a.update(c(Command::MaintenancePoll));
                assert!(
                    if zoom {
                        a.browser.zoom_menu_state.is_open()
                    } else {
                        a.browser.menu_state.is_open()
                    },
                    "background polling must not dismiss a user-opened menu"
                );
            }
            let _ = a.update(c(Command::Escape));
            assert_eq!(
                a.browser.library,
                Some(InternalPage::Passwords),
                "Escape closes only the menu above the manager"
            );
            assert!(
                !a.browser.menu_state.is_open() && !a.browser.zoom_menu_state.is_open(),
                "explicit dismissal still closes menus"
            );
        }
        let _ = a.update(c(Command::Menu(MenuButtonEvent::Open(OpenCause::Pointer))));
        let _ = a.update(c(Command::Credential(CredentialCommand::Open)));
        assert!(
            !a.browser.menu_state.is_open(),
            "opening password management remains a user action"
        );
    }
    #[test]
    fn browser_tests_framework_menu_dispatches_and_closes() {
        use enzui_core::menu_model::{CheckState, OpenCause};
        use enzui_widgets::menu_button::MenuButtonEvent;
        let mut a = app();
        let _ = a.update(c(Command::Menu(MenuButtonEvent::Open(OpenCause::Pointer))));
        assert!(a.browser.menu_state.is_open());
        let _ = a.update(c(Command::Menu(MenuButtonEvent::Activated {
            id: "settings".into(),
            check: CheckState::None,
        })));
        assert!(!a.browser.menu_state.is_open());
        assert_eq!(a.browser.panel, Panel::None);
        assert_eq!(
            a.browser.saved.tabs[a.browser.saved.active].url,
            "enzim://settings"
        );
        assert!(a.webview.borrow().native.is_none());
        let _ = a.update(c(Command::Menu(MenuButtonEvent::Open(
            OpenCause::KeyboardFirst,
        ))));
        let _ = a.update(c(Command::Menu(MenuButtonEvent::Activated {
            id: "new".into(),
            check: CheckState::None,
        })));
        assert_eq!(a.browser.panel, Panel::New);
        assert!(!a.browser.menu_state.is_open());
    }
    #[test]
    fn browser_tests_empty_native_locations_preserve_documents_and_later_redirects() {
        let mut a = app();
        a.browser_mode = true;
        a.browser.saved.tabs[0].url = "https://example.org/form".into();
        a.new_tab("https://www.youtube.com/".into());
        a.webview.borrow_mut().location = Some(String::new());
        a.browser.sleeping[0]
            .as_ref()
            .unwrap()
            .host
            .borrow_mut()
            .location = Some(String::new());
        let _ = a.update(c(Command::MaintenancePoll));
        assert_eq!(a.browser.saved.tabs[1].url, "https://www.youtube.com/");
        assert_eq!(a.address, "https://www.youtube.com/");
        assert_eq!(a.browser.saved.tabs[0].url, "https://example.org/form");
        a.webview.borrow_mut().location = Some("https://www.youtube.com/#ready".into());
        a.browser.sleeping[0]
            .as_ref()
            .unwrap()
            .host
            .borrow_mut()
            .location = Some("https://example.org/redirect".into());
        a.sync_browser_metadata();
        assert_eq!(
            a.browser.saved.tabs[1].url,
            "https://www.youtube.com/#ready"
        );
        assert_eq!(a.browser.saved.tabs[0].url, "https://example.org/redirect");
    }
    #[test]
    fn browser_tests_internal_pages_reuse_tabs_and_never_start_webkit() {
        let mut a = app();
        let _ = a.update(c(Command::Panel(Panel::Settings)));
        let settings = a.browser.saved.active;
        let count = a.browser.saved.tabs.len();
        let _ = a.update(c(Command::Panel(Panel::Bookmarks)));
        assert_eq!(a.browser.library, Some(InternalPage::Bookmarks));
        assert_eq!(a.address, "enzim://bookmarks");
        assert_eq!(a.browser.saved.tabs[settings].url, "enzim://settings");
        assert_eq!(a.browser.saved.tabs.len(), count);
        a.sync_browser();
        assert_eq!(a.browser.saved.tabs[settings].title, "Settings");
        let _ = a.update(c(Command::Panel(Panel::Settings)));
        assert_eq!(a.browser.library, None);
        assert_eq!(a.browser.saved.active, settings);
        assert_eq!(a.browser.saved.tabs.len(), count);
        assert!(a.webview.borrow().native.is_none());
        assert!(!a.pending_native_open);
    }
    #[test]
    fn browser_tests_library_switches_preserve_the_source_tab_and_navigation_closes_it() {
        let mut a = app();
        a.browser_mode = true;
        a.browser.saved.tabs[0].url = "https://example.org/form".into();
        let key = a.browser.saved.tabs[0].key;
        for page in [
            InternalPage::Bookmarks,
            InternalPage::History,
            InternalPage::Containers,
        ] {
            a.open_library(page);
            assert_eq!(a.browser.library, Some(page));
            assert_eq!(a.browser.saved.tabs.len(), 1);
            assert_eq!(a.browser.saved.tabs[0].key, key);
            assert_eq!(a.browser.saved.tabs[0].url, "https://example.org/form");
            assert!(!a.webview.borrow().is_visible());
        }
        let _ = a.update(Message::BrowserBack);
        assert_eq!(a.browser.library, None);
        assert_eq!(a.address, "https://example.org/form");
        assert!(a.webview.borrow().is_visible());
        a.open_library(InternalPage::History);
        let _ = a.update(Message::BrowserAddress("https://example.net/new".into()));
        let _ = a.update(Message::BrowserGo);
        assert_eq!(a.browser.library, None);
        assert_eq!(a.browser.saved.tabs.len(), 1);
        assert_eq!(a.browser.saved.tabs[0].url, "https://example.net/new");
        let _ = a.update(Message::BrowserAddress("enzim://bookmarks".into()));
        let _ = a.update(Message::BrowserGo);
        assert_eq!(a.browser.library, Some(InternalPage::Bookmarks));
        assert_eq!(a.browser.saved.tabs[0].url, "https://example.net/new");
    }
    #[test]
    fn browser_tests_unknown_internal_pages_restore_as_new_tabs_without_opening_webkit() {
        let mut a = app();
        a.browser_mode = true;
        a.browser.saved.tabs[0].url = "https://example.org/form".into();
        a.new_tab("enzim://retired-page".into());
        let key = a.browser.saved.tabs[1].key;
        a.browser.saved.tabs[1].title = "Retired page".into();
        assert_eq!(a.browser.saved.migrate_library_tabs(), None);
        assert_eq!(a.browser.saved.tabs[0].url, "https://example.org/form");
        assert_eq!(a.browser.saved.tabs[1].key, key);
        assert_eq!(a.browser.saved.tabs[1].url, "");
        assert_eq!(a.browser.saved.tabs[1].title, "New tab");
        assert_eq!(a.browser.saved.active, 1);
        assert_eq!(InternalPage::from_url("enzim://qa"), None);
        a.prepare_navigation(BrowserAction::Navigate("enzim://unavailable".into()));
        assert!(a.browser.saved.tabs[1].url.is_empty());
        assert!(a.webview.borrow().native.is_none());
        assert!(!a.pending_native_open);
        assert!(a.pending_navigation.is_none());
    }
    #[test]
    fn browser_tests_legacy_library_tabs_migrate_without_removing_websites() {
        let mut a = app();
        a.new_tab("https://example.org".into());
        a.new_tab("enzim://history".into());
        a.new_tab("enzim://bookmarks".into());
        let selected = a.browser.saved.active;
        assert_eq!(
            a.browser.saved.migrate_library_tabs(),
            Some(InternalPage::Bookmarks)
        );
        assert_eq!(a.browser.saved.tabs.len(), 2);
        assert!(
            a.browser
                .saved
                .tabs
                .iter()
                .any(|tab| tab.url == "https://example.org")
        );
        assert!(a.browser.saved.active < selected);
        a.browser.saved.tabs.retain(|tab| tab.url.is_empty());
        a.browser.saved.tabs[0].url = "enzim://containers".into();
        a.browser.saved.active = 0;
        assert_eq!(
            a.browser.saved.migrate_library_tabs(),
            Some(InternalPage::Containers)
        );
        assert_eq!(a.browser.saved.tabs.len(), 1);
        assert!(a.browser.saved.tabs[0].url.is_empty());
    }
    #[test]
    fn browser_tests_refinement_mixed_order_moves_groups_and_extracts_a_live_tab_after_a_group() {
        use enzui_core::drag::{DragOperation, DropRequest, InsertionSlot};
        let mut a = app();
        a.browser_mode = true;
        a.new_tab("https://example.com".into());
        a.new_tab("https://rust-lang.org".into());
        a.new_tab("https://wikipedia.org".into());
        let keys: Vec<_> = a.browser.saved.tabs.iter().map(|t| t.key).collect();
        let _ = a.browser_command(Command::Group(1));
        let last = a
            .browser
            .saved
            .tabs
            .iter()
            .position(|t| t.key == keys[3])
            .unwrap();
        let _ = a.browser_command(Command::Group(last));
        assert_eq!(
            a.browser.strip_order(),
            vec![
                TabStripEntry::Tab(keys[0]),
                TabStripEntry::Group(0),
                TabStripEntry::Tab(keys[2]),
                TabStripEntry::Group(1)
            ]
        );
        let drop = |source: &str, destination: &str, item: String, slot| DropRequest {
            source: source.into(),
            destination: destination.into(),
            items: vec![enzui_core::drag::ItemId::new(item)],
            slot: InsertionSlot(slot),
            operation: DragOperation::Move,
        };
        let _ = a.browser_command(Command::DropTabs(drop("main", "main", "group:1".into(), 1)));
        assert_eq!(
            a.browser.strip_order(),
            vec![
                TabStripEntry::Tab(keys[0]),
                TabStripEntry::Group(1),
                TabStripEntry::Group(0),
                TabStripEntry::Tab(keys[2])
            ]
        );
        let i = a
            .browser
            .saved
            .tabs
            .iter()
            .position(|t| t.key == keys[1])
            .unwrap();
        a.browser.sleeping[i]
            .as_ref()
            .unwrap()
            .host
            .borrow_mut()
            .dom_report = Some("Form and scroll retained".into());
        let _ = a.browser_command(Command::DropTabs(drop(
            "group:0",
            "main",
            format!("tab:{}", keys[1]),
            3,
        )));
        assert_eq!(
            a.browser.strip_order(),
            vec![
                TabStripEntry::Tab(keys[0]),
                TabStripEntry::Group(1),
                TabStripEntry::Group(0),
                TabStripEntry::Tab(keys[1]),
                TabStripEntry::Tab(keys[2])
            ]
        );
        assert_eq!(a.browser.saved.tabs[a.browser.saved.active].key, keys[3]);
        let moved = a
            .browser
            .saved
            .tabs
            .iter()
            .position(|t| t.key == keys[1])
            .unwrap();
        assert_eq!(a.browser.saved.tabs[moved].group, None);
        assert_eq!(
            a.browser.sleeping[moved]
                .as_ref()
                .unwrap()
                .host
                .borrow()
                .dom_report
                .as_deref(),
            Some("Form and scroll retained")
        );
        let bytes = serde_json::to_vec(&a.browser.saved).unwrap();
        let restored: Saved = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored.tab_order, a.browser.strip_order());
        let new = next_tab_key();
        a.browser.replace_tab_key(moved, new);
        assert_eq!(a.browser.strip_order()[3], TabStripEntry::Tab(new));
    }
    #[test]
    fn browser_tests_refinement_suggestions_use_titles_filter_bookmarks_and_commit_keyboard_selection()
     {
        let mut a = app();
        a.browser_mode = true;
        a.browser.saved.history = vec![crate::history::Visit {
            url: "https://www.youtube.com/watch?v=film".into(),
            title: "A useful film - YouTube".into(),
            visited_at: 100,
            ..Default::default()
        }];
        let mut bookmark = a.browser.saved.tabs[0].clone();
        bookmark.url = "https://docs.rust-lang.org/book".into();
        bookmark.title = "Language handbook".into();
        a.browser.saved.bookmarks.push(bookmark);
        let _ = a.update(c(Command::New));
        assert!(a.new_tab_suggestions().iter().any(|s| s.title == "YouTube"));
        let _ = a.update(c(Command::Query("handbook".into())));
        assert_eq!(
            a.new_tab_suggestions()[0].url,
            "https://docs.rust-lang.org/book"
        );
        let _ = a.update(c(Command::Query("you".into())));
        assert_eq!(a.new_tab_suggestions()[0].title, "YouTube");
        assert_eq!(a.new_tab_suggestions()[0].url, "https://www.youtube.com/");
        assert!(
            a.new_tab_suggestions()
                .iter()
                .any(|s| s.url.contains("watch?v=film"))
        );
        let _ = a.update(c(Command::SuggestionMove(false)));
        let _ = a.update(c(Command::Submit));
        assert_eq!(
            a.browser.saved.tabs[a.browser.saved.active].url,
            "https://www.youtube.com/"
        );
        let _ = a.update(c(Command::New));
        let _ = a.update(c(Command::Query("unmatched words".into())));
        assert!(a.browser.suggestion_highlight.is_none());
        let _ = a.update(c(Command::Submit));
        assert!(
            a.address
                .starts_with("https://duckduckgo.com/?q=unmatched+words")
        );
    }
    #[test]
    fn browser_tests_refinement_privacy_completion_follows_background_tab_and_rejects_rotated_key()
    {
        let mut a = app();
        a.new_tab("https://example.com/first".into());
        let first = a.browser.saved.tabs[a.browser.saved.active].key;
        a.new_tab("https://example.org/second".into());
        let second = a.browser.saved.tabs[a.browser.saved.active].key;
        let complete = |key| {
            Command::Shields(ShieldCommand::Ready(
                key,
                BrowserAction::Reload,
                Err("originating tab marker".into()),
            ))
        };
        let _ = a.browser_command(complete(first));
        assert_eq!(
            a.navigation_host(first)
                .unwrap()
                .borrow()
                .native_error
                .as_deref(),
            Some(
                "Privacy protection could not be prepared: originating tab marker. Try again, or change Privacy settings."
            )
        );
        assert!(
            a.navigation_host(second)
                .unwrap()
                .borrow()
                .native_error
                .is_none()
        );
        let index = a
            .browser
            .saved
            .tabs
            .iter()
            .position(|tab| tab.key == first)
            .unwrap();
        a.browser.replace_tab_key(index, next_tab_key());
        let _ = a.browser_command(complete(first));
        assert!(a.navigation_host(first).is_none());
        assert!(
            a.navigation_host(second)
                .unwrap()
                .borrow()
                .native_error
                .is_none()
        );
    }
    #[test]
    fn browser_tests_refinement_container_icon_edit_retains_storage_identity_and_legacy_profiles_load()
     {
        let mut a = app();
        let id = a.browser.saved.containers[1].id;
        let _ = a.container_command(ContainerCommand::Edit(Some(id)));
        let _ = a.container_command(ContainerCommand::Icon(7));
        let _ = a.container_command(ContainerCommand::Color(6));
        let _ = a.container_command(ContainerCommand::Save);
        let entry = a
            .browser
            .saved
            .containers
            .iter()
            .find(|c| c.id == id)
            .unwrap();
        assert_eq!(entry.icon_name(), "plane");
        assert_eq!(entry.color, 6);
        let mut value = serde_json::to_value(&a.browser.saved).unwrap();
        value.as_object_mut().unwrap().remove("tab_order");
        value["containers"][1]
            .as_object_mut()
            .unwrap()
            .remove("icon");
        let old: Saved = serde_json::from_value(value).unwrap();
        assert_eq!(old.containers[1].id, id);
        assert_eq!(old.containers[1].icon_name(), "briefcase-business");
    }
    #[test]
    fn browser_tests_drag_moves_live_tabs_between_groups_and_pins() {
        use enzui_core::drag::{DragOperation, DropRequest, InsertionSlot};
        let mut a = app();
        a.new_tab("https://example.com".into());
        a.new_tab("https://rust-lang.org".into());
        a.browser.saved.groups = vec![
            Group {
                name: "Reading".into(),
                color: 0,
            },
            Group {
                name: "Work".into(),
                color: 5,
            },
        ];
        a.browser.saved.tabs[1].group = Some(0);
        let key = a.browser.saved.tabs[1].key;
        let active = a.browser.saved.tabs[a.browser.saved.active].key;
        a.browser.sleeping[1]
            .as_ref()
            .unwrap()
            .host
            .borrow_mut()
            .dom_report = Some("Retained page state".into());
        let request = |source: &str, destination: &str| DropRequest {
            source: source.into(),
            destination: destination.into(),
            items: vec![enzui_core::drag::ItemId::new(format!("tab:{key}"))],
            slot: InsertionSlot(1),
            operation: DragOperation::Move,
        };
        let _ = a.update(c(Command::DropTabs(request("group:0", "group:1"))));
        let i = a
            .browser
            .saved
            .tabs
            .iter()
            .position(|t| t.key == key)
            .unwrap();
        assert_eq!(a.browser.saved.tabs[i].group, Some(1));
        assert_eq!(a.browser.saved.tabs[a.browser.saved.active].key, active);
        assert_eq!(
            a.browser.sleeping[i]
                .as_ref()
                .unwrap()
                .host
                .borrow()
                .dom_report
                .as_deref(),
            Some("Retained page state")
        );
        let _ = a.update(c(Command::DropTabs(request("group:1", "pinned"))));
        let i = a
            .browser
            .saved
            .tabs
            .iter()
            .position(|t| t.key == key)
            .unwrap();
        assert!(a.browser.saved.tabs[i].pinned);
        assert_eq!(a.browser.saved.tabs[i].group, None);
        let _ = a.update(c(Command::Close(i)));
        assert_eq!(a.browser.saved.tabs.len(), 3);
        let _ = a.update(c(Command::DropTabs(request("pinned", "main"))));
        let tab = a.browser.saved.tabs.iter().find(|t| t.key == key).unwrap();
        assert!(!tab.pinned);
        assert_eq!(tab.group, None);
    }
    #[test]
    fn browser_tests_drag_reorders_and_rejects_a_stale_group_drop() {
        use enzui_core::drag::{DragOperation, DropRequest, InsertionSlot};
        let mut a = app();
        a.new_tab("https://example.com".into());
        a.new_tab("https://rust-lang.org".into());
        let first = a.browser.saved.tabs[0].key;
        let second = a.browser.saved.tabs[1].key;
        let third = a.browser.saved.tabs[2].key;
        let _ = a.update(c(Command::DropTabs(DropRequest {
            source: "main".into(),
            destination: "main".into(),
            items: vec![enzui_core::drag::ItemId::new(format!("tab:{first}"))],
            slot: InsertionSlot(3),
            operation: DragOperation::Move,
        })));
        assert_eq!(
            a.browser
                .saved
                .tabs
                .iter()
                .map(|t| t.key)
                .collect::<Vec<_>>(),
            vec![second, third, first]
        );
        let before = a
            .browser
            .saved
            .tabs
            .iter()
            .map(|t| t.key)
            .collect::<Vec<_>>();
        let _ = a.update(c(Command::DropTabs(DropRequest {
            source: "main".into(),
            destination: "group:999".into(),
            items: vec![enzui_core::drag::ItemId::new(format!("tab:{first}"))],
            slot: InsertionSlot(0),
            operation: DragOperation::Move,
        })));
        assert_eq!(
            a.browser
                .saved
                .tabs
                .iter()
                .map(|t| t.key)
                .collect::<Vec<_>>(),
            before
        );
    }
    #[test]
    fn browser_tests_compact_layout_preserves_the_saved_preference() {
        let mut a = app();
        a.browser.saved.layout = Layout::Sidebar;
        for (width, expected) in [
            (1280.0, Some(Layout::Sidebar)),
            (800.0, Some(Layout::Icons)),
            (480.0, None),
        ] {
            let _ = a.update(c(Command::Viewport(iced::Size::new(width, 560.0))));
            assert_eq!(a.effective_tab_layout(), expected);
        }
        assert_eq!(a.browser.saved.layout, Layout::Sidebar);
        let _ = a.update(c(Command::Viewport(iced::Size::new(1280.0, 800.0))));
        assert_eq!(a.effective_tab_layout(), Some(Layout::Sidebar));
    }
    #[test]
    fn browser_tests_navigation_availability_follows_complete_native_metadata() {
        use crate::webview_native::NativeMetadata;
        let mut a = app();
        assert!(!a.webview.borrow().can_go_back);
        assert!(!a.webview.borrow().can_go_forward);
        a.webview
            .borrow_mut()
            .apply_native_metadata(std::sync::Arc::new(NativeMetadata {
                can_go_back: true,
                can_go_forward: false,
                ..Default::default()
            }));
        assert!(a.webview.borrow().can_go_back);
        assert!(!a.webview.borrow().can_go_forward);
        a.webview
            .borrow_mut()
            .apply_native_metadata(std::sync::Arc::new(NativeMetadata {
                can_go_back: false,
                can_go_forward: true,
                ..Default::default()
            }));
        assert!(!a.webview.borrow().can_go_back);
        assert!(a.webview.borrow().can_go_forward);
        a.show_internal_page(InternalPage::New);
        assert!(!a.webview.borrow().can_go_back);
        assert!(!a.webview.borrow().can_go_forward);
    }

    #[test]
    fn browser_tests_omnibox_routes_searches_and_addresses() {
        assert_eq!(
            resolve("rust async examples"),
            "https://duckduckgo.com/?q=rust+async+examples"
        );
        assert_eq!(resolve("example.com/a"), "https://example.com/a");
        assert_eq!(resolve("https://example.com"), "https://example.com");
        assert!(resolve("two words.test").starts_with("https://duckduckgo.com/?q="));
    }
    #[test]
    fn browser_tests_tabs_preserve_host_and_pinning_prevents_close() {
        let mut a = app();
        a.new_tab("https://example.com".into());
        a.new_tab("https://www.rust-lang.org".into());
        assert_eq!(a.browser.saved.tabs.len(), 3);
        assert!(a.browser.sleeping[1].is_some());
        a.select_tab(1);
        assert_eq!(a.address, "https://example.com");
        let key = a.browser.saved.tabs[1].key;
        let _ = a.browser_command(Command::Pin(1));
        let pinned = a
            .browser
            .saved
            .tabs
            .iter()
            .position(|t| t.key == key)
            .unwrap();
        let _ = a.browser_command(Command::Close(pinned));
        assert_eq!(a.browser.saved.tabs.len(), 3);
        let _ = a.browser_command(Command::Pin(pinned));
        let unpinned = a
            .browser
            .saved
            .tabs
            .iter()
            .position(|t| t.key == key)
            .unwrap();
        let _ = a.browser_command(Command::Close(unpinned));
        assert_eq!(a.browser.saved.tabs.len(), 2);
        let _ = a.browser_command(Command::Reopen);
        assert_eq!(a.browser.saved.tabs.len(), 3);
        assert_eq!(a.address, "https://example.com");
    }
    #[test]
    fn browser_tests_hovered_link_rebuilds_the_middle_button_target() {
        let mut a = app();
        a.browser_mode = true;
        a.browser.private = true;
        a.browser.saved.tabs[0].url = "https://example.com/".into();
        let before = a.browser.chrome_revision;
        a.webview
            .borrow_mut()
            .page_events
            .push(enzui_webkit::page::PageEvent::HoveredLink(Some(
                enzui_webkit::web::policy::PolicyUrl::new("https://example.com/"),
            )));
        let _ = a.update(c(Command::Pointer(iced::Point::new(200., 200.))));
        assert_eq!(
            a.browser.input.hovered_link.as_deref(),
            Some("https://example.com/")
        );
        assert!(a.browser.chrome_revision > before);
    }
    #[test]
    fn browser_tests_local_pdf_opens_separately_from_the_source_website() {
        let mut a = app();
        a.browser.private = true;
        a.browser.saved.tabs[0].url = "https://example.com/".into();
        a.browser.saved.tabs[0].container = Some(42);
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/two-pages.pdf");
        let _ = a.browser_command(Command::PdfOpen(path));
        assert_eq!(a.browser.saved.tabs.len(), 2);
        assert_eq!(a.browser.saved.tabs[0].url, "https://example.com/");
        assert_eq!(a.browser.saved.tabs[1].container, Some(42));
        assert!(a.browser.saved.tabs[1].url.is_empty());
        assert_eq!(a.browser.saved.tabs[1].title, "two-pages.pdf");
    }
    #[test]
    fn browser_tests_pdf_controls_reject_stale_renders_and_invalid_pages() {
        use crate::browser_pdf::PdfCommand as P;
        let mut a = app();
        a.browser.private = true;
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/two-pages.pdf");
        let page = crate::pdf::render(path.clone(), 1).unwrap();
        let key = a.browser.saved.tabs[0].key;
        let _ = a.open_pdf(path, None);
        let old = a.browser.pdf_requests[&key].ticket;
        let _ = a.pdf_command(P::Retry);
        let current = a.browser.pdf_requests[&key].ticket;
        assert_ne!(old, current);
        let _ = a.pdf_command(P::Rendered {
            key,
            ticket: old,
            result: Ok(page.clone()),
        });
        assert!(a.browser.pdf.is_none());
        let _ = a.pdf_command(P::Rendered {
            key,
            ticket: current,
            result: Ok(page.clone()),
        });
        assert_eq!(a.browser.pdf.as_ref().unwrap().number, 1);
        let _ = a.pdf_command(P::Scroll {
            key: key + 1,
            offset: iced::Vector::new(120., 80.),
        });
        assert_eq!(
            a.browser.pdf.as_ref().unwrap().view.scroll,
            iced::Vector::ZERO
        );
        let _ = a.pdf_command(P::Scroll {
            key,
            offset: iced::Vector::new(120., 80.),
        });
        assert_eq!(
            a.browser.pdf.as_ref().unwrap().view.scroll,
            iced::Vector::new(120., 80.)
        );
        let _ = a.pdf_command(P::Scroll {
            key,
            offset: iced::Vector::new(f32::NAN, -10.),
        });
        assert_eq!(
            a.browser.pdf.as_ref().unwrap().view.scroll,
            iced::Vector::ZERO
        );
        let _ = a.pdf_command(P::PageInput("999".into()));
        let _ = a.pdf_command(P::Go);
        assert!(!a.browser.pdf.as_ref().unwrap().view.error.is_empty());
        assert!(a.browser.pdf_requests.is_empty());
        let _ = a.pdf_command(P::Fit(crate::pdf::Fit::Custom));
        let _ = a.browser_command(Command::Zoom(10));
        assert_eq!(a.browser.pdf.as_ref().unwrap().view.percent, 110);
        assert_eq!(a.browser.zoom, 100);
        let _ = a.pdf_command(P::Rotate);
        assert_eq!(a.browser.pdf.as_ref().unwrap().view.turns, 1);
        let _ = a.pdf_command(P::Step(1));
        let _ = a.pdf_command(P::Step(1));
        assert_eq!(a.browser.pdf_requests[&key].number, 2);
        let _ = a.pdf_command(P::Zoom(10));
        assert_eq!(a.browser.pdf_requests[&key].number, 2);
        let ticket = a.browser.pdf_requests[&key].ticket;
        a.submit_browser_action(BrowserAction::Navigate("https://example.org".into()));
        let _ = a.pdf_command(P::Rendered {
            key,
            ticket,
            result: Ok(page),
        });
        assert!(a.browser.pdf.is_none());
        assert!(a.browser.pdf_requests.is_empty());
    }
    #[test]
    fn browser_tests_autoscroll_is_opt_in_latched_and_cancelled_on_tab_switch() {
        use crate::browser_input::{InputCommand as I, Surface};
        let mut a = app();
        a.browser.private = true;
        let view = a.webview.borrow().live_view.unwrap();
        let start = I::Start {
            surface: Surface::Web(view),
            window: iced::Point::new(200., 200.),
            local: iced::Vector::new(100., 100.),
        };
        a.browser.pointer = iced::Point::new(200., 200.);
        let _ = a.input_command(I::Bounds(
            Surface::Web(view),
            iced::Rectangle::new(iced::Point::ORIGIN, iced::Size::new(500., 400.)),
        ));
        let _ = a.input_command(start.clone());
        assert!(a.browser.input.anchor.is_none());
        let _ = a.input_command(I::Enabled(true));
        let _ = a.input_command(start.clone());
        assert!(a.browser.input.anchor.is_some());
        let _ = a.input_command(I::Click(iced::mouse::Button::Middle));
        assert!(a.browser.input.anchor.is_some());
        a.browser.pointer = iced::Point::new(200., 200.);
        let _ = a.input_command(I::Release);
        assert!(a.browser.input.anchor.is_some());
        let _ = a.browser_command(Command::Escape);
        assert!(a.browser.input.anchor.is_none());
        let _ = a.input_command(start.clone());
        a.browser.pointer = iced::Point::new(200., 240.);
        let _ = a.input_command(I::Release);
        assert!(a.browser.input.anchor.is_none());
        let _ = a.input_command(start);
        a.new_tab("https://example.com".into());
        assert!(a.browser.input.anchor.is_none());
        assert!(a.browser.saved.middle_autoscroll);
        assert_eq!(crate::browser_input::speed(12.), 0.);
        assert!(crate::browser_input::speed(40.) > crate::browser_input::speed(20.));
        assert_eq!(crate::browser_input::speed(-10000.), 2600.);
    }
    #[test]
    fn browser_tests_a_background_visit_never_leaves_the_tab_in_view() {
        let mut a = app();
        a.browser.saved.tabs[0].url = "https://example.com/".into();
        a.browser.input.hovered_link = Some("https://example.com/next".into());
        for _ in 0..2 {
            let _ = a.browser_command(Command::Action(actions::ActionCommand::VisitBackground(
                "https://example.com/next".into(),
            )));
        }
        assert_eq!(a.browser.saved.tabs.len(), 3);
        assert_eq!(a.browser.saved.active, 0);
        assert_eq!(a.browser.saved.tabs[2].url, "https://example.com/next");
        assert_eq!(a.browser.sleeping.len(), 3);
        // Switching away and back would forget the link under the pointer,
        // and the next middle-click would reach the page instead.
        assert_eq!(
            a.browser.input.hovered_link.as_deref(),
            Some("https://example.com/next")
        );
    }
    #[test]
    fn browser_tests_a_window_the_page_opened_with_the_middle_button_stays_behind() {
        let mut a = app();
        a.browser_mode = true;
        a.browser.private = true;
        a.browser.saved.tabs[0].url = "https://example.com/".into();
        for background in [true, false] {
            a.webview
                .borrow_mut()
                .page_events
                .push(enzui_webkit::page::PageEvent::PopupRequested {
                    url: enzui_webkit::web::policy::PolicyUrl::new("https://example.com/next"),
                    user_initiated: true,
                    background,
                });
            let _ = a.update(c(Command::Pointer(iced::Point::new(200., 200.))));
            if background {
                assert_eq!(a.browser.saved.tabs.len(), 2);
                assert_eq!(a.browser.saved.active, 0, "a middle-click opens behind");
            } else {
                assert_eq!(a.browser.saved.tabs.len(), 3);
                assert_eq!(a.browser.saved.active, 2, "a page's own window comes to the front");
            }
        }
    }
    #[test]
    fn browser_tests_a_group_can_be_ungrouped_or_closed() {
        let mut a = app();
        for site in ["https://a.example", "https://b.example", "https://c.example"] {
            a.new_tab(site.into());
        }
        // Two groups: tab 1 in the first, tabs 2 and 3 in the second.
        let _ = a.browser_command(Command::Group(1));
        let _ = a.browser_command(Command::Group(2));
        let _ = a.browser_command(Command::AssignGroup(3, 1));
        let keys: Vec<u64> = a.browser.saved.tabs.iter().map(|t| t.key).collect();
        assert_eq!(a.browser.saved.groups.len(), 2);

        // Ungrouping the first keeps its tab, and the second group moves
        // down to its place, with its tabs and its strip entry.
        let _ = a.browser_command(Command::RemoveGroup(0));
        assert_eq!(a.browser.saved.groups.len(), 1);
        assert_eq!(a.browser.saved.tabs.len(), 4);
        assert_eq!(a.browser.saved.tabs[1].group, None);
        assert_eq!(a.browser.saved.tabs[2].group, Some(0));
        assert_eq!(a.browser.saved.tabs[3].group, Some(0));
        assert!(a.browser.saved.tab_order.contains(&TabStripEntry::Tab(keys[1])));
        assert!(a.browser.saved.tab_order.contains(&TabStripEntry::Group(0)));
        assert!(!a.browser.saved.tab_order.contains(&TabStripEntry::Group(1)));

        // Closing the remaining group closes its two tabs and ends it.
        let _ = a.browser_command(Command::CloseGroup(0));
        assert!(a.browser.saved.groups.is_empty());
        // A tab's key follows its document; its address names it here.
        let left: Vec<&str> = a.browser.saved.tabs.iter().map(|t| t.url.as_str()).collect();
        assert_eq!(left.len(), 2);
        assert_eq!(left[1], "https://a.example");
        assert!(a.browser.saved.tabs.iter().all(|t| t.group.is_none()));
        assert_eq!(command_icon(&Command::NewGroup), "folder-plus");
    }
    #[test]
    fn browser_tests_pdf_results_follow_the_originating_tab_and_ignore_closed_tabs() {
        let mut a = app();
        let key = a.browser.saved.tabs[0].key;
        a.new_tab("https://example.com".into());
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/two-pages.pdf");
        let page = crate::pdf::render(path, 1).expect("render PDF fixture");
        let _ = a.update(c(Command::PdfReady(key, Ok(page.clone()))));
        assert!(a.browser.pdf.is_none());
        assert!(a.browser.sleeping[0].as_ref().unwrap().pdf.is_some());
        a.select_tab(0);
        assert_eq!(a.browser.pdf.as_ref().unwrap().number, 1);
        let _ = a.update(c(Command::Close(0)));
        let _ = a.update(c(Command::PdfReady(key, Ok(page))));
        assert!(a.browser.pdf.is_none());
        assert_eq!(a.browser.saved.tabs.len(), 1);
        let old_key = a.browser.saved.tabs[0].key;
        let page = crate::pdf::render(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/two-pages.pdf"),
            1,
        )
        .unwrap();
        a.submit_browser_action(BrowserAction::Navigate("https://example.org".into()));
        let _ = a.update(c(Command::PdfReady(old_key, Ok(page))));
        assert!(
            a.browser.pdf.is_none(),
            "a late PDF must not cover a later navigation"
        );
    }
    #[test]
    fn browser_tests_groups_bookmarks_and_layout_route_through_update() {
        let mut a = app();
        a.new_tab("https://example.com".into());
        let _ = a.update(c(Command::Group(1)));
        let _ = a.update(c(Command::GroupName("Research".into())));
        let _ = a.update(c(Command::GroupColor(2)));
        assert_eq!(a.browser.saved.groups[0].name, "Research");
        assert_eq!(a.browser.saved.groups[0].color, 2);
        assert_eq!(a.browser.saved.tabs[1].group, Some(0));
        let _ = a.update(c(Command::Bookmark));
        assert_eq!(a.browser.saved.bookmarks.len(), 1);
        let _ = a.update(c(Command::Bookmark));
        assert!(a.browser.saved.bookmarks.is_empty());
        let _ = a.update(c(Command::Layout(Layout::Icons)));
        let _ = a.update(c(Command::Right(true)));
        assert_eq!(a.browser.saved.layout, Layout::Icons);
        assert!(a.browser.saved.right);
    }
    #[test]
    fn browser_tests_stop_cancels_navigation_before_native_open() {
        let mut app = App::new(enzui_theme::EnzTheme::dark());
        app.browser_mode = true;
        app.submit_browser_action(BrowserAction::Navigate("https://example.org/".into()));
        assert!(app.pending_native_open);
        assert!(app.pending_navigation.is_some());
        drop(app.update(c(Command::Stop)));
        assert!(!app.pending_native_open);
        assert!(app.pending_navigation.is_none());
        assert!(!app.webview.borrow().loading);
    }
    #[test]
    fn browser_tests_background_history_is_recorded_without_activating_the_tab() {
        let mut app = App::new(enzui_theme::EnzTheme::dark());
        app.browser_mode = true;
        app.browser.saved.tabs.truncate(1);
        app.browser.saved.active = 0;
        app.browser.sleeping = vec![None];
        let mut tab = app.browser.saved.tabs[0].clone();
        tab.key = next_tab_key();
        tab.url = "https://example.org/background".into();
        tab.title = "Background page".into();
        app.browser.saved.tabs.push(tab.clone());
        let mut host = WebViewService::new();
        host.location = Some(tab.url.clone());
        host.dom_report = Some(tab.title.clone());
        host.page_events.push(enzui_webkit::page::PageEvent::Load(
            enzui_webkit::page::LoadStage::Finished,
        ));
        app.browser.sleeping.push(Some(SleepingTab {
            host: RefCell::new(host),
            pdf: None,
        }));
        app.sync_browser();
        app.sync_browser();
        assert_eq!(app.browser.saved.active, 0);
        let visits = app.browser.history_store.query("background", 0).unwrap();
        assert_eq!(visits.len(), 1);
        assert_eq!(visits[0].url, tab.url);
        assert!(visits[0].visited_at > 0);
        let original_id = visits[0].id;
        app.browser.sleeping[1]
            .as_ref()
            .unwrap()
            .host
            .borrow_mut()
            .dom_report = Some("Settled background title".into());
        app.sync_browser();
        let settled = app.browser.history_store.query("settled", 0).unwrap();
        assert_eq!(settled.len(), 1);
        assert_eq!(settled[0].id, original_id);
        assert_eq!(settled[0].url, tab.url);
    }
    #[test]
    fn browser_tests_delayed_title_updates_only_its_exact_recorded_visit() {
        let mut app = App::new(enzui_theme::EnzTheme::dark());
        app.browser_mode = true;
        app.browser.saved.tabs.truncate(1);
        app.browser.saved.active = 0;
        app.browser.sleeping = vec![None];
        let url = "https://example.org/title-order";
        app.browser.saved.tabs[0].url = url.into();
        app.browser.saved.tabs[0].title = "Previous document title".into();
        {
            let host = &mut *app.webview.borrow_mut();
            host.location = Some(url.into());
            host.dom_report = Some("Previous document title".into());
            host.page_events.push(enzui_webkit::page::PageEvent::Load(
                enzui_webkit::page::LoadStage::Finished,
            ));
        }
        app.sync_browser();
        let old = app.browser.history_store.query("title-order", 0).unwrap();
        assert_eq!(old.len(), 1);
        app.browser
            .history_store
            .record(&crate::history::Visit {
                url: url.into(),
                title: "Other container's title".into(),
                container: Some(98),
                visited_at: old[0].visited_at + 1,
                id: 0,
            })
            .unwrap();
        app.webview.borrow_mut().dom_report = Some("Current document title".into());
        app.sync_browser();
        let updated = app
            .browser
            .history_store
            .query("Current document title", 0)
            .unwrap();
        assert_eq!(updated.len(), 1);
        assert_eq!(updated[0].id, old[0].id);
        assert_eq!(updated[0].visited_at, old[0].visited_at);
        assert_eq!(
            app.browser
                .history_store
                .query("Other container's title", 0)
                .unwrap()
                .len(),
            1
        );
        assert!(
            app.browser
                .history_store
                .query("Previous document title", 0)
                .unwrap()
                .is_empty()
        );
        {
            let host = &mut *app.webview.borrow_mut();
            host.location = Some("https://example.org/replacement".into());
            host.dom_report = Some("Replacement title".into());
        }
        app.sync_browser();
        assert_eq!(
            app.browser
                .history_store
                .query("Current document title", 0)
                .unwrap()
                .len(),
            1
        );
        app.browser.history_store.delete(old[0].id).unwrap();
        {
            let host = &mut *app.webview.borrow_mut();
            host.location = Some(url.into());
            host.dom_report = Some("Title after history deletion".into());
        }
        app.sync_browser();
        assert_eq!(
            app.browser
                .history_store
                .query("title-order", 0)
                .unwrap()
                .len(),
            1
        );
        assert!(
            app.browser
                .history_store
                .query("Title after history deletion", 0)
                .unwrap()
                .is_empty()
        );
    }
}

#[cfg(test)]
mod profile_isolation_checks {
    use super::*;
    #[test]
    fn default_unit_save_leaves_existing_profile_bytes_untouched() {
        use std::io::Write;
        let path = state_path();
        let parent = path.parent().expect("unit profile parent");
        enzui_security::secure_dir::ensure_private_dir(parent).expect("private unit profile");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .expect("exclusive sentinel");
        file.write_all(b"Existing profile must remain unchanged")
            .expect("sentinel bytes");
        drop(file);
        let mut app = browser_tests::app();
        app.browser.private = false;
        app.browser.history_ready = true;
        app.browser.saved.tabs[0].url = "https://fixture.example/".into();
        app.browser.save();
        assert_eq!(
            std::fs::read(&path).expect("existing profile"),
            b"Existing profile must remain unchanged"
        );
        std::fs::remove_file(&path).expect("owned sentinel cleanup");
        let _ = std::fs::remove_dir(parent);
    }
}

impl crate::App {
    /// How readily hidden tabs give back memory, and what each choice means.
    fn memory_settings(&self) -> Element<'_, Message> {
        use crate::webview_native::MemoryMode;
        use enzui_widgets::inline_view_switcher::{InlineSwitcherPage as Page, enz_inline_view_switcher};
        let mode = self.browser.saved.memory_mode;
        let choice = enz_inline_view_switcher(
            MemoryMode::ALL.iter().map(|m| Page::new(m.label().to_lowercase(), m.label())).collect(),
            MemoryMode::ALL.iter().position(|m| *m == mode).unwrap_or(0),
            |i| c(Command::MemoryMode(MemoryMode::ALL.get(i).copied().unwrap_or_default())),
        )
        .tab_id_prefix("settings-memory")
        .into_element();
        enzui_widgets::preferences_row::enz_preferences_row("Background tabs")
            .icon("memory-stick")
            .subtitle(match mode {
                MemoryMode::Aggressive => "Least memory: a tab in the background gives back its caches after 15 seconds, and repaints when you return.",
                MemoryMode::Balanced => "The 5 tabs you used last stay ready for a minute; other background tabs give back their caches after 15 seconds.",
                MemoryMode::Relaxed => "Most responsive: the 10 tabs you used last stay ready for 5 minutes, the others for a minute. When memory runs low, tabs are unloaded after 30 minutes instead of 10.",
            })
            .control(choice)
            .accessory_width(320.0)
            .label_min_width(130.0)
            .into_element()
    }
}

fn settings_divider<'a>() -> Element<'a, Message> {
    iced::widget::container(iced::widget::rule::horizontal(1))
        .padding([10, 12])
        .into()
}

#[cfg(test)]
mod password_review_menu_tests {
    use super::*;
    #[test]
    fn password_review_sidebar_context_groups_containers_and_tab_locations() {
        use enzui_widgets::context_menu_host::ContextMenuItem as I;
        let containers = crate::profiles::defaults();
        let items = chrome_context_items(&containers, true);
        let children = items
            .iter()
            .find_map(|item| match item {
                I::Submenu { label, items, .. } if label == "Containers" => Some(items),
                _ => None,
            })
            .expect("container choices");
        assert_eq!(children.len(), containers.len() + 2);
        assert!(matches!(children[children.len() - 2], I::Separator));
        assert!(
            matches!(children.last(), Some(I::Command { command: Command::Container(ContainerCommand::Open), label, .. }) if label == "Manage containers")
        );
        for (item, entry) in children.iter().zip(&containers) {
            assert!(
                matches!(item, I::Command { command: Command::Container(ContainerCommand::NewTab(Some(id))), .. } if *id == entry.id)
            );
        }
        let locations = items
            .iter()
            .find_map(|item| match item {
                I::Submenu { label, items, .. } if label == "Tabs location" => Some(items),
                _ => None,
            })
            .expect("tab locations");
        assert_eq!(locations.len(), 3);
        assert!(!items.iter().any(|item| matches!(
            item,
            I::Command {
                command: Command::Layout(_),
                ..
            }
        )));
    }
    #[test]
    fn password_review_containers_submenu_opens_isolated_tabs_and_management() {
        use enzui_core::menu_model::{EntryId, EntryKind, OpenCause};
        use enzui_widgets::menu_button::MenuButtonEvent as E;
        let mut app = browser_tests::app();
        let id = app.browser.saved.containers[0].id;
        let _ = app.browser_command(Command::Menu(E::Open(OpenCause::Pointer)));
        let parent = EntryId::from("containers");
        assert_eq!(
            app.browser
                .menu_model
                .entry(&parent)
                .expect("containers")
                .kind(),
            EntryKind::Submenu
        );
        let child = app
            .browser
            .menu_model
            .submenu(&parent)
            .expect("container submenu");
        assert_eq!(
            child.entries().len(),
            app.browser.saved.containers.len() + 2
        );
        assert_eq!(
            child.entries()[child.entries().len() - 2].kind(),
            EntryKind::Separator
        );
        assert_eq!(
            child.entries().last().expect("manage").label(),
            "Manage containers"
        );
        let before = app.browser.saved.tabs.len();
        let _ = app.browser_command(Command::Menu(E::SubmenuEntered(parent.clone())));
        let _ = app.browser_command(Command::Menu(E::Activated {
            id: EntryId::from(format!("container-new:{id}")),
            check: enzui_core::menu_model::CheckState::None,
        }));
        assert_eq!(app.browser.saved.tabs.len(), before + 1);
        assert_eq!(
            app.browser.saved.tabs[app.browser.saved.active].container,
            Some(id)
        );
        let _ = app.browser_command(Command::Menu(E::Open(OpenCause::Pointer)));
        let _ = app.browser_command(Command::Menu(E::SubmenuEntered(parent)));
        let _ = app.browser_command(Command::Menu(E::Activated {
            id: EntryId::from("containers-manage"),
            check: enzui_core::menu_model::CheckState::None,
        }));
        assert_eq!(app.browser.library, Some(InternalPage::Containers));
    }
}

#[cfg(test)]
mod popup_refresh_review {
    use super::*;
    #[test]
    fn group_editor_close_has_framework_insets_and_dismisses_settings() {
        for (theme_name, theme) in [
            ("dark", enzui_theme::EnzTheme::dark()),
            ("light", enzui_theme::EnzTheme::light()),
        ] {
            for size in [iced::Size::new(400., 240.), iced::Size::new(396., 220.)] {
                let mut app = browser_tests::app();
                app.theme = enzui_core::theme_file::integration::ThemeIntegration::unmanaged_compiled_theme(theme.clone()).0;
                app.browser.private = true;
                app.browser.viewport = size;
                app.browser.saved.groups.push(Group {
                    name: "Reading".into(),
                    color: 0,
                });
                app.browser.panel = Panel::Group(0);
                let mut ui =
                    iced_test::Simulator::with_size(Default::default(), size, app.group_editor(0));
                let surface = ui
                    .find(iced::widget::Id::new("group-editor"))
                    .expect("group editor")
                    .bounds();
                let icon = ui
                    .find(iced::widget::Id::new("group-editor-icon"))
                    .expect("header icon")
                    .bounds();
                let close = ui
                    .find(iced::widget::Id::new("group-editor-close"))
                    .expect("framework close button")
                    .bounds();
                assert!(
                    (icon.x - surface.x - 16.).abs() < 0.1,
                    "header icon has a full side inset"
                );
                assert_eq!(
                    close.width,
                    enzui_widgets::modal_surface::CLOSE_CONTROL_SIZE
                );
                assert!(
                    (surface.x + surface.width - close.x - close.width - 8.).abs() < 0.1,
                    "framework close keeps its edge inset"
                );
                assert!(
                    (icon.center_y() - close.center_y()).abs() < 1.,
                    "header icon and close align"
                );
                assert!(surface.height <= size.height, "settings fit at small sizes");
                if let Some(output) = std::env::var_os("ENZIM_VISUAL_OUTPUT") {
                    let path = std::path::PathBuf::from(output).join(format!(
                        "group-close-{theme_name}-{}x{}",
                        size.width as u32, size.height as u32
                    ));
                    assert!(
                        ui.snapshot(&app.theme.iced_theme())
                            .expect("group editor render")
                            .matches_image(path)
                            .expect("PNG capture")
                    );
                }
                ui.click(iced::widget::Id::new("group-editor-close"))
                    .expect("close is clickable");
                let messages = ui.into_messages().collect::<Vec<_>>();
                assert!(
                    messages
                        .iter()
                        .any(|message| matches!(message, Message::Chrome(Command::Dismiss))),
                    "close requests dismissal"
                );
                for message in messages {
                    let _ = app.update(message);
                }
                assert_eq!(app.browser.panel, Panel::None);
            }
        }
    }
    #[test]
    fn popup_placement_new_tab_keeps_its_top_when_suggestions_change() {
        for (theme_name, theme) in [
            ("dark", enzui_theme::EnzTheme::dark()),
            ("light", enzui_theme::EnzTheme::light()),
        ] {
            for size in [iced::Size::new(1280., 800.), iced::Size::new(420., 360.)] {
                let mut app = browser_tests::app();
                app.theme = enzui_core::theme_file::integration::ThemeIntegration::unmanaged_compiled_theme(theme.clone()).0;
                app.browser.private = true;
                app.browser.viewport = size;
                app.browser.panel = Panel::New;
                app.browser.saved.bookmarks = (0..6)
                    .map(|i| TabInfo {
                        key: 100 + i,
                        url: format!("https://site{i}.example.test/"),
                        title: format!("Saved site {i}"),
                        pinned: false,
                        group: None,
                        container: None,
                        favicon: None,
                    })
                    .collect();
                let mut bounds = Vec::new();
                for (state, query) in [("many", ""), ("few", "unmatched-popup-geometry-zzzz")] {
                    app.browser.query = query.into();
                    let mut ui = iced_test::Simulator::with_size(
                        Default::default(),
                        size,
                        app.chrome_view(),
                    );
                    let dialog = ui
                        .find(iced::widget::Id::new("new-tab-dialog"))
                        .expect("new tab dialog")
                        .bounds();
                    assert!(
                        (dialog.y - size.height * 0.2).abs() < 1.,
                        "new tab starts above the window center: {dialog:?}"
                    );
                    assert!(
                        dialog.y + dialog.height <= size.height - 15.,
                        "results stay inside the window"
                    );
                    bounds.push(dialog);
                    if let Some(output) = std::env::var_os("ENZIM_VISUAL_OUTPUT") {
                        let path = std::path::PathBuf::from(output).join(format!(
                            "new-tab-{theme_name}-{state}-{}x{}",
                            size.width as u32, size.height as u32
                        ));
                        assert!(
                            ui.snapshot(&app.theme.iced_theme())
                                .expect("new tab render")
                                .matches_image(path)
                                .expect("PNG capture")
                        );
                    }
                }
                assert!(
                    (bounds[0].y - bounds[1].y).abs() < 0.1,
                    "typing must not move the popup top"
                );
                assert!(
                    bounds[0].height > bounds[1].height + 20.,
                    "fixture must exercise a changing results height: {bounds:?}"
                );
            }
        }
    }
    #[test]
    #[ignore = "rendered popup review; set ENZIM_VISUAL_OUTPUT to a disposable capture directory"]
    fn popup_refresh_snapshots_in_both_themes_and_small_windows() {
        let output = std::path::PathBuf::from(
            std::env::var_os("ENZIM_VISUAL_OUTPUT").expect("capture directory"),
        );
        for (theme_name, theme) in [
            ("dark", enzui_theme::EnzTheme::dark()),
            ("light", enzui_theme::EnzTheme::light()),
        ] {
            let mut app = browser_tests::app();
            app.theme =
                enzui_core::theme_file::integration::ThemeIntegration::unmanaged_compiled_theme(
                    theme,
                )
                .0;
            app.browser.private = false;
            app.browser.saved.layout = Layout::Top;
            app.browser.saved.tabs[0].url = "https://example.test/login".into();
            app.address = "https://example.test/login".into();
            app.webview.borrow_mut().loading = false;
            app.webview.borrow_mut().tls = Some("Subject: example.test\nIssuer: Example Test Authority\nWebKit reported no certificate errors".into());
            app.browser.dialog = Some(enzui_webkit::page::ScriptDialogRequest {
                id: enzui_webkit::page::PageRequestId::new(10),
                kind: enzui_webkit::page::ScriptDialogKind::Confirm,
                message: "Continue with the page's requested action?".into(),
            });
            app.browser.saved.groups.push(Group {
                name: "Reading".into(),
                color: 0,
            });
            app.browser.history_matches = vec![
                crate::history::Visit {
                    id: 1,
                    url: "https://example.test/".into(),
                    title: "Example account".into(),
                    ..Default::default()
                },
                crate::history::Visit {
                    id: 2,
                    url: "https://docs.example.test/".into(),
                    title: "Documentation".into(),
                    ..Default::default()
                },
            ];
            app.browser.history_total = 2;
            for view in [
                "security",
                "privacy",
                "permissions",
                "request",
                "downloads",
                "save",
                "script",
                "search",
                "find",
                "group",
                "saved-permissions",
                "history",
            ] {
                app.browser.permission = if view == "request" {
                    Some(enzui_webkit::site_permissions::SitePermissionRequest {
                        id: enzui_webkit::page::PageRequestId::new(20),
                        permission: enzui_webkit::site_permissions::SitePermission {
                            requesting_origin:
                                enzui_webkit::site_permissions::PermissionOrigin::parse(
                                    "https://embedded.example.test",
                                )
                                .expect("request origin"),
                            top_origin: enzui_webkit::site_permissions::PermissionOrigin::parse(
                                "https://example.test",
                            )
                            .expect("top origin"),
                            kind: enzui_webkit::web::policy::PermissionKind::Camera,
                        },
                    })
                } else {
                    None
                };
                for (width, height) in [(400., 560.), (396., 280.)] {
                    app.browser.viewport = iced::Size::new(width + 24., height + 80.);
                    let body = match view {
                        "security" => app.panel_body(Panel::Security),
                        "privacy" => app.panel_body(Panel::Shields),
                        "permissions" | "request" => app.panel_body(Panel::Permissions),
                        "downloads" => app.panel_body(Panel::Downloads),
                        "save" => app.panel_body(Panel::Export),
                        "script" => app.panel_body(Panel::Script),
                        "search" => app.search_engine_dialog(),
                        "find" => app.find_bar(),
                        "group" => app.group_editor(0),
                        "saved-permissions" => app.permission_settings_dialog(),
                        "history" => app.history_page(),
                        _ => unreachable!(),
                    };
                    let mut ui = iced_test::Simulator::with_size(
                        Default::default(),
                        iced::Size::new(width, height),
                        body,
                    );
                    let image = ui
                        .snapshot(&app.theme.iced_theme())
                        .expect("rendered popup");
                    assert!(
                        image
                            .matches_image(output.join(format!(
                                "popup-{view}-{theme_name}-{}x{}",
                                width as u32, height as u32
                            )))
                            .expect("PNG capture")
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tab_review_context_tests {
    use super::*;
    #[test]
    fn tab_review_context_has_conditional_group_actions_container_submenu_and_no_empty_sections() {
        use enzui_widgets::context_menu_host::ContextMenuItem as I;
        let mut app = browser_tests::app();
        for grouped in [false, true] {
            app.browser.saved.groups = vec![Group {
                name: "Reading".into(),
                color: 0,
            }];
            app.browser.saved.tabs[0].group = grouped.then_some(0);
            let items = tab_context_items(&app.browser, 0, true);
            assert_eq!(
                items.iter().any(|item| matches!(
                    item,
                    I::Command {
                        command: Command::Ungroup(_),
                        ..
                    }
                )),
                grouped
            );
            assert!(
                !items
                    .windows(2)
                    .any(|pair| matches!(pair, [I::Separator, I::Separator]))
            );
            assert!(
                matches!(items.first(), Some(I::Command { .. }))
                    && matches!(items.last(), Some(I::Command { .. }))
            );
            let children = items
                .iter()
                .find_map(|item| match item {
                    I::Submenu { label, items, .. } if label == "Open in container" => Some(items),
                    _ => None,
                })
                .expect("container submenu");
            assert_eq!(children.len(), app.browser.saved.containers.len() + 1);
            assert!(matches!(
                &children[0],
                I::Command {
                    command: Command::Container(ContainerCommand::Reopen(_, None)),
                    ..
                }
            ));
        }
        app.browser.saved.tabs[0].group = None;
        let before = app.browser.saved.tabs.len();
        let _ = app.browser_command(Command::NewGroup);
        assert_eq!(app.browser.saved.tabs.len(), before);
        assert!(app.browser.saved.tabs.iter().all(|tab| tab.group.is_none()));
        assert_eq!(app.browser.panel, Panel::Group(1));
        assert!(chrome_context_items(&[], true).iter().any(|item| matches!(
            item,
            I::Command {
                command: Command::NewGroup,
                ..
            }
        )));
    }
}
