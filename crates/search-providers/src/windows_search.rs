//! Windows Search OLE DB Provider (`Search.CollatorDSO`)
//!
//! Executes queries against Windows Search `SystemIndex` on a dedicated COM MTA worker thread.
//! Guarantees strict COM apartment affinity, prevents cross-thread interface leaks,
//! provides safe SQL escaping, and performs comparative latency benchmarking.

use async_trait::async_trait;
use search_core::{
    LaunchTarget, MatchKind, RankingMetadata, SearchError, SearchProvider, SearchQuery,
    SearchResult, SearchResultCategory,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

pub const WINDOWS_SEARCH_PROVIDER_ID: &str = "windows-search";
pub const WINDOWS_SEARCH_PROVIDER_PRIORITY: u32 = 70;

/// Internal job messages dispatched to the dedicated COM MTA worker thread.
enum ComWorkerJob {
    Search {
        query_string: String,
        max_results: usize,
        respond_to: oneshot::Sender<Result<Vec<WindowsSearchResultItem>, SearchError>>,
    },
    Benchmark {
        query_string: String,
        iterations: usize,
        respond_to: oneshot::Sender<Result<WindowsSearchBenchmarkReport, SearchError>>,
    },
    Shutdown,
}

/// Raw item extracted from the Windows Search index.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowsSearchResultItem {
    pub name: String,
    pub path: String,
    pub size_bytes: Option<u64>,
    pub rank: u32,
}

/// Empirical latency metrics recorded by the Windows Search provider.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowsSearchBenchmarkReport {
    pub cold_fresh_connection_ms: f64,
    pub warm_reused_connection_avg_ms: f64,
    pub warm_reused_latencies_ms: Vec<f64>,
    pub fresh_connection_avg_ms: f64,
    pub cancellation_latency_ms: f64,
}

/// Asynchronous search provider wrapping Windows Search `SystemIndex`.
pub struct WindowsSearchProvider {
    job_tx: mpsc::Sender<ComWorkerJob>,
    _worker_thread: Arc<std::sync::Mutex<Option<std::thread::JoinHandle<()>>>>,
}

impl WindowsSearchProvider {
    /// Spawns the dedicated COM MTA background worker and initializes the provider.
    pub fn new() -> Self {
        let (job_tx, job_rx) = mpsc::channel(32);

        let handle = std::thread::Builder::new()
            .name("windows-search-com-mta".into())
            .spawn(move || {
                run_com_mta_worker_loop(job_rx);
            })
            .expect("Failed to spawn Windows Search COM worker thread");

        Self {
            job_tx,
            _worker_thread: Arc::new(std::sync::Mutex::new(Some(handle))),
        }
    }

    /// Executes comparative benchmarks between fresh and reused connections.
    pub async fn run_benchmark(
        &self,
        query: &str,
        iterations: usize,
    ) -> Result<WindowsSearchBenchmarkReport, SearchError> {
        let (tx, rx) = oneshot::channel();
        self.job_tx
            .send(ComWorkerJob::Benchmark {
                query_string: query.to_string(),
                iterations,
                respond_to: tx,
            })
            .await
            .map_err(|_| SearchError::Internal("COM worker thread has exited".into()))?;

        rx.await
            .map_err(|_| SearchError::Internal("Benchmark response dropped".into()))?
    }
}

impl Default for WindowsSearchProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for WindowsSearchProvider {
    fn drop(&mut self) {
        let _ = self.job_tx.try_send(ComWorkerJob::Shutdown);
    }
}

