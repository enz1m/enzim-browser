use crate::browser::{Saved, TabInfo, next_tab_key};
use serde::{Deserialize, Serialize};

#[cfg(target_os = "linux")]
pub(crate) fn prepare_descriptor_table() {
    if std::env::var_os("ENZIM_PREPARE_FD_TABLE").is_some_and(|value| value == "1") {
        // Growing a shared Linux descriptor table waits for an RCU grace period.
        drop(rustix::io::fcntl_dupfd_cloexec(std::io::stderr(), 511));
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Mode {
    #[default]
    Restore,
    Empty,
    Page,
}

pub(crate) fn apply(saved: &mut Saved, private: bool) {
    if private || saved.startup == Mode::Restore {
        return;
    }
    let url = if saved.startup == Mode::Page {
        page_url(&saved.startup_page).unwrap_or_default()
    } else {
        String::new()
    };
    saved.tabs = vec![TabInfo {
        key: next_tab_key(),
        favicon: None,
        title: if url.is_empty() {
            "New tab".into()
        } else {
            url.clone()
        },
        url,
        pinned: false,
        group: None,
        container: None,
    }];
    saved.active = 0;
    saved.groups.clear();
    saved.tab_order.clear();
}

pub(crate) fn open_launch(saved: &mut Saved, url: &str) {
    let tab = TabInfo {
        key: next_tab_key(),
        favicon: None,
        url: url.into(),
        title: url.into(),
        pinned: false,
        group: None,
        container: None,
    };
    if saved.tabs.len() == 1 && saved.tabs[0].url.is_empty() && saved.tabs[0].container.is_none() {
        saved.tabs[0] = tab;
        saved.active = 0;
    } else {
        saved.active = saved.tabs.len();
        saved.tabs.push(tab);
    }
}

pub(crate) fn page_url(input: &str) -> Option<String> {
    let input = input.trim();
    if input.is_empty() || input.chars().any(char::is_whitespace) {
        return None;
    }
    let candidate = if input.contains("://") {
        input.to_owned()
    } else {
        let local = format!("http://{input}");
        if crate::browser::local::network_origin(&local).is_some() {
            local
        } else {
            format!("https://{input}")
        }
    };
    let url = url::Url::parse(&candidate).ok()?;
    (matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none())
    .then(|| url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_launch_preserves_restored_tabs_and_uses_the_default_container() {
        let mut saved = Saved::default();
        saved.tabs.push(TabInfo {
            key: next_tab_key(),
            favicon: None,
            url: "https://work.example".into(),
            title: "Work".into(),
            pinned: true,
            group: None,
            container: Some(7),
        });
        let key = saved.tabs[0].key;
        open_launch(&mut saved, "https://launch.example");
        assert_eq!(saved.tabs.len(), 2);
        assert_eq!(saved.tabs[0].url, "https://work.example");
        assert_eq!(saved.tabs[0].key, key);
        assert_eq!(saved.tabs[0].container, Some(7));
        assert!(saved.tabs[0].pinned);
        assert_eq!(saved.active, 1);
        assert_eq!(saved.tabs[1].url, "https://launch.example");
        assert!(saved.tabs[1].container.is_none());
        assert!(!saved.tabs[1].pinned);
        assert!(saved.tabs[1].group.is_none());
        let mut empty = Saved::default();
        open_launch(&mut empty, "https://launch.example");
        assert_eq!(empty.tabs.len(), 1);
        assert_eq!(empty.tabs[0].url, "https://launch.example");
    }
    #[test]
    fn startup_modes_preserve_sessions_or_replace_only_the_tab_strip() {
        let mut saved = Saved::default();
        saved.tabs.push(TabInfo {
            key: next_tab_key(),
            favicon: None,
            url: "https://example.org/work".into(),
            title: "Work".into(),
            pinned: true,
            group: Some(0),
            container: Some(1),
        });
        saved.groups.push(crate::browser::Group {
            name: "Research".into(),
            color: 2,
        });
        let original = saved.clone();
        apply(&mut saved, false);
        assert_eq!(saved.tabs[0].url, original.tabs[0].url);
        assert!(saved.tabs[0].pinned);
        assert_eq!(saved.tabs[0].container, Some(1));
        assert_eq!(saved.groups.len(), 1);
        saved.startup = Mode::Page;
        saved.startup_page = "example.net/welcome?first=1".into();
        let encoded = serde_json::to_vec(&saved).expect("serialize preferences");
        let mut restored: Saved = serde_json::from_slice(&encoded).expect("restore preferences");
        apply(&mut restored, false);
        assert_eq!(restored.tabs.len(), 1);
        assert_eq!(restored.active, 0);
        assert_eq!(restored.tabs[0].url, "https://example.net/welcome?first=1");
        assert!(restored.tabs[0].container.is_none());
        assert!(!restored.tabs[0].pinned);
        assert!(restored.groups.is_empty());
        assert_eq!(restored.containers.len(), original.containers.len());
        assert_eq!(restored.search.selected, original.search.selected);
        restored.startup = Mode::Empty;
        apply(&mut restored, false);
        assert_eq!(restored.tabs.len(), 1);
        assert!(restored.tabs[0].url.is_empty());
        let mut private = saved.clone();
        apply(&mut private, true);
        assert_eq!(private.tabs[0].url, original.tabs[0].url);
        assert_eq!(
            serde_json::from_str::<Saved>("{}")
                .expect("legacy preferences")
                .startup,
            Mode::Restore
        );
        assert_eq!(
            page_url("localhost:8080/welcome").as_deref(),
            Some("http://localhost:8080/welcome")
        );
        assert_eq!(
            page_url("192.168.50.1:32400/web").as_deref(),
            Some("http://192.168.50.1:32400/web")
        );
        for bad in [
            "",
            "not a website",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "https://user:secret@example.com",
        ] {
            assert!(page_url(bad).is_none(), "refuse {bad}");
        }
    }
}
