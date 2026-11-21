use serde::{Deserialize, Serialize};
pub(crate) fn encrypted_media_available() -> bool {
    std::env::var_os("ENZIM_WPE_ENCRYPTED_MEDIA").is_some_and(|value| value == "1")
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Privacy {
    pub(crate) blocking: usize,
    pub(crate) ads: bool,
    pub(crate) trackers: bool,
    pub(crate) site_settings:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, bool>>,
    pub(crate) allowed_sites: Vec<String>,
    pub(crate) javascript: bool,
    pub(crate) webgl: bool,
    pub(crate) webrtc: bool,
    pub(crate) autoplay: bool,
    pub(crate) encrypted_media: bool,
    pub(crate) third_party_cookies: bool,
    pub(crate) tracking_prevention: bool,
    pub(crate) clean_links: bool,
    pub(crate) custom_agent: bool,
    pub(crate) user_agent: String,
}
impl Default for Privacy {
    fn default() -> Self {
        Self {
            blocking: 1,
            ads: true,
            trackers: true,
            site_settings: Default::default(),
            allowed_sites: Vec::new(),
            javascript: true,
            webgl: true,
            webrtc: true,
            autoplay: false,
            encrypted_media: false,
            third_party_cookies: false,
            tracking_prevention: true,
            clean_links: true,
            custom_agent: false,
            user_agent: String::new(),
        }
    }
}
impl Privacy {
    pub(crate) fn origin(address: &str) -> Option<String> {
        let url = url::Url::parse(address).ok()?;
        matches!(url.scheme(), "http" | "https").then(|| url.origin().ascii_serialization())
    }
    pub(crate) fn value(&self, key: &str) -> bool {
        match key {
            "ads" => self.blocking > 0 && self.ads,
            "trackers" => self.blocking > 0 && self.trackers,
            "javascript" => self.javascript,
            "webgl" => self.webgl,
            "webrtc" => self.webrtc,
            "autoplay" => self.autoplay,
            "drm" => self.encrypted_media,
            "cookies" => self.third_party_cookies,
            "tracking" => self.tracking_prevention,
            "links" => self.clean_links,
            _ => false,
        }
    }
    pub(crate) fn set_control(&mut self, key: &str, value: bool) {
        let target = match key {
            "ads" => &mut self.ads,
            "trackers" => &mut self.trackers,
            "javascript" => &mut self.javascript,
            "webgl" => &mut self.webgl,
            "webrtc" => &mut self.webrtc,
            "autoplay" => &mut self.autoplay,
            "drm" => &mut self.encrypted_media,
            "cookies" => &mut self.third_party_cookies,
            "tracking" => &mut self.tracking_prevention,
            "links" => &mut self.clean_links,
            _ => return,
        };
        *target = value;
        if value && matches!(key, "ads" | "trackers") && self.blocking == 0 {
            self.blocking = 1;
        }
    }
    pub(crate) fn value_for(&self, key: &str, address: &str) -> bool {
        if !matches!(
            key,
            "ads" | "trackers" | "javascript" | "webgl" | "webrtc" | "autoplay" | "drm" | "links"
        ) {
            return self.value(key);
        }
        let origin = Self::origin(address);
        if let Some(value) = origin
            .as_ref()
            .and_then(|o| self.site_settings.get(o))
            .and_then(|s| s.get(key))
        {
            return *value;
        }
        if matches!(key, "ads" | "trackers")
            && origin.is_some_and(|o| self.allowed_sites.contains(&o))
        {
            return false;
        }
        self.value(key)
    }
    pub(crate) fn toggle_site(&mut self, key: &str, address: &str) {
        if !matches!(
            key,
            "ads" | "trackers" | "javascript" | "webgl" | "webrtc" | "autoplay" | "drm" | "links"
        ) {
            return;
        }
        if let Some(origin) = Self::origin(address) {
            let next = !self.value_for(key, address);
            self.site_settings
                .entry(origin)
                .or_default()
                .insert(key.into(), next);
        }
    }
    pub(crate) fn for_site(&self, address: &str) -> Self {
        let mut p = self.clone();
        p.javascript = self.value_for("javascript", address);
        p.webgl = self.value_for("webgl", address);
        p.webrtc = self.value_for("webrtc", address);
        p.autoplay = self.value_for("autoplay", address);
        p.encrypted_media = self.value_for("drm", address);
        p.clean_links = self.value_for("links", address);
        p
    }
    pub(crate) fn apply_for_site(
        &self,
        session: &enzui_webkit::ffi::runtime::WebSession,
        address: &str,
        developer_tools: bool,
        native_default_agent: &str,
    ) {
        let mut policy = self.for_site(address);
        if self.user_agent.is_empty()
            && let Some(agent) =
                crate::user_agent::site_compatibility_default(address, native_default_agent)
        {
            policy.user_agent = agent;
        }
        policy.apply_with_developer_tools(session, developer_tools);
    }
    pub(crate) fn blocking_for(&self, url: &str) -> usize {
        let ads = self.value_for("ads", url);
        let trackers = self.value_for("trackers", url);
        if !ads && !trackers {
            0
        } else {
            self.blocking.clamp(1, 2) | if ads { 0 } else { 4 } | if trackers { 0 } else { 8 }
        }
    }
    pub(crate) fn ad_script_guard(&self) -> String {
        let mut sites = std::collections::BTreeMap::new();
        for origin in self.site_settings.keys().chain(self.allowed_sites.iter()) {
            let enabled = self.value_for("ads", origin);
            if enabled != self.value("ads") {
                sites.insert(origin, enabled);
            }
        }
        let data = serde_json::json!({"enabled":self.value("ads"),"sites":sites});
        format!("const policy={data};if(!(policy.sites[location.origin]??policy.enabled))return;")
    }
    pub(crate) fn apply(&self, session: &enzui_webkit::ffi::runtime::WebSession) {
        self.apply_with_developer_tools(session, false);
    }
    pub(crate) fn apply_with_developer_tools(
        &self,
        session: &enzui_webkit::ffi::runtime::WebSession,
        developer_tools: bool,
    ) {
        use enzui_webkit::page::{CookiePolicy, UserAgent, WebSettings};
        session.apply_settings(&WebSettings {
            user_agent: if self.user_agent.is_empty() {
                UserAgent::Default
            } else {
                UserAgent::Custom(self.user_agent.clone())
            },
            javascript: self.javascript,
            webgl: self.webgl,
            webrtc: self.webrtc,
            media_requires_gesture: !self.autoplay,
            encrypted_media: self.encrypted_media,
            developer_extras: developer_tools,
            ..Default::default()
        });
        session.set_cookie_policy(
            if self.third_party_cookies {
                CookiePolicy::Always
            } else {
                CookiePolicy::NoThirdParty
            },
            self.tracking_prevention,
        );
    }
    pub(crate) fn clean_url(&self, value: &str) -> String {
        if !self.value_for("links", value) {
            return value.into();
        }
        let Ok(mut url) = url::Url::parse(value) else {
            return value.into();
        };
        if !matches!(url.scheme(), "http" | "https") {
            return value.into();
        }
        let pairs: Vec<(String, String)> = url
            .query_pairs()
            .filter(|(key, _)| {
                let key = key.to_ascii_lowercase();
                !key.starts_with("utm_")
                    && ![
                        "fbclid", "gclid", "dclid", "msclkid", "mc_cid", "mc_eid", "igshid",
                        "gbraid", "wbraid",
                    ]
                    .contains(&key.as_str())
            })
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        let previous = url.query_pairs().count();
        if pairs.len() != previous {
            url.set_query(None);
            if !pairs.is_empty() {
                url.query_pairs_mut().extend_pairs(pairs);
            }
        }
        url.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encrypted_media_is_opt_in_and_keeps_site_and_container_choices_separate() {
        let mut defaults: Privacy = serde_json::from_str("{}").expect("old privacy settings");
        assert!(!defaults.encrypted_media);
        defaults.set_control("drm", true);
        defaults.toggle_site("drm", "https://example.org/watch");
        assert!(
            !defaults
                .for_site("https://example.org/other")
                .encrypted_media
        );
        assert!(defaults.for_site("https://other.example/").encrypted_media);
        assert!(defaults.for_site("http://example.org/").encrypted_media);
        let mut container = crate::container_privacy::Overrides::default();
        assert!(
            container
                .apply(&defaults)
                .for_site("https://example.org/")
                .encrypted_media
        );
        container.controls.insert("drm".into(), false);
        container
            .sites
            .entry("https://work.example".into())
            .or_default()
            .insert("drm".into(), true);
        let policy = container.apply(&defaults);
        assert!(!policy.for_site("https://other.example/").encrypted_media);
        assert!(policy.for_site("https://work.example/").encrypted_media);
        assert!(defaults.encrypted_media);
        let saved = serde_json::to_vec(&policy).expect("save privacy");
        let restored: Privacy = serde_json::from_slice(&saved).expect("restore privacy");
        assert!(restored.for_site("https://work.example/").encrypted_media);
    }
    #[test]
    fn refinement_privacy_ads_exception_is_exact_origin_and_keeps_other_protections() {
        let mut privacy = Privacy::default();
        privacy.toggle_site("ads", "https://example.com/a");
        assert!(!privacy.value_for("ads", "https://example.com/b"));
        for origin in [
            "http://example.com/",
            "https://sub.example.com/",
            "https://example.com:8443/",
        ] {
            assert!(privacy.value_for("ads", origin));
        }
        assert!(privacy.value_for("trackers", "https://example.com/"));
        assert!(privacy.for_site("https://example.com").tracking_prevention);
        assert!(!privacy.for_site("https://example.com").third_party_cookies);
        assert_eq!(privacy.blocking_for("https://example.com"), 5);
        privacy.ads = false;
        assert_eq!(privacy.blocking_for("https://elsewhere.org"), 5);
        privacy.toggle_site("ads", "https://example.com");
        assert_eq!(privacy.blocking_for("https://example.com"), 1);
        let roundtrip: Privacy =
            serde_json::from_str(&serde_json::to_string(&privacy).unwrap()).unwrap();
        assert_eq!(roundtrip.blocking_for("https://example.com"), 1);
    }
}
