//! Search Index Database Manager with Transactional Writes & Recovery

use crate::error::{IndexError, Result};
use crate::frecency::{calculate_frecency, current_epoch_seconds};
use crate::schema::initialize_or_migrate;
use rusqlite::{params, Connection, OpenFlags};
use search_core::SearchResultCategory;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// A record representing historical application or file launch.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchHistoryItem {
    pub id: i64,
    pub query: String,
    pub target_key: String,
    pub title: String,
    pub category: String,
    pub use_count: u32,
    pub first_used_epoch: u64,
    pub last_used_epoch: u64,
    pub is_pinned: bool,
    pub frecency_score: f32,
}

/// Thread-safe SQLite index manager.
pub struct SearchDatabase {
    conn: Mutex<Connection>,
    db_path: Option<PathBuf>,
}

impl SearchDatabase {
    /// Opens or creates an SQLite index at the specified filesystem path with automatic recovery.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let p = path.as_ref().to_path_buf();
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let open_res = Self::try_open(&p);
        match open_res {
            Ok(db) => Ok(db),
            Err(e) => {
                tracing::warn!(
                    path = %p.display(),
                    error = %e,
                    "Database corrupted or unreadable. Commencing automatic quarantine and recovery..."
                );
                // Quarantine corrupt file
                let corrupt_backup =
                    p.with_extension(format!("corrupt.{}", current_epoch_seconds()));
                let _ = std::fs::rename(&p, &corrupt_backup);

                let fresh_db = Self::try_open(&p)?;
                tracing::info!(
                    quarantined = %corrupt_backup.display(),
                    "Fresh search database reinitialized successfully"
                );
                Ok(fresh_db)
            }
        }
    }

    fn try_open(path: &Path) -> Result<Self> {
        let mut conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
        )?;

        // Performance & durability settings
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA temp_store = MEMORY;",
        )?;

        initialize_or_migrate(&mut conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
            db_path: Some(path.to_path_buf()),
        })
    }

    /// Creates an in-memory database instance (ideal for isolated testing).
    pub fn open_in_memory() -> Result<Self> {
        let mut conn = Connection::open_in_memory()?;
        initialize_or_migrate(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            db_path: None,
        })
    }

    /// Records or updates application/file launch frecency.
    pub fn record_usage(
        &self,
        query: &str,
        target_key: &str,
        title: &str,
        category: &SearchResultCategory,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = current_epoch_seconds();
        let cat_str = format!("{:?}", category);

        conn.execute(
            "INSERT INTO search_history (query, target_key, title, category, use_count, first_used_epoch, last_used_epoch)
             VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5)
             ON CONFLICT(target_key) DO UPDATE SET
                 use_count = use_count + 1,
                 query = ?1,
                 title = ?3,
                 category = ?4,
                 last_used_epoch = ?5",
            params![query, target_key, title, cat_str, now],
        )?;

        Ok(())
    }

    /// Computes current frecency score for a target key if recorded.
    pub fn get_frecency_score(&self, target_key: &str) -> Result<Option<f32>> {
        let conn = self.conn.lock().unwrap();
        let now = current_epoch_seconds();

        let row = conn.query_row(
            "SELECT use_count, last_used_epoch, is_pinned FROM search_history WHERE target_key = ?1",
            params![target_key],
            |r| {
                let use_count: u32 = r.get(0)?;
                let last_used: u64 = r.get(1)?;
                let is_pinned: bool = r.get(2)?;
                Ok((use_count, last_used, is_pinned))
            },
        );

        match row {
            Ok((count, last_used, pinned)) => {
                let score = calculate_frecency(count, last_used, now, pinned);
                Ok(Some(score))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(IndexError::Sqlite(e)),
        }
    }

    /// Retrieves search history ordered by frecency score.
    pub fn get_recent_history(&self, limit: usize) -> Result<Vec<SearchHistoryItem>> {
        let conn = self.conn.lock().unwrap();
        let now = current_epoch_seconds();

        let mut stmt = conn.prepare(
            "SELECT id, query, target_key, title, category, use_count, first_used_epoch, last_used_epoch, is_pinned
             FROM search_history
             ORDER BY last_used_epoch DESC
             LIMIT ?1",
        )?;

        let rows = stmt.query_map(params![limit as i64], |r| {
            let use_count: u32 = r.get(5)?;
            let last_used: u64 = r.get(7)?;
            let is_pinned: bool = r.get(8)?;
            let frecency_score = calculate_frecency(use_count, last_used, now, is_pinned);

            Ok(SearchHistoryItem {
                id: r.get(0)?,
                query: r.get(1)?,
                target_key: r.get(2)?,
                title: r.get(3)?,
                category: r.get(4)?,
                use_count,
                first_used_epoch: r.get(6)?,
                last_used_epoch: last_used,
                is_pinned,
                frecency_score,
            })
        })?;

        let mut items = Vec::new();
        for item in rows {
            items.push(item?);
        }
        Ok(items)
    }

    /// Searches previous launch history using FTS5 BM25 match.
    pub fn search_history_fts(
        &self,
        query_term: &str,
        limit: usize,
    ) -> Result<Vec<SearchHistoryItem>> {
        let conn = self.conn.lock().unwrap();
        let now = current_epoch_seconds();

        // Escape query term for FTS5 prefix match
        let sanitized = query_term.replace('"', "\"\"");
        let fts_query = format!("\"{}\"*", sanitized);

        let mut stmt = conn.prepare(
            "SELECT h.id, h.query, h.target_key, h.title, h.category, h.use_count, h.first_used_epoch, h.last_used_epoch, h.is_pinned
             FROM search_history h
             JOIN search_history_fts f ON h.id = f.rowid
             WHERE search_history_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![fts_query, limit as i64], |r| {
            let use_count: u32 = r.get(5)?;
            let last_used: u64 = r.get(7)?;
            let is_pinned: bool = r.get(8)?;
            let frecency_score = calculate_frecency(use_count, last_used, now, is_pinned);

            Ok(SearchHistoryItem {
                id: r.get(0)?,
                query: r.get(1)?,
                target_key: r.get(2)?,
                title: r.get(3)?,
                category: r.get(4)?,
                use_count,
                first_used_epoch: r.get(6)?,
                last_used_epoch: last_used,
                is_pinned,
                frecency_score,
            })
        })?;

        let mut items = Vec::new();
        for item in rows {
            items.push(item?);
        }
        Ok(items)
    }

    /// Clears all stored search history for privacy.
    pub fn clear_history(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let deleted = conn.execute("DELETE FROM search_history;", [])?;
        conn.execute("DELETE FROM search_history_fts;", [])?;
        tracing::info!(deleted_count = deleted, "Cleared all search history");
        Ok(deleted)
    }

    /// Deletes a specific target from history.
    pub fn delete_target(&self, target_key: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let rows = conn.execute(
            "DELETE FROM search_history WHERE target_key = ?1;",
            params![target_key],
        )?;
        Ok(rows > 0)
    }

    pub fn db_path(&self) -> Option<&Path> {
        self.db_path.as_deref()
    }
}
