//! AsterShell Search Providers
//!
//! Concrete implementations of the `SearchProvider` trait for applications,
//! math calculation, Windows Search OLE DB, and indexed content.

pub mod application;
pub mod calculator;
pub mod windows_search;

pub use application::ApplicationDiscoveryProvider;
pub use calculator::CalculatorProvider;
pub use windows_search::{
    build_system_index_sql, categorize_path, parse_file_url_to_path, WindowsSearchBenchmarkReport,
    WindowsSearchProvider, WindowsSearchResultItem,
};

pub fn crate_name() -> &'static str {
    "search-providers"
}

#[cfg(test)]
mod tests {
    use super::*;
    use application::calculate_app_match_score;
    use astercore::{AppDiscoverySource, DiscoveredApp};
    use search_core::{MatchKind, SearchProvider, SearchQuery, SearchResultCategory};
    use tokio_util::sync::CancellationToken;

    fn sample_apps() -> Vec<DiscoveredApp> {
        vec![
            DiscoveredApp {
                id: "notepad:c:\\windows\\notepad.exe".into(),
                name: "Notepad".into(),
                executable_or_aumid: "C:\\Windows\\notepad.exe".into(),
                icon_location: None,
                is_packaged: false,
                source: AppDiscoverySource::StartMenuShortcutSystem,
            },
            DiscoveredApp {
                id: "wt:c:\\windows\\wt.exe".into(),
                name: "Windows Terminal".into(),
                executable_or_aumid: "C:\\Windows\\wt.exe".into(),
                icon_location: None,
                is_packaged: true,
                source: AppDiscoverySource::AppsFolderCom,
            },
            DiscoveredApp {
                id: "calc:calc.exe".into(),
                name: "Calculator".into(),
                executable_or_aumid: "calc.exe".into(),
                icon_location: None,
                is_packaged: true,
                source: AppDiscoverySource::AppsFolderCom,
            },
        ]
    }

    #[test]
    fn test_app_match_scoring_ranks_exact_above_prefix_and_substring() {
        let (exact_score, exact_kind) = calculate_app_match_score("notepad", "notepad").unwrap();
        assert_eq!(exact_kind, MatchKind::Exact);
        assert_eq!(exact_score, 1.0);

        let (prefix_score, prefix_kind) = calculate_app_match_score("note", "notepad").unwrap();
        assert_eq!(prefix_kind, MatchKind::Prefix);
        assert!(prefix_score < exact_score);

        let (sub_score, sub_kind) = calculate_app_match_score("pad", "notepad").unwrap();
        assert_eq!(sub_kind, MatchKind::Subsequence);
        assert!(sub_score < prefix_score);

        // Word-boundary match: "terminal" matches "Windows Terminal"
        let (wb_score, wb_kind) =
            calculate_app_match_score("terminal", "windows terminal").unwrap();
        assert_eq!(wb_kind, MatchKind::Prefix);
        assert!(wb_score > sub_score);

        // Acronym match: "wt" matches "Windows Terminal"
        let (ac_score, ac_kind) = calculate_app_match_score("wt", "windows terminal").unwrap();
        assert_eq!(ac_kind, MatchKind::Subsequence);
        assert!(ac_score > 0.7);
    }

    #[tokio::test]
    async fn test_application_discovery_provider_search() {
        let provider = ApplicationDiscoveryProvider::new_with_apps(sample_apps());
        let cancel = CancellationToken::new();

        let q = SearchQuery::new("note", 1);
        let results = provider
            .search(&q, &cancel)
            .await
            .expect("Search should succeed");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Notepad");
        assert_eq!(results[0].provider_id, "application-discovery");
    }

    #[tokio::test]
    async fn test_application_discovery_provider_refresh() {
        let provider = ApplicationDiscoveryProvider::new_with_apps(Vec::new());
        assert_eq!(provider.cached_count(), 0);

        let refreshed = provider.refresh();
        assert!(refreshed > 0);
        assert_eq!(provider.cached_count(), refreshed);
    }

    #[test]
    fn test_windows_search_sql_construction_and_escaping() {
        let sql = build_system_index_sql("my document'; DROP TABLE Students;--", 15);
        // Semicolons and quotes stripped from query input to prevent statement injection
        assert!(!sql.contains(';'));
        assert!(!sql.contains("';"));
        assert!(sql.contains("TOP 15"));
        assert!(sql.contains("WHERE SCOPE='file:'"));
    }

    #[test]
    fn test_windows_search_url_parsing_and_categorization() {
        assert_eq!(
            parse_file_url_to_path("file:///C:/Users/medha/report.docx"),
            "C:\\Users\\medha\\report.docx"
        );
        assert_eq!(
            parse_file_url_to_path("file:C:/Windows/system.ini"),
            "C:\\Windows\\system.ini"
        );

        assert_eq!(
            categorize_path("C:\\report.pdf"),
            SearchResultCategory::Document
        );
        assert_eq!(
            categorize_path("C:\\code.rs"),
            SearchResultCategory::Document
        );
        assert_eq!(categorize_path("C:\\image.png"), SearchResultCategory::File);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn test_windows_search_provider_live_query() {
        let provider = WindowsSearchProvider::new();
        let cancel = CancellationToken::new();

        // Querying for indexed items (e.g. "windows" or "excel" or "users")
        let q = SearchQuery::new("windows", 1);
        let results = provider.search(&q, &cancel).await;

        match results {
            Ok(items) => {
                println!("Live Windows Search returned {} items", items.len());
                for item in items.iter().take(3) {
                    println!("  Found: {} ({})", item.title, item.subtitle);
                    assert_eq!(item.provider_id, "windows-search");
                }
            }
            Err(search_core::SearchError::ProviderUnavailable(provider_id, reason)) => {
                println!(
                    "Windows Search service is unavailable in this environment ({}: {}) - live index query skipped",
                    provider_id, reason
                );
            }
            Err(e) => {
                panic!("Unexpected error from Windows Search provider: {:?}", e);
            }
        }
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn test_windows_search_cancellation() {
        let provider = WindowsSearchProvider::new();
        let cancel = CancellationToken::new();
        cancel.cancel(); // Pre-cancelled

        let q = SearchQuery::new("system", 1);
        let res = provider.search(&q, &cancel).await;
        assert_eq!(res.unwrap_err(), search_core::SearchError::Cancelled);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn test_windows_search_benchmark_execution() {
        let provider = WindowsSearchProvider::new();
        let report = provider
            .run_benchmark("report", 5)
            .await
            .expect("Benchmark should execute");

        println!("--- Windows Search Micro-Benchmark Results ---");
        println!(
            "Cold Fresh Connection Latency: {:.2} ms",
            report.cold_fresh_connection_ms
        );
        println!(
            "Warm Reused Connection Latency (Avg): {:.2} ms",
            report.warm_reused_connection_avg_ms
        );
        println!(
            "Fresh Connection Latency (Avg): {:.2} ms",
            report.fresh_connection_avg_ms
        );
        println!(
            "Cancellation Latency: {:.4} ms",
            report.cancellation_latency_ms
        );

        assert!(report.cancellation_latency_ms < 5.0);
    }
}
