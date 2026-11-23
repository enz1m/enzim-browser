use super::*;
use enzui_core::download::Phase;
use iced::widget::{column, container, row, text};
#[derive(Debug, Clone)]
pub(crate) enum DownloadCommand {
    Cancel(u64),
    Pause(u64),
    Resume(u64),
    Assembled(u64, Result<std::path::PathBuf, String>),
    Retry(u64),
    Open(u64),
    OpenExternal(u64),
    Remove(u64),
    Context(u64, Option<iced::Point>),
    Folder(u64),
    Clear,
    Ready(u64, u64, Result<crate::native_jobs::Payload, String>),
    CancelledRequest(u64, u64, Result<crate::native_jobs::Payload, String>),
    Opened(u64, Result<(), String>),
    /// A download reached the place the person chose, or could not.
    Placed(u64, Result<std::path::PathBuf, String>),
}
impl App {
    pub(super) fn download_host(
        &self,
        owner: enzui_webkit::web::ViewId,
    ) -> Option<&RefCell<WebViewService>> {
        std::iter::once(&self.webview)
            .chain(
                self.browser
                    .sleeping
                    .iter()
                    .filter_map(|t| t.as_ref().map(|t| &t.host)),
            )
            .chain(self.browser.download_hosts.iter())
            .find(|h| h.borrow().live_view == Some(owner))
    }
    pub(crate) fn retain_download_host(&mut self, host: RefCell<WebViewService>) {
        let mut changed = false;
        {
            let mut service = host.borrow_mut();
            service.ingest_native();
            if let Some(owner) = service.live_view {
                for event in &service.page_events {
                    if crate::downloads::is_event(event) {
                        self.browser.downloads.accept(owner, event);
                        changed = true;
                    }
                }
                service
                    .page_events
                    .retain(|event| !crate::downloads::is_event(event));
            }
        }
        let owner = host.borrow().live_view;
        if owner.is_some_and(|owner| self.browser.downloads.owns_active(owner)) {
            host.borrow_mut().hide_view();
            self.browser.download_hosts.push(host);
        }
        if changed {
            self.settle_download_places();
            self.sync_download_activity();
        }
    }
    pub(super) fn sync_downloads(&mut self) {
        let mut opened = false;
        let mut changed = false;
        let mut completed = false;
        let downloads = &mut self.browser.downloads;
        for cell in std::iter::once(&self.webview)
            .chain(
                self.browser
                    .sleeping
                    .iter()
                    .filter_map(|t| t.as_ref().map(|t| &t.host)),
            )
            .chain(self.browser.download_hosts.iter())
        {
            let mut host = cell.borrow_mut();
            host.ingest_native();
            let Some(owner) = host.live_view else {
                continue;
            };
            host.page_events.retain(|event| {
                if crate::downloads::is_event(event) {
                    changed = true;
                    completed |= matches!(
                        event,
                        enzui_webkit::page::PageEvent::DownloadFinished { .. }
                    );
                    opened |= downloads.accept(owner, event);
                    false
                } else {
                    true
                }
            });
        }
        self.browser.download_hosts.retain(|host| {
            host.borrow()
                .live_view
                .is_some_and(|v| downloads.owns_active(v))
        });
        let mut assemblies = Vec::new();
        for entry in &mut self.browser.downloads.entries {
            if let Some((partial, path)) = entry.assembly.take() {
                assemblies.push((entry.model.id.0, partial, path));
            }
        }
        for (id, partial, path) in assemblies {
            self.pending_tasks.push(Task::perform(
                async move {
                    let (send, receive) = iced::futures::channel::oneshot::channel();
                    std::thread::Builder::new()
                        .name("finish-download".into())
                        .spawn(move || {
                            let _ = send.send(crate::downloads::assemble_download(
                                &partial,
                                path.as_deref(),
                            ));
                        })
                        .map_err(|e| e.to_string())?;
                    receive
                        .await
                        .map_err(|_| "File assembly was interrupted".to_owned())?
                },
                move |result| c(Command::Download(DownloadCommand::Assembled(id, result))),
            ));
        }
        let cancellations = std::mem::take(&mut self.browser.downloads.cancel_native);
        for (owner, id) in cancellations {
            if let Some(host) = self.download_host(owner) {
                let host = host.borrow();
                if let Some(native) = &host.native {
                    let (job, _) = crate::native_jobs::Job::new();
                    let _ = native.browse(
                        &host.router,
                        owner,
                        BrowserAction::CancelDownload { id, job },
                    );
                }
            }
        }
        if changed {
            self.browser.chrome_revision = self.browser.chrome_revision.wrapping_add(1);
        }
        if completed && !cfg!(test) {
            self.browser.save();
        }
        let placement_changed = self.settle_download_places();
        if changed || placement_changed {
            self.sync_download_activity();
        }
        for id in self.browser.downloads.ready_to_open() {
            self.pending_tasks
                .push(Task::done(c(Command::Download(DownloadCommand::Open(id)))));
        }
        if opened && self.browser.panel == Panel::None {
            self.browser.panel = Panel::Downloads;
        }
    }
    pub(super) fn download_command(&mut self, command: DownloadCommand) -> Task<Message> {
        let task = self.reduce_download_command(command);
        self.sync_download_activity();
        task
    }

