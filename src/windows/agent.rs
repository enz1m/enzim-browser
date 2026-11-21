//! The browser as an app of the enzUI agent center.
//!
//! An agent enrolled with the center lists the browser's functions through the
//! center's four MCP tools and calls them by name; the center asks the person
//! before each function runs (see `enzui-mcp-center`). The browser takes part
//! only when the center gave it a credential (`CenterProvision::discover`).
//!
//! Calls that need a page run in steps: the tab is found and, when it is not
//! loaded, selected so that it loads; scripts run in an isolated world the page
//! cannot see; pointer and key input goes through the same path as the
//! person's own, so it needs the tab to be selected and shown.

use super::agent_functions::{DialogAction, NavigationType, Request, TakeScreenshot, registry};
use super::{Event, Windows};
use crate::Message;
use crate::browser::Command;
use crate::webview_native::BrowserAction;
use enzui_agent::center::{
    AppMetadata, CallRef, CenterCall, CenterFailure, CenterHost, CenterOutcome, CenterProvision,
    CenterTurn, ConnectorEvent,
};
use enzui_webkit::input::{InputEvent, KeyCode, Modifiers, NamedKey, PointerButton, ViewPoint};
use enzui_webkit::page::ScriptWorld;
use iced::{Task, window};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

const HELPER: &str = include_str!("../agent_page.js");
const WORLD: &str = "enzim-agent";
const POLL: Duration = Duration::from_millis(100);
const SETTLE: Duration = Duration::from_millis(200);
const DEFAULT_LOAD: Duration = Duration::from_secs(30);
const MAX_PENDING: usize = 32;

#[derive(Clone, Debug)]
pub(crate) enum AgentEvent {
    Connector(ConnectorEvent),
    Poll(u64),
    Script(u64, Result<Value, String>),
    Capture(u64, Result<std::sync::Arc<Vec<u8>>, String>),
}

/// The connection to the center and the calls in progress, the center's and
/// the assistant's.
pub(crate) struct Agent {
    /// `None` while only the assistant uses the functions.
    host: Option<CenterHost<Request>>,
    window_ids: BTreeMap<window::Id, u64>,
    next_window: u64,
    pending: BTreeMap<u64, Pending>,
    next_token: u64,
}

/// Who a call answers.
enum Caller {
    Center(CenterCall),
    /// The assistant of a window, by its tool call's id.
    Assistant(window::Id, String),
}

struct Pending {
    call: Caller,
    /// The center's reference, for its calls.
    reference: Option<CallRef>,
    request: Request,
    stage: Stage,
    started: Instant,
    limit: Duration,
    /// What an earlier step found, such as the element's place.
    found: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    /// Waiting for the page to exist, be loaded and finish loading.
    Load,
    /// Waiting for a script's value.
    Script,
    /// Waiting for a second script's value.
    SecondScript,
    /// Waiting for the page to take in input before the next step.
    Settle,
    /// Waiting for the page's picture.
    Capture,
}

fn failure(code: &'static str, message: impl Into<String>) -> CenterFailure {
    CenterFailure::new(code, message)
}
fn not_found(page: u64) -> CenterFailure {
    failure(
        "not_found",
        format!("No page {page}; list_pages names the open pages."),
    )
}

impl Agent {
    /// Connects when the center provisioned the browser; otherwise `None`.
    pub(crate) fn start() -> Option<Self> {
        let provision = CenterProvision::discover(crate::desktop::APP_ID)?;
        let registry = match registry() {
            Ok(registry) => registry,
            Err(error) => {
                eprintln!("agent functions are invalid: {error}");
                return None;
            }
        };
        let metadata = AppMetadata {
            id: crate::desktop::APP_ID.into(),
            name: "enzim Browser".into(),
            description: "Web browser: tabs, windows, pages and what is in them.".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        };
        match CenterHost::spawn(provision, metadata, registry) {
            Ok(host) => Some(Self::with_host(host)),
            Err(error) => {
                eprintln!("agent center unavailable: {error:?}");
                None
            }
        }
    }
    /// Without the center, for the assistant's calls.
    pub(crate) fn local() -> Self {
        Self {
            host: None,
            window_ids: BTreeMap::new(),
            next_window: 1,
            pending: BTreeMap::new(),
            next_token: 1,
        }
    }
    fn with_host(host: CenterHost<Request>) -> Self {
        Self {
            host: Some(host),
            window_ids: BTreeMap::new(),
            next_window: 1,
            pending: BTreeMap::new(),
            next_token: 1,
        }
    }
    pub(crate) fn subscription(&self) -> iced::Subscription<Event> {
        self.host.as_ref().map_or_else(iced::Subscription::none, |host| {
            host.subscription(|event| Event::Agent(AgentEvent::Connector(event)))
        })
    }
    pub(crate) fn shutdown(self) {
        if let Some(host) = self.host {
            host.shutdown();
        }
    }
    fn window_id(&mut self, id: window::Id) -> u64 {
        if let Some(number) = self.window_ids.get(&id) {
            return *number;
        }
        let number = self.next_window;
        self.next_window += 1;
        self.window_ids.insert(id, number);
        number
    }
    fn window(&self, number: u64) -> Option<window::Id> {
        self.window_ids
            .iter()
            .find(|(_, n)| **n == number)
            .map(|(id, _)| *id)
    }
}

fn poll(token: u64, after: Duration) -> Task<Event> {
    Task::perform(tokio::time::sleep(after), move |()| {
        Event::Agent(AgentEvent::Poll(token))
    })
}

impl Windows {
    pub(crate) fn agent_event(&mut self, event: AgentEvent) -> Task<Event> {
        match event {
            AgentEvent::Connector(event) => {
                let Some(agent) = self.agent.as_mut() else {
                    return Task::none();
                };
                let Some(host) = agent.host.as_mut() else {
                    return Task::none();
                };
                let update = host.receive_with_ends(event);
                for ended in update.ended {
                    agent
                        .pending
                        .retain(|_, pending| pending.reference.as_ref() != Some(&ended.call));
                }
                match update.turn {
                    CenterTurn::Call(call, request) => {
                        self.agent_call(Caller::Center(call), request)
                    }
                    CenterTurn::Handled => Task::none(),
                }
            }
            AgentEvent::Poll(token) => self.agent_step(token, None),
            AgentEvent::Script(token, result) => {
                self.agent_step(token, Some(Input::Script(result)))
            }
            AgentEvent::Capture(token, result) => {
                self.agent_step(token, Some(Input::Capture(result)))
            }
        }
    }

    /// Runs the assistant's calls of window `window`: `(id, function,
    /// arguments)`. Each answer comes back to that window's assistant.
    pub(crate) fn assistant_calls(
        &mut self,
        window: window::Id,
        calls: Vec<(String, String, Value)>,
    ) -> Task<Event> {
        let agent = self.agent.get_or_insert_with(Agent::local);
        let _ = agent.window_id(window);
        let mut tasks = Vec::new();
        for (id, function, arguments) in calls {
            match decode(&function, arguments) {
                Ok(request) => {
                    tasks.push(self.agent_call(Caller::Assistant(window, id), request));
                }
                Err(error) => tasks.push(assistant_answer(window, id, Err(error))),
            }
        }
        Task::batch(tasks)
    }

    fn agent_answer(&mut self, call: Caller, outcome: CenterOutcome) -> Task<Event> {
        match call {
            Caller::Center(call) => {
                if let Some(host) = self.agent.as_mut().and_then(|agent| agent.host.as_mut()) {
                    host.answer(call, outcome);
                }
                Task::none()
            }
            Caller::Assistant(window, id) => assistant_answer(
                window,
                id,
                outcome.map_err(|failure| failure.message),
            ),
        }
    }

