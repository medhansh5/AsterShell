//! Application Discovery Search Provider
//!
//! Matches user queries against cached installed applications with
//! multi-tier fuzzy, prefix, word-boundary, and exact matching.

use application_catalog::discover_installed_applications;
use astercore::DiscoveredApp;
use async_trait::async_trait;
use search_core::{
    LaunchTarget, MatchKind, RankingMetadata, SearchError, SearchProvider, SearchQuery,
    SearchResult, SearchResultCategory,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

pub const APP_PROVIDER_ID: &str = "application-discovery";
pub const APP_PROVIDER_PRIORITY: u32 = 90;

/// Fast, cached application discovery search provider.
#[derive(Clone)]
pub struct ApplicationDiscoveryProvider {
    cached_apps: Arc<RwLock<Vec<DiscoveredApp>>>,
    last_refresh_epoch_ms: Arc<AtomicU64>,
}

impl ApplicationDiscoveryProvider {
    /// Initializes the provider with an immediate scan of installed applications.
    pub fn new() -> Self {
        let apps = discover_installed_applications();
        Self::new_with_apps(apps)
    }

    /// Initializes the provider with a pre-populated application list (useful for testing).
    pub fn new_with_apps(apps: Vec<DiscoveredApp>) -> Self {
        let now_ms = Instant::now().elapsed().as_millis() as u64;
        Self {
            cached_apps: Arc::new(RwLock::new(apps)),
            last_refresh_epoch_ms: Arc::new(AtomicU64::new(now_ms)),
        }
    }

    /// Explicitly refreshes the cached application catalog.
    pub fn refresh(&self) -> usize {
        let fresh_apps = discover_installed_applications();
        let count = fresh_apps.len();
        if let Ok(mut cache) = self.cached_apps.write() {
            *cache = fresh_apps;
        }
        self.last_refresh_epoch_ms.store(
            Instant::now().elapsed().as_millis() as u64,
            Ordering::SeqCst,
        );
        tracing::info!(cached_count = count, "Application cache refreshed");
        count
    }

    /// Returns the number of applications currently cached.
    pub fn cached_count(&self) -> usize {
        self.cached_apps.read().map(|c| c.len()).unwrap_or(0)
    }
}

impl Default for ApplicationDiscoveryProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SearchProvider for ApplicationDiscoveryProvider {
    fn id(&self) -> &'static str {
        APP_PROVIDER_ID
    }

    fn display_name(&self) -> &'static str {
        "Applications"
    }

    fn priority(&self) -> u32 {
        APP_PROVIDER_PRIORITY
    }

    fn default_timeout(&self) -> Duration {
        Duration::from_millis(50)
    }

    async fn search(
        &self,
        query: &SearchQuery,
        cancel_token: &CancellationToken,
    ) -> Result<Vec<SearchResult>, SearchError> {
        if cancel_token.is_cancelled() {
            return Err(SearchError::Cancelled);
        }

        let q_norm = &query.normalized;
        if q_norm.is_empty() {
            return Ok(Vec::new());
        }

        let apps_guard = self
            .cached_apps
            .read()
            .map_err(|e| SearchError::Internal(format!("Cache lock poisoned: {}", e)))?;

        let mut results = Vec::new();

        for app in apps_guard.iter() {
            if cancel_token.is_cancelled() {
                return Err(SearchError::Cancelled);
            }

            let app_name_lower = app.name.to_lowercase();
            let score_opt = calculate_app_match_score(q_norm, &app_name_lower);

            if let Some((score, match_kind)) = score_opt {
                let launch_target = if app.is_packaged {
                    LaunchTarget::PackagedApplication {
                        aumid: app.executable_or_aumid.clone(),
                        arguments: None,
                    }
                } else {
                    LaunchTarget::Executable {
                        path: app.executable_or_aumid.clone(),
                        arguments: Vec::new(),
                        working_directory: None,
                    }
                };

                let meta = RankingMetadata {
                    match_kind,
                    base_score: score,
                    frecency_boost: 0.0,
                    provider_priority: APP_PROVIDER_PRIORITY,
                };

                let item = SearchResult::new(
                    format!("app:{}", app.id),
                    &app.name,
                    if app.is_packaged {
                        "Windows Store App"
                    } else {
                        "Application"
                    },
                    SearchResultCategory::Application,
                    launch_target,
                    APP_PROVIDER_ID,
                    score,
                )
                .with_ranking_meta(meta);

                results.push(item);
            }
        }

        Ok(results)
    }
}

/// Evaluates query against application name returning `(normalized_score, match_kind)` if matching.
pub fn calculate_app_match_score(query: &str, app_name_lower: &str) -> Option<(f32, MatchKind)> {
    if query.is_empty() {
        return None;
    }

    // 1. Exact match (case-insensitive)
    if app_name_lower == query {
        return Some((1.0, MatchKind::Exact));
    }

    // 2. Prefix match (e.g. "note" -> "notepad")
    if app_name_lower.starts_with(query) {
        let length_ratio = query.len() as f32 / app_name_lower.len() as f32;
        let score = (0.90 + 0.08 * length_ratio).clamp(0.0, 1.0);
        return Some((score, MatchKind::Prefix));
    }

    // 3. Word-boundary match (e.g. "term" -> "Windows Terminal")
    let words: Vec<&str> = app_name_lower.split_whitespace().collect();
    for (i, word) in words.iter().enumerate() {
        if word.starts_with(query) {
            let position_penalty = 0.02 * (i as f32).min(3.0);
            let length_ratio = query.len() as f32 / word.len() as f32;
            let score = (0.85 + 0.05 * length_ratio - position_penalty).clamp(0.0, 1.0);
            return Some((score, MatchKind::Prefix));
        }
    }

    // 4. Acronym / Initials match (e.g. "wt" -> "Windows Terminal", "vs" -> "Visual Studio")
    let acronym: String = words.iter().filter_map(|w| w.chars().next()).collect();
    if acronym == query {
        return Some((0.80, MatchKind::Subsequence));
    }

    // 5. General substring match (e.g. "pad" -> "Notepad")
    if app_name_lower.contains(query) {
        let length_ratio = query.len() as f32 / app_name_lower.len() as f32;
        let score = (0.65 + 0.05 * length_ratio).clamp(0.0, 1.0);
        return Some((score, MatchKind::Subsequence));
    }

    None
}
