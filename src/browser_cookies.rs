use super::*;
use enzui_webkit::ffi::runtime::{CookieInfo, CookieKey};
use enzui_widgets::toast::ToastTone;
use iced::widget::{Space, column, container, row, text};
#[derive(Clone, Debug)]
pub(crate) enum CookieCommand {
    Open,
    Profile(usize),
    Filter(String),
    Range(usize),
    Refresh,
    Page(i32),
    Remove(CookieKey),
    /// Asks to remove the matching cookies of one site.
    RemoveSite(String),
    /// Shows or hides one site's cookies in the dialog.
    Expand(String),
    ClearMatching,
    ClearAll,
    Confirm,
    Ready(u64, bool, Result<crate::native_jobs::Payload, String>),
}
#[derive(Default)]
pub(crate) struct CookieState {
    profile: Option<u64>,
    host: Option<RefCell<WebViewService>>,
    generation: u64,
    entries: Vec<CookieInfo>,
    filter: String,
    range: usize,
    page: usize,
    busy: bool,
    confirmation: Option<Option<Vec<CookieKey>>>,
    /// Whether the cookies are shown in the Settings dialog rather than
    /// their page; a clear confirmation returns to it.
    pub(crate) dialog: bool,
    expanded: Option<String>,
}
impl App {
    pub(crate) fn retire_container_cookie_host(&mut self, id: u64) {
        if self.browser.cookies.profile == Some(id) {
            self.browser.cookies.host = None;
            self.browser.cookies.generation = self.browser.cookies.generation.wrapping_add(1);
            self.browser.cookies.busy = false;
        }
    }

    pub(crate) fn website_data_clear_target(&self) -> Option<Option<u64>> {
        (matches!(self.browser.cookies.confirmation, Some(None))
            && self.browser.cookies.host.as_ref().is_some_and(|host| {
                let host = host.borrow();
                host.native.is_some() && host.live_view.is_some()
            }))
        .then_some(self.browser.cookies.profile)
    }

