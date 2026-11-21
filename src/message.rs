//! Messages routed between browser chrome and native pages.

#[derive(Debug, Clone)]
pub(crate) enum Message {
    Chrome(crate::browser::Command),
    Assistant(crate::assistant::Msg),
    Theme(enzui_core::theme_file::integration::ThemeEvent),
    WebSystemFont(Option<String>),
    BrowserAddress(String),
    BrowserGo,
    BrowserBack,
    BrowserForward,
    BrowserReload,
    BrowserFocusAddress,
    WebViewNavigateHttps,
    WebViewNavigateBadScheme,
    WebViewUnknownPermission,
    WebViewFocusGained,
    WebViewForwardTab,
    WebViewPageConsumedTab,
    WebViewPageExhaustedTab,
    WebViewStaleTabReply,
    WebViewFocusLost,
    WebViewOfferFrame,
    WebViewPresentFrame,
    WebViewStaleFrame,
    WebViewRecreate,
    WebViewCheckProfile,
    WebViewEphemeralProfile,
    WebViewEngineCameraLive,
    WebViewEngineCameraViewGone,
    WebViewDeliverPageA11y,
    WebViewDeliverStalePageA11y,
    WebViewActivatePageButton,
    WebViewActivatePageHeading,
    A11yActivatePageNode(iced::advanced::widget::Id),
    WebViewHide,
    WebViewShow,
    WebViewClose,
    WebViewPointerPress,
    WebViewInput(enzui_webkit::input::InputEvent),
    WebViewViewportResize(enzui_webkit::web::ViewId, enzui_webkit::ViewportSize),
    WebViewFocusTextInput,
    WebViewFocusPasswordInput,
    WebViewRelocateCaret,
    WebViewPreedit,
    WebViewCommitPreedit,
    WebViewCancelPreedit,
    WebViewBlurInput,
    WebViewStaleImeClaim,
    WebViewImeEvent(enzui_webkit::web::ToolkitImeEvent),
    WebViewPageCursor(Option<&'static str>),
    WebViewRefreshSandbox,
    WebViewOpenNative,
    WebViewRendererReady,
    WebViewNativeReady,
    WebViewNativeClick(f32, f32),
    WebViewNativeHover(f32, f32),
    WebViewNativeCopy,
    WebViewNativePaste(String),
    WebViewPasteReady(
        crate::browser_clipboard::PasteTarget,
        Option<enzui_security::secret::SecretText>,
    ),
}

// Per-action dispatch pairing the shared applier with the page-node activation
// only this app can perform: no widget-tree element carries page ids, so the
// shared applier does nothing for them. Ids that name no page node resolve to
// nothing.
pub(crate) fn a11y_action_tasks(
    routed: &enzui_core::app_runtime::RoutedAccessibilityAction,
) -> Vec<iced::Task<Message>> {
    use enzui_core::app_runtime::AccessibilityActionKind;

    let mut tasks = vec![enzui_widgets::a11y_actions::apply_accessibility_action(
        routed,
    )];
    if routed.kind() == AccessibilityActionKind::Activate {
        tasks.push(iced::Task::done(Message::A11yActivatePageNode(
            routed.widget().clone(),
        )));
    }
    tasks
}

// Actions queued since the last turn, applied on the next update even when
// their wake has not arrived yet.
pub(crate) fn drain_a11y_actions() -> Vec<iced::Task<Message>> {
    enzui_core::app_runtime::drain_accessibility_actions()
        .iter()
        .flat_map(a11y_action_tasks)
        .collect()
}
