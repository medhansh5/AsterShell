# AsterShell — Hybrid Search Engine Architecture Specification

Search is the flagship feature of AsterShell. It provides an instantaneous, Spotlight-inspired, keyboard-first desktop experience activated globally via `Alt + Space`.

---

## 1. Performance Target & Empirical Benchmarks

- **Target:** **p95 warm query-to-rendered-results < 100 ms** (Stretch goal: < 50 ms for application launching).
- **Empirical Benchmarks (Observed on Windows 11 Build 26300, 50-run sample):**
  - **Warm Application Search (End-to-End):** p50 = **0.16 ms**, p95 = **0.25 ms**, p99 = **0.35 ms** (sub-millisecond, exceeding target by 300x).
  - **Time to First Result (Incremental Delivery):** p50 = **0.17 ms**, p95 = **0.25 ms**.
  - **Local Calculator Evaluation:** p50 = **0.15 ms**, p95 = **0.27 ms**.
  - **Windows Search (`Search.CollatorDSO` OLE DB):** Cold fresh connection = **37.33 ms**; warm reused connection = **13.51 ms**; cancellation latency = **0.006 ms**.

---

## 2. Federated Provider Architecture

The search engine is partitioned into discrete, decoupled search providers implementing a common async trait in `search-core`:

```rust
use async_trait::async_trait;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[async_trait]
pub trait SearchProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn priority(&self) -> u32;
    fn default_timeout(&self) -> Duration {
        Duration::from_millis(250)
    }
    async fn search(
        &self,
        query: &SearchQuery,
        cancel_token: &CancellationToken,
    ) -> Result<Vec<SearchResult>, SearchError>;
}
```

```
                        +----------------------+
                        |      search-ui       | (Alt + Space)
                        +----------+-----------+
                                   | IPC (Named Pipe)
                                   v
                        +----------------------+
                        |  SearchOrchestrator  |
                        +----------+-----------+
                                   |
            +----------------------+-----------------------+
            |                      |                       |
     [ Generation ID ]      [ Cancellation ]       [ Frecency Scorer ]
            |                      |                       |
            +----------------------+-----------------------+
                                   |
          +------------------------+------------------------+
          |                        |                        |
          v                        v                        v
    [ Calculator ]          [ App Catalog ]         [ Windows Search ]
    - Pure Rust Math        - In-memory cache       - Dedicated COM MTA Thread
    - Priority 100          - Multi-tier matching   - OLE DB CollatorDSO
    - 0.15 ms p50           - Priority 90           - Priority 70
          |                        |                        |
          +------------------------+------------------------+
                                   |
                                   v
                    +------------------------------+
                    | Incremental Stream           |
                    | - Target Key Deduplication   |
                    | - Normalized Scoring (0..1)  |
                    | - Frecency (SQLite FTS5)     |
                    +--------------+---------------+
                                   | Incremental Events
                                   v
                        +----------------------+
                        |      search-ui       | (Instant Render)
                        +----------------------+
```

---

## 3. Implemented Provider Boundaries

### 3.1 Application Discovery Provider (`application-discovery`, Priority 90)
- In-memory cache wrapping `application-catalog`.
- Multi-tier matching hierarchy:
  1. Exact match (1.0 score, `MatchKind::Exact`).
  2. Prefix match (0.90–0.98 score, `MatchKind::Prefix`).
  3. Word-boundary token match (0.80–0.90 score, `MatchKind::Prefix`).
  4. Acronym match (0.80 score, `MatchKind::Subsequence`).
  5. Substring match (0.65–0.70 score, `MatchKind::Subsequence`).
- Cached catalog with explicit `.refresh()` method avoiding repeated filesystem traversal during typing.

### 3.2 Calculator Provider (`calculator`, Priority 100)
- Deterministic, zero-dependency recursive descent expression parser.
- Supports `+`, `-`, `*`, `/`, `%`, `^`, parentheses, unary signs, and decimal values.
- Explicit guards against division by zero and IEEE 754 overflow (`is_finite`).
- Zero `eval` or shell execution. Output mapped to `LaunchTarget::CopyToClipboard`.

### 3.3 Windows Search Provider (`windows-search`, Priority 70)
- Connects to `Search.CollatorDSO` targeting `SystemIndex`.
- **COM MTA Thread Affinity:** All COM operations execute strictly on a dedicated OS worker thread initialized with `CoInitializeEx(None, COINIT_MULTITHREADED)`.
- No raw COM interface pointers cross thread boundaries. Results communicate over typed asynchronous channels.
- Sanitized SQL queries prevent injection and syntax errors.
- Automatic graceful fallback if `WSearch` service is disabled or catalog is unavailable.

---

## 4. SQLite FTS5 Index & Frecency Architecture (`search-index`)

### Schema & Migrations
- Managed via versioned migrations (`schema_version`).
- `search_history` records launch targets, titles, usage counts, and epoch timestamps.
- `search_history_fts` full-text search table kept synchronized via `INSERT`, `UPDATE`, and `DELETE` database triggers.

### Frecency Scoring Algorithm
```
Frecency = min(use_count, 50) * Recency_Weight + Pin_Bonus
```
Where `Recency_Weight`:
- Within last 4 hours: **100.0**
- Within last 24 hours: **80.0**
- Within last 7 days: **60.0**
- Within last 30 days: **40.0**
- Older than 30 days: **20.0**
- Pinned bonus: **+200.0**

Normalized into a `[0.0, 1.0]` factor applied to elevate frequently launched items.

### Privacy & Recovery
- Complete history purging via `clear_history()`.
- Automatic detection and quarantine of corrupt database files (`.corrupt.<timestamp>`) with zero-downtime reinitialization.

---

## 5. Incremental Delivery & Query Lifecycle

1. **New Query Arrives:** Orchestrator generates a unique atomic `generation` ID.
2. **Cancellation:** Cancels the previous `CancellationToken`, immediately halting in-flight provider tasks.
3. **Event Stream:** Emits:
   - `SearchStarted { generation, query }`
   - `ResultsUpdated { generation, results, is_final: false }` (dispatched as each provider completes)
   - `ResultsUpdated { generation, results, is_final: true }` (dispatched when all providers complete)
   - `SearchCompleted { generation, total_results, duration_ms }`
4. **Stale Query Isolation:** If the generation counter advances while a provider is completing, the obsolete results are immediately dropped, preventing visual flicker or stale result contamination.
