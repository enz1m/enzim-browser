//! The menu of a right-click in a page. It holds what can be done with the
//! thing under the pointer and nothing else: a link's entries on a link, an
//! image's on an image, the editing entries in a text field, copying and
//! searching on selected text. What concerns the page as a whole is offered
//! only where the click was on nothing in particular. Going back, forward
//! and reloading have their buttons, and finding in the page its place in
//! the browser's menu.

use super::*;
use enzui_webkit::page::ContextTarget;
use enzui_widgets::context_menu_host::ContextMenuItem as I;

fn action(command: ActionCommand) -> Command {
    Command::Action(command)
}

/// The groups of entries for `target`, most particular first. A private
/// window (`private`) does not write to the person's word list.
fn groups(target: &ContextTarget, private: bool) -> Vec<Vec<I<Command>>> {
    let mut groups = Vec::new();
    if let Some(guesses) = &target.spelling {
        groups.push(
            spelling::menu_entries(guesses, !private)
                .into_iter()
                .map(|(label, fix)| match fix {
                    Some(fix) => {
                        I::command(Command::Spelling(spelling::SpellingCommand::Fix(fix)), label)
                    }
                    None => I::command(Command::Dismiss, label).enabled(false),
                })
                .collect(),
        );
    }
    if let Some(link) = &target.link {
        let url = link.as_str().to_owned();
        groups.push(vec![
            I::command(Command::Visit(url.clone()), "Open link in new tab"),
            I::command(
                action(ActionCommand::OpenWindow(url.clone(), false)),
                "Open link in new window",
            ),
            I::command(
                action(ActionCommand::OpenWindow(url.clone(), true)),
                "Open link in private window",
            ),
            I::command(action(ActionCommand::Save(url.clone())), "Save link as file"),
            I::command(Command::CopyText(url), "Copy link address"),
        ]);
    }
    for (source, name) in [(&target.image, "image"), (&target.media, "media")] {
        let Some(source) = source else { continue };
        let url = source.as_str().to_owned();
        groups.push(vec![
            I::command(Command::Visit(url.clone()), format!("Open {name} in new tab")),
            I::command(action(ActionCommand::Save(url.clone())), format!("Save {name}")),
            I::command(Command::CopyText(url), format!("Copy {name} address")),
        ]);
    }
    if target.editable {
        let mut editing = Vec::new();
        if target.selection {
            editing.push(I::command(action(ActionCommand::Cut), "Cut"));
            editing.push(I::command(Command::CopySelection, "Copy"));
        }
        editing.push(I::command(Command::Paste, "Paste"));
        editing.push(I::command(Command::Edit("selectAll"), "Select all"));
        groups.push(editing);
        groups.push(vec![
            I::command(Command::Edit("undo"), "Undo"),
            I::command(Command::Edit("redo"), "Redo"),
        ]);
    } else if target.selection {
        groups.push(vec![
            I::command(Command::CopySelection, "Copy"),
            I::command(action(ActionCommand::SearchSelection), "Search for selection"),
        ]);
    }
    if groups.is_empty() {
        groups.push(vec![
            I::command(Command::Bookmark, "Bookmark page"),
            I::command(
                Command::Export(crate::browser::export::ExportCommand::Open),
                "Save page as…",
            ),
            I::command(Command::Print, "Print…"),
        ]);
        groups.push(vec![
            I::command(action(ActionCommand::Translate), "Translate page"),
            I::command(action(ActionCommand::Reader), "Open reader view"),
        ]);
        groups.push(vec![
            I::command(action(ActionCommand::ViewSource), "View page source"),
            I::command(Command::Inspect(InspectCommand::Pick), "Inspect an element…"),
        ]);
    }
    groups
}

/// The menu for `target`: its groups with a divider between them.
pub(crate) fn page_menu(target: &ContextTarget, private: bool) -> Vec<I<Command>> {
    let mut items = Vec::new();
    for group in groups(target, private) {
        if !items.is_empty() {
            items.push(I::separator());
        }
        items.extend(group);
    }
    items
}

