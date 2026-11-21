//! Other apps through the enzUI agent center, as the enzimOS assistant
//! reaches them: `enzui-mcp` over stdio with the assistant's own agent
//! credential. The center asks the person before each app function runs; a
//! call it holds for that answer comes back `permission_required` and is asked
//! again until the person decides.
//!
//! The process runs only for a turn that uses one of its tools.

use serde_json::{Value, json};
use std::io::{BufRead as _, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// The agent identity the assistant is enrolled under, beside the browser's
/// own app identity: the center keeps the two roles apart.
pub(crate) const AGENT_ID: &str = "org.enzim.Browser.Assistant";

/// The tools the center's adapter offers; the browser's own functions are
/// the assistant's directly, so the browser is left out of `enzui_apps`.
pub(crate) const TOOLS: [&str; 4] = ["enzui_apps", "enzui_functions", "enzui_function", "enzui_call"];

/// Where the assistant's agent credential is kept.
pub(crate) fn credential_path() -> PathBuf {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from("/nonexistent"));
    config
        .join("enzui/agents")
        .join(format!("{AGENT_ID}.credential.json"))
}

/// A center program: `ENZIM_ENZUI_<NAME>` when set (development builds),
/// else the first on `PATH`. Looked up once; installing the center needs a
/// browser restart to be seen.
pub(crate) fn program(name: &str) -> Option<PathBuf> {
    static FOUND: std::sync::LazyLock<std::sync::Mutex<std::collections::BTreeMap<String, Option<PathBuf>>>> =
        std::sync::LazyLock::new(Default::default);
    let mut found = FOUND.lock().ok()?;
    found.entry(name.to_owned()).or_insert_with(|| find(name)).clone()
}

fn find(name: &str) -> Option<PathBuf> {
    let variable = format!("ENZIM_{}", name.to_uppercase().replace('-', "_"));
    if let Some(path) = std::env::var_os(variable).map(PathBuf::from)
        && path.is_file()
    {
        return Some(path);
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(name))
            .find(|path| path.is_file())
    })
}

/// Whether the center can be used: its adapter is installed and the
/// assistant is enrolled.
pub(crate) fn available() -> bool {
    program("enzui-mcp").is_some() && credential_path().is_file()
}

/// Enrolls the assistant as an agent of the center, writing its credential.
pub(crate) fn enroll() -> Result<(), String> {
    let center = program("enzui-center").ok_or("The agent center is not installed.")?;
    let path = credential_path();
    // The center writes credentials only into a private folder.
    if let Some(parent) = path.parent() {
        use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)
            .and_then(|()| std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)))
            .map_err(|_| "The credential folder could not be made private.")?;
    }
    let output = Command::new(center)
        .args(["enroll", "agent", "--id", AGENT_ID, "--name", "enzim Browser assistant", "--output"])
        .arg(&path)
        .stdin(Stdio::null())
        .output()
        .map_err(|_| "The agent center could not be started.")?;
    if output.status.success() && path.is_file() {
        Ok(())
    } else {
        let said = String::from_utf8_lossy(&output.stderr);
        Err(said
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .map_or_else(|| "The agent center refused the enrollment.".into(), |l| l.trim().chars().take(200).collect()))
    }
}

/// A running `enzui-mcp`.
pub(crate) struct Client {
    child: Child,
    stdin: ChildStdin,
    lines: mpsc::Receiver<String>,
    next: u64,
}

