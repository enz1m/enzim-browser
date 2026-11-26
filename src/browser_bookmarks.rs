//! Keeping bookmarks in order: a name and a folder for each, and a file of
//! them all that other browsers read.

use super::*;
use iced::widget::{Space, column, row, text};
use std::collections::BTreeMap;

/// The chooser's tag for the file bookmarks are exported to.
pub(super) const EXPORT_TAG: u64 = u64::MAX - 7;

#[derive(Debug, Clone)]
pub(crate) enum BookmarkCommand {
    /// Open the editor on the bookmark of this address.
    Edit(String),
    Title(String),
    Folder(String),
    Save,
    /// Ask where to write the bookmarks file.
    Export,
    Exported(Result<std::path::PathBuf, String>),
    /// Remove the bookmark being edited.
    Remove,
    /// Show or hide the bookmarks bar.
    Bar,
    /// Open the list of a folder on the bar, or close the one open.
    BarFolder(Option<String>),
    /// Open a bookmark chosen from a list of the bar.
    BarVisit(String),
}

/// A bookmark being edited: which one, and what is typed for it.
#[derive(Debug, Clone, Default)]
pub(crate) struct BookmarkEdit {
    url: String,
    title: String,
    folder: String,
}

/// The folder a person typed, as a path: the names between slashes. At most
/// eight deep and eighty characters a name, which is more than a list of
/// bookmarks is read with.
pub(crate) fn folder_path(typed: &str) -> Vec<String> {
    typed
        .split('/')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .take(8)
        .map(|name| name.chars().take(80).collect())
        .collect()
}

fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The bookmarks under one folder, in the order they were added.
#[derive(Default)]
struct Shelf<'a> {
    bookmarks: Vec<&'a TabInfo>,
    folders: Vec<(String, Shelf<'a>)>,
}

impl<'a> Shelf<'a> {
    fn place(&mut self, path: &[String], bookmark: &'a TabInfo) {
        let Some((name, rest)) = path.split_first() else {
            self.bookmarks.push(bookmark);
            return;
        };
        let index = match self.folders.iter().position(|(folder, _)| folder == name) {
            Some(index) => index,
            None => {
                self.folders.push((name.clone(), Shelf::default()));
                self.folders.len() - 1
            }
        };
        self.folders[index].1.place(rest, bookmark);
    }

    fn write(&self, depth: usize, out: &mut String) {
        use std::fmt::Write;
        let indent = "    ".repeat(depth);
        // Writing into a string cannot fail.
        for bookmark in &self.bookmarks {
            let _ = writeln!(
                out,
                "{indent}<DT><A HREF=\"{}\">{}</A>",
                escaped(&bookmark.url),
                escaped(&bookmark.title)
            );
        }
        for (name, shelf) in &self.folders {
            let _ = writeln!(out, "{indent}<DT><H3>{}</H3>\n{indent}<DL><p>", escaped(name));
            shelf.write(depth + 1, out);
            let _ = writeln!(out, "{indent}</DL><p>");
        }
    }
}

/// The bookmarks as the bookmark file every browser imports (the Netscape
/// format), folders within folders.
pub(crate) fn bookmarks_html(bookmarks: &[TabInfo], folders: &BTreeMap<String, Vec<String>>) -> String {
    let mut shelf = Shelf::default();
    for bookmark in bookmarks {
        let path = folders.get(&bookmark.url).map_or(&[][..], Vec::as_slice);
        shelf.place(path, bookmark);
    }
    let mut out = String::from(
        "<!DOCTYPE NETSCAPE-Bookmark-file-1>\n<META HTTP-EQUIV=\"Content-Type\" CONTENT=\"text/html; charset=UTF-8\">\n<TITLE>Bookmarks</TITLE>\n<H1>Bookmarks</H1>\n<DL><p>\n",
    );
    shelf.write(1, &mut out);
    out.push_str("</DL><p>\n");
    out
}

