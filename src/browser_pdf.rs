use crate::{
    App, Message,
    browser::{Command, c},
    pdf::{Fit, Page},
};
use enzui_widgets::scroll_view::enz_scrollable as scrollable;
use iced::{
    Element, Length, Task,
    widget::{Space, column, container, row, text},
};
#[derive(Debug, Clone)]
pub(crate) enum PdfCommand {
    Scroll {
        key: u64,
        offset: iced::Vector,
    },
    Step(i32),
    First,
    Last,
    PageInput(String),
    Go,
    Zoom(i32),
    Fit(Fit),
    Rotate,
    Retry,
    OpenExternal,
    ExternalOpened {
        key: u64,
        path: std::path::PathBuf,
        result: Result<(), String>,
    },
    Prefetched {
        ticket: u64,
    },
    Rendered {
        key: u64,
        ticket: u64,
        result: Result<Page, String>,
    },
}
pub(crate) struct Request {
    pub(crate) path: std::path::PathBuf,
    pub(crate) number: u32,
    pub(crate) raster: u32,
    pub(crate) ticket: u64,
    pub(crate) download: Option<u64>,
    pub(crate) error: String,
    alive: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl Drop for Request {
    fn drop(&mut self) {
        self.alive
            .store(false, std::sync::atomic::Ordering::Release);
    }
}
fn next_ticket() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}
fn render_task(
    key: u64,
    request: &Request,
    cache: std::sync::Arc<crate::pdf::Cache>,
    background: bool,
) -> Task<Message> {
    let path = request.path.clone();
    let number = request.number;
    let raster = request.raster;
    let ticket = request.ticket;
    let alive = std::sync::Arc::clone(&request.alive);
    Task::perform(
        async move {
            let (send, receive) = iced::futures::channel::oneshot::channel();
            std::thread::Builder::new()
                .name("pdf-preview".into())
                .spawn(move || {
                    let _ = send.send(crate::pdf::render_cached(
                        path, number, raster, &alive, &cache,
                    ));
                })
                .map_err(|e| e.to_string())?;
            receive
                .await
                .map_err(|_| "The document renderer stopped".to_owned())?
        },
        move |result| {
            if background {
                return c(Command::Pdf(PdfCommand::Prefetched { ticket }));
            }
            c(Command::Pdf(PdfCommand::Rendered {
                key,
                ticket,
                result,
            }))
        },
    )
}
impl App {
    pub(crate) fn open_pdf(
        &mut self,
        path: std::path::PathBuf,
        download: Option<u64>,
    ) -> Task<Message> {
        self.browser.input.anchor = None;
        self.browser.pdf = None;
        self.browser.pdf_prefetch = None;
        self.browser.pdf_external_error = None;
        self.browser.saved.tabs[self.browser.saved.active].title = path.file_name().map_or_else(
            || "PDF preview".into(),
            |name| name.to_string_lossy().into_owned(),
        );
        let key = self.browser.saved.tabs[self.browser.saved.active].key;
        self.browser.pdf_requests.insert(
            key,
            Request {
                path,
                number: 1,
                raster: 1800,
                ticket: next_ticket(),
                download,
                error: String::new(),
                alive: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
            },
        );
        render_task(
            key,
            &self.browser.pdf_requests[&key],
            std::sync::Arc::clone(&self.browser.pdf_cache),
            false,
        )
    }
    pub(crate) fn pdf_command(&mut self, command: PdfCommand) -> Task<Message> {
        if let PdfCommand::Scroll { key, offset } = command {
            if self.browser.saved.tabs[self.browser.saved.active].key == key
                && let Some(page) = self.browser.pdf.as_mut()
            {
                let finite = |value: f32| if value.is_finite() { value.max(0.) } else { 0. };
                page.view.scroll = iced::Vector::new(finite(offset.x), finite(offset.y));
            }
            return Task::none();
        }
        if let PdfCommand::Prefetched { ticket } = command {
            if self
                .browser
                .pdf_prefetch
                .as_ref()
                .is_some_and(|(_, request)| request.ticket == ticket)
            {
                self.browser.pdf_prefetch = None;
            }
            return Task::none();
        }
        if let PdfCommand::ExternalOpened { key, path, result } = command {
            if self.browser.saved.tabs[self.browser.saved.active].key == key
                && self
                    .browser
                    .pdf
                    .as_ref()
                    .map(|page| &page.path)
                    .or_else(|| {
                        self.browser
                            .pdf_requests
                            .get(&key)
                            .map(|request| &request.path)
                    })
                    .is_some_and(|current| *current == path)
            {
                self.browser.pdf_external_error = result.err().map(|error| (key, error));
            }
            return Task::none();
        }
        if matches!(command, PdfCommand::OpenExternal) {
            let key = self.browser.saved.tabs[self.browser.saved.active].key;
            let path = self
                .browser
                .pdf
                .as_ref()
                .map(|p| p.path.clone())
                .or_else(|| self.browser.pdf_requests.get(&key).map(|r| r.path.clone()));
            let Some(path) = path else {
                return Task::none();
            };
            self.browser.pdf_external_error = None;
            let result_path = path.clone();
            return Task::perform(
                async move {
                    let (send, receive) = iced::futures::channel::oneshot::channel();
                    std::thread::Builder::new()
                        .name("open-pdf-app".into())
                        .spawn(move || {
                            let _ = send
                                .send(crate::browser::download_panel::open_download(path, false));
                        })
                        .map_err(|e| e.to_string())?;
                    receive
                        .await
                        .map_err(|_| "Opening the system PDF app was interrupted".to_owned())?
                },
                move |result| {
                    c(Command::Pdf(PdfCommand::ExternalOpened {
                        key,
                        path: result_path,
                        result,
                    }))
                },
            );
        }
        if let PdfCommand::Rendered {
            key,
            ticket,
            result,
        } = command
        {
            let Some(request) = self
                .browser
                .pdf_requests
                .get(&key)
                .filter(|r| r.ticket == ticket)
            else {
                return Task::none();
            };
            let download = request.download;
            let at = self.browser.saved.tabs.iter().position(|t| t.key == key);
            let Some(at) = at else {
                self.browser.pdf_requests.remove(&key);
                return Task::none();
            };
            match result {
                Ok(mut page) => {
                    self.browser.pdf_requests.remove(&key);
                    let old = if at == self.browser.saved.active {
                        self.browser.pdf.as_ref()
                    } else {
                        self.browser.sleeping[at]
                            .as_ref()
                            .and_then(|t| t.pdf.as_ref())
                    };
                    if let Some(old) = old {
                        page.view = old.view.clone();
                        page.view.page_input = page.number.to_string();
                        page.view.error.clear();
                    }
                    if let Some(id) = download
                        && let Some(entry) = self
                            .browser
                            .downloads
                            .entries
                            .iter_mut()
                            .find(|e| e.model.id.0 == id)
                    {
                        entry.error.clear();
                    }
                    let prefetch = if page.number < page.count {
                        let request = Request {
                            path: page.path.clone(),
                            number: page.number + 1,
                            raster: 1800,
                            ticket: next_ticket(),
                            download: None,
                            error: String::new(),
                            alive: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
                        };
                        let task = render_task(
                            key,
                            &request,
                            std::sync::Arc::clone(&self.browser.pdf_cache),
                            true,
                        );
                        self.browser.pdf_prefetch = Some((key, request));
                        task
                    } else {
                        Task::none()
                    };
                    return Task::batch([
                        self.browser_command(Command::PdfReady(key, Ok(page))),
                        prefetch,
                    ]);
                }
                Err(error) => {
                    if let Some(r) = self.browser.pdf_requests.get_mut(&key) {
                        r.error.clone_from(&error);
                    }
                    if let Some(id) = download
                        && let Some(entry) = self
                            .browser
                            .downloads
                            .entries
                            .iter_mut()
                            .find(|e| e.model.id.0 == id)
                    {
                        entry.error.clone_from(&error);
                        self.browser.panel = crate::browser::Panel::Downloads;
                    }
                }
            }
            return Task::none();
        }
        let key = self.browser.saved.tabs[self.browser.saved.active].key;
        if matches!(command, PdfCommand::Retry)
            && let Some(r) = self.browser.pdf_requests.get_mut(&key)
        {
            r.alive.store(false, std::sync::atomic::Ordering::Release);
            r.alive = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
            r.ticket = next_ticket();
            r.error.clear();
            return render_task(
                key,
                r,
                std::sync::Arc::clone(&self.browser.pdf_cache),
                false,
            );
        }
        let size = self
            .browser
            .input
            .bounds
            .map_or(iced::Size::new(800., 600.), |(_, bounds)| bounds.size());
        let Some(page) = self.browser.pdf.as_mut() else {
            return Task::none();
        };
        self.browser.input.anchor = None;
        let pending = self
            .browser
            .pdf_requests
            .get(&key)
            .map_or(page.number, |r| r.number);
        let mut target = pending;
        match command {
            PdfCommand::PageInput(value) => {
                if value.len() <= 8 && value.chars().all(|c| c.is_ascii_digit()) {
                    page.view.page_input = value;
                }
                return Task::none();
            }
            PdfCommand::Go => match page.view.page_input.parse::<u32>() {
                Ok(n) if n > 0 && n <= page.count => target = n,
                _ => {
                    page.view.error = format!("Choose a page from 1 to {}.", page.count);
                    return Task::none();
                }
            },
            PdfCommand::Step(delta) => {
                target =
                    (i64::from(pending) + i64::from(delta)).clamp(1, i64::from(page.count)) as u32;
            }
            PdfCommand::First => target = 1,
            PdfCommand::Last => target = page.count,
            PdfCommand::Zoom(delta) => {
                let current = (page.scale(size) * 100.).round() as i32;
                if (delta < 0 && current <= 25) || (delta > 0 && current >= 400) {
                    return Task::none();
                }
                page.view.percent = if delta == 0 {
                    100
                } else {
                    (current + delta).clamp(25, 400)
                };
                page.view.fit = Fit::Custom;
            }
            PdfCommand::Fit(fit) => page.view.fit = fit,
            PdfCommand::Rotate => page.view.turns = (page.view.turns + 1) % 4,
            _ => {}
        }
        page.view.error.clear();
        self.browser.chrome_revision = self.browser.chrome_revision.wrapping_add(1);
        let raster = ((page.points.0.max(page.points.1) * 96. / 72. * page.scale(size)).ceil()
            as u32)
            .clamp(1800, 4096);
        page.view.page_input = target.to_string();
        if target == page.number
            && raster <= page.raster
            && !self.browser.pdf_requests.contains_key(&key)
        {
            return Task::none();
        }
        let path = page.path.clone();
        self.browser.pdf_requests.insert(
            key,
            Request {
                path,
                number: target,
                raster,
                ticket: next_ticket(),
                download: None,
                error: String::new(),
                alive: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
            },
        );
        let load = render_task(
            key,
            &self.browser.pdf_requests[&key],
            std::sync::Arc::clone(&self.browser.pdf_cache),
            false,
        );
        Task::batch([
            load,
            iced::advanced::widget::operate(
                iced::advanced::widget::operation::scrollable::scroll_to(
                    iced::advanced::widget::Id::from(format!("pdf-scroll-{key}")),
                    iced::widget::scrollable::AbsoluteOffset {
                        x: Some(0.),
                        y: Some(0.),
                    },
                    iced::advanced::widget::operation::Animation::Instant,
                ),
            ),
        ])
    }
    pub(crate) fn pdf_view(&self) -> Element<'_, Message> {
        use crate::browser::{glyph, ink_icon, palette};
        let key = self.browser.saved.tabs[self.browser.saved.active].key;
        let request = self.browser.pdf_requests.get(&key);
        let page = self.browser.pdf.as_ref();
        let path = page.map(|p| &p.path).or_else(|| request.map(|r| &r.path));
        let name = path.and_then(|p| p.file_name()).map_or_else(
            || "PDF preview".into(),
            |n| n.to_string_lossy().into_owned(),
        );
        let name_row = row![
            glyph("file-text", 18.),
            text(name)
                .size(14)
                .wrapping(iced::widget::text::Wrapping::None)
                .ellipsis(iced::widget::text::Ellipsis::End)
                .width(Length::Fill),
            enzui_widgets::toolbar_icon::enz_toolbar_icon(
                "external-link",
                c(Command::Pdf(PdfCommand::OpenExternal))
            )
            .aria_label("Open in system PDF app")
            .tooltip("Open in system PDF app")
            .enabled(path.is_some())
            .into_element(),
            enzui_widgets::toolbar_icon::enz_toolbar_icon("printer", c(Command::Print))
                .aria_label("Print PDF")
                .tooltip("Print PDF")
                .enabled(page.is_some())
                .into_element(),
            ink_icon("x", "Close PDF", c(Command::PdfClose), 28., 14.)
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);
        let external_error = self
            .browser
            .pdf_external_error
            .as_ref()
            .filter(|(owner, _)| *owner == key)
            .map(|(_, error)| error.as_str());
        let Some(page) = page else {
            let status = request.map_or("Opening document…", |r| {
                if r.error.is_empty() {
                    "Opening document…"
                } else {
                    r.error.as_str()
                }
            });
            let failed = request.is_some_and(|r| !r.error.is_empty());
            let mut content = column![
                glyph("file-text", 36.),
                text(if failed {
                    "Couldn’t open this PDF"
                } else {
                    "Opening document…"
                })
                .size(20).font(enzui_theme::text_defaults::font(enzui_theme::FontRole::Ui, enzui_theme::WeightRung::Semibold)),
                text(if failed {
                    status
                } else {
                    "Preparing your preview."
                })
                .size(14)
                .align_x(iced::alignment::Horizontal::Center)
            ]
            .spacing(16)
            .align_x(iced::Alignment::Center)
            .width(Length::Fill.max(440));
            if let Some(error) = external_error {
                content = content.push(text(error).size(12));
            }
            if failed {
                content = content.push(
                    row![
                        enzui_widgets::button::enz_button("Open another PDF", c(Command::OpenPdf))
                            .icon("folder-open")
                            .into_element(),
                        enzui_widgets::button::enz_button(
                            "Retry",
                            c(Command::Pdf(PdfCommand::Retry))
                        )
                        .icon("rotate-cw")
                        .kind(enzui_widgets::button::ButtonKind::Quiet)
                        .into_element()
                    ]
                    .spacing(8),
                );
            }
            return column![
                container(name_row).padding(10),
                container(content)
                    .padding(if self.browser.viewport.height < 500. {
                        12
                    } else {
                        24
                    })
                    .center(Length::Fill)
            ]
            .into();
        };
        let number = request.map_or(page.number, |r| r.number);
        let nav = |symbol: &str, label: &str, command: PdfCommand, enabled: bool| {
            enzui_widgets::toolbar_icon::enz_toolbar_icon(symbol, c(Command::Pdf(command)))
                .aria_label(label)
                .tooltip(label)
                .enabled(enabled)
                .into_element()
        };
        let input =
            enzui_widgets::text_field::enz_text_field("", "Page", &page.view.page_input, |s| {
                c(Command::Pdf(PdfCommand::PageInput(s)))
            })
            .id(iced::advanced::widget::Id::new("pdf-page-number"))
            .aria_label("PDF page number")
            .on_submit(c(Command::Pdf(PdfCommand::Go)))
            .into_element();
        let navigation = row![
            nav(
                "chevrons-left",
                "First page (Home)",
                PdfCommand::First,
                number > 1
            ),
            nav(
                "chevron-left",
                "Previous page (Page Up)",
                PdfCommand::Step(-1),
                number > 1
            ),
            container(input).width(52),
            text(format!("of {}", page.count)).size(12),
            nav(
                "chevron-right",
                "Next page (Page Down)",
                PdfCommand::Step(1),
                number < page.count
            ),
            nav(
                "chevrons-right",
                "Last page (End)",
                PdfCommand::Last,
                number < page.count
            )
        ]
        .spacing(2)
        .align_y(iced::Alignment::Center);
        let size = self
            .browser
            .input
            .bounds
            .map_or(iced::Size::new(800., 600.), |(_, b)| b.size());
        let zoom = row![
            nav(
                "minus",
                "Zoom out (Ctrl+-)",
                PdfCommand::Zoom(-10),
                page.scale(size) > 0.25
            ),
            text(format!("{}%", (page.scale(size) * 100.).round() as u32))
                .size(12)
                .width(44),
            nav(
                "plus",
                "Zoom in (Ctrl++)",
                PdfCommand::Zoom(10),
                page.scale(size) < 4.
            ),
            nav(
                "move-horizontal",
                "Fit width",
                PdfCommand::Fit(Fit::Width),
                true
            ),
            nav("maximize", "Fit page", PdfCommand::Fit(Fit::Page), true),
            nav("rotate-cw", "Rotate clockwise", PdfCommand::Rotate, true)
        ]
        .spacing(2)
        .align_y(iced::Alignment::Center);
        let controls: Element<'_, Message> = if self.browser.viewport.width < 700. {
            column![navigation, zoom].spacing(6).into()
        } else {
            row![navigation, Space::new().width(Length::Fill), zoom]
                .align_y(iced::Alignment::Center)
                .into()
        };
        let mut toolbar = column![name_row, controls].spacing(6);
        if let Some(error) = external_error {
            toolbar = toolbar.push(text(error).size(12));
        }
        if let Some(r) = request {
            let status = text(if r.error.is_empty() {
                format!("Loading page {}…", r.number)
            } else {
                r.error.clone()
            })
            .size(12)
            .width(Length::Fill);
            let mut line = row![status].align_y(iced::Alignment::Center).spacing(8);
            if !r.error.is_empty() {
                line = line.push(ink_icon(
                    "rotate-cw",
                    "Retry PDF",
                    c(Command::Pdf(PdfCommand::Retry)),
                    28.,
                    14.,
                ));
            }
            toolbar = toolbar.push(line);
        } else if !page.view.error.is_empty() {
            toolbar = toolbar.push(text(page.view.error.clone()).size(12));
        }
        let image = iced::widget::responsive(move |size| -> Element<'_, Message> {
            let scale = page.scale(size);
            let w = page.points.0 * 96. / 72. * scale;
            let h = page.points.1 * 96. / 72. * scale;
            let (display_w, display_h) = if page.view.turns % 2 == 0 {
                (w, h)
            } else {
                (h, w)
            };
            let paper = container(
                iced::widget::image(page.image.clone())
                    .width(display_w)
                    .height(display_h)
                    .rotation(iced::Rotation::Solid(iced::Radians(
                        f32::from(page.view.turns) * std::f32::consts::FRAC_PI_2,
                    ))),
            )
            .center_x(display_w)
            .center_y(display_h);
            let pages = container(paper)
                .padding(16)
                .center_x(size.width.max(display_w + 32.));
            scrollable(pages)
                .id(iced::advanced::widget::Id::from(format!(
                    "pdf-scroll-{key}"
                )))
                .direction(iced::widget::scrollable::Direction::Both {
                    vertical: enzui_widgets::scroll_view::overlay_scrollbar(),
                    horizontal: enzui_widgets::scroll_view::overlay_scrollbar(),
                })
                .initial_offset(iced::widget::scrollable::AbsoluteOffset {
                    x: page.view.scroll.x,
                    y: page.view.scroll.y,
                })
                .on_scroll(move |viewport| {
                    let offset = viewport.viewport.absolute_offset();
                    c(Command::Pdf(PdfCommand::Scroll {
                        key,
                        offset: iced::Vector::new(offset.x, offset.y),
                    }))
                })
                .style(move |theme, status| {
                    let canvas = palette(theme).surface_hover;
                    let band = enzui_theme::text_defaults::scrollbar().hit_width / 2.;
                    let paper_left = (size.width.max(display_w + 32.) - display_w) / 2.;
                    let x = page.view.scroll.x + size.width - band;
                    let y = page.view.scroll.y + size.height - band;
                    let vertical = if x >= paper_left && x <= paper_left + display_w {
                        iced::Color::WHITE
                    } else {
                        canvas
                    };
                    let horizontal = if y >= 16. && y <= 16. + display_h {
                        iced::Color::WHITE
                    } else {
                        canvas
                    };
                    let mut style = enzui_widgets::scroll_view::overlay_scrollbar_style_over(
                        theme,
                        status,
                        Some(horizontal),
                    );
                    style.vertical_rail = enzui_widgets::scroll_view::overlay_scrollbar_style_over(
                        theme,
                        status,
                        Some(vertical),
                    )
                    .vertical_rail;
                    style
                })
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        });
        let viewport =
            container(self.surface_input(image.into(), crate::browser_input::Surface::Pdf(key)))
                .height(Length::Fill)
                .style(|theme| container::Style {
                    background: Some(palette(theme).surface_hover.into()),
                    ..Default::default()
                });
        column![container(toolbar).padding(10), viewport].into()
    }
}
