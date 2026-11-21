use crate::{
    App, Message,
    browser::{self, Command, Panel},
};
use iced::Task;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Request {
    query: String,
    engine: String,
    url: String,
    proxy: Option<String>,
}

#[derive(Default)]
pub(crate) struct State {
    generation: u64,
    request: Option<Request>,
    pub(crate) results: Vec<String>,
    handle: Option<iced::task::Handle>,
    cancel: Option<Arc<AtomicBool>>,
}
impl Drop for State {
    fn drop(&mut self) {
        self.cancel();
    }
}
impl State {
    fn cancel(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, Ordering::Release);
        }
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
    fn replace(&mut self, request: Option<Request>) -> Task<Message> {
        if request == self.request {
            return Task::none();
        }
        self.cancel();
        self.generation = self.generation.wrapping_add(1);
        self.results.clear();
        self.request.clone_from(&request);
        let Some(request) = request else {
            return Task::none();
        };
        let generation = self.generation;
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = Some(Arc::clone(&cancel));
        let (task, handle) = Task::future(async move {
            tokio::time::sleep(Duration::from_millis(250)).await;
            if cancel.load(Ordering::Acquire) {
                return Vec::new();
            }
            let (send, receive) = iced::futures::channel::oneshot::channel();
            let _ = std::thread::Builder::new()
                .name("search-suggestions".into())
                .spawn(move || {
                    let _ = send.send(fetch(&request, &cancel).unwrap_or_default());
                });
            receive.await.unwrap_or_default()
        })
        .map(move |results| browser::c(Command::SearchSuggestionsReady(generation, results)))
        .abortable();
        self.handle = Some(handle);
        task
    }
    pub(crate) fn clear(&mut self) {
        drop(self.replace(None));
    }
    pub(crate) fn accept(&mut self, generation: u64, results: Vec<String>, selecting: bool) {
        if self.request.is_some() && generation == self.generation {
            self.handle = None;
            // Arrival must not change the meaning of an already selected keyboard row.
            if !selecting {
                self.results = results;
            }
        }
    }
    pub(crate) fn matches(&self, query: &str, engine: &str) -> bool {
        self.request
            .as_ref()
            .is_some_and(|request| request.query == query.trim() && request.engine == engine)
    }
}

