use crate::{App, browser::Panel, webview::WebViewService};
use enzui_webkit::web::WebProfile;
use std::cell::RefCell;

/// Quiet time after the visible page settles before a spare view is started.
const IDLE_SPARE_DELAY: std::time::Duration = std::time::Duration::from_millis(400);

pub(crate) struct PreparedTab {
    pub(crate) container: Option<u64>,
    pub(crate) host: RefCell<WebViewService>,
    tab: Option<u64>,
    filters_queued: bool,
}

impl App {
    pub(crate) fn profile_for_container(
        &mut self,
        container: Option<u64>,
    ) -> Result<WebProfile, String> {
        if let Some(id) = container
            && !self
                .browser
                .saved
                .containers
                .iter()
                .any(|entry| entry.id == id)
        {
            return Err(
                "This container no longer exists. Open the page in an available container.".into(),
            );
        }
        let profile = if self.browser.private {
            self.browser
                .private_profiles
                .entry(container)
                .or_insert_with(WebProfile::ephemeral)
                .clone()
        } else {
            self.browser
                .saved
                .containers
                .iter()
                .find(|entry| Some(entry.id) == container)
                .map_or_else(WebProfile::shared, |entry| {
                    WebProfile::persistent(entry.storage_name())
                })
        };
        self.routed_profile(container, profile)
            .map_err(|error| format!("Container proxy: {error}"))
    }

    fn native_profile_in_use(&self, profile: &WebProfile) -> bool {
        std::iter::once(&self.webview)
            .chain(self.browser.sleeping.iter().flatten().map(|tab| &tab.host))
            .chain(self.browser.download_hosts.iter())
            .any(|host| {
                let host = host.borrow();
                host.native.is_some() && host.profile == *profile
            })
    }

    pub(crate) fn note_blank_address_edit(&mut self) {
        let Some(active) = self.browser.saved.tabs.get(self.browser.saved.active) else {
            return;
        };
        let now = std::time::Instant::now();
        if !active.url.is_empty()
            || self.address.trim().is_empty()
            || self.webview.borrow().native.is_some()
        {
            return;
        }
        if self
            .address_preparation_until
            .is_some_and(|until| until > now)
        {
            return;
        }
        let lifetime = std::time::Duration::from_secs(15);
        self.address_preparation_until = Some(now + lifetime);
        self.pending_tasks.push(iced::Task::perform(
            async move { tokio::time::sleep(lifetime).await },
            |()| crate::Message::Chrome(crate::browser::Command::MaintenancePoll),
        ));
    }

    pub(crate) fn discard_prepared_tab(&mut self) {
        if let Some(prepared) = self.prepared_new_tab.take() {
            prepared.host.borrow_mut().close_view();
        }
    }

    /// Drops a view reserved for one blank tab. A spare that is not bound to a
    /// tab stays for the next page; reconciliation checks that it still fits.
    pub(crate) fn discard_tab_reservation(&mut self) {
        if self
            .prepared_new_tab
            .as_ref()
            .is_some_and(|prepared| prepared.tab.is_some())
        {
            self.discard_prepared_tab();
        }
    }

    /// Whether to keep one bootstrapped view ready while the browser is idle.
    ///
    /// A page opened by a link, a bookmark or another application otherwise
    /// waits for a renderer to start and pass its isolation check before its
    /// request can leave. The spare starts only once the visible page has
    /// shown its first frame or finished loading, so it never delays that
    /// first paint; an existing one is kept across later loads. Memory
    /// pressure removes it.
    fn idle_spare_wanted(&self, container: Option<u64>) -> bool {
        if !crate::webview_native::memory_pressure_normal() {
            return false;
        }
        let visible = self.webview.borrow();
        if visible.native.is_none() {
            return false;
        }
        !visible.loading
            || !visible.initial_paint.waiting()
            || self
                .prepared_new_tab
                .as_ref()
                .is_some_and(|prepared| prepared.tab.is_none() && prepared.container == container)
    }

