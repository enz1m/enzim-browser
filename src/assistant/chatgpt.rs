//! Sign in with ChatGPT, as the enzimOS AI gateway does it.
//!
//! The documented public-client flow: dynamic registration on the first
//! sign-in (`dynamic_agent_client`, which needs `agent_name_hint`; the
//! authorize endpoint refuses the gateway's `ext_agent_host_id`), PKCE, and a
//! loopback callback on
//! 127.0.0.1, which the browser itself opens in a tab. The tokens are one
//! record in the desktop keyring, never in the profile's files. The ID token
//! comes straight from the token endpoint over TLS, so its claims are checked
//! without a signature check (OpenID Connect Core 3.1.3.7); it only names the
//! account.

use base64::Engine as _;
use serde_json::{Value, json};
use sha2::Digest as _;
use std::io::{Read as _, Write as _};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const AUTH: &str = "https://auth.openai.com";
const RESOURCE: &str = "https://api.openai.com/v1";
const SCOPE: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
const PLAN_SCOPE: &str = "chatgpt.tokens.use.direct";
const CALLBACK: &str = "/auth/callback";
const PORT: u16 = 1455;
const RECORD: &str = "assistant-chatgpt";

/// Refresh tokens rotate: two windows refreshing at once would lose one.
static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn random(bytes: usize) -> Result<String, String> {
    let mut buffer = vec![0u8; bytes];
    getrandom::fill(&mut buffer).map_err(|_| "No system randomness.".to_owned())?;
    Ok(base64url(&buffer))
}

fn base64url(data: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

/// The claims of a JWT, unverified.
fn claims(token: &str) -> Option<Value> {
    let part = token.split('.').nth(1)?;
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(part.trim_end_matches('='))
        .ok()?;
    serde_json::from_slice(&raw).ok()
}

fn record() -> crate::credentials::KeyringText {
    crate::credentials::KeyringText::named(RECORD, "enzim Browser assistant ChatGPT sign-in")
}

fn load() -> Value {
    record()
        .read()
        .ok()
        .flatten()
        .and_then(|secret| serde_json::from_str(secret.expose()).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}))
}

fn store(account: &Value) -> Result<(), String> {
    if record().store(&crate::credentials::Secret::new(account.to_string())) {
        Ok(())
    } else {
        Err("The desktop keyring did not keep the sign-in.".into())
    }
}

/// What a sign-in reports as it goes.
#[derive(Debug, Clone)]
pub(crate) enum Progress {
    /// Open this page for the person; it returns to the loopback origin
    /// given, which the tab must be allowed to reach.
    Open(String, String),
    /// Signed in as this email, or why not.
    Done(Result<String, String>),
}

/// Runs a whole sign-in on this thread: `report` gets the page to open and
/// then the outcome. `cancel` gives up waiting.
///
/// OpenAI can refuse the first code of a new registration (`invalid_grant`).
/// The issued client id is then kept and authorized once more, in the same
/// tab: the callback answers with a redirect to the new request.
pub(crate) fn sign_in(cancel: &AtomicBool, report: impl FnMut(Progress)) {
    sign_in_with(cancel, report, load(), token_request, store);
}

