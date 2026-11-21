//! Printing a page or a PDF: which printer, how many copies, which pages.
//!
//! The printers are CUPS's, asked with `lpstat` and printed to with `lp`,
//! both away from the interface's thread. A website is first made into a PDF
//! by the engine, kept in the session's runtime folder for as long as `lp`
//! takes to read it.

use super::*;
use iced::widget::{Space, column, row, text};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub(crate) enum PrintCommand {
    /// Open the dialog and ask which printers there are.
    Open,
    Printers(Result<Printers, String>),
    Printer(usize),
    Copies(String),
    Pages(String),
    /// Print with what the dialog shows.
    Go,
    Exported(Result<Arc<Vec<u8>>, String>),
    Sent(Result<String, String>),
}

/// The printers CUPS knows and the one it prints to unasked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Printers {
    pub(crate) names: Vec<String>,
    pub(crate) default: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct PrintState {
    printers: Printers,
    /// Asked for, not answered yet.
    asking: bool,
    selected: usize,
    copies: String,
    pages: String,
    busy: bool,
    notice: String,
}

/// The printers in `lpstat -e`'s output and the default in `lpstat -d`'s.
pub(crate) fn printers(destinations: &str, default: &str) -> Printers {
    let names: Vec<String> = destinations
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect();
    let default = default
        .rsplit_once(": ")
        .map(|(_, name)| name.trim().to_owned())
        .filter(|name| names.contains(name));
    Printers { names, default }
}

/// The number of copies typed, 1 to 99; one where nothing is typed.
pub(crate) fn copies(typed: &str) -> Option<u8> {
    let typed = typed.trim();
    if typed.is_empty() {
        return Some(1);
    }
    typed.parse().ok().filter(|copies| (1..=99).contains(copies))
}

/// The pages typed, as `lp -P` takes them: numbers and ranges with commas
/// between (`1-3,7`). `Some(None)` for all pages, `None` for anything else.
pub(crate) fn pages(typed: &str) -> Option<Option<String>> {
    let typed: String = typed.chars().filter(|c| !c.is_whitespace()).collect();
    if typed.is_empty() {
        return Some(None);
    }
    let page = |text: &str| text.parse::<u32>().ok().filter(|page| *page > 0);
    typed
        .split(',')
        .all(|part| match part.split_once('-') {
            Some((first, last)) => page(first).zip(page(last)).is_some_and(|(a, b)| a <= b),
            None => page(part).is_some(),
        })
        .then_some(Some(typed))
}

/// What `lp` is run with. The file comes after `--`, so that no name of a
/// file is read as an option.
pub(crate) fn lp_arguments(
    printer: &str,
    copies: u8,
    pages: Option<&str>,
    file: &std::path::Path,
) -> Vec<std::ffi::OsString> {
    let mut arguments: Vec<std::ffi::OsString> =
        vec!["-d".into(), printer.into(), "-n".into(), copies.to_string().into()];
    if let Some(pages) = pages {
        arguments.extend(["-P".into(), pages.into()]);
    }
    arguments.extend(["--".into(), file.into()]);
    arguments
}

fn ask_printers() -> Result<Printers, String> {
    let run = |flag: &str| {
        std::process::Command::new("lpstat")
            .arg(flag)
            .env("LC_ALL", "C")
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
            .map_err(|error| format!("Printing needs CUPS (lpstat): {error}"))
    };
    Ok(printers(&run("-e")?, &run("-d")?))
}

fn send(arguments: Vec<std::ffi::OsString>, remove: Option<std::path::PathBuf>) -> Result<String, String> {
    let result = std::process::Command::new("lp").args(arguments).output();
    if let Some(file) = remove {
        let _ = std::fs::remove_file(file);
    }
    match result {
        Ok(output) if output.status.success() => {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
        }
        Ok(output) => Err(String::from_utf8_lossy(&output.stderr).trim().to_owned()),
        Err(error) => Err(format!("Printing needs CUPS (lp): {error}")),
    }
}

/// A file for a page's PDF on its way to the printer, readable by the
/// person alone.
fn spool(bytes: &[u8]) -> Result<std::path::PathBuf, String> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    let folder = std::path::PathBuf::from(
        std::env::var_os("XDG_RUNTIME_DIR").ok_or("No runtime folder for this session.")?,
    )
    .join("enzim-browser/print");
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&folder)
        .map_err(|error| error.to_string())?;
    let path = folder.join(format!("page-{}.pdf", next_tab_key()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|error| error.to_string())?;
    std::io::Write::write_all(&mut file, bytes).map_err(|error| error.to_string())?;
    Ok(path)
}

