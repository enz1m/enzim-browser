use enzui_core::download::{Download, DownloadId, Phase};
use enzui_webkit::page::{PageEvent, PageRequestId};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The folder the person chose for downloads; `None` for the default.
static FOLDER: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

pub(crate) fn set_folder(folder: Option<PathBuf>) {
    *FOLDER.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = folder;
}

/// Where downloads are saved: the folder the person chose, or
/// `Downloads` in their home folder.
pub(crate) fn folder() -> PathBuf {
    FOLDER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
        .unwrap_or_else(|| {
            if std::env::var_os("ENZIM_QA").is_some() {
                return std::env::var_os("XDG_DATA_HOME")
                    .map_or_else(
                        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("work/overnight/qa/data"),
                        PathBuf::from,
                    )
                    .join("enzim-browser/qa-downloads");
            }
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                .join("Downloads")
        })
}

/// The private folder the engine writes downloads into. A download leaves it
/// for the downloads folder once whole; a PDF shown in a tab never does.
pub(crate) fn incoming() -> PathBuf {
    crate::browser_profile::paths().cache.join("incoming-downloads")
}

/// Creates the incoming folder for the owner alone. What an earlier run left
/// there is removed once: shown PDFs and downloads cut short, which may be a
/// private window's, are not kept.
pub(crate) fn prepare_incoming(folder: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};
    static EMPTIED: std::sync::Once = std::sync::Once::new();
    EMPTIED.call_once(|| {
        for file in std::fs::read_dir(folder).into_iter().flatten().flatten() {
            let _ = std::fs::remove_file(file.path());
        }
    });
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(folder)?;
    std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o700))
}

