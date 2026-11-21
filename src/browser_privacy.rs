use super::*;
use iced::widget::{column, text};
#[derive(Clone, Debug)]
pub(crate) enum PrivacyCommand {
    Toggle(&'static str),
    Site(&'static str),
    ResetSite,
    Identity(crate::user_agent::Change),
}
impl App {
    pub(super) fn privacy_command(&mut self, command: PrivacyCommand) -> Task<Message> {
        let focus_custom = matches!(
            command,
            PrivacyCommand::Identity(crate::user_agent::Change::Browser(3))
        );
        let address = self.browser.saved.tabs[self.browser.saved.active]
            .url
            .clone();
        let reload = matches!(
            command,
            PrivacyCommand::Site(_)
                | PrivacyCommand::ResetSite
                | PrivacyCommand::Toggle("ads" | "trackers" | "javascript" | "drm")
        );
        let container = self.browser.saved.tabs[self.browser.saved.active].container;
        let current = self.effective_privacy(self.browser.saved.active);
        match command {
            PrivacyCommand::Site(key) => {
                if key == "webrtc" && self.container_uses_proxy(container) {
                    return Task::none();
                }
                if let Some(entry) = self
                    .browser
                    .saved
                    .containers
                    .iter_mut()
                    .find(|entry| Some(entry.id) == container)
                {
                    if let Some(origin) = crate::privacy::Privacy::origin(&address) {
                        entry
                            .privacy
                            .sites
                            .entry(origin)
                            .or_default()
                            .insert(key.into(), !current.value_for(key, &address));
                    }
                } else {
                    self.browser.saved.privacy.toggle_site(key, &address);
                }
            }
            PrivacyCommand::ResetSite => {
                if let Some(origin) = crate::privacy::Privacy::origin(&address) {
                    if let Some(entry) = self
                        .browser
                        .saved
                        .containers
                        .iter_mut()
                        .find(|entry| Some(entry.id) == container)
                    {
                        entry.privacy.sites.remove(&origin);
                    } else {
                        self.browser.saved.privacy.site_settings.remove(&origin);
                        self.browser
                            .saved
                            .privacy
                            .allowed_sites
                            .retain(|o| o != &origin);
                    }
                }
            }
            PrivacyCommand::Toggle(key) => {
                let policy = &mut self.browser.saved.privacy;
                policy.set_control(key, !policy.value(key));
            }
            PrivacyCommand::Identity(change) => {
                let policy = &mut self.browser.saved.privacy;
                (policy.user_agent, policy.custom_agent) =
                    crate::user_agent::apply(&policy.user_agent, change);
            }
        }
        self.apply_privacy_to_hosts();
        self.browser.save();
        if reload && InternalPage::from_url(&address).is_none() {
            self.submit_browser_action(BrowserAction::Reload);
        }
        if focus_custom {
            focus("settings-user-agent-custom")
        } else {
            Task::none()
        }
    }
    pub(crate) fn effective_privacy(&self, index: usize) -> crate::privacy::Privacy {
        let container = self.browser.saved.tabs[index].container;
        let mut policy = self
            .browser
            .saved
            .containers
            .iter()
            .find(|entry| Some(entry.id) == container)
            .map_or_else(
                || self.browser.saved.privacy.clone(),
                |entry| entry.privacy.apply(&self.browser.saved.privacy),
            );
        if self.container_uses_proxy(container) {
            // Real-time sockets are not qualified through the profile's HTTP CONNECT guard.
            policy.webrtc = false;
            for choices in policy.site_settings.values_mut() {
                choices.remove("webrtc");
            }
        }
        policy
    }
    pub(crate) fn container_uses_proxy(&self, container: Option<u64>) -> bool {
        container
            .and_then(|id| self.browser.container_routes.get(&id))
            .is_some_and(|route| !route.is_empty())
    }
    pub(super) fn apply_privacy_to_hosts(&self) {
        for index in 0..self.browser.saved.tabs.len() {
            let host = if index == self.browser.saved.active {
                Some(&self.webview)
            } else {
                self.browser.sleeping[index].as_ref().map(|tab| &tab.host)
            };
            let Some(host) = host else { continue };
            let service = host.borrow();
            if let (Some(native), Some(view)) = (&service.native, service.live_view) {
                let _ = native.browse(
                    &service.router,
                    view,
                    BrowserAction::Privacy(Box::new(self.effective_privacy(index))),
                );
            }
        }
    }
    pub(super) fn privacy_settings(&self) -> Element<'_, Message> {
        let p = &self.browser.saved.privacy;
        let mut body = column![].spacing(4);
        for (title, symbol, detail, key, value) in [
            (
                "Block ads",
                "megaphone",
                "Allow advertisements without changing tracker or cookie protection.",
                "ads",
                p.value("ads"),
            ),
            (
                "Block trackers",
                "scan-eye",
                "Block known tracking requests independently of advertisements.",
                "trackers",
                p.value("trackers"),
            ),
            (
                "Tracking prevention",
                "shield-check",
                "Use WebKit’s tracking prevention for cross-site storage.",
                "tracking",
                p.tracking_prevention,
            ),
            (
                "Allow third-party cookies",
                "cookie",
                "Cross-site cookies are blocked by default.",
                "cookies",
                p.third_party_cookies,
            ),
            (
                "Clean tracking parameters",
                "link",
                "Remove common advertising IDs from entered and copied URLs.",
                "links",
                p.clean_links,
            ),
            (
                "JavaScript",
                "code",
                "Disabling scripts can break interactive websites.",
                "javascript",
                p.javascript,
            ),
            (
                "WebGL",
                "box",
                "Allow accelerated 3D graphics.",
                "webgl",
                p.webgl,
            ),
            (
                "Camera, microphone and WebRTC",
                "video",
                "Websites must request permission before accessing your devices.",
                "webrtc",
                p.webrtc,
            ),
            (
                "Encrypted media (ClearKey)",
                "key-round",
                "Allow sites to request encrypted playback. Widevine and PlayReady are unavailable.",
                "drm",
                p.encrypted_media,
            ),
            (
                "Autoplay with sound",
                "volume-2",
                "Otherwise media must wait for your interaction.",
                "autoplay",
                p.autoplay,
            ),
        ] {
            if key == "javascript" {
                body = body.push(settings_divider());
            }
            body = body.push(
                enzui_widgets::preferences_row::enz_preferences_row(title)
                    .icon(symbol)
                    .subtitle(detail)
                    .switch(value, move |_| {
                        c(Command::Privacy(PrivacyCommand::Toggle(key)))
                    })
                    .enabled(key != "drm" || crate::privacy::encrypted_media_available())
                    .into_element(),
            );
        }
        if !crate::privacy::encrypted_media_available() {
            body = body.push(text("Encrypted media is unavailable in this runtime. Widevine and PlayReady are not supported.").size(12).color(self.look().palette.text_muted));
        }
        body = body.push(settings_divider());
        body = body.push(crate::user_agent::picker(
            &p.user_agent,
            p.custom_agent,
            false,
            "settings-user-agent",
            |change| c(Command::Privacy(PrivacyCommand::Identity(change))),
        ));
        body=body.push(text("User-agent changes do not change the rendering engine or hide your browser fingerprint.").size(12));
        section("Privacy and website compatibility", body)
    }
}

pub(super) const CONTROLS: [(&str, &str, &str); 8] = [
    ("ads", "Block ads", "megaphone"),
    ("trackers", "Block trackers", "scan-eye"),
    ("javascript", "JavaScript", "code"),
    ("webgl", "WebGL", "box"),
    ("webrtc", "Camera, microphone and WebRTC", "video"),
    ("autoplay", "Autoplay with sound", "volume-2"),
    ("drm", "Encrypted media (ClearKey)", "key-round"),
    ("links", "Clean tracking parameters", "link"),
];

pub(super) fn control_help(key: &str) -> &'static str {
    match key {
        "ads" => "Hide advertisements on this site.",
        "trackers" => "Block requests that track your activity.",
        "javascript" => "Allow interactive scripts and page features.",
        "webgl" => "Allow accelerated 3D graphics.",
        "webrtc" => "Allow device requests and real-time calls.",
        "autoplay" => "Let media start playing with sound.",
        "drm" => "Allow ClearKey playback requests; licensed CDMs are unavailable.",
        _ => "Remove known tracking IDs from links.",
    }
}

