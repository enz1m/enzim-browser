use super::*;
use crate::imports::{Options, Part, Preview, Source};
use iced::widget::{Space, column, row, text};
use std::{path::PathBuf, sync::Arc};
const IMPORT_TAG: u64 = u64::MAX - 3;
#[derive(Clone, Debug)]
pub(crate) enum ImportCommand {
    Open,
    Scan,
    Choose,
    Select(usize),
    Read,
    Cancel,
    Found(u64, bool, Result<Vec<Source>, String>),
    Prepared(u64, Result<Arc<Preview>, String>),
    Toggle(Part, bool),
    Apply,
    Applied(u64, Arc<Preview>, Options, Result<usize, String>),
}
pub(crate) fn background<T: Send + 'static>(
    name: &str,
    job: impl FnOnce() -> Result<T, String> + Send + 'static,
    message: impl Fn(Result<T, String>) -> Message + Send + 'static,
) -> Task<Message> {
    let name = name.to_owned();
    Task::perform(
        async move {
            let (send, receive) = iced::futures::channel::oneshot::channel();
            std::thread::Builder::new()
                .name(name)
                .spawn(move || {
                    let _ = send.send(job());
                })
                .map_err(|e| e.to_string())?;
            receive
                .await
                .map_err(|_| "The import operation was interrupted".to_owned())?
        },
        message,
    )
}
fn snapshot_cache() -> PathBuf {
    crate::browser_profile::paths()
        .cache
        .join("import-snapshots")
}
impl App {
    pub(super) fn import_folder_chosen(&mut self, paths: Vec<PathBuf>) -> Task<Message> {
        if self.browser.private || self.browser.importing.applying {
            return Task::none();
        }
        let Some(path) = paths.into_iter().next() else {
            return Task::none();
        };
        self.browser.importing.generation = self.browser.importing.generation.wrapping_add(1);
        let generation = self.browser.importing.generation;
        self.browser.importing.busy = true;
        self.browser.importing.review = None;
        self.browser.importing.notice.clear();
        self.browser.importing.completed = false;
        background(
            "find-browser-profile",
            move || crate::imports::from_folder(&path),
            move |result| {
                c(Command::Import(ImportCommand::Found(
                    generation, true, result,
                )))
            },
        )
    }
    pub(super) fn import_command(&mut self, command: ImportCommand) -> Task<Message> {
        use ImportCommand::*;
        if matches!(command, Open) {
            self.open_internal_page(InternalPage::Import);
        }
        if self.browser.private {
            self.browser.importing.notice = "Import data from a regular window.".into();
            return Task::none();
        }
        match command {
            Open if self.browser.importing.profiles.is_empty() && !self.browser.importing.busy => {
                return self.import_command(Scan);
            }
            Open => {}
            Scan => {
                if self.browser.importing.applying {
                    return Task::none();
                }
                let state = &mut self.browser.importing;
                state.generation = state.generation.wrapping_add(1);
                state.busy = true;
                state.review = None;
                state.notice.clear();
                state.completed = false;
                let generation = state.generation;
                let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
                let config = std::env::var_os("XDG_CONFIG_HOME")
                    .map_or_else(|| home.join(".config"), PathBuf::from);
                return background(
                    "discover-browsers",
                    move || Ok(crate::imports::discover(&home, &config)),
                    move |result| c(Command::Import(Found(generation, false, result))),
                );
            }
            Choose => {
                if self.browser.importing.busy {
                    return Task::none();
                }
                if let Err(error) = self.browser.chooser.start(
                    (IMPORT_TAG, enzui_webkit::page::PageRequestId::new(0)),
                    enzui_core::portal::backend::PortalOperation::OpenDirectory,
                    enzui_core::portal::chooser::ChooserOptions::new()
                        .title("Choose browser profile folder"),
                ) {
                    self.browser
                        .toasts
                        .error("import", format!("Could not open chooser: {error:?}"));
                }
            }
            Found(generation, append, result) => {
                let state = &mut self.browser.importing;
                if state.generation != generation {
                    return Task::none();
                }
                state.busy = false;
                match result {
                    Err(error) => self.browser.toasts.error("import", error),
                    Ok(profiles) => {
                        let selected = profiles.first().map(|s| s.path.clone());
                        if !append {
                            state.profiles.clear();
                        }
                        for source in profiles {
                            if !state.profiles.iter().any(|s| s.path == source.path) {
                                state.profiles.push(source);
                            }
                        }
                        state.selected = selected
                            .and_then(|path| state.profiles.iter().position(|s| s.path == path));
                        state.review = None;
                        state.completed = false;
                        state.notice = if state.profiles.is_empty() {
                            "No browser profiles were detected. Choose a profile folder to continue.".into()
                        } else {
                            String::new()
                        };
                    }
                }
            }
            Select(index) => {
                let state = &mut self.browser.importing;
                if state.busy || index >= state.profiles.len() {
                    return Task::none();
                }
                state.selected = Some(index);
                state.review = None;
                state.notice.clear();
                state.completed = false;
                state.generation = state.generation.wrapping_add(1);
            }
            Read => {
                let state = &mut self.browser.importing;
                if state.busy {
                    return Task::none();
                }
                let Some(source) = state.selected.and_then(|i| state.profiles.get(i)).cloned()
                else {
                    return Task::none();
                };
                state.generation = state.generation.wrapping_add(1);
                state.busy = true;
                state.notice.clear();
                state.completed = false;
                state.review = None;
                let generation = state.generation;
                let cache = snapshot_cache();
                return background(
                    "read-browser-data",
                    move || crate::imports::prepare(source, &cache).map(Arc::new),
                    move |result| c(Command::Import(Prepared(generation, result))),
                );
            }
            Prepared(generation, result) => {
                let state = &mut self.browser.importing;
                if state.generation != generation {
                    return Task::none();
                }
                state.busy = false;
                match result {
                    Ok(data) => {
                        state.options = Options::default();
                        state.review = Some(data);
                    }
                    Err(error) => self.browser.toasts.error("import", error),
                }
            }
            Toggle(part, value) => {
                let state = &mut self.browser.importing;
                if state.busy {
                    return Task::none();
                }
                match part {
                    Part::Bookmarks => state.options.bookmarks = value,
                    Part::History => state.options.history = value,
                    Part::Search => state.options.search = value,
                }
            }
            Cancel => {
                let state = &mut self.browser.importing;
                if state.applying {
                    return Task::none();
                }
                state.generation = state.generation.wrapping_add(1);
                state.busy = false;
                state.review = None;
                state.notice.clear();
            }
            Apply => {
                let state = &mut self.browser.importing;
                if state.busy || state.completed {
                    return Task::none();
                }
                let Some(data) = state.review.clone() else {
                    return Task::none();
                };
                let options = state.options;
                if !options.any(&data) {
                    return Task::none();
                }
                state.generation = state.generation.wrapping_add(1);
                state.busy = true;
                state.applying = true;
                state.notice.clear();
                let generation = state.generation;
                let read = Arc::clone(&data);
                let path = state_path().with_file_name("history.sqlite");
                return background(
                    "merge-browser-history",
                    move || {
                        if !options.history || read.history.is_empty() {
                            return Ok(0);
                        }
                        crate::history::Store::open(Some(&path)).import_visits(&read.history)
                    },
                    move |result| {
                        c(Command::Import(Applied(
                            generation,
                            Arc::clone(&data),
                            options,
                            result,
                        )))
                    },
                );
            }
            Applied(generation, data, options, result) => {
                if self.browser.importing.generation != generation {
                    return Task::none();
                }
                self.browser.importing.busy = false;
                self.browser.importing.applying = false;
                match result {
                    Ok(history) => {
                        let (bookmarks, engines, duplicates) =
                            merge(&mut self.browser.saved, &data, options);
                        self.browser.saved.history =
                            self.browser.history_store.query("", 0).unwrap_or_default();
                        let skipped = duplicates
                            + if options.history {
                                data.history.len().saturating_sub(history)
                            } else {
                                0
                            };
                        self.browser.toasts.done(format!(
                            "Imported {}, {} and {}. Skipped {}.",
                            count_label(bookmarks, "bookmark", "bookmarks"),
                            count_label(history, "visit", "visits"),
                            count_label(engines, "search engine", "search engines"),
                            count_label(skipped, "duplicate", "duplicates")
                        ));
                        self.browser.importing.completed = true;
                        if !cfg!(test) {
                            self.browser.save();
                        }
                    }
                    Err(error) => {
                        self.browser.toasts.error(
                            "import",
                            format!(
                                "Import did not finish: {error}. Your existing data is preserved; try again."
                            ),
                        );
                    }
                }
            }
        }
        Task::none()
    }
    pub(super) fn import_page(&self) -> Element<'_, Message> {
        if self.browser.private {
            return self.page_document(
                "Import browser data",
                "download",
                "Bring your data into enzim Browser",
                pref(
                    "Regular window required",
                    "venetian-mask",
                    "Browser-data import is available from a regular window.",
                ),
            );
        }
        let state = &self.browser.importing;
        let p = self.look().palette.clone();
        let profiles = state.profiles.iter().map(Source::label).collect::<Vec<_>>();
        let selector = enzui_widgets::combo_box::enz_combo_box(
            "",
            profiles,
            state
                .selected
                .and_then(|i| state.profiles.get(i))
                .map(Source::label),
            |_| c(Command::Import(ImportCommand::Select(0))),
        )
        .id_prefix("import-profile")
        .aria_label("Source browser profile")
        .selected_index(state.selected)
        .on_select_index(|i| c(Command::Import(ImportCommand::Select(i))))
        .enabled(!state.busy)
        .width(Length::Fill)
        .into_element();
        let mut source = column![selector].spacing(10);
        if let Some(profile) = state.selected.and_then(|i| state.profiles.get(i)) {
            source = source.push(
                text(profile.path.display().to_string())
                    .size(11)
                    .color(p.text_muted)
                    .wrapping(iced::widget::text::Wrapping::None)
                    .ellipsis(iced::widget::text::Ellipsis::Middle),
            );
        }
        let controls = row![
            import_button(
                "Choose folder…",
                "folder-open",
                ImportCommand::Choose,
                !state.busy
            ),
            ink_icon(
                "refresh-cw",
                "Scan for profiles",
                c(Command::Import(ImportCommand::Scan)),
                30.,
                15.
            ),
            Space::new().width(Length::Fill),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);
        source = source.push(controls);
        let mut body = column![section("Browser and profile", source)].spacing(18);
        if state.busy {
            body = body.push(pref(
                if state.applying {
                    "Importing selected data…"
                } else {
                    "Reading browser data…"
                },
                "loader",
                "Your existing tabs and browser settings stay in place.",
            ));
            if !state.applying {
                body = body.push(action(
                    "Cancel",
                    "x",
                    Command::Import(ImportCommand::Cancel),
                ));
            }
        } else if let Some(data) = &state.review {
            let categories = column![
                import_option(
                    "Bookmarks",
                    "bookmark",
                    format!(
                        "{} · folder paths are preserved",
                        count_label(data.bookmarks.len(), "bookmark", "bookmarks")
                    ),
                    Part::Bookmarks,
                    state.options.bookmarks && !data.bookmarks.is_empty(),
                    !data.bookmarks.is_empty() && !state.completed
                ),
                import_option(
                    "Browsing history",
                    "history",
                    format!(
                        "{} · duplicates are skipped",
                        count_label(data.history.len(), "dated visit", "dated visits")
                    ),
                    Part::History,
                    state.options.history && !data.history.is_empty(),
                    !data.history.is_empty() && !state.completed
                ),
                import_option(
                    "Search engines",
                    "search",
                    format!(
                        "{} · keep your current default",
                        count_label(
                            data.engines.len(),
                            "compatible engine",
                            "compatible engines"
                        )
                    ),
                    Part::Search,
                    state.options.search && !data.engines.is_empty(),
                    !data.engines.is_empty() && !state.completed
                ),
            ]
            .spacing(4);
            body = body.push(section("Choose what to import", categories));
            if !data.notes.is_empty() {
                let mut notes = column![].spacing(6);
                for note in &data.notes {
                    notes = notes.push(text(note.clone()).size(12).color(p.text_muted));
                }
                body = body.push(notes);
            }
            if state.completed {
                body = body.push(
                    row![
                        action("Bookmarks", "bookmark", Command::Panel(Panel::Bookmarks)),
                        action("History", "history", Command::Panel(Panel::History))
                    ]
                    .spacing(8),
                );
            } else {
                body = body.push(import_button(
                    "Import selected data",
                    "download",
                    ImportCommand::Apply,
                    state.options.any(data),
                ));
            }
        } else {
            body = body.push(import_button(
                "Review available data",
                "arrow-right",
                ImportCommand::Read,
                state.selected.is_some(),
            ));
        }
        if !state.notice.is_empty() {
            body = body.push(
                text(state.notice.clone())
                    .size(13)
                    .color(if state.completed {
                        p.text
                    } else {
                        p.text_muted
                    }),
            );
        }
        if state.completed {
            body = body.push(action(
                "Review again",
                "refresh-cw",
                Command::Import(ImportCommand::Read),
            ));
        }
        body=body.push(column![
            text("Adds selected website data without replacing existing data or your default search engine.").size(12).color(p.text_muted),
            text("Passwords, cookies, tabs, autofill, other browser settings are not imported.").size(12).color(p.text_muted),
        ].spacing(6));
        self.page_document(
            "Import browser data",
            "download",
            "Bring bookmarks, history and search engines from another browser",
            body.into(),
        )
    }
}
fn count_label(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

fn import_button(
    label: &str,
    icon_name: &str,
    command: ImportCommand,
    enabled: bool,
) -> Element<'static, Message> {
    let kind = if matches!(command, ImportCommand::Read | ImportCommand::Apply) {
        enzui_widgets::button::ButtonKind::Accent
    } else {
        enzui_widgets::button::ButtonKind::Quiet
    };
    enzui_widgets::button::enz_button(label.to_owned(), c(Command::Import(command)))
        .icon(icon_name)
        .kind(kind)
        .enabled(enabled)
        .into_element()
}
fn import_option(
    label: &str,
    symbol: &'static str,
    subtitle: String,
    part: Part,
    value: bool,
    enabled: bool,
) -> Element<'static, Message> {
    enzui_widgets::preferences_row::enz_preferences_row(label.to_owned())
        .icon(symbol)
        .subtitle(subtitle)
        .accessory_width(52.)
        .label_min_width(120.)
        .switch(value, move |value| {
            c(Command::Import(ImportCommand::Toggle(part, value)))
        })
        .enabled(enabled)
        .into_element()
}
fn merge(saved: &mut Saved, data: &Preview, options: Options) -> (usize, usize, usize) {
    let (mut bookmarks, mut engines, mut duplicates) = (0, 0, 0);
    if options.bookmarks {
        let mut existing = saved
            .bookmarks
            .iter()
            .filter_map(|b| crate::imports::web_url(&b.url))
            .collect::<std::collections::BTreeSet<_>>();
        for bookmark in &data.bookmarks {
            if !existing.insert(bookmark.url.clone()) {
                duplicates += 1;
                continue;
            }
            saved.bookmarks.push(TabInfo {
                key: next_tab_key(),
                favicon: None,
                url: bookmark.url.clone(),
                title: bookmark.title.clone(),
                pinned: false,
                group: None,
                container: None,
            });
            let mut folders = vec![format!("Imported from {}", data.source.label())];
            folders.extend(bookmark.folders.clone());
            saved.bookmark_folders.insert(bookmark.url.clone(), folders);
            bookmarks += 1;
        }
    }
    if options.search {
        let selected_search = saved.search.selected.clone();
        for engine in &data.engines {
            if saved
                .search
                .engines()
                .iter()
                .any(|e| e.name.eq_ignore_ascii_case(&engine.name) || e.template == engine.template)
            {
                duplicates += 1;
                continue;
            }
            if saved
                .search
                .add_custom(&engine.name, &engine.template)
                .is_ok()
            {
                engines += 1;
            }
        }
        saved.search.selected = selected_search;
    }
    (bookmarks, engines, duplicates)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn data() -> Arc<Preview> {
        Arc::new(Preview {
            source: Source {
                browser: "Firefox".into(),
                name: "Work".into(),
                path: "/unused".into(),
                family: crate::imports::Family::Firefox,
            },
            bookmarks: vec![
                crate::imports::Bookmark {
                    url: "https://example.org/".into(),
                    title: "Changed title".into(),
                    folders: vec!["Other".into()],
                },
                crate::imports::Bookmark {
                    url: "https://example.org/new".into(),
                    title: "New bookmark".into(),
                    folders: vec!["Research".into()],
                },
            ],
            history: Vec::new(),
            engines: vec![crate::search::Engine {
                id: "unused".into(),
                name: "Imported search".into(),
                template: "https://search.example/?q={query}".into(),
            }],
            notes: Vec::new(),
        })
    }
    #[test]
    fn import_selection_merges_once_without_replacing_titles_folders_or_search_default() {
        let mut saved = Saved::default();
        saved.bookmarks.push(TabInfo {
            key: 1,
            favicon: None,
            url: "https://example.org/".into(),
            title: "Existing title".into(),
            pinned: false,
            group: None,
            container: None,
        });
        saved.bookmark_folders.insert(
            "https://example.org/".into(),
            vec!["Existing folder".into()],
        );
        let data = data();
        saved.search.selected = "google".into();
        let old = saved.search.selected.clone();
        assert_eq!(
            merge(
                &mut saved,
                &data,
                Options {
                    bookmarks: false,
                    history: true,
                    search: false
                }
            ),
            (0, 0, 0)
        );
        assert_eq!(saved.bookmarks.len(), 1);
        assert!(saved.search.custom.is_empty());
        let options = Options {
            bookmarks: true,
            history: false,
            search: true,
        };
        assert_eq!(merge(&mut saved, &data, options), (1, 1, 1));
        assert_eq!(saved.bookmarks[0].title, "Existing title");
        assert_eq!(
            saved.bookmark_folders["https://example.org/"],
            ["Existing folder"]
        );
        assert_eq!(
            saved.bookmark_folders["https://example.org/new"],
            ["Imported from Firefox · Work", "Research"]
        );
        assert_eq!(saved.search.selected, old);
        assert_eq!(merge(&mut saved, &data, options), (0, 0, 3));
        let restored: Saved =
            serde_json::from_slice(&serde_json::to_vec(&saved).expect("serialize"))
                .expect("restore");
        assert_eq!(restored.bookmark_folders, saved.bookmark_folders);
    }
    #[test]
    fn import_rejects_private_operations_and_stale_worker_replies() {
        let mut app = App::new(enzui_theme::EnzTheme::dark());
        let data = data();
        app.browser.importing.generation = 7;
        app.browser.importing.busy = true;
        let _ = app.import_command(ImportCommand::Prepared(6, Ok(Arc::clone(&data))));
        assert!(app.browser.importing.review.is_none());
        assert!(app.browser.importing.busy);
        let _ = app.import_command(ImportCommand::Prepared(7, Ok(Arc::clone(&data))));
        assert!(app.browser.importing.review.is_some());
        assert!(!app.browser.importing.busy);
        let _ = app.import_command(ImportCommand::Cancel);
        let _ = app.import_command(ImportCommand::Applied(
            7,
            Arc::clone(&data),
            Options::default(),
            Ok(1),
        ));
        assert!(app.browser.saved.bookmarks.is_empty());
        assert!(!app.browser.importing.completed);
        app.browser.private = true;
        let generation = app.browser.importing.generation;
        let _ = app.import_command(ImportCommand::Prepared(generation, Ok(Arc::clone(&data))));
        assert!(app.browser.importing.review.is_none());
        app.browser.importing.review = Some(Arc::clone(&data));
        let _ = app.import_command(ImportCommand::Apply);
        assert!(!app.browser.importing.applying);
        let _ = app.import_command(ImportCommand::Applied(
            generation,
            data,
            Options::default(),
            Ok(1),
        ));
        assert!(app.browser.saved.bookmarks.is_empty());
        assert_eq!(app.browser.importing.generation, generation);
    }
}