fn sign_in_with(
    cancel: &AtomicBool,
    mut report: impl FnMut(Progress),
    mut account: Value,
    exchange: impl Fn(&[(&str, &str)]) -> Result<Value, String>,
    save: impl Fn(&Value) -> Result<(), String>,
) {
    let outcome = (|| -> Result<String, String> {
        let listener = std::net::TcpListener::bind(("127.0.0.1", PORT))
            .or_else(|_| std::net::TcpListener::bind(("127.0.0.1", 0)))
            .map_err(|_| "No loopback port for the sign-in.".to_owned())?;
        let port = listener
            .local_addr()
            .map_err(|_| "No loopback port.".to_owned())?
            .port();
        let redirect = format!("http://127.0.0.1:{port}{CALLBACK}");
        let mut waiting: Option<std::net::TcpStream> = None;
        let mut retried = false;
        loop {
            let verifier = random(32)?;
            let challenge = base64url(&sha2::Sha256::digest(verifier.as_bytes()));
            let (state, nonce) = (random(16)?, random(16)?);
            let registered = account["client_id"].as_str().map(str::to_owned);
            let url = authorize_url(&account, &redirect, &state, &nonce, challenge)?;
            match waiting.take() {
                Some(mut stream) => send_to(&mut stream, &url),
                None => report(Progress::Open(url, format!("http://127.0.0.1:{port}"))),
            }
            let (code, issued, mut stream) = callback(&listener, &state, cancel)?;
            let Some(client_id) = registered.clone().or(issued) else {
                page(&mut stream, 200, "The sign-in did not finish. Start it again in the assistant's settings.");
                return Err("The sign-in returned no client registration.".into());
            };
            let tokens = match exchange(&[
                ("grant_type", "authorization_code"),
                ("client_id", &client_id),
                ("code", &code),
                ("code_verifier", &verifier),
                ("redirect_uri", &redirect),
                ("resource", RESOURCE),
            ]) {
                Ok(tokens) => tokens,
                Err(error) if error == "invalid_grant" && !retried => {
                    retried = true;
                    if registered.is_none() {
                        let _guard = LOCK.lock().map_err(|_| "Sign-in state is unavailable.")?;
                        account["client_id"] = json!(client_id);
                        save(&account)?;
                    }
                    waiting = Some(stream);
                    continue;
                }
                Err(error) => {
                    page(&mut stream, 200, "The sign-in did not finish. Start it again in the assistant's settings.");
                    if error == "invalid_client" {
                        let _guard = LOCK.lock().map_err(|_| "Sign-in state is unavailable.")?;
                        if let Some(account) = account.as_object_mut() {
                            account.remove("client_id");
                        }
                        let _ = save(&account);
                    }
                    return Err(refusal(&error));
                }
            };
            let checked = (|| -> Result<Value, String> {
                let id = claims(tokens["id_token"].as_str().unwrap_or_default())
                    .ok_or("The sign-in returned no identity.")?;
                let audience = match &id["aud"] {
                    Value::Array(list) => list.contains(&json!(client_id)),
                    other => other == &json!(client_id),
                };
                if id["iss"].as_str().map(|i| i.trim_end_matches('/')) != Some(AUTH)
                    || !audience
                    || id["exp"].as_u64().is_none_or(|exp| exp <= now())
                    || id["nonce"].as_str() != Some(nonce.as_str())
                {
                    return Err("The sign-in identity did not match this request.".into());
                }
                Ok(id)
            })();
            let id = match checked {
                Ok(id) => id,
                Err(error) => {
                    page(&mut stream, 200, "The sign-in did not finish. Start it again in the assistant's settings.");
                    return Err(error);
                }
            };
            let _guard = LOCK.lock().map_err(|_| "Sign-in state is unavailable.")?;
            account["client_id"] = json!(client_id);
            account["email"] = id["email"].clone();
            account["id_token"] = tokens["id_token"].clone();
            let kept = keep(&mut account, &tokens).and_then(|()| save(&account));
            page(
                &mut stream,
                200,
                if kept.is_ok() {
                    "Signed in to ChatGPT. You can close this tab and go back to the assistant."
                } else {
                    "The sign-in did not finish. See the assistant's settings."
                },
            );
            kept?;
            return Ok(account["email"].as_str().unwrap_or("ChatGPT").to_owned());
        }
    })();
    report(Progress::Done(outcome));
}

