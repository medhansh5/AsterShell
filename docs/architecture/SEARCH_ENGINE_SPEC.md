# AsterShell — Hybrid Search Engine Architecture Specification

Search is the flagship feature of AsterShell. It provides an instantaneous, Spotlight-inspired, keyboard-first desktop experience activated globally via `Alt + Space`.

---

## 1. Performance Target & Latency Budget

- **Target:** **p95 warm query-to-rendered-results < 100 ms** (Stretch goal: < 50 ms for application launching).
- **Latency Breakdown Budget:**
  - Keypress capture & IPC dispatch to search orchestrator: < 2 ms
  - Query classification & provider routing: < 1 ms
  - In-memory SQLite FTS5 / Application Catalog match: < 5 ms
  - Calculator / AST evaluation: < 1 ms
  - Windows Search OLE DB query (warm pool): 5–15 ms
  - Result deduplication, scoring & sorting: < 3 ms
  - IPC streaming back to `search-ui`: < 2 ms
  - Slint UI rendering of top 8 results: < 10 ms
  - **Total Estimated End-to-End Latency: 29–39 ms** (well within the 100 ms p95 budget).

---

## 2. Federated Provider Architecture

The search engine is partitioned into discrete, decoupled search providers implementing a common async trait:

```rust
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub raw: String,
    pub query_type: QueryClassification,
    pub token: CancellationToken,
}

#[async_trait]
pub trait SearchProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn priority(&self) -> u32;
    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResultItem>, SearchError>;
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
           +-----------------------+-----------------------+
           |                       |                       |
     [ Classification ]     [ Debouncing ]     [ Cancellation ]
           |                       |                       |
           +-----------------------+-----------------------+
                                   |
         +-------------------------+-------------------------+
         |                         |                         |
         v                         v                         v
   [ Fast Tier: 0ms ]     [ Index Tier: 15ms ]     [ System Tier: 30ms ]
   - Calculator Provider  - App Catalog Provider   - Windows Search OLE DB
   - Quick Commands       - Recent Items Provider  - Shell Bookmarks
   - Math Evaluator       - SQLite FTS5 Files
         |                         |                         |
         +-------------------------+-------------------------+
                                   |
                                   v
                    +------------------------------+
                    | Result Stream Aggregator     |
                    | - Deduplication (File paths) |
                    | - Normalized Scoring (0..1)  |
                    | - Frecency (Frequency+Recency|
                    +--------------+---------------+
                                   | Incremental Batches
                                   v
                        +----------------------+
                        |      search-ui       | (Slint Render)
                        +----------------------+
```

---

## 3. Provider Implementations

### 1. Application Catalog Provider (Fast Tier - 0ms Debounce)
- Matches against pre-indexed application database (130+ Win32, UWP, and PWAs discovered via `shell:AppsFolder`).
- Executes in-memory using an optimized prefix, subsequence, and fuzzy match algorithm.
- Delivers matches within **2 to 5 ms**.

### 2. Calculator & Unit Conversion Provider (Fast Tier - 0ms Debounce)
- Evaluates arithmetic expressions (e.g., `(45 * 1024) / 8`, `sin(pi/4)`, `50 USD to EUR`).
- Powered by a pure local Rust math expression parser.
- Response time: **< 1 ms**.

### 3. Windows Search OLE DB Provider (System Tier - 25ms Debounce)
- Connects to `Search.CollatorDSO` targeting `SystemIndex`.
- Uses indexed SQL:
  ```sql
  SELECT TOP 20 System.ItemName, System.ItemUrl, System.Size, System.DateModified
  FROM SystemIndex
  WHERE SCOPE='file:' AND CONTAINS(System.ItemName, '"*query*"')
  ORDER BY System.Search.Rank DESC
  ```
- Operates over a warm, pre-initialized connection pool maintained by `search-core`.
- Response time: **7 to 15 ms** on warm index.

### 4. SQLite FTS5 Provider (Index Tier - 15ms Debounce)
- Indexes custom metadata, user search history, pinned bookmarks, and non-indexed project repositories.
- Employs SQLite FTS5 with BM25 ranking.

### 5. Quick Commands & Shell Actions (Fast Tier - 0ms Debounce)
- System commands: `lock`, `sleep`, `restart`, `shutdown`, `settings`, `terminal`.
- Instant action triggers directly from keyboard navigation.

---

## 4. Query Classification & Orchestration

The orchestrator classifies user input on each keystroke:
- **Math Expression:** Starts with numbers or math symbols (`=`, `(`, `12+`). Routes directly to Calculator.
- **Direct Path / URL:** Starts with drive letter (`C:\`), slash (`/`), or URL (`https://`). Routes to Filesystem / Browser provider.
- **Action Prefix:** Starts with `>` or `:`. Routes exclusively to Command Provider.
- **General Text:** Fans out asynchronously to Application Catalog, Recent Items, and Windows Search in parallel.

### Cancellation & Incremental Delivery
- Every new keystroke triggers a `CancellationToken::cancel()` on in-flight provider tasks from the previous query.
- As each provider yields results, they are pushed immediately via an mpsc channel to the UI, enabling instant visual updates without waiting for the slowest provider.