    fn agent_call(&mut self, call: Caller, request: Request) -> Task<Event> {
        let closed = page_of_request(&request).filter(|page| self.agent_closed_page(*page));
        // An agent's first use of a tab that is not loaded selects it to load
        // it; the assistant works beside the person, so it is refused instead.
        let unloaded = matches!(call, Caller::Assistant(..))
            && !needs_front(&request)
            && !matches!(
                request,
                Request::ClosePage(_) | Request::HandleDialog(_) | Request::SelectPage(_) | Request::NavigatePage(_)
            )
            && page_of_request(&request).is_some_and(|page| !self.agent_page_loaded(page));
        let immediate = match &request {
            _ if closed.is_some() => Some(Err(failure(
                "refused",
                "The tab is in a container the person closed to agents.",
            ))),
            // Reading it gives its address; the assistant reads that instead.
            Request::TakeSnapshot(args) if unloaded => Some(
                self.agent_page_state(args.page_id)
                    .map(|mut state| {
                        state["loaded"] = json!(false);
                        (state, Task::none())
                    }),
            ),
            _ if unloaded => Some(Err(failure(
                "unavailable",
                "This tab is not loaded, and loading it would switch the person's tab. Read its address with read_web_page instead, or ask them to open it.",
            ))),
            Request::ListWindows => Some(Ok(self.agent_windows())),
            Request::ListContainers => Some(Ok((self.agent_container_list(), Task::none()))),
            Request::ListPages(args) => Some(
                self.agent_container(args.container.as_deref(), false)
                    .and_then(|container| self.agent_pages(args.window_id, container)),
            ),
            Request::NewWindow(args) => Some(
                self.agent_container(args.container.as_deref(), true)
                    .and_then(|container| {
                        self.agent_new_window(args.url.clone(), container.flatten())
                    }),
            ),
            Request::CloseWindow(args) => Some(self.agent_close_window(args.window_id)),
            Request::ClosePage(args) => Some(self.agent_close_page(args.page_id)),
            Request::HandleDialog(args) => {
                Some(self.agent_dialog(args.page_id, args.action, args.prompt_text.clone()))
            }
            _ => None,
        };
        if let Some(outcome) = immediate {
            let (outcome, task) = match outcome {
                Ok((value, task)) => (Ok(value), task),
                Err(error) => (Err(error), Task::none()),
            };
            return Task::batch([task, self.agent_answer(call, outcome)]);
        }
        let Some(agent) = self.agent.as_mut() else {
            return Task::none();
        };
        if agent.pending.len() >= MAX_PENDING {
            return self.agent_answer(
                call,
                Err(failure("busy", "Too many browser calls are in progress.")),
            );
        }
        let limit = match &request {
            Request::NewPage(args) => args.timeout_ms,
            Request::NavigatePage(args) => args.timeout_ms,
            Request::WaitFor(args) => args.timeout_ms,
            _ => None,
        }
        .map_or(DEFAULT_LOAD, Duration::from_millis)
        .min(Duration::from_secs(60));
        let token = agent.next_token;
        agent.next_token += 1;
        let reference = match &call {
            Caller::Center(call) => Some(call.reference()),
            Caller::Assistant(..) => None,
        };
        agent.pending.insert(
            token,
            Pending {
                call,
                reference,
                request,
                stage: Stage::Load,
                started: Instant::now(),
                limit,
                found: Value::Null,
            },
        );
        let start = self.agent_begin(token);
        Task::batch([start, self.agent_step(token, None)])
    }

    /// The first change a call makes: opening, selecting or navigating.
    fn agent_begin(&mut self, token: u64) -> Task<Event> {
        let Some(request) = self
            .agent
            .as_ref()
            .and_then(|agent| agent.pending.get(&token))
            .map(|pending| pending.request.clone())
        else {
            return Task::none();
        };
        let mut found = Value::Null;
        let task = match request {
            Request::NewPage(args) => {
                let opened = self
                    .agent_container(args.container.as_deref(), true)
                    .and_then(|container| {
                        self.agent_new_page(
                            &args.url,
                            args.window_id,
                            args.background,
                            container.flatten(),
                        )
                    });
                match opened {
                    Ok((page, task)) => {
                        found = json!({"page": page, "background": args.background});
                        task
                    }
                    Err(error) => return self.agent_finish(token, Err(error)),
                }
            }
            Request::SelectPage(args) => {
                let task = self.agent_front(args.page_id, args.bring_to_front);
                found = json!({"page": args.page_id});
                task
            }
            Request::NavigatePage(args) => {
                let front = self.agent_front(args.page_id, false);
                let Some((id, index)) = self.agent_locate(args.page_id) else {
                    return self.agent_finish(token, Err(not_found(args.page_id)));
                };
                let Some(app) = self.apps.get_mut(&id) else {
                    return Task::none();
                };
                if index != app.browser.saved.active {
                    return self.agent_finish(
                        token,
                        Err(failure("unavailable", "The tab could not be selected.")),
                    );
                }
                let kind = args.kind.unwrap_or(NavigationType::Url);
                let task = match kind {
                    NavigationType::Url => {
                        let Some(url) = args.url.clone().filter(|url| !url.trim().is_empty())
                        else {
                            return self.agent_finish(
                                token,
                                Err(failure("invalid_input", "type url needs a url.")),
                            );
                        };
                        app.browser_command(Command::Navigate(url))
                    }
                    NavigationType::Back => {
                        app.submit_browser_action(BrowserAction::Back);
                        Task::none()
                    }
                    NavigationType::Forward => {
                        app.submit_browser_action(BrowserAction::Forward);
                        Task::none()
                    }
                    NavigationType::Reload => {
                        app.submit_browser_action(BrowserAction::Reload);
                        Task::none()
                    }
                };
                found = json!({"page": args.page_id});
                Task::batch([front, task.map(move |message| Event::App(id, message))])
            }
            _ => {
                if let Some(page) = page_of_request(&request) {
                    found = json!({"page": page});
                    if needs_front(&request) {
                        self.agent_front(page, false)
                    } else {
                        self.agent_load(page)
                    }
                } else {
                    Task::none()
                }
            }
        };
        if let Some(pending) = self
            .agent
            .as_mut()
            .and_then(|agent| agent.pending.get_mut(&token))
        {
            pending.found = found;
        }
        task
    }

    fn agent_step(&mut self, token: u64, input: Option<Input>) -> Task<Event> {
        let ended = self
            .agent
            .as_mut()
            .and_then(|agent| agent.host.as_mut())
            .map(CenterHost::poll_ended)
            .unwrap_or_default();
        if let Some(agent) = self.agent.as_mut() {
            for ended in ended {
                agent
                    .pending
                    .retain(|_, pending| pending.reference.as_ref() != Some(&ended.call));
            }
        }
        let Some((request, stage, started, limit, found)) = self
            .agent
            .as_ref()
            .and_then(|agent| agent.pending.get(&token))
            .map(|p| {
                (
                    p.request.clone(),
                    p.stage,
                    p.started,
                    p.limit,
                    p.found.clone(),
                )
            })
        else {
            return Task::none();
        };
        if started.elapsed() > limit + Duration::from_secs(5) {
            return self.agent_finish(
                token,
                Err(failure("timeout", "The page did not finish in time.")),
            );
        }
        let page = found.get("page").and_then(Value::as_u64);
        match (stage, input) {
            (Stage::Load, _) => {
                let Some(page) = page else {
                    return self.agent_finish(token, Err(failure("not_found", "No page.")));
                };
                if found.get("background").and_then(Value::as_bool) == Some(true) {
                    let state = self.agent_page_state(page);
                    return self.agent_finish(token, state);
                }
                match self.agent_ready(page) {
                    Readiness::Missing => self.agent_finish(token, Err(not_found(page))),
                    Readiness::Waiting => {
                        if started.elapsed() > limit {
                            let state = self.agent_page_state(page);
                            return self.agent_finish(
                                token,
                                state.map(|mut value| {
                                    value["loading"] = json!(true);
                                    value
                                }),
                            );
                        }
                        poll(token, POLL)
                    }
                    Readiness::Ready => self.agent_run(token, page, &request),
                }
            }
            (Stage::Script, Some(Input::Script(result))) => {
                let Some(page) = page else {
                    return Task::none();
                };
                self.agent_script_done(token, page, &request, result, found)
            }
            (Stage::SecondScript, Some(Input::Script(result))) => {
                let Some(page) = page else {
                    return Task::none();
                };
                match result {
                    Ok(value) => {
                        let mut answer = json!({"page_id": page});
                        if let Value::Object(map) = value {
                            if let Some(snapshot) = map.get("snapshot") {
                                answer["snapshot"] = snapshot.clone();
                            }
                            if let Some(url) = map.get("url") {
                                answer["url"] = url.clone();
                            }
                        }
                        if let Some(covered) = found.get("covered").filter(|v| !v.is_null()) {
                            answer["covered_by"] = covered.clone();
                        }
                        self.agent_finish(token, Ok(answer))
                    }
                    Err(error) => self.agent_finish(token, Err(script_failure(&error))),
                }
            }
            (Stage::Settle, _) => self.agent_after_settle(token, page, &request, found),
            (Stage::Capture, Some(Input::Capture(result))) => {
                let Some(page) = page else {
                    return Task::none();
                };
                let Request::TakeScreenshot(args) = &request else {
                    return Task::none();
                };
                let outcome = result
                    .map_err(|error| failure("unavailable", error))
                    .and_then(|png| screenshot(page, &png, &found, args));
                self.agent_finish(token, outcome)
            }
            _ => Task::none(),
        }
    }