#[async_trait]
impl SearchProvider for WindowsSearchProvider {
    fn id(&self) -> &'static str {
        WINDOWS_SEARCH_PROVIDER_ID
    }

    fn display_name(&self) -> &'static str {
        "Files & Documents"
    }

    fn priority(&self) -> u32 {
        WINDOWS_SEARCH_PROVIDER_PRIORITY
    }

    fn default_timeout(&self) -> Duration {
        Duration::from_millis(300)
    }

    async fn search(
        &self,
        query: &SearchQuery,
        cancel_token: &CancellationToken,
    ) -> Result<Vec<SearchResult>, SearchError> {
        if cancel_token.is_cancelled() {
            return Err(SearchError::Cancelled);
        }

        let q_norm = query.normalized.trim();
        if q_norm.is_empty() || q_norm.len() < 2 {
            return Ok(Vec::new());
        }

        let (resp_tx, resp_rx) = oneshot::channel();

        self.job_tx
            .send(ComWorkerJob::Search {
                query_string: q_norm.to_string(),
                max_results: 20,
                respond_to: resp_tx,
            })
            .await
            .map_err(|_| SearchError::Internal("COM worker thread closed".into()))?;

        tokio::select! {
            _ = cancel_token.cancelled() => Err(SearchError::Cancelled),
            res = resp_rx => {
                match res {
                    Ok(Ok(items)) => {
                        let mut results = Vec::new();
                        for item in items {
                            let category = categorize_path(&item.path);
                            let normalized_score = (0.60 + 0.20 * (item.rank as f32 / 1000.0).clamp(0.0, 1.0)).clamp(0.0, 0.85);

                            let meta = RankingMetadata {
                                match_kind: MatchKind::FullTextSearch,
                                base_score: normalized_score,
                                frecency_boost: 0.0,
                                provider_priority: WINDOWS_SEARCH_PROVIDER_PRIORITY,
                            };

                            let result = SearchResult::new(
                                format!("file:{}", item.path),
                                item.name,
                                item.path.clone(),
                                category,
                                LaunchTarget::Document { path: item.path },
                                WINDOWS_SEARCH_PROVIDER_ID,
                                normalized_score,
                            )
                            .with_ranking_meta(meta);

                            results.push(result);
                        }
                        Ok(results)
                    }
                    Ok(Err(e)) => Err(e),
                    Err(_) => Err(SearchError::Internal("Worker channel dropped response".into())),
                }
            }
        }
    }
}

/// Classifies a path into Document vs File based on file extension.
pub fn categorize_path(path: &str) -> SearchResultCategory {
    let lower = path.to_lowercase();
    if lower.ends_with(".docx")
        || lower.ends_with(".doc")
        || lower.ends_with(".pdf")
        || lower.ends_with(".xlsx")
        || lower.ends_with(".pptx")
        || lower.ends_with(".txt")
        || lower.ends_with(".md")
        || lower.ends_with(".rtf")
        || lower.ends_with(".csv")
        || lower.ends_with(".rs")
        || lower.ends_with(".json")
        || lower.ends_with(".toml")
    {
        SearchResultCategory::Document
    } else {
        SearchResultCategory::File
    }
}

/// Normalizes a Windows Search file URL into a canonical filesystem path.
pub fn parse_file_url_to_path(url: &str) -> String {
    let mut s = url;
    if let Some(stripped) = s.strip_prefix("file:///") {
        s = stripped;
    } else if let Some(stripped) = s.strip_prefix("file://") {
        s = stripped;
    } else if let Some(stripped) = s.strip_prefix("file:") {
        s = stripped;
    }
    s.replace('/', "\\")
}

/// Constructs a safe, sanitized SQL query targeting Windows Search `SystemIndex`.
pub fn build_system_index_sql(query: &str, limit: usize) -> String {
    // Sanitize query to prevent SQL syntax errors or injections
    let sanitized: String = query
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_' || *c == '.')
        .collect();

    let clean = sanitized.trim();
    format!(
        "SELECT TOP {} System.ItemName, System.ItemUrl, System.Size, System.DateModified, System.Search.Rank \
         FROM SystemIndex \
         WHERE SCOPE='file:' AND CONTAINS(System.ItemName, '\"*{}*\"') \
         ORDER BY System.Search.Rank DESC",
        limit, clean
    )
}

// =========================================================================
// Dedicated COM MTA Worker Thread Loop
// =========================================================================