    pub(crate) fn cookie_container_removed(&mut self, id: u64) {
        if self.browser.cookies.profile == Some(id) {
            self.browser.cookies.profile = None;
            self.browser.cookies.host = None;
            self.browser.cookies.entries.clear();
            self.browser.cookies.generation = self.browser.cookies.generation.wrapping_add(1);
            self.browser.cookies.busy = false;
            self.browser
                .toasts
                .info(ToastTone::Info, "The selected container was removed.");
        }
    }
    pub(super) fn cookie_command(&mut self, command: CookieCommand) -> Task<Message> {
        use CookieCommand::*;
        match command {
            Open => {
                self.browser.cookies.dialog = true;
                self.browser.cookies.expanded = None;
                self.browser.panel = Panel::Cookies;
                return self.cookie_command(Refresh);
            }
            Expand(site) => {
                let state = &mut self.browser.cookies;
                state.expanded = if state.expanded.as_deref() == Some(site.as_str()) {
                    None
                } else {
                    Some(site)
                };
            }
            RemoveSite(site) => match self.matching_cookies() {
                Ok(entries) => {
                    let keys: Vec<_> = entries
                        .into_iter()
                        .filter(|cookie| site_of(cookie) == site)
                        .map(|cookie| cookie.key.clone())
                        .collect();
                    if !keys.is_empty() {
                        self.browser.cookies.confirmation = Some(Some(keys));
                        self.browser.panel = Panel::CookieClear;
                    }
                }
                Err(error) => self.browser.toasts.error("cookies", error),
            },
            Profile(index) => {
                self.browser.cookies.profile = if index == 0 {
                    None
                } else {
                    self.browser.saved.containers.get(index - 1).map(|c| c.id)
                };
                self.browser.cookies.host = None;
                self.browser.cookies.confirmation = None;
                self.browser.cookies.busy = false;
                return self.cookie_command(Refresh);
            }
            Filter(value) => {
                self.browser.cookies.filter = value;
                self.browser.cookies.page = 0;
            }
            Range(index) => {
                self.browser.cookies.range = index.min(4);
                self.browser.cookies.page = 0;
            }
            Page(delta) => {
                let maximum = self
                    .matching_cookies()
                    .map_or(0, |entries| entries.len().saturating_sub(1) / 100);
                self.browser.cookies.page =
                    (self.browser.cookies.page as i64 + i64::from(delta)).max(0) as usize;
                self.browser.cookies.page = self.browser.cookies.page.min(maximum);
            }
            ClearMatching => match self.matching_cookies() {
                Ok(entries) => {
                    self.browser.cookies.confirmation =
                        Some(Some(entries.into_iter().map(|c| c.key.clone()).collect()));
                    self.browser.panel = Panel::CookieClear;
                }
                Err(error) => self.browser.toasts.error("cookies", error),
            },
            Remove(key) => {
                self.browser.cookies.confirmation = Some(Some(vec![key]));
                self.browser.panel = Panel::CookieClear;
            }
            ClearAll => {
                self.browser.cookies.confirmation = Some(None);
                self.browser.panel = Panel::CookieClear;
            }
            Confirm => {
                self.browser.panel = if self.browser.cookies.dialog {
                    Panel::Cookies
                } else {
                    Panel::None
                };
                if let Some(keys) = self.browser.cookies.confirmation.take() {
                    let (job, receiver) = crate::native_jobs::Job::new();
                    let action = if let Some(keys) = keys {
                        BrowserAction::DeleteCookies { keys, job }
                    } else {
                        BrowserAction::ClearDataJob {
                            data: enzui_webkit::page::WebsiteData::ALL,
                            job,
                        }
                    };
                    return self.cookie_job(action, receiver, true);
                }
            }
            Refresh => {
                self.browser.cookies.generation = self.browser.cookies.generation.wrapping_add(1);
                if self.browser.cookies.host.is_none() {
                    let id = self.browser.cookies.profile;
                    let profile = if self.browser.private {
                        self.browser
                            .private_profiles
                            .entry(id)
                            .or_insert_with(enzui_webkit::web::WebProfile::ephemeral)
                            .clone()
                    } else if let Some(id) = id {
                        let Some(container) =
                            self.browser.saved.containers.iter().find(|c| c.id == id)
                        else {
                            self.browser
                                .toasts
                                .error("cookies", "This container no longer exists.");
                            return Task::none();
                        };
                        enzui_webkit::web::WebProfile::persistent(container.storage_name())
                    } else {
                        enzui_webkit::web::WebProfile::shared()
                    };
                    let profile = match self.routed_profile(id, profile) {
                        Ok(profile) => profile,
                        Err(error) => {
                            self.browser.toasts.error("cookies", error);
                            return Task::none();
                        }
                    };
                    let mut host = WebViewService::new();
                    host.profile = profile;
                    host.open_native();
                    host.hide_view();
                    self.browser.cookies.host = Some(RefCell::new(host));
                }
                let (job, receiver) = crate::native_jobs::Job::new();
                return self.cookie_job(BrowserAction::Cookies { job }, receiver, false);
            }
            Ready(generation, deleted, result) => {
                if generation != self.browser.cookies.generation {
                    return Task::none();
                }
                self.browser.cookies.busy = false;
                match result {
                    Ok(crate::native_jobs::Payload::Cookies(mut entries)) => {
                        entries.sort_by(|a, b| {
                            a.key
                                .domain
                                .cmp(&b.key.domain)
                                .then_with(|| a.key.name.cmp(&b.key.name))
                        });
                        self.browser.cookies.entries = entries;
                    }
                    Ok(_) if deleted => {
                        self.browser
                            .toasts
                            .done("Cleanup complete. Matching site data was removed.");
                        return self.cookie_command(Refresh);
                    }
                    Ok(_) => {
                        self.browser.toasts.error(
                            "cookies",
                            "The engine returned an unexpected inventory result.",
                        );
                    }
                    Err(error) => self.browser.toasts.error("cookies", error),
                }
            }
        }
        Task::none()
    }
    fn cookie_job(
        &mut self,
        action: BrowserAction,
        receiver: tokio::sync::oneshot::Receiver<Result<crate::native_jobs::Payload, String>>,
        deleted: bool,
    ) -> Task<Message> {
        let Some(host) = &self.browser.cookies.host else {
            self.browser
                .toasts
                .error("cookies", "Refresh the cookie inventory first.");
            return Task::none();
        };
        let host = host.borrow();
        if let (Some(native), Some(view)) = (&host.native, host.live_view) {
            if let Err(error) = native.browse(&host.router, view, action) {
                self.browser.toasts.error("cookies", error);
                return Task::none();
            }
            self.browser.cookies.busy = true;
            let generation = self.browser.cookies.generation;
            Task::perform(crate::native_jobs::receive(receiver), move |result| {
                c(Command::Cookies(CookieCommand::Ready(
                    generation, deleted, result,
                )))
            })
        } else {
            self.browser
                .toasts
                .error("cookies", "The cookie manager could not be opened.");
            Task::none()
        }
    }
    fn matching_cookies(&self) -> Result<Vec<&CookieInfo>, String> {
        let state = &self.browser.cookies;
        let query = state.filter.trim().to_lowercase();
        let domains = if state.range == 0 {
            None
        } else {
            Some(
                self.browser.history_store.visited_domains(
                    crate::history::Range::OPTIONS[state.range - 1]
                        .start(chrono::Utc::now().timestamp_millis()),
                    state.profile,
                )?,
            )
        };
        Ok(state
            .entries
            .iter()
            .filter(|cookie| {
                let domain = cookie.key.domain.trim_start_matches('.').to_lowercase();
                (query.is_empty()
                    || format!(
                        "{} {} {}",
                        cookie.key.name, cookie.key.domain, cookie.key.path
                    )
                    .to_lowercase()
                    .contains(&query))
                    && domains.as_ref().is_none_or(|sites| {
                        sites
                            .iter()
                            .any(|site| site == &domain || site.ends_with(&format!(".{domain}")))
                    })
            })
            .collect())
    }
    pub(super) fn cookies_page(&self) -> Element<'_, Message> {
        let state = &self.browser.cookies;
        let mut names = vec![if self.browser.private {
            "Default private container".into()
        } else {
            "Default container".into()
        }];
        names.extend(self.browser.saved.containers.iter().map(|c| c.name.clone()));
        let selected = state
            .profile
            .and_then(|id| {
                self.browser
                    .saved
                    .containers
                    .iter()
                    .position(|c| c.id == id)
                    .map(|i| i + 1)
            })
            .unwrap_or(0);
        let profile = enzui_widgets::combo_box::enz_combo_box(
            "",
            names.clone(),
            Some(names[selected].clone()),
            |_| c(Command::Cookies(CookieCommand::Profile(0))),
        )
        .id_prefix("cookies-profile")
        .aria_label("Cookie container")
        .selected_index(Some(selected))
        .on_select_index(|i| c(Command::Cookies(CookieCommand::Profile(i))))
        .width(220)
        .into_element();
        let ranges = vec![
            "All sites".into(),
            "Visited in the last hour".into(),
            "Visited in the last 24 hours".into(),
            "Visited in the last 7 days".into(),
            "Visited in the last 30 days".into(),
        ];
        let range = enzui_widgets::combo_box::enz_combo_box(
            "",
            ranges.clone(),
            Some(ranges[state.range].clone()),
            |_| c(Command::Cookies(CookieCommand::Range(0))),
        )
        .id_prefix("cookies-range")
        .aria_label("Cookie time range")
        .selected_index(Some(state.range))
        .on_select_index(|i| c(Command::Cookies(CookieCommand::Range(i))))
        .width(240)
        .into_element();
        let search = field(
            "Search cookie name, domain or path",
            &state.filter,
            |q| c(Command::Cookies(CookieCommand::Filter(q))),
            "cookie-search",
        );
        let mut body = column![profile, search].spacing(14);
        let tools: Element<'_, Message> = if self.content_width() < 600. {
            column![
                range,
                action(
                    "Refresh",
                    "refresh-cw",
                    Command::Cookies(CookieCommand::Refresh)
                )
            ]
            .spacing(8)
            .into()
        } else {
            row![
                range,
                Space::new().width(Length::Fill),
                action(
                    "Refresh",
                    "refresh-cw",
                    Command::Cookies(CookieCommand::Refresh)
                )
            ]
            .spacing(8)
            .into()
        };
        body=body.push(tools).push(text("Time filters select cookies for sites visited in that period. Cookie creation dates are not supplied by WebKit.").size(12));
        if state.busy {
            body = body.push(text("Working with the native cookie store…").size(13));
        }
        let entries = match self.matching_cookies() {
            Ok(entries) => entries,
            Err(error) => {
                body = body.push(text(error));
                Vec::new()
            }
        };
        let page = state.page.min(entries.len().saturating_sub(1) / 100);
        body = body.push(
            row![
                text(format!("{} matching cookies", entries.len()))
                    .size(13)
                    .width(Length::Fill),
                enzui_widgets::button::enz_button(
                    "Clear matching…",
                    c(Command::Cookies(CookieCommand::ClearMatching))
                )
                .icon("trash-2")
                .kind(enzui_widgets::button::ButtonKind::DestructiveOutlined)
                .enabled(!entries.is_empty() && !state.busy)
                .into_element()
            ]
            .align_y(iced::Alignment::Center),
        );
        for cookie in entries.iter().skip(page * 100).take(100) {
            let mut flags = vec![];
            if cookie.secure {
                flags.push("Secure");
            }
            if cookie.http_only {
                flags.push("HttpOnly");
            }
            flags.push(match cookie.same_site {
                1 => "SameSite=Lax",
                2 => "SameSite=Strict",
                _ => "SameSite=None",
            });
            let expiry = cookie
                .expires
                .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
                .map_or_else(
                    || "Session cookie".into(),
                    |t| format!("Expires {}", t.format("%Y-%m-%d")),
                );
            body = body.push(section(
                "",
                row![
                    container(glyph("cookie", 18.)).width(24),
                    column![
                        text(cookie.key.name.clone()).size(14),
                        text(format!("{}{}", cookie.key.domain, cookie.key.path)).size(12),
                        text(format!("{} · {expiry}", flags.join(" · "))).size(11)
                    ]
                    .spacing(4)
                    .width(Length::Fill),
                    ink_icon(
                        "trash-2",
                        "Delete cookie",
                        c(Command::Cookies(CookieCommand::Remove(cookie.key.clone()))),
                        28.,
                        14.
                    )
                ]
                .spacing(10)
                .align_y(iced::Alignment::Center),
            ));
        }
        if entries.is_empty() && !state.busy {
            body = body.push(
                container(
                    column![
                        glyph("cookie", 32.),
                        text("No matching cookies").size(20).font(enzui_theme::text_defaults::font(enzui_theme::FontRole::Ui, enzui_theme::WeightRung::Semibold)),
                        text("Choose a different container or search.").size(13)
                    ]
                    .spacing(12)
                    .align_x(iced::Alignment::Center),
                )
                .padding(24)
                .center_x(Length::Fill),
            );
        }
        if entries.len() > 100 {
            body = body.push(
                row![
                    action(
                        "Previous",
                        "chevron-left",
                        Command::Cookies(CookieCommand::Page(-1))
                    ),
                    text(format!(
                        "Page {} of {}",
                        page + 1,
                        entries.len().div_ceil(100)
                    )),
                    action(
                        "Next",
                        "chevron-right",
                        Command::Cookies(CookieCommand::Page(1))
                    )
                ]
                .spacing(12)
                .align_y(iced::Alignment::Center),
            );
        }
        body=body.push(section("Reset this container",column![text("Remove all cookies, local storage, caches and other website data from the selected container. This can sign you out of websites.").size(13),action("Clear all website data…","eraser",Command::Cookies(CookieCommand::ClearAll))].spacing(12)));
        self.page_document(
            "Cookies and site data",
            "cookie",
            "Manage data held by websites.",
            body.into(),
        )
    }
    /// Cookies and site data as a compact dialog: the cookies grouped by
    /// site, a site's own shown when it is opened.
    pub(super) fn cookies_dialog(&self) -> Element<'_, Message> {
        let state = &self.browser.cookies;
        let p = &self.look().palette;
        let narrow = self.browser.viewport.width < 520.;
        let semibold = enzui_theme::text_defaults::font(
            enzui_theme::FontRole::Ui,
            enzui_theme::WeightRung::Semibold,
        );
        let entries = self.matching_cookies();
        let (entries, error) = match entries {
            Ok(entries) => (entries, None),
            Err(error) => (Vec::new(), Some(error)),
        };
        let mut sites: Vec<(String, Vec<&CookieInfo>)> = Vec::new();
        for cookie in &entries {
            let site = site_of(cookie);
            match sites.last_mut() {
                Some((last, list)) if *last == site => list.push(cookie),
                _ => sites.push((site, vec![cookie])),
            }
        }
        sites.sort_by(|a, b| a.0.cmp(&b.0));
        sites.dedup_by(|b, a| {
            if a.0 == b.0 {
                a.1.append(&mut b.1);
                true
            } else {
                false
            }
        });
        let (soft, accent) = (p.accent_soft, p.accent);
        let badge = container(
            enzui_widgets::icon::enz_icon("cookie")
                .size(18.)
                .tint(accent)
                .into_element(),
        )
        .center(36)
        .style(move |_| container::Style {
            background: Some(soft.into()),
            border: iced::Border {
                radius: 10.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });
        let summary = if state.busy && state.entries.is_empty() {
            "Reading the cookie store…".to_owned()
        } else {
            format!(
                "{} {} from {} {}",
                entries.len(),
                if entries.len() == 1 { "cookie" } else { "cookies" },
                sites.len(),
                if sites.len() == 1 { "site" } else { "sites" }
            )
        };
        let header = row![
            badge,
            column![
                text("Cookies and site data").size(16).font(semibold),
                text(summary).size(12).color(p.text_muted),
            ]
            .spacing(2)
            .width(Length::Fill),
            enzui_widgets::toolbar_icon::enz_toolbar_icon(
                "refresh-cw",
                c(Command::Cookies(CookieCommand::Refresh))
            )
            .aria_label("Refresh")
            .tooltip("Refresh")
            .enabled(!state.busy)
            .size(30., 15.)
            .into_element(),
            enzui_widgets::toolbar_icon::enz_toolbar_icon("x", c(Command::Dismiss))
                .aria_label("Close")
                .tooltip("Close")
                .size(30., 15.)
                .into_element(),
        ]
        .spacing(10)
        .align_y(iced::Alignment::Center);
        let ranges = vec![
            "All sites".to_owned(),
            "Visited in the last hour".into(),
            "Visited in the last day".into(),
            "Visited in the last week".into(),
            "Visited in the last 30 days".into(),
        ];
        let range = enzui_widgets::combo_box::enz_combo_box(
            "",
            ranges.clone(),
            Some(ranges[state.range].clone()),
            |_| c(Command::Cookies(CookieCommand::Range(0))),
        )
        .id_prefix("cookies-dialog-range")
        .aria_label("Sites visited in")
        .size(enzui_widgets::text_field::TextFieldSize::Sm)
        .selected_index(Some(state.range))
        .on_select_index(|i| c(Command::Cookies(CookieCommand::Range(i))))
        .width(if narrow { Length::Fill } else { Length::FillPortion(2) })
        .into_element();
        let search = enzui_widgets::text_field::enz_text_field(
            "",
            "Search sites and cookies",
            &state.filter,
            |q| c(Command::Cookies(CookieCommand::Filter(q))),
        )
        .id(iced::advanced::widget::Id::new("cookie-dialog-search"))
        .size(enzui_widgets::text_field::TextFieldSize::Sm)
        .leading(
            enzui_widgets::icon::enz_icon("search")
                .size(14.)
                .tint(p.text_subtle)
                .into_element(),
        )
        .into_element();
        let mut filters: Vec<Element<'_, Message>> = vec![
            container(search)
                .width(if narrow { Length::Fill } else { Length::FillPortion(3) })
                .into(),
        ];
        if !self.browser.saved.containers.is_empty() || self.browser.private {
            let mut names = vec![if self.browser.private {
                "Private".to_owned()
            } else {
                "Default".to_owned()
            }];
            names.extend(self.browser.saved.containers.iter().map(|c| c.name.clone()));
            let selected = state
                .profile
                .and_then(|id| {
                    self.browser
                        .saved
                        .containers
                        .iter()
                        .position(|c| c.id == id)
                        .map(|i| i + 1)
                })
                .unwrap_or(0);
            filters.push(
                enzui_widgets::combo_box::enz_combo_box(
                    "",
                    names.clone(),
                    Some(names[selected].clone()),
                    |_| c(Command::Cookies(CookieCommand::Profile(0))),
                )
                .id_prefix("cookies-dialog-profile")
                .aria_label("Container")
                .size(enzui_widgets::text_field::TextFieldSize::Sm)
                .selected_index(Some(selected))
                .on_select_index(|i| c(Command::Cookies(CookieCommand::Profile(i))))
                .width(if narrow { Length::Fill } else { Length::FillPortion(2) })
                .into_element(),
            );
        }
        filters.push(range);
        let filters: Element<'_, Message> = if narrow {
            iced::widget::Column::with_children(filters).spacing(8).into()
        } else {
            iced::widget::Row::with_children(filters).spacing(8).into()
        };
        const SHOWN: usize = 200;
        let mut list = column![].spacing(2);
        if let Some(error) = error {
            list = list.push(text(error).size(12).color(p.danger));
        }
        for (site, cookies) in sites.iter().take(SHOWN) {
            let open = state.expanded.as_deref() == Some(site.as_str());
            let (hover, ink) = (p.surface_hover, p.text);
            let initial = site
                .chars()
                .find(|c| c.is_alphanumeric())
                .map_or_else(|| "?".to_owned(), |c| c.to_uppercase().to_string());
            let (tile, tile_ink) = (p.surface_alt, p.text_muted);
            let head = row![
                container(text(initial).size(12).font(semibold).color(tile_ink))
                    .center(26)
                    .style(move |_| container::Style {
                        background: Some(tile.into()),
                        border: iced::Border {
                            radius: 7.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
                text(site.clone())
                    .size(13)
                    .width(Length::Fill)
                    .wrapping(iced::widget::text::Wrapping::None),
                text(format!(
                    "{} {}",
                    cookies.len(),
                    if cookies.len() == 1 { "cookie" } else { "cookies" }
                ))
                .size(12)
                .color(p.text_muted),
                glyph(if open { "chevron-up" } else { "chevron-down" }, 14.),
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center);
            let head = iced::widget::button(head)
                .on_press(c(Command::Cookies(CookieCommand::Expand(site.clone()))))
                .width(Length::Fill)
                .padding(iced::Padding::default().top(5).bottom(5).left(6).right(8))
                .style(move |_, status| iced::widget::button::Style {
                    background: (open
                        || matches!(
                            status,
                            iced::widget::button::Status::Hovered
                                | iced::widget::button::Status::Pressed
                        ))
                    .then(|| hover.into()),
                    text_color: ink,
                    border: iced::Border {
                        radius: 8.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                });
            list = list.push(
                row![
                    head,
                    ink_icon(
                        "trash-2",
                        "Remove this site's cookies",
                        c(Command::Cookies(CookieCommand::RemoveSite(site.clone()))),
                        28.,
                        14.
                    ),
                ]
                .spacing(2)
                .align_y(iced::Alignment::Center),
            );
            if open {
                let mut inner = column![].spacing(0);
                for cookie in cookies {
                    let mut flags = vec![];
                    if cookie.secure {
                        flags.push("Secure");
                    }
                    if cookie.http_only {
                        flags.push("HttpOnly");
                    }
                    let expiry = cookie
                        .expires
                        .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
                        .map_or_else(
                            || "Session".to_owned(),
                            |t| format!("Expires {}", t.format("%Y-%m-%d")),
                        );
                    flags.push(&expiry);
                    inner = inner.push(
                        row![
                            column![
                                text(cookie.key.name.clone())
                                    .size(12)
                                    .wrapping(iced::widget::text::Wrapping::Glyph),
                                text(format!(
                                    "{}{} · {}",
                                    cookie.key.domain,
                                    cookie.key.path,
                                    flags.join(" · ")
                                ))
                                .size(11)
                                .color(p.text_subtle)
                                .wrapping(iced::widget::text::Wrapping::Glyph),
                            ]
                            .spacing(1)
                            .width(Length::Fill),
                            ink_icon(
                                "x",
                                "Delete cookie",
                                c(Command::Cookies(CookieCommand::Remove(cookie.key.clone()))),
                                24.,
                                12.
                            ),
                        ]
                        .spacing(8)
                        .padding(iced::Padding::default().top(4).bottom(4))
                        .align_y(iced::Alignment::Center),
                    );
                }
                list = list.push(
                    container(inner).padding(iced::Padding::default().left(38).right(30).bottom(4)),
                );
            }
        }
        if sites.len() > SHOWN {
            list = list.push(
                text(format!(
                    "{} more sites. Search to find one.",
                    sites.len() - SHOWN
                ))
                .size(12)
                .color(p.text_muted),
            );
        }
        if sites.is_empty() && !state.busy {
            list = list.push(
                container(
                    column![
                        glyph("cookie", 26.),
                        text(if state.filter.trim().is_empty() && state.range == 0 {
                            "No site data here"
                        } else {
                            "Nothing matches"
                        })
                        .size(14)
                        .font(semibold),
                        text(if state.filter.trim().is_empty() && state.range == 0 {
                            "Websites you visit will keep their cookies here."
                        } else {
                            "Try other words or a longer time range."
                        })
                        .size(12)
                        .color(p.text_muted),
                    ]
                    .spacing(8)
                    .align_x(iced::Alignment::Center),
                )
                .padding(20)
                .center_x(Length::Fill),
            );
        }
        let rows = sites.len().min(SHOWN) as f32 * 38.
            + state
                .expanded
                .as_ref()
                .and_then(|open| sites.iter().find(|(site, _)| site == open))
                .map_or(0., |(_, cookies)| cookies.len() as f32 * 40.);
        let height = rows
            .max(120.)
            .min((self.browser.viewport.height - 260.).clamp(120., 380.));
        let edge = p.border;
        let list = container(
            enzui_widgets::scroll_view::enz_scrollable(list.padding(4))
                .id(iced::advanced::widget::Id::new("cookie-dialog-list"))
                .height(Length::Fill),
        )
        .height(height)
        .style(move |_| container::Style {
            border: iced::Border {
                radius: 10.0.into(),
                width: 1.,
                color: edge,
            },
            ..Default::default()
        });
        let filtered = !state.filter.trim().is_empty() || state.range != 0;
        let footer = row![
            enzui_widgets::button::enz_button(
                "Clear all data…",
                c(Command::Cookies(CookieCommand::ClearAll))
            )
            .icon("eraser")
            .kind(enzui_widgets::button::ButtonKind::Quiet)
            .size(enzui_widgets::button::ButtonSize::Sm)
            .enabled(!state.busy)
            .into_element(),
            Space::new().width(Length::Fill),
            enzui_widgets::button::enz_button(
                if filtered {
                    format!("Remove {} shown", entries.len())
                } else {
                    "Remove all cookies".to_owned()
                },
                c(Command::Cookies(CookieCommand::ClearMatching))
            )
            .icon("trash-2")
            .kind(enzui_widgets::button::ButtonKind::DestructiveOutlined)
            .size(enzui_widgets::button::ButtonSize::Sm)
            .enabled(!entries.is_empty() && !state.busy)
            .into_element(),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);
        enzui_widgets::modal_surface::enz_modal_surface(
            column![
                header,
                filters,
                list,
                text("Clear all data also removes local storage and caches, and can sign you out of websites.")
                    .size(11)
                    .color(p.text_subtle),
                footer
            ]
            .spacing(12)
            .into(),
        )
        .padding(18.0)
        .width(Length::Fill.max((self.browser.viewport.width - 32.).min(580.)))
        .into_element()
    }
    pub(super) fn cookie_clear_dialog(&self) -> Element<'_, Message> {
        let state = &self.browser.cookies;
        let title = state
            .profile
            .and_then(|id| {
                self.browser
                    .saved
                    .containers
                    .iter()
                    .find(|c| c.id == id)
                    .map(|c| c.name.clone())
            })
            .unwrap_or_else(|| "Default container".into());
        let detail = match &state.confirmation {
            Some(Some(keys)) => format!(
                "Remove {} selected cookies from {title}? Other cookies and containers remain untouched.",
                keys.len()
            ),
            _ => format!(
                "Clear every website’s cookies, local storage and caches in {title}? This can sign you out of websites."
            ),
        };
        enzui_widgets::modal_surface::enz_modal_surface(
            column![
                heading("trash-2", "Clear site data", title),
                text(detail).size(14),
                row![
                    Space::new().width(Length::Fill),
                    action("Cancel", "x", Command::Dismiss),
                    enzui_widgets::button::enz_button(
                        "Clear data",
                        c(Command::Cookies(CookieCommand::Confirm))
                    )
                    .icon("trash-2")
                    .kind(enzui_widgets::button::ButtonKind::Destructive)
                    .into_element()
                ]
                .spacing(10)
            ]
            .spacing(20)
            .into(),
        )
        .padding(20.0)
        .width(Length::Fill.max((self.browser.viewport.width - 32.).min(520.)))
        .into_element()
    }
}

/// The site a cookie belongs to, as its domain without the leading dot.
fn site_of(cookie: &CookieInfo) -> String {
    cookie
        .key
        .domain
        .trim_start_matches('.')
        .trim_start_matches("www.")
        .to_lowercase()
}
#[cfg(test)]
mod tests {
    use super::*;
    fn cookie(name: &str, domain: &str) -> CookieInfo {
        CookieInfo {
            key: CookieKey {
                name: name.into(),
                domain: domain.into(),
                path: "/".into(),
            },
            secure: true,
            http_only: false,
            expires: None,
            same_site: 1,
        }
    }
    fn fixture() -> App {
        let mut app = crate::browser::browser_tests::app();
        app.browser.cookies.entries = vec![
            cookie("sid", ".example.test"),
            cookie("theme", "www.example.test"),
            cookie("consent", ".news.test"),
        ];
        app.browser.cookies.dialog = true;
        app.browser.panel = Panel::Cookies;
        app
    }
    #[test]
    fn the_dialog_removes_a_sites_cookies_and_returns_after_the_confirmation() {
        let mut app = fixture();
        let _ = app.cookie_command(CookieCommand::RemoveSite("example.test".into()));
        assert_eq!(app.browser.panel, Panel::CookieClear);
        assert!(
            matches!(&app.browser.cookies.confirmation, Some(Some(keys)) if keys.len() == 2),
            "a site's cookies include its www. and dot domains"
        );
        let _ = app.update(c(Command::Dismiss));
        assert_eq!(app.browser.panel, Panel::Cookies, "cancel returns to the dialog");
        let _ = app.cookie_command(CookieCommand::Remove(cookie("consent", ".news.test").key));
        let _ = app.cookie_command(CookieCommand::Confirm);
        assert_eq!(app.browser.panel, Panel::Cookies, "clearing returns to the dialog");
        let _ = app.update(c(Command::Dismiss));
        assert_eq!(app.browser.panel, Panel::None);
        assert!(!app.browser.cookies.dialog);
    }
    #[test]
    #[ignore = "rendered cookie dialog; set ENZIM_VISUAL_OUTPUT to a disposable capture directory"]
    fn cookie_dialog_snapshots_both_themes_and_small_sizes() {
        let output = std::path::PathBuf::from(
            std::env::var_os("ENZIM_VISUAL_OUTPUT").expect("capture directory"),
        );
        for (name, theme) in [
            ("dark", enzui_theme::EnzTheme::dark()),
            ("light", enzui_theme::EnzTheme::light()),
        ] {
            let mut app = fixture();
            app.theme =
                enzui_core::theme_file::integration::ThemeIntegration::unmanaged_compiled_theme(
                    theme,
                )
                .0;
            for index in 0..14 {
                app.browser
                    .cookies
                    .entries
                    .push(cookie(&format!("_ga{index}"), &format!(".site{index}.test")));
            }
            app.browser.cookies.expanded = Some("example.test".into());
            for (width, height) in [(900., 700.), (420., 560.)] {
                app.browser.viewport = iced::Size::new(width, height);
                let content = container(app.cookies_dialog()).center(Length::Fill);
                let mut ui = iced_test::Simulator::with_size(
                    Default::default(),
                    iced::Size::new(width, height),
                    content,
                );
                let snapshot = ui.snapshot(&app.theme.iced_theme()).expect("rendered dialog");
                let path = output.join(format!("cookie-dialog-{name}-{}x{}", width as u32, height as u32));
                assert!(snapshot.matches_image(path).expect("PNG capture"));
            }
        }
    }
}
