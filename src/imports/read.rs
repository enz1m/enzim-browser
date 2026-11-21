use super::snapshot::{MAX_JSON, Profile};
use super::{Bookmark, Family, Preview, Source, web_url};
use crate::{
    history::Visit,
    search::{Engine, Search},
};
use rusqlite::Connection;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
const MAX_ROWS: usize = 100_000;
const MAX_TEXT_BYTES: usize = 64 * 1024 * 1024;
fn json(bytes: &[u8]) -> Result<Value, String> {
    serde_json::from_slice(bytes).map_err(|e| e.to_string())
}
fn title(value: &str) -> String {
    value.chars().take(512).collect()
}
fn add_bookmark(
    out: &mut Vec<Bookmark>,
    url: String,
    name: String,
    folders: &[String],
    bytes: &mut usize,
) -> Result<(), String> {
    let name = if name.trim().is_empty() {
        url.clone()
    } else {
        name
    };
    *bytes += url.len() + name.len() + folders.iter().map(String::len).sum::<usize>();
    if *bytes > MAX_TEXT_BYTES {
        return Err("Bookmarks exceed the 64 MiB text import limit.".into());
    }
    out.push(Bookmark {
        url,
        title: name,
        folders: folders.to_vec(),
    });
    Ok(())
}
fn chromium_bookmarks(
    node: &Value,
    folders: &mut Vec<String>,
    out: &mut Vec<Bookmark>,
    count: &mut usize,
    bytes: &mut usize,
) -> Result<(), String> {
    *count += 1;
    if *count > MAX_ROWS || folders.len() > 64 {
        return Err("The bookmark tree exceeds the import limit.".into());
    }
    match node["type"].as_str() {
        Some("url") => {
            if let Some(url) = node["url"].as_str().and_then(web_url) {
                let name = title(node["name"].as_str().unwrap_or(&url));
                add_bookmark(out, url, name, folders, bytes)?;
            }
        }
        Some("folder") => {
            let label = title(node["name"].as_str().unwrap_or("Bookmarks"));
            folders.push(label);
            if let Some(children) = node["children"].as_array() {
                for child in children {
                    chromium_bookmarks(child, folders, out, count, bytes)?;
                }
            }
            folders.pop();
        }
        _ => {}
    }
    Ok(())
}
fn read_chromium_bookmarks(profile: &Profile) -> Result<Vec<Bookmark>, String> {
    let value = json(&profile.bytes("Bookmarks")?)?;
    let roots = value["roots"]
        .as_object()
        .ok_or("No bookmark roots were found")?;
    let mut out = Vec::new();
    let mut count = 0;
    let mut bytes = 0;
    for (key, node) in roots {
        if key != "sync_transaction_version" {
            chromium_bookmarks(node, &mut Vec::new(), &mut out, &mut count, &mut bytes)?;
        }
    }
    Ok(out)
}
fn read_firefox_bookmarks(db: &Connection) -> Result<Vec<Bookmark>, String> {
    let has_guid = db
        .prepare("PRAGMA table_info(moz_bookmarks)")
        .map_err(|e| e.to_string())?
        .query_map([], |r| r.get::<_, String>(1))
        .map_err(|e| e.to_string())?
        .any(|row| row.is_ok_and(|name| name == "guid"));
    let guid = if has_guid { "coalesce(guid,'')" } else { "''" };
    let mut statement=db.prepare(&format!("SELECT id,parent,type,coalesce(title,''),coalesce(fk,0),{guid} FROM moz_bookmarks ORDER BY parent,position LIMIT 100001")).map_err(|e|e.to_string())?;
    let rows = statement
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut records = BTreeMap::new();
    let mut order = Vec::new();
    for row in rows {
        let (id, parent, kind, name, fk, guid) = row.map_err(|e| e.to_string())?;
        order.push(id);
        records.insert(id, (parent, kind, title(&name), fk, guid == "tags________"));
    }
    if records.len() > MAX_ROWS {
        return Err("The bookmark tree exceeds 100,000 entries.".into());
    }
    let mut places = db
        .prepare("SELECT url FROM moz_places WHERE id=?1 AND length(url)<=8192")
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    let mut bytes = 0;
    for id in order {
        let Some((parent, kind, name, fk, _)) = records.get(&id) else {
            continue;
        };
        if *kind != 1 {
            continue;
        }
        let url = match places.query_row([fk], |r| r.get::<_, String>(0)) {
            Ok(address) => web_url(&address),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(error) => return Err(error.to_string()),
        };
        let Some(url) = url else {
            continue;
        };
        let mut folders = Vec::new();
        let mut seen = BTreeSet::new();
        let mut ancestor = *parent;
        let mut tag = false;
        while let Some((next, kind, label, _, tags)) = records.get(&ancestor) {
            if !seen.insert(ancestor) || seen.len() > 64 {
                return Err(
                    "The bookmark folders contain a cycle or are too deeply nested.".into(),
                );
            }
            if *tags {
                tag = true;
                break;
            }
            if *kind == 2 && *next != 0 && !label.is_empty() {
                folders.push(label.clone());
            }
            ancestor = *next;
        }
        if tag {
            continue;
        }
        folders.reverse();
        add_bookmark(&mut out, url, name.clone(), &folders, &mut bytes)?;
    }
    Ok(out)
}

