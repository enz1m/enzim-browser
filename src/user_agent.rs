use crate::Message;
use iced::widget::{column, row, text};
use iced::{Element, Length};

const FAMILIES: [&str; 6] = [
    "WebKit default",
    "Firefox",
    "Chrome",
    "Custom",
    "Edge",
    "Safari",
];
const PLATFORMS: [&str; 5] = ["Linux", "Windows", "macOS", "Android", "iOS"];

pub(crate) fn site_compatibility_default(address: &str, native: &str) -> Option<String> {
    let url = url::Url::parse(address).ok()?;
    if url.scheme() != "https"
        || url.port_or_known_default() != Some(443)
        || !matches!(
            url.host_str(),
            Some(
                "youtube.com"
                    | "www.youtube.com"
                    | "m.youtube.com"
                    | "music.youtube.com"
                    | "youtube-nocookie.com"
                    | "www.youtube-nocookie.com"
            )
        )
        || !native.starts_with("Mozilla/5.0 (X11; Linux ")
        || !native.contains("AppleWebKit/")
        || !native.contains("Version/")
        || !native.contains("Safari/")
        || native.contains("Chrome/")
    {
        return None;
    }
    let (_, engine) = native.split_once(')')?;
    // YouTube serves its legacy JavaScript bundle to Safari's Linux platform token.
    Some(format!(
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7){engine}"
    ))
}

#[derive(Clone, Debug)]
pub(crate) enum Change {
    Browser(usize),
    Platform(usize),
    Version(usize),
    Custom(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Identity {
    pub(crate) browser: usize,
    pub(crate) platform: usize,
    pub(crate) version: usize,
}

impl Default for Identity {
    fn default() -> Self {
        Self {
            browser: 2,
            platform: 0,
            version: 0,
        }
    }
}

impl Identity {
    pub(crate) fn versions(self) -> &'static [&'static str] {
        match self.browser {
            1 => &["159.0", "153.0", "144.0"],
            5 => &["26.2", "26.0", "18.6"],
            _ => &["156.0", "155.0", "142.0"],
        }
    }

    pub(crate) fn platforms(self) -> &'static [usize] {
        if self.browser == 5 {
            &[2, 4]
        } else {
            &[0, 1, 2, 3, 4]
        }
    }

    pub(crate) fn normalized(mut self) -> Self {
        if !matches!(self.browser, 1 | 2 | 4 | 5) {
            self.browser = 2;
        }
        if !self.platforms().contains(&self.platform) {
            self.platform = self.platforms()[0];
        }
        self.version = self.version.min(self.versions().len() - 1);
        self
    }

    pub(crate) fn value(self) -> String {
        let this = self.normalized();
        let version = this.versions()[this.version];
        let desktop = match this.platform {
            1 => "Windows NT 10.0; Win64; x64",
            2 if this.browser == 1 => "Macintosh; Intel Mac OS X 10.15",
            2 => "Macintosh; Intel Mac OS X 10_15_7",
            _ => "X11; Linux x86_64",
        };
        if this.browser == 1 && this.platform != 4 {
            let platform = if this.platform == 3 {
                "Android 16; Mobile"
            } else {
                desktop
            };
            return format!(
                "Mozilla/5.0 ({platform}; rv:{version}) Gecko/20100101 Firefox/{version}"
            );
        }
        if this.browser == 5 {
            return if this.platform == 4 {
                format!(
                    "Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/{version} Mobile/15E148 Safari/604.1"
                )
            } else {
                format!(
                    "Mozilla/5.0 ({desktop}) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/{version} Safari/605.1.15"
                )
            };
        }
        if this.platform == 4 {
            let browser = match this.browser {
                1 => "FxiOS",
                4 => "EdgiOS",
                _ => "CriOS",
            };
            return format!(
                "Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) {browser}/{version} Mobile/15E148 Safari/604.1"
            );
        }
        let platform = if this.platform == 3 {
            "Linux; Android 10; K"
        } else {
            desktop
        };
        let mobile = if this.platform == 3 { " Mobile" } else { "" };
        let mut value = format!(
            "Mozilla/5.0 ({platform}) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{version}.0.0{mobile} Safari/537.36"
        );
        if this.browser == 4 {
            let token = if this.platform == 3 { "EdgA" } else { "Edg" };
            value.push(' ');
            value.push_str(token);
            value.push('/');
            value.push_str(version);
            value.push_str(".0.0");
        }
        value
    }

    pub(crate) fn detect(value: &str) -> Option<Self> {
        for browser in [1, 2, 4, 5] {
            let base = Self {
                browser,
                ..Self::default()
            };
            for &platform in base.platforms() {
                for version in 0..base.versions().len() {
                    let identity = Self {
                        browser,
                        platform,
                        version,
                    };
                    if identity.value() == value {
                        return Some(identity);
                    }
                }
            }
        }
        None
    }
}

pub(crate) fn mode(value: &str, custom: bool) -> usize {
    if custom {
        3
    } else if value.is_empty() {
        0
    } else {
        Identity::detect(value).map_or(3, |id| id.browser)
    }
}

pub(crate) fn apply(value: &str, change: Change) -> (String, bool) {
    let mut identity = Identity::detect(value).unwrap_or_default();
    match change {
        Change::Browser(0) => return (String::new(), false),
        Change::Browser(3) => return (value.into(), true),
        Change::Custom(input) => {
            return (
                if valid_custom(&input) {
                    input
                } else {
                    value.into()
                },
                true,
            );
        }
        Change::Browser(browser) => {
            identity.browser = browser;
            identity.version = 0;
        }
        Change::Platform(platform) => identity.platform = platform,
        Change::Version(version) => identity.version = version,
    }
    (identity.value(), false)
}

