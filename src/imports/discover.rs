use super::{Family, Source};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
fn json(path: &Path) -> Option<serde_json::Value> {
    let file = std::fs::File::open(path).ok()?;
    if !file.metadata().ok()?.is_file() || file.metadata().ok()?.len() > 4 * 1024 * 1024 {
        return None;
    }
    serde_json::from_reader(std::io::Read::take(file, 4 * 1024 * 1024 + 1)).ok()
}
fn is_profile(path: &Path, family: Family) -> bool {
    match family {
        Family::Firefox => path.join("places.sqlite").is_file(),
        Family::Chromium => {
            path.join("Bookmarks").is_file()
                || path.join("History").is_file()
                || path.join("Web Data").is_file()
        }
    }
}
fn push(out: &mut Vec<Source>, path: PathBuf, browser: &str, name: String, family: Family) {
    if out.len() >= 256 || !is_profile(&path, family) {
        return;
    }
    if let Ok(path) = path.canonicalize() {
        out.push(Source {
            path,
            browser: browser.into(),
            name,
            family,
        });
    }
}
fn profiles(root: &Path, browser: &str, family: Family, out: &mut Vec<Source>) {
    if !root.is_dir() {
        return;
    }
    if family == Family::Firefox
        && let Ok(file) = std::fs::File::open(root.join("profiles.ini"))
    {
        use std::io::Read;
        let mut text = String::new();
        if file.take(256 * 1024).read_to_string(&mut text).is_ok() {
            let mut section = std::collections::BTreeMap::<String, String>::new();
            let emit = |section: &std::collections::BTreeMap<String, String>,
                        out: &mut Vec<Source>| {
                if let Some(value) = section.get("Path") {
                    let path = if section.get("IsRelative").is_none_or(|v| v != "0") {
                        root.join(value)
                    } else {
                        PathBuf::from(value)
                    };
                    push(
                        out,
                        path,
                        browser,
                        section
                            .get("Name")
                            .cloned()
                            .unwrap_or_else(|| "Default".into()),
                        family,
                    );
                }
            };
            for line in text.lines().chain(std::iter::once("[end]")) {
                let line = line.trim();
                if line.starts_with('[') {
                    emit(&section, out);
                    section.clear();
                } else if let Some((k, v)) = line.split_once('=') {
                    section.insert(k.trim().into(), v.trim().into());
                }
            }
        }
    }
    let local = json(&root.join("Local State")).unwrap_or_default();
    push(out, root.to_path_buf(), browser, "Default".into(), family);
    if let Ok(entries) = std::fs::read_dir(root) {
        let mut entries: Vec<_> = entries.take(512).filter_map(Result::ok).collect();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let id = entry.file_name().to_string_lossy().into_owned();
            let name = local["profile"]["info_cache"][&id]["name"]
                .as_str()
                .unwrap_or(&id)
                .chars()
                .take(120)
                .collect();
            push(out, entry.path(), browser, name, family);
        }
    }
}
pub(crate) fn from_folder(path: &Path) -> Result<Vec<Source>, String> {
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (family, browser) in [
        (Family::Firefox, "Firefox family"),
        (Family::Chromium, "Chromium family"),
    ] {
        profiles(&path, browser, family, &mut out);
    }
    dedup(&mut out);
    if out.is_empty() {
        Err("Choose a browser profile folder containing places.sqlite, Bookmarks, History or Web Data, or its parent profile directory.".into())
    } else {
        Ok(out)
    }
}
fn dedup(out: &mut Vec<Source>) {
    let mut seen = BTreeSet::new();
    out.retain(|source| seen.insert(source.path.clone()));
}
pub(crate) fn discover(home: &Path, config: &Path) -> Vec<Source> {
    let mut out = Vec::new();
    for (browser, relative) in [
        ("Firefox", "mozilla/firefox"),
        ("Zen", "zen"),
        ("LibreWolf", "librewolf"),
        ("Floorp", "floorp"),
    ] {
        profiles(&config.join(relative), browser, Family::Firefox, &mut out);
    }
    for (browser, relative) in [
        ("Firefox", ".mozilla/firefox"),
        ("Firefox", ".config/mozilla/firefox"),
        ("Firefox", "snap/firefox/common/.mozilla/firefox"),
        (
            "Firefox (Flatpak)",
            ".var/app/org.mozilla.firefox/.mozilla/firefox",
        ),
        ("Zen", ".zen"),
        ("Zen", ".config/zen"),
        ("Zen (Flatpak)", ".var/app/app.zen_browser.zen/.zen"),
        ("LibreWolf", ".librewolf"),
        ("LibreWolf", ".config/librewolf"),
        (
            "LibreWolf (Flatpak)",
            ".var/app/io.gitlab.librewolf-community/.librewolf",
        ),
        ("Waterfox", ".waterfox"),
        ("Floorp", ".floorp"),
        ("Floorp", ".config/floorp"),
        ("Floorp (Flatpak)", ".var/app/one.ablaze.floorp/.floorp"),
    ] {
        profiles(&home.join(relative), browser, Family::Firefox, &mut out);
    }
    for (browser, relative) in [
        ("Chrome", "google-chrome"),
        ("Chrome Beta", "google-chrome-beta"),
        ("Chrome Dev", "google-chrome-unstable"),
        ("Chromium", "chromium"),
        ("Brave", "BraveSoftware/Brave-Browser"),
        ("Brave Beta", "BraveSoftware/Brave-Browser-Beta"),
        ("Brave Nightly", "BraveSoftware/Brave-Browser-Nightly"),
        ("Edge", "microsoft-edge"),
        ("Edge Beta", "microsoft-edge-beta"),
        ("Edge Dev", "microsoft-edge-dev"),
        ("Vivaldi", "vivaldi"),
        ("Vivaldi Snapshot", "vivaldi-snapshot"),
        ("Opera", "opera"),
        ("Opera Developer", "opera-developer"),
        ("Thorium", "thorium"),
        ("Ungoogled Chromium", "ungoogled-chromium"),
    ] {
        profiles(&config.join(relative), browser, Family::Chromium, &mut out);
    }
    for (browser, relative) in [
        (
            "Chrome (Flatpak)",
            ".var/app/com.google.Chrome/config/google-chrome",
        ),
        (
            "Chromium (Flatpak)",
            ".var/app/org.chromium.Chromium/config/chromium",
        ),
        (
            "Brave (Flatpak)",
            ".var/app/com.brave.Browser/config/BraveSoftware/Brave-Browser",
        ),
        (
            "Edge (Flatpak)",
            ".var/app/com.microsoft.Edge/config/microsoft-edge",
        ),
        (
            "Vivaldi (Flatpak)",
            ".var/app/com.vivaldi.Vivaldi/config/vivaldi",
        ),
        ("Opera (Flatpak)", ".var/app/com.opera.Opera/config/opera"),
        ("Chromium (Snap)", "snap/chromium/common/chromium"),
    ] {
        profiles(&home.join(relative), browser, Family::Chromium, &mut out);
    }
    dedup(&mut out);
    out.sort_by_key(|s| (s.browser.clone(), s.name.clone()));
    out
}
