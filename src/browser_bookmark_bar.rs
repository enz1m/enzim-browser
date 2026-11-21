//! The bookmarks bar: the bookmarks kept in no folder and the outermost
//! folders, in a row under the toolbar.

use super::*;
use iced::widget::{Space, column, container, row, text};
use std::collections::BTreeMap;

/// The name the list of what did not fit is open under; no folder is named
/// so, because a folder's name is trimmed text.
pub(crate) const MORE: &str = "\0more";

const GAP: f32 = 2.0;
/// The button that opens what did not fit.
const MORE_WIDTH: f32 = 30.0;
/// The most of a name a button on the bar shows.
const NAME_CHARS: usize = 22;

/// One button on the bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BarItem {
    Page { url: String, title: String },
    Folder(String),
}

/// What stands on the bar, in the order the bookmarks were added: a folder
/// stands where its first bookmark does.
pub(crate) fn bar_items(bookmarks: &[TabInfo], folders: &BTreeMap<String, Vec<String>>) -> Vec<BarItem> {
    let mut items = Vec::new();
    for bookmark in bookmarks {
        match folders.get(&bookmark.url).and_then(|path| path.first()) {
            Some(folder) => {
                let folder = BarItem::Folder(folder.clone());
                if !items.contains(&folder) {
                    items.push(folder);
                }
            }
            None => items.push(BarItem::Page {
                url: bookmark.url.clone(),
                title: bookmark.title.clone(),
            }),
        }
    }
    items
}

/// The bookmarks of one outermost folder with the names they are listed
/// under: those deeper in it carry the folders between.
pub(crate) fn folder_rows(
    bookmarks: &[TabInfo],
    folders: &BTreeMap<String, Vec<String>>,
    folder: &str,
) -> Vec<(String, String)> {
    bookmarks
        .iter()
        .filter_map(|bookmark| {
            let path = folders.get(&bookmark.url)?;
            let (first, rest) = path.split_first()?;
            (first == folder).then(|| {
                let mut name = rest.join(" / ");
                if !name.is_empty() {
                    name.push_str(" / ");
                }
                name.push_str(&bookmark.title);
                (bookmark.url.clone(), name)
            })
        })
        .collect()
}

fn shown_name(name: &str) -> String {
    if name.chars().count() <= NAME_CHARS {
        name.to_owned()
    } else {
        let mut short: String = name.chars().take(NAME_CHARS - 1).collect();
        short.push('…');
        short
    }
}

/// How wide a button is drawn for a name: its icon, the name at the bar's
/// text size, and the padding around them.
pub(crate) fn item_width(name: &str) -> f32 {
    40.0 + 7.6 * shown_name(name).chars().count() as f32
}

/// How many of the buttons fit in `room`. Where not all do, room is kept for
/// the button that lists the rest.
pub(crate) fn bar_fit(widths: &[f32], room: f32) -> usize {
    let all: f32 = widths.iter().sum::<f32>() + GAP * widths.len().saturating_sub(1) as f32;
    if all <= room {
        return widths.len();
    }
    let mut used = MORE_WIDTH;
    let mut fit = 0;
    for width in widths {
        if used + GAP + width > room {
            break;
        }
        used += GAP + width;
        fit += 1;
    }
    fit
}

/// A button of the bar or of one of its lists: no ground of its own, a
/// rounded one under the pointer as a tab has, and a ring for the keyboard.
fn bar_button(theme: &iced::Theme, status: enzui_widgets::focus_button::FocusStatus) -> iced::widget::button::Style {
    use enzui_widgets::focus_button::FocusStatus;
    let p = palette(theme);
    iced::widget::button::Style {
        background: matches!(status, FocusStatus::Hovered | FocusStatus::Pressed)
            .then(|| p.surface_hover.mix(p.text, 0.07).into()),
        text_color: p.text,
        border: iced::Border {
            radius: 7.0.into(),
            width: if status == FocusStatus::Focused { 1.5 } else { 0.0 },
            color: p.accent,
        },
        ..Default::default()
    }
}

impl App {
    pub(super) fn bookmark_bar_command(&mut self, command: BookmarkCommand) -> Task<Message> {
        match command {
            BookmarkCommand::Bar => {
                self.browser.saved.bookmarks_bar = !self.browser.saved.bookmarks_bar;
                self.browser.bar_open = None;
                if !cfg!(test) {
                    self.browser.save();
                }
            }
            BookmarkCommand::BarFolder(folder) => {
                self.browser.bar_open = if self.browser.bar_open == folder {
                    None
                } else {
                    folder
                };
            }
            BookmarkCommand::BarVisit(url) => {
                self.browser.bar_open = None;
                return self.browser_command(Command::Navigate(url));
            }
            _ => {}
        }
        self.browser.chrome_revision = self.browser.chrome_revision.wrapping_add(1);
        Task::none()
    }

