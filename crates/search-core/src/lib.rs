//! AsterShell Search Core Subsystem
//!
//! Provides the provider-independent search architecture, unified data model,
//! concurrent execution orchestrator, and incremental delivery pipeline.

pub mod error;
pub mod model;
pub mod orchestrator;
pub mod provider;

pub use error::{SearchError, SearchResult as Result};
pub use model::{
    LaunchTarget, MatchKind, RankingMetadata, SearchEvent, SearchQuery, SearchResult,
    SearchResultCategory,
};
pub use orchestrator::{deduplicate_and_rank, SearchOrchestrator};
pub use provider::SearchProvider;

pub fn crate_name() -> &'static str {
    "search-core"
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio_util::sync::CancellationToken;

    struct MockFastProvider {
        id: &'static str,
        priority: u32,
    }

    #[async_trait]
    impl SearchProvider for MockFastProvider {
        fn id(&self) -> &'static str {
            self.id
        }
        fn display_name(&self) -> &'static str {
            "Mock Fast"
        }
        fn priority(&self) -> u32 {
            self.priority
        }
        async fn search(
            &self,
            query: &SearchQuery,
            _cancel_token: &CancellationToken,
        ) -> Result<Vec<SearchResult>> {
            let res = vec![SearchResult::new(
                format!("{}:{}", self.id, query.raw),
                format!("Fast Match {}", query.raw),
                "Subtitle",
                SearchResultCategory::Application,
                LaunchTarget::Executable {
                    path: "C:\\app.exe".into(),
                    arguments: vec![],
                    working_directory: None,
                },
                self.id,
                0.95,
            )];
            Ok(res)
        }
    }

    struct MockSlowProvider {
        delay_ms: u64,
    }

    #[async_trait]
    impl SearchProvider for MockSlowProvider {
        fn id(&self) -> &'static str {
            "slow-provider"
        }
        fn display_name(&self) -> &'static str {
            "Mock Slow"
        }
        fn priority(&self) -> u32 {
            10
        }
        fn default_timeout(&self) -> Duration {
            Duration::from_millis(100)
        }
        async fn search(
            &self,
            _query: &SearchQuery,
            cancel_token: &CancellationToken,
        ) -> Result<Vec<SearchResult>> {
            tokio::select! {
                _ = cancel_token.cancelled() => Err(SearchError::Cancelled),
                _ = tokio::time::sleep(Duration::from_millis(self.delay_ms)) => {
                    Ok(vec![SearchResult::new(
                        "slow:1",
                        "Slow Result",
                        "Sub",
                        SearchResultCategory::File,
                        LaunchTarget::Document { path: "C:\\doc.txt".into() },
                        "slow-provider",
                        0.5,
                    )])
                }
            }
        }
    }

    struct MockFailingProvider;

    #[async_trait]
    impl SearchProvider for MockFailingProvider {
        fn id(&self) -> &'static str {
            "failing-provider"
        }
        fn display_name(&self) -> &'static str {
            "Mock Failing"
        }
        fn priority(&self) -> u32 {
            5
        }
        async fn search(
            &self,
            _query: &SearchQuery,
            _cancel_token: &CancellationToken,
        ) -> Result<Vec<SearchResult>> {
            Err(SearchError::ExecutionFailed(
                "failing-provider".into(),
                "Synthetic provider failure".into(),
            ))
        }
    }

    #[tokio::test]
    async fn test_orchestrator_concurrent_incremental_delivery() {
        let mut orchestrator = SearchOrchestrator::new();
        orchestrator.register_provider(Arc::new(MockFastProvider {
            id: "fast-1",
            priority: 50,
        }));
        orchestrator.register_provider(Arc::new(MockSlowProvider { delay_ms: 30 }));

        let (gen, mut rx) = orchestrator.execute_search("terminal").await;
        assert_eq!(gen, 1);

        let mut received_started = false;
        let mut update_count = 0;
        let mut completed = false;

        while let Some(event) = rx.recv().await {
            match event {
                SearchEvent::SearchStarted { generation, query } => {
                    assert_eq!(generation, 1);
                    assert_eq!(query, "terminal");
                    received_started = true;
                }
                SearchEvent::ResultsUpdated {
                    generation,
                    results,
                    is_final,
                } => {
                    assert_eq!(generation, 1);
                    update_count += 1;
                    if update_count == 1 {
                        // First incremental update arrived from fast provider
                        assert!(!results.is_empty());
                        assert_eq!(results[0].provider_id, "fast-1");
                    }
                    if is_final {
                        completed = true;
                    }
                }
                SearchEvent::SearchCompleted { generation, .. } => {
                    assert_eq!(generation, 1);
                    break;
                }
                _ => {}
            }
        }

        assert!(received_started);
        assert!(update_count >= 1);
        assert!(completed);
    }

    #[tokio::test]
    async fn test_orchestrator_cancellation_and_stale_isolation() {
        let mut orchestrator = SearchOrchestrator::new();
        orchestrator.register_provider(Arc::new(MockSlowProvider { delay_ms: 100 }));

        // Start Query 1
        let (_gen1, mut rx1) = orchestrator.execute_search("first").await;
        // Immediately start Query 2, superseding Query 1
        let (gen2, mut rx2) = orchestrator.execute_search("second").await;
        assert_eq!(gen2, 2);

        // rx1 should not produce completed results for query 1
        let mut gen1_completed = false;
        while let Ok(event) = rx1.try_recv() {
            if let SearchEvent::SearchCompleted { generation, .. } = event {
                if generation == 1 {
                    gen1_completed = true;
                }
            }
        }
        assert!(!gen1_completed);

        // rx2 should complete normally
        let mut gen2_completed = false;
        while let Some(event) = rx2.recv().await {
            if let SearchEvent::SearchCompleted { generation, .. } = event {
                assert_eq!(generation, 2);
                gen2_completed = true;
                break;
            }
        }
        assert!(gen2_completed);
    }

    #[tokio::test]
    async fn test_orchestrator_provider_timeout_and_failure_isolation() {
        let mut orchestrator = SearchOrchestrator::new();
        // Provider that sleeps 300ms exceeds default_timeout of 100ms
        orchestrator.register_provider(Arc::new(MockSlowProvider { delay_ms: 300 }));
        orchestrator.register_provider(Arc::new(MockFailingProvider));
        orchestrator.register_provider(Arc::new(MockFastProvider {
            id: "fast",
            priority: 20,
        }));

        let (_gen, mut rx) = orchestrator.execute_search("test").await;

        let mut had_failure = false;
        let mut had_success = false;

        while let Some(event) = rx.recv().await {
            match event {
                SearchEvent::ResultsUpdated { results, .. } => {
                    if results.iter().any(|r| r.provider_id == "fast") {
                        had_success = true;
                    }
                }
                SearchEvent::SearchFailed { provider_id, .. } => {
                    if provider_id == Some("failing-provider".into()) {
                        had_failure = true;
                    }
                }
                SearchEvent::SearchCompleted { .. } => break,
                _ => {}
            }
        }

        assert!(had_success, "Fast provider should still succeed");
        assert!(had_failure, "Failing provider error was reported");
    }

    #[test]
    fn test_deduplication_and_deterministic_ranking() {
        let res1 = SearchResult::new(
            "app1",
            "Notepad",
            "Editor",
            SearchResultCategory::Application,
            LaunchTarget::Executable {
                path: "C:\\notepad.exe".into(),
                arguments: vec![],
                working_directory: None,
            },
            "catalog",
            0.9,
        );

        // Same launch target with lower score should be deduplicated away
        let res2 = SearchResult::new(
            "file1",
            "Notepad Executable",
            "File",
            SearchResultCategory::File,
            LaunchTarget::Executable {
                path: "C:\\notepad.exe".into(),
                arguments: vec![],
                working_directory: None,
            },
            "windows-search",
            0.6,
        );

        // Different launch target with identical title must NOT be merged
        let res3 = SearchResult::new(
            "doc1",
            "Notepad",
            "Shortcut notes",
            SearchResultCategory::Document,
            LaunchTarget::Document {
                path: "C:\\notes\\notepad.txt".into(),
            },
            "windows-search",
            0.8,
        );

        let input = vec![res1.clone(), res2, res3.clone()];
        let ranked = deduplicate_and_rank(&input, 10);

        assert_eq!(ranked.len(), 2);
        // Highest score (0.9) first
        assert_eq!(ranked[0].id, "app1");
        assert_eq!(ranked[0].score, 0.9);
        // Second score (0.8) preserved
        assert_eq!(ranked[1].id, "doc1");
        assert_eq!(ranked[1].score, 0.8);
    }

    #[tokio::test]
    async fn test_orchestrator_frecency_ranking_influence() {
        let mut orchestrator = SearchOrchestrator::new();
        orchestrator.register_provider(Arc::new(MockFastProvider {
            id: "fast",
            priority: 50,
        }));

        // Inject frecency boost for target
        let orchestrator = orchestrator.with_frecency_scorer(|key| {
            if key == "exe:c:\\app.exe" {
                Some(1.0)
            } else {
                None
            }
        });

        let (_gen, mut rx) = orchestrator.execute_search("app").await;
        while let Some(event) = rx.recv().await {
            if let SearchEvent::ResultsUpdated { results, .. } = event {
                if !results.is_empty() {
                    // Score with frecency: 0.95 * 0.80 + 1.0 * 0.20 = 0.96
                    assert!(results[0].score > 0.95);
                    assert_eq!(
                        results[0].ranking_meta.as_ref().unwrap().frecency_boost,
                        1.0
                    );
                }
            }
        }
    }
}