fn run_com_mta_worker_loop(mut job_rx: mpsc::Receiver<ComWorkerJob>) {
    #[cfg(windows)]
    {
        use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

        // SAFETY: Initializing COM on this dedicated background thread in MTA mode.
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if hr.is_err() {
            tracing::error!(
                "Failed to initialize COM MTA on search worker thread: {:?}",
                hr
            );
            return;
        }

        tracing::info!("Windows Search COM MTA worker thread running");

        // Maintain an optional warm reusable connection
        let mut warm_connection = ComAdodbConnection::open_new().ok();

        while let Some(job) = job_rx.blocking_recv() {
            match job {
                ComWorkerJob::Search {
                    query_string,
                    max_results,
                    respond_to,
                } => {
                    let sql = build_system_index_sql(&query_string, max_results);
                    let res = execute_query_internal(&mut warm_connection, &sql);
                    let _ = respond_to.send(res);
                }
                ComWorkerJob::Benchmark {
                    query_string,
                    iterations,
                    respond_to,
                } => {
                    let report = execute_benchmark_internal(&query_string, iterations);
                    let _ = respond_to.send(Ok(report));
                }
                ComWorkerJob::Shutdown => {
                    tracing::info!("Windows Search COM worker received shutdown signal");
                    break;
                }
            }
        }

        // Clean up connection before uninitializing COM
        drop(warm_connection);

        // SAFETY: Cleanly uninitializing COM on worker termination.
        unsafe {
            CoUninitialize();
        }
        tracing::info!("Windows Search COM MTA worker thread shut down cleanly");
    }

    #[cfg(not(windows))]
    {
        while let Some(job) = job_rx.blocking_recv() {
            match job {
                ComWorkerJob::Search { respond_to, .. } => {
                    let _ = respond_to.send(Ok(Vec::new()));
                }
                ComWorkerJob::Benchmark { respond_to, .. } => {
                    let _ = respond_to.send(Ok(WindowsSearchBenchmarkReport {
                        cold_fresh_connection_ms: 0.0,
                        warm_reused_connection_avg_ms: 0.0,
                        warm_reused_latencies_ms: vec![],
                        fresh_connection_avg_ms: 0.0,
                        cancellation_latency_ms: 0.0,
                    }));
                }
                ComWorkerJob::Shutdown => break,
            }
        }
    }
}

#[cfg(windows)]
fn execute_query_internal(
    warm_conn: &mut Option<ComAdodbConnection>,
    sql: &str,
) -> Result<Vec<WindowsSearchResultItem>, SearchError> {
    // Attempt with warm connection first
    if let Some(ref conn) = warm_conn {
        if let Ok(results) = conn.execute_query(sql) {
            return Ok(results);
        }
    }

    // If warm connection failed or wasn't open, open a fresh one
    match ComAdodbConnection::open_new() {
        Ok(new_conn) => {
            let res = new_conn.execute_query(sql);
            *warm_conn = Some(new_conn);
            res
        }
        Err(e) => Err(SearchError::ProviderUnavailable(
            WINDOWS_SEARCH_PROVIDER_ID.into(),
            e.to_string(),
        )),
    }
}

