//! Asking where a download is saved. The engine writes every download into
//! the downloads folder under a fresh name of its own making; a download the
//! person gave a place is moved there once it is whole.

use super::*;
use crate::downloads::Place;
use std::path::{Path, PathBuf};

/// The chooser's tag for the place of a download; the request id beside it
/// is the download's.
pub(super) const PLACE_TAG: u64 = u64::MAX - 8;

/// Moves a whole download to the place chosen for it. The chooser asked
/// before naming a file that is there, so one in the way is replaced.
pub(crate) fn move_download(from: &Path, to: &Path) -> Result<(), String> {
    let failed = |error: std::io::Error| format!("Could not save to {}: {error}", to.display());
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        // Another disk: copied there, then removed here.
        Err(error) if error.kind() == std::io::ErrorKind::CrossesDevices => {
            if let Err(error) = std::fs::copy(from, to) {
                // Half a file under the chosen name is worse than none.
                let _ = std::fs::remove_file(to);
                return Err(failed(error));
            }
            // What stays behind in the downloads folder is a second copy,
            // not a loss.
            let _ = std::fs::remove_file(from);
            Ok(())
        }
        Err(error) => Err(failed(error)),
    }
}

impl App {
    /// Asks about one download at a time, and sends the whole ones to the
    /// places chosen for them.
    pub(super) fn settle_download_places(&mut self) -> bool {
        let mut changed = false;
        self.browser.downloads.note_places(
            self.browser.saved.ask_download_place,
            !self.browser.saved.external_pdfs,
        );
        if self.browser.download_ask.is_none()
            && !self.browser.chooser.is_pending()
            && let Some((id, name)) = self.browser.downloads.next_ask()
        {
            match self.browser.chooser.start(
                (PLACE_TAG, enzui_webkit::page::PageRequestId::new(id)),
                enzui_core::portal::backend::PortalOperation::SaveFile,
                enzui_core::portal::chooser::ChooserOptions::new()
                    .title("Save download")
                    .current_folder(crate::downloads::folder())
                    .current_name(name),
            ) {
                Ok(()) => self.browser.download_ask = Some(id),
                // Nothing to ask with: it stays in the downloads folder.
                Err(error) => {
                    self.set_download_place(id, Place::Folder);
                    changed = true;
                    self.browser
                        .toasts
                        .error("downloads", format!("Could not open chooser: {error:?}"));
                }
            }
        }
        for (id, from, to) in self.browser.downloads.ready_to_move() {
            changed = true;
            self.pending_tasks.push(Task::perform(
                async move { move_download(&from, &to).map(|()| to) },
                move |result| c(Command::Download(DownloadCommand::Placed(id, result))),
            ));
        }
        changed
    }

    fn set_download_place(&mut self, id: u64, place: Place) {
        if let Some(entry) = self
            .browser
            .downloads
            .entries
            .iter_mut()
            .find(|entry| entry.model.id.0 == id)
        {
            entry.place = place;
        }
    }

    /// The person named a place, or closed the question, which gives the
    /// download up.
    pub(super) fn download_place_chosen(&mut self, id: u64, paths: Vec<PathBuf>) -> Task<Message> {
        if self.browser.download_ask == Some(id) {
            self.browser.download_ask = None;
        }
        self.browser.chrome_revision = self.browser.chrome_revision.wrapping_add(1);
        if let Some(place) = paths.into_iter().next() {
            self.set_download_place(id, Place::Chosen(place));
            return Task::none();
        }
        self.set_download_place(id, Place::Folder);
        let Some(entry) = self
            .browser
            .downloads
            .entries
            .iter()
            .find(|entry| entry.model.id.0 == id)
        else {
            return Task::none();
        };
        if entry.active() {
            return self.download_command(DownloadCommand::Cancel(id));
        }
        // Already whole: the file nobody wanted goes, and its line with it.
        if let Some(path) = entry.path.clone()
            && !cfg!(test)
        {
            let _ = std::fs::remove_file(path);
        }
        self.download_command(DownloadCommand::Remove(id))
    }

    pub(super) fn download_placed(&mut self, id: u64, result: Result<PathBuf, String>) {
        let Some(entry) = self
            .browser
            .downloads
            .entries
            .iter_mut()
            .find(|entry| entry.model.id.0 == id)
        else {
            return;
        };
        entry.place = Place::Folder;
        match result {
            Ok(path) => {
                entry.name = path.file_name().map_or_else(
                    || "Download".into(),
                    |name| name.to_string_lossy().into_owned(),
                );
                entry.path = Some(path);
            }
            // It is whole and still in the downloads folder: say so there.
            Err(error) => {
                entry.error.clone_from(&error);
                self.browser.toasts.error("downloads", error);
            }
        }
        if !cfg!(test) {
            self.browser.save();
        }
    }

    pub(super) fn download_place_settings(&self) -> Element<'_, Message> {
        enzui_widgets::preferences_row::enz_preferences_row("Ask where to save each download")
            .icon("folder-search")
            .subtitle("When off, downloads go to the folder above. PDFs opened in the browser are not asked about.")
            .switch(self.browser.saved.ask_download_place, |value| {
                c(Command::Preference(PreferenceCommand::AskDownloadPlace(value)))
            })
            .into_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use enzui_core::download::Phase;
    use enzui_webkit::page::{PageEvent, PageRequestId};
    use enzui_webkit::web::{PolicyUrl, ViewId};