#[cfg(test)]
mod container_tests {
    use super::*;
    #[test]
    fn proxy_container_disables_realtime_even_with_an_allow_site_override() {
        let mut app = super::super::browser_tests::app();
        let id = app.browser.saved.containers[0].id;
        app.browser.saved.tabs[0].container = Some(id);
        app.browser.saved.tabs[0].url = "https://example.org/".into();
        app.browser.saved.containers[0]
            .privacy
            .sites
            .entry("https://example.org".into())
            .or_default()
            .insert("webrtc".into(), true);
        app.browser
            .container_routes
            .insert(id, "127.0.0.1:8080".into());
        assert!(
            !app.effective_privacy(0)
                .for_site("https://example.org/")
                .webrtc
        );
        let _ = app.privacy_command(PrivacyCommand::Site("webrtc"));
        assert!(
            !app.effective_privacy(0)
                .for_site("https://example.org/")
                .webrtc
        );
        app.browser.saved.containers[0]
            .privacy
            .upstream_proxy
            .clear();
        assert!(
            !app.effective_privacy(0).webrtc,
            "a pending direct route cannot enable realtime before restart"
        );
        app.browser.container_routes.insert(id, String::new());
        assert!(
            app.effective_privacy(0)
                .for_site("https://example.org/")
                .webrtc
        );
        app.browser.saved.tabs[0].container = None;
        assert!(app.effective_privacy(0).webrtc);
    }
    #[test]
    fn container_custom_agent_selection_retains_the_preset_as_editable_text() {
        let mut app = super::super::browser_tests::app();
        let id = app.browser.saved.containers[0].id;
        let _ = app.container_command(ContainerCommand::Edit(Some(id)));
        let _ = app.container_command(ContainerCommand::Agent(1));
        assert!(!app.browser.container_agent_custom);
        let preset = app.browser.container_policy.user_agent.clone();
        let _ = app.container_command(ContainerCommand::Agent(3));
        assert!(app.browser.container_agent_custom);
        assert_eq!(app.browser.container_policy.user_agent, preset);
        let _ = app.container_command(ContainerCommand::AgentText("Custom client".into()));
        let _ = app.container_command(ContainerCommand::Save);
        let _ = app.container_command(ContainerCommand::Edit(Some(id)));
        assert!(app.browser.container_agent_custom);
        assert_eq!(
            app.browser.container_policy.user_agent.as_deref(),
            Some("Custom client")
        );
        let _ = app.container_command(ContainerCommand::Agent(0));
        assert!(!app.browser.container_agent_custom);
        assert!(app.browser.container_policy.user_agent.is_none());
    }
    #[test]
    fn container_policy_editor_and_site_choices_do_not_change_other_profiles() {
        let mut app = super::super::browser_tests::app();
        let id = app.browser.saved.containers[0].id;
        let other = app.browser.saved.containers[1].id;
        let storage = app.browser.saved.containers[0].storage_name();
        let count = app.browser.saved.tabs.len();
        let _ = app.container_command(ContainerCommand::Edit(Some(id)));
        assert_eq!(app.browser.library, Some(InternalPage::Containers));
        assert!(app.browser.container_editor_open);
        assert_eq!(app.browser.saved.tabs.len(), count);
        let _ = app.container_command(ContainerCommand::Override("javascript", Some(false)));
        let _ = app.container_command(ContainerCommand::Agent(1));
        let _ = app.container_command(ContainerCommand::Save);
        assert!(!app.browser.container_editor_open);
        assert_eq!(app.browser.saved.containers[0].storage_name(), storage);
        assert!(app.effective_privacy(0).javascript);
        app.browser.saved.tabs[0].container = Some(id);
        app.browser.saved.tabs[0].url = "https://example.org/".into();
        assert!(!app.effective_privacy(0).javascript);
        assert!(app.effective_privacy(0).user_agent.contains("Firefox/"));
        let _ = app.privacy_command(PrivacyCommand::Site("ads"));
        assert!(
            !app.effective_privacy(0)
                .value_for("ads", "https://example.org/")
        );
        assert!(
            app.browser
                .saved
                .privacy
                .value_for("ads", "https://example.org/")
        );
        app.browser.saved.tabs[0].container = Some(other);
        assert!(app.effective_privacy(0).javascript);
        assert!(
            app.effective_privacy(0)
                .value_for("ads", "https://example.org/")
        );
        assert!(!app.effective_privacy(0).user_agent.contains("Firefox/"));
        app.browser.saved.tabs[0].container = Some(id);
        let _ = app.privacy_command(PrivacyCommand::ResetSite);
        assert!(
            app.effective_privacy(0)
                .value_for("ads", "https://example.org/")
        );
        let _ = app.container_command(ContainerCommand::Edit(Some(id)));
        let _ = app.container_command(ContainerCommand::Override("javascript", None));
        let _ = app.container_command(ContainerCommand::Agent(0));
        let _ = app.container_command(ContainerCommand::Save);
        assert!(app.effective_privacy(0).javascript);
        assert!(app.effective_privacy(0).user_agent.is_empty());
    }
}