    /// The page is loaded: do what the call is for.
    fn agent_run(&mut self, token: u64, page: u64, request: &Request) -> Task<Event> {
        let script = |call: &str| format!("{HELPER}\n__enzimAgent.{call}");
        match request {
            Request::NewPage(_) | Request::SelectPage(_) | Request::NavigatePage(_) => {
                let state = self.agent_page_state(page);
                self.agent_finish(token, state)
            }
            Request::TakeSnapshot(args) => {
                let call = script(&format!(
                    "snapshot({})",
                    json!({"verbose": args.verbose, "offset": args.offset, "maxBytes": args.max_chars.map(|max| max.clamp(2000, 48000))})
                ));
                self.agent_script(token, page, call, Stage::Script)
            }
            Request::TakeScreenshot(_) => {
                self.agent_script(token, page, script("state()") + ";({...__enzimAgent.state(), document_width: document.documentElement.scrollWidth, document_height: document.documentElement.scrollHeight})", Stage::Script)
            }
            Request::Click(args) => self.agent_script(
                token,
                page,
                script(&format!("clickTarget({})", json!(args.uid))),
                Stage::Script,
            ),
            Request::Hover(args) => self.agent_script(
                token,
                page,
                script(&format!("describe({})", json!(args.uid))),
                Stage::Script,
            ),
            Request::Fill(args) => self.agent_script(
                token,
                page,
                script(&format!("fill({}, {})", json!(args.uid), json!(args.value))),
                Stage::Script,
            ),
            Request::FillForm(args) => self.agent_script(
                token,
                page,
                format!(
                    "{HELPER}\n{}.map(({{uid, value}}) => __enzimAgent.fill(uid, value))",
                    json!(args.elements)
                ),
                Stage::Script,
            ),
            Request::TypeText(args) => {
                if let Some(uid) = &args.uid {
                    self.agent_script(token, page, script(&format!("focus({})", json!(uid))), Stage::Script)
                } else {
                    self.agent_type(token, page)
                }
            }
            Request::PressKey(_) => self.agent_type(token, page),
            Request::Scroll(args) => self.agent_script(
                token,
                page,
                script(&format!(
                    "scroll({}, {}, {})",
                    json!(args.uid),
                    json!(args.dx),
                    json!(args.dy)
                )),
                Stage::Script,
            ),
            Request::WaitFor(args) => self.agent_script(
                token,
                page,
                script(&format!("findText({})", json!(args.text))),
                Stage::Script,
            ),
            Request::EvaluateScript(args) => {
                if args.args.is_empty() {
                    self.agent_set(token, Stage::Script, json!({"page": page, "marked": true}));
                    self.agent_evaluate(token, page, &args.function, 0)
                } else {
                    self.agent_script(
                        token,
                        page,
                        script(&format!("markArgs({})", json!(args.args))),
                        Stage::Script,
                    )
                }
            }
            _ => self.agent_finish(token, Err(failure("unavailable", "Not a page call."))),
        }
    }

    fn agent_script_done(
        &mut self,
        token: u64,
        page: u64,
        request: &Request,
        result: Result<Value, String>,
        mut found: Value,
    ) -> Task<Event> {
        let value = match result {
            Ok(value) => value,
            Err(error) => {
                // A wait keeps trying while the page is replaced under it.
                if matches!(request, Request::WaitFor(_)) {
                    return self.agent_wait_again(token);
                }
                return self.agent_finish(token, Err(script_failure(&error)));
            }
        };
        match request {
            Request::TakeSnapshot(_) => {
                let mut answer = json!({"page_id": page});
                if let Value::Object(map) = value {
                    for (key, value) in map {
                        answer[key] = value;
                    }
                }
                self.agent_finish(token, Ok(answer))
            }
            Request::TakeScreenshot(args) => {
                found["viewport"] = value;
                self.agent_set(token, Stage::Capture, found);
                self.agent_capture(token, page, args)
            }
            // The page chose a list option itself: there is nothing to press.
            Request::Click(args) if value.get("chosen") == Some(&json!(true)) => {
                self.agent_after_action(token, page, args.include_snapshot, found)
            }
            Request::Click(_) | Request::Hover(_) => {
                found["covered"] = value.get("covered").cloned().unwrap_or(Value::Null);
                let Some(center) = value.get("center") else {
                    return self.agent_finish(
                        token,
                        Err(failure(
                            "unavailable",
                            "The element has no place on the page.",
                        )),
                    );
                };
                let x = center.get("x").and_then(Value::as_f64).unwrap_or(0.);
                let y = center.get("y").and_then(Value::as_f64).unwrap_or(0.);
                let double = matches!(request, Request::Click(args) if args.double_click);
                let hover_only = matches!(request, Request::Hover(_));
                if let Err(error) = self.agent_pointer(page, x, y, double, hover_only) {
                    return self.agent_finish(token, Err(error));
                }
                self.agent_set(token, Stage::Settle, found);
                poll(token, SETTLE)
            }
            Request::Fill(args) => {
                self.agent_after_action(token, page, args.include_snapshot, found)
            }
            Request::FillForm(args) => {
                self.agent_after_action(token, page, args.include_snapshot, found)
            }
            Request::TypeText(_) => self.agent_type(token, page),
            Request::Scroll(_) => {
                let mut answer = json!({"page_id": page});
                answer["scroll_x"] = value.get("scroll_x").cloned().unwrap_or_else(|| json!(0));
                answer["scroll_y"] = value.get("scroll_y").cloned().unwrap_or_else(|| json!(0));
                self.agent_finish(token, Ok(answer))
            }
            Request::WaitFor(_) => match value {
                Value::String(text) => {
                    self.agent_finish(token, Ok(json!({"page_id": page, "found": text})))
                }
                _ => self.agent_wait_again(token),
            },
            Request::EvaluateScript(args) => {
                if found.get("marked").is_none() {
                    found["marked"] = json!(true);
                    self.agent_set(token, Stage::Script, found);
                    return self.agent_evaluate(token, page, &args.function, args.args.len());
                }
                self.agent_finish(token, evaluated(page, value))
            }
            _ => self.agent_finish(token, Err(failure("unavailable", "Not a page call."))),
        }
    }

    fn agent_after_settle(
        &mut self,
        token: u64,
        page: Option<u64>,
        request: &Request,
        found: Value,
    ) -> Task<Event> {
        let Some(page) = page else {
            return Task::none();
        };
        let include = match request {
            Request::Click(args) => args.include_snapshot,
            _ => false,
        };
        self.agent_after_action(token, page, include, found)
    }

    fn agent_after_action(
        &mut self,
        token: u64,
        page: u64,
        include_snapshot: bool,
        found: Value,
    ) -> Task<Event> {
        if include_snapshot {
            let source = format!("{HELPER}\n__enzimAgent.snapshot({{}})");
            return self.agent_script(token, page, source, Stage::SecondScript);
        }
        let mut answer = json!({"page_id": page});
        if let Some(covered) = found.get("covered").filter(|v| !v.is_null()) {
            answer["covered_by"] = covered.clone();
        }
        if let Ok(state) = self.agent_page_state(page) {
            answer["url"] = state["url"].clone();
        }
        self.agent_finish(token, Ok(answer))
    }