impl App {
    pub(super) fn bookmark_command(&mut self, command: BookmarkCommand) -> Task<Message> {
        use BookmarkCommand::*;
        match command {
            Edit(url) => {
                let Some(bookmark) = self.browser.saved.bookmarks.iter().find(|b| b.url == url)
                else {
                    return Task::none();
                };
                self.browser.bookmark_edit = Some(BookmarkEdit {
                    title: bookmark.title.clone(),
                    folder: self
                        .browser
                        .saved
                        .bookmark_folders
                        .get(&url)
                        .map(|path| path.join(" / "))
                        .unwrap_or_default(),
                    url,
                });
                self.browser.panel = Panel::BookmarkEdit;
                return focus("bookmark-title");
            }
            Title(title) => {
                if let Some(edit) = &mut self.browser.bookmark_edit {
                    edit.title = title;
                }
            }
            Folder(folder) => {
                if let Some(edit) = &mut self.browser.bookmark_edit {
                    edit.folder = folder;
                }
            }
            Save => {
                let Some(edit) = self.browser.bookmark_edit.take() else {
                    return Task::none();
                };
                self.browser.panel = Panel::None;
                let Some(bookmark) = self
                    .browser
                    .saved
                    .bookmarks
                    .iter_mut()
                    .find(|b| b.url == edit.url)
                else {
                    return Task::none();
                };
                let title = edit.title.trim();
                if !title.is_empty() {
                    bookmark.title = title.chars().take(300).collect();
                }
                let path = folder_path(&edit.folder);
                if path.is_empty() {
                    self.browser.saved.bookmark_folders.remove(&edit.url);
                } else {
                    self.browser.saved.bookmark_folders.insert(edit.url, path);
                }
                if !cfg!(test) {
                    self.browser.save();
                }
            }
            Export => {
                let folder = std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
                if let Err(error) = self.browser.chooser.start(
                    (EXPORT_TAG, enzui_webkit::page::PageRequestId::new(0)),
                    enzui_core::portal::backend::PortalOperation::SaveFile,
                    enzui_core::portal::chooser::ChooserOptions::new()
                        .title("Export bookmarks")
                        .current_folder(folder)
                        .current_name("bookmarks.html"),
                ) {
                    self.browser
                        .toasts
                        .error("bookmarks", format!("Could not open chooser: {error:?}"));
                }
            }
            Exported(Ok(path)) => self
                .browser
                .toasts
                .done(format!("Bookmarks exported to {}", path.display())),
            Exported(Err(error)) => self.browser.toasts.error("bookmarks", error),
            Remove => {
                let Some(edit) = self.browser.bookmark_edit.take() else {
                    return Task::none();
                };
                self.browser.panel = Panel::None;
                return self.browser_command(Command::RemoveBookmark(edit.url));
            }
            command @ (Bar | BarFolder(_) | BarVisit(_)) => {
                return self.bookmark_bar_command(command);
            }
        }
        Task::none()
    }

    pub(super) fn bookmarks_export_chosen(&mut self, paths: Vec<std::path::PathBuf>) -> Task<Message> {
        let Some(path) = paths.into_iter().next() else {
            return Task::none();
        };
        let html = bookmarks_html(
            &self.browser.saved.bookmarks,
            &self.browser.saved.bookmark_folders,
        );
        Task::perform(
            async move {
                std::fs::write(&path, html)
                    .map(|()| path)
                    .map_err(|error| format!("Could not write the bookmarks: {error}"))
            },
            |result| c(Command::Bookmarks(BookmarkCommand::Exported(result))),
        )
    }