pub(crate) fn valid_custom(value: &str) -> bool {
    value.len() <= 1024 && !value.chars().any(char::is_control)
}

pub(crate) fn picker<'a>(
    value: &'a str,
    custom: bool,
    inherited: bool,
    prefix: &'static str,
    on_change: fn(Change) -> Message,
) -> Element<'a, Message> {
    let selected = mode(value, custom);
    let mut names: Vec<String> = FAMILIES.iter().map(|value| (*value).into()).collect();
    if inherited {
        names[0] = "Inherit browser".into();
    }
    let family = enzui_widgets::combo_box::enz_combo_box(
        "",
        names.clone(),
        Some(names[selected].clone()),
        move |_| on_change(Change::Browser(0)),
    )
    .id_prefix(format!("{prefix}-browser"))
    .aria_label("User-agent browser")
    .selected_index(Some(selected))
    .on_select_index(move |index| on_change(Change::Browser(index)))
    .width(170)
    .into_element();
    let mut body = column![
        enzui_widgets::preferences_row::enz_preferences_row("User agent")
            .icon("id-card")
            .subtitle(if inherited {
                "A separate website identity for this container."
            } else {
                "How the browser identifies itself to websites."
            })
            .control(family)
            .accessory_width(170.)
            .into_element()
    ]
    .spacing(8);
    if selected == 3 {
        let input = enzui_widgets::text_field::enz_text_field(
            "",
            "Custom user agent",
            value,
            move |value| on_change(Change::Custom(value)),
        )
        .id(iced::advanced::widget::Id::new(if inherited {
            "container-user-agent-custom"
        } else {
            "settings-user-agent-custom"
        }))
        .into_element();
        body = body.push(input);
    } else if selected > 0 {
        let identity = Identity::detect(value).unwrap_or_default().normalized();
        let platforms = identity.platforms();
        let platform_names: Vec<String> = platforms
            .iter()
            .map(|index| PLATFORMS[*index].into())
            .collect();
        let index = platforms
            .iter()
            .position(|platform| *platform == identity.platform)
            .unwrap_or(0);
        let platform = enzui_widgets::combo_box::enz_combo_box(
            "",
            platform_names.clone(),
            Some(platform_names[index].clone()),
            move |_| on_change(Change::Platform(platforms[0])),
        )
        .id_prefix(format!("{prefix}-platform"))
        .aria_label("User-agent operating system")
        .selected_index(Some(index))
        .on_select_index(move |index| {
            on_change(Change::Platform(platforms[index.min(platforms.len() - 1)]))
        })
        .width(Length::Fill)
        .into_element();
        let versions: Vec<String> = identity
            .versions()
            .iter()
            .map(|value| (*value).into())
            .collect();
        let version = enzui_widgets::combo_box::enz_combo_box(
            "",
            versions.clone(),
            Some(versions[identity.version].clone()),
            move |_| on_change(Change::Version(0)),
        )
        .id_prefix(format!("{prefix}-version"))
        .aria_label("User-agent version")
        .selected_index(Some(identity.version))
        .on_select_index(move |index| on_change(Change::Version(index)))
        .width(Length::Fill)
        .into_element();
        body = body.push(
            row![
                column![text("Operating system").size(12), platform]
                    .spacing(4)
                    .width(Length::Fill),
                column![text("Version").size(12), version]
                    .spacing(4)
                    .width(Length::Fill)
            ]
            .spacing(12),
        );
    }
    body.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn youtube_compatibility_preserves_webkit_tokens_and_exact_origin_scope() {
        let native = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/60.5 Safari/605.1.15";
        for host in [
            "youtube.com",
            "www.youtube.com",
            "m.youtube.com",
            "music.youtube.com",
            "youtube-nocookie.com",
            "www.youtube-nocookie.com",
        ] {
            let compatible =
                site_compatibility_default(&format!("https://{host}/watch?v=example"), native)
                    .expect("YouTube desktop compatibility");
            assert_eq!(
                compatible,
                native.replacen("X11; Linux x86_64", "Macintosh; Intel Mac OS X 10_15_7", 1)
            );
            assert!(site_compatibility_default(&format!("https://{host}/"), &compatible).is_none());
        }
        for address in [
            "http://www.youtube.com/",
            "https://www.youtube.com:8443/",
            "https://youtube.com.example.org/",
            "https://example.org/?youtube.com",
            "https://consent.youtube.com/",
            "https://notyoutube.com/",
        ] {
            assert!(
                site_compatibility_default(address, native).is_none(),
                "{address}"
            );
        }
        assert!(site_compatibility_default("https://www.youtube.com/", "explicit agent").is_none());
        assert!(
            site_compatibility_default("https://www.youtube.com/", &Identity::default().value())
                .is_none()
        );
    }

    #[test]
    fn user_agent_presets_roundtrip_valid_platforms_and_versions() {
        let mut values = std::collections::HashSet::new();
        for browser in [1, 2, 4, 5] {
            let base = Identity {
                browser,
                ..Default::default()
            };
            for &platform in base.platforms() {
                for version in 0..base.versions().len() {
                    let identity = Identity {
                        browser,
                        platform,
                        version,
                    };
                    let value = identity.value();
                    assert!(valid_custom(&value));
                    assert!(values.insert(value.clone()), "distinct choice");
                    assert_eq!(Identity::detect(&value), Some(identity));
                    assert_eq!(mode(&value, false), browser);
                }
            }
        }
        assert!(values.len() >= 50);
        assert_eq!(
            Identity {
                browser: 5,
                platform: 0,
                version: 0
            }
            .normalized()
            .platform,
            2
        );
        assert_eq!(mode("Unrecognized client", false), 3);
        assert_eq!(mode(&Identity::default().value(), true), 3);
        assert!(!valid_custom("Injected\r\nHeader: bad"));
    }
}
