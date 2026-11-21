use super::*;
use rusqlite::Connection;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "enzim-import-test-{}-{}",
            std::process::id(),
            crate::browser::next_tab_key()
        ));
        enzui_security::secure_dir::ensure_private_dir(&path).expect("private fixture");
        Self(path)
    }
    fn dir(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(&path).expect("fixture directory");
        path
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn fingerprint(path: &Path) -> BTreeMap<String, Vec<u8>> {
    std::fs::read_dir(path)
        .expect("source files")
        .map(|e| {
            let e = e.expect("entry");
            (
                e.file_name().to_string_lossy().into_owned(),
                std::fs::read(e.path()).expect("source bytes"),
            )
        })
        .collect()
}
fn source(path: PathBuf, family: Family) -> Source {
    Source {
        browser: "Fixture".into(),
        name: "Research".into(),
        path,
        family,
    }
}
fn literal(bytes: &[u8]) -> Vec<u8> {
    let mut out = b"mozLz40\0".to_vec();
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.push((bytes.len().min(15) as u8) << 4);
    if bytes.len() >= 15 {
        let mut rest = bytes.len() - 15;
        while rest >= 255 {
            out.push(255);
            rest -= 255;
        }
        out.push(rest as u8);
    }
    out.extend_from_slice(bytes);
    out
}
#[test]
fn import_discovers_named_profiles_forks_and_manual_folders_without_duplicates() {
    let scratch = Scratch::new();
    let home = scratch.dir("home");
    let config = home.join("custom-config");
    let firefox = home.join(".mozilla/firefox");
    std::fs::create_dir_all(firefox.join("random.default")).expect("Firefox");
    std::fs::write(firefox.join("random.default/places.sqlite"), []).expect("places");
    std::fs::write(
        firefox.join("profiles.ini"),
        "[Profile0]\nName=Personal\nIsRelative=1\nPath=random.default\n",
    )
    .expect("INI");
    let chrome = config.join("google-chrome/Profile 2");
    std::fs::create_dir_all(&chrome).expect("Chrome");
    std::fs::write(chrome.join("Bookmarks"), b"{}").expect("bookmarks");
    std::fs::write(
        config.join("google-chrome/Local State"),
        br#"{"profile":{"info_cache":{"Profile 2":{"name":"Work"}}}}"#,
    )
    .expect("profile names");
    let zen = home.join(".var/app/app.zen_browser.zen/.zen/z.default");
    std::fs::create_dir_all(&zen).expect("Zen");
    std::fs::write(zen.join("places.sqlite"), []).expect("places");
    let found = discover(&home, &config);
    assert_eq!(found.len(), 3);
    assert!(
        found
            .iter()
            .any(|s| s.browser == "Firefox" && s.name == "Personal")
    );
    assert!(
        found
            .iter()
            .any(|s| s.browser == "Chrome" && s.name == "Work")
    );
    assert!(found.iter().any(|s| s.browser == "Zen (Flatpak)"));
    assert_eq!(
        from_folder(&chrome).expect("manual Chromium")[0].family,
        Family::Chromium
    );
    assert_eq!(from_folder(&firefox).expect("Firefox root").len(), 1);
    assert!(from_folder(&scratch.dir("unrelated")).is_err());
}
#[test]
fn import_chromium_snapshot_reads_committed_wal_epochs_folders_and_engines_without_source_writes() {
    let scratch = Scratch::new();
    let profile = scratch.dir("chromium");
    std::fs::write(profile.join("Bookmarks"),br#"{"roots":{"bookmark_bar":{"type":"folder","name":"Bookmarks bar","children":[{"type":"folder","name":"Work","children":[{"type":"url","name":"Documentation","url":"https://example.org/docs"},{"type":"url","name":"Unsafe","url":"javascript:alert(1)"}]}]},"other":{"type":"folder","name":"Other","children":[]}}}"#).expect("bookmarks");
    let history = Connection::open(profile.join("History")).expect("history");
    history.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; CREATE TABLE urls(id INTEGER PRIMARY KEY,url TEXT,title TEXT,hidden INTEGER); CREATE TABLE visits(id INTEGER PRIMARY KEY,url INTEGER,visit_time INTEGER,transition INTEGER);
    INSERT INTO urls VALUES(1,'https://example.org/docs','Documentation',0),(2,'https://hidden.example','Hidden',1),(3,'chrome://settings','Internal',0);
    INSERT INTO visits VALUES(1,1,13344473600123000,536870913),(2,1,13344473600456000,536870920),(3,2,13344473600123000,536870913),(4,1,13344473600123000,536870915),(5,3,13344473600123000,536870913);").expect("WAL visits");
    let web = Connection::open(profile.join("Web Data")).expect("engines");
    web.execute_batch("CREATE TABLE keywords(short_name TEXT,url TEXT,search_url_post_params TEXT DEFAULT ''); INSERT INTO keywords(short_name,url) VALUES('Custom','https://search.example/?q={searchTerms}'),('Unsafe','http://search.example/?q={searchTerms}'),('Dynamic','{google:baseURL}search?q={searchTerms}');INSERT INTO keywords VALUES('POST engine','https://post.example/?q={searchTerms}','body=required');").expect("engine rows");
    let before = fingerprint(&profile);
    assert!(before.contains_key("History-wal"));
    let data = prepare(
        source(profile.clone(), Family::Chromium),
        &scratch.dir("snapshots"),
    )
    .expect("preview");
    assert_eq!(data.bookmarks.len(), 1);
    assert_eq!(data.bookmarks[0].folders, ["Bookmarks bar", "Work"]);
    assert_eq!(
        data.history
            .iter()
            .map(|v| v.visited_at)
            .collect::<Vec<_>>(),
        [1_700_000_000_456, 1_700_000_000_123]
    );
    assert!(data.history.iter().all(|v| v.container.is_none()));
    assert_eq!(data.engines.len(), 1);
    assert_eq!(
        data.engines[0].template,
        "https://search.example/?q={query}"
    );
    assert!(data.notes.is_empty(), "{:?}", data.notes);
    assert_eq!(fingerprint(&profile), before);
    assert_eq!(
        std::fs::read_dir(scratch.0.join("snapshots"))
            .expect("cleanup")
            .count(),
        0
    );
}
#[test]
fn import_firefox_keeps_folder_ancestry_dates_and_get_search_parameters() {
    let scratch = Scratch::new();
    let profile = scratch.dir("firefox");
    let db = Connection::open(profile.join("places.sqlite")).expect("places");
    db.execute_batch("CREATE TABLE moz_places(id INTEGER PRIMARY KEY,url TEXT,title TEXT); CREATE TABLE moz_bookmarks(id INTEGER PRIMARY KEY,parent INTEGER,type INTEGER,title TEXT,fk INTEGER,position INTEGER,guid TEXT DEFAULT '');CREATE TABLE moz_historyvisits(id INTEGER PRIMARY KEY,place_id INTEGER,visit_date INTEGER,visit_type INTEGER);
    INSERT INTO moz_places VALUES(1,'https://example.org/guide','Guide'),(2,'about:config','Internal');
    INSERT INTO moz_bookmarks(id,parent,type,title,fk,position) VALUES(1,0,2,'root',NULL,0),(2,1,2,'Bookmarks toolbar',NULL,0),(3,2,2,'Research',NULL,0),(4,3,1,'Guide',1,0),(5,3,1,'Internal',2,1); INSERT INTO moz_bookmarks VALUES(6,1,2,'Tags',NULL,1,'tags________'),(7,6,2,'Work tag',NULL,0,'tag-folder'),(8,7,1,'Tagged page',1,0,'tag-bookmark');
    INSERT INTO moz_historyvisits VALUES(1,1,1700000000123000,1),(2,1,1700000000456000,4),(3,2,1700000000123000,1);").expect("Firefox rows");
    let search=br#"{"engines":[{"_name":"Custom","_urls":[{"type":"text/html","method":"GET","template":"https://search.example/find","params":[{"name":"q","value":"{searchTerms}"},{"name":"src","value":"a&b"}]}]}]}"#;
    std::fs::write(profile.join("search.json.mozlz4"), literal(search)).expect("search JSON");
    let before = fingerprint(&profile);
    let data = prepare(
        source(profile.clone(), Family::Firefox),
        &scratch.dir("snapshots"),
    )
    .expect("preview");
    assert_eq!(data.bookmarks.len(), 1);
    assert_eq!(data.bookmarks[0].folders, ["Bookmarks toolbar", "Research"]);
    assert_eq!(data.history.len(), 1);
    assert_eq!(data.history[0].visited_at, 1_700_000_000_123);
    assert_eq!(data.engines.len(), 1);
    assert_eq!(
        data.engines[0].template,
        "https://search.example/find?q={query}&src=a%26b"
    );
    assert_eq!(fingerprint(&profile), before);
    db.execute("UPDATE moz_bookmarks SET parent=3 WHERE id=2", [])
        .expect("cycle");
    let invalid = prepare(
        source(profile, Family::Firefox),
        &scratch.0.join("snapshots"),
    )
    .expect("category failure");
    assert!(invalid.bookmarks.is_empty());
    assert_eq!(invalid.history.len(), 1);
    assert!(invalid.notes.iter().any(|s| s.contains("cycle")));
}
#[test]
fn import_rejects_symlinked_databases_and_bounded_lz4_failures() {
    use std::os::unix::fs::symlink;
    let scratch = Scratch::new();
    let profile = scratch.dir("profile");
    let target = scratch.0.join("outside");
    std::fs::write(&target, b"not a database").expect("target");
    symlink(&target, profile.join("History")).expect("symlink");
    let data =
        prepare(source(profile, Family::Chromium), &scratch.dir("snapshots")).expect("preview");
    assert!(data.history.is_empty());
    assert_eq!(
        std::fs::read(&target).expect("unchanged"),
        b"not a database"
    );
    let input = b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    assert_eq!(
        super::read::decompress_moz(&literal(input)).expect("literals"),
        input
    );
    let mut back = b"mozLz40\0".to_vec();
    back.extend(15u32.to_le_bytes());
    back.extend([
        0x33, b'a', b'b', b'c', 3, 0, 0x50, b'1', b'2', b'3', b'4', b'5',
    ]);
    assert_eq!(
        super::read::decompress_moz(&back).expect("overlapping back reference"),
        b"abcabcabca12345"
    );
    for invalid in [
        b"bad".to_vec(),
        b"mozLz40\0\xff\xff\xff\x7f".to_vec(),
        b"mozLz40\0\x04\0\0\0\0\0\0".to_vec(),
        b"mozLz40\0\x04\0\0\0\x40x".to_vec(),
    ] {
        assert!(super::read::decompress_moz(&invalid).is_err());
    }
}
