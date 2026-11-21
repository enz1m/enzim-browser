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
impl App {
    pub(super) fn open_local_site(&mut self) -> Task<Message> {
        let Some(origin) = origin(&self.address) else {
            return Task::none();
        };
        let container = self.browser.saved.tabs[self.browser.saved.active].container;
        if !self
            .browser
            .local_grants
            .iter()
            .any(|grant| grant == &(container, origin.clone()))
        {
            self.browser.local_grants.push((container, origin));
        }
        self.browser.address_suggestions_open = false;
        self.update(Message::BrowserGo)
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