    fn agent_wait_again(&mut self, token: u64) -> Task<Event> {
        self.agent_set(token, Stage::Load, Value::Null);
        if let Some(pending) = self
            .agent
            .as_mut()
            .and_then(|agent| agent.pending.get_mut(&token))
            && let Request::WaitFor(args) = &pending.request
        {
            pending.found = json!({"page": args.page_id});
        }
        poll(token, Duration::from_millis(250))
    }

    fn agent_set(&mut self, token: u64, stage: Stage, found: Value) {
        if let Some(pending) = self
            .agent
            .as_mut()
            .and_then(|agent| agent.pending.get_mut(&token))
        {
            pending.stage = stage;
            if !found.is_null() {
                pending.found = found;
            }
        }
    }

    fn agent_finish(&mut self, token: u64, outcome: CenterOutcome) -> Task<Event> {
        match self
            .agent
            .as_mut()
            .and_then(|agent| agent.pending.remove(&token))
        {
            Some(pending) => self.agent_answer(pending.call, outcome),
            None => Task::none(),
        }
    }

    /// Runs `source` in the agent's isolated world of the page.
    fn agent_script(&mut self, token: u64, page: u64, source: String, stage: Stage) -> Task<Event> {
        self.agent_set(token, stage, Value::Null);
        let world = ScriptWorld::Isolated(WORLD.into());
        match self.agent_browse(page, |job| BrowserAction::Evaluate { source, world, job }) {
            Ok(receiver) => Task::perform(crate::native_jobs::receive(receiver), move |result| {
                Event::Agent(AgentEvent::Script(
                    token,
                    result.and_then(|payload| match payload {
                        crate::native_jobs::Payload::Json(value) => Ok(value),
                        _ => Err("unexpected result".into()),
                    }),
                ))
            }),
            Err(error) => self.agent_finish(token, Err(error)),
        }
    }

    /// Runs the agent's function in the page's own world and awaits its value.
    fn agent_evaluate(
        &mut self,
        token: u64,
        page: u64,
        function: &str,
        args: usize,
    ) -> Task<Event> {
        self.agent_set(token, Stage::Script, Value::Null);
        // Elements marked in the isolated world are found again in the page's
        // own world by attribute, then unmarked.
        let source = format!(
            "await (async () => {{ const __args = []; for (let i = 0; i < {args}; i++) {{ const el = document.querySelector(`[data-enzim-agent-arg=\"${{i}}\"]`); el?.removeAttribute('data-enzim-agent-arg'); __args.push(el); }} const __value = await ({function})(...__args); return JSON.stringify(__value === undefined ? null : __value); }})()"
        );
        match self.agent_browse(page, |job| BrowserAction::Console { source, job }) {
            Ok(receiver) => Task::perform(crate::native_jobs::receive(receiver), move |result| {
                Event::Agent(AgentEvent::Script(
                    token,
                    result.and_then(|payload| match payload {
                        crate::native_jobs::Payload::Json(value) => Ok(value),
                        _ => Err("unexpected result".into()),
                    }),
                ))
            }),
            Err(error) => self.agent_finish(token, Err(error)),
        }
    }

    fn agent_capture(&mut self, token: u64, page: u64, _args: &TakeScreenshot) -> Task<Event> {
        match self.agent_browse(page, |job| BrowserAction::ExportScreenshot { job }) {
            Ok(receiver) => Task::perform(crate::native_jobs::receive(receiver), move |result| {
                Event::Agent(AgentEvent::Capture(
                    token,
                    result.and_then(|payload| match payload {
                        crate::native_jobs::Payload::Bytes(bytes) => Ok(bytes),
                        _ => Err("unexpected result".into()),
                    }),
                ))
            }),
            Err(error) => self.agent_finish(token, Err(error)),
        }
    }

    /// Sends an action to the page's own view, whether the tab is shown or not.
    fn agent_browse(
        &self,
        page: u64,
        action: impl FnOnce(crate::native_jobs::Job) -> BrowserAction,
    ) -> Result<
        tokio::sync::oneshot::Receiver<Result<crate::native_jobs::Payload, String>>,
        CenterFailure,
    > {
        let (id, index) = self.agent_locate(page).ok_or_else(|| not_found(page))?;
        let app = self.apps.get(&id).ok_or_else(|| not_found(page))?;
        let host =
            host(app, index).ok_or_else(|| failure("unavailable", "The page is not loaded."))?;
        let service = host.borrow();
        let (Some(native), Some(view)) = (&service.native, service.live_view) else {
            return Err(failure("unavailable", "The page is not loaded."));
        };
        let (job, receiver) = crate::native_jobs::Job::new();
        native
            .browse(&service.router, view, action(job))
            .map_err(|error| failure("unavailable", format!("{error:?}")))?;
        Ok(receiver)
    }

