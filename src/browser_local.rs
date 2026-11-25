use super::*;
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct LocalSite {
    pub(crate) container: u64,
    pub(crate) origin: String,
}
pub(super) fn origin(address: &str) -> Option<String> {
    let url = url::Url::parse(address).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    let origin = url.origin().ascii_serialization();
    enzui_webkit::local_origins::LocalOriginAllowlist::default()
        .allow_origin(&origin)
        .ok()?;
    Some(origin)
}
/// A local destination allowed for a container: `(private window, container, origin)`.
pub(crate) type Claim = (bool, Option<u64>, String);

impl App {
    pub(super) fn open_local_site(&mut self) -> Task<Message> {
        let Some(origin) = origin(&self.address) else {
            return Task::none();
        };
        let container = self.browser.saved.tabs[self.browser.saved.active].container;
        self.grant_local(container, origin);
        self.browser.address_suggestions_open = false;
        self.update(Message::BrowserGo)
    }

    pub(super) fn grant_local(&mut self, container: Option<u64>, origin: String) {
        if !self.browser.local_grants.iter().any(|grant| *grant == (container, origin.clone())) {
            self.browser.local_grants.push((container, origin));
        }
    }

    /// Allows the local destination of `address` for the selected tab's container;
    /// only for an address the person chose, never one a page or agent opened.
    pub(super) fn grant_chosen_local(&mut self, address: &str) {
        if let Some(origin) = network_origin(address) {
            self.grant_local(self.browser.saved.tabs[self.browser.saved.active].container, origin);
        }
    }

    /// Drops grants no tab of this window shows any more, then lists what
    /// this window allows: its grants and the origins of its local documents.
    pub(crate) fn local_claims(&mut self) -> Vec<Claim> {
        let tabs = &self.browser.saved.tabs;
        let sign_in = self.assistant.sign_in_return.as_deref();
        self.browser.local_grants.retain(|(container, origin)| {
            Some(origin.as_str()) == sign_in
                || tabs
                    .iter()
                    .any(|tab| tab.container == *container && network_origin(&tab.url).as_deref() == Some(origin))
        });
        let private = self.browser.private;
        let mut claims: Vec<Claim> =
            self.browser.local_grants.iter().map(|(container, origin)| (private, *container, origin.clone())).collect();
        for tab in tabs {
            for origin in self.local_files.origins_of(&tab.url) {
                claims.push((private, tab.container, origin));
            }
        }
        claims.sort();
        claims.dedup();
        claims
    }

    /// Every origin `container` may reach locally, from all windows.
    pub(crate) fn local_origins(&mut self, container: Option<u64>) -> Vec<String> {
        let private = self.browser.private;
        let mut origins: Vec<String> = self
            .local_claims()
            .into_iter()
            .chain(self.browser.local_elsewhere.iter().cloned())
            .filter(|(p, c, _)| *p == private && *c == container)
            .map(|(.., origin)| origin)
            .chain(
                self.browser
                    .saved
                    .local_sites
                    .iter()
                    .filter(|site| Some(site.container) == container)
                    .map(|site| site.origin.clone()),
            )
            .collect();
        origins.sort();
        origins.dedup();
        origins
    }

    /// Replaces the local destinations of every container with a loaded tab
    /// in this window, so revoked grants stop applying to pages already open.
    pub(crate) fn sync_local_origins(&mut self) {
        let mut done = Vec::new();
        let active = self.browser.saved.active;
        let order = std::iter::once(active).chain((0..self.browser.saved.tabs.len()).filter(|index| *index != active));
        for index in order.collect::<Vec<_>>() {
            let container = self.browser.saved.tabs[index].container;
            if done.contains(&container) {
                continue;
            }
            let origins = self.local_origins(container);
            let host = if index == active {
                &self.webview
            } else if let Some(Some(tab)) = self.browser.sleeping.get(index) {
                &tab.host
            } else {
                continue;
            };
            let service = host.borrow();
            if let (Some(native), Some(view)) = (&service.native, service.live_view)
                && native.browse(&service.router, view, BrowserAction::LocalOrigins(origins)).is_ok()
            {
                done.push(container);
            }
        }
    }
}

