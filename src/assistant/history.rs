//! Saved conversations, one JSON file per chat in the profile's
//! `assistant-chats` folder (owner-only). Pictures are not kept: in the
//! conversation the model reads they become a short note, and transcript
//! rows keep only text.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// A saved chat.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Saved {
    pub(crate) id: String,
    pub(crate) title: String,
    /// Seconds since the Unix epoch.
    pub(crate) updated: i64,
    pub(crate) rows: Vec<SavedRow>,
    /// What the model reads, without the system message.
    pub(crate) conversation: Vec<Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum SavedRow {
    User { text: String, pictures: usize },
    Reply { text: String },
    Thought { text: String },
    Tool { name: String, arguments: String, output: String, ok: bool },
    Links { links: Vec<(String, String, String)> },
    Note { text: String, error: bool },
}

/// What the history list shows of a chat.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub(crate) struct Summary {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) updated: i64,
}

pub(crate) fn folder() -> PathBuf {
    crate::browser::state_path().with_file_name("assistant-chats")
}

/// A chat id is a file name: digits only.
fn path(folder: &Path, id: &str) -> Option<PathBuf> {
    (!id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())).then(|| folder.join(format!("{id}.json")))
}

/// A new chat id: the time in milliseconds.
pub(crate) fn new_id() -> String {
    chrono::Utc::now().timestamp_millis().to_string()
}

pub(crate) fn save(folder: &Path, chat: &Saved) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let target = path(folder, &chat.id).ok_or_else(|| std::io::Error::other("Bad chat id."))?;
    std::fs::create_dir_all(folder)?;
    std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o700))?;
    let tmp = target.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(&tmp, serde_json::to_vec(chat)?)?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    std::fs::rename(tmp, target)
}

pub(crate) fn load(folder: &Path, id: &str) -> Option<Saved> {
    serde_json::from_slice(&std::fs::read(path(folder, id)?).ok()?).ok()
}

pub(crate) fn delete(folder: &Path, id: &str) {
    if let Some(path) = path(folder, id) {
        let _ = std::fs::remove_file(path);
    }
}

/// The saved chats, newest first.
pub(crate) fn list(folder: &Path) -> Vec<Summary> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut chats: Vec<Summary> = entries
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|e| e == "json"))
        .filter_map(|entry| serde_json::from_slice(&std::fs::read(entry.path()).ok()?).ok())
        .collect();
    chats.sort_by(|a, b| b.updated.cmp(&a.updated).then_with(|| b.id.cmp(&a.id)));
    chats
}

/// The saved chats that best match `query`, best first: each with its id,
/// title, date and up to two excerpts. A chat matches by the query's words
/// it contains (case ignored), more words first, then more often, then newer.
pub(crate) fn search(folder: &Path, query: &str, limit: usize) -> Vec<Value> {
    let words: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.chars().count() > 1)
        .map(str::to_lowercase)
        .collect();
    if words.is_empty() {
        return Vec::new();
    }
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found: Vec<(usize, usize, i64, Value)> = Vec::new();
    for entry in entries.flatten().filter(|entry| entry.path().extension().is_some_and(|e| e == "json")) {
        let Some(chat) = std::fs::read(entry.path()).ok().and_then(|bytes| serde_json::from_slice::<Saved>(&bytes).ok()) else {
            continue;
        };
        let text: String = chat
            .rows
            .iter()
            .filter_map(|row| match row {
                SavedRow::User { text, .. } => Some(format!("Person: {text}")),
                SavedRow::Reply { text } => Some(format!("You: {text}")),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        let lower = text.to_lowercase();
        let present: Vec<&String> = words.iter().filter(|word| lower.contains(word.as_str())).collect();
        if present.is_empty() {
            continue;
        }
        let times = present.iter().map(|word| lower.matches(word.as_str()).count()).sum();
        let mut excerpts = Vec::new();
        for word in present.iter().take(2) {
            if let Some(at) = lower.find(word.as_str()) {
                excerpts.push(excerpt(&text, &lower, at));
            }
        }
        excerpts.dedup();
        let date = chrono::DateTime::from_timestamp(chat.updated, 0).map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default();
        found.push((present.len(), times, chat.updated, json!({"chat_id": chat.id, "title": chat.title, "date": date, "excerpts": excerpts})));
    }
    found.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(b.2.cmp(&a.2)));
    found.into_iter().take(limit).map(|(.., chat)| chat).collect()
}

/// About 300 characters of `text` around byte `at` of its lowercase form.
fn excerpt(text: &str, lower: &str, at: usize) -> String {
    // Lowercasing can change lengths; count in characters of the lowercase text.
    let index = lower.get(..at).map_or(0, |head| head.chars().count());
    let start = index.saturating_sub(120);
    let piece: String = text.chars().skip(start).take(300).collect();
    format!("{}{}{}", if start > 0 { "…" } else { "" }, piece.trim(), if text.chars().count() > start + 300 { "…" } else { "" })
}