fn history(db: &Connection, family: Family) -> Result<(Vec<Visit>, bool), String> {
    let sql = match family {
        Family::Firefox => {
            "SELECT p.url,coalesce(p.title,''),v.visit_date FROM moz_places p JOIN moz_historyvisits v ON v.place_id=p.id WHERE v.visit_type IN (1,2,3,9) AND length(p.url)<=8192 AND length(coalesce(p.title,''))<=4096 ORDER BY v.visit_date DESC LIMIT 100001"
        }
        Family::Chromium => {
            "SELECT u.url,coalesce(u.title,''),v.visit_time FROM urls u JOIN visits v ON v.url=u.id WHERE u.hidden=0 AND (v.transition & 255) NOT IN (3,4,10) AND (v.transition & 536870912)!=0 AND length(u.url)<=8192 AND length(coalesce(u.title,''))<=4096 ORDER BY v.visit_time DESC LIMIT 100001"
        }
    };
    let mut statement = db.prepare(sql).map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    let mut bytes = 0;
    let mut truncated = false;
    let latest = chrono::Utc::now()
        .timestamp_millis()
        .saturating_add(24 * 60 * 60 * 1000);
    for (scanned, row) in rows.enumerate() {
        if scanned >= MAX_ROWS {
            truncated = true;
            break;
        }
        let (address, name, time) = row.map_err(|e| e.to_string())?;
        let Some(url) = web_url(&address) else {
            continue;
        };
        let visited_at = match family {
            Family::Firefox => time / 1000,
            Family::Chromium => (time / 1000).saturating_sub(11_644_473_600_000),
        };
        if visited_at <= 0 || visited_at > latest {
            continue;
        }
        let name = title(&name);
        bytes += url.len() + name.len();
        if out.len() >= MAX_ROWS || bytes > MAX_TEXT_BYTES {
            truncated = true;
            break;
        }
        out.push(Visit {
            id: 0,
            url,
            title: name,
            visited_at,
            container: None,
        });
    }
    Ok((out, truncated))
}
fn engine(name: &str, template: &str) -> Option<Engine> {
    let template = template
        .replace("{searchTerms}", "{query}")
        .replace("{inputEncoding}", "UTF-8");
    let remainder = template.replace("{query}", "");
    if remainder.contains(['{', '}']) {
        return None;
    }
    let mut search = Search::default();
    search.add_custom(&title(name), &template).ok()?;
    search.custom.pop()
}
fn chromium_engines(db: &Connection) -> Result<Vec<Engine>, String> {
    let has_post=db.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('keywords') WHERE name='search_url_post_params')",[],|row|row.get::<_,bool>(0)).map_err(|e|e.to_string())?;
    let post = if has_post {
        " AND coalesce(search_url_post_params,'')=''"
    } else {
        ""
    };
    let mut statement=db.prepare(&format!("SELECT short_name,url FROM keywords WHERE length(url)<=4096 AND length(short_name)<=512{post} LIMIT 256")).map_err(|e|e.to_string())?;
    let rows = statement
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let (name, url) = row.map_err(|e| e.to_string())?;
        if let Some(engine) = engine(&name, &url) {
            out.push(engine);
        }
    }
    Ok(out)
}
pub(super) fn decompress_moz(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.get(..8) != Some(b"mozLz40\0") {
        return Err("Unrecognized Firefox compressed JSON header".into());
    }
    let length = u32::from_le_bytes(
        bytes
            .get(8..12)
            .ok_or("Truncated Firefox JSON")?
            .try_into()
            .map_err(|_| "Invalid size")?,
    ) as usize;
    if length as u64 > MAX_JSON {
        return Err("Firefox search data exceeds the 16 MiB import limit".into());
    }
    let mut out = Vec::with_capacity(length);
    let mut at = 12;
    let extend = |at: &mut usize, mut len: usize| -> Result<usize, String> {
        if len == 15 {
            loop {
                let more = *bytes.get(*at).ok_or("Truncated LZ4 length")? as usize;
                *at += 1;
                len = len.checked_add(more).ok_or("Invalid LZ4 size")?;
                if len > length {
                    return Err("Invalid LZ4 size".into());
                }
                if more != 255 {
                    break;
                }
            }
        }
        Ok(len)
    };
    while at < bytes.len() {
        let token = bytes[at];
        at += 1;
        let literals = extend(&mut at, usize::from(token >> 4))?;
        let end = at.checked_add(literals).ok_or("Invalid LZ4 size")?;
        if out.len().saturating_add(literals) > length {
            return Err("Invalid LZ4 literals".into());
        }
        out.extend_from_slice(bytes.get(at..end).ok_or("Truncated LZ4 literals")?);
        at = end;
        if at == bytes.len() {
            break;
        }
        let offset = u16::from_le_bytes(
            bytes
                .get(at..at + 2)
                .ok_or("Truncated LZ4 offset")?
                .try_into()
                .map_err(|_| "Invalid offset")?,
        ) as usize;
        at += 2;
        let repeated = extend(&mut at, usize::from(token & 15))?
            .checked_add(4)
            .ok_or("Invalid LZ4 size")?;
        if offset == 0 || offset > out.len() || out.len().saturating_add(repeated) > length {
            return Err("Invalid LZ4 back-reference".into());
        }
        for _ in 0..repeated {
            out.push(out[out.len() - offset]);
        }
    }
    if out.len() != length {
        return Err("Firefox JSON size does not match its header".into());
    }
    Ok(out)
}
fn firefox_engines(profile: &Profile) -> Result<Vec<Engine>, String> {
    let value = json(&decompress_moz(&profile.bytes("search.json.mozlz4")?)?)?;
    let mut out = Vec::new();
    for item in value["engines"]
        .as_array()
        .ok_or("No search engines were found")?
        .iter()
        .take(256)
    {
        if item["_metaData"]["hidden"].as_bool() == Some(true) {
            continue;
        }
        let Some(name) = item["_name"].as_str().or_else(|| item["name"].as_str()) else {
            continue;
        };
        let Some(urls) = item["_urls"].as_array().or_else(|| item["urls"].as_array()) else {
            continue;
        };
        for url in urls {
            if url["type"] != "text/html" || url["method"].as_str().is_some_and(|s| s != "GET") {
                continue;
            }
            let Some(template) = url["template"].as_str() else {
                continue;
            };
            let mut template = template.to_owned();
            if let Some(params) = url["params"].as_array() {
                for param in params {
                    if let (Some(name), Some(value)) =
                        (param["name"].as_str(), param["value"].as_str())
                    {
                        let encode = |s: &str| {
                            url::form_urlencoded::byte_serialize(s.as_bytes())
                                .collect::<String>()
                                .replace("%7BsearchTerms%7D", "{searchTerms}")
                                .replace("%7BinputEncoding%7D", "{inputEncoding}")
                        };
                        template.push(if template.contains('?') { '&' } else { '?' });
                        template.push_str(&encode(name));
                        template.push('=');
                        template.push_str(&encode(value));
                    }
                }
            }
            if let Some(engine) = engine(name, &template) {
                out.push(engine);
                break;
            }
        }
    }
    Ok(out)
}
pub(crate) fn prepare(source: Source, cache: &Path) -> Result<Preview, String> {
    let profile = Profile::open(&source.path, cache)?;
    let mut preview = Preview {
        source: source.clone(),
        bookmarks: Vec::new(),
        history: Vec::new(),
        engines: Vec::new(),
        notes: Vec::new(),
    };
    let mut record = |label: &str, error: String| preview.notes.push(format!("{label}: {error}"));
    match source.family {
        Family::Firefox => {
            match profile.database("places.sqlite") {
                Ok(db) => {
                    match read_firefox_bookmarks(&db.connection) {
                        Ok(items) => preview.bookmarks = items,
                        Err(error) => record("Bookmarks", error),
                    }
                    match history(&db.connection, source.family) {
                        Ok((items, truncated)) => {
                            preview.history = items;
                            if truncated {
                                record("History","Only the most recent 100,000 visits or 64 MiB of text fit in this import.".into());
                            }
                        }
                        Err(error) => record("History", error),
                    }
                }
                Err(error) => record("Bookmarks and history", error),
            }
            if profile.has("search.json.mozlz4") {
                match firefox_engines(&profile) {
                    Ok(items) => preview.engines = items,
                    Err(error) => record("Search engines", error),
                }
            }
        }
        Family::Chromium => {
            if profile.has("Bookmarks") {
                match read_chromium_bookmarks(&profile) {
                    Ok(items) => preview.bookmarks = items,
                    Err(error) => record("Bookmarks", error),
                }
            }
            if profile.has("History") {
                match profile
                    .database("History")
                    .and_then(|db| history(&db.connection, source.family))
                {
                    Ok((items, truncated)) => {
                        preview.history = items;
                        if truncated {
                            record("History","Only the most recent 100,000 visits or 64 MiB of text fit in this import.".into());
                        }
                    }
                    Err(error) => record("History", error),
                }
            }
            if profile.has("Web Data") {
                match profile
                    .database("Web Data")
                    .and_then(|db| chromium_engines(&db.connection))
                {
                    Ok(items) => preview.engines = items,
                    Err(error) => record("Search engines", error),
                }
            }
        }
    }
    if preview.bookmarks.is_empty() && preview.history.is_empty() && preview.engines.is_empty() {
        preview
            .notes
            .push("No supported data was found in this profile.".into());
    }
    Ok(preview)
}
