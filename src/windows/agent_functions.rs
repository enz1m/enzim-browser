//! The browser's functions as the agent center lists them, and their arguments.
//!
//! Names follow Chrome's DevTools MCP server where one exists (`list_pages`,
//! `take_snapshot`, `click`, ...), so an agent that knows that server knows
//! these. A page is a tab, named by an id that survives navigation and moves
//! between windows. Private windows are not listed and cannot be reached.
//!
//! Each tab belongs to a container (separate cookies, sign-ins and storage)
//! or to none, the default. Containers are named by string ids, since theirs
//! are too large for a JSON number, and `"default"` is no container. A
//! container the person closed to agents is listed as such; its tabs are not
//! listed and no function reaches them or opens pages in it.

use enzui_agent::center::{
    AppFunction, AppFunctionRegistry, ConsentScope, FunctionRisk, FunctionSpec, object_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};

/// A call the browser has decoded and will apply.
#[derive(Debug, Clone)]
pub(crate) enum Request {
    ListWindows,
    ListContainers,
    ListPages(ListPages),
    SelectPage(PageArg),
    NewPage(NewPage),
    ClosePage(PageArg),
    NavigatePage(NavigatePage),
    NewWindow(NewWindow),
    CloseWindow(WindowArg),
    TakeSnapshot(TakeSnapshot),
    TakeScreenshot(TakeScreenshot),
    Click(Click),
    Hover(ElementArg),
    Fill(Fill),
    FillForm(FillForm),
    TypeText(TypeText),
    PressKey(PressKey),
    Scroll(Scroll),
    WaitFor(WaitFor),
    EvaluateScript(EvaluateScript),
    HandleDialog(HandleDialog),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListPages {
    pub(crate) window_id: Option<u64>,
    pub(crate) container: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PageArg {
    pub(crate) page_id: u64,
    #[serde(default)]
    pub(crate) bring_to_front: bool,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WindowArg {
    pub(crate) window_id: u64,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NewPage {
    pub(crate) url: String,
    pub(crate) window_id: Option<u64>,
    pub(crate) container: Option<String>,
    #[serde(default)]
    pub(crate) background: bool,
    pub(crate) timeout_ms: Option<u64>,
}
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum NavigationType {
    Url,
    Back,
    Forward,
    Reload,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NavigatePage {
    pub(crate) page_id: u64,
    #[serde(rename = "type")]
    pub(crate) kind: Option<NavigationType>,
    pub(crate) url: Option<String>,
    pub(crate) timeout_ms: Option<u64>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NewWindow {
    pub(crate) url: Option<String>,
    pub(crate) container: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TakeSnapshot {
    pub(crate) page_id: u64,
    #[serde(default)]
    pub(crate) verbose: bool,
    #[serde(default)]
    pub(crate) offset: u64,
    /// Characters per part; the page script's own default when absent.
    #[serde(default)]
    pub(crate) max_chars: Option<u32>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TakeScreenshot {
    pub(crate) page_id: u64,
    #[serde(default)]
    pub(crate) full_page: bool,
    pub(crate) quality: Option<u8>,
    pub(crate) max_width: Option<u32>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Click {
    pub(crate) page_id: u64,
    pub(crate) uid: String,
    #[serde(default)]
    pub(crate) double_click: bool,
    #[serde(default)]
    pub(crate) include_snapshot: bool,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ElementArg {
    pub(crate) page_id: u64,
    pub(crate) uid: String,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Fill {
    pub(crate) page_id: u64,
    pub(crate) uid: String,
    pub(crate) value: String,
    #[serde(default)]
    pub(crate) include_snapshot: bool,
}
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FormValue {
    pub(crate) uid: String,
    pub(crate) value: String,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FillForm {
    pub(crate) page_id: u64,
    pub(crate) elements: Vec<FormValue>,
    #[serde(default)]
    pub(crate) include_snapshot: bool,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TypeText {
    pub(crate) page_id: u64,
    pub(crate) text: String,
    pub(crate) uid: Option<String>,
    pub(crate) submit_key: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PressKey {
    pub(crate) page_id: u64,
    pub(crate) key: String,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Scroll {
    pub(crate) page_id: u64,
    pub(crate) uid: Option<String>,
    #[serde(default)]
    pub(crate) dx: f64,
    #[serde(default)]
    pub(crate) dy: f64,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WaitFor {
    pub(crate) page_id: u64,
    pub(crate) text: Vec<String>,
    pub(crate) timeout_ms: Option<u64>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EvaluateScript {
    pub(crate) page_id: u64,
    pub(crate) function: String,
    #[serde(default)]
    pub(crate) args: Vec<String>,
}
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum DialogAction {
    Accept,
    Dismiss,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HandleDialog {
    pub(crate) page_id: u64,
    pub(crate) action: DialogAction,
    pub(crate) prompt_text: Option<String>,
}

fn page_id() -> Value {
    json!({"type": "integer", "minimum": 0, "description": "A page id from list_pages."})
}
fn container() -> Value {
    json!({"type": "string", "minLength": 1, "maxLength": 128, "description": "A container_id or name from list_containers; \"default\" (or leaving it out) is no container."})
}
fn uid() -> Value {
    json!({"type": "string", "minLength": 1, "maxLength": 64, "description": "An element uid from the page's latest take_snapshot."})
}
fn timeout() -> Value {
    json!({"type": "integer", "minimum": 0, "maximum": 60000, "description": "How long to wait for the page to load, in ms (default 30000)."})
}
fn include_snapshot() -> Value {
    json!({"type": "boolean", "description": "Also return a snapshot of the page afterwards."})
}
/// An output object whose listed fields are checked and which may carry more.
fn output(properties: Value, required: &[&str]) -> Value {
    json!({"type": "object", "properties": properties, "required": required})
}
fn page_state() -> Value {
    output(
        json!({
            "page_id": {"type": "integer"},
            "url": {"type": "string"},
            "title": {"type": "string"},
        }),
        &["page_id", "url"],
    )
}
fn action_result() -> Value {
    output(
        json!({"page_id": {"type": "integer"}, "snapshot": {"type": "string"}}),
        &["page_id"],
    )
}

fn spec(
    id: &str,
    title: &str,
    description: &str,
    input: Value,
    result: Value,
    risk: FunctionRisk,
) -> FunctionSpec {
    let consent = match risk {
        FunctionRisk::ReadOnly | FunctionRisk::SensitiveRead | FunctionRisk::Mutating => {
            vec![
                ConsentScope::Once,
                ConsentScope::Session,
                ConsentScope::Always,
            ]
        }
        _ => vec![ConsentScope::Once, ConsentScope::Session],
    };
    FunctionSpec::new(id, title, description, input, result, risk, consent)
}

/// Every function the browser offers.
///
/// # Errors
/// When a declaration fails the framework's validation; that is a programming
/// error caught by the tests.
pub(crate) fn registry() -> Result<AppFunctionRegistry<Request>, String> {
    use FunctionRisk::{Destructive, Mutating, SensitiveRead};
    let functions = vec![
        AppFunction::new(
            spec(
                "list_windows",
                "List windows",
                "The browser's windows (private windows are not shown), each with its tab count and selected page.",
                object_schema(json!({}), &[]),
                output(json!({"windows": {"type": "array", "items": {"type": "object"}}}), &["windows"]),
                SensitiveRead,
            ),
            |_| Request::ListWindows,
        ),
        AppFunction::new(
            spec(
                "list_containers",
                "List containers",
                "The containers tabs can belong to, each keeping its own cookies, sign-ins and storage: its container_id and name, whether its traffic goes through a proxy, and whether agents may use it. \"default\" is no container.",
                object_schema(json!({}), &[]),
                output(json!({"containers": {"type": "array", "items": {"type": "object"}}}), &["containers"]),
                SensitiveRead,
            ),
            |_| Request::ListContainers,
        ),
        AppFunction::typed(
            spec(
                "list_pages",
                "List pages",
                "Every tab in every window, with its page id, address, title, container and whether it is selected, loaded and loading. A tab that is not loaded loads when an agent uses it. Tabs of a container closed to agents are not listed.",
                object_schema(json!({"window_id": {"type": "integer", "minimum": 0, "description": "Only this window's tabs."}, "container": container()}), &[]),
                output(json!({"pages": {"type": "array", "items": {"type": "object"}}}), &["pages"]),
                SensitiveRead,
            ),
            Request::ListPages,
        ),
        AppFunction::typed(
            spec(
                "select_page",
                "Select a page",
                "Shows the tab in its window, loading it if it is not loaded, and waits for the load. bring_to_front also asks the desktop to raise the window.",
                object_schema(json!({"page_id": page_id(), "bring_to_front": {"type": "boolean"}}), &["page_id"]),
                page_state(),
                Mutating,
            ),
            Request::SelectPage,
        ),
        AppFunction::typed(
            spec(
                "new_page",
                "Open a page",
                "Opens an address in a new tab, in the container given or in none, and waits for it to load. background opens the tab without selecting it; it loads when first used.",
                object_schema(
                    json!({
                        "url": {"type": "string", "minLength": 1, "maxLength": 8192, "description": "An address or search words."},
                        "window_id": {"type": "integer", "minimum": 0},
                        "container": container(),
                        "background": {"type": "boolean"},
                        "timeout_ms": timeout(),
                    }),
                    &["url"],
                ),
                page_state(),
                Mutating,
            ),
            Request::NewPage,
        ),
        AppFunction::typed(
            spec(
                "close_page",
                "Close a page",
                "Closes the tab. A pinned tab and the last tab of the last window are not closed.",
                object_schema(json!({"page_id": page_id()}), &["page_id"]),
                output(json!({"closed": {"type": "boolean"}}), &["closed"]),
                Destructive,
            ),
            Request::ClosePage,
        ),
        AppFunction::typed(
            spec(
                "navigate_page",
                "Navigate a page",
                "Loads an address in the tab, or goes back, forward or reloads, and waits for the load. The tab is selected first.",
                object_schema(
                    json!({
                        "page_id": page_id(),
                        "type": {"type": "string", "enum": ["url", "back", "forward", "reload"], "description": "Default url."},
                        "url": {"type": "string", "minLength": 1, "maxLength": 8192},
                        "timeout_ms": timeout(),
                    }),
                    &["page_id"],
                ),
                page_state(),
                Mutating,
            ),
            Request::NavigatePage,
        ),
        AppFunction::typed(
            spec(
                "new_window",
                "Open a window",
                "Opens a new browser window, on an address if one is given, its tab in the container given or in none.",
                object_schema(json!({"url": {"type": "string", "minLength": 1, "maxLength": 8192}, "container": container()}), &[]),
                output(json!({"window_id": {"type": "integer"}, "page_id": {"type": "integer"}}), &["window_id", "page_id"]),
                Mutating,
            ),
            Request::NewWindow,
        ),
        AppFunction::typed(
            spec(
                "close_window",
                "Close a window",
                "Closes a window and its tabs. The last window is not closed.",
                object_schema(json!({"window_id": {"type": "integer", "minimum": 0}}), &["window_id"]),
                output(json!({"closed": {"type": "boolean"}}), &["closed"]),
                Destructive,
            ),
            Request::CloseWindow,
        ),
        AppFunction::typed(
            spec(
                "take_snapshot",
                "Read a page",
                "A text outline of the page: interactive elements, headings and text, each element with a uid for click, fill and the other element functions. Long pages come in parts: pass next_offset as offset for the next part. Uids hold until the page navigates.",
                object_schema(
                    json!({"page_id": page_id(), "verbose": {"type": "boolean", "description": "Every element, not only the meaningful ones."}, "offset": {"type": "integer", "minimum": 0}, "max_chars": {"type": "integer", "minimum": 2000, "maximum": 48000, "description": "Characters per part, 48000 when not given."}}),
                    &["page_id"],
                ),
                output(
                    json!({"page_id": {"type": "integer"}, "url": {"type": "string"}, "title": {"type": "string"}, "snapshot": {"type": "string"}, "truncated": {"type": "boolean"}}),
                    &["page_id", "snapshot"],
                ),
                SensitiveRead,
            )
            .max_result_bytes(128 * 1024),
            Request::TakeSnapshot,
        ),
        AppFunction::typed(
            spec(
                "take_screenshot",
                "Capture a page",
                "A JPEG of the visible part of the page (or the whole page with full_page), as base64.",
                object_schema(
                    json!({
                        "page_id": page_id(),
                        "full_page": {"type": "boolean"},
                        "quality": {"type": "integer", "minimum": 10, "maximum": 95, "description": "JPEG quality, default 70."},
                        "max_width": {"type": "integer", "minimum": 64, "maximum": 2560, "description": "Default 1280 pixels."},
                    }),
                    &["page_id"],
                ),
                output(
                    json!({"page_id": {"type": "integer"}, "mime_type": {"type": "string"}, "width": {"type": "integer"}, "height": {"type": "integer"}, "data": {"type": "string"}}),
                    &["page_id", "mime_type", "data"],
                ),
                SensitiveRead,
            )
            .max_result_bytes(512 * 1024),
            Request::TakeScreenshot,
        ),
        AppFunction::typed(
            spec(
                "click",
                "Click an element",
                "Clicks the element at its centre with the pointer, as a person would. The tab is selected first.",
                object_schema(
                    json!({"page_id": page_id(), "uid": uid(), "double_click": {"type": "boolean"}, "include_snapshot": include_snapshot()}),
                    &["page_id", "uid"],
                ),
                action_result(),
                Mutating,
            )
            .max_result_bytes(128 * 1024),
            Request::Click,
        ),
        AppFunction::typed(
            spec(
                "hover",
                "Point at an element",
                "Moves the pointer over the element. The tab is selected first.",
                object_schema(json!({"page_id": page_id(), "uid": uid()}), &["page_id", "uid"]),
                action_result(),
                Mutating,
            ),
            Request::Hover,
        ),
        AppFunction::typed(
            spec(
                "fill",
                "Fill a field",
                "Sets a text field, a choice list (by value or label) or a checkbox (true or false) and tells the page it changed.",
                object_schema(
                    json!({"page_id": page_id(), "uid": uid(), "value": {"type": "string", "maxLength": 65536}, "include_snapshot": include_snapshot()}),
                    &["page_id", "uid", "value"],
                ),
                action_result(),
                Mutating,
            )
            .max_result_bytes(128 * 1024),
            Request::Fill,
        ),
        AppFunction::typed(
            spec(
                "fill_form",
                "Fill several fields",
                "Fills several fields at once, as fill does.",
                object_schema(
                    json!({
                        "page_id": page_id(),
                        "elements": {"type": "array", "minItems": 1, "maxItems": 64, "items": object_schema(json!({"uid": uid(), "value": {"type": "string", "maxLength": 65536}}), &["uid", "value"])},
                        "include_snapshot": include_snapshot(),
                    }),
                    &["page_id", "elements"],
                ),
                action_result(),
                Mutating,
            )
            .max_result_bytes(128 * 1024),
            Request::FillForm,
        ),
        AppFunction::typed(
            spec(
                "type_text",
                "Type text",
                "Types text with the keyboard into the focused element, or into the element named by uid. submit_key is pressed afterwards, such as Enter. The tab is selected first.",
                object_schema(
                    json!({"page_id": page_id(), "text": {"type": "string", "maxLength": 4096}, "uid": uid(), "submit_key": {"type": "string", "maxLength": 64}}),
                    &["page_id", "text"],
                ),
                action_result(),
                Mutating,
            ),
            Request::TypeText,
        ),
        AppFunction::typed(
            spec(
                "press_key",
                "Press a key",
                "Presses a key or a combination such as Enter, Tab, Escape, ArrowDown, Control+A or Control+Shift+R. The tab is selected first.",
                object_schema(json!({"page_id": page_id(), "key": {"type": "string", "minLength": 1, "maxLength": 64}}), &["page_id", "key"]),
                action_result(),
                Mutating,
            ),
            Request::PressKey,
        ),
        AppFunction::typed(
            spec(
                "scroll",
                "Scroll a page",
                "Scrolls the element named by uid into view, or scrolls the page by dx and dy CSS pixels.",
                object_schema(
                    json!({"page_id": page_id(), "uid": uid(), "dx": {"type": "number"}, "dy": {"type": "number"}}),
                    &["page_id"],
                ),
                output(json!({"page_id": {"type": "integer"}, "scroll_x": {"type": "number"}, "scroll_y": {"type": "number"}}), &["page_id"]),
                Mutating,
            ),
            Request::Scroll,
        ),
        AppFunction::typed(
            spec(
                "wait_for",
                "Wait for text",
                "Waits until any of the texts appears in the page.",
                object_schema(
                    json!({
                        "page_id": page_id(),
                        "text": {"type": "array", "minItems": 1, "maxItems": 16, "items": {"type": "string", "minLength": 1, "maxLength": 512}},
                        "timeout_ms": timeout(),
                    }),
                    &["page_id", "text"],
                ),
                output(json!({"page_id": {"type": "integer"}, "found": {"type": "string"}}), &["page_id", "found"]),
                SensitiveRead,
            ),
            Request::WaitFor,
        ),
        AppFunction::typed(
            spec(
                "evaluate_script",
                "Run a script",
                "Runs a JavaScript function in the page, such as `() => document.title` or `async (link) => link.href`, and returns its JSON value. args are element uids passed to the function as elements.",
                object_schema(
                    json!({
                        "page_id": page_id(),
                        "function": {"type": "string", "minLength": 1, "maxLength": 65536},
                        "args": {"type": "array", "maxItems": 16, "items": uid()},
                    }),
                    &["page_id", "function"],
                ),
                output(json!({"page_id": {"type": "integer"}, "result": {}}), &["page_id"]),
                Mutating,
            ),
            Request::EvaluateScript,
        ),
        AppFunction::typed(
            spec(
                "handle_dialog",
                "Answer a page dialog",
                "Accepts or dismisses the alert, confirmation or prompt the page shows; prompt_text is the answer to a prompt.",
                object_schema(
                    json!({"page_id": page_id(), "action": {"type": "string", "enum": ["accept", "dismiss"]}, "prompt_text": {"type": "string", "maxLength": 4096}}),
                    &["page_id", "action"],
                ),
                output(json!({"page_id": {"type": "integer"}, "message": {"type": "string"}}), &["page_id"]),
                Mutating,
            ),
            Request::HandleDialog,
        ),
    ];
    let functions = functions
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("{error:?}"))?;
    AppFunctionRegistry::from_functions(functions).map_err(|error| format!("{error:?}"))
}