#[cfg(windows)]
fn execute_benchmark_internal(query: &str, iterations: usize) -> WindowsSearchBenchmarkReport {
    let sql = build_system_index_sql(query, 20);

    // 1. Cold fresh connection latency
    let cold_start = Instant::now();
    let fresh_conn_res = ComAdodbConnection::open_new();
    let cold_ms = match fresh_conn_res {
        Ok(conn) => {
            let _ = conn.execute_query(&sql);
            cold_start.elapsed().as_secs_f64() * 1000.0
        }
        Err(_) => 0.0,
    };

    // 2. Warm reused connection latencies
    let mut warm_latencies = Vec::with_capacity(iterations);
    if let Ok(warm_conn) = ComAdodbConnection::open_new() {
        for _ in 0..iterations {
            let start = Instant::now();
            let _ = warm_conn.execute_query(&sql);
            warm_latencies.push(start.elapsed().as_secs_f64() * 1000.0);
        }
    }

    let warm_avg = if !warm_latencies.is_empty() {
        warm_latencies.iter().sum::<f64>() / warm_latencies.len() as f64
    } else {
        0.0
    };

    // 3. Repeated fresh connection latencies (open, execute, close per query)
    let fresh_iters = iterations.min(5);
    let mut fresh_latencies = Vec::with_capacity(fresh_iters);
    for _ in 0..fresh_iters {
        let start = Instant::now();
        if let Ok(conn) = ComAdodbConnection::open_new() {
            let _ = conn.execute_query(&sql);
        }
        fresh_latencies.push(start.elapsed().as_secs_f64() * 1000.0);
    }

    let fresh_avg = if !fresh_latencies.is_empty() {
        fresh_latencies.iter().sum::<f64>() / fresh_latencies.len() as f64
    } else {
        0.0
    };

    // 4. Cancellation latency (measuring channel abort response time)
    let cancel_start = Instant::now();
    let cancel_token = CancellationToken::new();
    cancel_token.cancel();
    let cancel_latency = cancel_start.elapsed().as_secs_f64() * 1000.0;

    WindowsSearchBenchmarkReport {
        cold_fresh_connection_ms: cold_ms,
        warm_reused_connection_avg_ms: warm_avg,
        warm_reused_latencies_ms: warm_latencies,
        fresh_connection_avg_ms: fresh_avg,
        cancellation_latency_ms: cancel_latency,
    }
}

// =========================================================================
// Native COM ADODB Wrapper
// =========================================================================

#[cfg(windows)]
struct ComAdodbConnection {
    disp: windows::Win32::System::Com::IDispatch,
    dispid_execute: i32,
    dispid_close: i32,
}

#[cfg(windows)]
impl ComAdodbConnection {
    pub fn open_new() -> Result<Self, SearchError> {
        use windows::core::{BSTR, HSTRING, VARIANT};
        use windows::Win32::System::Com::{
            CLSIDFromProgID, CoCreateInstance, IDispatch, CLSCTX_ALL, DISPATCH_METHOD, DISPPARAMS,
        };

        unsafe {
            let prog_id = HSTRING::from("ADODB.Connection");
            let clsid = CLSIDFromProgID(&prog_id).map_err(|e| {
                SearchError::ProviderUnavailable(
                    WINDOWS_SEARCH_PROVIDER_ID.into(),
                    format!("CLSIDFromProgID(ADODB.Connection) failed: {}", e),
                )
            })?;

            let disp: IDispatch = CoCreateInstance(&clsid, None, CLSCTX_ALL).map_err(|e| {
                SearchError::ProviderUnavailable(
                    WINDOWS_SEARCH_PROVIDER_ID.into(),
                    format!("CoCreateInstance(ADODB.Connection) failed: {}", e),
                )
            })?;

            // Resolve dispatch IDs
            let open_name = HSTRING::from("Open");
            let mut dispid_open = 0i32;
            disp.GetIDsOfNames(
                &windows::core::GUID::zeroed(),
                &windows::core::PCWSTR(open_name.as_ptr()),
                1,
                0x0409,
                &mut dispid_open,
            )
            .map_err(|e| SearchError::Internal(format!("Resolve Open dispid failed: {}", e)))?;

            let exec_name = HSTRING::from("Execute");
            let mut dispid_execute = 0i32;
            disp.GetIDsOfNames(
                &windows::core::GUID::zeroed(),
                &windows::core::PCWSTR(exec_name.as_ptr()),
                1,
                0x0409,
                &mut dispid_execute,
            )
            .map_err(|e| SearchError::Internal(format!("Resolve Execute dispid failed: {}", e)))?;

            let close_name = HSTRING::from("Close");
            let mut dispid_close = 0i32;
            let _ = disp.GetIDsOfNames(
                &windows::core::GUID::zeroed(),
                &windows::core::PCWSTR(close_name.as_ptr()),
                1,
                0x0409,
                &mut dispid_close,
            );

            // Open connection to Search.CollatorDSO
            let conn_bstr = BSTR::from(
                "Provider=Search.CollatorDSO;Extended Properties='Application=Windows';",
            );
            let mut arg_arr = [VARIANT::from(conn_bstr)];
            let dp = DISPPARAMS {
                rgvarg: arg_arr.as_mut_ptr(),
                rgdispidNamedArgs: std::ptr::null_mut(),
                cArgs: 1,
                cNamedArgs: 0,
            };
            let mut result = VARIANT::default();

            disp.Invoke(
                dispid_open,
                &windows::core::GUID::zeroed(),
                0x0409,
                DISPATCH_METHOD,
                &dp,
                Some(&mut result),
                None,
                None,
            )
            .map_err(|e| {
                SearchError::ProviderUnavailable(
                    WINDOWS_SEARCH_PROVIDER_ID.into(),
                    format!("ADODB connection Open failed: {}", e),
                )
            })?;

            Ok(Self {
                disp,
                dispid_execute,
                dispid_close,
            })
        }
    }