fn on_thread<T: Send + 'static>(
    name: &str,
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
    done: impl Fn(Result<T, String>) -> PrintCommand + Send + 'static,
) -> Task<Message> {
    let name = name.to_owned();
    Task::perform(
        async move {
            let (send, receive) = iced::futures::channel::oneshot::channel();
            let started = std::thread::Builder::new().name(name).spawn(move || {
                let _ = send.send(work());
            });
            match started {
                Ok(_) => receive
                    .await
                    .unwrap_or_else(|_| Err("the work was dropped".into())),
                Err(error) => Err(error.to_string()),
            }
        },
        move |result| c(Command::PrintJob(done(result))),
    )
}

impl App {
    fn printer(&self) -> Option<&str> {
        let state = &self.browser.print;
        state.printers.names.get(state.selected).map(String::as_str)
    }

    fn print_file(&mut self, file: std::path::PathBuf, remove: bool) -> Task<Message> {
        let state = &self.browser.print;
        let (Some(printer), Some(copies), Some(pages)) = (
            self.printer(),
            copies(&state.copies),
            pages(&state.pages),
        ) else {
            return Task::none();
        };
        let arguments = lp_arguments(printer, copies, pages.as_deref(), &file);
        on_thread(
            "print",
            move || send(arguments, remove.then_some(file)),
            PrintCommand::Sent,
        )
    }

    pub(super) fn print_command(&mut self, command: PrintCommand) -> Task<Message> {
        use PrintCommand::*;
        match command {
            Open => {
                self.browser.panel = Panel::Print;
                self.browser.print.notice.clear();
                self.browser.print.busy = false;
                self.browser.print.asking = true;
                return on_thread("printers", ask_printers, Printers);
            }
            Printers(result) => {
                let state = &mut self.browser.print;
                state.asking = false;
                match result {
                    Ok(printers) => {
                        state.selected = printers
                            .default
                            .as_ref()
                            .and_then(|default| printers.names.iter().position(|n| n == default))
                            .unwrap_or(0);
                        state.printers = printers;
                    }
                    Err(error) => {
                        state.printers = Default::default();
                        state.notice = error;
                    }
                }
            }
            Printer(index) => self.browser.print.selected = index,
            Copies(typed) => self.browser.print.copies = typed,
            Pages(typed) => self.browser.print.pages = typed,
            Go => {
                let state = &self.browser.print;
                if state.busy || self.printer().is_none() {
                    return Task::none();
                }
                if copies(&state.copies).is_none() {
                    self.browser.print.notice = "Copies: a number from 1 to 99.".into();
                    return Task::none();
                }
                if pages(&state.pages).is_none() {
                    self.browser.print.notice =
                        "Pages: numbers and ranges, such as 1-3, 7.".into();
                    return Task::none();
                }
                self.browser.print.busy = true;
                self.browser.print.notice = "Sending to the printer…".into();
                if let Some(pdf) = &self.browser.pdf {
                    let file = pdf.path.clone();
                    return self.print_file(file, false);
                }
                let (job, receiver) = crate::native_jobs::Job::new();
                self.submit_browser_action(BrowserAction::ExportPdf {
                    settings: enzui_webkit::printing::PdfPrintSettings::default(),
                    job,
                });
                return Task::perform(crate::native_jobs::receive(receiver), |result| {
                    c(Command::PrintJob(Exported(result.and_then(
                        |payload| match payload {
                            crate::native_jobs::Payload::Pdf(document) => {
                                Ok(Arc::new(document.into_bytes()))
                            }
                            _ => Err("The page could not be made into pages.".into()),
                        },
                    ))))
                });
            }
            Exported(Ok(bytes)) => match spool(&bytes) {
                Ok(file) => return self.print_file(file, true),
                Err(error) => {
                    self.browser.print.busy = false;
                    self.browser.print.notice = error;
                }
            },
            Exported(Err(error)) | Sent(Err(error)) => {
                self.browser.print.busy = false;
                self.browser.print.notice = error;
            }
            Sent(Ok(answer)) => {
                self.browser.print.busy = false;
                self.browser.print.notice.clear();
                if self.browser.panel == Panel::Print {
                    self.browser.panel = Panel::None;
                }
                self.browser.toasts.done(if answer.is_empty() {
                    "Sent to the printer".to_owned()
                } else {
                    answer
                });
            }
        }
        Task::none()
    }