impl App {
    /// Whether `point` (in the window) is on a website on show: not on the
    /// bars, and not on a page of the browser's own, a PDF or an error. A
    /// website answers a right-click itself, with the menu above or with
    /// one of the site's own making.
    pub(crate) fn over_website(&self, point: iced::Point) -> bool {
        matches!(
            self.browser.input.bounds,
            Some((crate::browser_input::Surface::Web(_), bounds)) if bounds.contains(point)
        ) && self.browser.library.is_none()
            && self.browser.pdf.is_none()
            && self.webview.borrow().native_error.is_none()
            && InternalPage::from_url(&self.browser.saved.tabs[self.browser.saved.active].url)
                .is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use enzui_webkit::web::PolicyUrl;

    #[test]
    fn the_browsers_own_menu_keeps_off_a_website() {
        let mut app = browser_tests::app();
        app.browser_mode = true;
        app.browser.saved.tabs[0].url = "https://video.example/watch".into();
        let page = iced::Rectangle {
            x: 240.0,
            y: 60.0,
            width: 900.0,
            height: 700.0,
        };
        app.browser.input.bounds = Some((
            crate::browser_input::Surface::Web(enzui_webkit::web::ViewId::new(1)),
            page,
        ));
        let (in_page, on_bar) = (iced::Point::new(600.0, 400.0), iced::Point::new(100.0, 400.0));
        assert!(app.over_website(in_page));
        assert!(!app.over_website(on_bar));
        // A right-click in the page opens nothing of the browser's: the
        // page's menu comes with the engine's word, or the site shows its own.
        let _ = app.update(c(Command::ChromeContext(in_page)));
        assert!(!app.browser.context.is_open());
        let _ = app.update(c(Command::ChromeContext(on_bar)));
        assert!(app.browser.context.is_open());
        // The browser's own pages have no menu but the browser's.
        app.browser.saved.tabs[0].url = "enzim://settings".into();
        assert!(!app.over_website(in_page));
    }

    /// The menu as its labels, a divider as `-`.
    fn labels(target: &ContextTarget) -> Vec<String> {
        page_menu(target, false)
            .into_iter()
            .map(|item| match item {
                I::Command { label, .. } | I::Submenu { label, .. } => label,
                I::Separator => "-".to_owned(),
            })
            .collect()
    }

    #[test]
    fn a_click_on_nothing_in_particular_offers_what_concerns_the_page() {
        let menu = labels(&ContextTarget::default());
        assert_eq!(
            menu,
            [
                "Bookmark page",
                "Save page as…",
                "Print…",
                "-",
                "Translate page",
                "Open reader view",
                "-",
                "View page source",
                "Inspect an element…",
            ]
        );
        // The toolbar and the browser's menu have these.
        for gone in ["Back", "Forward", "Reload", "Find in page"] {
            assert!(!menu.iter().any(|label| label == gone), "{gone}");
        }
    }

    #[test]
    fn selected_text_an_image_and_a_link_each_get_their_own_entries_only() {
        let selected = ContextTarget {
            selection: true,
            ..ContextTarget::default()
        };
        assert_eq!(labels(&selected), ["Copy", "Search for selection"]);

        let image = ContextTarget {
            image: Some(PolicyUrl::new("https://example.org/a.png")),
            ..ContextTarget::default()
        };
        assert_eq!(
            labels(&image),
            ["Open image in new tab", "Save image", "Copy image address"]
        );

        // An image that is a link: the link's entries, then the image's.
        let linked = ContextTarget {
            link: Some(PolicyUrl::new("https://example.org/")),
            ..image
        };
        let menu = labels(&linked);
        assert_eq!(menu.len(), 9);
        assert_eq!(menu[0], "Open link in new tab");
        assert_eq!(menu[5], "-");
        assert_eq!(menu[6], "Open image in new tab");
        assert!(!menu.iter().any(|label| label == "Bookmark page"));
    }

    #[test]
    fn a_text_field_offers_editing_and_a_misspelled_word_its_corrections_first() {
        let field = ContextTarget {
            editable: true,
            ..ContextTarget::default()
        };
        assert_eq!(labels(&field), ["Paste", "Select all", "-", "Undo", "Redo"]);

        let word = ContextTarget {
            editable: true,
            selection: true,
            spelling: Some(vec!["hello".to_owned()]),
            ..ContextTarget::default()
        };
        assert_eq!(
            labels(&word),
            [
                "hello",
                "Add to dictionary",
                "Ignore spelling",
                "-",
                "Cut",
                "Copy",
                "Paste",
                "Select all",
                "-",
                "Undo",
                "Redo",
            ]
        );
        // A private window leaves no word behind.
        let private: Vec<_> = page_menu(&word, true)
            .into_iter()
            .filter_map(|item| match item {
                I::Command { label, .. } => Some(label),
                _ => None,
            })
            .collect();
        assert!(!private.iter().any(|label| label == "Add to dictionary"));
    }
}