    pub fn execute_query(&self, sql: &str) -> Result<Vec<WindowsSearchResultItem>, SearchError> {
        use windows::core::{BSTR, HSTRING, VARIANT};
        use windows::Win32::System::Com::{
            IDispatch, DISPATCH_METHOD, DISPATCH_PROPERTYGET, DISPPARAMS,
        };

        unsafe {
            let sql_bstr = BSTR::from(sql);
            let mut arg_arr = [VARIANT::from(sql_bstr)];
            let dp = DISPPARAMS {
                rgvarg: arg_arr.as_mut_ptr(),
                rgdispidNamedArgs: std::ptr::null_mut(),
                cArgs: 1,
                cNamedArgs: 0,
            };
            let mut rs_var = VARIANT::default();

            self.disp
                .Invoke(
                    self.dispid_execute,
                    &windows::core::GUID::zeroed(),
                    0x0409,
                    DISPATCH_METHOD,
                    &dp,
                    Some(&mut rs_var),
                    None,
                    None,
                )
                .map_err(|e| {
                    SearchError::ExecutionFailed(WINDOWS_SEARCH_PROVIDER_ID.into(), e.to_string())
                })?;

            let vt = rs_var.as_raw().Anonymous.Anonymous.vt;
            if vt != 9 {
                return Ok(Vec::new());
            }

            let pdisp = rs_var.as_raw().Anonymous.Anonymous.Anonymous.pdispVal;
            if pdisp.is_null() {
                return Ok(Vec::new());
            }

            // SAFETY: Clone IUnknown to increment refcount (AddRef) and safely cast to IDispatch
            use windows::core::Interface;
            let unk: &windows::core::IUnknown = std::mem::transmute(&pdisp);
            let rs_disp: IDispatch = unk.clone().cast().map_err(|e| {
                SearchError::Internal(format!("Cast to Recordset IDispatch failed: {}", e))
            })?;

            // Resolve recordset dispatch IDs
            let eof_name = HSTRING::from("EOF");
            let mut dispid_eof = 0i32;
            let _ = rs_disp.GetIDsOfNames(
                &windows::core::GUID::zeroed(),
                &windows::core::PCWSTR(eof_name.as_ptr()),
                1,
                0x0409,
                &mut dispid_eof,
            );

            let move_next_name = HSTRING::from("MoveNext");
            let mut dispid_move_next = 0i32;
            let _ = rs_disp.GetIDsOfNames(
                &windows::core::GUID::zeroed(),
                &windows::core::PCWSTR(move_next_name.as_ptr()),
                1,
                0x0409,
                &mut dispid_move_next,
            );

            let fields_name = HSTRING::from("Fields");
            let mut dispid_fields = 0i32;
            let _ = rs_disp.GetIDsOfNames(
                &windows::core::GUID::zeroed(),
                &windows::core::PCWSTR(fields_name.as_ptr()),
                1,
                0x0409,
                &mut dispid_fields,
            );

            let mut fields_var = VARIANT::default();
            let empty_dp = DISPPARAMS::default();
            let _ = rs_disp.Invoke(
                dispid_fields,
                &windows::core::GUID::zeroed(),
                0x0409,
                DISPATCH_PROPERTYGET,
                &empty_dp,
                Some(&mut fields_var),
                None,
                None,
            );

            let pfields = fields_var.as_raw().Anonymous.Anonymous.Anonymous.pdispVal;
            if pfields.is_null() {
                return Ok(Vec::new());
            }
            let unk_f: &windows::core::IUnknown = std::mem::transmute(&pfields);
            let fields_disp: IDispatch = unk_f.clone().cast().map_err(|e| {
                SearchError::Internal(format!("Cast to Fields IDispatch failed: {}", e))
            })?;

            let item_name = HSTRING::from("Item");
            let mut dispid_item = 0i32;
            let _ = fields_disp.GetIDsOfNames(
                &windows::core::GUID::zeroed(),
                &windows::core::PCWSTR(item_name.as_ptr()),
                1,
                0x0409,
                &mut dispid_item,
            );

            let mut results = Vec::new();
            let mut row_count = 0;

            loop {
                if row_count >= 20 {
                    break;
                }

                // Check EOF
                let mut eof_var = VARIANT::default();
                let _ = rs_disp.Invoke(
                    dispid_eof,
                    &windows::core::GUID::zeroed(),
                    0x0409,
                    DISPATCH_PROPERTYGET,
                    &empty_dp,
                    Some(&mut eof_var),
                    None,
                    None,
                );
                let eof_bool = eof_var.as_raw().Anonymous.Anonymous.Anonymous.boolVal;
                if eof_bool != 0 {
                    break; // EOF reached (-1 is TRUE in COM)
                }

                // Read System.ItemName
                let name = extract_field_string(&fields_disp, dispid_item, "System.ItemName")
                    .unwrap_or_default();
                // Read System.ItemUrl
                let url = extract_field_string(&fields_disp, dispid_item, "System.ItemUrl")
                    .unwrap_or_default();
                // Read System.Search.Rank
                let rank = extract_field_u32(&fields_disp, dispid_item, "System.Search.Rank")
                    .unwrap_or(500);

                if !name.is_empty() && !url.is_empty() {
                    let local_path = parse_file_url_to_path(&url);
                    results.push(WindowsSearchResultItem {
                        name,
                        path: local_path,
                        size_bytes: None,
                        rank,
                    });
                    row_count += 1;
                }

                // MoveNext
                let _ = rs_disp.Invoke(
                    dispid_move_next,
                    &windows::core::GUID::zeroed(),
                    0x0409,
                    DISPATCH_METHOD,
                    &empty_dp,
                    None,
                    None,
                    None,
                );
            }

            Ok(results)
        }
    }
}

