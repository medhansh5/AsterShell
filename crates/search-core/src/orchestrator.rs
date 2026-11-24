//! Search Orchestration and Incremental Result Delivery

use crate::error::SearchError;
use crate::model::{SearchEvent, SearchQuery, SearchResult};
use crate::provider::SearchProvider;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

/// Type alias for thread-safe frecency scoring functions.
pub type FrecencyScorer = Arc<dyn Fn(&str) -> Option<f32> + Send + Sync>;

/// Coordinates search execution across multiple concurrent providers.
pub struct SearchOrchestrator {
    providers: Vec<Arc<dyn SearchProvider>>,
    current_generation: Arc<AtomicU64>,
    active_token: Arc<Mutex<Option<CancellationToken>>>,
    max_results: usize,
    frecency_scorer: Option<FrecencyScorer>,
}

impl SearchOrchestrator {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
            current_generation: Arc::new(AtomicU64::new(0)),
            active_token: Arc::new(Mutex::new(None)),
            max_results: 50,
            frecency_scorer: None,
        }
    }

    pub fn with_max_results(mut self, max: usize) -> Self {
        self.max_results = max;
        self
    }

    /// Injects an optional frecency scoring callback (e.g. from search-index).
    pub fn with_frecency_scorer<F>(mut self, scorer: F) -> Self
    where
        F: Fn(&str) -> Option<f32> + Send + Sync + 'static,
    {
        self.frecency_scorer = Some(Arc::new(scorer));
        self
    }

    /// Registers a search provider.
    pub fn register_provider(&mut self, provider: Arc<dyn SearchProvider>) {
        self.providers.push(provider);
    }

    /// Starts an asynchronous search query, cancelling any previous in-flight query.
    pub async fn execute_search(
        &self,
        raw_query: &str,
    ) -> (u64, mpsc::UnboundedReceiver<SearchEvent>) {
        let generation = self.current_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let new_token = CancellationToken::new();

        // Cancel previous query
        {
            let mut active = self.active_token.lock().await;
            if let Some(old_token) = active.take() {
                old_token.cancel();
            }
            *active = Some(new_token.clone());
        }

        let (tx, rx) = mpsc::unbounded_channel();
        let providers = self.providers.clone();
        let query = SearchQuery::new(raw_query, generation);
        let max_results = self.max_results;
        let current_gen_atomic = self.current_generation.clone();
        let frecency_scorer = self.frecency_scorer.clone();

        tokio::spawn(async move {
            let start_time = Instant::now();
            let _ = tx.send(SearchEvent::SearchStarted {
                generation,
                query: query.raw.clone(),
            });

            if query.normalized.is_empty() || providers.is_empty() {
                let _ = tx.send(SearchEvent::ResultsUpdated {
                    generation,
                    results: Vec::new(),
                    is_final: true,
                });
                let _ = tx.send(SearchEvent::SearchCompleted {
                    generation,
                    total_results: 0,
                    duration_ms: start_time.elapsed().as_millis() as u64,
                });
                return;
            }

            // Channel for provider task completion
            let (provider_tx, mut provider_rx) = mpsc::unbounded_channel();

            for provider in &providers {
                let p = provider.clone();
                let q = query.clone();
                let token = new_token.clone();
                let p_tx = provider_tx.clone();
                let timeout = p.default_timeout();
                let provider_id = p.id().to_string();
                let priority = p.priority();

                tokio::spawn(async move {
                    let p_start = Instant::now();
                    let res = tokio::select! {
                        _ = token.cancelled() => Err(SearchError::Cancelled),
                        r = tokio::time::timeout(timeout, p.search(&q, &token)) => {
                            match r {
                                Ok(inner) => inner,
                                Err(_) => Err(SearchError::Timeout(provider_id.clone())),
                            }
                        }
                    };
                    let duration = p_start.elapsed().as_millis() as u64;
                    let _ = p_tx.send((provider_id, priority, duration, res));
                });
            }
            drop(provider_tx); // Allow receiver to terminate when all tasks complete

            let mut accumulated: Vec<SearchResult> = Vec::new();
            let mut completed_count = 0;
            let total_providers = providers.len();

            while let Some((p_id, p_priority, duration, result)) = provider_rx.recv().await {
                // If a newer query has arrived, abort processing immediately
                if current_gen_atomic.load(Ordering::SeqCst) != generation
                    || new_token.is_cancelled()
                {
                    tracing::debug!(generation, "Discarding obsolete query results");
                    return;
                }

                completed_count += 1;
                let is_final = completed_count == total_providers;

                match result {
                    Ok(items) => {
                        let count = items.len();
                        for mut item in items {
                            if let Some(ref scorer) = frecency_scorer {
                                let key = item.launch_target.deduplication_key();
                                if let Some(boost) = scorer(&key) {
                                    item.score = (item.score * 0.80 + boost * 0.20).clamp(0.0, 1.0);
                                    if let Some(ref mut meta) = item.ranking_meta {
                                        meta.frecency_boost = boost;
                                    } else {
                                        item.ranking_meta = Some(crate::model::RankingMetadata {
                                            match_kind: crate::model::MatchKind::FrecencyHistory,
                                            base_score: item.score,
                                            frecency_boost: boost,
                                            provider_priority: p_priority,
                                        });
                                    }
                                }
                            }

                            // Assign provider priority if not already set in ranking_meta
                            if let Some(ref mut meta) = item.ranking_meta {
                                if meta.provider_priority == 0 {
                                    meta.provider_priority = p_priority;
                                }
                            }
                            accumulated.push(item);
                        }

                        // Deduplicate and rank
                        let sorted_results = deduplicate_and_rank(&accumulated, max_results);

                        let _ = tx.send(SearchEvent::ResultsUpdated {
                            generation,
                            results: sorted_results,
                            is_final,
                        });

                        let _ = tx.send(SearchEvent::ProviderCompleted {
                            generation,
                            provider_id: p_id,
                            count,
                            duration_ms: duration,
                        });
                    }
                    Err(SearchError::Cancelled) => {
                        return;
                    }
                    Err(e) => {
                        tracing::warn!(provider = %p_id, error = %e, "Search provider returned error");
                        let _ = tx.send(SearchEvent::SearchFailed {
                            generation,
                            provider_id: Some(p_id.clone()),
                            error: e.to_string(),
                        });
                        let _ = tx.send(SearchEvent::ProviderCompleted {
                            generation,
                            provider_id: p_id,
                            count: 0,
                            duration_ms: duration,
                        });
                    }
                }
            }

            let final_results = deduplicate_and_rank(&accumulated, max_results);
            let _ = tx.send(SearchEvent::SearchCompleted {
                generation,
                total_results: final_results.len(),
                duration_ms: start_time.elapsed().as_millis() as u64,
            });
        });

        (generation, rx)
    }
}

