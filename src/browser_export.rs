use super::*;
use iced::widget::{Space, column, container, row, text};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

const SAVE_TAG: u64 = u64::MAX - 1;
#[derive(Clone, Debug)]
pub(crate) enum ExportCommand {
    Open,
    Format(usize),
    Paper(usize),
    Backgrounds,
    Landscape,
    Begin,
    Screenshot,
    Ready(u64, Result<crate::native_jobs::Payload, String>),
    Written(Result<PathBuf, String>),
}
#[derive(Default)]
pub(crate) struct ExportState {
    paper: usize,
    backgrounds: bool,
    landscape: bool,
    busy: bool,
    notice: String,
    pending: BTreeMap<u64, Arc<Vec<u8>>>,
    format: usize,
    title: String,
}
impl App {
    pub(super) fn export_command(&mut self, command: ExportCommand) -> Task<Message> {
        use ExportCommand::*;
        match command {
            Open => {
                self.browser.panel = Panel::Export;
                self.browser.export.notice.clear();
            }
            Format(value) => {
                if !self.browser.export.busy {
                    self.browser.export.format = value.min(3);
                    self.browser.export.notice.clear();
                }
            }
            Paper(value) => {
                if !self.browser.export.busy {
                    self.browser.export.paper = value;
                }
            }
            Backgrounds => {
                if !self.browser.export.busy {
                    self.browser.export.backgrounds = !self.browser.export.backgrounds;
                }
            }
            Landscape => {
                if !self.browser.export.busy {
                    self.browser.export.landscape = !self.browser.export.landscape;
                }
            }
            Begin | Screenshot => {
                if self.browser.export.busy {
                    return Task::none();
                }
                self.browser.panel = Panel::Export;
                if matches!(command, Screenshot) {
                    self.browser.export.format = 3;
                }
                self.browser.export.title = self.browser.saved.tabs[self.browser.saved.active]
                    .title
                    .clone();
                let format = self.browser.export.format;
                let (job, receiver) = crate::native_jobs::Job::new();
                let id = job.id;
                let state = &mut self.browser.export;
                let (mut width, mut height) = if state.paper == 1 {
                    (612., 792.)
                } else {
                    (595.2756, 841.8898)
                };
                if state.landscape {
                    std::mem::swap(&mut width, &mut height);
                }
                let settings = enzui_webkit::printing::PdfPrintSettings {
                    paper_width: width,
                    paper_height: height,
                    print_backgrounds: state.backgrounds,
                    ..Default::default()
                };
                state.busy = true;
                state.notice = "Preparing page…".into();
                self.submit_browser_action(match format {
                    1 => BrowserAction::ExportHtml { job },
                    2 => BrowserAction::ExportMhtml { job },
                    3 => BrowserAction::ExportScreenshot { job },
                    _ => BrowserAction::ExportPdf { settings, job },
                });
                return Task::perform(crate::native_jobs::receive(receiver), move |result| {
                    c(Command::Export(Ready(id, result)))
                });
            }
            Ready(id, result) => {
                let result = result.and_then(|payload| match payload {
                    crate::native_jobs::Payload::Pdf(doc) => Ok(Arc::new(doc.into_bytes())),
                    crate::native_jobs::Payload::Bytes(bytes) => Ok(bytes),
                    crate::native_jobs::Payload::Json(serde_json::Value::String(html))
                        if html.len() <= 64 * 1024 * 1024 =>
                    {
                        Ok(Arc::new(html.into_bytes()))
                    }
                    _ => Err("Unexpected page export result.".into()),
                });
                match result {
                    Ok(document) => {
                        self.browser.panel = Panel::None;
                        self.browser.export.pending.insert(id, document);
                        let title = &self.browser.export.title;
                        let mut filename: String = title
                            .chars()
                            .map(|ch| {
                                if ch.is_control() || "/\\:*?\"<>|".contains(ch) {
                                    '_'
                                } else {
                                    ch
                                }
                            })
                            .take(100)
                            .collect();
                        while filename.len() > 200 {
                            filename.pop();
                        }
                        let folder = if std::env::var_os("ENZIM_QA").is_some() {
                            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("work/overnight/exports")
                        } else {
                            PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                                .join("Downloads")
                        };
                        if let Err(e) = std::fs::create_dir_all(&folder) {
                            self.browser.export.busy = false;
                            self.browser.toasts.error("export", e.to_string());
                            self.browser.export.pending.remove(&id);
                            return Task::none();
                        }
                        let extension = ["pdf", "html", "mhtml", "png"][self.browser.export.format];
                        match self.browser.chooser.start(
                            (SAVE_TAG, enzui_webkit::page::PageRequestId::new(id)),
                            enzui_core::portal::backend::PortalOperation::SaveFile,
                            enzui_core::portal::chooser::ChooserOptions::new()
                                .title(format!("Save page as {}", extension.to_ascii_uppercase()))
                                .current_folder(folder)
                                .current_name(format!(
                                    "{}.{extension}",
                                    if filename.trim().is_empty() {
                                        "Page"
                                    } else {
                                        filename.trim()
                                    }
                                )),
                        ) {
                            Ok(()) => self.browser.export.notice.clear(),
                            Err(e) => {
                                self.browser.export.busy = false;
                                self.browser.export.pending.remove(&id);
                                self.browser
                                    .toasts
                                    .error("export", format!("Could not open Save: {e:?}"));
                            }
                        }
                    }
                    Err(error) => {
                        self.browser.export.busy = false;
                        self.browser.export.notice.clear();
                        self.browser.toasts.error("export", error);
                    }
                }
            }
            Written(result) => {
                self.browser.export.busy = false;
                match result {
                    Ok(path) => {
                        let size = std::fs::metadata(&path).map_or(0, |m| m.len());
                        self.browser
                            .downloads
                            .completed(path.clone(), size, "Saved page".into());
                        self.browser.toasts.done(format!("Saved {}", path.display()));
                    }
                    Err(error) => {
                        // Back to the export options, to try again.
                        self.browser.panel = Panel::Export;
                        self.browser.export.notice.clear();
                        self.browser
                            .toasts
                            .error("export", format!("Save failed: {error}"));
                    }
                }
            }
        }
        Task::none()
    }
    pub(super) fn export_file_chosen(&mut self, id: u64, paths: Vec<PathBuf>) -> Task<Message> {
        let Some(bytes) = self.browser.export.pending.remove(&id) else {
            return Task::none();
        };
        let Some(path) = paths.into_iter().next() else {
            self.browser.export.busy = false;
            self.browser.panel = Panel::Export;
            self.browser.export.notice = "Save cancelled.".into();
            return Task::none();
        };
        Task::perform(
            async move { write_atomic(&path, &bytes, id).map(|()| path) },
            |result| c(Command::Export(ExportCommand::Written(result))),
        )
    }
    pub(super) fn export_panel(&self) -> Element<'_, Message> {
        let state = &self.browser.export;
        let formats = vec![
            "PDF document".into(),
            "HTML document".into(),
            "MHTML web archive".into(),
            "Full-page PNG".into(),
        ];
        let format = enzui_widgets::combo_box::enz_combo_box(
            "",
            formats.clone(),
            Some(formats[state.format].clone()),
            |_| c(Command::Export(ExportCommand::Format(0))),
        )
        .id_prefix("export-format")
        .aria_label("Save page format")
        .selected_index(Some(state.format))
        .on_select_index(|i| c(Command::Export(ExportCommand::Format(i))))
        .width(Length::Fill)
        .into_element();
        let paper = enzui_widgets::combo_box::enz_combo_box(
            "",
            vec!["A4".into(), "US Letter".into()],
            Some(if state.paper == 1 { "US Letter" } else { "A4" }.into()),
            |_| c(Command::Export(ExportCommand::Paper(0))),
        )
        .id_prefix("export-paper")
        .aria_label("PDF paper size")
        .selected_index(Some(state.paper))
        .on_select_index(|i| c(Command::Export(ExportCommand::Paper(i))))
        .width(150)
        .into_element();
        let orientation = action(
            if state.landscape {
                "Landscape"
            } else {
                "Portrait"
            },
            "rotate-cw",
            Command::Export(ExportCommand::Landscape),
        );
        let backgrounds = action(
            if state.backgrounds {
                "Backgrounds: included"
            } else {
                "Backgrounds: omitted"
            },
            "palette",
            Command::Export(ExportCommand::Backgrounds),
        );
        let save = enzui_widgets::button::enz_button(
            if state.busy {
                "Preparing…"
            } else {
                "Save…"
            },
            c(Command::Export(ExportCommand::Begin)),
        )
        .icon("download")
        .kind(enzui_widgets::button::ButtonKind::Accent)
        .enabled(
            !state.busy && self.webview.borrow().native.is_some() && self.browser.pdf.is_none(),
        )
        .into_element();
        let mut body = column![
            heading("file-down", "Save page", "Keep a local copy"),
            format,
            text(match state.format {
                1 => "Saves the original HTML source. Linked resources are not included.",
                2 => "Archives the document and loaded resources in one file.",
                3 => "Captures the entire document, including content outside the viewport.",
                _ => "Preserves selectable text, links and the page’s print layout.",
            })
            .size(13)
        ]
        .spacing(10);
        if state.format == 0 {
            body = body
                .push(row![paper, orientation].spacing(8).wrap())
                .push(backgrounds);
        }
        if !state.notice.is_empty() {
            body = body.push(text(state.notice.clone()).size(13));
        }
        let body = enzui_widgets::scroll_view::enz_scrollable(body)
            .height(Length::Fit.max((self.popup_max_height() - 72.).max(48.)));
        container(column![body, row![Space::new().width(Length::Fill), save]].spacing(10))
            .padding(12)
            .height(Length::Shrink.max(self.popup_max_height()))
            .into()
    }
}
fn write_atomic(path: &Path, bytes: &[u8], id: u64) -> Result<(), String> {
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    if !path.is_absolute() {
        return Err("Save destination must be an absolute path.".into());
    }
    let parent = path
        .parent()
        .ok_or("Save destination has no parent folder.")?;
    let temporary = parent.join(format!(".enzim-save-{}-{id}", std::process::id()));
    let result = (|| -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(|e| e.to_string())
}