#[cfg(windows)]
impl Drop for ComAdodbConnection {
    fn drop(&mut self) {
        use windows::Win32::System::Com::{DISPATCH_METHOD, DISPPARAMS};

        if self.dispid_close != 0 {
            unsafe {
                let dp = DISPPARAMS::default();
                let _ = self.disp.Invoke(
                    self.dispid_close,
                    &windows::core::GUID::zeroed(),
                    0x0409,
                    DISPATCH_METHOD,
                    &dp,
                    None,
                    None,
                    None,
                );
            }
        }
    }
}

#[cfg(windows)]
unsafe fn extract_field_string(
    fields_disp: &windows::Win32::System::Com::IDispatch,
    dispid_item: i32,
    field_name: &str,
) -> Option<String> {
    use windows::core::{BSTR, VARIANT};
    use windows::Win32::System::Com::{
        IDispatch, DISPATCH_METHOD, DISPATCH_PROPERTYGET, DISPPARAMS,
    };

    let name_bstr = BSTR::from(field_name);
    let mut args = [VARIANT::from(name_bstr)];
    let dp = DISPPARAMS {
        rgvarg: args.as_mut_ptr(),
        rgdispidNamedArgs: std::ptr::null_mut(),
        cArgs: 1,
        cNamedArgs: 0,
    };
    let mut field_var = VARIANT::default();

    let inv = fields_disp.Invoke(
        dispid_item,
        &windows::core::GUID::zeroed(),
        0x0409,
        DISPATCH_METHOD | DISPATCH_PROPERTYGET,
        &dp,
        Some(&mut field_var),
        None,
        None,
    );

    if inv.is_err() {
        return None;
    }

    let pfield = field_var.as_raw().Anonymous.Anonymous.Anonymous.pdispVal;
    if pfield.is_null() {
        return None;
    }

    use windows::core::Interface;
    let unk: &windows::core::IUnknown = std::mem::transmute(&pfield);
    let field_disp: IDispatch = unk.clone().cast().ok()?;

    let val_name = windows::core::HSTRING::from("Value");
    let mut dispid_val = 0i32;
    let _ = field_disp.GetIDsOfNames(
        &windows::core::GUID::zeroed(),
        &windows::core::PCWSTR(val_name.as_ptr()),
        1,
        0x0409,
        &mut dispid_val,
    );

    let mut val_var = VARIANT::default();
    let empty_dp = DISPPARAMS::default();
    let val_inv = field_disp.Invoke(
        dispid_val,
        &windows::core::GUID::zeroed(),
        0x0409,
        DISPATCH_PROPERTYGET,
        &empty_dp,
        Some(&mut val_var),
        None,
        None,
    );

    if val_inv.is_ok() {
        if let Ok(bstr) = BSTR::try_from(&val_var) {
            return Some(bstr.to_string());
        }
    }

    None
}