fn authorize_url(
    account: &Value,
    redirect: &str,
    state: &str,
    nonce: &str,
    challenge: String,
) -> Result<String, String> {
    let registered = account["client_id"].as_str();
    let mut pairs = vec![
        ("client_id", registered.unwrap_or("dynamic_agent_client").to_owned()),
        ("response_type", "code".into()),
        ("redirect_uri", redirect.to_owned()),
        ("scope", SCOPE.into()),
        ("resource", RESOURCE.into()),
        ("state", state.to_owned()),
        ("nonce", nonce.to_owned()),
        ("code_challenge_method", "S256".into()),
        ("code_challenge", challenge),
    ];
    if registered.is_none() {
        pairs.push(("agent_name_hint", "enzim Browser".into()));
    }
    for (key, field) in [("login_hint", "email"), ("id_token_hint", "id_token")] {
        if let (Some(_), Some(value)) = (registered, account[field].as_str()) {
            pairs.push((key, value.to_owned()));
        }
    }
    let mut url = url::Url::parse(&format!("{AUTH}/api/accounts/authorize"))
        .map_err(|_| "Bad sign-in address.".to_owned())?;
    url.query_pairs_mut()
        .extend_pairs(pairs.iter().map(|(k, v)| (*k, v.as_str())));
    Ok(url.into())
}

/// What a refused code exchange means to the person.
fn refusal(error: &str) -> String {
    match error {
        "invalid_grant" => "OpenAI did not accept the sign-in code twice in a row. Start the sign-in again.".into(),
        "invalid_client" => "OpenAI no longer knows this browser's registration. Start the sign-in again to register anew.".into(),
        "access_denied" => "The sign-in was declined.".into(),
        other => format!("The sign-in failed ({other})."),
    }
}

/// A current access token, refreshed when it is about to expire.
pub(crate) fn access_token() -> Result<String, String> {
    let _guard = LOCK.lock().map_err(|_| "Sign-in state is unavailable.")?;
    let mut account = load();
    let signed_out = || "Sign in to ChatGPT in the assistant's settings first.".to_owned();
    let refresh = account["refresh_token"]
        .as_str()
        .ok_or_else(signed_out)?
        .to_owned();
    if account["expires_at"]
        .as_u64()
        .is_some_and(|at| at > now() + 120)
    {
        return account["access_token"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(signed_out);
    }
    let client_id = account["client_id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let tokens = match token_request(&[
        ("grant_type", "refresh_token"),
        ("client_id", &client_id),
        ("refresh_token", &refresh),
        ("resource", RESOURCE),
    ]) {
        Ok(tokens) => tokens,
        Err(error) if error == "invalid_grant" => {
            forget(&mut account);
            let _ = store(&account);
            return Err("The ChatGPT sign-in has expired; sign in again in Settings.".into());
        }
        Err(_) => return Err("The ChatGPT sign-in service could not be reached.".into()),
    };
    keep(&mut account, &tokens)?;
    store(&account)?;
    account["access_token"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(signed_out)
}

/// Forgets the tokens; the registration stays for the next sign-in. The refresh token is revoked when the service allows it.
pub(crate) fn sign_out() {
    let Ok(_guard) = LOCK.lock() else {
        return;
    };
    let mut account = load();
    if let (Some(token), Some(client_id)) = (
        account["refresh_token"].as_str(),
        account["client_id"].as_str(),
    ) {
        let _ = revoke(token, client_id);
    }
    forget(&mut account);
    let _ = store(&account);
}

fn keep(account: &mut Value, tokens: &Value) -> Result<(), String> {
    let access = tokens["access_token"]
        .as_str()
        .ok_or("The sign-in returned no access.")?;
    let scope = tokens["scope"]
        .as_str()
        .or_else(|| tokens["scopes"].as_str());
    if scope.is_some_and(|s| !s.split(' ').any(|s| s == PLAN_SCOPE)) {
        return Err("This account did not allow ChatGPT plan use.".into());
    }
    account["access_token"] = json!(access);
    account["expires_at"] = json!(now() + tokens["expires_in"].as_u64().unwrap_or(3600));
    if let Some(refresh) = tokens["refresh_token"].as_str() {
        account["refresh_token"] = json!(refresh);
    }
    if !account["refresh_token"].is_string() {
        return Err("The sign-in returned no lasting access.".into());
    }
    Ok(())
}

fn forget(account: &mut Value) {
    if let Some(account) = account.as_object_mut() {
        for key in ["access_token", "refresh_token", "id_token", "expires_at"] {
            account.remove(key);
        }
    }
}

fn form(pairs: &[(&str, &str)]) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs)
        .finish()
}