    /// Clicks or hovers at a point of the page given in CSS pixels.
    fn agent_pointer(
        &mut self,
        page: u64,
        x: f64,
        y: f64,
        double: bool,
        hover_only: bool,
    ) -> Result<(), CenterFailure> {
        let (id, index) = self.agent_locate(page).ok_or_else(|| not_found(page))?;
        let app = self.apps.get(&id).ok_or_else(|| not_found(page))?;
        if index != app.browser.saved.active {
            return Err(failure(
                "unavailable",
                "The tab could not be selected for input.",
            ));
        }
        let (view, zoom) = {
            let service = app.webview.borrow();
            let view = service
                .live_view
                .ok_or_else(|| failure("unavailable", "The page is not loaded."))?;
            (view, f64::from(service.zoom_percent) / 100.)
        };
        #[allow(
            clippy::cast_possible_truncation,
            reason = "view coordinates are well within f32"
        )]
        let at = ViewPoint {
            x: (x * zoom) as f32,
            y: (y * zoom) as f32,
        };
        let modifiers = Modifiers::NONE;
        let mut events = vec![InputEvent::PointerMoved {
            view,
            at,
            modifiers,
        }];
        if !hover_only {
            for _ in 0..if double { 2 } else { 1 } {
                events.push(InputEvent::PointerPressed {
                    view,
                    at,
                    button: PointerButton::Primary,
                    modifiers,
                });
                events.push(InputEvent::PointerReleased {
                    view,
                    at,
                    button: PointerButton::Primary,
                    modifiers,
                });
            }
        }
        for event in events {
            drop(self.handle(Event::App(id, Message::WebViewInput(event))));
        }
        Ok(())
    }

    /// Types the call's text or key into the shown page, then answers.
    fn agent_type(&mut self, token: u64, page: u64) -> Task<Event> {
        let Some(request) = self
            .agent
            .as_ref()
            .and_then(|agent| agent.pending.get(&token))
            .map(|p| p.request.clone())
        else {
            return Task::none();
        };
        let keys = match &request {
            Request::TypeText(args) => {
                let mut keys: Vec<(KeyCode, Modifiers)> = args
                    .text
                    .chars()
                    .map(|c| match c {
                        '\n' => (KeyCode::Named(NamedKey::Enter), Modifiers::NONE),
                        '\t' => (KeyCode::Named(NamedKey::Tab), Modifiers::NONE),
                        c => (KeyCode::Character(c.to_string()), Modifiers::NONE),
                    })
                    .collect();
                if let Some(submit) = &args.submit_key {
                    match parse_key(submit) {
                        Ok(key) => keys.push(key),
                        Err(error) => return self.agent_finish(token, Err(error)),
                    }
                }
                keys
            }
            Request::PressKey(args) => match parse_key(&args.key) {
                Ok(key) => vec![key],
                Err(error) => return self.agent_finish(token, Err(error)),
            },
            _ => return Task::none(),
        };
        let Some((id, index)) = self.agent_locate(page) else {
            return self.agent_finish(token, Err(not_found(page)));
        };
        let Some(app) = self.apps.get(&id) else {
            return Task::none();
        };
        if index != app.browser.saved.active {
            return self.agent_finish(
                token,
                Err(failure(
                    "unavailable",
                    "The tab could not be selected for input.",
                )),
            );
        }
        if app.webpage_keyboard_blocked() {
            return self.agent_finish(
                token,
                Err(failure("busy", "A browser panel or menu is open over the page; the page does not take keys until it closes.")),
            );
        }
        let Some(view) = app.webview.borrow().live_view else {
            return self.agent_finish(
                token,
                Err(failure("unavailable", "The page is not loaded.")),
            );
        };
        drop(self.handle(Event::App(
            id,
            Message::WebViewInput(InputEvent::FocusGained {
                view,
                modifiers: Modifiers::NONE,
            }),
        )));
        for (key, modifiers) in keys {
            drop(self.handle(Event::App(
                id,
                Message::WebViewInput(InputEvent::KeyPressed {
                    view,
                    key: key.clone(),
                    modifiers,
                }),
            )));
            drop(self.handle(Event::App(
                id,
                Message::WebViewInput(InputEvent::KeyReleased {
                    view,
                    key,
                    modifiers,
                }),
            )));
        }
        self.agent_set(token, Stage::Settle, json!({"page": page}));
        poll(token, SETTLE)
    }

    // -- Pages and windows ---------------------------------------------------

    /// Finds a tab agents may use; a private window's tab or one in a
    /// container closed to agents is not found.
    fn agent_locate(&self, page: u64) -> Option<(window::Id, usize)> {
        self.agent_find(page)
            .filter(|(id, index)| self.agent_tab_allowed(*id, *index))
    }

    fn agent_find(&self, page: u64) -> Option<(window::Id, usize)> {
        self.apps
            .iter()
            .filter(|(_, app)| !app.browser.private)
            .find_map(|(id, app)| {
                app.browser
                    .saved
                    .tabs
                    .iter()
                    .position(|tab| crate::page_ids::page_of(tab.key) == page)
                    .map(|index| (*id, index))
            })
    }

    fn agent_closed_page(&self, page: u64) -> bool {
        self.agent_find(page)
            .is_some_and(|(id, index)| !self.agent_tab_allowed(id, index))
    }

    fn agent_tab_allowed(&self, id: window::Id, index: usize) -> bool {
        self.apps
            .get(&id)
            .and_then(|app| app.browser.saved.tabs.get(index))
            .is_some_and(|tab| !self.agent_denied(tab.container))
    }

    fn agent_containers(&self) -> &[crate::profiles::Container] {
        self.apps
            .values()
            .find(|app| !app.browser.private)
            .map_or(&[], |app| app.browser.saved.containers.as_slice())
    }

    /// Whether the person closed this container (`None` is the default,
    /// which is always open) to agents. A container that no longer exists
    /// is closed: its tabs are on their way out.
    fn agent_denied(&self, container: Option<u64>) -> bool {
        container.is_some_and(|wanted| {
            self.agent_containers()
                .iter()
                .find(|c| c.id == wanted)
                .is_none_or(|c| c.privacy.deny_agents)
        })
    }

    /// The container an argument names: `None` when left out, `Some(None)`
    /// for the default. Opening in a closed container is refused.
    fn agent_container(
        &self,
        name: Option<&str>,
        opening: bool,
    ) -> Result<Option<Option<u64>>, CenterFailure> {
        let Some(name) = name.map(str::trim) else {
            return Ok(None);
        };
        if name.eq_ignore_ascii_case("default") {
            return Ok(Some(None));
        }
        let found = self
            .agent_containers()
            .iter()
            .find(|c| c.id.to_string() == name)
            .or_else(|| {
                self.agent_containers()
                    .iter()
                    .find(|c| c.name.trim().to_lowercase() == name.to_lowercase())
            })
            .ok_or_else(|| {
                failure(
                    "not_found",
                    "No such container; list_containers names them.",
                )
            })?;
        if found.privacy.deny_agents {
            return Err(failure(
                "refused",
                if opening {
                    "The person closed this container to agents; pages cannot be opened in it."
                } else {
                    "The person closed this container to agents; its tabs are not listed."
                },
            ));
        }
        Ok(Some(Some(found.id)))
    }

    fn agent_container_entry(&self, container: Option<u64>) -> Value {
        container
            .and_then(|id| self.agent_containers().iter().find(|c| c.id == id))
            .map_or(Value::Null, |c| {
                json!({"container_id": c.id.to_string(), "name": c.name})
            })
    }

    /// Whether a container uses a proxy is told; never its address or
    /// sign-in.
    fn agent_container_list(&self) -> Value {
        let mut containers = vec![json!({
            "container_id": "default",
            "name": "Default",
            "proxied": false,
            "agents_allowed": true,
        })];
        containers.extend(self.agent_containers().iter().map(|c| {
            json!({
                "container_id": c.id.to_string(),
                "name": c.name,
                "proxied": !c.privacy.upstream_proxy.trim().is_empty(),
                "agents_allowed": !c.privacy.deny_agents,
            })
        }));
        json!({"containers": containers})
    }

    fn agent_ready(&self, page: u64) -> Readiness {
        let Some((id, index)) = self.agent_locate(page) else {
            return Readiness::Missing;
        };
        let Some(app) = self.apps.get(&id) else {
            return Readiness::Missing;
        };
        let Some(host) = host(app, index) else {
            return Readiness::Waiting;
        };
        let service = host.borrow();
        let pending = index == app.browser.saved.active
            && (app.pending_navigation.is_some() || app.pending_native_open);
        if service.native.is_some() && service.live_view.is_some() && !service.loading && !pending {
            Readiness::Ready
        } else {
            Readiness::Waiting
        }
    }

    /// Selects the tab, so that it loads and takes input.
    fn agent_front(&mut self, page: u64, raise: bool) -> Task<Event> {
        let Some((id, index)) = self.agent_locate(page) else {
            return Task::none();
        };
        let Some(app) = self.apps.get_mut(&id) else {
            return Task::none();
        };
        let select = if index == app.browser.saved.active {
            Task::none()
        } else {
            app.browser_command(Command::Select(index))
                .map(move |message| Event::App(id, message))
        };
        let raise = if raise {
            window::gain_focus(id)
        } else {
            Task::none()
        };
        Task::batch([select, raise])
    }

    fn agent_page_loaded(&self, page: u64) -> bool {
        self.agent_locate(page).is_some_and(|(id, index)| {
            self.apps
                .get(&id)
                .and_then(|app| host(app, index))
                .is_some_and(|host| host.borrow().native.is_some())
        })
    }

    /// Loads a tab that is not loaded; a loaded tab stays where it is.
    fn agent_load(&mut self, page: u64) -> Task<Event> {
        let Some((id, index)) = self.agent_locate(page) else {
            return Task::none();
        };
        let loaded = self
            .apps
            .get(&id)
            .and_then(|app| host(app, index))
            .is_some_and(|host| host.borrow().native.is_some());
        if loaded {
            Task::none()
        } else {
            self.agent_front(page, false)
        }
    }

    fn agent_page_state(&self, page: u64) -> CenterOutcome {
        let (id, index) = self.agent_locate(page).ok_or_else(|| not_found(page))?;
        let app = self.apps.get(&id).ok_or_else(|| not_found(page))?;
        let tab = &app.browser.saved.tabs[index];
        Ok(json!({"page_id": page, "url": tab.url, "title": tab.title}))
    }

    fn agent_windows(&mut self) -> (Value, Task<Event>) {
        let ids: Vec<_> = self
            .apps
            .iter()
            .filter(|(_, app)| !app.browser.private)
            .map(|(id, _)| *id)
            .collect();
        let mut windows = Vec::new();
        for id in ids {
            let number = self.agent.as_mut().map_or(0, |agent| agent.window_id(id));
            let Some(app) = self.apps.get(&id) else {
                continue;
            };
            let active = &app.browser.saved.tabs[app.browser.saved.active];
            let shown = !self.agent_denied(active.container);
            windows.push(json!({
                "window_id": number,
                "tabs": app.browser.saved.tabs.iter().filter(|tab| !self.agent_denied(tab.container)).count(),
                "selected_page_id": shown.then(|| crate::page_ids::page_of(active.key)),
                "title": if shown { active.title.as_str() } else { "" },
            }));
        }
        (json!({"windows": windows}), Task::none())
    }

    /// `container` is `None` for every container agents may use, or the one
    /// wanted (`Some(None)` is the default).
    fn agent_pages(
        &mut self,
        window: Option<u64>,
        container: Option<Option<u64>>,
    ) -> Result<(Value, Task<Event>), CenterFailure> {
        let ids: Vec<_> = self
            .apps
            .iter()
            .filter(|(_, app)| !app.browser.private)
            .map(|(id, _)| *id)
            .collect();
        let mut pages = Vec::new();
        let mut matched = window.is_none();
        for id in ids {
            let number = self.agent.as_mut().map_or(0, |agent| agent.window_id(id));
            if window.is_some_and(|wanted| wanted != number) {
                continue;
            }
            matched = true;
            let Some(app) = self.apps.get(&id) else {
                continue;
            };
            for (index, tab) in app.browser.saved.tabs.iter().enumerate() {
                if self.agent_denied(tab.container)
                    || container.is_some_and(|wanted| wanted != tab.container)
                {
                    continue;
                }
                let service = host(app, index);
                let loaded = service.is_some_and(|host| host.borrow().native.is_some());
                let loading = service.is_some_and(|host| host.borrow().loading);
                let mut entry = json!({
                    "page_id": crate::page_ids::page_of(tab.key),
                    "window_id": number,
                    "index": index,
                    "url": tab.url,
                    "title": tab.title,
                    "selected": index == app.browser.saved.active,
                    "loaded": loaded,
                    "loading": loading,
                    "pinned": tab.pinned,
                    "container": self.agent_container_entry(tab.container),
                });
                if app.browser.prompt_tab == Some(tab.key)
                    && let Some(dialog) = &app.browser.dialog
                {
                    entry["dialog"] = json!(dialog_message(dialog));
                }
                pages.push(entry);
            }
        }
        if !matched {
            return Err(failure(
                "not_found",
                "No such window; list_windows names them.",
            ));
        }
        Ok((json!({"pages": pages}), Task::none()))
    }

    fn agent_new_page(
        &mut self,
        url: &str,
        window: Option<u64>,
        background: bool,
        container: Option<u64>,
    ) -> Result<(u64, Task<Event>), CenterFailure> {
        let id = match window {
            Some(number) => self
                .agent
                .as_ref()
                .and_then(|agent| agent.window(number))
                .filter(|id| self.apps.get(id).is_some_and(|app| !app.browser.private))
                .ok_or_else(|| failure("not_found", "No such window; list_windows names them."))?,
            None => self
                .apps
                .iter()
                .find(|(_, app)| !app.browser.private)
                .map(|(id, _)| *id)
                .ok_or_else(|| failure("unavailable", "No browser window is open."))?,
        };
        let app = self
            .apps
            .get_mut(&id)
            .ok_or_else(|| failure("unavailable", "The window closed."))?;
        let task = if background {
            let resolved = app.browser.saved.search.resolve(url);
            app.browser.saved.tabs.push(crate::browser::TabInfo {
                key: crate::browser::next_tab_key(),
                favicon: None,
                title: crate::browser::site_name(&resolved),
                url: resolved,
                pinned: false,
                group: None,
                container,
            });
            app.browser.sleeping.push(None);
            Task::none()
        } else if container.is_some() {
            app.browser_command(Command::ExternalVisitIn(url.to_owned(), container))
        } else {
            app.browser_command(Command::ExternalVisit(url.to_owned()))
        };
        let key = app
            .browser
            .saved
            .tabs
            .last()
            .map(|tab| tab.key)
            .ok_or_else(|| failure("unavailable", "The tab was not opened."))?;
        Ok((
            crate::page_ids::page_of(key),
            task.map(move |message| Event::App(id, message)),
        ))
    }

    fn agent_close_page(&mut self, page: u64) -> Result<(Value, Task<Event>), CenterFailure> {
        let (id, index) = self.agent_locate(page).ok_or_else(|| not_found(page))?;
        let windows = self
            .apps
            .values()
            .filter(|app| !app.browser.private)
            .count();
        let app = self.apps.get_mut(&id).ok_or_else(|| not_found(page))?;
        if app.browser.saved.tabs[index].pinned {
            return Err(failure(
                "refused",
                "The tab is pinned; the person pinned it to keep it.",
            ));
        }
        if app.browser.saved.tabs.len() == 1 && windows == 1 {
            return Err(failure(
                "refused",
                "This is the last tab; open another page first.",
            ));
        }
        if app.browser.saved.tabs.len() == 1 {
            // A window's last tab closes the window.
            return Ok((json!({"closed": true}), self.update(Event::Close(id))));
        }
        let task = app.browser_command(Command::Close(index));
        Ok((
            json!({"closed": true}),
            task.map(move |message| Event::App(id, message)),
        ))
    }

    fn agent_new_window(
        &mut self,
        url: Option<String>,
        container: Option<u64>,
    ) -> Result<(Value, Task<Event>), CenterFailure> {
        let parent = self
            .apps
            .iter()
            .find(|(_, app)| !app.browser.private)
            .or_else(|| self.apps.iter().next())
            .map(|(id, _)| *id)
            .ok_or_else(|| failure("unavailable", "No browser window is open."))?;
        let before: Vec<_> = self.apps.keys().copied().collect();
        let task = self.open_window(parent, false, url, None, None);
        let id = self
            .apps
            .keys()
            .copied()
            .find(|id| !before.contains(id))
            .ok_or_else(|| failure("unavailable", "The window was not opened."))?;
        let number = self.agent.as_mut().map_or(0, |agent| agent.window_id(id));
        // The tab's first load runs after this returns, in its container.
        let page = self
            .apps
            .get_mut(&id)
            .map(|app| {
                app.browser.saved.tabs[0].container = container;
                crate::page_ids::page_of(app.browser.saved.tabs[0].key)
            })
            .unwrap_or_default();
        Ok((json!({"window_id": number, "page_id": page}), task))
    }

    fn agent_close_window(&mut self, number: u64) -> Result<(Value, Task<Event>), CenterFailure> {
        let id = self
            .agent
            .as_ref()
            .and_then(|agent| agent.window(number))
            .filter(|id| self.apps.get(id).is_some_and(|app| !app.browser.private))
            .ok_or_else(|| failure("not_found", "No such window; list_windows names them."))?;
        if self.apps.len() == 1 {
            return Err(failure(
                "refused",
                "This is the last window; closing it would quit the browser.",
            ));
        }
        if self.apps.get(&id).is_some_and(|app| {
            app.browser
                .saved
                .tabs
                .iter()
                .any(|tab| self.agent_denied(tab.container))
        }) {
            return Err(failure(
                "refused",
                "The window holds tabs of a container the person closed to agents.",
            ));
        }
        Ok((json!({"closed": true}), self.update(Event::Close(id))))
    }

    fn agent_dialog(
        &mut self,
        page: u64,
        action: DialogAction,
        text: Option<String>,
    ) -> Result<(Value, Task<Event>), CenterFailure> {
        let (id, _) = self.agent_locate(page).ok_or_else(|| not_found(page))?;
        let app = self.apps.get_mut(&id).ok_or_else(|| not_found(page))?;
        let owner = app.browser.prompt_tab.map(crate::page_ids::page_of);
        let Some(dialog) = app.browser.dialog.as_ref().filter(|_| owner == Some(page)) else {
            return Err(failure("not_found", "The page shows no dialog."));
        };
        let message = dialog_message(dialog);
        let mut tasks = Vec::new();
        if let Some(text) = text {
            tasks.push(app.browser_command(Command::PromptText(text)));
        }
        tasks.push(app.browser_command(Command::AnswerDialog(action == DialogAction::Accept)));
        Ok((
            json!({"page_id": page, "message": message}),
            Task::batch(tasks).map(move |message| Event::App(id, message)),
        ))
    }
}

