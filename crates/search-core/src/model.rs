//! Unified Search Data Model and Events

use serde::{Deserialize, Serialize};

/// High-level categories for grouping and badging search results.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SearchResultCategory {
    Application,
    Document,
    File,
    Folder,
    Calculation,
    Command,
    Setting,
    Web,
    Custom(String),
}

/// Action to execute when the user selects or launches a search result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LaunchTarget {
    Executable {
        path: String,
        arguments: Vec<String>,
        working_directory: Option<String>,
    },
    Document {
        path: String,
    },
    Uri {
        uri: String,
    },
    Action {
        action_id: String,
    },
    CopyToClipboard {
        text: String,
    },
    PackagedApplication {
        aumid: String,
        arguments: Option<String>,
    },
}

impl LaunchTarget {
    /// Canonical string key for deduplication.
    pub fn deduplication_key(&self) -> String {
        match self {
            Self::Executable { path, .. } => format!("exe:{}", path.to_lowercase()),
            Self::Document { path } => format!("doc:{}", path.to_lowercase()),
            Self::Uri { uri } => format!("uri:{}", uri.to_lowercase()),
            Self::Action { action_id } => format!("act:{}", action_id.to_lowercase()),
            Self::CopyToClipboard { text } => format!("clip:{}", text),
            Self::PackagedApplication { aumid, .. } => format!("pkg:{}", aumid.to_lowercase()),
        }
    }
}

/// Classification of how a search item matched the user's query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchKind {
    Exact,
    Prefix,
    Subsequence,
    FullTextSearch,
    Calculated,
    FrecencyHistory,
}

/// Detailed ranking signals used to compute the normalized score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RankingMetadata {
    pub match_kind: MatchKind,
    pub base_score: f32,
    pub frecency_boost: f32,
    pub provider_priority: u32,
}

/// Unified, provider-independent search result item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResult {
    /// Unique stable identifier (e.g. `app:c:\windows\notepad.exe`).
    pub id: String,
    /// Primary user-visible title.
    pub title: String,
    /// Contextual secondary line (e.g. file path, application category, math result).
    pub subtitle: String,
    /// Result category.
    pub category: SearchResultCategory,
    /// Optional icon path or symbolic icon identifier.
    pub icon_hint: Option<String>,
    /// How this result should be activated upon user selection.
    pub launch_target: LaunchTarget,
    /// Identifier of the provider that produced this result.
    pub provider_id: String,
    /// Normalized relevance score in the range [0.0, 1.0].
    pub score: f32,
    /// Granular scoring metadata.
    pub ranking_meta: Option<RankingMetadata>,
}

impl SearchResult {
    /// Constructs a new search result with default score and metadata.
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        subtitle: impl Into<String>,
        category: SearchResultCategory,
        launch_target: LaunchTarget,
        provider_id: impl Into<String>,
        score: f32,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            subtitle: subtitle.into(),
            category,
            icon_hint: None,
            launch_target,
            provider_id: provider_id.into(),
            score: score.clamp(0.0, 1.0),
            ranking_meta: None,
        }
    }

    pub fn with_icon(mut self, icon_hint: impl Into<String>) -> Self {
        self.icon_hint = Some(icon_hint.into());
        self
    }

    pub fn with_ranking_meta(mut self, meta: RankingMetadata) -> Self {
        self.ranking_meta = Some(meta);
        self
    }
}

/// An incoming search query with metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    pub raw: String,
    pub normalized: String,
    pub generation: u64,
}

impl SearchQuery {
    pub fn new(raw: impl Into<String>, generation: u64) -> Self {
        let raw_str = raw.into();
        let normalized = raw_str.trim().to_lowercase();
        Self {
            raw: raw_str,
            normalized,
            generation,
        }
    }
}

/// Incremental events emitted during a search session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SearchEvent {
    SearchStarted {
        generation: u64,
        query: String,
    },
    ResultsUpdated {
        generation: u64,
        results: Vec<SearchResult>,
        is_final: bool,
    },
    ProviderCompleted {
        generation: u64,
        provider_id: String,
        count: usize,
        duration_ms: u64,
    },
    SearchCompleted {
        generation: u64,
        total_results: usize,
        duration_ms: u64,
    },
    SearchFailed {
        generation: u64,
        provider_id: Option<String>,
        error: String,
    },
}

impl SearchEvent {
    /// Returns the generation number associated with this search event.
    pub fn generation(&self) -> u64 {
        match self {
            SearchEvent::SearchStarted { generation, .. } => *generation,
            SearchEvent::ResultsUpdated { generation, .. } => *generation,
            SearchEvent::ProviderCompleted { generation, .. } => *generation,
            SearchEvent::SearchCompleted { generation, .. } => *generation,
            SearchEvent::SearchFailed { generation, .. } => *generation,
        }
    }
}
