use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Overrides {
    pub(crate) controls: BTreeMap<String, bool>,
    pub(crate) user_agent: Option<String>,
    pub(crate) upstream_proxy: String,
    /// The proxy's username, when it asks for one. Its password is kept in
    /// the desktop keyring, never here.
    pub(crate) proxy_username: String,
    /// Agents (through the agent center) may not see or use this
    /// container's tabs, nor open pages in it.
    pub(crate) deny_agents: bool,
    pub(crate) sites: BTreeMap<String, BTreeMap<String, bool>>,
}

impl Overrides {
    pub(crate) fn apply(&self, defaults: &crate::privacy::Privacy) -> crate::privacy::Privacy {
        let mut policy = defaults.clone();
        policy.ads = defaults.value("ads");
        policy.trackers = defaults.value("trackers");
        for (key, value) in &self.controls {
            policy.set_control(key, *value);
        }
        if let Some(agent) = &self.user_agent {
            policy.user_agent.clone_from(agent);
        }
        policy.site_settings.clone_from(&self.sites);
        policy.allowed_sites.clear();
        policy
    }
}

/// A container's proxy: how it is spoken to and where it is.
pub(crate) type Proxy = (enzui_webkit::destination_guard::ProxyKind, std::net::SocketAddr);

/// Separates a route's proxy from its username; a username holds no control
/// characters.
const ROUTE_USER: char = '\u{1f}';

/// The proxy written in a container's settings: an HTTP CONNECT proxy as
/// `127.0.0.1:8080` (or with `http://`), a SOCKS5 proxy as
/// `socks5://127.0.0.1:1080`, both at numeric addresses.
pub(crate) fn parse_proxy(value: &str) -> Result<Option<Proxy>, String> {
    use enzui_webkit::destination_guard::ProxyKind;
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    let (kind, rest) = if let Some(rest) = value.strip_prefix("socks5://") {
        (ProxyKind::Socks5, rest)
    } else {
        (ProxyKind::HttpConnect, value.strip_prefix("http://").unwrap_or(value))
    };
    let rest = rest.strip_suffix('/').unwrap_or(rest);
    let address: std::net::SocketAddr = rest.parse().map_err(|_| "Use a numeric proxy address such as 127.0.0.1:8080 for an HTTP proxy, or socks5://127.0.0.1:1080 for a SOCKS5 proxy. Put the username and password in their own fields.".to_owned())?;
    if address.port() == 0 || address.ip().is_unspecified() {
        return Err("Choose a proxy address and nonzero port.".into());
    }
    Ok(Some((kind, address)))
}

/// The proxy written back in its plain form: an HTTP proxy as its bare
/// address, as before SOCKS5 was offered.
pub(crate) fn proxy_text((kind, address): Proxy) -> String {
    match kind {
        enzui_webkit::destination_guard::ProxyKind::HttpConnect => address.to_string(),
        enzui_webkit::destination_guard::ProxyKind::Socks5 => format!("socks5://{address}"),
    }
}

/// Whether `username` can sign in to a proxy of `kind`.
pub(crate) fn check_username(
    kind: enzui_webkit::destination_guard::ProxyKind,
    username: &str,
) -> Result<(), String> {
    if username.len() > 255 || username.chars().any(char::is_control) {
        return Err("Use a proxy username of at most 255 bytes without control characters.".into());
    }
    if kind == enzui_webkit::destination_guard::ProxyKind::HttpConnect && username.contains(':') {
        return Err("An HTTP proxy username cannot contain a colon.".into());
    }
    Ok(())
}

/// A container's network route, as applied when the browser starts: its
/// proxy and, after a separator, the username it signs in with.
pub(crate) fn route(privacy: &Overrides) -> String {
    let username = privacy.proxy_username.trim();
    if privacy.upstream_proxy.trim().is_empty() || username.is_empty() {
        privacy.upstream_proxy.clone()
    } else {
        format!("{}{ROUTE_USER}{username}", privacy.upstream_proxy)
    }
}

