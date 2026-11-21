//! One turn of the assistant, on a thread of its own.
//!
//! A turn asks the model, runs the tools it calls and asks again, until the
//! model answers without calling a tool. The browser's own functions run on
//! the UI thread: the turn hands them over ([`Event::Run`]) and waits for
//! their result on its input channel, which also brings the person's
//! steering. Text reaches the UI a paragraph at a time, not per token, so a
//! reply redraws the panel a few times rather than hundreds.

use super::provider::{self, Assembler, Delta, Endpoint};
use base64::Engine as _;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// A paragraph that keeps growing is still shown this often.
const TEXT_GATE: Duration = Duration::from_millis(900);
const THOUGHT_GATE: Duration = Duration::from_millis(1200);
/// How much of one tool result the model gets.
const RESULT_LIMIT: usize = 48 * 1024;
/// Characters of a page outline per part, unless the model asks otherwise.
const SNAPSHOT_PART: u32 = 16_000;
/// Older bulky tool results are shortened to keep requests small.
const KEEP_RECENT_RESULTS: usize = 6;

/// How a turn reaches its model.
#[derive(Clone, Debug)]
pub(crate) enum Source {
    /// `key` names the keyring record of the service's key.
    Compatible { base: String, model: String, key: String },
    ChatGpt { model: String },
}

/// What a turn needs.
pub(crate) struct Turn {
    pub(crate) source: Source,
    /// The conversation so far, system message first, ending with the
    /// person's new message.
    pub(crate) messages: Vec<Value>,
    /// The browser's functions and the panel's own tools.
    pub(crate) tools: Vec<Value>,
    /// Whether the agent center's tools are offered.
    pub(crate) apps: bool,
    /// The saved notes, when memory is on.
    pub(crate) memories: Option<Vec<String>>,
}

/// A link the assistant showed with `show_links`.
#[derive(Debug, Clone)]
pub(crate) struct Link {
    pub(crate) url: String,
    pub(crate) title: String,
    pub(crate) note: String,
}

#[derive(Debug, Clone)]
pub(crate) enum Event {
    Started(Handle),
    /// The reasoning of step `step` so far.
    Thought { step: usize, text: String, done: bool },
    /// The reply of step `step` so far.
    Text { step: usize, text: String, done: bool },
    /// A tool call began; `arguments` as the model wrote them.
    Tool { id: String, name: String, arguments: String },
    /// A browser function for the UI thread to run.
    Run { id: String, name: String, arguments: Value },
    ToolDone {
        id: String,
        ok: bool,
        output: String,
        image: Option<Arc<Vec<u8>>>,
    },
    Suggest(Vec<String>),
    Links(Vec<Link>),
    /// What the turn waits for, shown in the status strip.
    Status(String),
    /// What step `step` said came with function calls: a note on the way,
    /// not the answer.
    Interim(usize),
    /// The saved notes changed; the panel keeps them in the settings.
    Memory(Vec<String>),
    /// Whether the answer is complete and only its extras (suggested replies,
    /// link cards) remain; the panel then shows the turn as finished.
    Settled(bool),
    /// A steering message reached the model.
    Steered,
    Finished {
        messages: Vec<Value>,
        error: Option<String>,
        stopped: bool,
    },
}

pub(crate) enum Input {
    Steer(Value),
    Result(String, Result<Value, String>),
}

/// Stops or steers a running turn.
#[derive(Clone, Debug)]
pub(crate) struct Handle {
    cancel: Arc<AtomicBool>,
    input: mpsc::Sender<Input>,
}

impl Handle {
    pub(crate) fn stop(&self) {
        self.cancel.store(true, Ordering::Release);
    }
    pub(crate) fn steer(&self, message: Value) {
        let _ = self.input.send(Input::Steer(message));
    }
    pub(crate) fn answer(&self, id: String, result: Result<Value, String>) {
        let _ = self.input.send(Input::Result(id, result));
    }
}

/// The panel's own tools: they shape how the answer is shown.
pub(crate) fn panel_tools() -> Vec<Value> {
    vec![
        json!({"type": "function", "function": {
            "name": "suggest_replies",
            "description": "Offer the person up to 4 short replies or next steps as buttons under your answer. Use after an answer when an obvious follow-up exists.",
            "parameters": {"type": "object", "properties": {"replies": {"type": "array", "items": {"type": "string", "maxLength": 60}, "minItems": 1, "maxItems": 4}}, "required": ["replies"]},
        }}),
        json!({"type": "function", "function": {
            "name": "web_search",
            "description": "Search the web without opening a tab; returns titles, addresses and snippets. Then read a result with read_web_page.",
            "parameters": {"type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"]},
        }}),
        json!({"type": "function", "function": {
            "name": "read_web_page",
            "description": "Read the text of a public web page without opening a tab, such as to check a link or look something up. No cookies or sign-ins are sent.",
            "parameters": {"type": "object", "properties": {"url": {"type": "string"}}, "required": ["url"]},
        }}),
        json!({"type": "function", "function": {
            "name": "show_links",
            "description": "Show up to 6 links as cards with a preview of each page. Use when recommending pages to open.",
            "parameters": {"type": "object", "properties": {"links": {"type": "array", "maxItems": 6, "items": {"type": "object", "properties": {"url": {"type": "string"}, "title": {"type": "string"}, "note": {"type": "string", "description": "One short line on why it is worth opening."}}, "required": ["url"]}}}, "required": ["links"]},
        }}),
    ]
}