/// The conversation with its pictures replaced by a note.
pub(crate) fn without_pictures(conversation: &[Value]) -> Vec<Value> {
    conversation
        .iter()
        .map(|message| {
            let Some(parts) = message["content"].as_array() else {
                return message.clone();
            };
            let parts: Vec<Value> = parts
                .iter()
                .map(|part| {
                    if part["type"] == "image_url" {
                        json!({"type": "text", "text": "(A picture was here; it was not kept.)"})
                    } else {
                        part.clone()
                    }
                })
                .collect();
            let mut message = message.clone();
            message["content"] = Value::Array(parts);
            message
        })
        .collect()
}

/// A chat's title: the start of its first message.
pub(crate) fn title_of(text: &str) -> String {
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    let mut title: String = line.chars().take(60).collect();
    if line.chars().count() > 60 {
        title.push('…');
    }
    if title.is_empty() { "Pictures".into() } else { title }
}

/// When a chat was last used, as the list shows it.
pub(crate) fn when(updated: i64, now: chrono::DateTime<chrono::Local>) -> String {
    use chrono::TimeZone as _;
    let Some(then) = chrono::Local.timestamp_opt(updated, 0).single() else {
        return String::new();
    };
    let days = (now.date_naive() - then.date_naive()).num_days();
    match days {
        0 => then.format("%H:%M").to_string(),
        1 => "Yesterday".into(),
        2..=6 => then.format("%A").to_string(),
        _ if then.format("%Y").to_string() == now.format("%Y").to_string() => then.format("%b %-d").to_string(),
        _ => then.format("%b %-d, %Y").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chats_round_trip_newest_first_without_pictures() {
        let folder = std::env::temp_dir().join(format!("enzim-chats-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let conversation = vec![json!({"role": "user", "content": [
            {"type": "text", "text": "What is this?"},
            {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}}
        ]})];
        let older = Saved { id: "100".into(), title: "Older".into(), updated: 10, ..Saved::default() };
        let newer = Saved {
            id: "200".into(),
            title: "Newer".into(),
            updated: 20,
            rows: vec![SavedRow::User { text: "What is this?".into(), pictures: 1 }, SavedRow::Reply { text: "A cat.".into() }],
            conversation: without_pictures(&conversation),
        };
        save(&folder, &older).expect("saved");
        save(&folder, &newer).expect("saved");
        let listed: Vec<String> = list(&folder).into_iter().map(|s| s.title).collect();
        assert_eq!(listed, ["Newer", "Older"]);
        let loaded = load(&folder, "200").expect("loaded");
        assert_eq!(loaded, newer);
        assert!(!serde_json::to_string(&loaded.conversation).expect("json").contains("base64"), "pictures are not kept");
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(folder.join("200.json")).expect("file").permissions().mode();
        assert_eq!(mode & 0o077, 0, "only the owner reads saved chats");
        assert!(load(&folder, "../session").is_none(), "an id is never a path");
        delete(&folder, "100");
        assert_eq!(list(&folder).len(), 1);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn chats_are_found_by_their_words() {
        let folder = std::env::temp_dir().join(format!("enzim-chat-search-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let chat = |id: &str, updated: i64, user: &str, reply: &str| Saved {
            id: id.into(),
            title: title_of(user),
            updated,
            rows: vec![SavedRow::User { text: user.into(), pictures: 0 }, SavedRow::Reply { text: reply.into() }],
            conversation: Vec::new(),
        };
        save(&folder, &chat("1", 10, "Plan a trip to Rome", "Fly into Fiumicino and stay near Trastevere.")).expect("save");
        save(&folder, &chat("2", 20, "Which laptop?", "The ThinkPad is lighter.")).expect("save");
        save(&folder, &chat("3", 30, "Rome hotels", "Try the one by the Pantheon.")).expect("save");
        let found = search(&folder, "Rome trip", 4);
        let ids: Vec<&str> = found.iter().filter_map(|chat| chat["chat_id"].as_str()).collect();
        assert_eq!(ids, ["1", "3"], "both words before one; laptops not at all");
        assert!(found[0]["excerpts"][0].as_str().expect("excerpt").contains("Rome"));
        assert_eq!(found[0]["date"], "1970-01-01");
        assert!(search(&folder, "a", 4).is_empty(), "one-letter words are not searched");
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn titles_take_the_first_line() {
        assert_eq!(title_of("\n  Plan a trip to Rome\nwith kids"), "Plan a trip to Rome");
        assert_eq!(title_of(""), "Pictures");
        assert!(title_of(&"x".repeat(80)).ends_with('…'));
    }
}