enum Input {
    Script(Result<Value, String>),
    Capture(Result<std::sync::Arc<Vec<u8>>, String>),
}

enum Readiness {
    Missing,
    Waiting,
    Ready,
}

fn host(
    app: &crate::App,
    index: usize,
) -> Option<&std::cell::RefCell<crate::webview::WebViewService>> {
    if index == app.browser.saved.active {
        Some(&app.webview)
    } else {
        app.browser
            .sleeping
            .get(index)?
            .as_ref()
            .map(|tab| &tab.host)
    }
}

fn assistant_answer(window: window::Id, id: String, result: Result<Value, String>) -> Task<Event> {
    Task::done(Event::App(
        window,
        Message::Assistant(crate::assistant::Msg::ToolResult(id, result)),
    ))
}

/// The assistant's call of `function` as the request the center would send.
fn decode(function: &str, arguments: Value) -> Result<Request, String> {
    static REGISTRY: std::sync::LazyLock<Option<enzui_agent::center::AppFunctionRegistry<Request>>> =
        std::sync::LazyLock::new(|| registry().ok());
    let registry = REGISTRY.as_ref().ok_or("The browser's functions are unavailable.")?;
    let arguments = if arguments.is_null() { json!({}) } else { arguments };
    // The registry decodes an invocation; the assistant has no center call,
    // so its caller fields are placeholders.
    let invocation = serde_json::from_value(json!({
        "invocation_id": "00000000-0000-0000-0000-000000000000",
        "function_id": function,
        "arguments": arguments,
        "caller": {
            "credential_id": "assistant",
            "display_name": "Assistant",
            "session_id": "00000000-0000-0000-0000-000000000000",
        },
        "deadline_unix_ms": 0,
        "idempotency_key": null,
    }))
    .map_err(|_| "The call could not be read.".to_owned())?;
    registry.dispatch(&invocation).map_err(|error| match error {
        enzui_agent::center::DispatchError::UnknownFunction { .. } => {
            format!("There is no function {function}.")
        }
        _ => format!("The arguments for {function} do not fit its schema."),
    })
}