/// Starts a turn; its events arrive on the returned stream.
pub(crate) fn start(turn: Turn) -> impl iced::futures::Stream<Item = Event> {
    iced::stream::channel(64, move |mut output: iced::futures::channel::mpsc::Sender<Event>| async move {
        let (events, mut received) = iced::futures::channel::mpsc::unbounded();
        let cancel = Arc::new(AtomicBool::new(false));
        let (input, inputs) = mpsc::channel();
        let handle = Handle {
            cancel: Arc::clone(&cancel),
            input,
        };
        let _ = events.unbounded_send(Event::Started(handle));
        std::thread::Builder::new()
            .name("assistant turn".into())
            .spawn(move || {
                let send = |event: Event| {
                    let _ = events.unbounded_send(event);
                };
                run(turn, &cancel, &inputs, &send);
            })
            .ok();
        use iced::futures::{SinkExt as _, StreamExt as _};
        while let Some(event) = received.next().await {
            if output.send(event).await.is_err() {
                break;
            }
        }
    })
}

/// Function calls in one turn between reminders to finish.
const NUDGE_EVERY: usize = 12;

/// The longest text beside function calls that is folded as a note.
const INTERIM_LIMIT: usize = 400;

/// Waits before asking a busy service again.
#[cfg(not(test))]
const RETRIES: [Duration; 3] = [Duration::from_secs(3), Duration::from_secs(8), Duration::from_secs(20)];
#[cfg(test)]
const RETRIES: [Duration; 3] = [Duration::from_millis(10); 3];

/// Functions that only add to an answer: a step that said something and called
/// only these ends the turn without asking the model again.
const EXTRAS: [&str; 2] = ["suggest_replies", "show_links"];

