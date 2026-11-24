use super::*;
use iced::widget::{column, container, row, text};

impl App {
    pub(super) fn navigation_error_view(&self, error: &str) -> Element<'_, Message> {
        match self.webview.borrow().native_worker_error.clone() {
            Some(crate::webview_native::NativeWorkerError::Renderer { memory }) => {
                return self.page_stopped_view(memory);
            }
            Some(crate::webview_native::NativeWorkerError::Certificate { host }) => {
                return self.certificate_view(host);
            }
            _ => {}
        }
        let status = match self.webview.borrow().native_worker_error {
            Some(crate::webview_native::NativeWorkerError::Http { status }) => Some(status),
            _ => None,
        };
        let (title, detail) = match status {
            Some(403) => (
                "Access denied",
                "This website refused the request. It may need you to sign in, or it may not be ready yet.",
            ),
            Some(404 | 410) => (
                "Page not found",
                "This address does not have a page. Check the link or try again later.",
            ),
            Some(429) => (
                "Too many requests",
                "This website needs a little time before accepting another request. Try again shortly.",
            ),
            Some(500..=599) => (
                "The website is having trouble",
                "The server could not complete the request. Try again in a moment.",
            ),
            Some(_) => (
                "The request could not be completed",
                "This website rejected the request. Check the address or try again later.",
            ),
            None => (
                "This page could not be opened",
                "Check the address and your connection, then try again.",
            ),
        };
        let p = &self.look().palette;
        let mut body = column![
            glyph(
                if status.is_some() {
                    "circle-alert"
                } else {
                    "globe-off"
                },
                38.
            ),
            text(title).size(28).font(enzui_theme::text_defaults::font(enzui_theme::FontRole::Ui, enzui_theme::WeightRung::Semibold)),
            text(detail).size(14).color(p.text_muted),
        ]
        .spacing(16)
        .width(Length::Fill);
        if let Some(status) = status {
            body = body.push(text(format!("HTTP {status}")).size(12).color(p.text_muted));
        }
        body = body.push(
            row![
                enzui_widgets::button::enz_button("Try again", Message::BrowserGo)
                    .icon("rotate-cw")
                    .kind(enzui_widgets::button::ButtonKind::Accent)
                    .into_element(),
                action("Details", "info", Command::Panel(Panel::Security)),
            ]
            .spacing(10),
        );
        if status.is_none() && local::origin(&self.address).is_some() {
            body = body.push(
                column![
                    action("Open address", "arrow-right", Command::OpenLocal),
                    text("Allows this destination in this container while a tab shows it.")
                        .size(12)
                        .color(p.text_muted),
                ]
                .spacing(8),
            );
        } else if status.is_none() && (error.contains("Cleartext") || error.contains("cleartext")) {
            body = body.push(
                text("This address uses HTTP. Try its HTTPS address if one is available.")
                    .size(13)
                    .color(p.text_muted),
            );
        }
        self.error_page(body.into())
    }    /// The site's certificate failed validation. Continuing is offered for
    /// this host alone and said to be unsafe: it is what a person needs for
    /// a device on their own network, and what an attacker hopes for.
    fn certificate_view(&self, host: String) -> Element<'_, Message> {
        let p = &self.look().palette;
        let body = column![
            glyph("shield-alert", 38.),
            text("This connection is not private").size(28).font(
                enzui_theme::text_defaults::font(
                    enzui_theme::FontRole::Ui,
                    enzui_theme::WeightRung::Semibold
                )
            ),
            text(format!(
                "The certificate of {host} could not be verified, so nothing proves this is really that site. Someone on the network could be reading or changing what you see."
            ))
            .size(14)
            .color(p.text_muted),
            row![
                enzui_widgets::button::enz_button("Try again", Message::BrowserGo)
                    .icon("rotate-cw")
                    .kind(enzui_widgets::button::ButtonKind::Accent)
                    .into_element(),
                action("Details", "info", Command::Panel(Panel::Security)),
            ]
            .spacing(10),
            column![
                action(
                    format!("Continue to {host} (unsafe)"),
                    "arrow-right",
                    Command::TrustCertificate
                ),
                text("Only for a site or device you know, such as one on your own network. Applies to every tab that shares this site data until Browser quits.")
                    .size(12)
                    .color(p.text_muted),
            ]
            .spacing(8),
        ]
        .spacing(16)
        .width(Length::Fill);
        self.error_page(body.into())
    }
    /// The page's own process ended. Nothing of the page is left to show;
    /// a reload starts a fresh process on the same address.
    fn page_stopped_view(&self, memory: bool) -> Element<'_, Message> {
        let p = &self.look().palette;
        let body = column![
            glyph("circle-alert", 38.),
            text("This page stopped working").size(28).font(enzui_theme::text_defaults::font(
                enzui_theme::FontRole::Ui,
                enzui_theme::WeightRung::Semibold
            )),
            text(if memory {
                "It was using too much memory and was closed. Reload to open it again."
            } else {
                "Something went wrong while showing it. Reload to open it again."
            })
            .size(14)
            .color(p.text_muted),
            row![
                enzui_widgets::button::enz_button("Reload", Message::BrowserReload)
                    .icon("rotate-cw")
                    .kind(enzui_widgets::button::ButtonKind::Accent)
                    .into_element()
            ],
        ]
        .spacing(16)
        .width(Length::Fill);
        self.error_page(body.into())
    }
    /// An error's page: in the middle of the page area, and in a low window
    /// from its top with the rest in reach by scrolling.
    fn error_page<'a>(&self, body: Element<'a, Message>) -> Element<'a, Message> {
        let body = container(body).width((self.content_width() - 48.).clamp(180., 480.));
        if self.browser.viewport.height < 620. {
            enzui_widgets::scroll_view::enz_scrollable(
                container(body).padding(24).center_x(Length::Fill),
            )
            .height(Length::Fill)
            .into()
        } else {
            container(body).padding(24).center(Length::Fill).into()
        }
    }
}
