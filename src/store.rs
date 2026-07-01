//! Local SQLite cache for offline-first reads (TASK-41).
//!
//! roses persists fetched feeds/entries and their read state in a SQLite
//! database under the XDG data dir, so the TUI can paint instantly from cache on
//! launch and stay readable offline. **Feedbin remains the source of truth** for
//! read/unread: the cache is reconciled after every successful network load.
//!
//! The store is blocking (matching the app's `spawn_blocking` model) and is
//! owned and used only on the main UI thread — all cache writes happen in the
//! message-drain, so the `Connection` never crosses threads. Schema and sync
//! strategy are documented in `docs/persistence.md`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, Transaction};

use crate::feedbin::{Entry, Validators};

const APP_NAME: &str = "roses";
const DB_FILE: &str = "roses.db";
/// Bump when the schema changes incompatibly (drives future migrations).
const SCHEMA_VERSION: i64 = 1;

/// A cached snapshot for the initial paint: unread entries (newest-first) plus
/// feed names and the cached unread count.
pub struct CachedSnapshot {
    pub entries: Vec<Entry>,
    pub feed_titles: HashMap<i64, String>,
    pub total_unread: usize,
}

/// The on-disk cache. Cheap to construct; not `Send` (single-threaded by design).
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open (creating if needed) the cache under the XDG data dir and migrate it.
    pub fn open() -> Result<Store> {
        let path = db_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        Store::open_at(&path)
    }

    /// Open a store at an explicit path (used by tests with a temp file).
    pub fn open_at(path: &Path) -> Result<Store> {
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        let store = Store { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        // `starred` is present from v1 so TASK-29 (star/unstar) slots in without a
        // migration; only read/unread is wired today.
        self.conn
            .execute_batch(
                "PRAGMA journal_mode = WAL;
                 CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 CREATE TABLE IF NOT EXISTS feeds (feed_id INTEGER PRIMARY KEY, title TEXT);
                 CREATE TABLE IF NOT EXISTS entries (
                     id        INTEGER PRIMARY KEY,
                     feed_id   INTEGER NOT NULL,
                     published TEXT,
                     unread    INTEGER NOT NULL DEFAULT 1,
                     starred   INTEGER NOT NULL DEFAULT 0,
                     json      TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS idx_entries_unread ON entries(unread);",
            )
            .context("creating the cache schema")?;
        self.conn
            .execute(
                "INSERT INTO meta (key, value) VALUES ('schema_version', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [SCHEMA_VERSION.to_string()],
            )
            .context("recording the schema version")?;
        Ok(())
    }

    /// Load the newest cached unread entries for the initial paint, plus feed
    /// names and the cached unread count.
    pub fn load_unread(&self, limit: usize) -> Result<CachedSnapshot> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT json FROM entries
                 WHERE unread = 1
                 ORDER BY published DESC
                 LIMIT ?1",
            )
            .context("preparing the cached-unread query")?;
        let rows = stmt
            .query_map([limit as i64], |row| row.get::<_, String>(0))
            .context("querying cached unread entries")?;
        let mut entries = Vec::new();
        for json in rows {
            let json = json.context("reading a cached entry row")?;
            // Tolerate a stale/incompatible row rather than failing the paint.
            if let Ok(entry) = serde_json::from_str::<Entry>(&json) {
                entries.push(entry);
            }
        }
        let total_unread: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM entries WHERE unread = 1", [], |r| {
                r.get(0)
            })
            .context("counting cached unread")?;
        Ok(CachedSnapshot {
            entries,
            feed_titles: self.feed_titles()?,
            total_unread: total_unread as usize,
        })
    }

    fn feed_titles(&self) -> Result<HashMap<i64, String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT feed_id, title FROM feeds WHERE title IS NOT NULL")
            .context("preparing the feed-titles query")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .context("querying feed titles")?;
        let mut map = HashMap::new();
        for row in rows {
            let (id, title) = row.context("reading a feed row")?;
            map.insert(id, title);
        }
        Ok(map)
    }

    /// Reconcile the cache with a fresh network snapshot: upsert feeds + the
    /// hydrated entries (as unread), and mark every cached entry no longer in the
    /// unread set as read. Feedbin is the source of truth for read state.
    pub fn replace_snapshot(
        &mut self,
        entries: &[Entry],
        feed_titles: &HashMap<i64, String>,
        unread_ids: &[i64],
    ) -> Result<()> {
        let unread_set: HashSet<i64> = unread_ids.iter().copied().collect();
        let tx = self
            .conn
            .transaction()
            .context("beginning a cache transaction")?;
        {
            let mut feed = tx
                .prepare(
                    "INSERT INTO feeds (feed_id, title) VALUES (?1, ?2)
                     ON CONFLICT(feed_id) DO UPDATE SET title = excluded.title",
                )
                .context("preparing the feed upsert")?;
            for (&feed_id, title) in feed_titles {
                feed.execute(rusqlite::params![feed_id, title])
                    .context("upserting a feed")?;
            }
        }
        upsert_entries_tx(&tx, entries)?;
        // Mark reads among the (bounded) cached-unread rows that fell out of the
        // fresh unread set — no need to materialize the whole server list.
        let cached_unread: Vec<i64> = {
            let mut stmt = tx
                .prepare("SELECT id FROM entries WHERE unread = 1")
                .context("preparing the cached-unread scan")?;
            let ids = stmt
                .query_map([], |r| r.get::<_, i64>(0))
                .context("scanning cached unread ids")?;
            ids.collect::<rusqlite::Result<Vec<i64>>>()
                .context("collecting cached unread ids")?
        };
        {
            let mut mark = tx
                .prepare("UPDATE entries SET unread = 0 WHERE id = ?1")
                .context("preparing the mark-read update")?;
            for id in cached_unread {
                if !unread_set.contains(&id) {
                    mark.execute([id]).context("marking a cached entry read")?;
                }
            }
        }
        tx.commit().context("committing the cache snapshot")?;
        Ok(())
    }

    /// Upsert a batch of hydrated entries (as unread) — used by lazy load-more so
    /// later-loaded entries are also cached for offline reading.
    pub fn upsert_entries(&mut self, entries: &[Entry]) -> Result<()> {
        let tx = self
            .conn
            .transaction()
            .context("beginning a cache transaction")?;
        upsert_entries_tx(&tx, entries)?;
        tx.commit().context("committing entries")?;
        Ok(())
    }

    /// Write-through a read/unread change for one entry (mark-read / undo).
    pub fn set_unread(&self, id: i64, unread: bool) -> Result<()> {
        self.conn
            .execute(
                "UPDATE entries SET unread = ?2 WHERE id = ?1",
                rusqlite::params![id, unread as i64],
            )
            .context("updating cached read state")?;
        Ok(())
    }

    /// Read the stored HTTP validators for `endpoint` (TASK-42). Missing values
    /// come back as `None`; a read error degrades to empty (best-effort cache).
    pub fn get_validators(&self, endpoint: &str) -> Validators {
        Validators {
            etag: self.get_meta(&format!("{endpoint}.etag")),
            last_modified: self.get_meta(&format!("{endpoint}.last_modified")),
        }
    }

    /// Persist the HTTP validators for `endpoint` so the next request can send
    /// them as `If-None-Match` / `If-Modified-Since`.
    pub fn set_validators(&self, endpoint: &str, v: &Validators) -> Result<()> {
        self.set_meta(&format!("{endpoint}.etag"), v.etag.as_deref())?;
        self.set_meta(
            &format!("{endpoint}.last_modified"),
            v.last_modified.as_deref(),
        )?;
        Ok(())
    }

    fn get_meta(&self, key: &str) -> Option<String> {
        self.conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| {
                r.get::<_, String>(0)
            })
            .optional()
            .ok()
            .flatten()
    }

    fn set_meta(&self, key: &str, value: Option<&str>) -> Result<()> {
        match value {
            Some(v) => self
                .conn
                .execute(
                    "INSERT INTO meta (key, value) VALUES (?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    rusqlite::params![key, v],
                )
                .map(|_| ())
                .context("writing a meta value")?,
            None => self
                .conn
                .execute("DELETE FROM meta WHERE key = ?1", [key])
                .map(|_| ())
                .context("clearing a meta value")?,
        }
        Ok(())
    }
}