fn run(mut turn: Turn, cancel: &AtomicBool, inputs: &mpsc::Receiver<Input>, send: &dyn Fn(Event)) {
    let finish = |messages: Vec<Value>, error: Option<String>| {
        send(Event::Finished {
            messages,
            error,
            stopped: cancel.load(Ordering::Acquire),
        });
    };
    let mut apps = None;
    if turn.apps {
        let center = super::mcp::program("enzui-mcp")
            .ok_or_else(|| "The agent center is not installed.".to_owned())
            .and_then(|program| super::mcp::Client::start(&program, &super::mcp::credential_path()));
        match center.and_then(|mut client| client.tools().map(|tools| (client, tools))) {
            Ok((client, tools)) => {
                turn.tools.extend(tools);
                apps = Some(client);
            }
            Err(error) => send(Event::Status(format!("Other apps are unavailable: {error}"))),
        }
    }
    let mut steers = Vec::new();
    // The turn's function calls so far, as name and arguments, in order.
    let mut made: Vec<(String, Value)> = Vec::new();
    // Whether the turn was already asked to act on an announced step.
    let mut pressed = false;
    // A turn has no step or time limit: it runs until the model answers
    // without calling a tool, an error, or Stop.
    for step in 0usize.. {
        if cancel.load(Ordering::Acquire) {
            return finish(turn.messages, None);
        }
        drain(inputs, &mut steers, &mut Vec::new());
        if !steers.is_empty() {
            turn.messages.append(&mut steers);
            send(Event::Steered);
        }
        shorten_old_results(&mut turn.messages);
        let endpoint = match endpoint(&turn.source) {
            Ok(endpoint) => endpoint,
            Err(error) => return finish(turn.messages, Some(error)),
        };
        let model = match &turn.source {
            Source::Compatible { model, .. } | Source::ChatGpt { model } => model.clone(),
        };
        let body = provider::body(&endpoint, &model, &turn.messages, &turn.tools);
        let mut attempt = 0usize;
        let (said, streamed) = loop {
            let mut assembler = Assembler::default();
            let mut shown_text = 0usize;
            // The gate counts from the reply's first words, not the request.
            let mut text_at: Option<Instant> = None;
            let mut thought_at = Instant::now();
            let mut settled = false;
            let chatgpt = matches!(endpoint, Endpoint::ChatGpt { .. });
            let streamed = provider::stream(&endpoint, &body, cancel, |data| {
                let changes = if chatgpt {
                    assembler.responses_event(data)?
                } else {
                    assembler.chat_event(data)?
                };
                if changes.contains(&Delta::Text) {
                    let text = &assembler.said.text;
                    let since = *text_at.get_or_insert_with(Instant::now);
                    if let Some(end) = paragraph_end(text, shown_text) {
                        shown_text = end;
                        text_at = Some(Instant::now());
                        send(Event::Text { step, text: shown(text.get(..end).unwrap_or(text)).to_owned(), done: false });
                    } else if since.elapsed() > TEXT_GATE && text.len() > shown_text {
                        shown_text = text.len();
                        text_at = Some(Instant::now());
                        send(Event::Text { step, text: shown(text).to_owned(), done: false });
                    }
                }
                if changes.contains(&Delta::Call) {
                    let said = &assembler.said;
                    let extras = !said.text.trim().is_empty()
                        && said.calls.iter().all(|call| call.name.is_empty() || EXTRAS.contains(&call.name.as_str()));
                    if extras != settled {
                        settled = extras;
                        if settled {
                            shown_text = said.text.len();
                            send(Event::Text { step, text: shown(&said.text).trim().to_owned(), done: true });
                        }
                        send(Event::Settled(settled));
                    }
                }
                if changes.contains(&Delta::Reasoning) && thought_at.elapsed() > THOUGHT_GATE {
                    thought_at = Instant::now();
                    send(Event::Thought { step, text: assembler.said.reasoning.clone(), done: false });
                }
                Ok(!assembler.done)
            });
            // A busy or rate-limited service is asked again a few times, as long
            // as nothing of the answer has arrived.
            if let Err(error) = &streamed
                && assembler.said == provider::Said::default()
                && attempt < RETRIES.len()
                && provider::retryable(error)
                && !cancel.load(Ordering::Acquire)
            {
                send(Event::Status("The model service is busy; trying again".into()));
                let until = Instant::now() + RETRIES[attempt];
                while Instant::now() < until && !cancel.load(Ordering::Acquire) {
                    std::thread::sleep(Duration::from_millis(200));
                }
                attempt += 1;
                continue;
            }
            if attempt > 0 {
                send(Event::Status(String::new()));
            }
            break (assembler.said, streamed);
        };
        let mut said = said;
        if !said.reasoning.trim().is_empty() {
            send(Event::Thought { step, text: said.reasoning.trim().to_owned(), done: true });
        }
        // Some models write these functions into the answer instead of
        // calling them; what they wrote is used as if called.
        let (text, written) = written_extras(&said.text);
        said.text = text;
        for (name, arguments) in written {
            match name {
                "suggest_replies" => send(Event::Suggest(replies_of(&arguments))),
                _ => send(Event::Links(links_of(&arguments))),
            }
        }
        if !said.text.trim().is_empty() {
            send(Event::Text { step, text: said.text.trim().to_owned(), done: true });
        }
        if let Err(error) = streamed {
            if !said.text.is_empty() {
                turn.messages.push(json!({"role": "assistant", "content": said.text}));
            }
            return finish(turn.messages, Some(error));
        }
        if cancel.load(Ordering::Acquire) {
            if !said.text.is_empty() {
                turn.messages.push(json!({"role": "assistant", "content": said.text}));
            }
            return finish(turn.messages, None);
        }
        let calls: Vec<_> = said
            .calls
            .into_iter()
            .filter(|call| !call.name.is_empty())
            .enumerate()
            .map(|(index, mut call)| {
                if call.id.is_empty() {
                    call.id = format!("call_{step}_{index}");
                }
                call
            })
            .collect();
        let mut message = json!({"role": "assistant", "content": if said.text.is_empty() { Value::Null } else { json!(said.text) }});
        if !calls.is_empty() {
            message["tool_calls"] = json!(
                calls
                    .iter()
                    .map(|c| json!({"id": c.id, "type": "function", "function": {"name": c.name, "arguments": if c.arguments.trim().is_empty() { "{}" } else { c.arguments.as_str() }}}))
                    .collect::<Vec<_>>()
            );
        }
        turn.messages.push(message);
        if calls.is_empty() {
            drain(inputs, &mut steers, &mut Vec::new());
            // A small model may end by announcing its next step ("Let me take
            // a snapshot") without taking it: it is asked once to act.
            if steers.is_empty() && !pressed && announces_step(&said.text) {
                pressed = true;
                send(Event::Interim(step));
                turn.messages.push(json!({"role": "user", "content": "(From the browser: you said what you would do next but called no function. Call it now, or give your answer.)"}));
                continue;
            }
            if steers.is_empty() {
                return finish(turn.messages, None);
            }
            continue;
        }
        let only_extras = !said.text.trim().is_empty() && calls.iter().all(|call| EXTRAS.contains(&call.name.as_str()));
        // Short text beside real work ("Let me open the page") is a note on
        // the way; a longer one may be the answer and stays.
        if !only_extras && !said.text.trim().is_empty() && said.text.chars().count() <= INTERIM_LIMIT {
            send(Event::Interim(step));
        }
        let mut images = Vec::new();
        let calls_made_now = calls.len();
        for call in calls {
            if cancel.load(Ordering::Acquire) {
                turn.messages.push(json!({"role": "tool", "tool_call_id": call.id, "content": "{\"error\":\"Stopped by the person.\"}"}));
                continue;
            }
            send(Event::Tool {
                id: call.id.clone(),
                name: call.name.clone(),
                arguments: call.arguments.clone(),
            });
            let arguments: Value = serde_json::from_str(if call.arguments.trim().is_empty() { "{}" } else { &call.arguments })
                .unwrap_or_else(|_| json!({}));
            // The same call a third time running gets the same answer: a
            // small model may otherwise repeat it until stopped.
            let repeated = made.len() >= 2 && made[made.len() - 2..].iter().all(|(name, args)| *name == call.name && *args == arguments);
            made.push((call.name.clone(), arguments.clone()));
            let result = match call.name.as_str() {
                _ if repeated => Err(format!(
                    "You already called {} twice with these arguments. Use what it returned, try something else, or answer.",
                    call.name
                )),
                "suggest_replies" => {
                    send(Event::Suggest(replies_of(&arguments)));
                    Ok(json!({"shown": true}))
                }
                "show_links" => {
                    send(Event::Links(links_of(&arguments)));
                    Ok(json!({"shown": true}))
                }
                name if super::memory::changes(name) => match turn.memories.as_mut() {
                    Some(notes) => {
                        let result = super::memory::apply(notes, name, &arguments);
                        if result.is_ok() {
                            send(Event::Memory(notes.clone()));
                        }
                        result
                    }
                    None => Err("Memory is off.".into()),
                },
                "search_chats" if turn.memories.is_some() => {
                    let query = arguments["query"].as_str().unwrap_or_default();
                    Ok(json!({"chats": super::history::search(&super::history::folder(), query, 4)}))
                }
                "web_search" => super::preview::search(arguments["query"].as_str().unwrap_or_default()),
                "read_web_page" => {
                    let url = arguments["url"].as_str().unwrap_or_default().trim().to_owned();
                    super::preview::page_text(&url)
                }
                name if super::mcp::TOOLS.contains(&name) => match apps.as_mut() {
                    Some(client) => client.call(name, &arguments, cancel, || {
                        send(Event::Status("Waiting for your answer in the agent center".into()));
                    }),
                    None => Err("Other apps are not available.".into()),
                },
                _ => {
                    // Pages come in smaller parts for the panel's model: a part
                    // is sent with every later request of the turn.
                    let mut arguments = arguments;
                    if call.name == "take_snapshot"
                        && let Some(object) = arguments.as_object_mut()
                    {
                        object.entry("max_chars").or_insert(json!(SNAPSHOT_PART));
                    }
                    send(Event::Run {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        arguments,
                    });
                    let result = wait(&call.id, inputs, &mut steers, cancel);
                    match result {
                        Ok(value) if call.name == "take_snapshot" && value["loaded"] == false => unloaded_tab(&value),
                        other => other,
                    }
                }
            };
            let (ok, mut value) = match result {
                Ok(value) => (true, value),
                Err(error) => (false, json!({"error": error})),
            };
            let image = take_image(&mut value);
            let output = serde_json::to_string_pretty(&value).unwrap_or_default();
            send(Event::ToolDone {
                id: call.id.clone(),
                ok,
                output: clip(&output, 4096),
                image: image.clone(),
            });
            turn.messages.push(json!({"role": "tool", "tool_call_id": call.id, "content": clip(&value.to_string(), RESULT_LIMIT)}));
            if let Some(image) = image {
                images.push(image);
            }
        }
        // A long turn is reminded to finish: it has no step limit, but a
        // small model can wander.
        if made.len() / NUDGE_EVERY > (made.len() - calls_made_now) / NUDGE_EVERY {
            turn.messages.push(json!({"role": "user", "content": format!(
                "(From the browser: you have made {} function calls for this request. If you have what you need, answer now; if not, say what you could not find.)",
                made.len()
            )}));
        }
        // Tool replies carry text only; a picture the model asked for
        // follows as the person's message.
        if !images.is_empty() {
            let mut parts = vec![json!({"type": "text", "text": "(The screenshot you took.)"})];
            for image in images {
                let data = base64::engine::general_purpose::STANDARD.encode(image.as_slice());
                parts.push(json!({"type": "image_url", "image_url": {"url": format!("data:image/jpeg;base64,{data}")}}));
            }
            turn.messages.push(json!({"role": "user", "content": parts}));
        }
        if only_extras {
            drain(inputs, &mut steers, &mut Vec::new());
            if steers.is_empty() {
                return finish(turn.messages, None);
            }
            send(Event::Settled(false));
        }
    }
}