    pub(crate) fn reconcile_prepared_tab(&mut self) {
        let now = std::time::Instant::now();
        let active = self.browser.saved.tabs.get(self.browser.saved.active);
        let mut idle_spare = false;
        let target = if self.browser.panel == Panel::New {
            Some((None, self.browser.new_tab_container))
        } else if let Some(active) = active
            && self.browser.panel == Panel::None
            && self.browser.library.is_none()
            && active.url.is_empty()
            && self.address_dirty
            && !self.address.trim().is_empty()
            && self
                .address_preparation_until
                .is_some_and(|until| until > now)
            && self.webview.borrow().native.is_none()
        {
            Some((Some(active.key), active.container))
        } else if let Some(active) = active
            && self.idle_spare_wanted(active.container)
        {
            idle_spare = true;
            Some((None, active.container))
        } else {
            None
        };
        let Some((tab, container)) = target.filter(|_| self.welcome.is_none()) else {
            self.address_preparation_until = None;
            self.prepared_new_tab_due = None;
            self.discard_prepared_tab();
            return;
        };
        if !self.browser_mode
            || !self.renderer_ready
            || !self.web_system_font_ready
            || self.window_id.is_none()
        {
            return;
        }
        let Ok(profile) = self.profile_for_container(container) else {
            self.discard_prepared_tab();
            return;
        };
        let profile_in_use = self.native_profile_in_use(&profile);
        if tab.is_none() && !profile_in_use {
            self.discard_prepared_tab();
            return;
        }
        if self.prepared_new_tab.as_ref().is_some_and(|prepared| {
            prepared.container != container
                || prepared.tab != tab
                || prepared.host.borrow().profile != profile
        }) {
            self.discard_prepared_tab();
            self.prepared_new_tab_due = None;
        }
        let Some(due) = self.prepared_new_tab_due else {
            // Paint input feedback before native allocation occupies the UI thread.
            let delay = if idle_spare {
                IDLE_SPARE_DELAY
            } else {
                std::time::Duration::from_millis(if profile_in_use { 20 } else { 60 })
            };
            self.prepared_new_tab_due = Some(now + delay);
            self.pending_tasks.push(iced::Task::perform(
                async move { tokio::time::sleep(delay).await },
                |()| crate::Message::Chrome(crate::browser::Command::MaintenancePoll),
            ));
            return;
        };
        if now < due {
            return;
        }
        if self.prepared_new_tab.is_none() {
            let mut host = WebViewService::new();
            host.profile = profile;
            host.native_viewport = self.webview.borrow().native_viewport;
            if tab.is_some() {
                host.zoom_percent = self.webview.borrow().zoom_percent;
            }
            host.open_reserved_native();
            host.hide_view();
            self.prepared_new_tab = Some(PreparedTab {
                container,
                host: RefCell::new(host),
                tab,
                filters_queued: false,
            });
        }
        let index = if tab.is_some() {
            Some(self.browser.saved.active)
        } else {
            self.browser
                .saved
                .tabs
                .iter()
                .position(|entry| entry.container == container)
        };
        if let Some(index) = index
            && (tab.is_none() || profile_in_use || crate::filters::cold_preparation_enabled())
            // A spare can live for a long time; resolve its policy once, not on every update.
            && self
                .prepared_new_tab
                .as_ref()
                .is_some_and(|prepared| !prepared.filters_queued)
        {
            let privacy = self.effective_privacy(index);
            if let Some(task) = self
                .prepared_new_tab
                .as_mut()
                .and_then(|prepared| prepared.prepare_filters(privacy))
            {
                self.pending_tasks.push(task);
            }
        }
    }

    fn take_prepared_for(
        &mut self,
        tab: Option<u64>,
        container: Option<u64>,
    ) -> Option<RefCell<WebViewService>> {
        let Ok(profile) = self.profile_for_container(container) else {
            self.discard_prepared_tab();
            return None;
        };
        let valid = self.prepared_new_tab.as_ref().is_some_and(|prepared| {
            prepared.container == container
                && prepared.tab == tab
                && prepared.host.borrow().profile == profile
        });
        if !valid {
            self.discard_prepared_tab();
            return None;
        }
        self.prepared_new_tab.take().map(|prepared| prepared.host)
    }

    pub(crate) fn take_prepared_tab(
        &mut self,
        container: Option<u64>,
    ) -> Option<RefCell<WebViewService>> {
        self.take_prepared_for(None, container)
    }

    pub(crate) fn adopt_blank_prepared_tab(&mut self) {
        if self.webview.borrow().native.is_some()
            || self
                .prepared_new_tab
                .as_ref()
                .is_none_or(|prepared| prepared.tab.is_none())
        {
            return;
        }
        let active = &self.browser.saved.tabs[self.browser.saved.active];
        if let Some(host) = self.take_prepared_for(Some(active.key), active.container) {
            self.webview.borrow_mut().close_view();
            self.webview = host;
            self.webview.borrow_mut().show_view();
            self.last_location = None;
            self.address_preparation_until = None;
        }
    }
}