fn page_of_request(request: &Request) -> Option<u64> {
    Some(match request {
        Request::SelectPage(a) | Request::ClosePage(a) => a.page_id,
        Request::NavigatePage(a) => a.page_id,
        Request::TakeSnapshot(a) => a.page_id,
        Request::TakeScreenshot(a) => a.page_id,
        Request::Click(a) => a.page_id,
        Request::Hover(a) => a.page_id,
        Request::Fill(a) => a.page_id,
        Request::FillForm(a) => a.page_id,
        Request::TypeText(a) => a.page_id,
        Request::PressKey(a) => a.page_id,
        Request::Scroll(a) => a.page_id,
        Request::WaitFor(a) => a.page_id,
        Request::EvaluateScript(a) => a.page_id,
        Request::HandleDialog(a) => a.page_id,
        Request::ListWindows
        | Request::ListContainers
        | Request::ListPages(_)
        | Request::NewPage(_)
        | Request::NewWindow(_)
        | Request::CloseWindow(_) => return None,
    })
}

/// Pointer and key input reaches only the shown tab.
fn needs_front(request: &Request) -> bool {
    matches!(
        request,
        Request::Click(_) | Request::Hover(_) | Request::TypeText(_) | Request::PressKey(_)
    )
}

fn dialog_message(dialog: &enzui_webkit::page::ScriptDialogRequest) -> String {
    format!("{dialog:?}").chars().take(512).collect()
}

fn script_failure(error: &str) -> CenterFailure {
    // WebKit reports `source:line:column: Error: message`; the helper's own
    // refusals start their message with a code.
    let found = [
        ("stale_uid: ", "stale_uid"),
        ("no_option: ", "invalid_input"),
        ("not_fillable: ", "invalid_input"),
    ]
    .into_iter()
    .find_map(|(marker, code)| {
        error
            .find(marker)
            .map(|at| (code, error.get(at + marker.len()..).unwrap_or_default()))
    });
    let (code, message) = found.unwrap_or_else(|| {
        (
            "script_error",
            error.split_once("Error: ").map_or(error, |(_, rest)| rest),
        )
    });
    // Script messages come from the page; keep them short.
    failure(code, message.chars().take(1024).collect::<String>())
}

fn evaluated(page: u64, value: Value) -> CenterOutcome {
    // The console path answers {kind, preview}; a JSON string came back as text.
    let preview = match &value {
        Value::Object(map) => map.get("preview").cloned().unwrap_or(Value::Null),
        other => other.clone(),
    };
    let result = match preview {
        Value::String(text) => match serde_json::from_str::<Value>(&text) {
            Ok(Value::String(inner)) => {
                serde_json::from_str(&inner).unwrap_or(Value::String(inner))
            }
            Ok(other) => other,
            Err(_) => Value::String(text),
        },
        other => other,
    };
    Ok(json!({"page_id": page, "result": result}))
}

fn parse_key(spec: &str) -> Result<(KeyCode, Modifiers), CenterFailure> {
    let parts: Vec<&str> = spec.split('+').map(str::trim).collect();
    let (key, held) = parts
        .split_last()
        .filter(|(key, _)| !key.is_empty())
        .ok_or_else(|| failure("invalid_input", "Name a key, such as Enter or Control+A."))?;
    let mut modifiers = Modifiers::NONE.bits();
    for modifier in held {
        modifiers |= match modifier.to_ascii_lowercase().as_str() {
            "control" | "ctrl" => Modifiers::CTRL.bits(),
            "shift" => Modifiers::SHIFT.bits(),
            "alt" | "option" => Modifiers::ALT.bits(),
            "meta" | "super" | "command" | "cmd" => Modifiers::META.bits(),
            other => {
                return Err(failure(
                    "invalid_input",
                    format!("Unknown modifier {other}."),
                ));
            }
        };
    }
    let named = |name: NamedKey| KeyCode::Named(name);
    let code = match key.to_ascii_lowercase().as_str() {
        "enter" | "return" => named(NamedKey::Enter),
        "tab" => named(NamedKey::Tab),
        "escape" | "esc" => named(NamedKey::Escape),
        "backspace" => named(NamedKey::Backspace),
        "delete" | "del" => named(NamedKey::Delete),
        "space" | " " => named(NamedKey::Space),
        "arrowup" | "up" => named(NamedKey::ArrowUp),
        "arrowdown" | "down" => named(NamedKey::ArrowDown),
        "arrowleft" | "left" => named(NamedKey::ArrowLeft),
        "arrowright" | "right" => named(NamedKey::ArrowRight),
        "home" => named(NamedKey::Home),
        "end" => named(NamedKey::End),
        "pageup" => named(NamedKey::PageUp),
        "pagedown" => named(NamedKey::PageDown),
        "insert" => named(NamedKey::Insert),
        "f1" => named(NamedKey::F1),
        "f2" => named(NamedKey::F2),
        "f3" => named(NamedKey::F3),
        "f4" => named(NamedKey::F4),
        "f5" => named(NamedKey::F5),
        "f6" => named(NamedKey::F6),
        "f7" => named(NamedKey::F7),
        "f8" => named(NamedKey::F8),
        "f9" => named(NamedKey::F9),
        "f10" => named(NamedKey::F10),
        "f11" => named(NamedKey::F11),
        "f12" => named(NamedKey::F12),
        _ if key.chars().count() == 1 => {
            let shifted = modifiers & Modifiers::SHIFT.bits() != 0;
            KeyCode::Character(if shifted {
                key.to_uppercase()
            } else {
                (*key).to_owned()
            })
        }
        _ => return Err(failure("invalid_input", format!("Unknown key {key}."))),
    };
    Ok((code, Modifiers::from_bits(modifiers)))
}