    pub(super) fn print_view(&self) -> Element<'_, Message> {
        let state = &self.browser.print;
        let what = self.browser.pdf.as_ref().map_or_else(
            || self.browser.saved.tabs[self.browser.saved.active].title.clone(),
            |pdf| {
                pdf.path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            },
        );
        let mut body = column![heading("printer", "Print", what)].spacing(10);
        let job = |command| c(Command::PrintJob(command));
        if state.asking {
            body = body.push(text("Looking for printers…").size(13));
        } else if state.printers.names.is_empty() {
            body = body.push(popup_info(
                "No printer is set up",
                "printer",
                "Add one in the desktop's printer settings, or save the page as a PDF.",
            ));
        } else {
            let names = state.printers.names.clone();
            let printer = enzui_widgets::combo_box::enz_combo_box(
                "",
                names.clone(),
                names.get(state.selected).cloned(),
                move |_| job(PrintCommand::Printer(0)),
            )
            .selected_index(Some(state.selected))
            .on_select_index(move |index| job(PrintCommand::Printer(index)))
            .aria_label("Printer")
            .id_prefix("print-printer")
            .into_element();
            body = body.push(printer).push(
                row![
                    column![
                        text("Copies").size(12),
                        field("1", &state.copies, move |typed| job(PrintCommand::Copies(typed)), "print-copies")
                    ]
                    .spacing(4)
                    .width(Length::FillPortion(1)),
                    column![
                        text("Pages").size(12),
                        field("All", &state.pages, move |typed| job(PrintCommand::Pages(typed)), "print-pages")
                    ]
                    .spacing(4)
                    .width(Length::FillPortion(2)),
                ]
                .spacing(10),
            );
        }
        if !state.notice.is_empty() {
            body = body.push(text(state.notice.clone()).size(12));
        }
        let mut actions = row![Space::new().width(Length::Fill)].spacing(8);
        if self.browser.pdf.is_none() {
            actions = actions.push(action(
                "Save as PDF…",
                "file-down",
                Command::Export(ExportCommand::Open),
            ));
        }
        if !state.printers.names.is_empty() && !state.busy {
            actions = actions.push(primary("Print", "printer", Command::PrintJob(PrintCommand::Go)));
        }
        body.push(actions).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_printers_are_read_from_cups_and_its_default_is_one_of_them() {
        let found = printers(
            "Office_Laser\nPhoto-Inkjet\n\n",
            "system default destination: Photo-Inkjet\n",
        );
        assert_eq!(found.names, ["Office_Laser", "Photo-Inkjet"]);
        assert_eq!(found.default.as_deref(), Some("Photo-Inkjet"));
        assert_eq!(
            printers("Office_Laser\n", "no system default destination\n").default,
            None
        );
        // A default that is not among the printers is not offered.
        assert_eq!(
            printers("Office_Laser\n", "system default destination: Gone\n").default,
            None
        );
        assert_eq!(printers("", ""), Printers::default());
    }

    #[test]
    fn copies_and_pages_are_taken_only_as_the_printer_understands_them() {
        assert_eq!(copies(""), Some(1));
        assert_eq!(copies(" 3 "), Some(3));
        for refused in ["0", "100", "-1", "two", "1.5"] {
            assert_eq!(copies(refused), None, "{refused}");
        }
        assert_eq!(pages("  "), Some(None));
        assert_eq!(pages("1-3, 7"), Some(Some("1-3,7".into())));
        assert_eq!(pages("12"), Some(Some("12".into())));
        for refused in ["0", "3-1", "1-", "a", "1;2", "1,,2", "-o raw", "1-2-3"] {
            assert_eq!(pages(refused), None, "{refused}");
        }
    }

    #[test]
    fn the_file_is_never_read_as_an_option() {
        let arguments = lp_arguments(
            "Office_Laser",
            2,
            Some("1-3"),
            std::path::Path::new("-o raw.pdf"),
        );
        assert_eq!(
            arguments,
            ["-d", "Office_Laser", "-n", "2", "-P", "1-3", "--", "-o raw.pdf"]
                .map(std::ffi::OsString::from)
        );
        assert_eq!(
            lp_arguments("P", 1, None, std::path::Path::new("/tmp/a.pdf")),
            ["-d", "P", "-n", "1", "--", "/tmp/a.pdf"].map(std::ffi::OsString::from)
        );
    }
}