    fn bar_label<'a>(icon: Element<'a, Message>, name: String) -> Element<'a, Message> {
        row![
            icon,
            text(name)
                .size(12)
                .wrapping(iced::widget::text::Wrapping::None)
                .ellipsis(iced::widget::text::Ellipsis::End)
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center)
        .into()
    }

    /// A list hanging from a button of the bar: one row a bookmark.
    fn bar_list(&self, rows: Vec<(String, String)>) -> Element<'_, Message> {
        // A row is a line of its text and the padding around it, and a
        // pixel between rows; a long list scrolls in the room the window has.
        let height = (rows.len() as f32 * 31.0 + 2.0)
            .min((self.browser.viewport.height - 140.0).clamp(120.0, 420.0));
        let mut list = column![].spacing(1);
        for (url, name) in rows {
            list = list.push(
                enzui_widgets::focus_button::focus_button(
                    container(Self::bar_label(self.site_icon(&url, 14), name)).width(Length::Fill),
                    c(Command::Bookmarks(BookmarkCommand::BarVisit(url))),
                )
                .width(Length::Fill)
                .padding([6, 8])
                .style_focus(bar_button),
            );
        }
        container(iced::widget::scrollable(list))
            .height(height)
            .into()
    }

    fn bar_popover<'a>(
        &'a self,
        name: &str,
        button: Element<'a, Message>,
        rows: impl FnOnce() -> Vec<(String, String)>,
        label: &str,
    ) -> Element<'a, Message> {
        let open = self.browser.bar_open.as_deref() == Some(name);
        enzui_widgets::popover::enz_popover(button, open.then(|| self.bar_list(rows())), || {
            c(Command::Bookmarks(BookmarkCommand::BarFolder(None)))
        })
        .width((self.browser.viewport.width - 24.0).clamp(160.0, 300.0))
        .padding(4.0)
        .label(label.to_owned())
        .into_element()
    }

    /// The bar, where the person has it shown. Its buttons open a bookmark
    /// in the tab on show; the middle button opens it in a tab behind, the
    /// right one edits it.
    pub(super) fn bookmark_bar(&self) -> Option<Element<'_, Message>> {
        if !self.browser.saved.bookmarks_bar {
            return None;
        }
        let saved = &self.browser.saved;
        let items = bar_items(&saved.bookmarks, &saved.bookmark_folders);
        let widths: Vec<f32> = items
            .iter()
            .map(|item| match item {
                BarItem::Page { title, .. } => item_width(title),
                BarItem::Folder(name) => item_width(name),
            })
            .collect();
        let fit = bar_fit(&widths, self.browser.viewport.width - 20.0);
        let command = |command| c(Command::Bookmarks(command));
        let mut bar = row![].spacing(GAP).align_y(iced::Alignment::Center);
        if items.is_empty() {
            bar = bar.push(
                text("Bookmarks you keep in no folder stand here.")
                    .size(12)
                    .style(|theme| iced::widget::text::Style {
                        color: Some(palette(theme).text_muted),
                    }),
            );
        }
        for (item, width) in items.iter().zip(&widths).take(fit) {
            bar = bar.push(match item {
                BarItem::Page { url, title } => iced::widget::mouse_area(
                    enzui_widgets::focus_button::focus_button(
                        Self::bar_label(self.site_icon(url, 14), shown_name(title)),
                        c(Command::Navigate(url.clone())),
                    )
                    .width(*width)
                    .padding([4, 8])
                    .style_focus(bar_button),
                )
                .on_middle_press(c(Command::Action(ActionCommand::VisitBackground(url.clone()))))
                .on_right_press(command(BookmarkCommand::Edit(url.clone())))
                .into(),
                BarItem::Folder(name) => self.bar_popover(
                    name,
                    enzui_widgets::focus_button::focus_button(
                        Self::bar_label(glyph("folder", 14.0), shown_name(name)),
                        command(BookmarkCommand::BarFolder(Some(name.clone()))),
                    )
                    .width(*width)
                    .padding([4, 8])
                    .style_focus(bar_button)
                    .into(),
                    || folder_rows(&saved.bookmarks, &saved.bookmark_folders, name),
                    name,
                ),
            });
        }
        if fit < items.len() {
            bar = bar.push(Space::new().width(Length::Fill));
            bar = bar.push(self.bar_popover(
                MORE,
                ink_icon(
                    "chevrons-right",
                    "More bookmarks",
                    command(BookmarkCommand::BarFolder(Some(MORE.to_owned()))),
                    26.0,
                    14.0,
                ),
                || {
                    items[fit..]
                        .iter()
                        .flat_map(|item| match item {
                            BarItem::Page { url, title } => vec![(url.clone(), title.clone())],
                            BarItem::Folder(name) => {
                                folder_rows(&saved.bookmarks, &saved.bookmark_folders, name)
                                    .into_iter()
                                    .map(|(url, row)| (url, format!("{name} / {row}")))
                                    .collect()
                            }
                        })
                        .collect()
                },
                "More bookmarks",
            ));
        }
        Some(
            container(bar)
                .padding(iced::Padding {
                    top: 0.0,
                    right: 10.0,
                    bottom: 4.0,
                    left: 10.0,
                })
                .width(Length::Fill)
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
            url: url.into(),
            title: title.into(),
            pinned: false,
            group: None,
            container: None,
        }
    }

    fn shelf() -> (Vec<TabInfo>, BTreeMap<String, Vec<String>>) {
        (
            vec![
                bookmark("https://a.example/", "A"),
                bookmark("https://docs.example/", "Docs"),
                bookmark("https://b.example/", "B"),
                bookmark("https://deep.example/", "Deep"),
                bookmark("https://play.example/", "Play"),
            ],
            BTreeMap::from([
                ("https://docs.example/".to_owned(), vec!["Work".to_owned()]),
                (
                    "https://deep.example/".to_owned(),
                    vec!["Work".to_owned(), "Later".to_owned()],
                ),
                ("https://play.example/".to_owned(), vec!["Fun".to_owned()]),
            ]),
        )
    }

    #[test]
    fn the_bar_holds_loose_bookmarks_and_each_outer_folder_once() {
        let (bookmarks, folders) = shelf();
        let page = |url: &str, title: &str| BarItem::Page {
            url: url.into(),
            title: title.into(),
        };
        assert_eq!(
            bar_items(&bookmarks, &folders),
            [
                page("https://a.example/", "A"),
                BarItem::Folder("Work".into()),
                page("https://b.example/", "B"),
                BarItem::Folder("Fun".into()),
            ]
        );
        assert_eq!(
            folder_rows(&bookmarks, &folders, "Work"),
            [
                ("https://docs.example/".to_owned(), "Docs".to_owned()),
                ("https://deep.example/".to_owned(), "Later / Deep".to_owned()),
            ]
        );
    }

    #[test]
    fn buttons_that_do_not_fit_leave_room_for_the_one_that_lists_them() {
        let widths = [100.0, 100.0, 100.0];
        // All three and the two gaps between them.
        assert_eq!(bar_fit(&widths, 304.0), 3);
        // One pixel short: the third goes, and so does the second, whose
        // place the button for the rest needs.
        assert_eq!(bar_fit(&widths, 303.0), 2);
        assert_eq!(bar_fit(&widths, 233.0), 1);
        assert_eq!(bar_fit(&widths, 100.0), 0);
        assert_eq!(bar_fit(&[], 0.0), 0);
        // A long name is cut, so no one bookmark takes the bar.
        assert_eq!(item_width(&"x".repeat(200)), item_width(&"x".repeat(NAME_CHARS)));
    }

    #[test]
    fn the_bar_is_shown_and_hidden_and_a_bookmark_opened_from_a_list_closes_it() {
        let mut app = browser_tests::app();
        app.browser_mode = true;
        let (bookmarks, folders) = shelf();
        app.browser.saved.bookmarks = bookmarks;
        app.browser.saved.bookmark_folders = folders;
        assert!(app.bookmark_bar().is_none());
        let command = |command| c(Command::Bookmarks(command));
        let _ = app.update(command(BookmarkCommand::Bar));
        assert!(app.browser.saved.bookmarks_bar);
        assert!(app.bookmark_bar().is_some());
        let _ = app.update(command(BookmarkCommand::BarFolder(Some("Work".into()))));
        assert_eq!(app.browser.bar_open.as_deref(), Some("Work"));
        // The same button again closes its list.
        let _ = app.update(command(BookmarkCommand::BarFolder(Some("Work".into()))));
        assert_eq!(app.browser.bar_open, None);
        let _ = app.update(command(BookmarkCommand::BarFolder(Some("Work".into()))));
        let _ = app.update(command(BookmarkCommand::BarVisit("https://docs.example/".into())));
        assert_eq!(app.browser.bar_open, None);
        assert_eq!(app.address, "https://docs.example/");
        let _ = app.update(command(BookmarkCommand::Bar));
        assert!(app.bookmark_bar().is_none());
    }
}