/// Upsert entries within a transaction, preserving the existing `starred` flag
/// (left untouched by the `ON CONFLICT` clause) so TASK-29 stays forward-safe.
fn upsert_entries_tx(tx: &Transaction, entries: &[Entry]) -> Result<()> {
    let mut stmt = tx
        .prepare(
            "INSERT INTO entries (id, feed_id, published, unread, json)
             VALUES (?1, ?2, ?3, 1, ?4)
             ON CONFLICT(id) DO UPDATE SET
                 feed_id   = excluded.feed_id,
                 published = excluded.published,
                 unread    = 1,
                 json      = excluded.json",
        )
        .context("preparing the entry upsert")?;
    for entry in entries {
        let json = serde_json::to_string(entry).context("serializing an entry")?;
        stmt.execute(rusqlite::params![
            entry.id,
            entry.feed_id,
            entry.published,
            json
        ])
        .context("upserting an entry")?;
    }
    Ok(())
}

fn db_path() -> Result<PathBuf> {
    Ok(data_dir()?.join(DB_FILE))
}

/// XDG data dir: `$XDG_DATA_HOME/roses` if set and non-empty, else
/// `~/.local/share/roses` (honored on macOS too, matching `config.rs`'s style).
fn data_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("could not determine the home directory")?;
    let base = match std::env::var_os("XDG_DATA_HOME") {
        Some(x) if !x.is_empty() => PathBuf::from(x),
        _ => home.join(".local").join("share"),
    };
    Ok(base.join(APP_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: i64, feed_id: i64, published: &str) -> Entry {
        Entry {
            id,
            feed_id,
            title: Some(format!("E{id}")),
            url: None,
            author: None,
            published: Some(published.to_string()),
            summary: None,
            content: None,
            images: None,
            enclosure: None,
            json_feed: None,
        }
    }

    fn mem_store() -> Store {
        let store = Store {
            conn: Connection::open_in_memory().unwrap(),
        };
        store.migrate().unwrap();
        store
    }

    #[test]
    fn round_trips_entries_feeds_and_unread_count() {
        let mut s = mem_store();
        let mut feeds = HashMap::new();
        feeds.insert(7, "Feed Seven".to_string());
        s.replace_snapshot(
            &[entry(2, 7, "2026-01-02"), entry(1, 7, "2026-01-01")],
            &feeds,
            &[2, 1],
        )
        .unwrap();

        let snap = s.load_unread(50).unwrap();
        assert_eq!(snap.entries.len(), 2);
        assert_eq!(snap.entries[0].id, 2, "newest-first by published");
        assert_eq!(snap.total_unread, 2);
        assert_eq!(
            snap.feed_titles.get(&7).map(String::as_str),
            Some("Feed Seven")
        );
    }

    #[test]
    fn reconcile_marks_absent_entries_read() {
        let mut s = mem_store();
        let feeds = HashMap::new();
        s.replace_snapshot(
            &[entry(2, 7, "2026-01-02"), entry(1, 7, "2026-01-01")],
            &feeds,
            &[2, 1],
        )
        .unwrap();
        assert_eq!(s.load_unread(50).unwrap().entries.len(), 2);

        // A later reconcile lists only id 2 as unread → id 1 becomes read.
        s.replace_snapshot(&[entry(2, 7, "2026-01-02")], &feeds, &[2])
            .unwrap();
        let snap = s.load_unread(50).unwrap();
        assert_eq!(snap.entries.len(), 1);
        assert_eq!(snap.entries[0].id, 2);
        assert_eq!(snap.total_unread, 1);
    }

    #[test]
    fn set_unread_write_through_persists() {
        let mut s = mem_store();
        s.replace_snapshot(&[entry(1, 7, "2026-01-01")], &HashMap::new(), &[1])
            .unwrap();
        s.set_unread(1, false).unwrap();
        assert_eq!(
            s.load_unread(50).unwrap().entries.len(),
            0,
            "a marked-read entry is hidden from the unread view"
        );
        s.set_unread(1, true).unwrap();
        assert_eq!(
            s.load_unread(50).unwrap().entries.len(),
            1,
            "undo restores it to unread"
        );
    }

    #[test]
    fn upsert_entries_appends_later_batches() {
        let mut s = mem_store();
        s.replace_snapshot(&[entry(3, 7, "2026-01-03")], &HashMap::new(), &[3, 2, 1])
            .unwrap();
        // A lazy load-more hydrates the two older ids.
        s.upsert_entries(&[entry(2, 7, "2026-01-02"), entry(1, 7, "2026-01-01")])
            .unwrap();
        assert_eq!(s.load_unread(50).unwrap().entries.len(), 3);
    }

    #[test]
    fn reopening_the_same_file_keeps_data() {
        let dir = std::env::temp_dir().join(format!("roses-store-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.db");
        {
            let mut s = Store::open_at(&path).unwrap();
            s.replace_snapshot(&[entry(1, 7, "2026-01-01")], &HashMap::new(), &[1])
                .unwrap();
        }
        let s = Store::open_at(&path).unwrap();
        assert_eq!(s.load_unread(50).unwrap().entries.len(), 1, "persisted");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn validators_round_trip_in_meta() {
        let s = mem_store();
        assert!(
            s.get_validators("unread").etag.is_none(),
            "no validators initially"
        );
        s.set_validators(
            "unread",
            &Validators {
                etag: Some("\"e1\"".to_string()),
                last_modified: Some("Sat, 02 Feb 2013 15:20:46 GMT".to_string()),
            },
        )
        .unwrap();
        let v = s.get_validators("unread");
        assert_eq!(v.etag.as_deref(), Some("\"e1\""));
        assert_eq!(
            v.last_modified.as_deref(),
            Some("Sat, 02 Feb 2013 15:20:46 GMT")
        );
        // Clearing (None) removes them.
        s.set_validators("unread", &Validators::default()).unwrap();
        assert!(s.get_validators("unread").etag.is_none());
    }
}
