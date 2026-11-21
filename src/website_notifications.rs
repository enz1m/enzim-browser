use enzui_core::{
    app_data::AppId,
    portal::notification::{
        DesktopNotification, DesktopNotificationClient, NotificationAction, NotificationActionId,
        NotificationBackend, NotificationError, NotificationEvent, NotificationIcon,
        NotificationId, NotificationShutdown, freedesktop::FreedesktopNotificationBackend,
    },
};
use enzui_webkit::{
    page::NotificationRequest,
    web::{ViewId, WebProfile},
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::Instant;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Policy {
    #[default]
    Ask,
    #[serde(other)]
    Block,
}
#[derive(Clone, Debug)]
pub(crate) enum Command {
    Policy(usize),
    Wake,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Target {
    pub(crate) tab: u64,
    pub(crate) view: ViewId,
    pub(crate) profile: WebProfile,
    pub(crate) native_id: u64,
}
#[derive(Debug)]
pub(crate) enum Relay {
    Click(Target, Option<enzui_core::activation::ActivationContext>),
    Close(Target),
}
struct Shown {
    id: NotificationId,
    target: Target,
    origin: String,
    tag: Option<String>,
}
type Client = DesktopNotificationClient<FreedesktopNotificationBackend>;
#[derive(Default)]
pub(crate) struct State {
    client: Option<Client>,
    shown: VecDeque<Shown>,
    owner_seen: bool,
    activation: enzui_core::activation::ActivationBroker,
}
impl State {
    fn client(&mut self) -> Result<&mut Client, String> {
        if self.client.is_none() {
            let id = AppId::new("org.enzim.Browser")
                .map_err(|_| "Invalid desktop notification identity.".to_owned())?;
            let backend = FreedesktopNotificationBackend::new(id.clone())
                .map_err(|_| "Desktop notifications are unavailable.".to_owned())?;
            self.client = Some(DesktopNotificationClient::new(id, backend));
        }
        self.client
            .as_mut()
            .ok_or_else(|| "Desktop notifications are unavailable.".into())
    }
    pub(crate) fn shutdown(&mut self) -> Option<iced::Task<()>> {
        let mut client = self.client.take()?;
        self.shown.clear();
        Some(iced::Task::perform(
            async move {
                let Some(signal) = client.backend().notification_completion_signal() else {
                    return;
                };
                let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
                let mut seen = signal.generation();
                let mut pending = client
                    .begin_shutdown(NotificationShutdown::Withdraw)
                    .into_iter()
                    .filter_map(Result::ok)
                    .map(|receipt| receipt.operation)
                    .collect::<Vec<_>>();
                loop {
                    while let Some(event) = client.poll_event() {
                        match event {
                            NotificationEvent::Operation { receipt, .. } => {
                                pending.retain(|id| *id != receipt.operation);
                            }
                            NotificationEvent::ServiceChanged {
                                available: false, ..
                            } => return,
                            _ => {}
                        }
                    }
                    if pending.is_empty() {
                        return;
                    }
                    match tokio::time::timeout_at(deadline, signal.wait_after(seen)).await {
                        Ok(generation) => seen = generation,
                        Err(_) => return,
                    }
                }
            },
            |()| (),
        ))
    }
    pub(crate) fn subscription(&self) -> iced::Subscription<crate::Message> {
        self.client
            .as_ref()
            .map_or_else(iced::Subscription::none, |client| {
                client.wake_subscription().map(|_| {
                    crate::browser::c(crate::browser::Command::Notifications(Command::Wake))
                })
            })
    }
    fn retire(&mut self, shown: Shown) -> Relay {
        if let Some(client) = &mut self.client {
            let _ = client.withdraw(&shown.id);
        }
        Relay::Close(shown.target)
    }
    pub(crate) fn show(
        &mut self,
        target: Target,
        request: NotificationRequest,
        policy: Policy,
        private: bool,
    ) -> (Vec<Relay>, Option<String>) {
        let Some(origin) = request
            .origin
            .as_ref()
            .map(|origin| origin.as_str().to_owned())
        else {
            return (vec![Relay::Close(target)], None);
        };
        if policy == Policy::Block || private {
            return (vec![Relay::Close(target)], None);
        }
        let mut relays = Vec::new();
        if let Some(tag) = &request.tag
            && let Some(index) = self.shown.iter().position(|shown| {
                shown.target.profile == target.profile
                    && shown.origin == origin
                    && shown.tag.as_ref() == Some(tag)
            })
            && let Some(old) = self.shown.remove(index)
        {
            relays.push(self.retire(old));
        }
        while self.shown.len() >= 32 {
            if let Some(old) = self.shown.pop_front() {
                relays.push(self.retire(old));
            }
        }
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let id = NotificationId::new(format!("website-{}-{serial}", std::process::id()))
            .expect("bounded native notification id");
        let title = bounded(&request.title, 256).replace('\0', "");
        let body = format!(
            "{origin}\n{}",
            bounded(&request.body, 12 * 1024).replace('\0', "")
        );
        let notification = DesktopNotification::new(
            id.clone(),
            if title.is_empty() {
                "Website notification".into()
            } else {
                title
            },
        )
        .body(body)
        .icon(NotificationIcon::Themed(vec![
            "web-browser-symbolic".into(),
        ]))
        .default_action(NotificationAction::new(
            NotificationActionId::new("open").expect("static action id"),
        ));
        let result = self.client().and_then(|client| match client.send(notification) {
            Ok(_) | Err(NotificationError::Deferred) => Ok(()),
            Err(_) => Err(
                "Desktop notification delivery was not accepted. Check your desktop's notification settings.".into()
            ),
        });
        match result {
            Ok(()) => {
                self.shown.push_back(Shown {
                    id,
                    target,
                    origin,
                    tag: request.tag,
                });
                (relays, None)
            }
            Err(error) => {
                relays.push(Relay::Close(target));
                (relays, Some(error))
            }
        }
    }
    pub(crate) fn native_closed(&mut self, view: ViewId, id: u64) {
        if let Some(index) = self
            .shown
            .iter()
            .position(|shown| shown.target.view == view && shown.target.native_id == id)
            && let Some(shown) = self.shown.remove(index)
        {
            self.retire(shown);
        }
    }
    pub(crate) fn drain(&mut self) -> Vec<Relay> {
        let mut relays = Vec::new();
        while let Some(event) = self.client.as_mut().and_then(Client::poll_event) {
            match event {
                NotificationEvent::Action(mut action) => {
                    if let Some(shown) = self.shown.iter().find(|shown| shown.id == action.notification_id)
                        && let Ok(context) = self.activation.admit_notification_invocation(&mut action, Instant::now())
                    {
                        relays.push(Relay::Click(shown.target.clone(), context));
                    }
                }
                NotificationEvent::Closed { notification_id, .. } => {
                    if let Some(index) = self.shown.iter().position(|shown| shown.id == notification_id)
                        && let Some(shown) = self.shown.remove(index)
                    {
                        relays.push(Relay::Close(shown.target));
                    }
                }
                NotificationEvent::ServiceChanged { available, .. } => {
                    if self.owner_seen || !available {
                        relays.extend(self.clear());
                    }
                    self.owner_seen = true;
                }
                NotificationEvent::ActionDeliveryUnavailable => relays.extend(self.clear()),
                NotificationEvent::Operation { receipt, outcome }
                    if matches!(receipt.kind, enzui_core::portal::notification::NotificationReceiptKind::Send(_))
                        && matches!(outcome,
                            enzui_core::portal::notification::NotificationOperationOutcome::Unavailable
                            | enzui_core::portal::notification::NotificationOperationOutcome::Unsupported(_)
                            | enzui_core::portal::notification::NotificationOperationOutcome::Denied
                            | enzui_core::portal::notification::NotificationOperationOutcome::Failed
                        ) =>
                {
                    if let Some(index) = self.shown.iter().position(|shown| shown.id == receipt.notification_id)
                        && let Some(shown) = self.shown.remove(index)
                    {
                        relays.push(self.retire(shown));
                    }
                }
                _ => {}
            }
        }
        relays
    }
    pub(crate) fn clear(&mut self) -> Vec<Relay> {
        let shown = self.shown.drain(..).collect::<Vec<_>>();
        shown.into_iter().map(|shown| self.retire(shown)).collect()
    }
    pub(crate) fn tab_closed(&mut self, tab: u64) {
        let mut index = 0;
        while index < self.shown.len() {
            if self.shown[index].target.tab == tab {
                if let Some(shown) = self.shown.remove(index) {
                    self.retire(shown);
                }
            } else {
                index += 1;
            }
        }
    }
}
fn bounded(text: &str, max: usize) -> String {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.get(..end).unwrap_or_default().into()
}

impl crate::App {
    pub(crate) fn notification_command(&mut self, command: Command) -> iced::Task<crate::Message> {
        match command {
            Command::Wake => {
                let relays = self.browser.desktop_notifications.drain();
                self.relay_notifications(relays);
            }
            Command::Policy(index) => {
                if self.browser.private {
                    return iced::Task::none();
                }
                self.browser.saved.notifications = if index == 1 {
                    Policy::Block
                } else {
                    Policy::Ask
                };
                self.apply_notification_policy();
                self.browser.save();
            }
        }
        iced::Task::none()
    }
    pub(crate) fn apply_notification_policy(&mut self) {
        let enabled = !self.browser.private && self.browser.saved.notifications == Policy::Ask;
        for host in std::iter::once(&self.webview)
            .chain(self.browser.sleeping.iter().flatten().map(|tab| &tab.host))
        {
            let host = host.borrow();
            if let (Some(native), Some(view)) = (&host.native, host.live_view) {
                let _ = native.browse(
                    &host.router,
                    view,
                    crate::webview_native::BrowserAction::NotificationsEnabled(enabled),
                );
            }
        }
        if !enabled {
            let relays = self.browser.desktop_notifications.clear();
            self.relay_notifications(relays);
        }
    }
    pub(crate) fn relay_notifications(&mut self, relays: Vec<Relay>) {
        for relay in relays {
            let (target, context, clicked) = match relay {
                Relay::Click(target, context) => (target, context, true),
                Relay::Close(target) => (target, None, false),
            };
            let Some(index) = self
                .browser
                .saved
                .tabs
                .iter()
                .position(|tab| tab.key == target.tab)
            else {
                continue;
            };
            let host = if index == self.browser.saved.active {
                Some(&self.webview)
            } else {
                self.browser
                    .sleeping
                    .get(index)
                    .and_then(Option::as_ref)
                    .map(|tab| &tab.host)
            };
            let valid = if let Some(host) = host {
                let host = host.borrow();
                if host.live_view == Some(target.view)
                    && host.profile == target.profile
                    && let Some(native) = &host.native
                {
                    let action = if clicked {
                        crate::webview_native::BrowserAction::NotificationClick(target.native_id)
                    } else {
                        crate::webview_native::BrowserAction::NotificationClose(target.native_id)
                    };
                    let _ = native.browse(&host.router, target.view, action);
                    true
                } else {
                    false
                }
            } else {
                false
            };
            if clicked && valid {
                let task = self.browser_command(crate::browser::Command::Select(index));
                self.pending_tasks.push(task);
                if let Some(window) = self.window_id {
                    let task = if let Some(context) = context {
                        context.activate(window, Instant::now()).map(|_| {
                            crate::browser::c(crate::browser::Command::Notifications(Command::Wake))
                        })
                    } else {
                        iced::window::gain_focus(window)
                    };
                    self.pending_tasks.push(task);
                }
            }
        }
    }
    pub(crate) fn notification_settings(&self) -> iced::Element<'_, crate::Message> {
        let labels = vec!["Ask before allowing".into(), "Never allow".into()];
        let selected = usize::from(self.browser.saved.notifications == Policy::Block);
        let control = enzui_widgets::combo_box::enz_combo_box(
            "",
            labels.clone(),
            Some(labels[selected].clone()),
            |_| crate::browser::c(crate::browser::Command::Notifications(Command::Policy(0))),
        )
        .id_prefix("website-notifications-policy")
        .selected_index(Some(selected))
        .on_select_index(|index| {
            crate::browser::c(crate::browser::Command::Notifications(Command::Policy(
                index,
            )))
        })
        .into_element();
        enzui_widgets::preferences_group::enz_preferences_group(enzui_widgets::preferences_row::enz_preferences_row("Website notifications").icon("bell").subtitle("Let websites ask for desktop notifications, or block every request. Your desktop controls Do Not Disturb.").control(control).accessory_width(205.).into_element()).title("Notifications").bordered(true).into_element()
    }
}
