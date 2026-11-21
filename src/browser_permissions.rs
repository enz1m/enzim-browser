use super::*;
use enzui_webkit::{
    page::PageRequestId, site_permissions::PermissionOrigin, web::policy::PermissionKind,
};
use iced::widget::{column, text};
#[derive(Clone, Debug)]
pub(crate) enum PermissionCommand {
    Answer(PageRequestId, bool),
    Remember(PageRequestId, bool),
    Choice(&'static str, usize),
    Reset(crate::site_permissions::Choice),
}
/// A request refused for the rest of the session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Denial {
    container: Option<u64>,
    requesting: String,
    top: String,
    kind: PermissionKind,
}
/// The remembered choices a block of `kind` stands for; kinds without
/// a remembered choice are refused for the session only.
fn origin_for_denials(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()
        .and_then(|url| PermissionOrigin::parse(&url.origin().ascii_serialization()))
        .map(|origin| origin.as_str().to_owned())
}
fn remembered_kinds(kind: PermissionKind) -> &'static [&'static str] {
    match kind {
        PermissionKind::Camera => &["camera"],
        PermissionKind::Microphone => &["microphone"],
        PermissionKind::CameraAndMicrophone => &["camera", "microphone"],
        PermissionKind::Geolocation => &["location"],
        PermissionKind::Notifications => &["notifications"],
        _ => &[],
    }
}
impl Denial {
    pub(super) fn new(
        container: Option<u64>,
        request: &enzui_webkit::site_permissions::SitePermissionRequest,
    ) -> Self {
        Self {
            container,
            requesting: request.permission.requesting_origin.as_str().to_owned(),
            top: request.permission.top_origin.as_str().to_owned(),
            kind: request.permission.kind,
        }
    }
}
impl App {
    fn denial(&self, request: &enzui_webkit::site_permissions::SitePermissionRequest) -> Denial {
        Denial::new(self.browser.saved.tabs[self.browser.saved.active].container, request)
    }
    /// Refuses `request` for the rest of the session.
    pub(super) fn deny_permission(
        &mut self,
        request: &enzui_webkit::site_permissions::SitePermissionRequest,
    ) {
        let denial = self.denial(request);
        if !self.browser.permission_denied.contains(&denial) {
            self.browser.permission_denied.push(denial);
        }
    }
    /// Lets a site of `container` whose choices changed ask again.
    fn forget_denials(&mut self, container: Option<u64>, top: &str) {
        self.browser
            .permission_denied
            .retain(|denial| !(denial.container == container && denial.top == top));
    }
    pub(super) fn permission_command(&mut self, command: PermissionCommand) -> Task<Message> {
        match command {
            PermissionCommand::Reset(choice) => {
                self.browser
                    .saved
                    .site_permissions
                    .retain(|saved| saved != &choice);
                self.forget_denials(choice.container, &choice.top);
                self.apply_container_permissions(choice.container);
                self.browser.save();
            }
            PermissionCommand::Answer(id, allow) | PermissionCommand::Remember(id, allow) => {
                let remember = matches!(command, PermissionCommand::Remember(..));
                if let Some(request) = self
                    .browser
                    .permission
                    .as_ref()
                    .filter(|request| request.id == id)
                    .cloned()
                {
                    self.browser.permission = None;
                    self.browser.panel = Panel::None;
                    self.submit_browser_action(BrowserAction::PermissionAnswer(id, allow));
                    if !allow {
                        self.deny_permission(&request);
                    }
                    // Notifications remember either answer; a device or
                    // location block is remembered too, so the site cannot
                    // ask again on its next page. "Allow once" stays once.
                    let kinds = remembered_kinds(request.permission.kind);
                    if (remember || !allow) && !kinds.is_empty() {
                        let container =
                            self.browser.saved.tabs[self.browser.saved.active].container;
                        let requesting = request.permission.requesting_origin.as_str().to_owned();
                        let top = request.permission.top_origin.as_str().to_owned();
                        for kind in kinds {
                            self.browser.saved.site_permissions.retain(|choice| {
                                !(choice.container == container
                                    && choice.requesting == requesting
                                    && choice.top == top
                                    && choice.kind == *kind)
                            });
                            self.browser
                                .saved
                                .site_permissions
                                .push(crate::site_permissions::Choice {
                                    container,
                                    requesting: requesting.clone(),
                                    top: top.clone(),
                                    kind: (*kind).into(),
                                    allowed: allow,
                                });
                        }
                        self.apply_container_permissions(container);
                        self.browser.save();
                    }
                }
            }
            PermissionCommand::Choice(kind, index) => {
                let tab = &self.browser.saved.tabs[self.browser.saved.active];
                let origin = url::Url::parse(&tab.url)
                    .ok()
                    .and_then(|url| PermissionOrigin::parse(&url.origin().ascii_serialization()))
                    .map(|origin| origin.as_str().to_owned());
                if let Some(origin) = origin {
                    let choices = &mut self.browser.saved.site_permissions;
                    choices.retain(|c| {
                        !(c.container == tab.container
                            && c.requesting == origin
                            && c.top == origin
                            && c.kind == kind)
                    });
                    if index > 0 {
                        choices.push(crate::site_permissions::Choice {
                            container: tab.container,
                            requesting: origin.clone(),
                            top: origin,
                            kind: kind.into(),
                            allowed: index == 1,
                        });
                    }
                    let container = tab.container;
                    let top = origin_for_denials(&self.browser.saved.tabs[self.browser.saved.active].url);
                    if let Some(top) = top {
                        self.forget_denials(container, &top);
                    }
                    self.apply_container_permissions(container);
                    self.browser.save();
                }
            }
        }
        Task::none()
    }
    fn apply_container_permissions(&mut self, container: Option<u64>) {
        let choices = self
            .browser
            .saved
            .site_permissions
            .iter()
            .filter(|choice| choice.container == container)
            .cloned()
            .collect::<Vec<_>>();
        for (index, _) in self
            .browser
            .saved
            .tabs
            .iter()
            .enumerate()
            .filter(|(_, tab)| tab.container == container)
        {
            let host = if index == self.browser.saved.active {
                Some(&self.webview)
            } else {
                self.browser
                    .sleeping
                    .get(index)
                    .and_then(Option::as_ref)
                    .map(|tab| &tab.host)
            };
            if let Some(host) = host {
                let host = host.borrow();
                if let (Some(native), Some(view)) = (&host.native, host.live_view) {
                    let _ = native.browse(
                        &host.router,
                        view,
                        BrowserAction::PermissionChoices(choices.clone()),
                    );
                }
            }
        }
    }
    pub(super) fn permission_settings_dialog(&self) -> Element<'_, Message> {
        use iced::widget::{container, row};
        let mut rows = column![].spacing(8);
        if self.browser.saved.site_permissions.is_empty() {
            rows = rows.push(text("No remembered permissions. Websites will ask before using your camera, microphone or location. Notification prompts follow your notification settings.").size(13));
        }
        for choice in &self.browser.saved.site_permissions {
            let container_name = choice
                .container
                .and_then(|id| {
                    self.browser
                        .saved
                        .containers
                        .iter()
                        .find(|container| container.id == id)
                })
                .map_or("Default", |container| container.name.as_str());
            let detail = format!(
                "{} · {} · {container_name}",
                choice.kind,
                if choice.allowed { "Allowed" } else { "Blocked" }
            );
            let mut label = column![
                text(&choice.requesting).size(14),
                text(detail)
                    .size(12)
                    .color(palette(&self.theme()).text_muted)
            ]
            .spacing(4)
            .width(Length::Fill);
            if choice.requesting != choice.top {
                label = label.push(text(format!("Embedded in {}", choice.top)).size(12));
            }
            rows = rows.push(
                row![
                    label,
                    action(
                        "Reset",
                        "rotate-ccw",
                        Command::Permission(PermissionCommand::Reset(choice.clone()))
                    )
                ]
                .spacing(12)
                .align_y(iced::Alignment::Center),
            );
            rows = rows.push(settings_divider());
        }
        container(column![
            heading("sliders-horizontal", "Site permissions", "Saved website choices"),
            text("Reset a saved choice to let the website ask again. Camera and microphone access already in use stops when you reload the page.").size(13),
            iced::widget::container(scrollable(rows).height(Length::Shrink))
                .height(((self.browser.saved.site_permissions.len().max(1) * 88) as f32).min((self.browser.viewport.height - 210.).clamp(80., 420.))),
        ].spacing(10)).padding(12).width((self.browser.viewport.width - 32.).min(540.))
            .style(|theme| { let p = palette(theme); container::Style {
                background: Some(p.surface_elevated.into()),
                border: iced::Border { radius: 14.into(), width: 1., color: p.border },
                ..Default::default()
            }}).into()
    }
    pub(super) fn site_permissions_panel(&self) -> Element<'_, Message> {
        let tab = &self.browser.saved.tabs[self.browser.saved.active];
        let mut body = column![heading(
            "sliders-horizontal",
            "Site permissions",
            site_name(&tab.url)
        )]
        .spacing(10);
        if let Some(request) = &self.browser.permission {
            let kind = match request.permission.kind {
                PermissionKind::Camera => "camera",
                PermissionKind::Microphone => "microphone",
                PermissionKind::CameraAndMicrophone => "camera and microphone",
                PermissionKind::Geolocation => "location",
                PermissionKind::Notifications => "notifications",
                PermissionKind::MediaKeySystem => "encrypted media",
                PermissionKind::ScreenCapture => "screen",
                _ => "device",
            };
            let question = match request.permission.kind {
                PermissionKind::Notifications => "Show desktop notifications?".to_owned(),
                PermissionKind::MediaKeySystem => "Allow encrypted media playback?".to_owned(),
                PermissionKind::ScreenCapture => "Let this site see your screen?".to_owned(),
                _ => format!("Allow access to your {kind}?"),
            };
            let remembered = request.permission.kind == PermissionKind::Notifications;
            let answer = |allow| {
                Command::Permission(if remembered {
                    PermissionCommand::Remember(request.id, allow)
                } else {
                    PermissionCommand::Answer(request.id, allow)
                })
            };
            let mut prompt = column![
                text(question).size(16),
                text(format!(
                    "Requested by {}",
                    request.permission.requesting_origin.as_str()
                ))
                .size(12),
                text(format!("Page: {}", request.permission.top_origin.as_str())).size(12),
            ]
            .spacing(12);
            if request.permission.kind == PermissionKind::ScreenCapture {
                prompt = prompt.push(
                    text(
                        "It sees only the screen or window chosen in your desktop's own dialog. \
                         The site has to ask again next time.",
                    )
                    .size(12),
                );
            }
            prompt = prompt.push(
                iced::widget::row![
                    action("Block", "shield-off", answer(false)),
                    primary(
                        if remembered { "Allow" } else { "Allow once" },
                        "check",
                        answer(true)
                    )
                ]
                .spacing(10),
            );
            body = body.push(popup_section("Permission request", prompt));
        }
        let origin = url::Url::parse(&tab.url)
            .ok()
            .and_then(|u| PermissionOrigin::parse(&u.origin().ascii_serialization()))
            .map(|o| o.as_str().to_owned());
        if origin.is_none() {
            return column![
                heading(
                    "sliders-horizontal",
                    "Browser page",
                    "No website permissions"
                ),
                text("This page is part of the browser and does not request website permissions.")
                    .size(13)
            ]
            .spacing(10)
            .into();
        }
        let mut rows = column![].spacing(4);
        for (kind, label, icon_name) in [
            ("camera", "Camera", "camera"),
            ("microphone", "Microphone", "mic"),
            ("location", "Location", "map-pin"),
            ("notifications", "Notifications", "bell"),
        ] {
            let choice = self.browser.saved.site_permissions.iter().find(|c| {
                c.container == tab.container
                    && Some(&c.top) == origin.as_ref()
                    && c.requesting == c.top
                    && c.kind == kind
            });
            let index = choice.map_or(0, |choice| if choice.allowed { 1 } else { 2 });
            let labels = vec!["Ask".into(), "Allow".into(), "Block".into()];
            let select = enzui_widgets::combo_box::enz_combo_box(
                "",
                labels.clone(),
                Some(labels[index].clone()),
                move |_| c(Command::Permission(PermissionCommand::Choice(kind, 0))),
            )
            .id_prefix(format!("site-permission-{kind}"))
            .aria_label(format!("{label} permission"))
            .selected_index(Some(index))
            .on_select_index(move |index| {
                c(Command::Permission(PermissionCommand::Choice(kind, index)))
            })
            .width(110)
            .into_element();
            rows = rows.push(
                enzui_widgets::preferences_row::enz_preferences_row(label)
                    .icon(icon_name)
                    .tokens(popup_row_tokens())
                    .control(select)
                    .accessory_width(110.)
                    .label_min_width(100.)
                    .into_element(),
            );
        }
        body = body.push(rows);
        body.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_refused_request_is_not_asked_again_until_the_sites_choices_change() {
        let mut app = crate::browser::browser_tests::app();
        let active = app.browser.saved.active;
        app.browser.saved.tabs[active].url = "https://example.test/".into();
        let request = |id, kind| enzui_webkit::site_permissions::SitePermissionRequest {
            id: PageRequestId::new(id),
            permission: enzui_webkit::site_permissions::SitePermission {
                requesting_origin: PermissionOrigin::parse("https://example.test").expect("origin"),
                top_origin: PermissionOrigin::parse("https://example.test").expect("origin"),
                kind,
            },
        };
        let camera = request(1, PermissionKind::Camera);
        app.browser.permission = Some(camera.clone());
        let _ = app.permission_command(PermissionCommand::Answer(camera.id, false));
        assert!(
            app.browser.saved.site_permissions.iter().any(|choice| choice.kind == "camera" && !choice.allowed),
            "a camera block is remembered"
        );
        assert!(app.browser.permission_denied.contains(&app.denial(&request(2, PermissionKind::Camera))));
        let screen = request(3, PermissionKind::ScreenCapture);
        app.browser.permission = Some(screen.clone());
        app.browser.panel = Panel::Permissions;
        let _ = app.update(c(Command::Dismiss));
        assert!(
            app.browser.permission_denied.contains(&app.denial(&request(4, PermissionKind::ScreenCapture))),
            "a dismissed prompt is refused for the session"
        );
        let allowed = request(5, PermissionKind::Geolocation);
        app.browser.permission = Some(allowed.clone());
        let _ = app.permission_command(PermissionCommand::Answer(allowed.id, true));
        assert!(!app.browser.saved.site_permissions.iter().any(|choice| choice.kind == "location"), "allow once stays once");
        let _ = app.permission_command(PermissionCommand::Choice("camera", 0));
        assert!(app.browser.permission_denied.is_empty(), "changing the site's choices lets it ask again");
        assert!(!app.browser.saved.site_permissions.iter().any(|choice| choice.kind == "camera"));
    }
    #[test]
    fn permission_reset_preserves_other_container_and_embedded_origin_choices() {
        let mut app = crate::browser::browser_tests::app();
        let choice = crate::site_permissions::Choice {
            container: None,
            requesting: "https://example.com".into(),
            top: "https://example.com".into(),
            kind: "camera".into(),
            allowed: true,
        };
        let mut container = choice.clone();
        container.container = Some(42);
        let mut embedded = choice.clone();
        embedded.requesting = "https://embedded.example.com".into();
        app.browser.saved.site_permissions =
            vec![choice.clone(), container.clone(), embedded.clone()];
        let _ = app.permission_command(PermissionCommand::Reset(choice.clone()));
        assert_eq!(
            app.browser.saved.site_permissions,
            vec![container.clone(), embedded.clone()],
            "reset is scoped to one exact remembered choice"
        );
        let _ = app.permission_command(PermissionCommand::Reset(choice));
        assert_eq!(
            app.browser.saved.site_permissions,
            vec![container, embedded],
            "replayed reset cannot affect another permission"
        );
    }
}
