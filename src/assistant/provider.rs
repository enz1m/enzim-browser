//! The model's side of a turn: one streamed request and what it said.
//!
//! The conversation is kept in Chat Completions form (`messages` with
//! `tool_calls` and `tool` replies), which every OpenAI-compatible service
//! speaks. A ChatGPT sign-in only offers the Responses API, so its requests
//! are translated on the way out and its events on the way back.

use serde_json::{Map, Value, json};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Where a request goes and how it signs in.
#[derive(Clone, Debug)]
pub(crate) enum Endpoint {
    /// `{base}/chat/completions` with a bearer key (empty: none).
    Compatible { base: String, key: String },
    /// The plan's Responses API with a ChatGPT access token.
    ChatGpt { token: String },
}

const CHATGPT_RESPONSES: &str = "https://api.openai.com/v1/responses";
/// A whole streamed reply, events included.
const MAX_STREAM: usize = 16 * 1024 * 1024;
/// One event line.
const MAX_LINE: usize = 4 * 1024 * 1024;

/// What the model said in one request.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct Said {
    pub(crate) text: String,
    pub(crate) reasoning: String,
    pub(crate) calls: Vec<Call>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct Call {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) arguments: String,
}

/// A change while the reply streams in.
#[derive(Debug, PartialEq)]
pub(crate) enum Delta {
    Text,
    Reasoning,
    Call,
}

/// Builds Chat Completions events into [`Said`].
#[derive(Default)]
pub(crate) struct Assembler {
    pub(crate) said: Said,
    pub(crate) done: bool,
}

impl Assembler {
    /// One `data:` payload of a Chat Completions stream.
    pub(crate) fn chat_event(&mut self, data: &str) -> Result<Vec<Delta>, String> {
        if data == "[DONE]" {
            self.done = true;
            return Ok(Vec::new());
        }
        let Ok(event) = serde_json::from_str::<Value>(data) else {
            return Ok(Vec::new());
        };
        if let Some(error) = event.get("error").filter(|e| !e.is_null()) {
            return Err(error_text(error));
        }
        let mut changes = Vec::new();
        let delta = &event["choices"][0]["delta"];
        if let Some(text) = delta["content"].as_str().filter(|t| !t.is_empty()) {
            self.said.text.push_str(text);
            changes.push(Delta::Text);
        }
        let thought = delta["reasoning"]
            .as_str()
            .or_else(|| delta["reasoning_content"].as_str());
        if let Some(text) = thought.filter(|t| !t.is_empty()) {
            self.said.reasoning.push_str(text);
            changes.push(Delta::Reasoning);
        }
        for part in delta["tool_calls"].as_array().into_iter().flatten() {
            let index = part["index"]
                .as_u64()
                .and_then(|i| usize::try_from(i).ok())
                .unwrap_or_else(|| self.said.calls.len().saturating_sub(1));
            if index > 64 {
                continue;
            }
            while self.said.calls.len() <= index {
                self.said.calls.push(Call::default());
            }
            let call = &mut self.said.calls[index];
            if let Some(id) = part["id"].as_str().filter(|id| !id.is_empty()) {
                id.clone_into(&mut call.id);
            }
            if let Some(name) = part["function"]["name"].as_str() {
                call.name.push_str(name);
            }
            if let Some(arguments) = part["function"]["arguments"].as_str() {
                call.arguments.push_str(arguments);
            }
            changes.push(Delta::Call);
        }
        Ok(changes)
    }