/// A route's proxy and username.
pub(crate) fn parse_route(route: &str) -> Result<Option<(Proxy, Option<&str>)>, String> {
    let (proxy, username) = match route.split_once(ROUTE_USER) {
        Some((proxy, username)) => (proxy, Some(username)),
        None => (route, None),
    };
    Ok(parse_proxy(proxy)?.map(|proxy| (proxy, username.filter(|u| !u.is_empty()))))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn enabling_one_container_filter_preserves_the_other_inherited_off_category() {
        let defaults = crate::privacy::Privacy {
            blocking: 0,
            ..Default::default()
        };
        let mut container = Overrides::default();
        container.controls.insert("ads".into(), true);
        let policy = container.apply(&defaults);
        assert!(policy.value("ads"));
        assert!(!policy.value("trackers"));
        assert!(!defaults.value("ads"));
        container.controls.clear();
        container.controls.insert("trackers".into(), true);
        let policy = container.apply(&defaults);
        assert!(!policy.value("ads"));
        assert!(policy.value("trackers"));
    }
    #[test]
    fn container_proxy_parsing_is_explicit_and_routes_stay_pinned_until_restart() {
        assert_eq!(parse_proxy("").expect("direct"), None);
        assert_eq!(
            parse_proxy("http://127.0.0.1:8080/")
                .expect("HTTP proxy")
                .map(proxy_text),
            Some("127.0.0.1:8080".into())
        );
        assert!(parse_proxy("[::1]:8080").expect("IPv6 proxy").is_some());
        for value in [
            "http://user:pass@127.0.0.1:8080",
            "socks5://user:pass@127.0.0.1:1080",
            "socks4://127.0.0.1:1080",
            "proxy.example:8080",
            "127.0.0.1:0",
            "0.0.0.0:8080",
        ] {
            assert!(parse_proxy(value).is_err(), "must refuse {value}");
        }
        let mut app = crate::browser::browser_tests::app();
        let id = app.browser.saved.containers[0].id;
        app.browser
            .container_routes
            .insert(id, "127.0.0.1:8080".into());
        app.browser.saved.containers[0].privacy.upstream_proxy = "127.0.0.1:9090".into();
        let profile = app
            .routed_profile(Some(id), enzui_webkit::web::WebProfile::ephemeral())
            .expect("running route");
        assert_eq!(
            profile.upstream_proxy().expect("proxy").address().port(),
            8080
        );
        assert!(
            app.routed_profile(None, enzui_webkit::web::WebProfile::shared())
                .expect("normal profile")
                .upstream_proxy()
                .is_none()
        );
        app.browser.container_routes.insert(id, "invalid".into());
        assert!(
            app.routed_profile(Some(id), enzui_webkit::web::WebProfile::ephemeral())
                .is_err(),
            "invalid proxy must never fall back direct"
        );
    }
    #[test]
    fn a_socks5_or_signed_in_proxy_keeps_its_password_out_of_the_route() {
        use enzui_webkit::destination_guard::ProxyKind;
        let socks = parse_proxy("socks5://127.0.0.1:1080").expect("SOCKS5 proxy");
        assert_eq!(socks.map(|proxy| proxy.0), Some(ProxyKind::Socks5));
        assert_eq!(socks.map(proxy_text), Some("socks5://127.0.0.1:1080".into()));
        assert!(check_username(ProxyKind::HttpConnect, "a:b").is_err());
        assert!(check_username(ProxyKind::Socks5, "a:b").is_ok());

        let privacy = Overrides {
            upstream_proxy: "socks5://127.0.0.1:1080".into(),
            proxy_username: "bob".into(),
            ..Default::default()
        };
        let route = route(&privacy);
        assert_ne!(route, privacy.upstream_proxy, "a username changes the route");
        let ((kind, _), username) = parse_route(&route).expect("route").expect("proxied");
        assert_eq!((kind, username), (ProxyKind::Socks5, Some("bob")));

        let mut app = crate::browser::browser_tests::app();
        let id = app.browser.saved.containers[0].id;
        app.browser.container_routes.insert(id, route);
        app.browser
            .proxy_passwords
            .borrow_mut()
            .insert(id, crate::credentials::Secret::new("hunter2".into()));
        let proxy = app
            .routed_profile(Some(id), enzui_webkit::web::WebProfile::ephemeral())
            .expect("signed-in route")
            .upstream_proxy()
            .expect("proxy");
        assert_eq!(proxy.kind(), ProxyKind::Socks5);
        assert!(proxy.authenticated(), "the cached password signs in");
        assert!(!format!("{proxy:?}").contains("hunter2"));
        let saved = serde_json::to_string(&privacy).expect("settings");
        assert!(!saved.contains("hunter2"), "the settings never hold the password");
    }
    #[test]
    fn container_overrides_inherit_defaults_and_keep_site_choices_separate() {
        let mut defaults = crate::privacy::Privacy::default();
        defaults.toggle_site("ads", "https://example.org/");
        let mut work = Overrides::default();
        work.controls.insert("javascript".into(), false);
        work.user_agent = Some("Container agent".into());
        let policy = work.apply(&defaults);
        assert!(!policy.javascript);
        assert_eq!(policy.user_agent, "Container agent");
        assert!(policy.value_for("ads", "https://example.org/"));
        assert!(!defaults.value_for("ads", "https://example.org/"));
        assert!(defaults.javascript);
        work.sites
            .entry("https://example.org".into())
            .or_default()
            .insert("ads".into(), false);
        assert!(
            !work
                .apply(&defaults)
                .value_for("ads", "https://example.org/")
        );
        assert!(
            Overrides::default()
                .apply(&defaults)
                .value_for("ads", "https://example.org/")
        );
        defaults.autoplay = true;
        assert!(work.apply(&defaults).autoplay);
        let saved = serde_json::to_vec(&work).expect("serialize policy");
        let restored: Overrides = serde_json::from_slice(&saved).expect("restore policy");
        assert_eq!(restored.apply(&defaults).user_agent, "Container agent");
    }
}
