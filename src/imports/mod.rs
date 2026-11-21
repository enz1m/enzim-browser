use crate::{history::Visit, search::Engine};
use std::{path::PathBuf, sync::Arc};
mod discover;
mod read;
mod snapshot;
pub(crate) use discover::{discover, from_folder};
pub(crate) use read::prepare;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Family {
    Firefox,
    Chromium,
}
#[derive(Clone, Debug)]
pub(crate) struct Source {
    pub(crate) browser: String,
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    pub(crate) family: Family,
}
impl Source {
    pub(crate) fn label(&self) -> String {
        format!("{} · {}", self.browser, self.name)
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Bookmark {
    pub(crate) url: String,
    pub(crate) title: String,
    pub(crate) folders: Vec<String>,
}
#[derive(Clone, Debug)]
pub(crate) struct Preview {
    pub(crate) source: Source,
    pub(crate) bookmarks: Vec<Bookmark>,
    pub(crate) history: Vec<Visit>,
    pub(crate) engines: Vec<Engine>,
    pub(crate) notes: Vec<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Part {
    Bookmarks,
    History,
    Search,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Options {
    pub(crate) bookmarks: bool,
    pub(crate) history: bool,
    pub(crate) search: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            bookmarks: true,
            history: true,
            search: false,
        }
    }
}
impl Options {
    pub(crate) fn any(self, data: &Preview) -> bool {
        (self.bookmarks && !data.bookmarks.is_empty())
            || (self.history && !data.history.is_empty())
            || (self.search && !data.engines.is_empty())
    }
}
#[derive(Default)]
pub(crate) struct State {
    pub(crate) profiles: Vec<Source>,
    pub(crate) selected: Option<usize>,
    pub(crate) review: Option<Arc<Preview>>,
    pub(crate) options: Options,
    pub(crate) busy: bool,
    pub(crate) applying: bool,
    pub(crate) generation: u64,
    pub(crate) notice: String,
    pub(crate) completed: bool,
}
pub(super) fn web_url(value: &str) -> Option<String> {
    if value.len() > 8192 {
        return None;
    }
    let url = url::Url::parse(value).ok()?;
    (matches!(url.scheme(), "https" | "http")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none())
    .then(|| url.to_string())
}
#[cfg(test)]
mod tests;
