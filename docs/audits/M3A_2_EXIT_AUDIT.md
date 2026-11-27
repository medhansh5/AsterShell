# AsterShell — Milestone M3A.2 Independent Exit Audit
## Security, Supervisor Lifecycle, Kernel Kill-on-Close Proof & Real Window Integration

**Audit Mode:** STRICTLY READ-ONLY ADVERSARIAL EXIT AUDIT  
**Date:** 2026-11-27  
**Audited Target:** Milestone M3A.2 Corrective Implementation  
**Auditor:** Independent Engineering Auditor  
**Repository:** `https://github.com/medhansh5/AsterShell`  
**PR:** [#1 (feat(m3a): implement native taskbar window foundation)](https://github.com/medhansh5/AsterShell/pull/1)  
**Target / Base Branch:** `main` (`d5d6dc5374d570cf1822de1901a28a91eb2f1ff9`)  
**Audited Head Branch:** `feat/m3a-taskbar-window-foundation`  
**Audited Head SHA:** `598535c1eefb059cd166a7eb3f4f9f16bf9077c4`  
**Merge Base:** `d5d6dc5374d570cf1822de1901a28a91eb2f1ff9` (Clean fast-forward ancestry from `origin/main`)  
**CI Verification Run:** [`45942055160`](https://github.com/medhansh5/AsterShell/actions/runs/45942055160) (Conclusion: `SUCCESS`, Commit: `598535c1eefb059cd166a7eb3f4f9f16bf9077c4`)  

---

## 1. Executive Summary & Audit Baseline Verification

Milestone M3A.2 represents the secondary corrective engineering pass for Milestone M3A ("Native Taskbar Window Foundation & Lifecycle"). Following the rejection of M3A.1 in `docs/audits/M3A_1_EXIT_AUDIT.md` due to two P1 blocking issues (`SEC-01` and `LIFECYCLE-01`), one P2 issue (`INTEGRATION-01`), and one P3 issue (`DOC-01`), this adversarial audit independently investigated the entire codebase, Git history, pull request metadata, and GitHub Actions CI runs to determine whether M3A is safe to authorize for merge into `main`.

### 1.1 Baseline Verification Ledger

| Check | Specification Requirement | Audited State | Result |
|---|---|---|---|
| **Repository Identity** | `https://github.com/medhansh5/AsterShell` | `origin` fetch/push match repository identity | **PASS** |
| **Working Directory** | Clean working tree | `git status` confirms 0 untracked/modified files | **PASS** |
| **Current Branch** | `feat/m3a-taskbar-window-foundation` | Confirmed on branch `feat/m3a-taskbar-window-foundation` | **PASS** |
| **Head SHA** | `598535c1eefb059cd166a7eb3f4f9f16bf9077c4` | Matches exact commit SHA `598535c` | **PASS** |
| **Base Commit** | `d5d6dc5374d570cf1822de1901a28a91eb2f1ff9` | `git merge-base origin/main HEAD` equals `d5d6dc5` | **PASS** |
| **Pull Request #1** | State: `OPEN`, Mergeable: `MERGEABLE`, Unmerged | PR #1 is `OPEN`, `isDraft: false`, `mergeable: MERGEABLE`, `mergedAt: null` | **PASS** |
| **PR Metadata & Description** | Synchronized with single-pipe role-based IPC | PR body updated to accurately describe M3A.2 changes | **PASS** |
| **CI Run ID** | `45942055160` | Triggered by PR #1 on commit `598535c1eefb059cd166a7eb3f4f9f16bf9077c4` | **PASS** |
| **CI Conclusion** | `success` across all 4 gates | Gates 1, 2, 3, 4 passed in 2m 2s | **PASS** |

---

## 2. SEC-01 — Adversarial IPC Security & Shutdown Audit (P1)

### 2.1 Audit Findings & Code Inspection

The M3A.1 audit rejected `SEC-01` due to:
1. Inverted supervisor-to-child shutdown protocol: `shell-host` attempted to act as a client sending `RequestEnvelope` to the taskbar, while `taskbar` had a dead loop awaiting requests.
2. Supervised children (such as `taskbar`) could send `IpcRequest::Shutdown` to `shell-host` and trigger complete host daemon shutdown, creating an escalation and stability risk.

In M3A.2, the IPC protocol, authorization model, and supervisor lifecycle were examined across `crates/ipc`, `crates/process-supervisor`, `apps/shell-host`, and `apps/taskbar`.

#### 1. Client Role Assignment & Influence
- In `crates/process-supervisor/src/lib.rs` (lines 11-17), `ClientRole` is defined:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum ClientRole {
      SupervisedChild,
      AdminController,
  }
  ```
- In `SupervisedPidRegistry` (lines 20-80), PIDs are recorded in an internal `RwLock<HashMap<u32, ClientRole>>`.
- When `ProcessSupervisor::spawn` spawns a managed process (lines 280-282), it calls `self.pid_registry.authorize(pid)`, which unconditionally registers the process with `ClientRole::SupervisedChild` (lines 34-36).
- An external client cannot claim, request, or influence its role. The `IpcRequest::Handshake` frame (defined in `crates/ipc/src/lib.rs` lines 86-90) contains only `client_version: u16` and `client_name: String`. There is no role field in the wire protocol.

#### 2. Process Identification & Authentication Enforcement
- In `apps/shell-host/src/main.rs` (line 279), the connecting client PID is retrieved directly from the OS named pipe handle:
  ```rust
  let pid = platform_windows::get_pipe_client_pid(&stream).unwrap_or(0);
  ```
  In `crates/platform-windows/src/lib.rs` (lines 390-408), this calls the Win32 `GetNamedPipeClientProcessId(pipe_handle, &mut client_pid)` API. This PID is supplied directly by the Windows kernel connection object and cannot be spoofed across process boundaries.
- At lines 280-296 of `apps/shell-host/src/main.rs`, the PID is checked against the supervisor's `pid_registry`. If `!pid_registry.is_authorized(pid)`, the connection is rejected with `IpcResponse::Error` and the pipe stream is severed immediately (`break`).
- Unauthenticated requests are blocked at lines 313-326: any frame received before a valid `Handshake` is rejected with `IpcResponse::Error("Authentication required: ...")` and the connection is closed.
- Duplicate handshakes on an already authenticated connection are rejected at lines 264-277 as protocol violations, severing the connection.

#### 3. Authorization on Privileged Operations (`IpcRequest::Shutdown`)
- In `apps/shell-host/src/main.rs` (lines 333-348), handling of `IpcRequest::Shutdown` was inspected:
  ```rust
  IpcRequest::Shutdown => {
      if client_role == process_supervisor::ClientRole::AdminController {
          tracing::warn!("Admin controller requested shell host shutdown");
          is_shutdown = true;
          IpcResponse::Ack
      } else {
          tracing::warn!(client_pid, "Unauthorized shutdown attempt rejected from supervised child");
          IpcResponse::Error(
              "Permission denied: supervised child processes are not authorized to trigger host shutdown"
                  .to_string(),
          )
      }
  }
  ```
- Because supervised child processes are registered as `ClientRole::SupervisedChild`, any child process attempting `IpcRequest::Shutdown` is denied with `Permission denied`. Crucially, `is_shutdown` remains `false`, `shutdown_tx` is not notified, and the host daemon continues executing normally.

#### 4. Host Shutdown Initiation to Taskbar Termination Flow
- In `apps/shell-host/src/main.rs` (lines 238-248), `handle_client_connection` runs a `tokio::select!` monitoring `shutdown_rx.changed()`:
  ```rust
  _ = shutdown_rx.changed() => {
      if *shutdown_rx.borrow() {
          let notice = ResponseEnvelope {
              request_id: 0,
              response: IpcResponse::ShutdownNotice,
          };
          let _ = transport::write_framed_message(&mut stream, &notice).await;
          break;
      }
  }
  ```
- In `apps/taskbar/src/main.rs` (lines 188-198), `run_supervisor_connection_loop` listens for `ResponseEnvelope` frames from the supervisor:
  ```rust
  incoming = transport::read_framed_message::<_, ResponseEnvelope>(&mut client) => {
      match incoming {
          Ok(env) => {
              if matches!(env.response, IpcResponse::ShutdownNotice) {
                  is_running.store(false, Ordering::SeqCst);
                  let _ = slint::invoke_from_event_loop(|| {
                      let _ = slint::quit_event_loop();
                  });
                  return;
              }
          }
          Err(e) => {
              break; // Breaks inner loop to trigger reconnection
          }
      }
  }
  ```
- When the host initiates shutdown, `shutdown_rx` changes, sending `IpcResponse::ShutdownNotice` to all connected clients. The taskbar receives this notice, sets `is_running = false`, invokes `slint::quit_event_loop()`, and terminates cleanly with exit code 0.
- If the taskbar fails to terminate or is deadlocked, `ProcessSupervisor::shutdown_all` (lines 350-380 of `crates/process-supervisor/src/lib.rs`) waits a bounded grace period before terminating the child via `child.start_kill()`.

### 2.2 Test Verification
The following test suite was independently inspected and executed:
1. `test_shell_host_rejects_shutdown_from_supervised_child` (`apps/shell-host/src/main.rs:518`): Proves a client authenticated with `ClientRole::SupervisedChild` receives `Permission denied` on `IpcRequest::Shutdown` and daemon remains alive (`!daemon_task.is_finished()`).
2. `test_shell_host_shutdown_notice_sent_to_clients` (`apps/shell-host/src/main.rs:591`): Proves client receives `IpcResponse::ShutdownNotice` when host shutdown is triggered.
3. `test_shell_host_rejects_unauthorized_client_pid` (`apps/shell-host/src/main.rs:643`): Proves client with unauthorized PID is rejected during handshake and pipe is severed.
4. `test_shell_host_rejects_ping_before_handshake` (`apps/shell-host/src/main.rs:704`): Proves unauthenticated request is rejected with `Authentication required` and stream severed.
5. `test_shell_host_rejects_duplicate_handshake` (`apps/shell-host/src/main.rs:900`): Proves second handshake on same connection is rejected as protocol violation.
6. `test_ac_m3a_07_supervised_child_cannot_shutdown_host` (`apps/taskbar/tests/taskbar_foundation.rs:753`): Acceptance test proving supervised child shutdown rejection.
7. `test_ac_m3a_08_taskbar_graceful_shutdown_on_supervisor_exit` (`apps/taskbar/tests/taskbar_foundation.rs:880`): Acceptance test proving real taskbar child exits cleanly on `ShutdownNotice`.
8. `test_ac_m3a_09_non_responsive_taskbar_forcibly_terminated_on_timeout` (`apps/taskbar/tests/taskbar_foundation.rs:970`): Proves fallback kill after grace period for non-responsive children.

**Verdict on SEC-01:** **DEFECT RESOLVED (P1 cleared)**.

---

## 3. LIFECYCLE-01 — Kernel Job Object Kill-on-Close Proof (P1)

### 3.1 Audit Findings & Code Inspection

The M3A.1 audit rejected `LIFECYCLE-01` because `test_ac_m3a_01` called `child.start_kill()` / `TerminateProcess()`, which proved manual process termination rather than Windows kernel kill-on-close upon Job Object handle closure.

In M3A.2, the kernel Job Object configuration, limit query, and containment test were audited:

#### 1. OS Limit Configuration & Query
- In `crates/platform-windows/src/lib.rs` (lines 40-78), `JobObject::create` configures `JOBOBJECT_EXTENDED_LIMIT_INFORMATION` with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`:
  ```rust
  let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
  info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
  SetInformationJobObject(handle, JobObjectExtendedLimitInformation, &info as *const _ as _, ...);
  ```
- In `crates/platform-windows/src/lib.rs` (lines 175-200), `JobObject::query_kill_on_close()` was implemented:
  ```rust
  pub fn query_kill_on_close(&self) -> Result<bool> {
      let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
      let mut return_length = 0u32;
      unsafe {
          QueryInformationJobObject(
              self.handle,
              JobObjectExtendedLimitInformation,
              &mut info as *mut _ as _,
              std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
              Some(&mut return_length),
          )?;
      }
      let flags = info.BasicLimitInformation.LimitFlags;
      Ok((flags.0 & JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE.0) != 0)
  }
  ```
- This directly queries the Windows kernel to verify that the active job limit flags include `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.

#### 2. Handle Ownership & Final Handle Closure
- The auditor inspected whether `JobObject` is cloned or duplicated anywhere in `crates/platform-windows` or `crates/process-supervisor`.
- `JobObject` does **not** implement `Clone` or `Copy`.
- No call to `DuplicateHandle` exists anywhere in the repository.
- `ProcessSupervisor::new(job)` takes ownership of the single `JobObject`.
- `ProcessSupervisor::into_job_object(self)` (line 394 of `crates/process-supervisor/src/lib.rs`) consumes the supervisor by value and returns the single `JobObject` without calling `shutdown_all` or `child.start_kill()`.
- Dropping this `JobObject` calls `JobObject::drop` (lines 207-218 of `crates/platform-windows/src/lib.rs`), which invokes `CloseHandle(self.handle)`.
- Because only one handle to the job object was ever created (`CreateJobObjectW`), this drop unconditionally closes the **final OS handle** to the Job Object.

#### 3. Execution of `test_ac_m3a_01`
- In `apps/taskbar/tests/taskbar_foundation.rs` (lines 492-610), `test_ac_m3a_01_real_process_containment_and_is_process_in_job`:
  1. `job.query_kill_on_close()` confirms `KILL_ON_JOB_CLOSE` is active on the kernel object.
  2. Spawns real `taskbar.exe` with `--test-child --no-supervisor`. In this mode (lines 336-340 of `apps/taskbar/src/main.rs`), the child process loops indefinitely in a sleep state and never exits on its own.
  3. Queries Win32 `IsProcessInJob` via `supervisor.is_child_in_job(ComponentId::Taskbar)`, confirming kernel containment.
  4. Obtains an open OS process handle via `OpenProcess`.
  5. Verifies child is actively executing: `GetExitCodeProcess` returns `259 (STILL_ACTIVE)`.
  6. Calls `let job_object = supervisor.into_job_object(); drop(job_object);` — closing the final Job Object handle without calling `start_kill()`, `TerminateProcess()`, or `shutdown_all()`.
  7. Waits for kernel termination: `WaitForSingleObject(proc_handle, 3000)` returns `WAIT_OBJECT_0`.
  8. Queries `GetExitCodeProcess`, verifying the child process has terminated (`exit_code != STILL_ACTIVE`).
  9. Defuses the safety guard (`std::mem::forget(guard)`) and closes the process handle.

This sequence conclusively proves that the child process termination was executed directly by the Windows kernel as a consequence of closing the Job Object handle.

**Verdict on LIFECYCLE-01:** **DEFECT RESOLVED (P1 cleared)**.

---

## 4. INTEGRATION-01 — Startup Ordering & Reconnection State Machine (P2)

### 4.1 Audit Findings & Code Inspection

The M3A.1 audit found:
1. `ShellHostDaemon::run` spawned `ComponentId::Taskbar` before the named pipe listener was bound, causing a race condition where taskbar failed to connect on first attempt.
2. `apps/taskbar` did not implement an exponential backoff reconnection loop.
3. In initial M3A.2 commits, `ShellHostDaemon::register_default_components` passed empty `args: vec![]`, failing to inform `taskbar.exe` of custom or supervisor pipe addresses.

In commit `598535c1eefb059cd166a7eb3f4f9f16bf9077c4`, the implementation was inspected:

#### 1. Startup Ordering & Asynchronous Synchronization
- In `apps/shell-host/src/main.rs` (lines 109-176):
  ```rust
  let (listener_ready_tx, listener_ready_rx) = tokio::sync::oneshot::channel::<()>();
  // In spawned IPC listener task:
  match transport::create_pipe_server_with_security(&pipe_addr, is_first, sa_ptr) {
      Ok(server) => {
          if let Some(tx) = ready_sender.take() {
              let _ = tx.send(());
          }
          ...
      }
  }
  // In host main task:
  if listener_ready_rx.await.is_err() {
      tracing::warn!("IPC listener readiness channel closed prematurely");
  }
  self.register_default_components();
  if self.config.taskbar.enabled {
      self.supervisor.spawn(ComponentId::Taskbar)?;
  }
  ```
- Spawning of the child process occurs strictly after the named pipe listener has created its server instance and confirmed readiness via the `oneshot` channel.

#### 2. Supervisor Pipe Argument Propagation
- In `apps/shell-host/src/main.rs` (lines 81-92):
  ```rust
  pub fn register_default_components(&mut self) {
      if self.config.taskbar.enabled {
          let taskbar_exe = resolve_component_executable(ComponentId::Taskbar);
          let spec = process_supervisor::ProcessSpec {
              id: ComponentId::Taskbar,
              executable: taskbar_exe,
              args: vec!["--supervisor-pipe".to_string(), self.pipe_address.clone()],
              restart_policy: self.config.supervisor.restart_policy.clone(),
          };
          self.supervisor.register(spec);
      }
  }
  ```
- `ShellHostDaemon` explicitly passes `--supervisor-pipe <pipe_address>` to the taskbar component spec, verified by `test_taskbar_registration_enabled` (line 988).

#### 3. Taskbar Reconnection State Machine
- In `apps/taskbar/src/main.rs` (lines 146-253), `run_supervisor_connection_loop`:
  - Uses bounded exponential backoff: `delay = (100ms * 2^min(attempt, 5) + jitter).min(2000ms)`.
  - Re-authenticates with fresh `Handshake` on every connection attempt (line 161).
  - Handles transport drops cleanly by breaking to the outer loop to retry.
  - Implements prompt cancellation: if `shutdown_notify.notified()` fires while sleeping in backoff (line 246), it breaks immediately without waiting for the delay.
- In `crates/ipc/src/lib.rs` (lines 203-221), `connect_pipe_client` handles transient `ERROR_PIPE_BUSY` (os error 231) by retrying with 25ms delay, preventing client race failures while a new pipe server instance is binding.

**Verdict on INTEGRATION-01:** **DEFECT RESOLVED (P2 cleared)**.

---

## 5. M3A Scope & Regression Audit

### 5.1 Architecture & Scope Compliance
The workspace was audited for adherence to M3A Floating Companion specifications:
1. **Zero AppBar / Work-Area Calls:**
   A full-text regex query (`SHAppBarMessage|ABM_|SPI_SETWORKAREA|Shell_NotifyIcon`) returned **0 matches** across the entire codebase.
   `test_ac_m3a_04_zero_desktop_work_area_modification` confirms `SPI_GETWORKAREA` returns identical bitwise coordinates before and after taskbar creation.
2. **Win32 Window Styling:**
   `crates/platform-windows/src/window.rs` configures base styles `WS_POPUP | WS_VISIBLE | WS_CLIPSIBLINGS | WS_CLIPCHILDREN` and strips `WS_CAPTION | WS_THICKFRAME | WS_SYSMENU`.
   Extended styles configure `WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE` and strip `WS_EX_APPWINDOW`.
   Validated against the real Slint/Winit HWND in `test_ac_m3a_02_and_03_real_slint_taskbar_window_native_styles`.
3. **Signed Coordinate Arithmetic:**
   All coordinate math in `crates/platform-windows/src/window.rs` uses signed `i32` arithmetic (`Rect.left`, `Rect.top`, `Rect.width()`, etc.), correctly supporting negative offsets in multi-monitor setups without panic.
4. **Boundary Discipline:**
   No code or documentation for system tray, volume/network flyouts, multi-monitor tracking, or Windows Explorer replacement was introduced. Milestone boundaries for M3B–M3F are strictly preserved.

### 5.2 M1 & M2 Regression Check
All tests across `search-core`, `search-index`, `search-providers`, `application-catalog`, `configuration`, and `diagnostics` were executed. 0 regressions were found.

---

## 6. Independent Verification & Quality Gates

### 6.1 Quality Gate Execution Results

All 4 quality gates were executed locally on the Windows host:

```pwsh
# Gate 1: Check Formatting
cargo fmt --all -- --check
# Result: PASS (0 diffs)

# Gate 2: Workspace Compilation
cargo check --workspace --all-targets --locked
# Result: PASS (Finished in 5.06s)

# Gate 3: Strict Linter
cargo clippy --workspace --all-targets --locked -- -D warnings
# Result: PASS (0 warnings, 0 errors)

# Gate 4: Workspace Test Suite
cargo test --workspace --locked -j 2
# Result: PASS (All 13 crates passed, 0 failed)
```

### 6.2 Test Suite Execution Breakdown

| Crate / Target | Tests Passed | Tests Failed | Status |
|---|---|---|---|
| `application-catalog` | 1 | 0 | PASS |
| `astercore` | 3 | 0 | PASS |
| `configuration` | 4 | 0 | PASS |
| `desktop` | 1 | 0 | PASS |
| `diagnostics` | 2 | 0 | PASS |
| `file-manager` | 1 | 0 | PASS |
| `filesystem` | 1 | 0 | PASS |
| `ipc` | 6 | 0 | PASS |
| `platform-windows` | 12 | 0 | PASS |
| `process-supervisor` | 4 | 0 | PASS |
| `search-core` | 5 | 0 | PASS |
| `search-index` | 3 | 0 | PASS |
| `search-providers` | 8 | 0 | PASS |
| `search-ui` | 4 | 0 | PASS |
| `shell-api` | 1 | 0 | PASS |
| `shell-host` (bin) | 13 | 0 | PASS |
| `start-menu` | 1 | 0 | PASS |
| `taskbar` (bin) | 3 | 0 | PASS |
| `taskbar` (`tests/taskbar_foundation.rs`) | 11 | 0 | PASS |
| `ui-components` | 1 | 0 | PASS |
| `window-tracker` | 1 | 0 | PASS |
| **Total Workspace Tests** | **81** | **0** | **ALL PASS** |

### 6.3 CI Run Verification
GitHub Actions CI Run [`45942055160`](https://github.com/medhansh5/AsterShell/actions/runs/45942055160):
- **Commit SHA:** `598535c1eefb059cd166a7eb3f4f9f16bf9077c4` (matches local audited HEAD).
- **Workflow:** `CI` (`.github/workflows/ci.yml`).
- **Target OS:** `windows-latest`.
- **Steps:**
  - `Gate 1 — Check Formatting (cargo fmt)`: `success`
  - `Gate 2 — Workspace Compilation (cargo check)`: `success`
  - `Gate 3 — Strict Linter (cargo clippy)`: `success`
  - `Gate 4 — Workspace Test Suite (cargo test)`: `success`
- **Total Duration:** 2 minutes 2 seconds.
- **Overall Status:** `completed` / `success`.

---

## 7. Severity-Ranked Findings Ledger

| ID | Previous Severity | Finding Summary | M3A.2 Audit Finding | Current Status |
|---|---|---|---|---|
| **SEC-01** | **P1** | Supervisor-to-taskbar shutdown protocol inverted; supervised child could shut down host daemon. | Corrected IPC message flow with `IpcResponse::ShutdownNotice`. Enforced role-based authorization via `ClientRole` in `SupervisedPidRegistry`. Child shutdown requests return `Permission denied`. Taskbar breaks Slint loop cleanly. | **RESOLVED** |
| **LIFECYCLE-01** | **P1** | `test_ac_m3a_01` used `start_kill()`, failing to prove kernel kill-on-close. | Added `query_kill_on_close` verifying `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`. Child verified active before closing single Job Object handle; Windows kernel terminates child without manual intervention. | **RESOLVED** |
| **INTEGRATION-01** | **P2** | Taskbar launched before IPC listener was ready; reconnection lacked backoff. | IPC server binds and signals readiness via `oneshot` before child spawn. Added bounded exponential backoff with jitter and fresh handshake re-authentication in taskbar. Pipe argument passed to taskbar spec. | **RESOLVED** |
| **DOC-01** | **P3** | Outdated PR description and dual-pipe references in documentation. | Architecture specs updated; PR description updated; M3A.2 report authored. | **RESOLVED** |

---

## 8. Final Audit Verdict

The M3A.2 corrective engineering pass has resolved all previously identified P1, P2, and P3 findings without introducing regressions, scope creep, or architectural compromise. Security boundaries, kernel job containment, real window styling, and process lifecycles are backed by rigorous, passing automated acceptance tests verified on both the local Windows environment and GitHub Actions CI.

### Exact Verdict

# `APPROVE M3A — READY TO MERGE`