/// A path in `folder` for `name` that names no file yet.
pub(crate) fn free_path(folder: &std::path::Path, name: &str) -> PathBuf {
    let name = std::path::Path::new(name);
    let stem = name.file_stem().map_or_else(|| "Download".into(), |stem| stem.to_string_lossy().into_owned());
    let extension = name.extension().map(|extension| format!(".{}", extension.to_string_lossy())).unwrap_or_default();
    (0..)
        .map(|n| folder.join(if n == 0 { format!("{stem}{extension}") } else { format!("{stem} ({n}){extension}") }))
        .find(|path| std::fs::symlink_metadata(path).is_err())
        .unwrap_or_else(|| folder.join(name))
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Archive {
    pub(crate) path: PathBuf,
    pub(crate) bytes: u64,
    pub(crate) source: String,
    #[serde(default)]
    pub(crate) date: i64,
}
/// Where a download is kept once it is whole.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum Place {
    /// Where it stays: where the engine wrote it until it is placed.
    #[default]
    Folder,
    /// Into the downloads folder under a free name, once whole.
    ToFolder,
    /// The person is being asked.
    Asking,
    /// Where the person said.
    Chosen(PathBuf),
    /// On its way there.
    Moving,
}
pub(crate) struct Entry {
    pub(crate) place: Place,
    /// A PDF for a tab, kept privately and removed when no tab shows it.
    pub(crate) viewed: bool,
    /// Its PDF was opened in a tab.
    pub(crate) shown: bool,
    /// The person asked to save it, rather than a page to show it.
    pub(crate) requested: bool,
    /// Whether the question of its place was settled, asked or not.
    pub(crate) placed: bool,
    pub(crate) model: Download,
    pub(crate) owner: Option<enzui_webkit::web::ViewId>,
    pub(crate) native: Option<PageRequestId>,
    pub(crate) url: String,
    pub(crate) path: Option<PathBuf>,
    pub(crate) name: String,
    pub(crate) source: String,
    pub(crate) date: i64,
    pub(crate) error: String,
    pub(crate) pending: bool,
    pub(crate) open_when_finished: bool,
    auto_opened: bool,
    pub(crate) control: u64,
    pub(crate) retrying: bool,
    pub(crate) partial: Option<enzui_webkit::page::PartialDownload>,
    pub(crate) response: Option<enzui_webkit::page::DownloadResponse>,
    pub(crate) range_decision: Option<enzui_core::download::RangeDecision>,
    pub(crate) resume_requested: bool,
    pub(crate) assembly: Option<(enzui_webkit::page::PartialDownload, Option<PathBuf>)>,
}
pub(crate) fn validator(
    response: &enzui_webkit::page::DownloadResponse,
) -> Option<enzui_core::download::Validator> {
    use enzui_core::download::Validator;
    response
        .etag
        .as_deref()
        .and_then(Validator::strong_etag)
        .or_else(|| {
            response
                .modified
                .as_deref()
                .and_then(Validator::last_modified)
        })
}
impl Entry {
    pub(crate) fn is_pdf(&self) -> bool {
        self.path.as_ref().is_some_and(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
        }) || self.name.to_ascii_lowercase().ends_with(".pdf")
    }
    pub(crate) fn network_source(&self) -> bool {
        url::Url::parse(&self.url).is_ok_and(|u| matches!(u.scheme(), "http" | "https"))
    }
    pub(crate) fn active(&self) -> bool {
        !self.model.phase.is_terminal()
    }
}
#[derive(Default)]
pub(crate) struct Store {
    pub(crate) entries: Vec<Entry>,
    next: u64,
    pub(crate) cancel_native: Vec<(enzui_webkit::web::ViewId, PageRequestId)>,
    /// Addresses the person chose to save, until their download starts.
    pub(crate) saving: Vec<String>,
}
impl Store {
    pub(crate) fn ready_to_open(&mut self) -> Vec<u64> {
        self.entries
            .iter_mut()
            .filter_map(|entry| {
                if entry.model.phase != Phase::Complete
                    || entry.path.is_none()
                    // Not before it is where it stays.
                    || entry.place != Place::Folder
                    || entry.auto_opened
                    || (!entry.open_when_finished && !entry.viewed)
                {
                    return None;
                }
                entry.auto_opened = true;
                entry.open_when_finished = false;
                Some(entry.model.id.0)
            })
            .collect()
    }
    pub(crate) fn from_archive(archive: &[Archive]) -> Self {
        let mut store = Self::default();
        for saved in archive.iter().rev().take(100).rev() {
            store.completed(saved.path.clone(), saved.bytes, saved.source.clone());
            if let Some(entry) = store.entries.last_mut() {
                entry.date = saved.date;
            }
        }
        store
    }
    pub(crate) fn archive(&self) -> Vec<Archive> {
        self.entries
            .iter()
            .filter(|e| e.model.phase == Phase::Complete && !e.viewed)
            .filter_map(|e| {
                Some(Archive {
                    path: e.path.clone()?,
                    bytes: e.model.received,
                    source: e.source.clone(),
                    date: e.date,
                })
            })
            .collect()
    }
    pub(crate) fn completed(&mut self, path: PathBuf, bytes: u64, source: String) {
        let mut entry = self.fresh(None, String::new());
        entry.name = file_name(&path);
        entry.path = Some(path);
        entry.source = source;
        entry.model.progress(entry.model.attempt, bytes);
        entry.model.set_total(entry.model.attempt, Some(bytes));
        entry.model.finish(entry.model.attempt, Phase::Complete);
        entry.auto_opened = true;
        self.entries.push(entry);
        self.trim();
    }
    fn claim_saving(&mut self, url: &str) -> bool {
        let at = self.saving.iter().position(|saving| saving == url);
        at.map(|at| self.saving.remove(at)).is_some()
    }
    fn fresh(&mut self, owner: Option<enzui_webkit::web::ViewId>, url: String) -> Entry {
        self.next = self.next.wrapping_add(1).max(1);
        Entry {
            place: Place::Folder,
            viewed: false,
            shown: false,
            requested: false,
            placed: false,
            model: Download::queued(DownloadId(self.next)),
            owner,
            native: None,
            source: source_label(&url),
            date: chrono::Utc::now().timestamp(),
            url,
            path: None,
            name: "Download".into(),
            error: String::new(),
            pending: false,
            open_when_finished: false,
            auto_opened: false,
            control: 0,
            retrying: false,
            partial: None,
            response: None,
            range_decision: None,
            resume_requested: false,
            assembly: None,
        }
    }
    /// Settles, once for each download that has a file, whether the person
    /// is asked where it goes. A download that failed is not worth asking
    /// about, and a PDF shown in the browser (`shown_pdfs`) is read, not
    /// kept.
    pub(crate) fn note_places(&mut self, ask: bool, shown_pdfs: bool) {
        for entry in &mut self.entries {
            if entry.placed || entry.owner.is_none() || entry.path.is_none() {
                continue;
            }
            entry.placed = true;
            if !entry.active() && entry.model.phase != Phase::Complete {
                continue;
            }
            if shown_pdfs && entry.is_pdf() && !entry.requested {
                entry.viewed = true;
            } else if ask {
                entry.place = Place::Asking;
            } else {
                entry.place = Place::ToFolder;
            }
        }
    }
    /// Takes the shown PDFs no tab shows any more, for their files to be removed.
    pub(crate) fn unviewed(&mut self, showing: impl Fn(&std::path::Path) -> bool) -> Vec<PathBuf> {
        let mut gone = Vec::new();
        self.entries.retain(|entry| {
            let unused = entry.viewed && entry.shown && entry.path.as_deref().is_some_and(|path| !showing(path));
            if unused {
                gone.extend(entry.path.clone());
            }
            !unused
        });
        gone
    }
    /// The download the person is asked about next, and the name it has.
    pub(crate) fn next_ask(&self) -> Option<(u64, String)> {
        self.entries
            .iter()
            .find(|entry| entry.place == Place::Asking)
            .map(|entry| (entry.model.id.0, entry.name.clone()))
    }
    /// The whole downloads that have a place to go to, as (id, from, to,
    /// whether a file at `to` is replaced); each is handed out once.
    pub(crate) fn ready_to_move(&mut self) -> Vec<(u64, PathBuf, PathBuf, bool)> {
        self.entries
            .iter_mut()
            .filter_map(|entry| {
                if entry.model.phase != Phase::Complete || entry.pending {
                    return None;
                }
                let from = entry.path.clone()?;
                let moved = match &entry.place {
                    Place::Chosen(to) => (entry.model.id.0, from, to.clone(), true),
                    Place::ToFolder => (entry.model.id.0, from, free_path(&folder(), &entry.name), false),
                    _ => return None,
                };
                entry.place = Place::Moving;
                Some(moved)
            })
            .collect()
    }
    fn trim(&mut self) {
        while self.entries.len() > 200 {
            let Some(at) = self.entries.iter().position(|e| !e.active()) else {
                break;
            };
            self.entries.remove(at);
        }
    }
    pub(crate) fn owns_active(&self, owner: enzui_webkit::web::ViewId) -> bool {
        self.entries
            .iter()
            .any(|e| e.owner == Some(owner) && (e.active() || e.partial.is_some()))
    }
    pub(crate) fn accept(&mut self, owner: enzui_webkit::web::ViewId, event: &PageEvent) -> bool {
        match event {
            PageEvent::DownloadRequested { id, url } => {
                if self
                    .entries
                    .iter()
                    .any(|e| e.owner == Some(owner) && e.native == Some(*id))
                {
                    return false;
                }
                let retry = self
                    .entries
                    .iter()
                    .position(|e| e.owner == Some(owner) && e.retrying && e.url == url.as_str());
                let mut entry = match retry {
                    Some(at) => self.entries.remove(at),
                    None => self.fresh(Some(owner), url.as_str().into()),
                };
                entry.requested |= self.claim_saving(url.as_str());
                entry.native = Some(*id);
                entry.model.phase = Phase::Queued;
                entry.pending = false;
                entry.retrying = false;
                entry.name = url::Url::parse(url.as_str())
                    .ok()
                    .and_then(|u| {
                        u.path_segments()
                            .and_then(|mut parts| parts.next_back())
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned)
                    })
                    .unwrap_or_else(|| "Starting download…".into());
                self.entries.push(entry);
                self.trim();
                true
            }
            PageEvent::DownloadStarted { id, url, path, .. } => {
                let existing = self
                    .entries
                    .iter()
                    .position(|e| e.owner == Some(owner) && e.native == Some(*id));
                let mut entry = match existing {
                    Some(at) => {
                        if !self.entries[at].active() {
                            return false;
                        }
                        self.entries.remove(at)
                    }
                    None => self.fresh(Some(owner), url.as_str().into()),
                };
                entry.requested |= self.claim_saving(url.as_str());
                entry.native = Some(*id);
                entry.name = file_name(path);
                entry.path = Some(path.clone());
                entry.model.phase = Phase::Active;
                entry.pending = false;
                entry.retrying = false;
                self.entries.push(entry);
                self.trim();
                existing.is_none()
            }
            PageEvent::DownloadResponse { id, response } => {
                if let Some(e) = self
                    .entries
                    .iter_mut()
                    .find(|e| e.owner == Some(owner) && e.native == Some(*id) && e.active())
                {
                    e.response = Some(response.clone());
                    if let Some(partial) = e.partial.as_ref().filter(|_| e.resume_requested) {
                        use enzui_core::download::{
                            ContentCoding, RangeDecision, RangeRequestPlan, RangeResponse,
                            Validator,
                        };
                        let validator = partial
                            .response
                            .etag
                            .as_deref()
                            .and_then(Validator::strong_etag)
                            .or_else(|| {
                                partial
                                    .response
                                    .modified
                                    .as_deref()
                                    .and_then(Validator::last_modified)
                            });
                        let response_validator = response
                            .etag
                            .as_deref()
                            .and_then(Validator::parse_etag)
                            .or_else(|| {
                                response
                                    .modified
                                    .as_deref()
                                    .and_then(Validator::last_modified)
                            });
                        let plan = validator
                            .and_then(|v| RangeRequestPlan::conditional(partial.received, v));
                        if let Some(plan) = plan {
                            e.model.set_total(e.model.attempt, partial.response.length);
                            let result = e.model.handle_range_response(
                                e.model.attempt,
                                &plan,
                                &ContentCoding::Identity,
                                &RangeResponse {
                                    status: response.status as u16,
                                    content_range: response.range.as_deref(),
                                    response_validator: response_validator.as_ref(),
                                    content_length: response.length,
                                    content_encoding: response.coding.as_deref(),
                                },
                            );
                            if matches!(result, Some(RangeDecision::RestartWithBody { .. })) {
                                e.partial = None;
                                e.resume_requested = false;
                            } else if !matches!(result, Some(RangeDecision::Resume { .. })) {
                                self.cancel_native.push((owner, *id));
                                e.error="The server could not safely resume this file. Retry to start over.".into();
                                e.model.finish(e.model.attempt, Phase::Failed);
                                e.partial = None;
                                e.resume_requested = false;
                            }
                            e.range_decision = result;
                        } else {
                            self.cancel_native.push((owner, *id));
                            e.error = "The retained download has no valid range identity".into();
                            e.model.finish(e.model.attempt, Phase::Failed);
                            e.partial = None;
                            e.resume_requested = false;
                        }
                    } else if response.status >= 400 || response.status == 206 {
                        self.cancel_native.push((owner, *id));
                        e.error = if response.status == 206 {
                            "This partial response no longer matches a resumable download. Retry to start over.".into()
                        } else {
                            format!("Server returned HTTP {}", response.status)
                        };
                        e.model.finish(e.model.attempt, Phase::Failed);
                    }
                }
                false
            }
            PageEvent::DownloadPaused { id, partial }
            | PageEvent::DownloadInterrupted { id, partial, .. } => {
                if let Some(e) = self
                    .entries
                    .iter_mut()
                    .find(|e| e.owner == Some(owner) && e.native == Some(*id) && e.active())
                {
                    e.model.progress(e.model.attempt, partial.received);
                    e.model.set_total(e.model.attempt, partial.response.length);
                    if validator(&partial.response).is_none() {
                        e.error="The server did not supply a safe resume validator. Retry to start over.".into();
                        e.model.finish(e.model.attempt, Phase::Failed);
                        e.pending = false;
                        return false;
                    }
                    e.partial = Some(partial.clone());
                    if partial.response.length == Some(partial.received) {
                        e.assembly = Some((partial.clone(), None));
                        e.model.phase = Phase::Active;
                        e.pending = true;
                        return false;
                    }
                    e.pending = false;
                    e.resume_requested = false;
                    if let PageEvent::DownloadInterrupted { message, .. } = event {
                        e.model.finish(e.model.attempt, Phase::Failed);
                        e.error.clone_from(message);
                    } else {
                        e.model.pause();
                    }
                }
                false
            }
            PageEvent::DownloadProgress {
                id,
                received,
                expected,
            } => {
                if let Some(e) = self
                    .entries
                    .iter_mut()
                    .find(|e| e.owner == Some(owner) && e.native == Some(*id) && e.active())
                {
                    let offset = e
                        .partial
                        .as_ref()
                        .filter(|_| e.resume_requested)
                        .map_or(0, |p| p.received);
                    if offset == 0 {
                        e.model.set_total(e.model.attempt, *expected);
                    }
                    e.model
                        .progress(e.model.attempt, offset.saturating_add(*received));
                }
                false
            }
            PageEvent::DownloadFinished { id, path } => {
                if let Some(e) = self
                    .entries
                    .iter_mut()
                    .find(|e| e.owner == Some(owner) && e.native == Some(*id) && e.active())
                {
                    e.path = Some(path.clone());
                    if let Some(partial) = e.partial.as_ref().filter(|_| e.resume_requested) {
                        let received = e.model.received.saturating_sub(partial.received);
                        let valid = e
                            .range_decision
                            .as_ref()
                            .and_then(enzui_core::download::RangeDecision::expected_body)
                            .is_some_and(|expected| expected.check(received).is_ok());
                        if valid {
                            e.assembly = Some((partial.clone(), Some(path.clone())));
                            e.pending = true;
                        } else {
                            e.model.finish(e.model.attempt, Phase::Failed);
                            e.error="The resumed response body was incomplete or had an invalid length.".into();
                        }
                        return false;
                    }
                    if e.model.total().is_some_and(|n| e.model.received != n) {
                        e.error =
                            "The server sent an incomplete response body. Retry to start over."
                                .into();
                        e.model.finish(e.model.attempt, Phase::Failed);
                    } else {
                        e.model.finish(e.model.attempt, Phase::Complete);
                    }
                    e.pending = false;
                }
                false
            }
            PageEvent::DownloadFailed {
                id,
                message,
                cancelled,
            } => {
                if let Some(e) = self
                    .entries
                    .iter_mut()
                    .find(|e| e.owner == Some(owner) && e.native == Some(*id) && e.active())
                {
                    e.error.clone_from(message);
                    e.model.finish(
                        e.model.attempt,
                        if *cancelled {
                            Phase::Cancelled
                        } else {
                            Phase::Failed
                        },
                    );
                    e.pending = false;
                }
                false
            }
            PageEvent::DownloadDeclined { url } => {
                let mut entry = self.fresh(Some(owner), url.as_str().into());
                entry.error =
                    "The page could not start this download. Try again or use another download link."
                        .into();
                entry.model.finish_blocked(entry.model.attempt);
                self.entries.push(entry);
                self.trim();
                true
            }
            _ => false,
        }
    }
}
pub(crate) fn is_event(event: &PageEvent) -> bool {
    matches!(
        event,
        PageEvent::DownloadRequested { .. }
            | PageEvent::DownloadStarted { .. }
            | PageEvent::DownloadResponse { .. }
            | PageEvent::DownloadPaused { .. }
            | PageEvent::DownloadInterrupted { .. }
            | PageEvent::DownloadProgress { .. }
            | PageEvent::DownloadFinished { .. }
            | PageEvent::DownloadFailed { .. }
            | PageEvent::DownloadDeclined { .. }
    )
}
fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map_or_else(|| "Download".into(), |n| n.to_string_lossy().into_owned())
}
fn source_label(value: &str) -> String {
    let value = value.strip_prefix("blob:").unwrap_or(value);
    url::Url::parse(value)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_else(|| "This browser".into())
}
pub(crate) fn bytes(value: u64) -> String {
    if value < 1024 {
        format!("{value} B")
    } else if value < 1024 * 1024 {
        format!("{:.0} KB", value as f64 / 1024.)
    } else if value < 1024 * 1024 * 1024 {
        format!("{:.1} MB", value as f64 / (1024. * 1024.))
    } else {
        format!("{:.1} GB", value as f64 / (1024. * 1024. * 1024.))
    }
}