fn post(url: &str, body: &str) -> Result<(u32, Vec<u8>), String> {
    let mut easy = curl::easy::Easy::new();
    let mut out = Vec::new();
    let run = |easy: &mut curl::easy::Easy, out: &mut Vec<u8>| -> Result<(), curl::Error> {
        easy.url(url)?;
        easy.post(true)?;
        easy.post_fields_copy(body.as_bytes())?;
        let mut headers = curl::easy::List::new();
        headers.append("Content-Type: application/x-www-form-urlencoded")?;
        headers.append("Accept: application/json")?;
        easy.http_headers(headers)?;
        easy.connect_timeout(Duration::from_secs(15))?;
        easy.timeout(Duration::from_secs(30))?;
        let mut transfer = easy.transfer();
        transfer.write_function(|data| {
            if out.len() + data.len() > 256 * 1024 {
                return Ok(0);
            }
            out.extend_from_slice(data);
            Ok(data.len())
        })?;
        transfer.perform()
    };
    run(&mut easy, &mut out).map_err(|_| "The sign-in service is unreachable.".to_owned())?;
    Ok((easy.response_code().unwrap_or(0), out))
}

/// POST a token request; an OAuth error comes back as its `error` code.
fn token_request(pairs: &[(&str, &str)]) -> Result<Value, String> {
    let (status, body) = post(&format!("{AUTH}/api/accounts/oauth/token"), &form(pairs))?;
    let value: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    if status != 200 {
        return Err(value["error"]
            .as_str()
            .unwrap_or("The sign-in service refused.")
            .to_owned());
    }
    Ok(value)
}

fn revoke(token: &str, client_id: &str) -> Result<(), String> {
    let discovery = discover()?;
    let endpoint = discovery["revocation_endpoint"]
        .as_str()
        .and_then(|u| url::Url::parse(u).ok())
        .ok_or("No revocation endpoint.")?;
    if endpoint.scheme() != "https" || endpoint.host_str() != Some("auth.openai.com") {
        return Err("Revocation endpoint on another host.".into());
    }
    let (status, _) = post(
        endpoint.as_str(),
        &form(&[
            ("token", token),
            ("token_type_hint", "refresh_token"),
            ("client_id", client_id),
        ]),
    )?;
    if status == 200 {
        Ok(())
    } else {
        Err("Not revoked.".into())
    }
}

fn discover() -> Result<Value, String> {
    let mut easy = curl::easy::Easy::new();
    let mut out = Vec::new();
    let run = |easy: &mut curl::easy::Easy, out: &mut Vec<u8>| -> Result<(), curl::Error> {
        easy.url(&format!("{AUTH}/.well-known/openid-configuration"))?;
        easy.timeout(Duration::from_secs(15))?;
        let mut transfer = easy.transfer();
        transfer.write_function(|data| {
            if out.len() + data.len() > 256 * 1024 {
                return Ok(0);
            }
            out.extend_from_slice(data);
            Ok(data.len())
        })?;
        transfer.perform()
    };
    run(&mut easy, &mut out).map_err(|_| "Unreachable.".to_owned())?;
    serde_json::from_slice(&out).map_err(|_| "No discovery.".to_owned())
}