    /// One `data:` payload of a Responses stream.
    pub(crate) fn responses_event(&mut self, data: &str) -> Result<Vec<Delta>, String> {
        let Ok(event) = serde_json::from_str::<Value>(data) else {
            return Ok(Vec::new());
        };
        Ok(match event["type"].as_str().unwrap_or_default() {
            "response.output_text.delta" => {
                self.said.text.push_str(event["delta"].as_str().unwrap_or_default());
                vec![Delta::Text]
            }
            "response.reasoning_summary_text.delta" | "response.reasoning_text.delta" => {
                self.said
                    .reasoning
                    .push_str(event["delta"].as_str().unwrap_or_default());
                vec![Delta::Reasoning]
            }
            "response.reasoning_summary_part.done" => {
                self.said.reasoning.push_str("\n\n");
                Vec::new()
            }
            // A call is named when it starts, so the panel knows what follows
            // the answer before its arguments arrive.
            "response.output_item.added" | "response.output_item.done" if event["item"]["type"] == "function_call" => {
                let item = &event["item"];
                let call = Call {
                    id: item["call_id"].as_str().unwrap_or_default().to_owned(),
                    name: item["name"].as_str().unwrap_or_default().to_owned(),
                    arguments: item["arguments"].as_str().unwrap_or("{}").to_owned(),
                };
                match self.said.calls.iter_mut().find(|c| !call.id.is_empty() && c.id == call.id) {
                    Some(started) => *started = call,
                    None => self.said.calls.push(call),
                }
                vec![Delta::Call]
            }
            "response.completed" => {
                self.done = true;
                Vec::new()
            }
            "response.failed" | "response.incomplete" | "error" => {
                let error = &event["response"]["error"];
                let code = error["code"]
                    .as_str()
                    .or_else(|| event["code"].as_str())
                    .unwrap_or("response failed");
                return Err(match code {
                    "subscription_sharing_usage_limit_exceeded" => {
                        "Your ChatGPT plan's usage limit is reached.".into()
                    }
                    "subscription_sharing_usage_unavailable" => {
                        "ChatGPT plan use is unavailable for this account.".into()
                    }
                    _ => error["message"]
                        .as_str()
                        .or_else(|| event["message"].as_str())
                        .map_or_else(|| format!("ChatGPT: {code}"), str::to_owned),
                });
            }
            _ => Vec::new(),
        })
    }
}

fn error_text(error: &Value) -> String {
    let message = error["message"]
        .as_str()
        .or_else(|| error.as_str())
        .unwrap_or("The model service refused the request.");
    let detail = error["metadata"]["raw"].as_str().filter(|raw| raw.len() < 300);
    match detail {
        Some(raw) => format!("{message} ({raw})"),
        None => message.to_owned(),
    }
}

/// The request body. `messages` are Chat Completions messages, system first.
pub(crate) fn body(endpoint: &Endpoint, model: &str, messages: &[Value], tools: &[Value]) -> Value {
    match endpoint {
        Endpoint::Compatible { .. } => {
            let mut body = json!({"model": model, "messages": messages, "stream": true});
            if !tools.is_empty() {
                body["tools"] = json!(tools);
            }
            body
        }
        Endpoint::ChatGpt { .. } => responses_body(model, messages, tools),
    }
}

/// Function tools of the browser's assistant are offered in one namespace,
/// as plan use asks of public clients.
const NAMESPACE: &str = "enzim";

/// A Chat Completions request as a plan Responses request: system messages
/// become `instructions`, images `input_image` parts, and the request is
/// streamed and not stored, as plan use requires.
pub(crate) fn responses_body(model: &str, messages: &[Value], tools: &[Value]) -> Value {
    let mut instructions = Vec::new();
    let mut input = Vec::new();
    let text = |content: &Value| match content {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p["text"].as_str())
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    };
    for message in messages {
        match message["role"].as_str().unwrap_or_default() {
            "system" | "developer" => instructions.push(text(&message["content"])),
            "user" => {
                let content = match &message["content"] {
                    Value::Array(parts) => Value::Array(
                        parts
                            .iter()
                            .filter_map(|part| match part["type"].as_str() {
                                Some("text") => {
                                    Some(json!({"type": "input_text", "text": part["text"]}))
                                }
                                Some("image_url") => Some(
                                    json!({"type": "input_image", "image_url": part["image_url"]["url"]}),
                                ),
                                _ => None,
                            })
                            .collect(),
                    ),
                    other => json!(text(other)),
                };
                input.push(json!({"role": "user", "content": content}));
            }
            "assistant" => {
                let said = text(&message["content"]);
                if !said.is_empty() {
                    input.push(json!({"role": "assistant", "content": said}));
                }
                for call in message["tool_calls"].as_array().into_iter().flatten() {
                    let mut item = json!({"type": "function_call", "call_id": call["id"], "name": call["function"]["name"], "arguments": call["function"]["arguments"]});
                    if !tools.is_empty() {
                        item["namespace"] = json!(NAMESPACE);
                    }
                    input.push(item);
                }
            }
            "tool" => input.push(json!({"type": "function_call_output", "call_id": message["tool_call_id"], "output": text(&message["content"])})),
            _ => {}
        }
    }
    let mut request = Map::new();
    request.insert("model".into(), json!(model));
    if !instructions.is_empty() {
        request.insert("instructions".into(), json!(instructions.join("\n\n")));
    }
    request.insert("input".into(), Value::Array(input));
    if !tools.is_empty() {
        let functions = tools
            .iter()
            .filter(|t| t["type"] == "function")
            .map(|t| {
                let f = &t["function"];
                json!({"type": "function", "name": f["name"], "description": f["description"], "parameters": f["parameters"]})
            })
            .collect::<Vec<_>>();
        request.insert(
            "tools".into(),
            json!([{"type": "namespace", "name": NAMESPACE, "description": "Tools of the enzim Browser assistant", "tools": functions}]),
        );
    }
    request.insert("reasoning".into(), json!({"summary": "auto"}));
    request.insert("stream".into(), json!(true));
    request.insert("store".into(), json!(false));
    Value::Object(request)
}

