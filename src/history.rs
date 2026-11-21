use chrono::{Local, TimeZone};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Visit {
    pub(crate) id: i64,
    pub(crate) url: String,
    pub(crate) title: String,
    pub(crate) visited_at: i64,
    pub(crate) container: Option<u64>,
}

impl Visit {
    pub(crate) fn date_label(&self) -> String {
        if self.visited_at == 0 {
            return "Imported history".into();
        }
        Local
            .timestamp_millis_opt(self.visited_at)
            .single()
            .map_or_else(
                || "Unknown date".into(),
                |d| d.format("%A, %-d %B %Y").to_string(),
            )
    }
    pub(crate) fn time_label(&self) -> String {
        Local
            .timestamp_millis_opt(self.visited_at)
            .single()
            .map_or_else(String::new, |d| d.format("%H:%M").to_string())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Range {
    Hour,
    Day,
    Week,
    Month,
    #[default]
    All,
}

impl Range {
    pub(crate) const OPTIONS: [Self; 5] =
        [Self::Hour, Self::Day, Self::Week, Self::Month, Self::All];
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Hour => "Last hour",
            Self::Day => "Last 24 hours",
            Self::Week => "Last 7 days",
            Self::Month => "Last 30 days",
            Self::All => "All time",
        }
    }
    pub(crate) fn start(self, now: i64) -> i64 {
        let seconds: i64 = match self {
            Self::Hour => 3600,
            Self::Day => 86400,
            Self::Week => 604800,
            Self::Month => 2592000,
            Self::All => return 0,
        };
        now.saturating_sub(seconds * 1000)
    }
}

pub(crate) struct Store {
    db: Result<Connection, String>,
}

#[derive(Debug)]
pub(crate) struct RecordedVisit {
    id: i64,
    url: String,
    title: String,
}

