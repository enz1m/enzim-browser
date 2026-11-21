use super::*;
use enzui_widgets::scroll_view::enz_scrollable as scrollable;
use iced::widget::{Space, column, container, row, text};

#[derive(Clone, Copy)]
enum SuggestionTarget {
    Address,
    NewTab,
    StartPage,
}

impl App {
    pub(crate) fn show_internal_page(&mut self, page: InternalPage) {
        if page.is_library() {
            self.open_library(page);
            return;
        }
        self.close_library();
        if page == InternalPage::History {
            self.refresh_history();
        }
        self.webview.borrow_mut().hide_view();
        let old = std::mem::replace(&mut self.webview, RefCell::new(WebViewService::new()));
        self.retain_download_host(old);
        self.pending_navigation = None;
        self.pending_native_open = false;
        self.pending_pdf = None;
        self.browser.pdf = None;
        // The view that asked is gone, and what it asked with it.
        self.browser.dialog = None;
        self.browser.sign_in = None;
        self.browser.options = None;
        self.browser.prompt_tab = None;
        self.browser.panel = Panel::None;
        self.browser.query.clear();
        self.browser.suggestion_highlight = None;
        self.browser.start_suggestions_open = false;
        self.browser.tabs_open = false;
        let tab = &mut self.browser.saved.tabs[self.browser.saved.active];
        if page != InternalPage::New {
            tab.container = None;
        }
        tab.url = page.url().into();
        tab.title = page.title().into();
        tab.favicon = None;
        let old_key = tab.key;
        tab.key = next_tab_key();
        let new_key = tab.key;
        crate::page_ids::rename(old_key, new_key);
        self.browser.replace_order_key(old_key, new_key);
        self.address = page.url().into();
        self.address_dirty = false;
        self.last_location = None;
        if page == InternalPage::Import
            && !self.browser.private
            && self.browser.importing.profiles.is_empty()
            && !self.browser.importing.busy
        {
            let task = self.import_command(ImportCommand::Scan);
            self.pending_tasks.push(task);
        }
        if page == InternalPage::Cookies {
            self.browser.cookies.dialog = false;
            let task = self.cookie_command(cookies::CookieCommand::Refresh);
            self.pending_tasks.push(task);
        }
        if !cfg!(test) {
            self.browser.save();
        }
    }
    pub(super) fn open_internal_page(&mut self, page: InternalPage) {
        if page.is_library() {
            self.open_library(page);
            return;
        }
        self.close_library();
        if let Some(i) = self
            .browser
            .saved
            .tabs
            .iter()
            .position(|t| InternalPage::from_url(&t.url) == Some(page))
        {
            self.select_tab(i);
        } else {
            self.new_tab_in_container(page.url().into(), None);
        }
        self.browser.panel = Panel::None;
        self.browser.tabs_open = false;
        if !cfg!(test) {
            self.browser.save();
        }
    }
    pub(crate) fn open_library(&mut self, page: InternalPage) {
        if self.browser.library == Some(page) {
            self.close_library();
            self.resume_selected_document();
            return;
        }
        if self.browser.library == Some(InternalPage::Passwords)
            || self.browser.panel == Panel::Passwords
        {
            self.browser.credentials.dismiss();
        }
        self.browser.container_editor_open = false;
        self.browser.library = Some(page);
        self.browser.input.anchor = None;
        self.webview.borrow_mut().hide_view();
        self.browser.panel = Panel::None;
        self.browser.tabs_open = false;
        self.address = page.url().into();
        self.address_dirty = false;
        self.refresh_library_menu();
        if page == InternalPage::History {
            self.refresh_history();
        }
    }
    pub(crate) fn close_library(&mut self) {
        let previous = self.browser.library.take();
        if previous == Some(InternalPage::Passwords) {
            self.browser.credentials.dismiss();
        }
        if previous.is_some() {
            self.webview.borrow_mut().show_view();
            self.address = self.browser.saved.tabs[self.browser.saved.active]
                .url
                .clone();
            self.address_dirty = false;
            self.last_location = None;
            self.refresh_library_menu();
            self.refresh_credential_documents();
        }
    }
    pub(crate) fn resume_selected_document(&mut self) {
        let url = self.browser.saved.tabs[self.browser.saved.active]
            .url
            .clone();
        if self.webview.borrow().native.is_none()
            && self.browser.pdf.is_none()
            && InternalPage::from_url(&url).is_none()
        {
            self.submit_browser_action(BrowserAction::Navigate(url));
        }
    }
    pub(crate) fn refresh_library_menu(&mut self) {
        let enabled = self.browser.library.is_none();
        let entries = menu_model(&self.browser.saved.containers)
            .entries()
            .iter()
            .cloned()
            .map(|entry| {
                if matches!(
                    entry.id().as_str(),
                    "find" | "save" | "print" | "screenshot" | "devtools"
                ) {
                    entry.enabled(enabled)
                } else {
                    entry
                }
            })
            .collect();
        self.browser.menu_model.reconcile(entries);
        self.browser.chrome_revision = self.browser.chrome_revision.wrapping_add(1);
    }
    /// A line along the top of a page that is loading, as long as the part
    /// of the load the engine counts done. It lies over the page: the page
    /// is not moved or resized for it.
    pub(super) fn load_progress_over<'a>(&self, page: Element<'a, Message>) -> Element<'a, Message> {
        let (loading, progress) = {
            let service = self.webview.borrow();
            (service.loading, service.load_progress)
        };
        if !loading || progress <= 0.0 || progress >= 1.0 {
            return page;
        }
        let accent = self.look().palette.accent;
        // In thousandths, so that a share of the width is a whole number.
        let done = (progress * 1000.0).round().clamp(1.0, 999.0) as u16;
        let line = row![
            container(Space::new())
                .width(Length::FillPortion(done))
                .height(2)
                .style(move |_| container::Style {
                    background: Some(accent.into()),
                    ..Default::default()
                }),
            Space::new().width(Length::FillPortion(1000 - done)),
        ];
        iced::widget::stack![page, line].into()
    }
    pub(super) fn content_width(&self) -> f32 {
        let rail = match self.effective_tab_layout() {
            Some(Layout::Sidebar) => 232.0,
            Some(Layout::Icons) => 56.0,
            _ => 0.0,
        };
        (self.browser.viewport.width - 20.0 - rail).max(180.0)
    }
    pub(super) fn internal_page_view(&self, page: InternalPage) -> Element<'_, Message> {
        match page {
            InternalPage::New => self.new_tab_page(),
            InternalPage::Settings => self.page_document(
                "Settings",
                "settings",
                "Make this browser yours.",
                self.panel_body(Panel::Settings),
            ),
            InternalPage::Bookmarks => self.bookmarks_page(),
            InternalPage::History => self.history_page(),
            InternalPage::Containers => self.containers_page(),
            InternalPage::Cookies => self.cookies_page(),
            InternalPage::Import => self.import_page(),
            InternalPage::Passwords => self.credentials_page(),
        }
    }
    pub(super) fn page_document<'a>(
        &'a self,
        title: &str,
        symbol: &str,
        subtitle: impl Into<String>,
        body: Element<'a, Message>,
    ) -> Element<'a, Message> {
        self.page_document_with_action(title, symbol, subtitle, body, None)
    }
    pub(super) fn page_document_with_action<'a>(
        &'a self,
        title: &str,
        symbol: &str,
        subtitle: impl Into<String>,
        body: Element<'a, Message>,
        action: Option<Element<'a, Message>>,
    ) -> Element<'a, Message> {
        let width = (self.content_width() - 32.0).clamp(180.0, 820.0);
        let mut title_row = row![text(title.to_owned()).size(if width < 420.0 { 24 } else { 28 })]
            .align_y(iced::Alignment::Center)
            .spacing(12);
        if let Some(action) = action {
            title_row = title_row
                .push(Space::new().width(Length::Fill))
                .push(action);
        }
        let heading = row![
            container(glyph(symbol, 26.0)).width(28).height(28),
            column![title_row, text(subtitle.into()).size(13)]
                .spacing(5)
                .width(Length::Fill)
        ]
        .spacing(14)
        .align_y(iced::Alignment::Center);
        scrollable(
            container(
                container(column![heading, body].spacing(28))
                    .width(width)
                    .padding(iced::Padding {
                        top: 28.0,
                        right: 8.0,
                        bottom: 32.0,
                        left: 8.0,
                    }),
            )
            .center_x(Length::Fill),
        )
        .id(iced::widget::Id::new("library-document-scroll"))
        .height(Length::Fill)
        .into()
    }
    pub(super) fn reset_library_scroll(&mut self) {
        self.pending_tasks.push(iced::widget::operation::snap_to(
            iced::widget::Id::new("library-document-scroll"),
            iced::widget::scrollable::RelativeOffset::START,
            iced::widget::operation::Animation::Instant,
        ));
    }
    pub(super) fn site_icon(&self, url: &str, size: u16) -> Element<'static, Message> {
        if let Some(handle) = self.browser.favicons.get(url) {
            iced::widget::image(handle)
                .width(u32::from(size))
                .height(u32::from(size))
                .into()
        } else {
            container(glyph("globe", f32::from(size)))
                .width(u32::from(size))
                .height(u32::from(size))
                .into()
        }
    }
    fn frequent_sites(&self) -> Vec<(String, String)> {
        self.suggestions_for("")
            .into_iter()
            .take(6)
            .map(|s| (s.title, s.url))
            .collect()
    }
    pub(super) fn new_tab_suggestions(&self) -> Vec<Suggestion> {
        self.suggestions_for(&self.browser.query)
    }
    pub(super) fn suggestions_for(&self, query: &str) -> Vec<Suggestion> {
        let mut sites = ranked_suggestions(
            &self.browser.saved.history,
            &self.browser.saved.bookmarks,
            query,
        );
        let engine = self.browser.saved.search.active();
        let search_result = |term: &str| Suggestion {
            title: term.into(),
            url: engine.template.replace(
                "{query}",
                &url::form_urlencoded::byte_serialize(term.as_bytes()).collect::<String>(),
            ),
            bookmarked: false,
            shortcut: false,
            search: true,
        };
        if self.browser.saved.search.suggestions
            && !self.browser.private
            && self.browser.remote_search.matches(query, &engine.id)
        {
            for term in self.browser.remote_search.results.iter().take(4) {
                sites.push(search_result(term));
            }
        }
        if sites.is_empty() && crate::search_suggestions::is_search(query) {
            sites.push(search_result(query.trim()));
        }
        sites
    }
    fn suggestion_options(
        &self,
        sites: &[Suggestion],
        highlight: Option<usize>,
        target: SuggestionTarget,
    ) -> Element<'_, Message> {
        let prefix = match target {
            SuggestionTarget::Address => "address",
            SuggestionTarget::NewTab => "new",
            SuggestionTarget::StartPage => "start",
        };
        let mut links = column![].spacing(2).width(Length::Fill);
        for (i, suggestion) in sites.iter().enumerate() {
            let selected = Some(i) == highlight;
            let title = suggestion.title.clone();
            let url = suggestion.url.clone();
            let detail = if suggestion.search {
                format!("Search with {}", self.browser.saved.search.active().name)
            } else if suggestion.shortcut {
                site_name(&url)
            } else {
                url.strip_prefix("https://")
                    .or_else(|| url.strip_prefix("http://"))
                    .unwrap_or(&url)
                    .to_owned()
            };
            links = links.push(
                enzui_widgets::focus_button::focus_button(
                    row![
                        if suggestion.search {
                            glyph("search", 20.)
                        } else {
                            self.site_icon(&url, 20)
                        },
                        column![
                            text(title.clone())
                                .size(14)
                                .wrapping(iced::widget::text::Wrapping::None)
                                .ellipsis(iced::widget::text::Ellipsis::End),
                            text(detail)
                                .size(11)
                                .wrapping(iced::widget::text::Wrapping::None)
                                .ellipsis(iced::widget::text::Ellipsis::End)
                        ]
                        .spacing(3)
                        .width(Length::Fill),
                        glyph(
                            if suggestion.search {
                                "search"
                            } else if suggestion.bookmarked {
                                "bookmark"
                            } else if suggestion.shortcut {
                                "globe"
                            } else {
                                "history"
                            },
                            14.
                        )
                    ]
                    .spacing(12)
                    .align_y(iced::Alignment::Center),
                    match target {
                        SuggestionTarget::NewTab => c(Command::Visit(url)),
                        SuggestionTarget::Address | SuggestionTarget::StartPage => {
                            c(Command::Navigate(url))
                        }
                    },
                )
                .id(iced::widget::Id::from(format!("{prefix}-suggestion-{i}")))
                .semantics(
                    enzui_core::semantics::Semantics::new(
                        enzui_core::semantics::Role::ListBoxOption,
                    )
                    .label(title)
                    .selected(selected),
                )
                .width(Length::Fill)
                .height(52)
                .padding([6, 10])
                .style_focus(move |theme, status| {
                    let p = palette(theme);
                    iced::widget::button::Style {
                        background: Some(
                            if selected {
                                p.surface_selected
                            } else if matches!(
                                status,
                                enzui_widgets::focus_button::FocusStatus::Hovered
                            ) {
                                p.surface_hover
                            } else {
                                iced::Color::TRANSPARENT
                            }
                            .into(),
                        ),
                        text_color: p.text,
                        border: iced::Border {
                            radius: 8.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                }),
            );
        }
        links.into()
    }
    pub(crate) fn has_address_suggestions(&self) -> bool {
        !self.suggestions_for(&self.address).is_empty()
    }
    pub(crate) fn selected_address_suggestion(&self) -> Option<String> {
        self.browser
            .address_suggestions_open
            .then_some(self.browser.address_suggestion_highlight)
            .flatten()
            .and_then(|i| {
                self.suggestions_for(&self.address)
                    .get(i)
                    .map(|s| s.url.clone())
            })
    }
    pub(super) fn address_suggestions(&self, sites: &[Suggestion]) -> Element<'_, Message> {
        let height = ((sites.len() as f32 * 54.) + 8.)
            .min((self.browser.viewport.height - 155.).max(100.))
            .min(400.);
        let links = enzui_widgets::semantics_container::SemanticsContainer::new(
            self.suggestion_options(
                sites,
                self.browser.address_suggestion_highlight,
                SuggestionTarget::Address,
            ),
            enzui_core::semantics::Semantics::new(enzui_core::semantics::Role::ListBox)
                .label("Website suggestions"),
        )
        .id(iced::widget::Id::new("address-suggestion-options"));
        scrollable(links)
            .id(iced::widget::Id::new("address-suggestions"))
            .height(height)
            .width(Length::Fill)
            .into()
    }
    pub(super) fn selected_new_tab_target(&self) -> Option<String> {
        self.browser
            .suggestion_highlight
            .and_then(|index| {
                self.new_tab_suggestions()
                    .get(index)
                    .map(|entry| entry.url.clone())
            })
            .or_else(|| {
                (!self.browser.query.trim().is_empty())
                    .then(|| self.browser.saved.search.resolve(&self.browser.query))
            })
    }
    pub(super) fn new_tab_top_inset(&self) -> u16 {
        (self.browser.viewport.height * 0.2).clamp(16., 180.) as u16
    }
    pub(super) fn new_tab_dialog(&self) -> Element<'_, Message> {
        let sites = self.new_tab_suggestions();
        let search = enzui_widgets::text_field::enz_text_field(
            "",
            "Search or enter address",
            &self.browser.query,
            |q| c(Command::Query(q)),
        )
        .aria_label("New tab address or search")
        .id(iced::widget::Id::new("new-address"))
        .size(enzui_widgets::text_field::TextFieldSize::Lg)
        .subtle_focus(true)
        .radius(10.)
        .leading(container(glyph("search", 20.)).width(24).height(24))
        .trailing(ink_icon(
            "arrow-right",
            "Search or open",
            c(Command::Submit),
            32.,
            18.,
        ))
        .on_submit(c(Command::Submit))
        .into_element();
        let search = enzui_widgets::keyboard_capture::keyboard_capture_any_focus(
            search,
            |key, modifiers| suggestion_key(key, modifiers, false),
        );
        let links = self.suggestion_options(
            &sites,
            self.browser.suggestion_highlight,
            SuggestionTarget::NewTab,
        );
        let height = (134. + sites.len() as f32 * 54.).min(510.).min(
            (self.browser.viewport.height - f32::from(self.new_tab_top_inset()) - 16.).max(0.),
        );
        let links = enzui_widgets::semantics_container::SemanticsContainer::new(
            links,
            enzui_core::semantics::Semantics::new(enzui_core::semantics::Role::ListBox)
                .label("Website suggestions"),
        )
        .id(iced::widget::Id::new("new-tab-suggestion-options"));
        let body = column![
            search,
            scrollable(links)
                .id(iced::widget::Id::new("new-tab-suggestions"))
                .height(Length::Fill),
            row![
                self.container_selector(),
                Space::new().width(Length::Fill),
                text(if sites.is_empty() {
                    "Enter to open"
                } else {
                    "↑ ↓ to choose · Enter to open"
                })
                .size(11)
            ]
            .align_y(iced::Alignment::Center)
        ]
        .spacing(10)
        .width(Length::Fill)
        .height(Length::Fill);
        container(body)
            .id(iced::widget::Id::new("new-tab-dialog"))
            .padding(12)
            .width((self.browser.viewport.width - 32.).min(600.))
            .height(height)
            .style(|theme| {
                let p = palette(theme);
                container::Style {
                    background: Some(p.surface_elevated.into()),
                    border: iced::Border {
                        radius: 14.into(),
                        width: 1.,
                        color: p.border,
                    },
                    ..Default::default()
                }
            })
            .into()
    }
    fn new_tab_page(&self) -> Element<'_, Message> {
        let width = (self.content_width() - 40.0).clamp(180.0, 640.0);
        let compact = self.browser.viewport.height < 520.;
        let columns = if width < 280. {
            1
        } else if width < 520. {
            2
        } else {
            3
        };
        let search = enzui_widgets::text_field::enz_text_field(
            "",
            format!(
                "Search with {} or enter an address",
                self.browser.saved.search.active().name
            ),
            if self.browser.panel == Panel::New {
                ""
            } else {
                &self.browser.query
            },
            |q| c(Command::Query(q)),
        )
        .aria_label("Search the web")
        .id(iced::advanced::widget::Id::new("start-search"))
        .size(enzui_widgets::text_field::TextFieldSize::Lg)
        .radius(12.0)
        .subtle_focus(true)
        .leading(container(glyph("search", 18.)).width(20).height(20))
        .trailing(ink_icon(
            "arrow-right",
            "Search or open address",
            c(Command::StartGo),
            28.0,
            16.0,
        ))
        .on_submit(c(Command::StartGo))
        .into_element();
        let search = enzui_widgets::keyboard_capture::keyboard_capture_any_focus(
            search,
            |key, modifiers| suggestion_key(key, modifiers, false),
        );
        let mut tiles = column![].spacing(10);
        let sites = self.frequent_sites();
        for batch in sites.chunks(columns) {
            let mut line = row![].spacing(10);
            for (name, url) in batch {
                let target = url.clone();
                let favicon = self
                    .browser
                    .saved
                    .tabs
                    .iter()
                    .find(|t| site_name(&t.url) == site_name(url))
                    .and_then(|t| t.favicon.clone())
                    .or_else(|| self.browser.favicons.get(url));
                let mark: Element<'_, Message> = if let Some(handle) = favicon {
                    iced::widget::image(handle).width(22).height(22).into()
                } else {
                    container(glyph("globe", 22.0)).width(22).height(22).into()
                };
                line = line.push(
                    enzui_widgets::focus_button::focus_button(
                        row![
                            container(mark).padding(2),
                            text(name.clone())
                                .size(13)
                                .wrapping(iced::widget::text::Wrapping::None)
                                .ellipsis(iced::widget::text::Ellipsis::End)
                                .width(Length::Fill)
                        ]
                        .align_y(iced::Alignment::Center)
                        .spacing(10),
                        c(Command::Navigate(target)),
                    )
                    .semantics(
                        enzui_core::semantics::Semantics::new(enzui_core::semantics::Role::Button)
                            .label(format!("Open {name}")),
                    )
                    .width((width - (columns - 1) as f32 * 10.0) / columns as f32)
                    .height(if compact { 44 } else { 52 })
                    .padding(10)
                    .style_focus(|theme: &iced::Theme, status| {
                        let p = palette(theme);
                        iced::widget::button::Style {
                            background: Some(
                                if matches!(
                                    status,
                                    enzui_widgets::focus_button::FocusStatus::Hovered
                                        | enzui_widgets::focus_button::FocusStatus::Pressed
                                ) {
                                    p.surface_hover
                                } else {
                                    p.surface_elevated
                                }
                                .into(),
                            ),
                            text_color: p.text,
                            border: iced::Border {
                                color: p.border,
                                width: 0.0,
                                radius: 10.0.into(),
                            },
                            ..Default::default()
                        }
                    }),
                );
            }
            tiles = tiles.push(line);
        }
        let destinations: Element<'_, Message> = if self.browser.start_suggestions_open
            && self.browser.panel != Panel::New
            && !self.browser.address_suggestions_open
        {
            let sites = self.new_tab_suggestions();
            let links = enzui_widgets::semantics_container::SemanticsContainer::new(
                self.suggestion_options(
                    &sites,
                    self.browser.suggestion_highlight,
                    SuggestionTarget::StartPage,
                ),
                enzui_core::semantics::Semantics::new(enzui_core::semantics::Role::ListBox)
                    .label("Website suggestions"),
            )
            .id(iced::widget::Id::new("start-page-suggestion-options"));
            scrollable(links)
                .id(iced::widget::Id::new("start-page-suggestions"))
                .height(Length::Shrink.max((self.browser.viewport.height - 300.).clamp(110., 340.)))
                .into()
        } else {
            column![text("Your shortcuts").size(12), tiles]
                .spacing(10)
                .into()
        };
        let mut content = column![]
            .spacing(if compact { 14 } else { 24 })
            .width(width);
        if !compact {
            content = content.push(
                container(glyph(
                    if self.browser.private {
                        "venetian-mask"
                    } else {
                        "compass"
                    },
                    32.,
                ))
                .width(36)
                .height(36),
            );
        }
        let mut intro = column![
            text(if self.browser.private {
                "Browse privately."
            } else if compact {
                "New tab"
            } else {
                "Where would you like to go?"
            })
            .size(if compact {
                22
            } else if width < 480. {
                24
            } else {
                30
            })
        ]
        .spacing(8);
        if !compact {
            intro = intro.push(text(if self.browser.private {
                "History and site data stay out of your regular profile. Downloaded files remain on this device."
            } else { "A fresh page for your next idea." }).size(14));
        }
        content = content.push(intro).push(search).push(destinations);
        if !compact {
            content = content.push(action(
                "Customize your browser",
                "settings",
                Command::Panel(Panel::Settings),
            ));
        }
        let top = if compact {
            12.
        } else {
            ((self.browser.viewport.height - 520.) / 2.).clamp(20., 90.)
        };
        scrollable(
            container(content)
                .padding(iced::Padding {
                    top,
                    right: 0.0,
                    bottom: if compact { 12. } else { 24. },
                    left: 0.0,
                })
                .center_x(Length::Fill),
        )
        .height(Length::Fill)
        .into()
    }
    fn bookmarks_page(&self) -> Element<'_, Message> {
        let query = self.browser.bookmark_query.to_lowercase();
        let search = enzui_widgets::text_field::enz_text_field(
            "",
            "Search bookmarks",
            &self.browser.bookmark_query,
            |q| c(Command::BookmarkSearch(q)),
        )
        .aria_label("Search bookmarks")
        .leading(container(glyph("search", 16.0)).width(18).height(18))
        .into_element();
        let mut body = column![search].spacing(12).width(Length::Fill);
        let entries: Vec<_> = self
            .browser
            .saved
            .bookmarks
            .iter()
            .filter(|t| {
                format!(
                    "{} {} {}",
                    t.title,
                    t.url,
                    self.browser
                        .saved
                        .bookmark_folders
                        .get(&t.url)
                        .map(|path| path.join(" / "))
                        .unwrap_or_default()
                )
                .to_lowercase()
                .contains(&query)
            })
            .collect();
        if entries.is_empty() {
            body = body.push(
                container(
                    column![
                        container(glyph("bookmark", 32.0)).width(36).height(36),
                        text(if query.is_empty() {
                            "Keep your favorite places close."
                        } else {
                            "No matching bookmarks"
                        })
                        .size(20).font(enzui_theme::text_defaults::font(enzui_theme::FontRole::Ui, enzui_theme::WeightRung::Semibold)),
                        text(if query.is_empty() {
                            "Use the star in the address bar to save a page here."
                        } else {
                            "Try a different title or address."
                        })
                        .size(13)
                    ]
                    .spacing(16)
                    .align_x(iced::Alignment::Center),
                )
                .padding([36, 12])
                .center_x(Length::Fill),
            );
        }
        // One folder's bookmarks stand together under its name, in the order
        // they were added; those in no folder come first.
        let mut entries = entries;
        entries.sort_by_key(|tab| {
            self.browser
                .saved
                .bookmark_folders
                .get(&tab.url)
                .cloned()
                .unwrap_or_default()
        });
        let more = entries.len() > self.browser.bookmarks_limit;
        let mut last_folder = Vec::new();
        for tab in entries.into_iter().take(self.browser.bookmarks_limit) {
            let folder = self
                .browser
                .saved
                .bookmark_folders
                .get(&tab.url)
                .cloned()
                .unwrap_or_default();
            if folder != last_folder && !folder.is_empty() {
                body = body.push(
                    row![glyph("folder", 14.), text(folder.join(" / ")).size(12)]
                        .spacing(8)
                        .align_y(iced::Alignment::Center),
                );
            }
            last_folder = folder;
            let open = enzui_widgets::focus_button::focus_button(
                row![
                    self.site_icon(&tab.url, 18),
                    column![
                        text(tab.title.clone())
                            .size(14)
                            .wrapping(iced::widget::text::Wrapping::None)
                            .ellipsis(iced::widget::text::Ellipsis::End),
                        text(tab.url.clone())
                            .size(12)
                            .wrapping(iced::widget::text::Wrapping::None)
                            .ellipsis(iced::widget::text::Ellipsis::End)
                    ]
                    .spacing(4)
                    .width(Length::Fill)
                ]
                .spacing(12)
                .align_y(iced::Alignment::Center),
                c(Command::Visit(tab.url.clone())),
            )
            .width(Length::Fill)
            .padding(12);
            body = body.push(
                enzui_widgets::hover_group::enz_hover_group(
                    row![
                        open,
                        ink_icon(
                            "pencil",
                            "Edit bookmark",
                            c(Command::Bookmarks(BookmarkCommand::Edit(tab.url.clone()))),
                            28.0,
                            14.0
                        ),
                        ink_icon(
                            "trash-2",
                            "Remove bookmark",
                            c(Command::RemoveBookmark(tab.url.clone())),
                            28.0,
                            14.0
                        )
                    ]
                    .align_y(iced::Alignment::Center),
                )
                .fill_width()
                .into_element(),
            );
        }
        if more {
            body = body.push(action(
                "Show more bookmarks",
                "chevron-down",
                Command::BookmarkMore,
            ));
        }
        self.page_document_with_action(
            "Bookmarks",
            "bookmark",
            format!(
                "{} saved {}",
                self.browser.saved.bookmarks.len(),
                if self.browser.saved.bookmarks.len() == 1 {
                    "page"
                } else {
                    "pages"
                }
            ),
            body.into(),
            (!self.browser.saved.bookmarks.is_empty()).then(|| {
                action(
                    "Export…",
                    "file-down",
                    Command::Bookmarks(BookmarkCommand::Export),
                )
            }),
        )
    }
    pub(super) fn group_editor(&self, index: usize) -> Element<'_, Message> {
        let group = &self.browser.saved.groups[index];
        let width = (self.browser.viewport.width - 32.0).clamp(240.0, 360.0);
        let mut colors = row![].spacing(6);
        for (i, name) in [
            "Lavender", "Sage", "Amber", "Rose", "Sky", "Teal", "Orange", "Slate",
        ]
        .into_iter()
        .enumerate()
        {
            let selected = group.color % 8 == i;
            let color = group_ink(i, self.look().palette.is_dark());
            let outline = self.look().palette.text;
            let swatch = container(dot(color, 18.0))
                .center(Length::Fixed(28.0))
                .style(move |_| container::Style {
                    border: iced::Border {
                        radius: 14.0.into(),
                        color: if selected {
                            outline
                        } else {
                            iced::Color::TRANSPARENT
                        },
                        width: 2.0,
                    },
                    ..Default::default()
                });
            let button =
                enzui_widgets::focus_button::focus_button(swatch, c(Command::GroupColor(i)))
                    .semantics(
                        enzui_core::semantics::Semantics::new(
                            enzui_core::semantics::Role::RadioButton,
                        )
                        .label(format!("{name} group color"))
                        .selected(selected),
                    )
                    .width(30)
                    .height(30)
                    .padding(0)
                    .style_focus(|theme: &iced::Theme, _| iced::widget::button::Style {
                        background: None,
                        text_color: palette(theme).text,
                        ..Default::default()
                    });
            colors = colors.push(enzui_widgets::tooltip::FadeTooltip::new(
                button.into(),
                name,
                iced::widget::tooltip::Position::Bottom,
            ));
        }
        let header = row![
            container(glyph("folder", 18.0))
                .width(20)
                .height(20)
                .id(iced::widget::Id::new("group-editor-icon")),
            text("Tab group").size(16),
            Space::new().width(Length::Fill),
            Space::new().width(28),
        ]
        .spacing(8)
        .height(28)
        .align_y(iced::Alignment::Center);
        let body = column![
            header,
            field(
                "Group name",
                &group.name,
                |name| c(Command::GroupName(name)),
                "group-name"
            ),
            column![
                text("Color").size(12),
                container(colors).center_x(Length::Fill)
            ]
            .spacing(8),
            // Ending the group: its tabs stay, or close with it.
            row![
                enzui_widgets::button::enz_button("Ungroup", c(Command::RemoveGroup(index)))
                    .icon("ungroup")
                    .kind(enzui_widgets::button::ButtonKind::Quiet)
                    .into_element(),
                Space::new().width(Length::Fill),
                enzui_widgets::button::enz_button("Close group", c(Command::CloseGroup(index)))
                    .icon("folder-x")
                    .kind(enzui_widgets::button::ButtonKind::DestructiveOutlined)
                    .into_element(),
            ]
            .align_y(iced::Alignment::Center),
        ]
        .spacing(12);
        container(
            enzui_widgets::modal_surface::enz_modal_surface(body.into())
                .width(width)
                .padding(16.)
                .padding_top(8.)
                .radius(14.)
                .on_dismiss(|| c(Command::Dismiss))
                .close_button_id(iced::widget::Id::new("group-editor-close"))
                .close_label("Close tab group settings")
                .into_element(),
        )
        .id(iced::widget::Id::new("group-editor"))
        .into()
    }
}