    fn started(store: &mut crate::downloads::Store, id: u64, name: &str) -> u64 {
        store.accept(
            ViewId::new(1),
            &PageEvent::DownloadStarted {
                id: PageRequestId::new(id),
                url: PolicyUrl::new(format!("https://files.example/{name}")),
                suggested_name: name.into(),
                path: PathBuf::from("/downloads").join(name),
            },
        );
        store.entries.last().expect("entry").model.id.0
    }

    fn finish(store: &mut crate::downloads::Store, id: u64, name: &str) {
        store.accept(
            ViewId::new(1),
            &PageEvent::DownloadFinished {
                id: PageRequestId::new(id),
                path: PathBuf::from("/downloads").join(name),
            },
        );
    }

    #[test]
    fn a_download_is_asked_about_once_and_moved_when_it_is_whole() {
        let mut store = crate::downloads::Store::default();
        let first = started(&mut store, 1, "a.zip");
        // Asking is off: it stays where the engine put it, for good.
        store.note_places(false, true);
        store.note_places(true, true);
        assert_eq!(store.next_ask(), None);

        let second = started(&mut store, 2, "b.zip");
        let pdf = started(&mut store, 3, "paper.pdf");
        store.note_places(true, true);
        assert_eq!(store.next_ask(), Some((second, "b.zip".to_owned())));
        // A PDF the browser shows is not asked about; one it hands on is.
        assert_eq!(store.entries[2].model.id.0, pdf);
        assert_eq!(store.entries[2].place, Place::Folder);
        let other = started(&mut store, 4, "other.pdf");
        store.note_places(true, false);
        assert_eq!(
            store.entries.iter().find(|e| e.model.id.0 == other).expect("pdf").place,
            Place::Asking
        );

        // Chosen while it loads: nothing moves until it is whole.
        let entry = store.entries.iter_mut().find(|e| e.model.id.0 == second).expect("entry");
        entry.place = Place::Chosen("/home/me/b.zip".into());
        assert!(store.ready_to_move().is_empty());
        finish(&mut store, 2, "b.zip");
        assert!(store.ready_to_open().is_empty());
        assert_eq!(
            store.ready_to_move(),
            [(second, PathBuf::from("/downloads/b.zip"), PathBuf::from("/home/me/b.zip"))]
        );
        // Handed out once.
        assert!(store.ready_to_move().is_empty());
        let _ = first;
    }

    #[test]
    fn closing_the_question_gives_the_download_up_and_a_place_renames_it() {
        let mut app = browser_tests::app();
        app.browser_mode = true;
        app.browser.saved.ask_download_place = true;
        let kept = started(&mut app.browser.downloads, 1, "kept.zip");
        let dropped = started(&mut app.browser.downloads, 2, "dropped.zip");
        finish(&mut app.browser.downloads, 2, "dropped.zip");
        app.browser.downloads.note_places(true, true);
        app.browser.download_ask = Some(dropped);

        let _ = app.download_place_chosen(dropped, Vec::new());
        assert_eq!(app.browser.download_ask, None);
        assert!(app.browser.downloads.entries.iter().all(|e| e.model.id.0 != dropped));

        let _ = app.download_place_chosen(kept, vec!["/home/me/Kept.zip".into()]);
        finish(&mut app.browser.downloads, 1, "kept.zip");
        assert_eq!(app.browser.downloads.ready_to_move().len(), 1);
        app.download_placed(kept, Ok("/home/me/Kept.zip".into()));
        let entry = &app.browser.downloads.entries[0];
        assert_eq!(entry.name, "Kept.zip");
        assert_eq!(entry.path.as_deref(), Some(Path::new("/home/me/Kept.zip")));
        assert_eq!(entry.place, Place::Folder);
        assert_eq!(entry.model.phase, Phase::Complete);

        // A move that failed leaves the file where it was, and says so.
        let stuck = started(&mut app.browser.downloads, 3, "stuck.zip");
        app.download_placed(stuck, Err("Could not save to /mnt/x: full".into()));
        let entry = app.browser.downloads.entries.last().expect("entry");
        assert_eq!(entry.path.as_deref(), Some(Path::new("/downloads/stuck.zip")));
        assert!(entry.error.contains("Could not save"));
    }

    #[test]
    fn a_whole_download_is_moved_over_what_is_in_its_place() {
        let dir = std::env::temp_dir().join(format!("enzim-place-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let (from, to) = (dir.join("from.bin"), dir.join("to.bin"));
        std::fs::write(&from, b"new").expect("from");
        std::fs::write(&to, b"old").expect("to");
        move_download(&from, &to).expect("move");
        assert_eq!(std::fs::read(&to).expect("read"), b"new");
        assert!(!from.exists());
        // Nothing to move: an error that names the place, and no file made.
        let error = move_download(&from, &dir.join("none.bin")).expect_err("missing");
        assert!(error.contains("none.bin"), "{error}");
        assert!(!dir.join("none.bin").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