pub(crate) fn is_search(query: &str) -> bool {
    let query = query.trim();
    !query.is_empty()
        && query.chars().count() <= 160
        && !query.chars().any(char::is_control)
        && !query.contains([':', '/', '\\', '@', '.'])
        && !query.eq_ignore_ascii_case("localhost")
        && !query.chars().all(|c| c.is_ascii_digit())
}
fn endpoint(engine: &str) -> Option<&'static str> {
    Some(match engine {
        "duckduckgo" => "https://ac.duckduckgo.com/ac/?type=list&q=",
        "google" => "https://www.google.com/complete/search?client=firefox&q=",
        "bing" => "https://www.bing.com/osjson.aspx?query=",
        "startpage" => "https://www.startpage.com/osuggestions?query=",
        "brave" => "https://search.brave.com/api/suggest?q=",
        "wikipedia" => "https://en.wikipedia.org/w/api.php?action=opensearch&limit=6&search=",
        _ => return None,
    })
}
fn parse(bytes: &[u8]) -> Result<Vec<String>, ()> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| ())?;
    let list = value
        .get(1)
        .and_then(serde_json::Value::as_array)
        .ok_or(())?;
    let mut result = Vec::new();
    for item in list.iter().take(24) {
        let Some(term) = item.as_str().map(str::trim) else {
            continue;
        };
        if term.is_empty()
            || term.chars().count() > 160
            || term.chars().any(char::is_control)
            || result
                .iter()
                .any(|entry: &String| entry.eq_ignore_ascii_case(term))
        {
            continue;
        }
        result.push(term.into());
        if result.len() == 6 {
            break;
        }
    }
    Ok(result)
}
fn fetch(request: &Request, cancel: &AtomicBool) -> Result<Vec<String>, ()> {
    if cancel.load(Ordering::Acquire) {
        return Ok(Vec::new());
    }
    let mut easy = curl::easy::Easy::new();
    easy.url(&request.url).map_err(|_| ())?;
    easy.connect_timeout(Duration::from_millis(900))
        .map_err(|_| ())?;
    easy.timeout(Duration::from_secs(3)).map_err(|_| ())?;
    easy.follow_location(false).map_err(|_| ())?;
    easy.useragent("enzim Browser search suggestions")
        .map_err(|_| ())?;
    easy.progress(true).map_err(|_| ())?;
    if let Some(proxy) = &request.proxy {
        easy.proxy(proxy).map_err(|_| ())?;
        easy.noproxy("").map_err(|_| ())?;
    }
    let mut bytes = Vec::new();
    {
        let mut transfer = easy.transfer();
        transfer
            .progress_function(|_, _, _, _| !cancel.load(Ordering::Acquire))
            .map_err(|_| ())?;
        transfer
            .write_function(|data| {
                if cancel.load(Ordering::Acquire) || bytes.len() + data.len() > 32 * 1024 {
                    return Ok(0);
                }
                bytes.extend_from_slice(data);
                Ok(data.len())
            })
            .map_err(|_| ())?;
        transfer.perform().map_err(|_| ())?;
    }
    if cancel.load(Ordering::Acquire) || easy.response_code().map_err(|_| ())? != 200 {
        return Ok(Vec::new());
    }
    parse(&bytes)
}
impl App {
    pub(crate) fn refresh_search_suggestions(&mut self) -> Task<Message> {
        let query = if self.browser.address_suggestions_open {
            Some(self.address.as_str())
        } else if self.browser.panel == Panel::New || self.browser.start_suggestions_open {
            Some(self.browser.query.as_str())
        } else {
            None
        };
        let request = query
            .filter(|query| {
                self.browser.saved.search.suggestions
                    && !self.browser.private
                    && is_search(query)
                    && query.trim().chars().count() >= 2
            })
            .and_then(|query| {
                let engine = self.browser.saved.search.active();
                let prefix = endpoint(&engine.id)?;
                let container = if self.browser.panel == Panel::New {
                    self.browser.new_tab_container
                } else {
                    self.browser.saved.tabs[self.browser.saved.active].container
                };
                let proxy = container
                    .and_then(|id| self.browser.container_routes.get(&id))
                    .filter(|proxy| !proxy.is_empty())
                    .cloned();
                let query = query.trim().to_owned();
                let encoded: String =
                    url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
                Some(Request {
                    query,
                    engine: engine.id,
                    url: format!("{prefix}{encoded}"),
                    proxy,
                })
            });
        self.browser.remote_search.replace(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remote_search_eligibility_protects_addresses_and_local_names() {
        for input in [
            "",
            "https://private.example/path",
            "192.168.1.8",
            "localhost",
            "server:32400",
            "user@example.com",
            "intranet/secret",
            "query\nsecret",
            "12345",
        ] {
            assert!(!is_search(input), "{input:?}");
        }
        assert!(is_search("rust programming"));
        assert!(is_search("東京 観光"));
        assert!(endpoint("custom-1").is_none());
        let old: crate::search::Search =
            serde_json::from_str(r#"{"selected":"google","custom":[]}"#).expect("old settings");
        assert!(old.suggestions);
    }
    #[test]
    fn remote_search_parser_bounds_and_deduplicates_untrusted_results() {
        assert_eq!(
            parse(br#"["rust",["Rust book","rust BOOK","",12,"bad\nterm","Rust tutorial"],[],{}]"#)
                .expect("suggestions"),
            ["Rust book", "Rust tutorial"]
        );
        assert!(parse(br#"{"unexpected":true}"#).is_err());
        let terms = vec!["x".repeat(200), "valid".into()];
        assert_eq!(
            parse(&serde_json::to_vec(&serde_json::json!(["x", terms])).expect("json"))
                .expect("bounded"),
            ["valid"]
        );
    }
    #[test]
    fn remote_search_rejects_late_results_and_keeps_keyboard_selection_stable() {
        let request = Request {
            query: "rust".into(),
            engine: "google".into(),
            url: String::new(),
            proxy: None,
        };
        let mut state = State {
            generation: 2,
            request: Some(request),
            results: Vec::new(),
            handle: None,
            cancel: None,
        };
        state.accept(1, vec!["stale".into()], false);
        assert!(state.results.is_empty());
        state.accept(2, vec!["valid".into()], true);
        assert!(state.results.is_empty());
        state.accept(2, vec!["valid".into()], false);
        assert_eq!(state.results, ["valid"]);
        let cancel = Arc::new(AtomicBool::new(false));
        state.cancel = Some(Arc::clone(&cancel));
        let _ = state.replace(None);
        assert!(cancel.load(Ordering::Acquire));
        state.accept(2, vec!["after dismiss".into()], false);
        assert!(state.results.is_empty());
    }
}

#[cfg(test)]
mod browser_behavior_tests {
    use super::*;
    #[test]
    fn remote_search_is_shared_cancelled_and_never_reopens_after_opt_out() {
        let mut app = crate::browser::browser_tests::app();
        app.browser_mode = true;
        app.browser.saved.history.clear();
        app.browser.saved.bookmarks.clear();
        let _ = app.update(Message::BrowserAddress("rust language".into()));
        let generation = app.browser.remote_search.generation;
        assert!(app.browser.remote_search.request.is_some());
        let _ = app.update(browser::c(Command::SearchSuggestionsReady(
            generation,
            vec!["rust book".into()],
        )));
        let _ = app.update(browser::c(Command::AddressSuggestionMove(false)));
        let selected = app
            .selected_address_suggestion()
            .expect("provider suggestion");
        let selected = url::Url::parse(&selected).expect("search URL");
        assert_eq!(
            selected
                .query_pairs()
                .find(|(key, _)| key == "q")
                .map(|(_, value)| value.into_owned()),
            Some("rust book".into())
        );
        let _ = app.update(browser::c(Command::SearchSuggestionsEnabled(false)));
        assert!(app.browser.remote_search.request.is_none());
        let _ = app.update(browser::c(Command::SearchSuggestionsReady(
            generation,
            vec!["late result".into()],
        )));
        assert!(app.browser.remote_search.results.is_empty());
        app.browser.private = true;
        let _ = app.update(browser::c(Command::SearchSuggestionsEnabled(true)));
        let _ = app.update(Message::BrowserAddress("private query".into()));
        assert!(app.browser.remote_search.request.is_none());
        app.browser.private = false;
        let _ = app.update(Message::BrowserAddress(
            "https://private.example/path".into(),
        ));
        assert!(app.browser.remote_search.request.is_none());
        app.browser
            .saved
            .search
            .add_custom("Example", "https://example.org/search?q={query}")
            .expect("custom engine");
        let _ = app.update(Message::BrowserAddress("custom engine query".into()));
        assert!(app.browser.remote_search.request.is_none());
    }
}
