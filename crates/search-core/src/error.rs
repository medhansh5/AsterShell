//! Error types for the search subsystem.

use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum SearchError {
    #[error("Search query was cancelled")]
    Cancelled,

    #[error("Provider '{0}' timed out")]
    Timeout(String),

    #[error("Provider '{0}' is unavailable: {1}")]
    ProviderUnavailable(String, String),

    #[error("Search execution failed in provider '{0}': {1}")]
    ExecutionFailed(String, String),

    #[error("Invalid query: {0}")]
    InvalidQuery(String),

    #[error("Internal search error: {0}")]
    Internal(String),
}

pub type SearchResult<T> = std::result::Result<T, SearchError>;

impl From<SearchError> for astercore::AsterError {
    fn from(err: SearchError) -> Self {
        astercore::AsterError::Search(err.to_string())
    }
}