pub(super) struct Suggestion {
    pub(super) title: String,
    pub(super) url: String,
    bookmarked: bool,
    shortcut: bool,
    search: bool,
}
fn ranked_suggestions(
    history: &[crate::history::Visit],
    bookmarks: &[TabInfo],
    query: &str,
) -> Vec<Suggestion> {
    struct Entry {
        title: String,
        url: String,
        host: String,
        count: usize,
        bookmarked: bool,
        recent: i64,
        homepage: bool,
        shortcut: bool,
    }
    let query = query.trim().to_lowercase();
    let mut entries = std::collections::BTreeMap::<String, Entry>::new();
    for (url, title, recent, bookmarked) in history
        .iter()
        .map(|v| (&v.url, &v.title, v.visited_at, false))
        .chain(bookmarks.iter().map(|v| (&v.url, &v.title, i64::MAX, true)))
    {
        let Ok(parsed) = url::Url::parse(url) else {
            continue;
        };
        if !matches!(parsed.scheme(), "http" | "https") {
            continue;
        }
        let Some(host) = parsed.host_str() else {
            continue;
        };
        let key = if query.is_empty() {
            parsed.origin().ascii_serialization()
        } else {
            let mut value = parsed.clone();
            value.set_fragment(None);
            value.to_string()
        };
        let e = entries.entry(key).or_insert_with(|| Entry {
            title: host.to_owned(),
            url: url.clone(),
            host: host.into(),
            count: 0,
            bookmarked: false,
            recent: i64::MIN,
            homepage: false,
            shortcut: query.is_empty(),
        });
        e.count += usize::from(!bookmarked);
        e.bookmarked |= bookmarked;
        let homepage = parsed.path() == "/" && parsed.query().is_none();
        if (query.is_empty() && homepage && !e.homepage)
            || (recent >= e.recent && (!query.is_empty() || homepage || !e.homepage))
        {
            e.recent = recent;
            e.homepage = homepage;
            e.url = if query.is_empty() {
                format!("{}/", parsed.origin().ascii_serialization())
            } else {
                parsed.to_string()
            };
            if !title.trim().is_empty() && title != url {
                e.title = if query.is_empty() {
                    cached_site_title(title, host)
                } else {
                    title.clone()
                };
            }
        }
    }
    if !query.is_empty() {
        let mut sites = std::collections::BTreeMap::<String, Entry>::new();
        for entry in entries.values() {
            if !query
                .split_whitespace()
                .all(|term| entry.host.to_lowercase().contains(term))
            {
                continue;
            }
            let Ok(parsed) = url::Url::parse(&entry.url) else {
                continue;
            };
            let root = format!("{}/", parsed.origin().ascii_serialization());
            let site = sites.entry(root.clone()).or_insert_with(|| Entry {
                title: cached_site_title(&entry.title, &entry.host),
                url: root,
                host: entry.host.clone(),
                count: 0,
                bookmarked: false,
                recent: i64::MIN,
                homepage: false,
                shortcut: true,
            });
            site.count += entry.count;
            site.bookmarked |= entry.bookmarked && entry.homepage;
            if (entry.homepage && !site.homepage)
                || (entry.recent >= site.recent && (entry.homepage || !site.homepage))
            {
                site.title = cached_site_title(&entry.title, &entry.host);
                site.homepage = entry.homepage;
                site.recent = entry.recent;
            }
        }
        for (root, site) in sites {
            entries.insert(root, site);
        }
    }
    let mut matches: Vec<_> = entries
        .into_values()
        .filter_map(|e| {
            let title = e.title.to_lowercase();
            let address = e.url.to_lowercase();
            if !query
                .split_whitespace()
                .all(|term| title.contains(term) || address.contains(term))
            {
                return None;
            }
            let relevance = if query.is_empty() {
                0
            } else {
                usize::from(e.host.trim_start_matches("www.").starts_with(&query)) * 100
                    + usize::from(title.starts_with(&query)) * 70
                    + usize::from(title.contains(&query)) * 30
            };
            let score = relevance
                + e.count.min(20) * 2
                + usize::from(e.bookmarked) * 18
                + usize::from(e.shortcut && !query.is_empty()) * 300;
            Some((score, e))
        })
        .collect();
    matches.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.recent.cmp(&a.1.recent))
            .then_with(|| a.1.title.cmp(&b.1.title))
    });
    let suggestions: Vec<_> = matches
        .into_iter()
        .take(8)
        .map(|(_, e)| Suggestion {
            title: e.title,
            url: e.url,
            bookmarked: e.bookmarked,
            shortcut: e.shortcut,
            search: false,
        })
        .collect();
    suggestions
}