impl Store {
    pub(crate) fn open(path: Option<&Path>) -> Self {
        let db = (|| -> Result<Connection, String> {
            let db = match path {
                Some(path) => {
                    let parent = path.parent().ok_or("The history path has no parent.")?;
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
                        .map_err(|e| e.to_string())?;
                    if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
                        return Err("The history database must not be a symbolic link.".into());
                    }
                    let db = Connection::open(path).map_err(|e| e.to_string())?;
                    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                        .map_err(|e| e.to_string())?;
                    db
                }
                None => Connection::open_in_memory().map_err(|e| e.to_string())?,
            };
            db.busy_timeout(std::time::Duration::from_secs(3))
                .map_err(|e| e.to_string())?;
            db.execute_batch(
                "PRAGMA journal_mode=WAL;
                CREATE TABLE IF NOT EXISTS visits (
                    id INTEGER PRIMARY KEY, url TEXT NOT NULL, title TEXT NOT NULL,
                    visited_at INTEGER NOT NULL, container TEXT, search TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS visits_time ON visits(visited_at DESC);
                CREATE TABLE IF NOT EXISTS metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
            )
            .map_err(|e| e.to_string())?;
            Ok(db)
        })();
        Self { db }
    }
    fn connection(&self) -> Result<&Connection, String> {
        self.db.as_ref().map_err(Clone::clone)
    }
    pub(crate) fn visited_domains(
        &self,
        start: i64,
        container: Option<u64>,
    ) -> Result<std::collections::BTreeSet<String>, String> {
        let mut statement = self
            .connection()?
            .prepare("SELECT DISTINCT url FROM visits WHERE visited_at>=?1 AND container IS ?2")
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map(
                params![start, container.map(|c| format!("{c:016x}"))],
                |row| row.get::<_, String>(0),
            )
            .map_err(|e| e.to_string())?;
        let mut domains = std::collections::BTreeSet::new();
        for row in rows {
            if let Ok(url) = url::Url::parse(&row.map_err(|e| e.to_string())?)
                && let Some(host) = url.host_str()
            {
                domains.insert(host.to_ascii_lowercase());
            }
        }
        Ok(domains)
    }
    pub(crate) fn record(&self, visit: &Visit) -> Result<RecordedVisit, String> {
        let db = self.connection()?;
        db.execute(
            "INSERT INTO visits(url,title,visited_at,container,search) VALUES(?1,?2,?3,?4,?5)",
            params![
                visit.url,
                visit.title,
                visit.visited_at,
                visit.container.map(|c| format!("{c:016x}")),
                format!("{} {}", visit.title, visit.url).to_lowercase()
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(RecordedVisit {
            id: db.last_insert_rowid(),
            url: visit.url.clone(),
            title: visit.title.clone(),
        })
    }
    pub(crate) fn refresh_title(
        &self,
        recorded: &mut Option<RecordedVisit>,
        url: &str,
        title: &str,
    ) -> Result<bool, String> {
        let Some(recorded) = recorded else {
            return Ok(false);
        };
        if recorded.url != url || title.is_empty() || recorded.title == title {
            return Ok(false);
        }
        let changed = self
            .connection()?
            .execute(
                "UPDATE visits SET title=?1,search=?2 WHERE id=?3 AND url=?4",
                params![
                    title,
                    format!("{title} {url}").to_lowercase(),
                    recorded.id,
                    url
                ],
            )
            .map_err(|error| error.to_string())?;
        title.clone_into(&mut recorded.title);
        Ok(changed > 0)
    }
    pub(crate) fn migrate(&mut self, legacy: &[Visit]) -> Result<(), String> {
        let db = self.db.as_mut().map_err(|e| e.clone())?;
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        let done = tx
            .query_row(
                "SELECT COUNT(*) FROM metadata WHERE key='legacy-imported'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map_err(|e| e.to_string())?;
        if done != 0 {
            return Ok(());
        }
        for visit in legacy {
            tx.execute(
                "INSERT INTO visits(url,title,visited_at,container,search) VALUES(?1,?2,?3,?4,?5)",
                params![
                    visit.url,
                    visit.title,
                    visit.visited_at,
                    visit.container.map(|c| format!("{c:016x}")),
                    format!("{} {}", visit.title, visit.url).to_lowercase()
                ],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.execute(
            "INSERT OR IGNORE INTO metadata(key,value) VALUES('legacy-imported','1')",
            [],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }
    pub(crate) fn import_visits(&mut self, visits: &[Visit]) -> Result<usize, String> {
        let db = self.db.as_mut().map_err(|error| error.clone())?;
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        tx.execute(
            "CREATE INDEX IF NOT EXISTS visits_import_identity ON visits(url,visited_at,container)",
            [],
        )
        .map_err(|error| error.to_string())?;
        let mut inserted = 0;
        {
            let mut statement = tx.prepare("INSERT INTO visits(url,title,visited_at,container,search) SELECT ?1,?2,?3,NULL,?4 WHERE NOT EXISTS(SELECT 1 FROM visits WHERE url=?1 AND visited_at=?3 AND container IS NULL)").map_err(|error|error.to_string())?;
            for visit in visits {
                let Some(url) = crate::imports::web_url(&visit.url) else {
                    continue;
                };
                if visit.visited_at <= 0 {
                    continue;
                }
                inserted += statement
                    .execute(params![
                        url,
                        visit.title,
                        visit.visited_at,
                        format!("{} {}", visit.title, url).to_lowercase()
                    ])
                    .map_err(|error| error.to_string())?;
            }
        }
        tx.commit().map_err(|error| error.to_string())?;
        Ok(inserted)
    }
    pub(crate) fn count(&self, filter: &str, start: i64) -> Result<usize, String> {
        self.connection()?
            .query_row(
                "SELECT COUNT(*) FROM visits WHERE instr(search,?1)>0 AND visited_at>=?2",
                params![filter.trim().to_lowercase(), start],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())
    }
    pub(crate) fn query(&self, filter: &str, start: i64) -> Result<Vec<Visit>, String> {
        self.page(filter, start, 10000, 0)
    }
    pub(crate) fn page(
        &self,
        filter: &str,
        start: i64,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Visit>, String> {
        let mut stmt=self.connection()?.prepare("SELECT id,url,title,visited_at,container FROM visits WHERE instr(search,?1)>0 AND visited_at>=?2 ORDER BY visited_at DESC,id DESC LIMIT ?3 OFFSET ?4").map_err(|e|e.to_string())?;
        stmt.query_map(
            params![
                filter.trim().to_lowercase(),
                start,
                limit.min(10000),
                offset
            ],
            |r| {
                let c: Option<String> = r.get(4)?;
                Ok(Visit {
                    id: r.get(0)?,
                    url: r.get(1)?,
                    title: r.get(2)?,
                    visited_at: r.get(3)?,
                    container: c.and_then(|s| u64::from_str_radix(&s, 16).ok()),
                })
            },
        )
        .map_err(|e| e.to_string())?
        .map(|r| r.map_err(|e| e.to_string()))
        .collect()
    }
    pub(crate) fn delete(&self, id: i64) -> Result<usize, String> {
        self.connection()?
            .execute("DELETE FROM visits WHERE id=?1", [id])
            .map_err(|e| e.to_string())
    }
    pub(crate) fn clear(&self, filter: &str, start: i64) -> Result<usize, String> {
        self.connection()?
            .execute(
                "DELETE FROM visits WHERE instr(search,?1)>0 AND visited_at>=?2",
                params![filter.trim().to_lowercase(), start],
            )
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn visit(title: &str, url: &str, at: i64, container: Option<u64>) -> Visit {
        Visit {
            id: 0,
            title: title.into(),
            url: url.into(),
            visited_at: at,
            container,
        }
    }
    #[test]
    fn imported_history_is_idempotent_profile_scoped_and_rolls_back_on_failure() {
        let mut store = Store::open(None);
        let first = visit(
            "Existing title",
            "https://example.org/",
            1_700_000_000_123,
            None,
        );
        store.record(&first).expect("existing visit");
        store
            .record(&visit(
                "Container visit",
                "https://example.org/new",
                1_700_000_000_456,
                Some(7),
            ))
            .expect("container");
        let rows = [
            visit(
                "Replacement",
                "https://example.org/",
                first.visited_at,
                None,
            ),
            visit(
                "Imported",
                "https://example.org/new",
                1_700_000_000_456,
                None,
            ),
        ];
        assert_eq!(store.import_visits(&rows).expect("import"), 1);
        assert_eq!(store.import_visits(&rows).expect("repeat"), 0);
        assert_eq!(store.count("", 0).expect("count"), 3);
        assert_eq!(store.count("Existing title", 0).expect("preserve title"), 1);
        store.connection().expect("database").execute_batch("CREATE TRIGGER reject_import BEFORE INSERT ON visits WHEN NEW.title='Reject' BEGIN SELECT RAISE(ABORT,'deliberate import refusal'); END;").expect("failure trigger");
        let before = store.count("", 0).expect("before");
        assert!(
            store
                .import_visits(&[
                    visit(
                        "Would insert",
                        "https://example.org/first",
                        1_700_000_000_999,
                        None
                    ),
                    visit(
                        "Reject",
                        "https://example.org/second",
                        1_700_000_000_999,
                        None
                    )
                ])
                .is_err()
        );
        assert_eq!(store.count("", 0).expect("rollback"), before);
    }
    #[test]
    fn history_filters_delete_only_matching_dates_and_text() {
        let store = Store::open(None);
        let now = 1_800_000_000_000;
        store
            .record(&visit(
                "Rust today",
                "https://rust-lang.org/new",
                now - 1000,
                Some(u64::MAX),
            ))
            .expect("record");
        store
            .record(&visit(
                "Rust old",
                "https://rust-lang.org/old",
                now - 8 * 86400 * 1000,
                None,
            ))
            .expect("record");
        store
            .record(&visit(
                "Other today",
                "https://example.com",
                now - 1000,
                None,
            ))
            .expect("record");
        assert_eq!(store.query("rust", 0).expect("query").len(), 2);
        assert_eq!(
            store.clear("RUST", Range::Week.start(now)).expect("clear"),
            1
        );
        let remaining = store.query("", 0).expect("remaining");
        assert_eq!(remaining.len(), 2);
        assert!(remaining.iter().any(|v| v.title == "Rust old"));
        assert!(remaining.iter().any(|v| v.title == "Other today"));
    }
    #[test]
    fn history_literal_unicode_search_and_row_deletion_preserve_other_visits() {
        let store = Store::open(None);
        store
            .record(&visit(
                "ÄRGER 100%_literal",
                "https://example.com/a",
                100,
                Some(u64::MAX),
            ))
            .expect("record");
        store
            .record(&visit("Other", "https://example.com/b", 101, None))
            .expect("record");
        let hits = store.query("ärger 100%_", 0).expect("query");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].container, Some(u64::MAX));
        assert_eq!(store.delete(hits[0].id).expect("delete"), 1);
        assert_eq!(
            store.query("", 0).expect("query")[0].url,
            "https://example.com/b"
        );
    }
    #[test]
    fn history_migration_is_once_only_and_shared_connections_see_changes() {
        let root = std::env::temp_dir().join(format!(
            "enzim-history-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let path = root.join("history.sqlite");
        {
            let mut a = Store::open(Some(&path));
            let imported = vec![visit("Imported", "https://example.com", 0, None)];
            a.migrate(&imported).expect("migration");
            a.migrate(&imported).expect("repeat migration");
            assert_eq!(a.query("", 0).expect("query").len(), 1);
            let b = Store::open(Some(&path));
            b.record(&visit("New visit", "https://rust-lang.org", 100, None))
                .expect("record");
            assert_eq!(a.query("", 0).expect("shared read").len(), 2);
            assert_eq!(a.clear("Imported", 0).expect("clear"), 1);
            assert_eq!(b.query("", 0).expect("shared read").len(), 1);
        }
        let c = Store::open(Some(&path));
        assert_eq!(c.query("", 0).expect("reopen")[0].title, "New visit");
        drop(c);
        std::fs::remove_dir_all(root).expect("cleanup");
    }
}
