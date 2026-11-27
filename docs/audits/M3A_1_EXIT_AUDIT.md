# AsterShell — Milestone M3A.1 Independent Exit Audit
## Security, Supervisor Lifecycle, Job Object Semantics & Real Window Integration

**Audit Date:** November 26, 2026  
**Auditor:** Independent Architecture, Security & Systems Engineering Reviewer  
**Audit Mode:** STRICTLY READ-ONLY ADVERSARIAL AUDIT  
**Repository:** `medhansh5/AsterShell` (`https://github.com/medhansh5/AsterShell.git`)  
**Base Branch / Commit:** `main` (`d5d6dc5374d570cf1822de1901a28a91eb2f1ff9`)  
**Audit Branch / Commit:** `feat/m3a-taskbar-window-foundation` (`ec26bc9da027a3e8e87a5a77202df2e75a8b5b66`)  
**Pull Request:** [#1 — feat(m3a): implement native taskbar window foundation](https://github.com/medhansh5/AsterShell/pull/1) (Status: OPEN, UNMERGED)  
**Reported CI Run:** Run ID [45847407746](https://github.com/medhansh5/AsterShell/actions/runs/45847407746) (Status: SUCCESS, Duration: 3m 14s)  
**Previous Audit:** `docs/audits/M3A_EXIT_AUDIT.md`  
**Underlying Implementation Report:** `docs/milestones/m3a-1-engineering-report.md`  

---

## 1. Executive Summary & Verdict

### Verdict: REJECT M3A.1

Milestone M3A.1 represents a substantial engineering effort that successfully resolves several major architectural and testing deficits identified in the initial M3A exit audit. Specifically:
- The insecure standalone named pipe server (`\\.\pipe\astershell-taskbar-*`) has been completely removed.
- Taskbar child processes are now spawned directly into Windows Job Objects with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
- Genuine process containment was verified using the Win32 `IsProcessInJob` API on real operating system process handles.
- A genuine Slint taskbar window is now instantiated and realized in acceptance testing, with its native Win32 `HWND` styles, extended styles, and window flags directly queried and validated against shell specifications.
- `apps/shell-host` has been updated to automatically register and supervise `ComponentId::Taskbar` based on configuration.

However, an adversarial, line-by-line inspection of the implementation and runtime behavior reveals **two blocking P1 architectural and verification defects**, alongside an unhandled **P2 startup race condition**:

1. **CRITICAL ARCHITECTURAL DEFECT (P1 — SEC-01 Protocol Direction Inversion & Dead Code):**
   In `apps/taskbar/src/main.rs`, the taskbar client attempts to read incoming `RequestEnvelope` messages from the supervisor named pipe, expecting the supervisor to send an `IpcRequest::Shutdown` message to trigger graceful taskbar shutdown. However, in `apps/shell-host/src/main.rs`, the supervisor's named pipe server operates on a strictly client-to-server request/response model: it **only reads `RequestEnvelope` and only writes `ResponseEnvelope`**. The supervisor server **never** sends `RequestEnvelope` to connected clients. Furthermore, `crates/process-supervisor/src/lib.rs` (`shutdown_all`) does not utilize IPC messaging at all; it immediately issues a hard Win32 `TerminateProcess` call via `child.start_kill()`. Consequently, the graceful IPC shutdown code path in `taskbar` is **completely unreachable dead code**.
   Worse, `shell-host`'s `handle_client_connection` handles incoming `IpcRequest::Shutdown` by executing `daemon_shutdown_tx.send(())`—meaning that any client sending a shutdown request will terminate the **shell host daemon**, rather than shutting down the client.
2. **CRITICAL VERIFICATION INCOMPLETENESS (P1 — LIFECYCLE-01 Masked Kill-on-Close Semantics):**
   In `apps/taskbar/tests/taskbar_foundation.rs` (`test_ac_m3a_01_real_process_containment_and_is_process_in_job`), the test attempts to verify process containment. While it successfully demonstrates that the spawned child process handle is in the Job Object via `IsProcessInJob`, the test terminates the child process by invoking `supervisor.shutdown_all(Duration::from_millis(1500)).await`. Because `shutdown_all` explicitly calls `child.start_kill()` (Win32 `TerminateProcess`), the process termination was caused by explicit manual termination, **not by the kernel-level `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` mechanism**. The test fails to verify that dropping or closing the Job Object handle automatically terminates orphaned child processes at the kernel level without manual `TerminateProcess` intervention.
3. **INTEGRATION RACE CONDITION (P2 — INTEGRATION-01 Child Spawn Prior to Pipe Listener Creation):**
   In `apps/shell-host/src/main.rs` (`ShellHostDaemon::run()`), `self.supervisor.spawn(ComponentId::Taskbar)` is called at line 103, whereas the named pipe server listener `ipc_task` is not spawned until line 121. Because child process creation proceeds concurrently with daemon initialization, the taskbar executable regularly attempts to connect to `\\.\pipe\astershell-ipc-{hash}` before the server pipe instance exists. Because the taskbar has no reconnection or retry loop, it logs `"Supervisor offline or unreachable; continuing in standalone companion mode"` (observed repeatedly in integration test traces) and runs permanently unmanaged without IPC.

Under the authoritative audit charter, any unresolved or partially resolved P1 finding mandates a **REJECTION** of the milestone. An **M3A.2** corrective engineering pass is strictly required prior to merging PR #1 into `main` and prior to authorizing Milestone M3B.

---

## 2. Finding Remediation Ledger

The table below summarizes the remediation status of all six findings originally identified in `docs/audits/M3A_EXIT_AUDIT.md`.

| Finding ID | Severity | M3A Original Finding | M3A.1 Claimed Remediation | Audit Evaluation | Status |
|---|---|---|---|---|---|
| **SEC-01** | **P1** | Standalone taskbar control pipe accepted unauthenticated `Shutdown` requests from any local process. | Removed standalone pipe server and `--pipe` CLI flag. Taskbar connects as authenticated client to supervisor pipe. | **PARTIALLY RESOLVED.** Standalone pipe removed (fixing unauthenticated local access). However, supervisor-to-child shutdown protocol is architecturally inverted and dead code; supervisor `shutdown_all` bypasses IPC; client shutdown triggers daemon termination. | ⚠️ **PARTIAL (BLOCKED)** |
| **LIFECYCLE-01** | **P1** | `test_ac_m3a_01` did not spawn a real process or test Job Object membership. | Test spawns real compiled `taskbar.exe` with `--test-child --no-supervisor`, calls `IsProcessInJob`. | **PARTIALLY RESOLVED.** Real process spawn and `IsProcessInJob` verification are fully proven. However, kill-on-close kernel termination was masked by manual `child.start_kill()`. | ⚠️ **PARTIAL (BLOCKED)** |
| **INTEGRATION-01** | **P2** | `shell-host` did not automatically start or supervise `apps/taskbar`. | Added `taskbar.autostart` config check and automatic spawn in `ShellHostDaemon::run()`. | **PARTIALLY RESOLVED.** Component registration and auto-start implemented, but child is spawned before the named pipe listener is created, causing a startup connection race. | ⚠️ **PARTIAL** |
| **TEST-01** | **P2** | Native window tests operated on mock handles or uninstantiated widgets. | Added `test_ac_m3a_02_and_03_real_slint_taskbar_window_native_styles` with real Slint event loop and Win32 HWND. | **RESOLVED.** Real Slint window instantiated and shown; Win32 styles and extended styles verified on real OS window handle. | ✅ **RESOLVED** |
| **WINDOW-01** | **P2** | Potential title bar flicker; non-activating visibility unverified. | Masked out `WS_CAPTION \| WS_THICKFRAME \| WS_SYSMENU`; added `ShowWindow(SW_SHOWNA)` prior to `SetWindowPos`. | **RESOLVED.** Window style stripping verified; non-activating visibility call sequence confirmed idempotent. | ✅ **RESOLVED** |
| **DOC-01** | **P3** | Documentation claimed bidirectional IPC lifecycle that did not exist. | Authored `docs/milestones/m3a-1-engineering-report.md`. | **PARTIALLY RESOLVED.** Engineering report written, but PR #1 GitHub description remains stale and still asserts the defunct dual-pipe model. | ⚠️ **PARTIAL** |

---

## 3. Detailed Technical Analysis

### 3.1. SEC-01 Protocol Direction Mismatch & Unreachable IPC Shutdown

The M3A.1 implementation report asserts:
> *"The taskbar process is now purely a client that connects to the supervisor's authenticated named pipe... When an `IpcRequest::Shutdown` envelope is received from the supervisor, taskbar breaks the event loop and gracefully exits."* (`m3a-1-engineering-report.md`, Section 2)

An inspection of the actual source code demonstrates that this statement is false and describes an architecture that does not function:

#### Code Analysis: `apps/taskbar/src/main.rs`
```rust
// Lines 247-277 in apps/taskbar/src/main.rs
info!(supervisor_pipe = %sup_addr, "Connecting to supervisor IPC...");
match transport::connect_pipe_client(&sup_addr).await {
    Ok(mut client) => {
        let handshake = RequestEnvelope::new(IpcRequest::Handshake {
            client_version: 1,
            client_pid: current_pid,
        });
        if let Err(e) = transport::write_framed_message(&mut client, &handshake).await {
            warn!(error = %e, "Supervisor handshake write failed");
        } else {
            match transport::read_framed_message::<ResponseEnvelope>(&mut client).await {
                Ok(resp) => {
                    info!(response = ?resp.response, "Taskbar supervisor mutual handshake confirmed");
                    // Enter wait loop for supervisor requests
                    loop {
                        match transport::read_framed_message::<RequestEnvelope>(&mut client).await {
                            Ok(envelope) => match envelope.request {
                                IpcRequest::Shutdown => {
                                    info!("Received supervisor shutdown command; breaking taskbar loop");
                                    break;
                                }
                                other => {
                                    debug!(request = ?other, "Ignored supervisor request");
                                }
                            },
                            Err(_) => {
                                info!("Supervisor pipe closed; terminating taskbar lifecycle");
                                break;
                            }
                        }
                    }
                }
                ...
            }
        }
    }
}
```

#### Code Analysis: `apps/shell-host/src/main.rs`
```rust
// Lines 207-285 in apps/shell-host/src/main.rs (handle_client_connection)
async fn handle_client_connection(
    mut stream: NamedPipeServer,
    ...
) -> Result<()> {
    // Phase 1: Wait for Handshake from client
    let handshake_req: RequestEnvelope = transport::read_framed_message(&mut stream).await?;
    ...
    // Phase 2: Loop reading RequestEnvelope from client and writing ResponseEnvelope
    loop {
        let req_env: RequestEnvelope = match transport::read_framed_message(&mut stream).await {
            Ok(env) => env,
            Err(_) => break, // Client disconnected
        };

        let response = match req_env.request {
            IpcRequest::Shutdown => {
                info!("Shutdown request received from authenticated child");
                let _ = daemon_shutdown_tx.send(());
                IpcResponse::ShutdownAck
            }
            ...
        };

        let resp_env = ResponseEnvelope::new(req_env.request_id, response);
        transport::write_framed_message(&mut stream, &resp_env).await?;
    }
    Ok(())
}
```

#### Defect Findings:
1. **Unidirectional Server Loop:** `shell-host` acts as a request/response server. It only reads `RequestEnvelope` and only writes `ResponseEnvelope`. It never has the capability to write a `RequestEnvelope` to the client.
2. **Dead Code in Taskbar:** Because `shell-host` never writes a `RequestEnvelope` down the pipe, the taskbar's `match envelope.request { IpcRequest::Shutdown => ... }` will **never trigger**. The only reason the taskbar terminates over IPC is when the pipe stream is severed (`Err(_) => info!("Supervisor pipe closed...")`).
3. **Supervisor `shutdown_all` Bypasses IPC:** In `crates/process-supervisor/src/lib.rs` (lines 305-338), `shutdown_all` iterates over child processes and executes `child.start_kill()`. It has no reference to the IPC server, no connection map, and sends zero IPC frames.
4. **Dangerous Inverse Shutdown Semantics:** In `shell-host`, receiving `IpcRequest::Shutdown` on the pipe executes `daemon_shutdown_tx.send(())`, which instructs the **shell-host daemon** to shut down. If a child process were to send a shutdown request, it would crash the entire shell infrastructure rather than shutting itself down.

---

### 3.2. LIFECYCLE-01 Process Containment & Job Object Verification

M3A.1 claimed to fully satisfy `AC-M3A-01` by verifying that `taskbar.exe` is placed into a Windows Job Object configured with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.

#### Analysis of `apps/taskbar/tests/taskbar_foundation.rs`:
```rust
// Lines 75-115 in apps/taskbar/tests/taskbar_foundation.rs
#[tokio::test]
async fn test_ac_m3a_01_real_process_containment_and_is_process_in_job() -> Result<()> {
    ...
    supervisor.spawn(ComponentId::Taskbar).await?;
    
    // 1. Verify child PID exists
    let child_pid = supervisor.get_child_pid(ComponentId::Taskbar).await
        .expect("Taskbar child PID must exist");
    assert!(child_pid > 0);

    // 2. Verify IsProcessInJob via real Win32 API
    let in_job = supervisor.is_child_in_job(ComponentId::Taskbar).await?;
    assert!(in_job, "AC-M3A-01: Child process must be confirmed in Windows Job Object");

    // 3. Clean shutdown
    supervisor.shutdown_all(Duration::from_millis(1500)).await?;

    let is_running = supervisor.is_running(ComponentId::Taskbar).await;
    assert!(!is_running, "AC-M3A-01: Child process must terminate on supervisor shutdown");
    Ok(())
}
```

#### Defect Findings:
1. **Real Process Spawn (PASS):** The test locates `target/debug/taskbar.exe` and spawns the real binary with `--test-child --no-supervisor`.
2. **Win32 `IsProcessInJob` Verification (PASS):** The test invokes `supervisor.is_child_in_job`, which calls the Win32 `IsProcessInJob` API on the child's raw OS handle, confirming genuine kernel Job Object assignment.
3. **Kill-On-Close Verification (INCOMPLETE / MASKED):**
   In step 3, `supervisor.shutdown_all(Duration::from_millis(1500)).await` is called. Looking inside `process_supervisor/src/lib.rs`:
   ```rust
   pub async fn shutdown_all(&self, timeout: Duration) -> Result<()> {
       let mut children = self.children.lock().await;
       for (id, child) in children.iter_mut() {
           if let Err(e) = child.start_kill() { ... }
       }
       ...
   }
   ```
   `child.start_kill()` directly issues a Win32 `TerminateProcess` call.
   Therefore, the test does **not** verify that dropping or closing the Job Object handle automatically kills child processes. The child was explicitly killed by the supervisor calling `TerminateProcess`. To genuinely verify `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, the test must drop the Job Object handle (or allow the supervisor struct to drop) while the child process is alive, without calling `start_kill()`, and assert that Windows kernel process termination occurs.

---

### 3.3. INTEGRATION-01 Daemon Lifecycle & Startup Race Condition

M3A.1 added taskbar supervision into `ShellHostDaemon::run()`. However, the initialization ordering introduces an unhandled race condition:

#### Code Analysis: `apps/shell-host/src/main.rs`
```rust
// Lines 95-125 in apps/shell-host/src/main.rs
pub async fn run(self) -> Result<()> {
    let mut self_mut = self;
    self_mut.register_default_components().await?;

    // Line 103: SPAWN TASKBAR CHILD FIRST
    if self_mut.config.taskbar.autostart {
        if let Err(e) = self_mut.supervisor.spawn(ComponentId::Taskbar).await {
            error!(error = %e, "Failed to spawn taskbar during shell-host startup");
        }
    }

    // Line 121: SPAWN NAMED PIPE SERVER LISTENER AFTERWARDS
    let ipc_task = tokio::spawn(async move {
        loop {
            match listener.accept().await { ... }
        }
    });
}
```

#### Defect Findings:
1. **Startup Ordering Inversion:** `taskbar` is spawned before `listener.accept()` is active. On multi-core systems, `taskbar.exe` frequently initializes and attempts `transport::connect_pipe_client` before the named pipe instance has been created by the daemon.
2. **Missing Reconnection Logic:** In `apps/taskbar/src/main.rs` (lines 305-312), when connection fails with `os error 2` (`The system cannot find the file specified`), the taskbar immediately logs:
   `"Supervisor offline or unreachable; continuing in standalone companion mode"`
   It never retries. As a result, in production environments and tests, `taskbar` frequently detaches into standalone companion mode at boot and never establishes IPC with `shell-host`.
3. **Integration Test Proof:** During the full workspace integration test run, the following log was emitted:
   `2026-11-26T15:38:15.801505Z INFO ThreadId(13) taskbar: Supervisor offline or unreachable; continuing in standalone companion mode error=IPC communication error: Failed to connect to pipe '\\.\pipe\astershell-ipc-940': The system cannot find the file specified. (os error 2)`

---

### 3.4. TEST-01 Native-Window Acceptance Test Classification & Execution

The previous exit audit flagged that M3A acceptance tests were testing in-memory structs rather than real Slint windows. M3A.1 added `test_ac_m3a_02_and_03_real_slint_taskbar_window_native_styles`.

#### Test Suite Ledger (`apps/taskbar/tests/taskbar_foundation.rs`):

| Test Identifier | Test Target | Execution Category | Win32 Handle Realized? |
|---|---|---|---|
| `test_ac_m3a_01_real_process_containment_and_is_process_in_job` | Process Supervisor & Job Object | **Real Process + Win32 API** | Yes (`GetProcessId`, `IsProcessInJob`) |
| `test_ac_m3a_02_native_frameless_window_styles_unit` | `configure_taskbar_native_styles` | **In-Memory Bitwise Unit** | No (Mock integer `0x00CF0000`) |
| `test_ac_m3a_03_extended_shell_styles_and_alt_tab_exclusion_unit` | `configure_taskbar_native_styles` | **In-Memory Bitwise Unit** | No (Mock integer `0x00040000`) |
| `test_ac_m3a_02_and_03_real_slint_taskbar_window_native_styles` | Slint Taskbar Window + Native Config | **Real Slint Window + Win32 HWND** | **Yes (`HWND` obtained via `with_winit_window`)** |
| `test_ac_m3a_04_zero_desktop_work_area_modification` | Work Area Non-Interference | **Real Win32 System API** | Yes (`SystemParametersInfoW`) |
| `test_ac_m3a_05_signed_primary_monitor_placement` | Taskbar Coordinate Math | **Pure Math Unit Test** | No (Pure coordinate calculations) |
| `test_ac_m3a_06_dwm_backdrop_realization_and_fallback` | DWM Composition & Backdrop Fallback | **Real Win32 API / Mock HWND** | Yes (`DwmIsCompositionEnabled`) |
| `test_ac_m3a_07_supervisor_authenticated_ipc_and_shutdown` | Supervisor Mutual IPC Handshake | **Real Named Pipe IPC** | Yes (Live Named Pipe Server & Client) |

#### Analysis of Real Slint Window Realization:
In `test_ac_m3a_02_and_03_real_slint_taskbar_window_native_styles`:
- `TestTaskbarWindow::new()` is called to instantiate the compiled Slint UI component.
- `win.show()` is called, followed by `slint::run_event_loop()`.
- Within a 50ms timer, `with_winit_window` is invoked to retrieve the Win32 `HWND`.
- The real window handle is passed to `configure_taskbar_native_styles`.
- Real Win32 `GetWindowLongW(hwnd, GWL_STYLE)` and `GetWindowLongW(hwnd, GWL_EXSTYLE)` are queried on the active OS window:
  - Asserted presence of `WS_POPUP`, `WS_VISIBLE`, `WS_CLIPSIBLINGS`, `WS_CLIPCHILDREN`.
  - Asserted presence of `WS_EX_TOPMOST`, `WS_EX_TOOLWINDOW`, `WS_EX_NOACTIVATE`.
  - Asserted removal of `WS_CAPTION`, `WS_THICKFRAME`, `WS_SYSMENU`, `WS_MINIMIZEBOX`, `WS_MAXIMIZEBOX`, `WS_EX_APPWINDOW`.

**Finding TEST-01 is fully RESOLVED.**

---

### 3.5. WINDOW-01 Native Style Stripping, Visibility & Idempotency

In `crates/platform-windows/src/lib.rs` (`configure_taskbar_native_styles`), the style masking logic was audited:
```rust
let stripped_style = current_style & !(
    WS_CAPTION.0
    | WS_THICKFRAME.0
    | WS_MINIMIZEBOX.0
    | WS_MAXIMIZEBOX.0
    | WS_SYSMENU.0
);
let new_style = stripped_style | WS_POPUP.0 | WS_CLIPSIBLINGS.0 | WS_CLIPCHILDREN.0;

let stripped_ex = current_ex & !WS_EX_APPWINDOW.0;
let new_ex = stripped_ex | WS_EX_TOPMOST.0 | WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0;
```
Furthermore:
1. `ShowWindow(handle, SW_SHOWNA)` is invoked to ensure visibility without stealing user focus.
2. `SetWindowPos(handle, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED | SWP_SHOWWINDOW)` flushes frame attributes and window positioning.
3. Multiple invocations on the same window handle were tested and verified to be strictly idempotent with zero style degradation or title bar flicker.

**Finding WINDOW-01 is fully RESOLVED.**

---

### 3.6. DOC-01 Documentation and Artifact Integrity

1. `docs/milestones/m3a-1-engineering-report.md` was authored and provides an accurate chronicle of the code changes made in M3A.1.
2. However, **Pull Request #1 description on GitHub** (`https://github.com/medhansh5/AsterShell/pull/1`) was never updated. It continues to describe the rejected M3A architecture, citing:
   - *"dual IPC lifecycle handling (supervisor client handshake and control pipe listener)"*
   - Unauthenticated control pipe commands.
   The PR description must be updated in GitHub to reflect the single-pipe architecture and supervisor integration.

---

## 4. Verification Evidence & Quality Gates

### 4.1. GitHub Actions CI Run Evidence
- **Run ID:** `45847407746`
- **Workflow:** `AsterShell CI`
- **Commit:** `ec26bc9da027a3e8e87a5a77202df2e75a8b5b66`
- **Status:** Completed successfully (`conclusion: success`)
- **Execution Duration:** 3m 14s
- **Job Matrix Breakdown:**
  - `Formatting`: PASS (28s)
  - `Clippy Lints`: PASS (1m 18s)
  - `Workspace Tests`: PASS (1m 24s)
  - `Documentation`: PASS (34s)

### 4.2. Local Quality Gates Verification (Executed November 26, 2026)
All quality gates were locally executed in PowerShell under Windows 11 with `$env:RUST_MIN_STACK = "16777216"`:

1. **Code Formatting:**
   ```powershell
   cargo fmt --all -- --check
   ```
   *Result:* Clean (0 formatting diffs across workspace).

2. **Compilation & Type Check:**
   ```powershell
   cargo check --workspace --all-targets --locked -j 1
   ```
   *Result:* Clean (`Finished dev profile [unoptimized + debuginfo] in 1.29s`).

3. **Static Analysis & Linting:**
   ```powershell
   cargo clippy --workspace --all-targets --locked -j 1 -- -D warnings
   ```
   *Result:* Clean (0 warnings across all targets in 0.72s).

4. **Taskbar Foundation Acceptance Suite:**
   ```powershell
   cargo test -j 1 -p taskbar --test taskbar_foundation -- --nocapture
   ```
   *Result:* `8 passed; 0 failed; 0 ignored; finished in 1.97s`.

5. **Shell-Host Integration Suite:**
   ```powershell
   cargo test -j 1 -p shell-host
   ```
   *Result:* `11 passed; 0 failed; 0 ignored; finished in 0.25s`.

6. **Process Supervisor Unit Suite:**
   ```powershell
   cargo test -j 1 -p process-supervisor
   ```
   *Result:* `4 passed; 0 failed; 0 ignored; finished in 0.00s`.

7. **Platform Windows Unit Suite:**
   ```powershell
   cargo test -j 1 -p platform-windows
   ```
   *Result:* `12 passed; 0 failed; 0 ignored; finished in 0.02s`.

8. **Full Workspace Test Suite:**
   ```powershell
   cargo test --workspace --locked -j 1
   ```
   *Result:* All 13 crates passed tests without regression in 34.72s.

---

## 5. Non-Regression & Scope Review

1. **Non-Regression:** All core Phase 0, M1A, M1B, M2, and M2.1 features remain intact. The application catalog, desktop shell host, configuration parser, diagnostics subsystem, Windows Search COM manager, and hybrid search indexing continue to compile and pass all automated tests without regression.
2. **Scope Discipline:** No out-of-scope M3B taskbar features (such as taskbar buttons, window thumbnail previews, multi-monitor tracking, or system tray integration) were prematurely introduced into the codebase.

---

## 6. Prioritized M3A.2 Corrective Action Plan

To achieve final approval and merge PR #1, the following concrete engineering corrections must be executed in Milestone **M3A.2**:

### Priority 1: Correct IPC Protocol Architecture & Shutdown Flow
1. **Unify Protocol Roles:**
   - Define whether shutdown is initiated by the supervisor dropping the pipe or via an explicit message.
   - If explicit IPC shutdown is required, `shell-host`'s supervisor pipe must support supervisor-initiated commands or notifications to children, OR `taskbar` must simply wait for pipe disconnection (`transport::read_framed_message` returning EOF) as its signal to terminate.
   - Remove the dead `match envelope.request { IpcRequest::Shutdown => ... }` client loop in `apps/taskbar/src/main.rs`.
2. **Protect Shell-Host from Child-Triggered Shutdown:**
   - In `apps/shell-host/src/main.rs` (`handle_client_connection`), reject or restrict `IpcRequest::Shutdown` so that untrusted or child components cannot trigger `daemon_shutdown_tx.send(())`.

### Priority 2: Rigorous Kernel Kill-On-Close Test
1. In `apps/taskbar/tests/taskbar_foundation.rs` (`test_ac_m3a_01`), test genuine kernel Job Object termination:
   - Spawn the supervised child process.
   - Confirm `IsProcessInJob` is `true`.
   - Explicitly drop or close the Job Object handle without calling `child.start_kill()`.
   - Poll or wait for the process handle with a timeout and assert that Windows kernel terminated the child process with exit code indicating job termination.

### Priority 3: Resolve Startup Ordering Race & Add Client Connection Retry
1. In `apps/shell-host/src/main.rs` (`ShellHostDaemon::run()`), initialize and bind the named pipe listener **before** calling `self.supervisor.spawn(ComponentId::Taskbar)`.
2. In `apps/taskbar/src/main.rs`, implement a brief exponential backoff retry loop (e.g., 5 attempts across 1000ms) when attempting to connect to the supervisor pipe before falling back to standalone companion mode.

### Priority 4: Update Pull Request Metadata
1. Update the description of PR #1 on GitHub to accurately reflect the unified single-pipe architecture, job object containment, and native Slint window validation.

---

## 7. Final Audit Conclusion

Milestone M3A.1 achieved notable progress in real window realization and Job Object integration, but it falls short of production readiness due to an inverted, non-functional IPC shutdown architecture, masked kill-on-close test coverage, and a startup connection race condition.

**Milestone M3A.1 is REJECTED.** Execution of Milestone M3A.2 is authorized and required.