/// The visible part of the page, or the whole page, as a JPEG within the
/// result limit.
fn screenshot(page: u64, png: &[u8], found: &Value, args: &TakeScreenshot) -> CenterOutcome {
    use base64::Engine as _;
    let image =
        image::load_from_memory_with_format(png, image::ImageFormat::Png).map_err(|error| {
            failure(
                "unavailable",
                format!("The capture could not be read: {error}"),
            )
        })?;
    let viewport = &found["viewport"];
    let number = |key: &str| {
        viewport
            .get("viewport")
            .and_then(|v| v.get(key))
            .and_then(Value::as_f64)
    };
    let document_width = viewport
        .get("document_width")
        .and_then(Value::as_f64)
        .unwrap_or(0.);
    let cropped = if args.full_page || document_width <= 0. {
        image
    } else {
        let scale = f64::from(image.width()) / document_width;
        let (x, y, w, h) = (
            number("scroll_x").unwrap_or(0.) * scale,
            number("scroll_y").unwrap_or(0.) * scale,
            number("width").unwrap_or_else(|| f64::from(image.width())) * scale,
            number("height").unwrap_or_else(|| f64::from(image.height())) * scale,
        );
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped to the image below"
        )]
        let (x, y, w, h) = (
            x.max(0.) as u32,
            y.max(0.) as u32,
            w.max(1.) as u32,
            h.max(1.) as u32,
        );
        let x = x.min(image.width().saturating_sub(1));
        let y = y.min(image.height().saturating_sub(1));
        image.crop_imm(x, y, w.min(image.width() - x), h.min(image.height() - y))
    };
    let mut width = args.max_width.unwrap_or(1280).min(cropped.width());
    let mut quality = args.quality.unwrap_or(70);
    loop {
        let scaled = if width < cropped.width() {
            cropped.resize(width, u32::MAX, image::imageops::FilterType::Triangle)
        } else {
            cropped.clone()
        };
        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, quality)
            .encode_image(&scaled.to_rgb8())
            .map_err(|error| {
                failure(
                    "unavailable",
                    format!("The picture could not be made: {error}"),
                )
            })?;
        let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
        if data.len() < 480 * 1024 || width <= 320 {
            return Ok(json!({
                "page_id": page,
                "mime_type": "image/jpeg",
                "width": scaled.width(),
                "height": scaled.height(),
                "data": data,
            }));
        }
        width = width * 3 / 4;
        quality = quality.saturating_sub(10).max(30);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_registry_validates() {
        let registry = registry().expect("valid registry");
        assert!(registry.len() >= 20, "every function is registered");
    }

    #[test]
    fn keys_parse_like_chrome_names() {
        let (key, modifiers) = parse_key("Control+Shift+R").expect("combination");
        assert_eq!(key, KeyCode::Character("R".into()), "shifted letter");
        assert!(
            modifiers.contains(Modifiers::CTRL) && modifiers.contains(Modifiers::SHIFT),
            "both held"
        );
        assert_eq!(
            parse_key("Enter").expect("named").0,
            KeyCode::Named(NamedKey::Enter),
            "named key"
        );
        assert!(parse_key("Hyper+A").is_err(), "unknown modifier refused");
    }

    #[test]
    fn page_errors_keep_their_code_without_the_location() {
        let failure =
            script_failure("http://x.test/a:21:94: Error: stale_uid: take a new snapshot");
        assert_eq!(
            (failure.code, failure.message.as_str()),
            ("stale_uid", "take a new snapshot"),
            "coded"
        );
        let failure = script_failure("http://x.test/a:1:2: TypeError: Error: boom");
        assert_eq!(failure.code, "script_error", "other errors");
    }

    fn page(windows: &crate::windows::Windows, id: window::Id, index: usize) -> u64 {
        crate::page_ids::page_of(windows.apps[&id].browser.saved.tabs[index].key)
    }

    /// Two windows; the first holds a default tab, a Work tab and a Banking
    /// tab, and the person closed Banking to agents.
    fn contained() -> (crate::windows::Windows, window::Id, window::Id, u64, u64) {
        let (mut windows, a, b) = crate::windows::tests::windows();
        let ids: Vec<u64> = windows.apps[&a]
            .browser
            .saved
            .containers
            .iter()
            .map(|c| c.id)
            .collect();
        let (work, banking) = (ids[1], ids[2]);
        for app in windows.apps.values_mut() {
            app.browser.saved.containers[2].privacy.deny_agents = true;
        }
        let app = windows.apps.get_mut(&a).expect("window");
        for container in [Some(work), Some(banking)] {
            let mut tab = app.browser.saved.tabs[0].clone();
            tab.key = crate::browser::next_tab_key();
            tab.container = container;
            app.browser.saved.tabs.push(tab);
            app.browser.sleeping.push(None);
        }
        (windows, a, b, work, banking)
    }

    #[test]
    fn containers_are_listed_without_their_proxy() {
        let (mut windows, _, _, work, _) = contained();
        for app in windows.apps.values_mut() {
            app.browser.saved.containers[1].privacy.upstream_proxy = "127.0.0.1:9050".into();
            app.browser.saved.containers[1].privacy.proxy_username = "someone".into();
        }
        let listed = windows.agent_container_list();
        let text = listed.to_string();
        assert!(
            // Container ids are random digits: look for the whole address.
            !text.contains("127.0.0.1:9050") && !text.contains("someone"),
            "no address or sign-in: {text}"
        );
        let containers = listed["containers"].as_array().expect("array");
        assert_eq!(containers[0]["container_id"], "default", "default first");
        let work = work.to_string();
        let entry = containers
            .iter()
            .find(|c| c["container_id"] == work.as_str())
            .expect("Work listed");
        assert_eq!(
            (&entry["name"], &entry["proxied"], &entry["agents_allowed"]),
            (&json!("Work"), &json!(true), &json!(true)),
            "Work"
        );
        assert_eq!(containers[3]["agents_allowed"], false, "Banking closed");
    }

    #[test]
    fn pages_name_their_container_and_closed_ones_are_hidden() {
        let (mut windows, a, _, work, _) = contained();
        let (all, _) = windows.agent_pages(None, None).expect("pages");
        let pages = all["pages"].as_array().expect("array");
        assert_eq!(pages.len(), 3, "the Banking tab is hidden: {all}");
        assert!(pages[0]["container"].is_null(), "default tab");
        assert_eq!(
            pages[1]["container"],
            json!({"container_id": work.to_string(), "name": "Work"}),
            "Work tab"
        );
        let by_name = windows.agent_container(Some("work"), false).expect("named");
        let (only, _) = windows.agent_pages(None, by_name).expect("filtered");
        assert_eq!(only["pages"].as_array().map(Vec::len), Some(1), "only Work");
        let default = windows.agent_container(Some("default"), false).expect("default");
        let (rest, _) = windows.agent_pages(None, default).expect("filtered");
        assert_eq!(rest["pages"].as_array().map(Vec::len), Some(2), "both windows' default tabs");
        assert_eq!(
            windows.agent_container(Some("Banking"), false).map_err(|f| f.code),
            Err("refused"),
            "closed container"
        );
        assert_eq!(
            windows.agent_container(Some("Nowhere"), true).map_err(|f| f.code),
            Err("not_found"),
            "unknown container"
        );
        let (listed, _) = windows.agent_windows();
        let first = listed["windows"]
            .as_array()
            .and_then(|w| w.iter().find(|w| w["tabs"] == 2))
            .cloned();
        assert!(first.is_some(), "the closed tab is not counted: {listed}");
        let _ = a;
    }

    #[test]
    fn closed_container_tabs_cannot_be_reached() {
        let (mut windows, a, _, _, banking) = contained();
        let closed = page(&windows, a, 2);
        assert!(windows.agent_locate(closed).is_none(), "not found");
        assert!(windows.agent_closed_page(closed), "known as closed");
        assert!(!windows.agent_closed_page(page(&windows, a, 1)), "Work is open");
        assert_eq!(
            windows.agent_close_page(closed).map(|_| ()).map_err(|f| f.code),
            Err("not_found"),
            "close refused"
        );
        assert!(windows.agent_denied(Some(banking)), "Banking");
        assert!(windows.agent_denied(Some(u64::MAX)), "a removed container");
        assert!(!windows.agent_denied(None), "default");
    }

    #[test]
    fn new_pages_open_in_the_container_asked_for() {
        let (mut windows, _, _, work, _) = contained();
        let container = windows
            .agent_container(Some(&work.to_string()), true)
            .expect("by id")
            .flatten();
        let (opened, _) = windows
            .agent_new_page("https://example.test/", None, true, container)
            .expect("opened");
        let (id, index) = windows.agent_locate(opened).expect("listed");
        assert_eq!(
            windows.apps[&id].browser.saved.tabs[index].container,
            Some(work),
            "in Work"
        );
        assert_eq!(
            windows.agent_container(Some("Banking"), true).map_err(|f| f.code),
            Err("refused"),
            "closed container refused"
        );
    }

    #[test]
    fn evaluated_values_come_back_as_json() {
        let value = json!({"kind": "json", "preview": "\"{\\\"a\\\":1}\""});
        assert_eq!(
            evaluated(3, value).expect("ok")["result"],
            json!({"a": 1}),
            "decoded twice"
        );
    }
}