/// Whether a reply ends by announcing a step rather than answering: its last
/// sentence says what the model will do ("Let me open the page.").
fn announces_step(text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() || text.chars().count() > 600 || text.ends_with('?') {
        return false;
    }
    let last = text
        .trim_end_matches(['.', '!', ':', '…'])
        .rsplit(['.', '!', '\n', ':'])
        .map(str::trim)
        .find(|sentence| !sentence.is_empty())
        .unwrap_or_default()
        .to_lowercase()
        .replace('’', "'");
    let starts = ["let me ", "i'll ", "i will ", "i need to ", "i am going to ", "i'm going to ", "now i'll ", "next, i", "first, i'll", "first i'll"];
    let polite = ["let me know", "i'll be ", "i will be ", "i'll leave", "i'll wait"];
    starts.iter().any(|s| last.starts_with(s)) && !polite.iter().any(|p| last.starts_with(p))
}

/// A tab of the person's that is not loaded is read from its address, as
/// read_web_page would, rather than shown to load it.
fn unloaded_tab(state: &Value) -> Result<Value, String> {
    let url = state["url"].as_str().unwrap_or_default();
    let read = super::preview::page_text(url).map_err(|_| {
        "This tab is not loaded and its address could not be read; ask the person to open the tab.".to_owned()
    })?;
    Ok(json!({
        "page_id": state["page_id"],
        "url": url,
        "title": read["title"],
        "text": read["text"],
        "note": "The tab is not loaded, so this is the text at its address, without element uids. To act on the tab, ask the person to open it.",
    }))
}