pub(crate) fn assemble_download(
    partial: &enzui_webkit::page::PartialDownload,
    path: Option<&std::path::Path>,
) -> Result<std::path::PathBuf, String> {
    use enzui_security::tempstage::{
        TempStage, link_new_by_identity, remove_if_path_names_file, verify_path_names_file,
    };
    use std::io::{Read, Write};
    use std::os::unix::fs::FileExt;
    let mut part = path
        .map(std::fs::File::open)
        .transpose()
        .map_err(|e| e.to_string())?;
    let part_size = part
        .as_ref()
        .map(|f| f.metadata().map(|m| m.len()))
        .transpose()
        .map_err(|e| e.to_string())?
        .unwrap_or(0);
    if let (Some(path), Some(file)) = (path, &part) {
        verify_path_names_file(file, path).map_err(|e| e.to_string())?;
        if !file.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("The resumed body is not a regular file".into());
        }
    }
    let finished = partial
        .received
        .checked_add(part_size)
        .ok_or("Download size overflow")?;
    if partial.response.length.is_some_and(|n| n != finished) {
        return Err("The assembled file has an invalid length".into());
    }
    let mut stage = TempStage::create_document(&partial.path).map_err(|e| e.to_string())?;
    let mut offset = 0;
    let mut buffer = vec![0u8; 64 * 1024];
    while offset < partial.received {
        let wanted = (partial.received - offset).min(buffer.len() as u64) as usize;
        let read = partial
            .file()
            .read_at(&mut buffer[..wanted], offset)
            .map_err(|e| e.to_string())?;
        if read == 0 {
            return Err("The retained download prefix is incomplete".into());
        }
        stage
            .file_mut()
            .write_all(&buffer[..read])
            .map_err(|e| e.to_string())?;
        offset += read as u64;
    }
    let copied = if let Some(part) = &mut part {
        std::io::copy(
            &mut std::io::Read::by_ref(part).take(part_size),
            stage.file_mut(),
        )
        .map_err(|e| e.to_string())?
    } else {
        0
    };
    if copied != part_size {
        return Err("The resumed body changed while finishing".into());
    }
    stage.file_mut().sync_all().map_err(|e| e.to_string())?;
    let parent = partial.path.parent().ok_or("No download directory")?;
    let stem = partial
        .path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let ext = partial
        .path
        .extension()
        .map_or_else(String::new, |e| format!(".{}", e.to_string_lossy()));
    for attempt in 0..100 {
        let dest = if attempt == 0 {
            partial.path.clone()
        } else {
            parent.join(format!("{stem} ({}){ext}", attempt + 1))
        };
        match link_new_by_identity(stage.file_mut(), &dest) {
            Ok(()) => {
                std::fs::File::open(parent)
                    .and_then(|directory| directory.sync_all())
                    .map_err(|e| e.to_string())?;
                verify_path_names_file(stage.file_mut(), &dest).map_err(|e| e.to_string())?;
                if let (Some(path), Some(part)) = (path, &part) {
                    remove_if_path_names_file(path, part).map_err(|e| e.to_string())?;
                }
                return Ok(dest);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("No free filename for the completed download".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn downloads_match_native_identity_with_owner_and_terminal_progress_cannot_reopen_files() {
        let mut store = Store::default();
        let id = PageRequestId::new(7);
        let mut router = enzui_webkit::web::ViewRouter::new();
        let first = router.create(11);
        let second = router.recreate(11);
        let request = PageEvent::DownloadRequested {
            id,
            url: enzui_webkit::web::PolicyUrl::new("https://example.com/file.pdf"),
        };
        assert!(store.accept(first, &request));
        assert!(store.accept(second, &request));
        assert_eq!(store.entries.len(), 2);
        store.accept(
            first,
            &PageEvent::DownloadStarted {
                id,
                url: enzui_webkit::web::PolicyUrl::new("https://example.com/file.pdf"),
                suggested_name: "file.pdf".into(),
                path: "/tmp/file.pdf".into(),
            },
        );
        store.accept(
            first,
            &PageEvent::DownloadProgress {
                id,
                received: 512,
                expected: Some(1024),
            },
        );
        assert_eq!(store.entries[1].model.percent(), Some(50));
        assert_eq!(store.entries[0].model.received, 0);
        store.accept(
            first,
            &PageEvent::DownloadFailed {
                id,
                message: "Cancelled".into(),
                cancelled: true,
            },
        );
        store.accept(
            first,
            &PageEvent::DownloadProgress {
                id,
                received: 1024,
                expected: Some(1024),
            },
        );
        store.accept(
            first,
            &PageEvent::DownloadStarted {
                id,
                url: enzui_webkit::web::PolicyUrl::new("https://example.com/file.pdf"),
                suggested_name: "late.pdf".into(),
                path: "/tmp/late.pdf".into(),
            },
        );
        assert_eq!(store.entries[1].model.phase, Phase::Cancelled);
        assert_eq!(store.entries[1].model.received, 512);
        assert!(store.owns_active(second));
        assert!(!store.owns_active(first));
        store.accept(
            second,
            &PageEvent::DownloadStarted {
                id,
                url: enzui_webkit::web::PolicyUrl::new("https://example.com/file.pdf"),
                suggested_name: "file.pdf".into(),
                path: "/tmp/other.pdf".into(),
            },
        );
        store.accept(
            second,
            &PageEvent::DownloadProgress {
                id,
                received: 1536,
                expected: None,
            },
        );
        store.accept(
            second,
            &PageEvent::DownloadFinished {
                id,
                path: "/tmp/other.pdf".into(),
            },
        );
        let denied = PageRequestId::new(8);
        store.accept(
            second,
            &PageEvent::DownloadRequested {
                id: denied,
                url: enzui_webkit::web::PolicyUrl::new("https://example.com/denied"),
            },
        );
        store.accept(
            second,
            &PageEvent::DownloadResponse {
                id: denied,
                response: enzui_webkit::page::DownloadResponse {
                    url: enzui_webkit::web::PolicyUrl::new("https://example.com/denied"),
                    status: 403,
                    length: Some(99),
                    ranges: false,
                    etag: None,
                    modified: None,
                    range: None,
                    coding: None,
                },
            },
        );
        store.accept(
            second,
            &PageEvent::DownloadStarted {
                id: denied,
                url: enzui_webkit::web::PolicyUrl::new("https://example.com/denied"),
                suggested_name: "error.html".into(),
                path: "/tmp/error.html".into(),
            },
        );
        assert_eq!(store.entries.last().unwrap().model.phase, Phase::Failed);
        assert_eq!(store.cancel_native, [(second, denied)]);
        let short = PageRequestId::new(9);
        store.accept(
            second,
            &PageEvent::DownloadRequested {
                id: short,
                url: enzui_webkit::web::PolicyUrl::new("https://example.com/short"),
            },
        );
        store.accept(
            second,
            &PageEvent::DownloadProgress {
                id: short,
                received: 50,
                expected: Some(100),
            },
        );
        store.accept(
            second,
            &PageEvent::DownloadFinished {
                id: short,
                path: "/tmp/short".into(),
            },
        );
        assert_eq!(store.entries.last().unwrap().model.phase, Phase::Failed);
        assert_eq!(store.entries.last().unwrap().model.received, 50);
        let stale = PageRequestId::new(10);
        store.accept(
            second,
            &PageEvent::DownloadRequested {
                id: stale,
                url: enzui_webkit::web::PolicyUrl::new("https://example.com/late-range"),
            },
        );
        store.accept(
            second,
            &PageEvent::DownloadResponse {
                id: stale,
                response: enzui_webkit::page::DownloadResponse {
                    url: enzui_webkit::web::PolicyUrl::new("https://example.com/late-range"),
                    status: 206,
                    length: Some(50),
                    ranges: true,
                    etag: Some("\"v1\"".into()),
                    modified: None,
                    range: Some("bytes 50-99/100".into()),
                    coding: None,
                },
            },
        );
        store.accept(
            second,
            &PageEvent::DownloadFinished {
                id: stale,
                path: "/tmp/late-range".into(),
            },
        );
        assert_eq!(store.entries.last().unwrap().model.phase, Phase::Failed);
        let restored = Store::from_archive(&store.archive());
        assert_eq!(restored.entries.len(), 1);
        assert_eq!(restored.entries[0].model.received, 1536);
        assert_eq!(restored.entries[0].model.phase, Phase::Complete);
    }
}

#[cfg(test)]
mod opening_tests {
    use super::*;
    #[test]
    fn new_pdfs_open_once_after_completion_but_restored_files_stay_closed() {
        let mut store = Store::default();
        let mut pdf = store.fresh(None, "https://example.org/document.pdf".into());
        pdf.name = "document.pdf".into();
        pdf.path = Some("/tmp/document.pdf".into());
        pdf.viewed = true;
        let id = pdf.model.id.0;
        store.entries.push(pdf);
        assert!(store.ready_to_open().is_empty());
        let entry = &mut store.entries[0];
        entry.model.finish(entry.model.attempt, Phase::Complete);
        assert_eq!(store.ready_to_open(), vec![id]);
        assert!(store.ready_to_open().is_empty());
        let mut text = store.fresh(None, "https://example.org/text.txt".into());
        text.name = "text.txt".into();
        text.path = Some("/tmp/text.txt".into());
        let text_id = text.model.id.0;
        text.model.finish(text.model.attempt, Phase::Complete);
        store.entries.push(text);
        assert!(store.ready_to_open().is_empty());
        store.entries[1].open_when_finished = true;
        assert_eq!(store.ready_to_open(), vec![text_id]);
        store.completed("/tmp/restored.pdf".into(), 100, "example.org".into());
        assert!(store.ready_to_open().is_empty());
    }
}

#[cfg(test)]
mod date_tests {
    use super::*;

    #[test]
    fn completed_download_dates_survive_archive_and_old_archives_remain_readable() {
        let mut store = Store::default();
        store.completed("/tmp/report.pdf".into(), 4096, "example.org".into());
        let date = store.entries[0].date;
        assert!(date > 0);
        let archive = store.archive();
        let encoded = serde_json::to_vec(&archive).expect("serialize downloads");
        let saved: Vec<Archive> = serde_json::from_slice(&encoded).expect("restore downloads");
        let restored = Store::from_archive(&saved);
        assert_eq!(restored.entries[0].date, date);
        assert_eq!(restored.entries[0].model.received, 4096);
        let old: Archive = serde_json::from_str(
            r#"{"path":"/tmp/missing.pdf","bytes":12,"source":"example.org"}"#,
        )
        .expect("legacy archive");
        assert_eq!(Store::from_archive(&[old]).entries[0].date, 0);
    }
}