impl Default for SearchOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

/// Deduplicates results by canonical launch target key and ranks them deterministically.
pub fn deduplicate_and_rank(results: &[SearchResult], max_results: usize) -> Vec<SearchResult> {
    let mut deduped: HashMap<String, SearchResult> = HashMap::new();

    for item in results {
        let key = item.launch_target.deduplication_key();
        match deduped.get(&key) {
            Some(existing) => {
                // Keep the one with the higher score or higher priority
                let existing_priority = existing
                    .ranking_meta
                    .as_ref()
                    .map(|m| m.provider_priority)
                    .unwrap_or(0);
                let current_priority = item
                    .ranking_meta
                    .as_ref()
                    .map(|m| m.provider_priority)
                    .unwrap_or(0);

                if item.score > existing.score
                    || (item.score == existing.score && current_priority > existing_priority)
                {
                    deduped.insert(key, item.clone());
                }
            }
            None => {
                deduped.insert(key, item.clone());
            }
        }
    }

    let mut ranked: Vec<SearchResult> = deduped.into_values().collect();

    // Deterministic ranking sort:
    // 1. Descending score
    // 2. Descending provider priority
    // 3. Alphabetical title
    // 4. Stable ID tie-breaker
    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let prio_a = a
                    .ranking_meta
                    .as_ref()
                    .map(|m| m.provider_priority)
                    .unwrap_or(0);
                let prio_b = b
                    .ranking_meta
                    .as_ref()
                    .map(|m| m.provider_priority)
                    .unwrap_or(0);
                prio_b.cmp(&prio_a)
            })
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
            .then_with(|| a.id.cmp(&b.id))
    });

    if ranked.len() > max_results {
        ranked.truncate(max_results);
    }

    ranked
}