fn replies_of(arguments: &Value) -> Vec<String> {
    arguments["replies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|r| r.trim().chars().take(80).collect::<String>())
        .filter(|r| !r.is_empty())
        .take(4)
        .collect()
}

fn links_of(arguments: &Value) -> Vec<Link> {
    arguments["links"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|link| {
            let url = link["url"].as_str()?.trim();
            url::Url::parse(url).ok().filter(|u| matches!(u.scheme(), "http" | "https"))?;
            Some(Link {
                url: url.to_owned(),
                title: link["title"].as_str().unwrap_or_default().chars().take(120).collect(),
                note: link["note"].as_str().unwrap_or_default().chars().take(200).collect(),
            })
        })
        .take(6)
        .collect()
}

/// The part of a reply to show while it streams: up to a written extra
/// (see [`written_extras`]), or to the start of one still arriving.
fn shown(text: &str) -> &str {
    let mut end = text.len();
    for name in EXTRAS {
        if let Some(at) = text.find(&format!("<{name}")) {
            end = end.min(at);
        }
    }
    let text = text.get(..end).unwrap_or(text);
    if let Some(at) = text.rfind('<')
        && let (Some(head), Some(tail)) = (text.get(..at), text.get(at..))
        && EXTRAS.iter().any(|name| format!("<{name}").starts_with(tail))
    {
        return head;
    }
    text
}

/// Removes `<suggest_replies>…</suggest_replies>` and `<show_links>…</show_links>`
/// written into a reply, and returns their arguments as a call would carry
/// them. The inside may be JSON (an array, or the call's object), `<item>`
/// elements, or one entry per line; links are kept only as http(s) addresses.
fn written_extras(text: &str) -> (String, Vec<(&'static str, Value)>) {
    let mut rest = text.to_owned();
    let mut found = Vec::new();
    for name in EXTRAS {
        let open = format!("<{name}");
        let close = format!("</{name}>");
        while let Some(start) = rest.find(&open) {
            let inner_start = rest.get(start..).and_then(|tail| tail.find('>')).map_or(rest.len(), |at| start + at + 1);
            let (inner_end, end) = match rest.get(inner_start..).and_then(|tail| tail.find(&close)) {
                Some(at) => (inner_start + at, inner_start + at + close.len()),
                None => (rest.len(), rest.len()),
            };
            let inner = rest.get(inner_start..inner_end).unwrap_or_default().trim().to_owned();
            rest.replace_range(start..end, "");
            let entries: Vec<String> = match serde_json::from_str::<Value>(&inner) {
                Ok(Value::Object(object)) => {
                    found.push((name, Value::Object(object)));
                    continue;
                }
                Ok(Value::Array(items)) if name == "show_links" => {
                    found.push((name, json!({"links": items.into_iter().map(|item| if item.is_string() { json!({"url": item}) } else { item }).collect::<Vec<_>>()})));
                    continue;
                }
                Ok(Value::Array(items)) => items.into_iter().filter_map(|item| item.as_str().map(str::to_owned)).collect(),
                _ if inner.contains("<item>") => inner
                    .split("<item>")
                    .skip(1)
                    .map(|item| item.split("</item>").next().unwrap_or_default().trim().to_owned())
                    .collect(),
                _ => inner
                    .lines()
                    .map(|line| line.trim().trim_start_matches(|c: char| c == '-' || c == '*' || c == '•' || c.is_ascii_digit() || c == '.' || c == ')').trim().to_owned())
                    .collect(),
            };
            let entries: Vec<String> = entries.into_iter().filter(|entry| !entry.is_empty()).collect();
            found.push(if name == "show_links" {
                (name, json!({"links": entries.iter().map(|url| json!({"url": url})).collect::<Vec<_>>()}))
            } else {
                (name, json!({"replies": entries}))
            });
        }
    }
    (rest.trim_end().to_owned(), found)
}

fn endpoint(source: &Source) -> Result<Endpoint, String> {
    match source {
        Source::Compatible { base, key, .. } => {
            let key = super::api_key(key).map(|k| k.expose().to_owned()).unwrap_or_default();
            Ok(Endpoint::Compatible { base: base.clone(), key })
        }
        Source::ChatGpt { .. } => super::chatgpt::access_token().map(|token| Endpoint::ChatGpt { token }),
    }
}

fn drain(inputs: &mpsc::Receiver<Input>, steers: &mut Vec<Value>, results: &mut Vec<(String, Result<Value, String>)>) {
    while let Ok(input) = inputs.try_recv() {
        match input {
            Input::Steer(message) => steers.push(message),
            Input::Result(id, result) => results.push((id, result)),
        }
    }
}

/// Waits for the UI thread's result of call `id`, keeping steering for the
/// next request.
fn wait(
    id: &str,
    inputs: &mpsc::Receiver<Input>,
    steers: &mut Vec<Value>,
    cancel: &AtomicBool,
) -> Result<Value, String> {
    let started = Instant::now();
    loop {
        if cancel.load(Ordering::Acquire) {
            return Err("Stopped by the person.".into());
        }
        // Approval may take the person a while.
        if started.elapsed() > Duration::from_secs(15 * 60) {
            return Err("No answer in time.".into());
        }
        match inputs.recv_timeout(Duration::from_millis(200)) {
            Ok(Input::Steer(message)) => steers.push(message),
            Ok(Input::Result(done, result)) if done == id => return result,
            Ok(Input::Result(..)) | Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return Err("The window closed.".into()),
        }
    }
}

/// The end of the last whole paragraph after `shown`, outside code blocks.
fn paragraph_end(text: &str, shown: usize) -> Option<usize> {
    let end = text.rfind("\n\n")?;
    if end <= shown {
        return None;
    }
    // An odd number of fences means a code block is still open.
    let fences = text.get(..end)?.matches("```").count();
    fences.is_multiple_of(2).then_some(end)
}

/// Takes a screenshot's picture out of a result, so the model gets it as an
/// image and the transcript shows it.
fn take_image(value: &mut Value) -> Option<Arc<Vec<u8>>> {
    let data = value.get("data")?.as_str()?;
    if value["mime_type"].as_str().is_some_and(|m| !m.starts_with("image/"))
        && value["mime"].as_str().is_some_and(|m| !m.starts_with("image/"))
    {
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD.decode(data).ok()?;
    value["data"] = json!("(the image follows)");
    Some(Arc::new(bytes))
}

fn clip(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_owned();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… ({} more bytes)", text.get(..end).unwrap_or_default(), text.len() - end)
}

fn shorten_old_results(messages: &mut [Value]) {
    let tools: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| m["role"] == "tool")
        .map(|(i, _)| i)
        .collect();
    for &index in tools.iter().rev().skip(KEEP_RECENT_RESULTS) {
        let content = messages[index]["content"].as_str().unwrap_or_default();
        if content.len() > 2048 {
            messages[index]["content"] = json!(format!("{} … (shortened; call the tool again for the rest)", clip(content, 1024)));
        }
    }
    // Pictures older than the last two are dropped; their text stays.
    let pictures: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| m["role"] == "user" && m["content"].as_array().is_some_and(|p| p.iter().any(|p| p["type"] == "image_url")))
        .map(|(i, _)| i)
        .collect();
    for &index in pictures.iter().rev().skip(2) {
        if let Some(parts) = messages[index]["content"].as_array_mut() {
            parts.retain(|p| p["type"] != "image_url");
            parts.push(json!({"type": "text", "text": "(an earlier picture, no longer attached)"}));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A live turn against an OpenAI-compatible service:
    /// `ENZIM_ASSISTANT_TEST_KEY`, `_BASE` and `_MODEL` name it.
    #[test]
    #[ignore = "needs a model service and its key"]
    fn a_live_turn_streams_and_calls_tools() {
        let var = |name: &str| std::env::var(format!("ENZIM_ASSISTANT_TEST_{name}")).expect(name);
        if let Ok(mut keys) = super::super::API_KEYS.lock() {
            keys.insert("test-live".into(), Arc::new(crate::credentials::Secret::new(var("KEY"))));
        }
        let mut tools = panel_tools();
        tools.push(json!({"type": "function", "function": {"name": "list_pages", "description": "Every open tab.", "parameters": {"type": "object", "properties": {}}}}));
        let turn = Turn {
            source: Source::Compatible { base: var("BASE"), model: var("MODEL"), key: "test-live".into() },
            messages: vec![
                json!({"role": "system", "content": super::super::SYSTEM}),
                json!({"role": "user", "content": "Which tabs do I have open? Then write two short paragraphs about the first one."}),
            ],
            tools,
            apps: false,
            memories: None,
        };
        let cancel = AtomicBool::new(false);
        let (input, inputs) = mpsc::channel();
        let handle = Handle { cancel: Arc::new(AtomicBool::new(false)), input };
        let finished = std::cell::RefCell::new(None);
        run(turn, &cancel, &inputs, &|event| {
            match &event {
                Event::Run { id, name, .. } => {
                    assert_eq!(name, "list_pages");
                    handle.answer(id.clone(), Ok(json!({"pages": [{"page_id": 4, "title": "Rust (programming language) - Wikipedia", "url": "https://en.wikipedia.org/wiki/Rust_(programming_language)"}]})));
                }
                Event::Finished { error, .. } => *finished.borrow_mut() = Some(error.clone()),
                _ => {}
            }
            eprintln!("{event:?}");
        });
        assert_eq!(finished.into_inner(), Some(None), "the turn finished cleanly");
    }

    #[test]
    fn paragraphs_are_shown_whole_and_code_blocks_stay_together() {
        assert_eq!(paragraph_end("One.\n\nTw", 0), Some(4));
        assert_eq!(paragraph_end("One.\n\nTw", 4), None, "nothing new");
        assert_eq!(paragraph_end("One.\n\n```\nlet a;\n\nlet b", 4), None, "inside a fence");
        assert_eq!(paragraph_end("```\na\n```\n\nMore", 0), Some(9));
    }

    /// A tiny model service: each request is answered with the next stream
    /// of `answers` (the last repeats); returns its address and a request count.
    fn service(answers: Vec<String>) -> (String, Arc<std::sync::atomic::AtomicUsize>) {
        use std::io::{BufRead as _, Read as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = format!("http://{}", listener.local_addr().expect("address"));
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let served = Arc::clone(&count);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut reader = std::io::BufReader::new(stream);
                let mut length = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0; length];
                let _ = reader.read_exact(&mut body);
                let n = served.fetch_add(1, Ordering::SeqCst);
                let answer = &answers[n.min(answers.len() - 1)];
                let mut stream = reader.into_inner();
                // "!429 body" answers with that status and body.
                let (status, kind, answer) = match answer.strip_prefix("!429 ") {
                    Some(body) => ("429 Too Many Requests", "application/json", body),
                    None => ("200 OK", "text/event-stream", answer.as_str()),
                };
                let _ = write!(stream, "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}", answer.len());
            }
        });
        (address, count)
    }

    fn events(chunks: &[Value]) -> String {
        chunks.iter().map(|chunk| format!("data: {chunk}\n\n")).collect::<String>() + "data: [DONE]\n\n"
    }

    #[test]
    fn an_answer_with_only_suggestions_ends_the_turn() {
        let delta = |d: Value| json!({"choices": [{"delta": d}]});
        let (base, requests) = service(vec![
            events(&[
                delta(json!({"content": "Rome has two airports."})),
                delta(json!({"tool_calls": [{"index": 0, "id": "s1", "function": {"name": "suggest_replies", "arguments": ""}}]})),
                delta(json!({"tool_calls": [{"index": 0, "function": {"arguments": "{\"replies\":[\"Which is closer?\"]}"}}]})),
            ]),
            events(&[delta(json!({"content": "Asked again."}))]),
        ]);
        if let Ok(mut keys) = super::super::API_KEYS.lock() {
            keys.insert("test-suggestions".into(), Arc::new(crate::credentials::Secret::new("test".into())));
        }
        let turn = Turn {
            source: Source::Compatible { base, model: "m".into(), key: "test-suggestions".into() },
            messages: vec![json!({"role": "user", "content": "Airports in Rome?"})],
            tools: panel_tools(),
            apps: false,
            memories: None,
        };
        let (_input, inputs) = mpsc::channel();
        let settled = std::cell::Cell::new(None);
        let suggested = std::cell::RefCell::new(Vec::new());
        let finished = std::cell::RefCell::new(None);
        run(turn, &AtomicBool::new(false), &inputs, &|event| match event {
            Event::Settled(on) => settled.set(Some(on)),
            Event::Suggest(replies) => *suggested.borrow_mut() = replies,
            Event::Finished { messages, error, .. } => *finished.borrow_mut() = Some((messages, error)),
            _ => {}
        });
        let (messages, error) = finished.into_inner().expect("finished");
        assert_eq!(error, None);
        assert_eq!(settled.get(), Some(true), "the answer settles once only extras follow");
        assert_eq!(suggested.into_inner(), ["Which is closer?"]);
        assert_eq!(requests.load(Ordering::SeqCst), 1, "no second request for the extras");
        assert_eq!(messages.last().expect("last")["role"], "tool", "the call is answered for the next turn");
    }

    #[test]
    fn repeated_calls_are_refused_and_long_turns_reminded() {
        let call = |id: usize, query: &str| {
            events(&[json!({"choices": [{"delta": {"tool_calls": [{"index": 0, "id": format!("c{id}"), "function": {"name": "web_search", "arguments": json!({"query": query}).to_string()}}]}}]})])
        };
        // Three identical calls, then ten different ones, then an answer.
        let mut answers: Vec<String> = (0..3).map(|i| call(i, " ")).collect();
        answers.extend((3..13).map(|i| call(i, &" ".repeat(i))));
        answers.push(events(&[json!({"choices": [{"delta": {"content": "Done."}}]})]));
        let (base, _) = service(answers);
        let turn = Turn {
            source: Source::Compatible { base, model: "m".into(), key: String::new() },
            messages: vec![json!({"role": "user", "content": "Look it up"})],
            tools: panel_tools(),
            apps: false,
            memories: None,
        };
        let (_input, inputs) = mpsc::channel();
        let finished = std::cell::RefCell::new(None);
        run(turn, &AtomicBool::new(false), &inputs, &|event| {
            if let Event::Finished { messages, .. } = event {
                *finished.borrow_mut() = Some(messages);
            }
        });
        let messages = finished.into_inner().expect("finished");
        let results: Vec<&str> = messages.iter().filter(|m| m["role"] == "tool").filter_map(|m| m["content"].as_str()).collect();
        assert!(!results[1].contains("already called"), "a second identical call still runs");
        assert!(results[2].contains("already called web_search twice"), "the third is refused: {}", results[2]);
        let reminders: Vec<&Value> = messages.iter().filter(|m| m["role"] == "user" && m["content"].as_str().is_some_and(|c| c.starts_with("(From the browser"))).collect();
        assert_eq!(reminders.len(), 1, "one reminder after twelve calls");
        assert!(reminders[0]["content"].as_str().is_some_and(|c| c.contains("12 function calls")));
        assert_eq!(messages.last().expect("answer")["content"], "Done.");
    }

    #[test]
    fn an_announced_step_is_asked_for_once() {
        assert!(announces_step("I need to take a new snapshot to get the uids. Let me do that first."));
        assert!(announces_step("I’ll open the page now"));
        assert!(!announces_step("It was completed in 1889. Let me know if you need more."));
        assert!(!announces_step("Should I open it for you?"));
        assert!(!announces_step("The answer is 37 years."));
        let (base, requests) = service(vec![
            events(&[json!({"choices": [{"delta": {"content": "Let me look at the page."}}]})]),
            events(&[json!({"choices": [{"delta": {"content": "Let me look again."}}]})]),
        ]);
        let turn = Turn {
            source: Source::Compatible { base, model: "m".into(), key: String::new() },
            messages: vec![json!({"role": "user", "content": "What is on the page?"})],
            tools: Vec::new(),
            apps: false,
            memories: None,
        };
        let (_input, inputs) = mpsc::channel();
        let finished = std::cell::RefCell::new(None);
        run(turn, &AtomicBool::new(false), &inputs, &|event| {
            if let Event::Finished { messages, .. } = event {
                *finished.borrow_mut() = Some(messages);
            }
        });
        let messages = finished.into_inner().expect("finished");
        assert_eq!(requests.load(Ordering::SeqCst), 2, "asked once, not again");
        assert!(messages.iter().any(|m| m["content"].as_str().is_some_and(|c| c.starts_with("(From the browser: you said"))));
    }

    #[test]
    fn a_rate_limited_request_is_asked_again() {
        let (base, requests) = service(vec![
            r#"!429 {"error": {"message": "google/gemma is temporarily rate-limited upstream."}}"#.to_owned(),
            events(&[json!({"choices": [{"delta": {"content": "Here it is."}}]})]),
        ]);
        let turn = Turn {
            source: Source::Compatible { base, model: "m".into(), key: String::new() },
            messages: vec![json!({"role": "user", "content": "Hi"})],
            tools: Vec::new(),
            apps: false,
            memories: None,
        };
        let (_input, inputs) = mpsc::channel();
        let finished = std::cell::RefCell::new(None);
        let statuses = std::cell::RefCell::new(Vec::new());
        run(turn, &AtomicBool::new(false), &inputs, &|event| match event {
            Event::Status(status) => statuses.borrow_mut().push(status),
            Event::Finished { messages, error, .. } => *finished.borrow_mut() = Some((messages, error)),
            _ => {}
        });
        let (messages, error) = finished.into_inner().expect("finished");
        assert_eq!(error, None, "the second answer is used");
        assert_eq!(requests.load(Ordering::SeqCst), 2);
        assert_eq!(messages.last().expect("reply")["content"], "Here it is.");
        assert_eq!(statuses.into_inner(), ["The model service is busy; trying again", ""], "the wait shows, then clears");
        assert!(!provider::retryable("The service refused the API key."), "a refused key is not asked again");
    }

    #[test]
    fn extras_written_into_the_answer_are_used_and_hidden() {
        let text = "That's **Luke**.\n\n<suggest_replies><item>Summarize the video</item><item>When was it posted?</item></suggest_replies>";
        let (clean, written) = written_extras(text);
        assert_eq!(clean, "That's **Luke**.");
        assert_eq!(written.len(), 1);
        assert_eq!(replies_of(&written[0].1), ["Summarize the video", "When was it posted?"]);
        for inside in [r#"["A", "B"]"#, r#"{"replies": ["A", "B"]}"#, "- A\n- B", "1. A\n2. B"] {
            let (_, written) = written_extras(&format!("Answer.\n<suggest_replies>\n{inside}\n</suggest_replies>"));
            assert_eq!(replies_of(&written[0].1), ["A", "B"], "{inside}");
        }
        let (clean, written) = written_extras("See these.\n<show_links>[\"https://example.org/a\", \"javascript:alert(1)\"]</show_links>\nEnjoy.");
        assert_eq!(clean, "See these.\n\nEnjoy.");
        let links = links_of(&written[0].1);
        assert_eq!(links.len(), 1, "only http(s) links");
        assert_eq!(links[0].url, "https://example.org/a");
        let (clean, written) = written_extras("Unclosed.\n<suggest_replies><item>One</item>");
        assert_eq!((clean.as_str(), replies_of(&written[0].1)), ("Unclosed.", vec!["One".to_owned()]));
        assert_eq!(written_extras("Use a <b>tag</b> here.").1.len(), 0, "other markup stays");

        assert_eq!(shown("Done.\n\n<suggest_replies><item>A"), "Done.\n\n");
        assert_eq!(shown("Done.\n\n<sugg"), "Done.\n\n", "a tag still arriving is not shown");
        assert_eq!(shown("a < b"), "a < b");
    }

    #[test]
    fn screenshots_leave_the_result_as_images() {
        let mut value = json!({"page_id": 1, "data": base64::engine::general_purpose::STANDARD.encode(b"\xff\xd8\xff"), "mime_type": "image/jpeg"});
        let image = take_image(&mut value).expect("image");
        assert_eq!(image.as_slice(), b"\xff\xd8\xff");
        assert_eq!(value["data"], "(the image follows)");
        assert!(take_image(&mut json!({"page_id": 1})).is_none());
    }

    #[test]
    fn old_results_and_pictures_are_shortened() {
        let mut messages: Vec<Value> = (0..9)
            .map(|i| json!({"role": "tool", "tool_call_id": format!("c{i}"), "content": "x".repeat(5000)}))
            .collect();
        for _ in 0..3 {
            messages.push(json!({"role": "user", "content": [{"type": "image_url", "image_url": {"url": "data:"}}]}));
        }
        shorten_old_results(&mut messages);
        assert!(messages[0]["content"].as_str().expect("text").len() < 1500, "old result shortened");
        assert_eq!(messages[8]["content"].as_str().map(str::len), Some(5000), "recent kept");
        assert!(messages[9]["content"].as_array().expect("parts").iter().all(|p| p["type"] != "image_url"));
        assert!(messages[11]["content"][0]["type"] == "image_url");
    }
}
