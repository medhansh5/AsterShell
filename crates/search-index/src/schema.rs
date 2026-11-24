//! SQLite Schema Definitions and Versioned Migrations

use crate::error::Result;
use rusqlite::Connection;

pub const CURRENT_SCHEMA_VERSION: i32 = 1;

pub fn initialize_or_migrate(conn: &mut Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY,
            applied_at INTEGER NOT NULL
        );",
    )?;

    let current_v: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if current_v < 1 {
        apply_migration_v1(conn)?;
    }

    Ok(())
}

fn apply_migration_v1(conn: &mut Connection) -> Result<()> {
    let tx = conn.transaction()?;

    tx.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS search_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            query TEXT NOT NULL,
            target_key TEXT NOT NULL UNIQUE,
            title TEXT NOT NULL,
            category TEXT NOT NULL,
            use_count INTEGER NOT NULL DEFAULT 1,
            first_used_epoch INTEGER NOT NULL,
            last_used_epoch INTEGER NOT NULL,
            is_pinned INTEGER NOT NULL DEFAULT 0
        );

        CREATE INDEX IF NOT EXISTS idx_search_history_last_used 
        ON search_history(last_used_epoch DESC);

        CREATE INDEX IF NOT EXISTS idx_search_history_query 
        ON search_history(query);

        CREATE VIRTUAL TABLE IF NOT EXISTS search_history_fts USING fts5(
            title,
            query,
            content='search_history',
            content_rowid='id'
        );

        -- Sync triggers for FTS5 index
        CREATE TRIGGER IF NOT EXISTS trg_history_insert AFTER INSERT ON search_history BEGIN
            INSERT INTO search_history_fts(rowid, title, query) 
            VALUES (new.id, new.title, new.query);
        END;

        CREATE TRIGGER IF NOT EXISTS trg_history_delete AFTER DELETE ON search_history BEGIN
            INSERT INTO search_history_fts(search_history_fts, rowid, title, query) 
            VALUES ('delete', old.id, old.title, old.query);
        END;

        CREATE TRIGGER IF NOT EXISTS trg_history_update AFTER UPDATE ON search_history BEGIN
            INSERT INTO search_history_fts(search_history_fts, rowid, title, query) 
            VALUES ('delete', old.id, old.title, old.query);
            INSERT INTO search_history_fts(rowid, title, query) 
            VALUES (new.id, new.title, new.query);
        END;

        INSERT INTO schema_version (version, applied_at) 
        VALUES (1, strftime('%s', 'now'));
        ",
    )?;

    tx.commit()?;
    tracing::info!("Applied search index schema migration v1");
    Ok(())
}
