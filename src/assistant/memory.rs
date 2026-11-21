//! Short notes the assistant keeps between chats, when the person turns
//! memory on. They live in the assistant's settings, are bounded in size, and
//! go into the system message at the start of each turn. The `memory` tool
//! changes them; `search_chats` reads saved chats.

use serde_json::{Value, json};

/// All notes together, in characters.
pub(crate) const LIMIT: usize = 2_000;
/// One note, in characters.
pub(crate) const NOTE_LIMIT: usize = 300;

/// The tools offered while memory is on. Small models call plainly named
/// tools more reliably than one tool with an action.
pub(crate) fn tools() -> Vec<Value> {
    vec![
        json!({"type": "function", "function": {
            "name": "remember",
            "description": "Save a note about the user for later chats: a lasting fact they share or ask you to remember (name, preferences, projects). One fact, under 300 characters. Not page contents or task progress.",
            "parameters": {"type": "object", "properties": {"note": {"type": "string"}}, "required": ["note"]},
        }}),
        json!({"type": "function", "function": {
            "name": "forget",
            "description": "Delete the saved note containing this text, when it is wrong, outdated or the user asks. To change a note, forget it and remember the new one.",
            "parameters": {"type": "object", "properties": {"part": {"type": "string", "description": "A part of the note, unique among the notes."}}, "required": ["part"]},
        }}),
        json!({"type": "function", "function": {
            "name": "search_chats",
            "description": "Search the user's earlier chats with you by words; returns matching chats with their date and short excerpts.",
            "parameters": {"type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"]},
        }}),
    ]
}

/// Whether `name` is one of the tools that change the notes.
pub(crate) fn changes(name: &str) -> bool {
    matches!(name, "remember" | "forget")
}

fn used(notes: &[String]) -> usize {
    notes.iter().map(|note| note.chars().count()).sum()
}

/// Applies a `remember` or `forget` call to `notes`; the error says what to
/// do instead.
pub(crate) fn apply(notes: &mut Vec<String>, name: &str, arguments: &Value) -> Result<Value, String> {
    let text = arguments["note"].as_str().unwrap_or_default().trim();
    let old = arguments["part"].as_str().unwrap_or_default().trim();
    let find = |notes: &[String]| -> Result<usize, String> {
        if old.is_empty() {
            return Err("Give part, a part of the note.".into());
        }
        let found: Vec<usize> = notes.iter().enumerate().filter(|(_, note)| note.contains(old)).map(|(i, _)| i).collect();
        match found.as_slice() {
            [one] => Ok(*one),
            [] => Err(format!("No note contains \"{old}\".")),
            _ => Err(format!("{} notes contain \"{old}\"; give a longer part.", found.len())),
        }
    };
    let mut changed = notes.clone();
    match name {
        "remember" if text.is_empty() => return Err("Give the note.".into()),
        "remember" if text.chars().count() > NOTE_LIMIT => {
            return Err(format!("A note is at most {NOTE_LIMIT} characters; shorten it."));
        }
        "remember" if changed.iter().any(|note| note == text) => return Ok(json!({"saved": true, "note": "Already saved."})),
        "remember" => changed.push(text.to_owned()),
        "forget" => {
            let at = find(&changed)?;
            changed.remove(at);
        }
        other => return Err(format!("Unknown memory function {other}.")),
    }
    if used(&changed) > LIMIT {
        return Err(format!(
            "Memory is full ({} of {LIMIT} characters). Forget notes first; they are: {}",
            used(notes),
            serde_json::to_string(notes).unwrap_or_default()
        ));
    }
    *notes = changed;
    Ok(json!({"saved": true, "used": format!("{} of {LIMIT} characters", used(notes))}))
}

/// The notes as the system message carries them.
pub(crate) fn prompt(notes: &[String]) -> String {
    let mut block = format!(
        "You have a memory. When the user asks you to remember something, or shares a lasting fact about themselves, call remember. When they mention an earlier chat, call search_chats. What you know about the user ({}% of your {LIMIT}-character memory):",
        used(notes) * 100 / LIMIT
    );
    if notes.is_empty() {
        block.push_str(" nothing yet.");
    }
    for note in notes {
        block.push_str("\n- ");
        block.push_str(note);
    }
    block
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_are_remembered_forgotten_and_bounded() {
        let mut notes = Vec::new();
        let remember = |notes: &mut Vec<String>, note: &str| apply(notes, "remember", &json!({"note": note}));
        remember(&mut notes, "Prefers answers in French.").expect("remember");
        remember(&mut notes, "Works on the enzim Browser.").expect("remember");
        remember(&mut notes, "Works on the enzim Browser.").expect("a repeat is kept once");
        assert_eq!(notes.len(), 2);
        assert!(apply(&mut notes, "forget", &json!({"part": "s"})).unwrap_err().contains("2 notes"), "an ambiguous part is refused");
        apply(&mut notes, "forget", &json!({"part": "enzim"})).expect("forget");
        assert_eq!(notes, ["Prefers answers in French."]);
        assert!(remember(&mut notes, &"x".repeat(NOTE_LIMIT + 1)).is_err());
        for i in 0..7 {
            remember(&mut notes, &format!("{i} {}", "y".repeat(270))).expect("fits");
        }
        let full = remember(&mut notes, &"z".repeat(200)).unwrap_err();
        assert!(full.contains("Memory is full") && full.contains("French"), "a full memory lists the notes to tidy");
        assert!(used(&notes) <= LIMIT);
        assert!(prompt(&notes).contains("\n- Prefers answers in French."));
        assert!(prompt(&[]).ends_with("nothing yet."));
        assert!(changes("forget") && !changes("search_chats"));
    }
}