/// Waits for the return to the loopback callback, answering every other
/// request with a small page, and returns the code, the issued client id and
/// the tab's request, still unanswered.
fn callback(
    listener: &std::net::TcpListener,
    state: &str,
    cancel: &AtomicBool,
) -> Result<(String, Option<String>, std::net::TcpStream), String> {
    listener
        .set_nonblocking(true)
        .map_err(|_| "No loopback port.".to_owned())?;
    let started = Instant::now();
    loop {
        if cancel.load(Ordering::Acquire) {
            return Err("The sign-in was cancelled.".into());
        }
        if started.elapsed() > Duration::from_secs(600) {
            return Err("The sign-in was not finished in time.".into());
        }
        let mut stream = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(150));
                continue;
            }
            Err(_) => continue,
        };
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let mut head = Vec::new();
        let mut buffer = [0u8; 2048];
        while !head.windows(4).any(|w| w == b"\r\n\r\n") && head.len() < 16 * 1024 {
            match stream.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(n) => head.extend_from_slice(&buffer[..n]),
            }
        }
        let head = String::from_utf8_lossy(&head);
        let mut words = head.lines().next().unwrap_or_default().split(' ');
        let (method, target) = (words.next().unwrap_or_default(), words.next().unwrap_or("/"));
        let Ok(url) = url::Url::parse(&format!("http://127.0.0.1{target}")) else {
            page(&mut stream, 400, "Not found.");
            continue;
        };
        if method != "GET" || url.path() != CALLBACK {
            page(&mut stream, 404, "Not found.");
            continue;
        }
        let pairs: std::collections::BTreeMap<String, String> =
            url.query_pairs().into_owned().collect();
        if pairs.get("state").map(String::as_str) != Some(state) {
            page(
                &mut stream,
                400,
                "This sign-in link is out of date. Start the sign-in again in the assistant's settings.",
            );
            continue;
        }
        if let Some(error) = pairs.get("error") {
            page(&mut stream, 200, "The sign-in was cancelled. You can close this tab.");
            return Err(if error == "access_denied" {
                "The sign-in was declined.".into()
            } else {
                format!("The sign-in failed ({error}).")
            });
        }
        let Some(code) = pairs.get("code").cloned() else {
            page(&mut stream, 200, "The sign-in did not finish. Start it again in the assistant's settings.");
            return Err("The sign-in returned no code.".into());
        };
        return Ok((code, pairs.get("client_id").cloned(), stream));
    }
}

/// Sends the waiting tab on to `url`.
fn send_to(stream: &mut std::net::TcpStream, url: &str) {
    let head = format!(
        "HTTP/1.1 302 Found\r\nLocation: {url}\r\nContent-Length: 0\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n"
    );
    let _ = stream.write_all(head.as_bytes());
}