impl Client {
    pub(crate) fn start(program: &Path, credential: &Path) -> Result<Self, String> {
        let mut child = Command::new(program)
            .arg("--credential")
            .arg(credential)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "The agent center adapter could not be started.")?;
        let stdin = child.stdin.take().ok_or("No adapter input.")?;
        let stdout: ChildStdout = child.stdout.take().ok_or("No adapter output.")?;
        let (send, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if send.send(line).is_err() {
                    break;
                }
            }
        });
        let mut client = Self {
            child,
            stdin,
            lines,
            next: 0,
        };
        client.request(
            "initialize",
            json!({"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "enzim-browser-assistant", "version": env!("CARGO_PKG_VERSION")}}),
            Duration::from_secs(10),
            &AtomicBool::new(false),
        )?;
        client.write(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))?;
        Ok(client)
    }

    fn write(&mut self, value: &Value) -> Result<(), String> {
        let mut line = value.to_string();
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .and_then(|()| self.stdin.flush())
            .map_err(|_| "The agent center adapter closed.".to_owned())
    }

    fn request(
        &mut self,
        method: &str,
        params: Value,
        limit: Duration,
        cancel: &AtomicBool,
    ) -> Result<Value, String> {
        self.next += 1;
        let id = self.next;
        self.write(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        let started = Instant::now();
        loop {
            if cancel.load(Ordering::Acquire) {
                return Err("Stopped.".into());
            }
            let left = limit.saturating_sub(started.elapsed());
            if left.is_zero() {
                return Err("The agent center did not answer in time.".into());
            }
            match self.lines.recv_timeout(left.min(Duration::from_millis(200))) {
                Ok(line) => {
                    let Ok(value) = serde_json::from_str::<Value>(&line) else {
                        continue;
                    };
                    if value["id"] != json!(id) {
                        continue;
                    }
                    if let Some(error) = value.get("error") {
                        return Err(error["message"]
                            .as_str()
                            .unwrap_or("The agent center refused.")
                            .to_owned());
                    }
                    return Ok(value["result"].clone());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("The agent center adapter closed.".into());
                }
            }
        }
    }

    /// The adapter's tools, as function tools for the model.
    pub(crate) fn tools(&mut self) -> Result<Vec<Value>, String> {
        let listed = self.request("tools/list", json!({}), Duration::from_secs(10), &AtomicBool::new(false))?;
        Ok(listed["tools"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|tool| tool["name"].as_str().is_some_and(|n| TOOLS.contains(&n)))
            .map(|tool| {
                json!({"type": "function", "function": {
                    "name": tool["name"],
                    "description": tool["description"],
                    "parameters": tool.get("inputSchema").cloned().unwrap_or_else(|| json!({"type": "object"})),
                }})
            })
            .collect())
    }

    /// Calls a tool; `waiting` is told while the center waits for the
    /// person's consent.
    pub(crate) fn call(
        &mut self,
        name: &str,
        arguments: &Value,
        cancel: &AtomicBool,
        mut waiting: impl FnMut(),
    ) -> Result<Value, String> {
        let started = Instant::now();
        let mut told = false;
        loop {
            let result = self.request(
                "tools/call",
                json!({"name": name, "arguments": arguments}),
                Duration::from_secs(90),
                cancel,
            )?;
            let text = result["content"][0]["text"].as_str().unwrap_or_default();
            let error = (result["isError"] == json!(true))
                .then(|| serde_json::from_str::<Value>(text).unwrap_or_else(|_| json!({"message": text})));
            let Some(error) = error else {
                let mut value = result
                    .get("structuredContent")
                    .cloned()
                    .unwrap_or_else(|| serde_json::from_str(text).unwrap_or_else(|_| json!({"text": text})));
                if name == "enzui_apps" {
                    hide_browser(&mut value);
                }
                return Ok(value);
            };
            if error["code"] != "permission_required" {
                return Err(error["message"]
                    .as_str()
                    .map_or_else(|| text.chars().take(400).collect(), str::to_owned));
            }
            if started.elapsed() > Duration::from_secs(180) {
                return Err("The person did not answer the agent center in time.".into());
            }
            if !told {
                waiting();
                told = true;
            }
            for _ in 0..10 {
                if cancel.load(Ordering::Acquire) {
                    return Err("Stopped.".into());
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

fn hide_browser(value: &mut Value) {
    if let Some(apps) = value["apps"].as_array_mut() {
        apps.retain(|app| app["id"] != crate::desktop::APP_ID);
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