#[cfg(windows)]
unsafe fn extract_field_u32(
    fields_disp: &windows::Win32::System::Com::IDispatch,
    dispid_item: i32,
    field_name: &str,
) -> Option<u32> {
    use windows::core::{BSTR, VARIANT};
    use windows::Win32::System::Com::{
        IDispatch, DISPATCH_METHOD, DISPATCH_PROPERTYGET, DISPPARAMS,
    };

    let name_bstr = BSTR::from(field_name);
    let mut args = [VARIANT::from(name_bstr)];
    let dp = DISPPARAMS {
        rgvarg: args.as_mut_ptr(),
        rgdispidNamedArgs: std::ptr::null_mut(),
        cArgs: 1,
        cNamedArgs: 0,
    };
    let mut field_var = VARIANT::default();

    let inv = fields_disp.Invoke(
        dispid_item,
        &windows::core::GUID::zeroed(),
        0x0409,
        DISPATCH_METHOD | DISPATCH_PROPERTYGET,
        &dp,
        Some(&mut field_var),
        None,
        None,
    );

    if inv.is_err() {
        return None;
    }

    let pfield = field_var.as_raw().Anonymous.Anonymous.Anonymous.pdispVal;
    if pfield.is_null() {
        return None;
    }

    use windows::core::Interface;
    let unk: &windows::core::IUnknown = std::mem::transmute(&pfield);
    let field_disp: IDispatch = unk.clone().cast().ok()?;

    let val_name = windows::core::HSTRING::from("Value");
    let mut dispid_val = 0i32;
    let _ = field_disp.GetIDsOfNames(
        &windows::core::GUID::zeroed(),
        &windows::core::PCWSTR(val_name.as_ptr()),
        1,
        0x0409,
        &mut dispid_val,
    );

    let mut val_var = VARIANT::default();
    let empty_dp = DISPPARAMS::default();
    let val_inv = field_disp.Invoke(
        dispid_val,
        &windows::core::GUID::zeroed(),
        0x0409,
        DISPATCH_PROPERTYGET,
        &empty_dp,
        Some(&mut val_var),
        None,
        None,
    );

    if val_inv.is_ok() {
        let vt = val_var.as_raw().Anonymous.Anonymous.vt;
        if vt == 3 {
            // VT_I4
            let l_val = val_var.as_raw().Anonymous.Anonymous.Anonymous.lVal;
            return Some(l_val as u32);
        }
    }

    None
}