    pub(super) fn bookmark_edit_view(&self) -> Option<Element<'_, Message>> {
        let edit = self.browser.bookmark_edit.as_ref()?;
        let command = |command| c(Command::Bookmarks(command));
        Some(
            column![
                heading("bookmark", "Edit bookmark", site_name(&edit.url)),
                text("Name").size(12),
                field(
                    "Name",
                    &edit.title,
                    move |title| command(BookmarkCommand::Title(title)),
                    "bookmark-title",
                ),
                text("Folder").size(12),
                field(
                    "None; for one inside another: Work / Reading",
                    &edit.folder,
                    move |folder| command(BookmarkCommand::Folder(folder)),
                    "bookmark-folder",
                ),
                row![
                    action("Remove", "trash-2", Command::Bookmarks(BookmarkCommand::Remove)),
                    Space::new().width(Length::Fill),
                    action("Cancel", "x", Command::Dismiss),
                    primary("Save", "check", Command::Bookmarks(BookmarkCommand::Save)),
                ]
                .spacing(8),
            ]
            .spacing(8)
            .into(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bookmark(url: &str, title: &str) -> TabInfo {
        TabInfo {
            key: next_tab_key(),
            favicon: None,
            page_copy: None,
            url: url.into(),
            title: title.into(),
            pinned: false,
            group: None,
            container: None,
        }
    }

    #[test]
    fn a_typed_folder_is_a_path_of_trimmed_names() {
        assert_eq!(folder_path(" Work /  Reading / "), ["Work", "Reading"]);
        assert_eq!(folder_path("  "), Vec::<String>::new());
        assert_eq!(folder_path("a/b/c/d/e/f/g/h/i/j").len(), 8);
        assert_eq!(folder_path(&"x".repeat(200))[0].chars().count(), 80);
    }

    #[test]
    fn the_exported_file_nests_folders_and_escapes_what_pages_named() {
        let bookmarks = [
            bookmark("https://example.org/?a=1&b=2", "Tom & \"Jerry\" <b>"),
            bookmark("https://docs.example/", "Docs"),
            bookmark("https://deep.example/", "Deep"),
            bookmark("https://docs.example/api", "API"),
        ];
        let folders = BTreeMap::from([
            ("https://docs.example/".to_owned(), vec!["Work".to_owned()]),
            (
                "https://deep.example/".to_owned(),
                vec!["Work".to_owned(), "Later".to_owned()],
            ),
            ("https://docs.example/api".to_owned(), vec!["Work".to_owned()]),
        ]);
        let html = bookmarks_html(&bookmarks, &folders);
        assert!(html.starts_with("<!DOCTYPE NETSCAPE-Bookmark-file-1>\n"));
        let body = html.split_once("<DL><p>\n").expect("list").1;
        assert_eq!(
            body,
            "    <DT><A HREF=\"https://example.org/?a=1&amp;b=2\">Tom &amp; &quot;Jerry&quot; &lt;b&gt;</A>\n    <DT><H3>Work</H3>\n    <DL><p>\n        <DT><A HREF=\"https://docs.example/\">Docs</A>\n        <DT><A HREF=\"https://docs.example/api\">API</A>\n        <DT><H3>Later</H3>\n        <DL><p>\n            <DT><A HREF=\"https://deep.example/\">Deep</A>\n        </DL><p>\n    </DL><p>\n</DL><p>\n"
        );
    }

    #[test]
    fn an_edited_bookmark_takes_its_name_and_folder_and_keeps_its_address() {
        let mut app = browser_tests::app();
        app.browser_mode = true;
        app.browser
            .saved
            .bookmarks
            .push(bookmark("https://docs.example/", "docs.example"));
        let edit = |command| c(Command::Bookmarks(command));
        let _ = app.update(edit(BookmarkCommand::Edit("https://docs.example/".into())));
        assert_eq!(app.browser.panel, Panel::BookmarkEdit);
        let _ = app.update(edit(BookmarkCommand::Title("  Documentation ".into())));
        let _ = app.update(edit(BookmarkCommand::Folder("Work / Reading".into())));
        // Enter saves.
        let _ = app.update(c(Command::Prompt(PromptCommand::Confirm)));
        assert_eq!(app.browser.panel, Panel::None);
        assert_eq!(app.browser.saved.bookmarks[0].title, "Documentation");
        assert_eq!(app.browser.saved.bookmarks[0].url, "https://docs.example/");
        assert_eq!(
            app.browser.saved.bookmark_folders["https://docs.example/"],
            ["Work", "Reading"]
        );
        // An emptied name is not a name, and an emptied folder is none.
        let _ = app.update(edit(BookmarkCommand::Edit("https://docs.example/".into())));
        let _ = app.update(edit(BookmarkCommand::Title(" ".into())));
        let _ = app.update(edit(BookmarkCommand::Folder(String::new())));
        let _ = app.update(edit(BookmarkCommand::Save));
        assert_eq!(app.browser.saved.bookmarks[0].title, "Documentation");
        assert!(app.browser.saved.bookmark_folders.is_empty());
    }
}