impl PreparedTab {
    fn prepare_filters(
        &mut self,
        privacy: crate::privacy::Privacy,
    ) -> Option<iced::Task<crate::Message>> {
        if self.filters_queued {
            return None;
        }
        let policy = crate::filters::cached_for_privacy(&privacy).or_else(|| {
            crate::filters::cold_preparation_enabled()
                .then(|| crate::filters::bundled_for_privacy(&privacy))
                .flatten()
        });
        if let Some((mode, filters)) = policy {
            let host = self.host.borrow();
            if let (Some(native), Some(view)) = (&host.native, host.live_view) {
                let (job, receiver) = crate::native_jobs::Job::new();
                self.filters_queued = true;
                if native
                    .browse(
                        &host.router,
                        view,
                        crate::webview_native::BrowserAction::ContentFilters { mode, filters, job },
                    )
                    .is_ok()
                {
                    return Some(iced::Task::perform(
                        crate::native_jobs::receive(receiver),
                        |_| crate::Message::Chrome(crate::browser::Command::MaintenancePoll),
                    ));
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prepare(app: &mut App, container: Option<u64>) -> WebProfile {
        let profile = app
            .profile_for_container(container)
            .expect("available profile");
        let mut host = WebViewService::new();
        host.profile = profile.clone();
        app.prepared_new_tab = Some(PreparedTab {
            container,
            host: RefCell::new(host),
            tab: None,
            filters_queued: false,
        });
        profile
    }

    #[test]
    fn prepared_tab_adopts_only_the_current_private_profile() {
        let mut app = crate::browser::browser_tests::app();
        app.browser.private = true;
        let first = prepare(&mut app, None);
        let host = app.take_prepared_tab(None).expect("same private profile");
        assert_eq!(host.borrow().profile, first);
        prepare(&mut app, None);
        app.browser
            .private_profiles
            .insert(None, WebProfile::ephemeral());
        assert!(
            app.take_prepared_tab(None).is_none(),
            "another private session cannot inherit a prepared view"
        );
        assert!(app.prepared_new_tab.is_none());
    }

    #[test]
    fn prepared_tab_rejects_changed_container_or_proxy_route() {
        let mut app = crate::browser::browser_tests::app();
        let id = app.browser.saved.containers[0].id;
        prepare(&mut app, Some(id));
        assert!(
            app.take_prepared_tab(None).is_none(),
            "default profile cannot inherit container state"
        );
        let first = prepare(&mut app, Some(id));
        app.browser
            .container_routes
            .insert(id, "127.0.0.1:8080".into());
        let routed = app.profile_for_container(Some(id)).expect("routed profile");
        assert_eq!(
            first.id(),
            routed.id(),
            "route changes preserve the storage identity"
        );
        assert!(
            app.take_prepared_tab(Some(id)).is_none(),
            "storage identity alone cannot admit a view on the wrong network route"
        );
    }

    #[test]
    fn prepared_tab_is_discarded_when_the_dialog_closes() {
        let mut app = crate::browser::browser_tests::app();
        prepare(&mut app, None);
        app.prepared_new_tab_due = Some(std::time::Instant::now());
        app.browser.panel = Panel::None;
        app.reconcile_prepared_tab();
        assert!(app.prepared_new_tab.is_none());
        assert!(app.prepared_new_tab_due.is_none());
    }

    #[cfg(feature = "wpe")]
    fn show_live_page(app: &mut App) {
        let mut host = app.webview.borrow_mut();
        let view = host.live_view.expect("test host view");
        host.native = Some(crate::webview_native::NativePage::with_pending_update(
            crate::webview_native::NativeUpdate {
                view,
                page_events: Vec::new(),
                frame: None,
                gpu_frame: None,
                metadata: None,
                report: None,
                tab_reply: None,
                clipboard: None,
                selection: None,
                view_focused: None,
                error: None,
            },
        ));
    }

    #[cfg(feature = "wpe")]
    #[test]
    fn idle_spare_stays_while_a_page_is_live_and_serves_the_next_tab() {
        let mut app = crate::browser::browser_tests::app();
        show_live_page(&mut app);
        let profile = prepare(&mut app, None);
        app.browser.panel = Panel::None;
        app.reconcile_prepared_tab();
        assert!(
            app.prepared_new_tab.is_some(),
            "a spare beside a live page must outlast the dialog"
        );
        app.webview.borrow_mut().loading = true;
        app.reconcile_prepared_tab();
        assert!(
            app.prepared_new_tab.is_some(),
            "a later load must not retire an existing spare"
        );
        app.discard_tab_reservation();
        assert!(
            app.prepared_new_tab.is_some(),
            "a blank tab must leave the unbound spare for the next page"
        );
        let host = app
            .take_prepared_tab(None)
            .expect("spare for the default container");
        assert_eq!(host.borrow().profile, profile);
    }

    #[cfg(feature = "wpe")]
    #[test]
    fn idle_spare_waits_for_the_visible_page_to_paint() {
        let mut app = crate::browser::browser_tests::app();
        show_live_page(&mut app);
        app.webview.borrow_mut().loading = true;
        assert!(
            !app.idle_spare_wanted(None),
            "starting a renderer must wait for the visible page's first frame"
        );
        {
            let mut host = app.webview.borrow_mut();
            host.initial_paint
                .load(enzui_webkit::page::LoadStage::Committed);
            assert!(host.initial_paint.frame_received());
        }
        assert!(
            app.idle_spare_wanted(None),
            "a page that has painted no longer needs the spare to wait"
        );
        app.webview.borrow_mut().initial_paint = Default::default();
        app.webview.borrow_mut().loading = false;
        assert!(app.idle_spare_wanted(None));
    }

    #[cfg(feature = "wpe")]
    #[test]
    fn idle_spare_is_released_under_memory_pressure() {
        let mut app = crate::browser::browser_tests::app();
        show_live_page(&mut app);
        prepare(&mut app, None);
        app.reconcile_prepared_tab();
        assert!(app.prepared_new_tab.is_some(), "control: no pressure");
        crate::webview_native::TEST_MEMORY_PRESSURE.with(|pressure| pressure.set(true));
        app.reconcile_prepared_tab();
        crate::webview_native::TEST_MEMORY_PRESSURE.with(|pressure| pressure.set(false));
        assert!(
            app.prepared_new_tab.is_none(),
            "memory pressure must release the spare renderer"
        );
    }

    #[cfg(feature = "wpe")]
    #[test]
    fn idle_spare_for_another_container_is_not_kept() {
        let mut app = crate::browser::browser_tests::app();
        show_live_page(&mut app);
        let id = app.browser.saved.containers[0].id;
        prepare(&mut app, Some(id));
        app.webview.borrow_mut().loading = true;
        app.reconcile_prepared_tab();
        assert!(
            app.prepared_new_tab.is_none(),
            "a spare of another container cannot wait behind this tab"
        );
    }

    #[test]
    fn tab_reservation_is_dropped_when_its_blank_tab_is_replaced() {
        let mut app = crate::browser::browser_tests::app();
        prepare(&mut app, None);
        app.prepared_new_tab.as_mut().expect("reserved view").tab =
            Some(app.browser.saved.tabs[app.browser.saved.active].key);
        app.discard_tab_reservation();
        assert!(app.prepared_new_tab.is_none());
    }

    #[test]
    fn blank_preparation_adopts_only_its_original_tab() {
        let mut app = crate::browser::browser_tests::app();
        prepare(&mut app, None);
        let key = app.browser.saved.tabs[app.browser.saved.active].key;
        let prepared = app.prepared_new_tab.as_mut().expect("reserved view");
        prepared.tab = Some(key);
        prepared.host.borrow_mut().zoom_percent = 137;
        app.adopt_blank_prepared_tab();
        assert!(app.prepared_new_tab.is_none());
        assert_eq!(app.webview.borrow().zoom_percent, 137);

        prepare(&mut app, None);
        app.prepared_new_tab.as_mut().expect("reserved view").tab = Some(key.wrapping_add(1));
        app.prepared_new_tab
            .as_ref()
            .expect("reserved view")
            .host
            .borrow_mut()
            .zoom_percent = 191;
        app.adopt_blank_prepared_tab();
        assert!(
            app.prepared_new_tab.is_none(),
            "a reservation for another tab must be retired"
        );
        assert_eq!(
            app.webview.borrow().zoom_percent,
            137,
            "another tab cannot replace this host"
        );
    }

    #[test]
    fn blank_preparation_cannot_be_adopted_as_a_new_tab() {
        let mut app = crate::browser::browser_tests::app();
        prepare(&mut app, None);
        app.prepared_new_tab.as_mut().expect("reserved view").tab =
            Some(app.browser.saved.tabs[app.browser.saved.active].key);
        assert!(app.take_prepared_tab(None).is_none());
        assert!(app.prepared_new_tab.is_none());
    }

    #[test]
    fn blank_preparation_expires_without_replacing_the_current_host() {
        let mut app = crate::browser::browser_tests::app();
        let key = app.browser.saved.tabs[app.browser.saved.active].key;
        app.browser.saved.tabs[app.browser.saved.active].url.clear();
        app.address = "https://example.com/".into();
        app.address_dirty = true;
        app.address_preparation_until =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(1));
        prepare(&mut app, None);
        app.prepared_new_tab.as_mut().expect("reserved view").tab = Some(key);
        app.browser.panel = Panel::None;
        app.reconcile_prepared_tab();
        assert!(app.prepared_new_tab.is_none());
        assert!(app.address_preparation_until.is_none());
        assert!(app.webview.borrow().native.is_none());
    }
}