    fn reduce_download_command(&mut self, command: DownloadCommand) -> Task<Message> {
        match command {
            DownloadCommand::Context(id, point) => {
                let Some(entry) = self
                    .browser
                    .downloads
                    .entries
                    .iter()
                    .find(|entry| entry.model.id.0 == id)
                else {
                    return Task::none();
                };
                let items = download_actions(entry)
                    .into_iter()
                    .map(|(label, icon, command)| {
                        enzui_widgets::context_menu_host::ContextMenuItem::command(
                            Command::Download(command),
                            label,
                        )
                        .icon(icon)
                    })
                    .collect();
                self.open_context(
                    self.browser.saved.active,
                    point.unwrap_or(self.browser.pointer),
                    items,
                );
                return Task::none();
            }
            DownloadCommand::Remove(id) => self
                .browser
                .downloads
                .entries
                .retain(|entry| entry.model.id.0 != id || entry.active()),
            DownloadCommand::Assembled(id, result) => {
                if let Some(e) = self
                    .browser
                    .downloads
                    .entries
                    .iter_mut()
                    .find(|e| e.model.id.0 == id)
                {
                    e.pending = false;
                    match result {
                        Ok(path) => {
                            e.name = path.file_name().map_or_else(
                                || "Download".into(),
                                |n| n.to_string_lossy().into_owned(),
                            );
                            e.path = Some(path);
                            e.partial = None;
                            e.resume_requested = false;
                            e.model.finish(e.model.attempt, Phase::Complete);
                        }
                        Err(error) => {
                            e.error = error;
                            e.model.finish(e.model.attempt, Phase::Failed);
                        }
                    }
                }
            }
            DownloadCommand::Pause(id) | DownloadCommand::Resume(id) => {
                let resume = matches!(command, DownloadCommand::Resume(_));
                let Some(entry) = self
                    .browser
                    .downloads
                    .entries
                    .iter_mut()
                    .find(|e| e.model.id.0 == id && !e.pending)
                else {
                    return Task::none();
                };
                let Some(owner) = entry.owner else {
                    return Task::none();
                };
                let (job, receive) = crate::native_jobs::Job::new();
                let action = if resume {
                    let Some(partial) = entry.partial.as_ref() else {
                        return Task::none();
                    };
                    use enzui_core::download::Validator;
                    let Some(validator) = partial
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
                        })
                        .and_then(|v| v.if_range_value().map(str::to_owned))
                    else {
                        return Task::none();
                    };
                    let offset = partial.received;
                    let url = partial.response.url.as_str().to_owned();
                    entry.url.clone_from(&url);
                    entry.resume_requested = true;
                    entry.retrying = true;
                    entry.range_decision = None;
                    entry.error.clear();
                    if entry.model.phase == Phase::Paused {
                        entry.model.resume();
                    } else {
                        entry.model.retry();
                        entry.model.progress(entry.model.attempt, offset);
                    }
                    BrowserAction::ResumeDownload {
                        url,
                        offset,
                        validator,
                        job,
                    }
                } else {
                    let Some(id) = entry.native else {
                        return Task::none();
                    };
                    BrowserAction::PauseDownload { id, job }
                };
                entry.pending = true;
                entry.control = entry.control.wrapping_add(1);
                let control = entry.control;
                if let Some(host) = self.download_host(owner) {
                    let host = host.borrow();
                    if let (Some(native), Some(view)) = (&host.native, host.live_view) {
                        let _ = native.browse(&host.router, view, action);
                        return Task::perform(
                            crate::native_jobs::receive(receive),
                            move |result| {
                                c(Command::Download(DownloadCommand::Ready(
                                    id, control, result,
                                )))
                            },
                        );
                    }
                }
                if let Some(e) = self
                    .browser
                    .downloads
                    .entries
                    .iter_mut()
                    .find(|e| e.model.id.0 == id)
                {
                    e.pending = false;
                    e.error = "The native download connection is unavailable.".into();
                }
            }
            DownloadCommand::Placed(id, result) => self.download_placed(id, result),
            DownloadCommand::Clear => self
                .browser
                .downloads
                .entries
                .retain(crate::downloads::Entry::active),
            DownloadCommand::Opened(id, result) => {
                if let Some(entry) = self
                    .browser
                    .downloads
                    .entries
                    .iter_mut()
                    .find(|e| e.model.id.0 == id)
                {
                    entry.error = result.as_ref().err().cloned().unwrap_or_default();
                }
                if let Err(error) = result {
                    self.browser.toasts.error("downloads", error);
                    self.browser.panel = Panel::Downloads;
                }
            }
            DownloadCommand::CancelledRequest(id, control, result) => {
                if let Some(e) = self
                    .browser
                    .downloads
                    .entries
                    .iter_mut()
                    .find(|e| e.model.id.0 == id && e.control == control)
                {
                    e.pending = false;
                    e.retrying = false;
                    match result {
                        Ok(_) => {
                            e.partial = None;
                            e.resume_requested = false;
                            e.error.clear();
                            e.model.finish(e.model.attempt, Phase::Cancelled);
                        }
                        Err(error) => {
                            e.error = error;
                            e.model.finish(e.model.attempt, Phase::Failed);
                        }
                    }
                }
            }
            DownloadCommand::Ready(id, control, result) => {
                if let Some(entry) = self.browser.downloads.entries.iter_mut().find(|e| {
                    e.model.id.0 == id
                        && e.control == control
                        && !matches!(e.model.phase, Phase::Complete | Phase::Cancelled)
                }) {
                    entry.pending = false;
                    if let Err(error) = result {
                        entry.error = error;
                        if entry.retrying {
                            entry.model.finish(entry.model.attempt, Phase::Failed);
                            entry.retrying = false;
                        }
                    }
                }
            }
            DownloadCommand::Open(id)
                if self
                    .browser
                    .downloads
                    .entries
                    .iter()
                    .any(|entry| entry.model.id.0 == id && entry.active()) =>
            {
                if let Some(entry) = self
                    .browser
                    .downloads
                    .entries
                    .iter_mut()
                    .find(|entry| entry.model.id.0 == id)
                {
                    entry.open_when_finished = !entry.open_when_finished;
                }
            }
            DownloadCommand::Open(id)
            | DownloadCommand::OpenExternal(id)
            | DownloadCommand::Folder(id) => {
                let Some(entry) = self
                    .browser
                    .downloads
                    .entries
                    .iter()
                    .find(|e| e.model.id.0 == id && e.model.phase == Phase::Complete)
                else {
                    return Task::none();
                };
                let Some(path) = entry.path.clone() else {
                    return Task::none();
                };
                let folder = matches!(command, DownloadCommand::Folder(_));
                self.browser.panel = Panel::None;
                if !folder
                    && !matches!(command, DownloadCommand::OpenExternal(_))
                    && entry.is_pdf()
                    && !self.browser.saved.external_pdfs
                {
                    self.new_tab(String::new());
                    return self.open_pdf(path, Some(id));
                }
                return Task::perform(
                    async move {
                        let (send, receive) = iced::futures::channel::oneshot::channel();
                        std::thread::Builder::new()
                            .name("open-download".into())
                            .spawn(move || {
                                let _ = send.send(open_download(path, folder));
                            })
                            .map_err(|e| e.to_string())?;
                        receive
                            .await
                            .map_err(|_| "Opening the file was interrupted".to_owned())?
                    },
                    move |result| c(Command::Download(DownloadCommand::Opened(id, result))),
                );
            }
            DownloadCommand::Cancel(id) | DownloadCommand::Retry(id) => {
                let Some(entry) = self.browser.downloads.entries.iter_mut().find(|e| {
                    e.model.id.0 == id
                        && (!e.pending
                            || matches!(command, DownloadCommand::Cancel(_))
                                && e.resume_requested
                                && e.retrying)
                }) else {
                    return Task::none();
                };
                let retry = matches!(command, DownloadCommand::Retry(_));
                if !retry {
                    entry.open_when_finished = false;
                }
                if !retry && entry.model.phase == Phase::Paused {
                    entry.partial = None;
                    entry.model.finish(entry.model.attempt, Phase::Cancelled);
                    return Task::none();
                }
                if retry
                    && (!entry.network_source()
                        || !matches!(entry.model.phase, Phase::Failed | Phase::Cancelled))
                {
                    return Task::none();
                }
                if !retry && !entry.active() {
                    return Task::none();
                }
                let Some(owner) = entry.owner else {
                    return Task::none();
                };
                let cancel_request =
                    !retry && entry.pending && entry.resume_requested && entry.retrying;
                let native_id = entry.native;
                let url = entry.url.clone();
                entry.pending = true;
                entry.error.clear();
                if retry {
                    entry.partial = None;
                    entry.resume_requested = false;
                    entry.range_decision = None;
                    entry.model.retry();
                    entry.retrying = true;
                }
                entry.control = entry.control.wrapping_add(1);
                let control = entry.control;
                let (job, receive) = crate::native_jobs::Job::new();
                let action = if retry {
                    BrowserAction::DownloadUrl { url, job }
                } else if cancel_request {
                    BrowserAction::CancelRangeDownload { url, job }
                } else if let Some(id) = native_id {
                    BrowserAction::CancelDownload { id, job }
                } else {
                    entry.pending = false;
                    return Task::none();
                };
                if let Some(host) = self.download_host(owner) {
                    let host = host.borrow();
                    if let (Some(native), Some(view)) = (&host.native, host.live_view) {
                        let _ = native.browse(&host.router, view, action);
                        return Task::perform(
                            crate::native_jobs::receive(receive),
                            move |result| {
                                c(Command::Download(if cancel_request {
                                    DownloadCommand::CancelledRequest(id, control, result)
                                } else {
                                    DownloadCommand::Ready(id, control, result)
                                }))
                            },
                        );
                    }
                }
                if let Some(entry) = self
                    .browser
                    .downloads
                    .entries
                    .iter_mut()
                    .find(|e| e.model.id.0 == id)
                {
                    entry.pending = false;
                    entry.model.finish(entry.model.attempt, Phase::Failed);
                    entry.error = "The originating page is no longer available.".into();
                }
            }
        }
        if !cfg!(test) {
            self.browser.save();
        }
        Task::none()
    }
    pub(super) fn downloads_panel(&self) -> Element<'_, Message> {
        let p = self.look().palette.clone();
        let active = self
            .browser
            .downloads
            .entries
            .iter()
            .filter(|entry| entry.active() && entry.model.phase != Phase::Paused)
            .count();
        let paused = self
            .browser
            .downloads
            .entries
            .iter()
            .filter(|entry| entry.model.phase == Phase::Paused)
            .count();
        let summary = match (active, paused) {
            (0, 0) => String::new(),
            (0, paused) => format!("{paused} paused"),
            (active, 0) => format!("{active} downloading"),
            (active, paused) => format!("{active} downloading · {paused} paused"),
        };
        let mut title = column![text("Downloads").size(16)]
            .spacing(3)
            .width(Length::Fill);
        if !summary.is_empty() {
            title = title.push(text(summary).size(11).color(p.text_muted));
        }
        let header = row![
            glyph("download", 17.),
            title,
            ink_icon(
                "list-x",
                "Clear finished",
                c(Command::Download(DownloadCommand::Clear)),
                28.,
                15.
            ),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);
        let mut body = column![].spacing(2);
        for entry in self.browser.downloads.entries.iter().rev().take(100) {
            let id = entry.model.id.0;
            let phase = entry.model.phase;
            let transferred = crate::downloads::bytes(entry.model.received);
            let detail = if phase == Phase::Complete {
                transferred
            } else if let Some(total) = entry.model.total() {
                format!(
                    "{} · {transferred} / {}",
                    phase.word(),
                    crate::downloads::bytes(total)
                )
            } else {
                format!("{} · {transferred}", phase.word())
            };
            let date = chrono::DateTime::from_timestamp(entry.date, 0)
                .filter(|_| entry.date > 0)
                .map(|date| {
                    date.with_timezone(&chrono::Local)
                        .format("%e %b · %H:%M")
                        .to_string()
                });
            let status = if !entry.error.is_empty() {
                format!("{detail} · {}", entry.error)
            } else if entry.open_when_finished {
                format!("{detail} · Opens when finished")
            } else if let Some(date) = date {
                format!("{detail}  ·  {date}")
            } else {
                detail
            };
            let name = row![
                glyph(
                    if entry.is_pdf() {
                        "file-text"
                    } else {
                        "file-down"
                    },
                    16.
                ),
                text(entry.name.clone())
                    .size(13)
                    .font(iced::Font {
                        weight: iced::font::Weight::Semibold,
                        ..Default::default()
                    })
                    .wrapping(iced::widget::text::Wrapping::None)
                    .ellipsis(iced::widget::text::Ellipsis::Middle)
                    .width(Length::Fill)
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center);
            let mut info = column![
                name,
                text(status)
                    .size(11)
                    .color(if phase == Phase::Failed {
                        p.danger
                    } else {
                        p.text_muted
                    })
                    .wrapping(iced::widget::text::Wrapping::None)
                    .ellipsis(iced::widget::text::Ellipsis::End)
            ]
            .spacing(4)
            .width(Length::Fill);
            if entry.active() {
                info = info.push(
                    enzui_widgets::progress_bar::enz_progress_bar(
                        entry.model.fraction().unwrap_or(0.),
                    )
                    .bar_height(2.)
                    .into_element(),
                );
            }
            let file = enzui_widgets::focus_button::focus_button(
                info,
                c(Command::Download(DownloadCommand::Open(id))),
            )
            .on_press_maybe(
                (entry.active() || phase == Phase::Complete)
                    .then(|| c(Command::Download(DownloadCommand::Open(id)))),
            )
            .padding([7, 8])
            .width(Length::Fill)
            .semantics(
                enzui_core::semantics::Semantics::new(enzui_core::semantics::Role::Button)
                    .label(format!("Open {}", entry.name)),
            )
            .style_focus(|theme, _| iced::widget::button::Style {
                background: None,
                text_color: palette(theme).text,
                ..Default::default()
            });
            let mut controls = row![].spacing(0).align_y(iced::Alignment::Center);
            let mut actions = download_actions(entry);
            actions.sort_by_key(|(_, _, command)| !matches!(command, DownloadCommand::Folder(_)));
            for (label, icon, command) in actions.into_iter().filter(|(_, _, command)| {
                matches!(
                    command,
                    DownloadCommand::Folder(_)
                        | DownloadCommand::Open(_)
                        | DownloadCommand::Pause(_)
                        | DownloadCommand::Resume(_)
                        | DownloadCommand::Retry(_)
                        | DownloadCommand::Cancel(_)
                )
            }) {
                controls = controls.push(ink_icon(
                    icon,
                    label,
                    c(Command::Download(command)),
                    26.,
                    14.,
                ));
            }
            let file = enzui_widgets::hover_group::enz_hover_group(
                row![
                    file,
                    container(controls).padding(iced::Padding {
                        top: 3.,
                        right: 4.,
                        bottom: 0.,
                        left: 0.
                    })
                ]
                .align_y(iced::Alignment::Start),
            )
            .width(Length::Fill)
            .style(
                |theme, hovered| enzui_widgets::hover_group::HoverGroupStyle {
                    background: if hovered {
                        palette(theme).surface_hover
                    } else {
                        iced::Color::TRANSPARENT
                    },
                    border_color: iced::Color::TRANSPARENT,
                    border_width: 0.,
                    radius: 8.,
                },
            )
            .into_element();
            let file = enzui_widgets::context_target::enz_context_target(file, move |point| {
                c(Command::Download(DownloadCommand::Context(id, Some(point))))
            })
            .width(Length::Fill)
            .into_element();
            body = body.push(file);
        }
        if self.browser.downloads.entries.is_empty() {
            body = body.push(
                container(
                    column![
                        text("No downloads yet").size(14),
                        text("Downloaded files will appear here.")
                            .size(12)
                            .color(p.text_muted)
                    ]
                    .spacing(6),
                )
                .padding([24, 6]),
            );
        }
        let files = enzui_widgets::scroll_view::enz_scroll_view(body)
            .height(Length::Shrink.max((self.popup_max_height() - 72.).max(48.)))
            .width(Length::Fill)
            .into_element();
        container(column![header, iced::widget::rule::horizontal(1), files].spacing(10))
            .padding(12)
            .height(Length::Shrink.max(self.popup_max_height()))
            .width(Length::Fill)
            .into()
    }
}
fn download_actions(
    entry: &crate::downloads::Entry,
) -> Vec<(&'static str, &'static str, DownloadCommand)> {
    let id = entry.model.id.0;
    let mut items = Vec::new();
    if entry.active() {
        items.push((
            if entry.open_when_finished {
                "Cancel opening when finished"
            } else {
                "Open when finished"
            },
            "external-link",
            DownloadCommand::Open(id),
        ));
    }
    if entry.model.phase == Phase::Complete {
        items.push((
            if entry.is_pdf() {
                "Open in browser"
            } else {
                "Open file"
            },
            "external-link",
            DownloadCommand::Open(id),
        ));
        if entry.is_pdf() {
            items.push((
                "Open in system app",
                "app-window",
                DownloadCommand::OpenExternal(id),
            ));
        }
        items.push(("Show in folder", "folder-open", DownloadCommand::Folder(id)));
        items.push(("Remove from list", "list-x", DownloadCommand::Remove(id)));
    } else if !entry.pending {
        if entry.partial.is_some() && matches!(entry.model.phase, Phase::Paused | Phase::Failed) {
            items.push(("Resume download", "play", DownloadCommand::Resume(id)));
        } else if entry.model.phase == Phase::Active
            && entry.model.received > 0
            && entry.response.as_ref().is_some_and(|response| {
                response.resumable() && crate::downloads::validator(response).is_some()
            })
        {
            items.push(("Pause download", "pause", DownloadCommand::Pause(id)));
        }
        if entry.active() {
            items.push(("Cancel download", "x", DownloadCommand::Cancel(id)));
        } else {
            if entry.network_source() {
                items.push(("Retry download", "rotate-cw", DownloadCommand::Retry(id)));
            }
            items.push(("Remove from list", "list-x", DownloadCommand::Remove(id)));
        }
    } else if entry.resume_requested && entry.retrying {
        items.push(("Cancel download", "x", DownloadCommand::Cancel(id)));
    }
    items
}

