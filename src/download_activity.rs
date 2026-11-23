use crate::{
    App,
    downloads::{Entry, Place},
};
use enzui_activity::{Job, Outcome, Progress, Publisher, State, Status, Unit, Visibility};
use enzui_core::download::{DownloadId, Phase};
use std::{collections::BTreeMap, time::Duration};

#[derive(Default)]
pub(crate) struct DownloadActivity {
    publisher: Option<Publisher>,
    window_token: Option<String>,
    jobs: BTreeMap<DownloadId, Job>,
}

impl DownloadActivity {
    fn sync(&mut self, entries: &[Entry], external_pdfs: bool) {
        let Some(publisher) = &self.publisher else {
            return;
        };
        self.jobs
            .retain(|id, _| entries.iter().any(|entry| entry.model.id == *id));
        for entry in entries {
            let id = entry.model.id;
            if !file_transfer(entry, external_pdfs) {
                self.jobs.remove(&id);
                continue;
            }
            if let Some(outcome) = outcome(entry) {
                if let Some(job) = self.jobs.remove(&id) {
                    let _ = job.finish(outcome);
                }
                continue;
            }
            if let Some(job) = self.jobs.get(&id) {
                let _ = job.update(status(entry, self.window_token.clone()));
            } else if matches!(entry.model.phase, Phase::Active | Phase::Paused)
                && let Ok(job) = publisher.start_after(
                    status(entry, self.window_token.clone()),
                    Visibility::WhenUnfocused,
                    Duration::from_millis(800),
                )
            {
                self.jobs.insert(id, job);
            }
        }
    }
}

fn file_transfer(entry: &Entry, external_pdfs: bool) -> bool {
    entry.owner.is_some() && entry.path.is_some() && (external_pdfs || !entry.is_pdf())
}

fn outcome(entry: &Entry) -> Option<Outcome> {
    match entry.model.phase {
        Phase::Complete if entry.place == Place::Folder => Some(Outcome::Succeeded),
        Phase::Failed => Some(Outcome::Failed),
        Phase::Cancelled => Some(Outcome::Cancelled),
        _ => None,
    }
}

fn status(entry: &Entry, window_token: Option<String>) -> Status {
    let state = match (&entry.model.phase, &entry.place) {
        (Phase::Paused | Phase::Queued, _) | (Phase::Complete, Place::Asking) => State::Paused,
        _ => State::Running,
    };
    let progress = entry
        .model
        .total()
        .filter(|total| *total > 0 && entry.model.received <= *total)
        .map(|total| Progress {
            done: entry.model.received,
            total,
            unit: Unit::Bytes,
        });
    Status {
        state,
        progress,
        window_token,
        ..Status::running(Phase::Active.word())
    }
}

impl App {
    pub(crate) fn sync_download_activity(&mut self) {
        let entries = &self.browser.downloads.entries;
        let external_pdfs = self.browser.saved.external_pdfs;
        let activity = &mut self.download_activity;
        if cfg!(test) || self.browser.private {
            return;
        }
        if activity.publisher.is_none() {
            if !entries.iter().any(|entry| {
                matches!(entry.model.phase, Phase::Active | Phase::Paused)
                    && file_transfer(entry, external_pdfs)
            }) {
                return;
            }
            let Ok(publisher) = Publisher::new(crate::desktop::APP_ID) else {
                return;
            };
            activity.publisher = Some(publisher);
            if let Some(window) = self.window_id
                && let Ok(token) = enzui_activity::new_window_token()
            {
                self.pending_tasks
                    .push(iced::window::set_toplevel_tag(window, token.clone()).discard());
                activity.window_token = Some(token);
            }
        }
        activity.sync(entries, external_pdfs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::downloads::Store;
    use enzui_webkit::{
        page::{PageEvent, PageRequestId},
        web::{PolicyUrl, ViewRouter},
    };

    #[test]
    fn activity_follows_native_transfers_and_keeps_file_identity_private() {
        let mut store = Store::default();
        let view = ViewRouter::new().create(1);
        let id = PageRequestId::new(1);
        let url = PolicyUrl::new("https://example.test/private-payload.zip");
        store.accept(
            view,
            &PageEvent::DownloadRequested {
                id,
                url: url.clone(),
            },
        );
        assert!(!file_transfer(&store.entries[0], true));
        store.accept(
            view,
            &PageEvent::DownloadStarted {
                id,
                url,
                suggested_name: "private-payload.zip".into(),
                path: "/private/path/private-payload.zip".into(),
            },
        );
        store.accept(
            view,
            &PageEvent::DownloadProgress {
                id,
                received: 25,
                expected: Some(100),
            },
        );
        let entry = &mut store.entries[0];
        assert!(file_transfer(entry, false));
        assert!(entry.model.pause());
        let status = status(entry, Some("browser-origin".into()));
        assert_eq!(status.state, State::Paused);
        assert_eq!(
            status.progress,
            Some(Progress {
                done: 25,
                total: 100,
                unit: Unit::Bytes
            })
        );
        assert_eq!(status.window_token.as_deref(), Some("browser-origin"));
        let payload = serde_json::to_string(&status).expect("serialize activity");
        assert!(!payload.contains("private"));
        assert!(!payload.contains("example.test"));

        entry.path = Some("/private/viewed.pdf".into());
        assert!(!file_transfer(entry, false));
        assert!(file_transfer(entry, true));

        entry.model.finish(entry.model.attempt, Phase::Complete);
        entry.place = Place::Moving;
        assert_eq!(outcome(entry), None);
        entry.place = Place::Folder;
        assert_eq!(outcome(entry), Some(Outcome::Succeeded));
        store.completed("/private/from-history.zip".into(), 100, String::new());
        assert!(!file_transfer(&store.entries[1], true));
    }
}
