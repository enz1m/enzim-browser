//! Browser message dispatch and native publication.

use crate::{App, Message, drain_a11y_actions};
use iced::Task;

impl App {
    fn background_visual_revision(&self) -> u64 {
        self.browser
            .sleeping
            .iter()
            .flatten()
            .map(|tab| &tab.host)
            .chain(self.browser.download_hosts.iter())
            .fold(0u64, |revision, host| {
                revision.wrapping_add(host.borrow().native_visual_revision)
            })
    }

    pub(crate) fn update(&mut self, message: Message) -> Task<Message> {
        if self.welcome.is_some()
            && matches!(&message, Message::BrowserGo | Message::BrowserAddress(_))
        {
            return Task::none();
        }
        let keyboard_blocked = self.webpage_keyboard_blocked();
        let native_ready = matches!(message, Message::WebViewNativeReady);
        let dispatched_publication =
            native_ready && self.native_dispatch_publication.replace(false);
        let native_ready = native_ready && !dispatched_publication;
        let input_only = self.browser_mode
            && !self.native_dispatch_publication.get()
            && matches!(
                &message,
                Message::Chrome(
                    crate::browser::Command::Pointer(_)
                        | crate::browser::Command::Input(crate::browser_input::InputCommand::Tick(
                            _
                        ))
                ) | Message::WebViewInput(
                    enzui_webkit::input::InputEvent::WheelScrolled { .. }
                        | enzui_webkit::input::InputEvent::WheelScrolledAt { .. }
                        | enzui_webkit::input::InputEvent::PointerMoved { .. }
                )
            );
        #[cfg(feature = "benchmark")]
        let input_only = input_only && std::env::var_os("ENZIM_BENCH_INPUT_REDRAW").is_none();
        let quiet_candidate = native_ready || input_only;
        #[cfg(feature = "benchmark")]
        let quiet_candidate = quiet_candidate && std::env::var_os("ENZUI_BENCH_NOQUIET").is_none();
        let previous_chrome = self.browser.chrome_revision;
        let previous_toasts = self.browser.toasts.revision();
        let previous_background = self.background_visual_revision();
        let previous_visual = self.webview.borrow().native_visual_revision;
        let discarded_gpu = self.webview.borrow_mut().discard_refused_gpu_frame();
        crate::webview_native::pump_main_thread();
        // Actions applied in the publishing window; enqueue and wake are
        // separate boundaries, so unwoken work applies on the next update.
        let mut task = Task::none();
        let a11y_tasks = if self.window_id.is_none() {
            drain_a11y_actions()
        } else {
            Vec::new()
        };
        let had_a11y = !a11y_tasks.is_empty();
        if had_a11y {
            task = Task::batch(a11y_tasks);
        }

        match message {
            Message::Chrome(command) => task = self.browser_command(command),
            Message::Assistant(msg) => task = self.assistant_update(msg),
            Message::WebSystemFont(font) => {
                self.web_system_font_ready = true;
                self.web_system_font = font;
                self.flush_browser_navigation();
            }
            Message::Theme(event) => {
                task = self.theme.update(event).map(Message::Theme);
                self.appearance_theme_changed();
                task = Task::batch([
                    task,
                    self.appearance_backdrop(),
                    self.appearance_corners(),
                ]);
                self.browser.container_menu_model =
                    crate::browser::containers::container_menu_model(
                        &self.browser.saved.containers,
                        self.browser.new_tab_container,
                        self.look().palette.is_dark(),
                    );
            }
            Message::A11yActivatePageNode(id) => {
                self.webview.borrow_mut().activate_published_node(&id);
            }
            message => task = self.update_webview(message, task),
        }
        self.reconcile_prepared_tab();
        self.settle_page_prompts();
        self.settle_color();
        self.settle_date();
        self.settle_fullscreen();
        task = Task::batch([task, self.refresh_search_suggestions()]);
        crate::webview_native::pump_main_thread();
        self.sync_web_theme();
        if input_only {
            self.sync_browser_state();
            let stable_input = !had_a11y
                && self.webview.borrow().native_visual_revision == previous_visual
                && self.background_visual_revision() == previous_background
                && !self.browser_events_pending();
            #[cfg(feature = "benchmark")]
            let stable_input = stable_input && self.input_sync_fast;
            if !stable_input {
                self.sync_browser_metadata();
            }
        } else {
            self.sync_browser();
        }
        if self.browser_mode {
            let service = self.webview.borrow();
            if service.location != self.last_location {
                if self.browser.library.is_none()
                    && !self.address_dirty
                    && let Some(uri) = &service.location
                    && !uri.is_empty()
                    && !uri.starts_with("data:")
                {
                    self.address = self
                        .local_files
                        .display_uri(uri)
                        .unwrap_or_else(|| uri.clone());
                }
                self.last_location.clone_from(&service.location);
            }
        }
        // A file being opened in a tab that shows another page is named by
        // the address alone until its load commits; its folder's server must
        // outlive that.
        self.local_files.retain(
            self.browser
                .saved
                .tabs
                .iter()
                .map(|tab| tab.url.as_str())
                .chain(std::iter::once(self.address.as_str())),
        );
        self.sync_zoom_model();
        let mut copies: Vec<_> = self
            .webview
            .borrow_mut()
            .pending_copy
            .take()
            .into_iter()
            .collect();
        for tab in self.browser.sleeping.iter().flatten() {
            copies.extend(tab.host.borrow_mut().pending_copy.take());
        }
        for selection in copies {
            task = Task::batch([
                task,
                iced::clipboard::write(selection.text)
                    .map(|_| Message::Chrome(crate::browser::Command::MaintenancePoll)),
            ]);
        }
        if let Some(path) = self.pending_pdf.take() {
            task = Task::batch([
                task,
                self.browser_command(crate::browser::Command::PdfOpen(path)),
            ]);
        }
        self.dismiss_stale_credential_offer();
        if !keyboard_blocked && self.webpage_keyboard_blocked() {
            self.suspend_webpage_keyboard();
            if !self.browser.address_suggestions_open {
                task = iced::advanced::widget::operate(
                    iced::advanced::widget::operation::focusable::unfocus(),
                )
                .chain(task);
            }
        }
        task = Task::batch(std::iter::once(task).chain(std::mem::take(&mut self.pending_tasks)));
        self.browser.toasts.sync();
        if self.allow_quiet
            && quiet_candidate
            && self.browser.chrome_revision == previous_chrome
            && self.browser.toasts.revision() == previous_toasts
            && self.background_visual_revision() == previous_background
            && !had_a11y
            && !discarded_gpu
            && self.webview.borrow().native_visual_revision == previous_visual
        {
            Task::batch([task, enzui_core::native_loop::quiet()])
        } else {
            task
        }
    }
}

include!("update/webview.rs");