pub(crate) fn loopback_origin(address: &str) -> Option<String> {
    let url = url::Url::parse(address).ok()?;
    let loopback = match url.host()? {
        url::Host::Domain(name) => {
            name.eq_ignore_ascii_case("localhost")
                || name.to_ascii_lowercase().ends_with(".localhost")
        }
        url::Host::Ipv4(ip) => ip.is_loopback(),
        url::Host::Ipv6(ip) => ip.is_loopback(),
    };
    loopback.then(|| origin(address)).flatten()
}

pub(crate) fn network_origin(address: &str) -> Option<String> {
    if let Some(origin) = loopback_origin(address) {
        return Some(origin);
    }
    let url = url::Url::parse(address).ok()?;
    let local = match url.host()? {
        url::Host::Domain(name) => {
            let name = name.to_ascii_lowercase();
            !name.contains('.')
                || [
                    ".localhost",
                    ".local",
                    ".lan",
                    ".home",
                    ".home.arpa",
                    ".internal",
                ]
                .iter()
                .any(|suffix| name.ends_with(suffix))
        }
        url::Host::Ipv4(ip) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
        url::Host::Ipv6(ip) => {
            ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local()
        }
    };
    local.then(|| origin(address)).flatten()
}

#[cfg(test)]
mod tests {
    #[test]
    fn browser_tests_typed_localhost_stays_http_without_granting_unrelated_destinations() {
        let search = crate::search::Search::default();
        for input in [
            "localhost:3000/path",
            "127.0.0.1:8080",
            "[::1]:9000/",
            "dev.localhost:4321/",
        ] {
            let address = search.resolve(input);
            assert!(address.starts_with("http://"), "{address}");
            assert!(super::loopback_origin(&address).is_some());
        }
        for address in [
            "https://example.com/",
            "http://192.168.50.1:3000/",
            "http://localhost.example.com/",
            "http://user:password@localhost:8080/",
            "file:///tmp/test",
        ] {
            assert!(super::loopback_origin(address).is_none(), "{address}");
        }
        assert_eq!(search.resolve("example.com"), "https://example.com");
    }
}

#[cfg(test)]
mod grant_tests {
    use crate::Message;
    use crate::browser::{Command, browser_tests::app, c};

    #[test]
    fn local_destinations_need_the_persons_choice_and_end_with_the_last_tab_showing_them() {
        let mut a = app();
        a.browser_mode = true;
        a.address = "http://192.168.50.1:8080/".into();
        let _ = a.update(Message::BrowserGo);
        assert!(a.local_origins(None).is_empty(), "an address a page or agent opened grants nothing");

        a.address = "192.168.50.1:8080".into();
        let _ = a.update(c(Command::AddressSubmit));
        assert_eq!(a.local_origins(None), ["http://192.168.50.1:8080"]);
        assert!(a.local_origins(Some(7)).is_empty(), "other containers stay closed");

        a.browser.local_elsewhere = vec![(false, None, "http://nas.local".into()), (true, None, "http://printer.local".into())];
        assert_eq!(a.local_origins(None), ["http://192.168.50.1:8080", "http://nas.local"]);
        a.browser.local_elsewhere.clear();

        a.address = "https://example.org/".into();
        let _ = a.update(Message::BrowserGo);
        assert!(a.local_origins(None).is_empty(), "leaving the site ends its grant");
        assert!(a.browser.local_grants.is_empty());
    }
}

#[cfg(test)]
mod lan_tests {
    #[test]
    fn local_network_addresses_use_http_and_exact_origin_grants() {
        let search = crate::search::Search::default();
        for input in [
            "192.168.50.1:32400/web",
            "10.20.0.2:8000/",
            "172.16.1.3",
            "[fd00::2]:9000/",
            "nas.local:8080/",
        ] {
            let address = search.resolve(input);
            assert!(address.starts_with("http://"), "{address}");
            let grant = super::network_origin(&address).expect("local origin");
            assert_eq!(
                url::Url::parse(&address)
                    .expect("URL")
                    .origin()
                    .ascii_serialization(),
                grant
            );
        }
        assert!(super::network_origin("http://nas/").is_some());
        for address in [
            "https://example.com/",
            "https://8.8.8.8/",
            "http://192.168.50.1.example.com/",
            "http://user:secret@192.168.50.1/",
        ] {
            assert!(super::network_origin(address).is_none(), "{address}");
        }
        assert!(search.resolve("rust").contains("duckduckgo.com"));
    }
}
