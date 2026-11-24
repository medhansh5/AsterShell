//! Search Provider Abstraction

use crate::error::SearchError;
use crate::model::{SearchQuery, SearchResult};
use async_trait::async_trait;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Asynchronous search provider interface.
#[async_trait]
pub trait SearchProvider: Send + Sync {
    /// Stable machine identifier for the provider (e.g. `app-catalog`, `windows-search`, `calculator`).
    fn id(&self) -> &'static str;

    /// Human-readable display label.
    fn display_name(&self) -> &'static str;

    /// Priority order for ranking tie-breakers (higher number = higher priority).
    fn priority(&self) -> u32;

    /// Maximum time allowed for this provider to execute before timing out.
    fn default_timeout(&self) -> Duration {
        Duration::from_millis(250)
    }

    /// Asynchronously searches for results matching the query.
    ///
    /// Must periodically check `cancel_token.is_cancelled()` to support cooperative cancellation.
    async fn search(
        &self,
        query: &SearchQuery,
        cancel_token: &CancellationToken,
    ) -> Result<Vec<SearchResult>, SearchError>;
}