/// Sends `body` and feeds each streamed event to `event`, which returns
/// whether to go on. `cancel` stops the transfer at once.
/// Whether a failed request may succeed if asked again shortly: rate limits,
/// overload and gateway errors, as services word them.
pub(crate) fn retryable(error: &str) -> bool {
    let error = error.to_lowercase();
    ["rate limit", "rate-limit", "temporarily", "overloaded", "try again", "too many requests", "http 502", "http 503", "http 504", "capacity"]
        .iter()
        .any(|phrase| error.contains(phrase))
}

pub(crate) fn stream(
    endpoint: &Endpoint,
    body: &Value,
    cancel: &AtomicBool,
    mut event: impl FnMut(&str) -> Result<bool, String>,
) -> Result<(), String> {
    let (url, token) = match endpoint {
        Endpoint::Compatible { base, key } => (
            format!("{}/chat/completions", base.trim_end_matches('/')),
            key.as_str(),
        ),
        Endpoint::ChatGpt { token } => (CHATGPT_RESPONSES.to_owned(), token.as_str()),
    };
    let raw = serde_json::to_vec(body).map_err(|_| "The request could not be written.")?;
    let mut easy = curl::easy::Easy::new();
    let mut setup = || -> Result<(), curl::Error> {
        easy.url(&url)?;
        easy.post(true)?;
        easy.post_fields_copy(&raw)?;
        let mut headers = curl::easy::List::new();
        headers.append("Content-Type: application/json")?;
        headers.append("Accept: text/event-stream")?;
        if !token.is_empty() {
            headers.append(&format!("Authorization: Bearer {token}"))?;
        }
        // OpenRouter names the app a request comes from; others ignore it.
        headers.append("X-Title: enzim Browser")?;
        easy.http_headers(headers)?;
        easy.useragent(concat!("enzim-browser/", env!("CARGO_PKG_VERSION")))?;
        easy.connect_timeout(Duration::from_secs(15))?;
        // A reply that stops arriving for two minutes has failed.
        easy.low_speed_limit(1)?;
        easy.low_speed_time(Duration::from_secs(120))?;
        easy.follow_location(false)?;
        easy.progress(true)?;
        Ok(())
    };
    setup().map_err(|_| "The request could not be prepared.".to_owned())?;
    let mut line = Vec::new();
    let mut total = 0usize;
    let mut failure: Option<String> = None;
    let mut error_body = Vec::new();
    let status = std::cell::Cell::new(0u32);
    let result = {
        let mut transfer = easy.transfer();
        transfer
            .progress_function(|_, _, _, _| !cancel.load(Ordering::Acquire))
            .map_err(|_| "The request could not be prepared.".to_owned())?;
        transfer
            .header_function(|header| {
                if let Some(code) = std::str::from_utf8(header)
                    .ok()
                    .filter(|h| h.starts_with("HTTP/"))
                    .and_then(|h| h.split_whitespace().nth(1))
                    .and_then(|c| c.parse().ok())
                {
                    status.set(code);
                }
                true
            })
            .map_err(|_| "The request could not be prepared.".to_owned())?;
        transfer
            .write_function(|data| {
                total += data.len();
                if cancel.load(Ordering::Acquire) || total > MAX_STREAM {
                    return Ok(0);
                }
                if status.get() != 200 {
                    if error_body.len() < 64 * 1024 {
                        error_body.extend_from_slice(data);
                    }
                    return Ok(data.len());
                }
                for &byte in data {
                    if byte != b'\n' {
                        if line.len() < MAX_LINE {
                            line.push(byte);
                        }
                        continue;
                    }
                    let text = String::from_utf8_lossy(&line).into_owned();
                    line.clear();
                    let Some(payload) = text
                        .trim_end_matches('\r')
                        .strip_prefix("data:")
                        .map(str::trim)
                    else {
                        continue;
                    };
                    match event(payload) {
                        Ok(true) => {}
                        Ok(false) => return Ok(0),
                        Err(error) => {
                            failure = Some(error);
                            return Ok(0);
                        }
                    }
                }
                Ok(data.len())
            })
            .map_err(|_| "The request could not be prepared.".to_owned())?;
        transfer.perform()
    };
    if let Some(failure) = failure {
        return Err(failure);
    }
    if cancel.load(Ordering::Acquire) {
        return Ok(());
    }
    let code = status.get();
    if code != 200 && code != 0 {
        let value: Value = serde_json::from_slice(&error_body).unwrap_or(Value::Null);
        let said = value
            .get("error")
            .map(error_text)
            .or_else(|| value["detail"].as_str().map(str::to_owned));
        return Err(match (code, said) {
            (401 | 403, _) if matches!(endpoint, Endpoint::ChatGpt { .. }) => {
                "ChatGPT refused the sign-in; sign in again in Settings.".into()
            }
            (401, None) => "The service refused the API key.".into(),
            (429, None) => "The service is rate limiting requests; try again shortly.".into(),
            (_, Some(said)) => said,
            (code, None) => format!("The model service answered HTTP {code}."),
        });
    }
    match result {
        Ok(()) => Ok(()),
        // The callback ending the transfer on purpose is not a failure.
        Err(error) if error.is_write_error() => Ok(()),
        Err(error) if error.is_operation_timedout() => {
            Err("The model service stopped answering.".into())
        }
        Err(error) if error.is_couldnt_resolve_host() || error.is_couldnt_connect() => {
            Err("The model service could not be reached.".into())
        }
        Err(_) => Err("The connection to the model service failed.".into()),
    }
}