fn cached_site_title(title: &str, host: &str) -> String {
    let normalize = |value: &str| {
        value
            .chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    for separator in [" | ", " — ", " – ", " - "] {
        if let Some(suffix) = title.rsplit(separator).next().filter(|s| s.len() < 60)
            && host
                .split('.')
                .any(|label| normalize(label) == normalize(suffix))
        {
            return suffix.trim().into();
        }
    }
    title.trim().into()
}

impl Saved {
    pub(super) fn migrate_library_tabs(&mut self) -> Option<InternalPage> {
        for tab in &mut self.tabs {
            if InternalPage::from_url(&tab.url) == Some(InternalPage::New) {
                tab.url.clear();
                tab.title = InternalPage::New.title().into();
            }
        }
        let active_key = self.tabs[self.active].key;
        let library =
            InternalPage::from_url(&self.tabs[self.active].url).filter(|page| page.is_library());
        self.tabs
            .retain(|tab| !InternalPage::from_url(&tab.url).is_some_and(InternalPage::is_library));
        if self.tabs.is_empty() {
            self.tabs.push(TabInfo {
                key: next_tab_key(),
                favicon: None,
                url: String::new(),
                title: "New tab".into(),
                pinned: false,
                group: None,
                container: None,
            });
        }
        self.active = self
            .tabs
            .iter()
            .position(|tab| tab.key == active_key)
            .unwrap_or_else(|| self.active.min(self.tabs.len() - 1));
        self.tab_order.retain(|entry| match entry {
            TabStripEntry::Tab(key) => self.tabs.iter().any(|tab| tab.key == *key),
            TabStripEntry::Group(_) => true,
        });
        library
    }
}

pub(super) fn suggestion_key(
    key: &iced::keyboard::Key,
    modifiers: iced::keyboard::Modifiers,
    address: bool,
) -> Option<Message> {
    if modifiers.control() || modifiers.alt() || modifiers.logo() {
        return None;
    }
    let back = match key.as_ref() {
        iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowDown) => false,
        iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowUp) => true,
        _ => return None,
    };
    Some(c(if address {
        Command::AddressSuggestionMove(back)
    } else {
        Command::SuggestionMove(back)
    }))
}
pub(super) fn next_suggestion(current: Option<usize>, n: usize, back: bool) -> Option<usize> {
    if n == 0 {
        return None;
    }
    Some(match current {
        None => {
            if back {
                n - 1
            } else {
                0
            }
        }
        Some(i) => {
            if back {
                (i + n - 1) % n
            } else {
                (i + 1) % n
            }
        }
    })
}

#[cfg(test)]
mod suggestion_tests {
    #[test]
    fn fresh_new_tab_does_not_invent_frequent_sites() {
        let mut app = crate::browser::browser_tests::app();
        app.browser.saved.history.clear();
        app.browser.saved.bookmarks.clear();
        app.browser.query.clear();
        assert!(app.new_tab_suggestions().is_empty());
        assert!(app.frequent_sites().is_empty());
        app.browser.saved.history.push(crate::history::Visit {
            title: "Visited site".into(),
            url: "https://example.org/".into(),
            ..Default::default()
        });
        assert!(!app.new_tab_suggestions().is_empty());
    }
}