pub(crate) fn open_download(path: std::path::PathBuf, folder: bool) -> Result<(), String> {
    use enzui_core::portal::open_uri::{
        OpenDirectoryRequest, OpenFileRequest, RetainedDirectory, RetainedFile,
    };
    let mut backend = enzui_core::portal::open_uri::desktop_launcher::host_open_uri_backend();
    let parent = enzui_core::portal::parent::PortalParent::None;
    let mut pending = if folder {
        let folder = path.parent().ok_or("No download folder")?;
        let file = std::fs::File::open(folder).map_err(|e| e.to_string())?;
        OpenDirectoryRequest::new(
            RetainedDirectory::new(file).map_err(|e| e.to_string())?,
            parent,
        )
        .start(backend.as_mut(), |_| false)
    } else {
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        OpenFileRequest::new(RetainedFile::new(file).map_err(|e| e.to_string())?, parent)
            .start(backend.as_mut(), |_| false)
    }
    .map_err(|e| format!("Could not open download: {e:?}"))?;
    let end = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while std::time::Instant::now() < end {
        if let Some(result) = pending.poll(backend.as_mut()) {
            return if result.is_completed() {
                Ok(())
            } else {
                Err(format!("Could not open download: {result:?}"))
            };
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    Err("The desktop did not respond to the file request".into())
}