fn page(stream: &mut std::net::TcpStream, status: u16, text: &str) {
    let body = format!(
        "<!doctype html><meta charset=utf-8><title>enzim Browser</title><body style=\"font:16px system-ui;margin:3em\"><p>{text}</p>"
    );
    let head = format!(
        "HTTP/1.1 {status} OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_token_claims_are_read() {
        let token = format!(
            "x.{}.y",
            base64url(br#"{"email":"a@b.test","nonce":"n"}"#)
        );
        assert_eq!(claims(&token).expect("claims")["email"], "a@b.test");
        assert!(claims("nonsense").is_none());
    }

    /// One GET to the loopback callback, as the tab makes it; returns the
    /// response head.
    fn visit(url: &str) -> String {
        let url = url::Url::parse(url).expect("url");
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", url.port().expect("port")))
            .expect("connect");
        let query = url.query().map(|q| format!("?{q}")).unwrap_or_default();
        write!(stream, "GET {}{query} HTTP/1.1\r\nHost: x\r\n\r\n", url.path()).expect("send");
        let mut out = String::new();
        stream.read_to_string(&mut out).expect("answer");
        out
    }

    fn param(url: &str, key: &str) -> String {
        url::Url::parse(url)
            .expect("url")
            .query_pairs()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.into_owned())
            .unwrap_or_default()
    }

    #[test]
    fn a_refused_first_code_is_retried_in_the_same_tab() {
        let (opened, opens) = std::sync::mpsc::channel();
        let (done, outcome) = std::sync::mpsc::channel();
        let exchanges = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let saved = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
        let (seen, kept) = (std::sync::Arc::clone(&exchanges), std::sync::Arc::clone(&saved));
        let nonces = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let nonce_of = std::sync::Arc::clone(&nonces);
        std::thread::spawn(move || {
            let cancel = AtomicBool::new(false);
            sign_in_with(
                &cancel,
                |progress| match progress {
                    Progress::Open(url, origin) => opened.send((url, origin)).expect("open"),
                    Progress::Done(result) => done.send(result).expect("done"),
                },
                json!({}),
                |pairs| {
                    let get = |k: &str| pairs.iter().find(|(key, _)| *key == k).map(|(_, v)| v.to_string()).unwrap_or_default();
                    seen.lock().expect("log").push(format!("{}:{}", get("client_id"), get("code")));
                    if get("code") == "first" {
                        return Err("invalid_grant".into());
                    }
                    let nonce = nonce_of.lock().expect("nonce").last().cloned().unwrap_or_default();
                    let id = format!(
                        "x.{}.y",
                        base64url(json!({"iss": AUTH, "aud": "oaiapp_new", "exp": now() + 600, "nonce": nonce, "email": "a@b.test"}).to_string().as_bytes())
                    );
                    Ok(json!({"access_token": "a", "refresh_token": "r", "id_token": id, "scope": PLAN_SCOPE}))
                },
                move |account| {
                    kept.lock().expect("saved").push(account.clone());
                    Ok(())
                },
            );
        });
        let (first, origin) = opens.recv_timeout(Duration::from_secs(5)).expect("opened");
        assert_eq!(param(&first, "client_id"), "dynamic_agent_client");
        assert!(param(&first, "redirect_uri").starts_with(&origin));
        nonces.lock().expect("nonce").push(param(&first, "nonce"));
        let back = format!("{}?code=first&client_id=oaiapp_new&state={}", param(&first, "redirect_uri"), param(&first, "state"));
        let answer = visit(&back);
        assert!(answer.starts_with("HTTP/1.1 302"), "the tab is sent on: {answer}");
        let again = answer
            .lines()
            .find_map(|l| l.strip_prefix("Location: "))
            .expect("location")
            .to_owned();
        assert_eq!(param(&again, "client_id"), "oaiapp_new", "the issued client is authorized");
        assert_ne!(param(&again, "state"), param(&first, "state"));
        assert_eq!(saved.lock().expect("saved")[0]["client_id"], "oaiapp_new", "the registration is kept");
        nonces.lock().expect("nonce").push(param(&again, "nonce"));
        let back = format!("{}?code=second&state={}", param(&again, "redirect_uri"), param(&again, "state"));
        assert!(visit(&back).contains("Signed in to ChatGPT"));
        assert_eq!(outcome.recv_timeout(Duration::from_secs(5)).expect("done"), Ok("a@b.test".into()));
        assert_eq!(*exchanges.lock().expect("log"), ["oaiapp_new:first", "oaiapp_new:second"]);
        assert!(opens.try_recv().is_err(), "no second tab");
    }

    #[test]
    fn tokens_without_plan_scope_are_refused() {
        let mut account = json!({});
        assert!(
            keep(
                &mut account,
                &json!({"access_token": "a", "refresh_token": "r", "scope": "openid email"})
            )
            .is_err()
        );
        keep(
            &mut account,
            &json!({"access_token": "a", "refresh_token": "r", "scope": "openid chatgpt.tokens.use.direct", "expires_in": 60}),
        )
        .expect("kept");
        assert_eq!(account["refresh_token"], "r");
        forget(&mut account);
        assert!(account.get("refresh_token").is_none());
    }
}