/// Lists a service's models: `{base}/models` with the key.
pub(crate) fn models(endpoint: &Endpoint) -> Result<Vec<String>, String> {
    let (url, token) = match endpoint {
        Endpoint::Compatible { base, key } => {
            (format!("{}/models", base.trim_end_matches('/')), key.as_str())
        }
        Endpoint::ChatGpt { token } => ("https://api.openai.com/v1/models".into(), token.as_str()),
    };
    let (code, body) = get(&url, token)?;
    if code != 200 {
        return Err(format!("The service answered HTTP {code}."));
    }
    let listed: Value = serde_json::from_slice(&body).map_err(|_| "No model list.".to_owned())?;
    let mut models: Vec<String> = listed["data"]
        .as_array()
        .or_else(|| listed["models"].as_array())
        .into_iter()
        .flatten()
        .filter(|m| m.get("visibility").is_none_or(|v| v == "list"))
        .filter_map(|m| m["id"].as_str().or_else(|| m["slug"].as_str()))
        .map(str::to_owned)
        .collect();
    models.sort();
    models.dedup();
    Ok(models)
}

fn get(url: &str, token: &str) -> Result<(u32, Vec<u8>), String> {
    let mut easy = curl::easy::Easy::new();
    let mut body = Vec::new();
    let run = |easy: &mut curl::easy::Easy, body: &mut Vec<u8>| -> Result<(), curl::Error> {
        easy.url(url)?;
        let mut headers = curl::easy::List::new();
        if !token.is_empty() {
            headers.append(&format!("Authorization: Bearer {token}"))?;
        }
        easy.http_headers(headers)?;
        easy.connect_timeout(Duration::from_secs(10))?;
        easy.timeout(Duration::from_secs(30))?;
        let mut transfer = easy.transfer();
        transfer.write_function(|data| {
            if body.len() + data.len() > 8 * 1024 * 1024 {
                return Ok(0);
            }
            body.extend_from_slice(data);
            Ok(data.len())
        })?;
        transfer.perform()
    };
    run(&mut easy, &mut body).map_err(|_| "The service could not be reached.".to_owned())?;
    Ok((easy.response_code().unwrap_or(0), body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_events_build_text_reasoning_and_calls() {
        let mut a = Assembler::default();
        let events = [
            json!({"choices": [{"delta": {"reasoning": "think"}}]}),
            json!({"choices": [{"delta": {"content": "Hel"}}]}),
            json!({"choices": [{"delta": {"content": "lo"}}]}),
            json!({"choices": [{"delta": {"tool_calls": [{"index": 0, "id": "c1", "function": {"name": "take_", "arguments": "{\"pa"}}]}}]}),
            json!({"choices": [{"delta": {"tool_calls": [{"index": 0, "function": {"name": "snapshot", "arguments": "ge_id\":3}"}}]}}]}),
        ];
        for event in events {
            a.chat_event(&event.to_string()).expect("event");
        }
        a.chat_event("[DONE]").expect("done");
        assert!(a.done);
        assert_eq!(a.said.text, "Hello");
        assert_eq!(a.said.reasoning, "think");
        assert_eq!(
            a.said.calls,
            vec![Call {
                id: "c1".into(),
                name: "take_snapshot".into(),
                arguments: "{\"page_id\":3}".into()
            }]
        );
        assert!(
            a.chat_event(&json!({"error": {"message": "Rate limited"}}).to_string())
                .is_err(),
            "an error event fails the stream"
        );
    }

    #[test]
    fn responses_events_and_requests_translate() {
        let mut a = Assembler::default();
        for event in [
            json!({"type": "response.reasoning_summary_text.delta", "delta": "hm"}),
            json!({"type": "response.output_text.delta", "delta": "Hi"}),
            json!({"type": "response.output_item.added", "item": {"type": "function_call", "call_id": "c9", "name": "list_pages", "arguments": ""}}),
            json!({"type": "response.output_item.done", "item": {"type": "function_call", "call_id": "c9", "name": "list_pages", "arguments": "{}"}}),
            json!({"type": "response.completed"}),
        ] {
            a.responses_event(&event.to_string()).expect("event");
        }
        assert!(a.done);
        assert_eq!((a.said.text.as_str(), a.said.reasoning.as_str()), ("Hi", "hm"));
        assert_eq!(a.said.calls.len(), 1, "a call is named when it starts and completed in place");
        assert_eq!((a.said.calls[0].id.as_str(), a.said.calls[0].arguments.as_str()), ("c9", "{}"));
        let limit = json!({"type": "response.failed", "response": {"error": {"code": "subscription_sharing_usage_limit_exceeded"}}});
        assert!(
            Assembler::default()
                .responses_event(&limit.to_string())
                .unwrap_err()
                .contains("usage limit")
        );

        let messages = [
            json!({"role": "system", "content": "Be brief."}),
            json!({"role": "user", "content": [{"type": "text", "text": "what is this"}, {"type": "image_url", "image_url": {"url": "data:image/png;base64,AA"}}]}),
            json!({"role": "assistant", "content": null, "tool_calls": [{"id": "c1", "type": "function", "function": {"name": "list_pages", "arguments": "{}"}}]}),
            json!({"role": "tool", "tool_call_id": "c1", "content": "{\"pages\":[]}"}),
        ];
        let tools = [
            json!({"type": "function", "function": {"name": "list_pages", "description": "Pages", "parameters": {"type": "object"}}}),
        ];
        let request = responses_body("gpt-x", &messages, &tools);
        assert_eq!(request["instructions"], "Be brief.");
        assert_eq!((request["stream"].clone(), request["store"].clone()), (json!(true), json!(false)));
        assert_eq!(
            request["input"][0]["content"][1],
            json!({"type": "input_image", "image_url": "data:image/png;base64,AA"})
        );
        assert_eq!(request["input"][1]["namespace"], "enzim");
        assert_eq!(request["input"][2]["type"], "function_call_output");
        assert_eq!(request["tools"][0]["tools"][0]["name"], "list_pages");
    }
}
