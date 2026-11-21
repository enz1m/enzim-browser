use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Engine {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) template: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Search {
    pub(crate) selected: String,
    pub(crate) custom: Vec<Engine>,
    pub(crate) suggestions: bool,
}

impl Default for Search {
    fn default() -> Self {
        Self {
            selected: "duckduckgo".into(),
            custom: Vec::new(),
            suggestions: true,
        }
    }
}

pub(crate) fn builtins() -> Vec<Engine> {
    [
        (
            "duckduckgo",
            "DuckDuckGo",
            "https://duckduckgo.com/?q={query}",
        ),
        (
            "google",
            "Google",
            "https://www.google.com/search?q={query}",
        ),
        (
            "startpage",
            "Startpage",
            "https://www.startpage.com/sp/search?query={query}",
        ),
        (
            "brave",
            "Brave Search",
            "https://search.brave.com/search?q={query}",
        ),
        ("bing", "Bing", "https://www.bing.com/search?q={query}"),
        (
            "wikipedia",
            "Wikipedia",
            "https://en.wikipedia.org/w/index.php?search={query}",
        ),
    ]
    .into_iter()
    .map(|(id, name, template)| Engine {
        id: id.into(),
        name: name.into(),
        template: template.into(),
    })
    .collect()
}

impl Search {
    pub(crate) fn engines(&self) -> Vec<Engine> {
        let mut engines = builtins();
        engines.extend(self.custom.iter().cloned());
        engines
    }

    pub(crate) fn active(&self) -> Engine {
        self.engines()
            .into_iter()
            .find(|e| e.id == self.selected)
            .unwrap_or_else(|| Engine {
                id: "duckduckgo".into(),
                name: "DuckDuckGo".into(),
                template: "https://duckduckgo.com/?q={query}".into(),
            })
    }

    pub(crate) fn resolve(&self, input: &str) -> String {
        let input = input.trim();
        if input.contains("://") || input.starts_with("about:") {
            return input.into();
        }
        if let Some(file) = local_file(input) {
            return file;
        }
        // No host starts with a slash: a path to nothing is not an address.
        if !input.contains(char::is_whitespace) && !input.starts_with('/') {
            let candidate = format!("http://{input}");
            if (input.contains('.') || input.contains(':') || input == "localhost")
                && crate::browser::local::network_origin(&candidate).is_some()
            {
                return candidate;
            }
            if input.contains('.') || input.starts_with("localhost") {
                return format!("https://{input}");
            }
        }
        self.search_url(input)
    }

    /// The selected engine's address for a search, whatever the words look
    /// like.
    pub(crate) fn search_url(&self, query: &str) -> String {
        self.active().template.replace(
            "{query}",
            &url::form_urlencoded::byte_serialize(query.as_bytes()).collect::<String>(),
        )
    }

    pub(crate) fn add_custom(&mut self, name: &str, template: &str) -> Result<(), String> {
        let name = name.trim();
        let template = template.trim().replace("%s", "{query}");
        if name.is_empty() || name.chars().count() > 60 {
            return Err("Use a name between 1 and 60 characters.".into());
        }
        if self
            .engines()
            .iter()
            .any(|e| e.name.eq_ignore_ascii_case(name))
        {
            return Err("A search engine already uses that name.".into());
        }
        if template.len() > 4096 || template.matches("{query}").count() != 1 {
            return Err("Include one {query} or %s placeholder in the search URL.".into());
        }
        let probe = url::Url::parse(&template.replace("{query}", "enzimquery"))
            .map_err(|_| "Enter a valid HTTPS search URL.")?;
        if probe.scheme() != "https"
            || probe.host_str().is_none()
            || !probe.username().is_empty()
            || probe.password().is_some()
            || probe.fragment().is_some()
        {
            return Err("Use an HTTPS search URL without credentials or a fragment.".into());
        }
        let prefix = template.split_once("{query}").map_or("", |(s, _)| s);
        if !prefix
            .split_once("://")
            .is_some_and(|(_, rest)| rest.contains('/'))
        {
            return Err("Place the query in the URL path or query parameters.".into());
        }
        let id = format!("custom-{}", chrono::Utc::now().timestamp_micros());
        self.custom.push(Engine {
            id: id.clone(),
            name: name.into(),
            template,
        });
        self.selected = id;
        Ok(())
    }
}

/// The address of a file named by its path, as a person pastes one from a
/// file manager or a terminal. Only a path to a file that is there: anything
/// else that starts with a slash is words to search for.
fn local_file(input: &str) -> Option<String> {
    let path = if let Some(rest) = input.strip_prefix("~/") {
        std::path::PathBuf::from(std::env::var_os("HOME")?).join(rest)
    } else if input.starts_with('/') {
        std::path::PathBuf::from(input)
    } else {
        return None;
    };
    path.is_file()
        .then(|| url::Url::from_file_path(&path).ok())
        .flatten()
        .map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_custom_queries_are_encoded_without_turning_into_parameters() {
        let mut search = Search::default();
        search
            .add_custom("Library", "https://example.com/find?term=%s")
            .expect("custom engine");
        let result = url::Url::parse(&search.resolve("rust &λ=1 #docs")).expect("search URL");
        let pairs = result.query_pairs().collect::<Vec<_>>();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].1, "rust &λ=1 #docs");
        assert_eq!(
            search.resolve("example.org/path"),
            "https://example.org/path"
        );
        assert_eq!(search.resolve("enzim://history"), "enzim://history");
    }
    #[test]
    fn a_typed_path_to_a_file_that_is_there_opens_that_file() {
        let search = Search::default();
        let folder = std::env::temp_dir().join(format!("enzim-search-{}", std::process::id()));
        std::fs::create_dir_all(&folder).expect("folder");
        let file = folder.join("notes one.txt");
        std::fs::write(&file, "notes").expect("file");
        let typed = file.to_str().expect("path");
        let address = search.resolve(typed);
        assert!(address.starts_with("file:///"), "{address}");
        assert!(address.ends_with("/notes%20one.txt"), "{address}");
        // A path to nothing is words to search for, as before.
        let missing = folder.join("missing.txt");
        assert!(
            search
                .resolve(missing.to_str().expect("path"))
                .starts_with("https://duckduckgo.com/")
        );
        assert!(search.resolve("/r/rust").starts_with("https://duckduckgo.com/"));
        std::fs::remove_dir_all(&folder).expect("remove the folder");
    }
    #[test]
    fn search_rejects_invalid_templates_without_replacing_the_current_engine() {
        let mut search = Search::default();
        for template in [
            "javascript:alert('{query}')",
            "http://example.org/?q={query}",
            "https://user:secret@example.org/?q={query}",
            "https://{query}.example.org/",
            "https://example.org/?q=missing",
            "https://example.org/?a={query}&b={query}",
        ] {
            assert!(search.add_custom("Unsafe", template).is_err(), "{template}");
        }
        assert_eq!(search.selected, "duckduckgo");
        assert!(search.custom.is_empty());
    }
}
