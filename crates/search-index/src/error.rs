//! Error types for the search index subsystem.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum IndexError {
    #[error("SQLite database error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("Database migration error: {0}")]
    Migration(String),

    #[error("Corrupt database recovered and reinitialized: {0}")]
    CorruptDatabaseRecovered(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Index error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, IndexError>;

impl From<IndexError> for astercore::AsterError {
    fn from(err: IndexError) -> Self {
        astercore::AsterError::Search(err.to_string())
    }
}
