# AsterShell — Milestone 3A Independent Exit Audit Report

**Audit Mode:** STRICTLY READ-ONLY ENGINEERING AUDIT  
**Audit Date:** 2026-11-26  
**Audited Target:** AsterShell Milestone M3A — Native Taskbar Window Foundation & Lifecycle  
**Repository:** `medhansh5/AsterShell`  
**Feature Branch:** `feat/m3a-taskbar-window-foundation`  
**Target Branch:** `main`  
**Audited Head Commit (PR #1):** `28bcdfeef9b6c311ed024ea44a5e847f19a3de98`  
**Merge Base Commit (`origin/main`):** `d5d6dc5374d570cf1822de1901a28a91eb2f1ff9`  
**GitHub Pull Request:** [PR #1](https://github.com/medhansh5/AsterShell/pull/1) (State: OPEN)  
**Verified CI Run:** [Run 45768836705](https://github.com/medhansh5/AsterShell/actions/runs/45768836705) (Status: SUCCESS)  
**Final Audit Verdict:** **`REJECT M3A`**

---

## 1. Repository, Git, PR, and CI Provenance

The provenance and integrity of the repository, branches, commits, PR, and CI runs were independently verified prior to code inspection:

```powershell
# 1. Verify remotes and working tree
$ git remote -v
origin  https://github.com/medhansh5/AsterShell.git (fetch)
origin  https://github.com/medhansh5/AsterShell.git (push)

$ git status
On branch feat/m3a-taskbar-window-foundation
Your branch is up to date with 'origin/feat/m3a-taskbar-window-foundation'.
nothing to commit, working tree clean

# 2. Verify commit lineage and merge base
$ git rev-parse HEAD
28bcdfeef9b6c311ed024ea44a5e847f19a3de98

$ git rev-parse origin/main
d5d6dc5374d570cf1822de1901a28a91eb2f1ff9

$ git merge-base HEAD origin/main
d5d6dc5374d570cf1822de1901a28a91eb2f1ff9

# 3. Verify Pull Request state via GitHub CLI
$ gh pr view 1 --json number,title,state,headRefName,headRefOid,baseRefName,baseRefOid
{
  "number": 1,
  "title": "feat(m3a): implement native taskbar window foundation",
  "state": "OPEN",
  "headRefName": "feat/m3a-taskbar-window-foundation",
  "headRefOid": "28bcdfeef9b6c311ed024ea44a5e847f19a3de98",
  "baseRefName": "main",
  "baseRefOid": "d5d6dc5374d570cf1822de1901a28a91eb2f1ff9"
}

# 4. Verify GitHub Actions CI Run
$ gh run view 45768836705 --json headSha,conclusion,status,event
{
  "headSha": "28bcdfeef9b6c311ed024ea44a5e847f19a3de98",
  "event": "pull_request",
  "status": "completed",
  "conclusion": "success"
}
```

### Provenance Audit Findings:
1. **Direct Lineage:** Feature branch `feat/m3a-taskbar-window-foundation` branches cleanly from `origin/main` at commit `d5d6dc5`. Exactly one commit (`28bcdfe`) was added.
2. **PR Configuration:** Pull Request #1 is currently **OPEN**, correctly targets `main`, and points to `28bcdfe`. It has **NOT** been merged.
3. **CI Execution:** GitHub Actions run `45768836705` executed on Windows 11 runner against the exact PR head commit `28bcdfe`. All four quality gates (`cargo fmt`, `cargo check`, `cargo clippy`, `cargo test`) completed successfully in 2m 16s.
4. **Post-CI Hygiene:** Zero commits were added to the feature branch following the successful CI run.

---

## 2. Executive Summary

Milestone M3A aimed to establish the foundational Win32 windowing, Slint presentation, and supervised process lifecycle for **AsterTaskbar** (`apps/taskbar`) operating strictly in **M3A Floating Companion Mode**.

### Strengths & Validated Accomplishments:
- **Native Window Styling:** Native Win32 styles (`WS_POPUP | WS_VISIBLE | WS_CLIPSIBLINGS | WS_CLIPCHILDREN`) and extended styles (`WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`) are correctly applied using pointer-width APIs (`GetWindowLongPtrW`, `SetWindowLongPtrW`, `SetWindowPos`), with `WS_EX_APPWINDOW` explicitly stripped.
- **Signed Coordinate Arithmetic:** Positioning math uses signed `i32` arithmetic over `MonitorFromPoint` and `GetMonitorInfoW`, correctly centering the window at the top of the primary monitor and cleanly handling negative virtual desktop coordinate spaces.
- **Zero Work-Area Contraction:** Strict adherence to M3A Floating Companion Mode is maintained. Zero `SHAppBarMessage` (`ABM_NEW`, `ABM_QUERYPOS`, `ABM_SETPOS`) or `SPI_SETWORKAREA` calls exist in production code, leaving the native Windows 11 Explorer taskbar and desktop work area 100% untouched.
- **Scope Containment:** Prohibited M3B–M3F features (`SetWinEventHook`, window tracking, tray icons, audio flyouts, fullscreen hooks) were completely excluded.

### Critical Deficiencies & Audit Failures:
1. **Unauthenticated Remote Shutdown (P1 / Finding SEC-01):** `apps/taskbar` introduces a second, dedicated Named Pipe server (`taskbar_pipe_address()`) that accepts `IpcRequest::Shutdown` on the very first frame without requiring authentication, without checking client PID, and without verifying the client against any supervisor registry. Any local process in the same user session can terminate AsterTaskbar at will. This completely bypasses the mandatory-first-frame handshake and mutual PID authentication security boundary established in Milestone M2.1.
2. **Superficial AC-M3A-01 Acceptance Test (P1 / Finding LIFECYCLE-01):** The automated test for Criterion AC-M3A-01 (`test_ac_m3a_01_supervised_lifecycle_and_job_object`) does not spawn the taskbar process, does not attach any child to a Windows Job Object, does not verify mutual PID handshake, and does not test process termination when the supervisor exits. It merely creates an in-memory `ProcessSpec` struct and tests that `spec.id.as_str() == "taskbar"`. Real process containment remains unverified.
3. **Supervisor Auto-Launch Missing (P2 / Finding INTEGRATION-01):** `apps/shell-host` exposes helper methods `register_component` and `spawn_component`, but its default `run()` loop does not register or spawn `apps/taskbar`.

Because project governance explicitly dictates that **Approval requires no unresolved P0 or P1 issues, verified critical security boundaries, and proven supervisor lifecycle/containment**, Milestone M3A cannot be approved in its present form. The verdict is **`REJECT M3A`**, with a requirement to execute a narrowly scoped corrective milestone (**M3A.1**) before merging PR #1.

---

## 3. Review Methodology

This audit conducted an adversarial, read-only technical inspection across all changed files in `origin/main..HEAD`:
1. **Security & Protocol Audit:** Traced the Named Pipe server in `apps/taskbar/src/main.rs:164-222` against the M2.1 security remediation requirements.
2. **Supervisor & Job Object Audit:** Traced process spawning, Job Object assignment (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`), and PID registration in `crates/process-supervisor` and `apps/shell-host`.
3. **Win32 Windowing Audit:** Inspected HWND acquisition via `WinitWindowAccessor`, style bitmask manipulation in `crates/platform-windows/src/window.rs`, and `WS_EX_NOACTIVATE` behavior.
4. **DWM Composition Audit:** Inspected DWM attribute configuration and dark fallback behavior.
5. **DPI & Coordinate Pipeline Audit:** Verified mathematical soundness of logical-to-physical scaling and primary monitor signed offset placement.
6. **Acceptance Test Verification:** Evaluated each test in `apps/taskbar/tests/taskbar_foundation.rs` to determine whether it tests real Windows APIs or superficial mocks.

---

## 4. File-by-File Findings

### 4.1 `apps/taskbar/src/main.rs`
- **Lines 28-140:** Defines Slint `TaskbarWindow` (720x48, `no-frame: true`, dark fallback background `rgba(20, 20, 26, 0.94)`). Structurally sound and compliant with Fluent design requirements.
- **Lines 142-162:** `apply_native_taskbar_configuration` acquires HWND via `with_winit_window`, calls DWM styling, configures native styles, queries DPI, and calls `position_taskbar_window`.
- **Lines 164-222 (`handle_taskbar_control_connection`):** **CRITICAL SECURITY FLAW (Finding SEC-01).** Spawns a Named Pipe server on `taskbar_pipe_address()`. Reads `RequestEnvelope`. If `IpcRequest::Shutdown` is received, immediately invokes `slint::quit_event_loop()` and terminates. Does NOT verify client PID, does NOT enforce mandatory handshake, and tracks zero authentication state.
- **Lines 299-370:** Spawns supervisor IPC client connecting to `supervisor_pipe`. Sends `IpcRequest::Handshake`, awaits `HandshakeAck`, and listens for pipe closure. Correctly implements client-side supervisor coordination.
- **Lines 387-410:** Shows window, immediately applies native configuration, and schedules a 50ms single-shot timer to re-verify styles after Winit message pump starts. Sound defense against Winit style overwrite.

### 4.2 `apps/taskbar/tests/taskbar_foundation.rs`
- **Lines 41-81 (`TestWindow`):** Creates a Win32 `STATIC` window with `WS_POPUP` via `CreateWindowExW`. RAII drop calls `DestroyWindow`.
- **Lines 84-108 (`test_ac_m3a_01`):** **VERIFICATION DEFICIENCY (Finding LIFECYCLE-01).** Constructs `JobObject` and in-memory `ProcessSpec`. Authorizes test runner's own PID in registry. Does NOT spawn child, does NOT verify Job Object containment, does NOT test kill-on-close.
- **Lines 111-238 (`test_ac_m3a_02`, `test_ac_m3a_03`, `test_ac_m3a_04`):** Thoroughly exercises real Win32 APIs (`GetWindowLongPtrW`, `SetWindowLongPtrW`, `SetWindowPos`, `SystemParametersInfoW(SPI_GETWORKAREA)`). Accurately validates native styles and work-area preservation.
- **Lines 241-282 (`test_ac_m3a_05`):** Validates primary monitor placement, 150% DPI scaling (720 -> 1080, 48 -> 72), and negative virtual desktop coordinate space math.
- **Lines 284-298 (`test_ac_m3a_06`):** Calls `apply_dark_mode_and_backdrop(hwnd, DwmBackdropType::Mica)`. Validates return value on Windows.
- **Lines 300-408 (`test_ac_m3a_07`):** Tests client/server exchange of Handshake and Shutdown on a synthetic test pipe. Accurately tests framing, but mirrors the unauthenticated control handler flaw.

### 4.3 `crates/platform-windows/src/window.rs`
- **Lines 8-15:** Defines bitflag constants (`WS_POPUP_FLAG`, `WS_EX_TOPMOST_FLAG`, etc.).
- **Lines 288-375 (`get_primary_monitor_work_area`):** Queries `MonitorFromPoint(POINT { 0, 0 }, MONITOR_DEFAULTTOPRIMARY)` + `GetMonitorInfoW`. Falls back to `SPI_GETWORKAREA` and screen metrics. Correctly preserves signed `i32` coordinates.
- **Lines 408-475 (`configure_taskbar_native_styles`):** Reads existing styles, applies target bitmasks, and calls `SetWindowPos` with `HWND_TOPMOST` and `SWP_FRAMECHANGED | SWP_NOACTIVATE`. Includes an idempotent retry check if `WS_EX_TOPMOST` was not immediately reflected.
- **Lines 477-515 (`calculate_taskbar_coordinates_with_dpi`, `position_taskbar_window`):** Calculates horizontal centering and top alignment in physical pixels. Positions window via `SetWindowPos` with `HWND_TOPMOST` and `SWP_NOACTIVATE | SWP_SHOWWINDOW`.

### 4.4 `crates/platform-windows/src/lib.rs`
- **Lines 227-235 (`taskbar_pipe_address`):** Builds pipe name `\\.\pipe\astershell-taskbar-<hash>` from user SID.
- **Lines 513-574:** Unit tests for style flags, primary monitor work area, and signed coordinate arithmetic.

### 4.5 `apps/shell-host/src/main.rs`
- **Lines 61-75:** Adds `supervisor(&self)`, `register_component()`, and `spawn_component()` helper methods to `ShellHostDaemon`. Does not integrate `taskbar` into daemon startup loop.

---

## 5. Critical Security Audit — Independent Control Pipe

### 5.1 Architecture & Threat Model Comparison
In Milestone M2.1, AsterShell addressed an independent audit finding by mandating a strict IPC security model on `apps/shell-host`:
1. Every connection must begin with `IpcRequest::Handshake` as frame 1.
2. The server calls Win32 `GetNamedPipeClientProcessId(stream.handle)` to retrieve the connecting client's OS PID.
3. The server validates `client_pid` against `SupervisedPidRegistry` (a thread-safe registry of PIDs spawned by the supervisor).
4. If unauthorized, the connection is closed immediately.
5. All non-handshake messages are rejected prior to authentication.

In Milestone M3A, `apps/taskbar` implemented a secondary Named Pipe server:
```rust
// apps/taskbar/src/main.rs:166-222
async fn handle_taskbar_control_connection(mut stream: NamedPipeServer, is_running: Arc<AtomicBool>) {
    loop {
        let req_res: Result<RequestEnvelope> = transport::read_framed_message(&mut stream).await;
        let req = match req_res {
            Ok(r) => r,
            Err(_) => break,
        };

        let is_shutdown = matches!(req.request, IpcRequest::Shutdown);
        let response = match req.request {
            IpcRequest::Handshake { .. } => {
                IpcResponse::HandshakeAck { negotiated_version: PROTOCOL_VERSION_1, server_pid: std::process::id() }
            }
            IpcRequest::Ping { timestamp_ms } => IpcResponse::Pong { timestamp_ms },
            IpcRequest::Shutdown => {
                is_running.store(false, Ordering::SeqCst);
                let _ = slint::invoke_from_event_loop(|| { slint::quit_event_loop(); });
                IpcResponse::Ack
            }
            _ => IpcResponse::Error("Unsupported operation on taskbar control pipe".into()),
        };
        // write response and exit if is_shutdown ...
    }
}
```

### 5.2 Security Vulnerability Analysis (Finding SEC-01)
1. **Zero Authentication Check:** The handler maintains no `is_authenticated` boolean or state machine.
2. **Zero PID Authorization:** The handler never calls `GetNamedPipeClientProcessId` and has no access to any authorized PID registry.
3. **Unauthenticated Frame 1 Shutdown:** If a connecting client immediately transmits `RequestEnvelope { request_id: 1, request: IpcRequest::Shutdown }`, the server directly processes the command, instructs Slint to quit its event loop, and terminates `apps/taskbar`.
4. **Deterministic Pipe Name:** The pipe address `\\.\pipe\astershell-taskbar-<hash>` is derived deterministically from the user's SID (`taskbar_pipe_address()`). Any unprivileged process running in the same user session can compute the address and connect.
5. **Attack Vector:** An unprivileged, untrusted background script or malware in the user session can send a 32-byte framed `Shutdown` payload to the Named Pipe, instantly killing AsterTaskbar without authorization or detection.
6. **Severity:** **P1 (High Security Concern)**.

---

## 6. Supervisor and Job Object Audit

### 6.1 Process Containment Assessment
- **Job Object Limit Configuration:** `JobObject::create("AsterShellHostJob")` in `platform-windows` sets `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` via `SetInformationJobObject`. Verified structurally correct.
- **Process Assignment:** `ProcessSupervisor::spawn(id)` in `process-supervisor` assigns child handles via `job_object.assign_raw_handle(raw_h)` and authorizes PIDs in `pid_registry.authorize(pid)`. Verified structurally correct.
- **Production Integration Failure (Finding INTEGRATION-01):** `apps/shell-host/src/main.rs` never calls `self.supervisor.register(...)` or `self.supervisor.spawn(ComponentId::Taskbar)` during its `run()` loop. Therefore, in the current build, running `shell-host` does not launch `taskbar`.
- **Test Integrity Failure (Finding LIFECYCLE-01):** Acceptance test `test_ac_m3a_01` claims to verify supervised lifecycle, but does not invoke `ProcessSupervisor::spawn` or execute the taskbar binary. Real process containment under Windows Job Object was not proven in automated test execution.

---

## 7. Native Window, Styling, and DWM Audit

### 7.1 HWND Acquisition & Style Configuration
- **Winit Realization:** `apps/taskbar` uses `WinitWindowAccessor::with_winit_window` to obtain the raw `HWND` from `RawWindowHandle::Win32`. This occurs after `show()`, ensuring a valid native window handle exists.
- **Base Styles (`WS_POPUP | WS_VISIBLE | WS_CLIPSIBLINGS | WS_CLIPCHILDREN`):** Verified present via `GetWindowLongPtrW(hwnd, GWL_STYLE)`. Slint component declares `no-frame: true;`, which prevents standard caption and frame creation.
- **Extended Styles (`WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`):**
  - `WS_EX_TOPMOST`: Floats above standard windows.
  - `WS_EX_TOOLWINDOW`: Excludes window from Windows Taskbar and `Alt + Tab` switcher.
  - `WS_EX_NOACTIVATE`: Tells Windows not to activate window on mouse click.
  - `& !WS_EX_APPWINDOW`: Strips application window style.
- **SetWindowPos Concurrency Hardening:** In `configure_taskbar_native_styles`, an explicit read-back check retries `SetWindowPos` if User32 delayed topmost state attachment.
- **Focus Integrity:** Zero calls to `SetForegroundWindow`, `SetActiveWindow`, or `AttachThreadInput`. All `SetWindowPos` calls strictly include `SWP_NOACTIVATE`.

### 7.2 DWM Composition & Dark Fallback
- Calls `DwmSetWindowAttribute` with `DWMWA_USE_IMMERSIVE_DARK_MODE` (20), `DWMWA_WINDOW_CORNER_PREFERENCE` (33), and `DWMWA_SYSTEMBACKDROP_TYPE` (38, Mica = 2).
- Non-Windows-11 environments safely log a warning on attribute 38 failure and fall back to the root Slint rectangle (`rgba(20, 20, 26, 0.94)`).

---

## 8. Monitor Placement & Coordinate Pipeline Audit

### 8.1 Signed Coordinate Pipeline
The coordinate calculation in `calculate_taskbar_coordinates_with_dpi` was evaluated:
$$\text{physical\_width} = \frac{720 \times \text{dpi}}{96}, \quad \text{physical\_height} = \frac{48 \times \text{dpi}}{96}$$
$$X = \text{work.left} + \frac{\text{work.width}() - \text{physical\_width}}{2}, \quad Y = \text{work.top}$$

### 8.2 Audit Findings on Coordinates:
1. **Signed `i32` Safety:** All fields in `Rect` (`left`, `top`, `right`, `bottom`) and dimensions are signed `i32`. Simulated virtual displays with negative origins (`left: -1920`, `top: -1080`) calculate to `X = -1320, Y = -1080` without underflow or overflow.
2. **Single DPI Scaling:** Slint's root window specifies `preferred-width: 720px; preferred-height: 48px;`. Winit renders at native physical scale, and `position_taskbar_window` sets physical pixels matching `720 * dpi / 96`. No double scaling was observed.
3. **Primary Monitor Bounds:** Primary monitor is determined via `MonitorFromPoint(POINT { 0, 0 }, MONITOR_DEFAULTTOPRIMARY)`.

---

## 9. Scope Compliance

The complete git diff (`origin/main..HEAD`) was audited against the prohibited features ledger:

| Prohibited Feature in M3A | Codebase Check | Audit Finding |
|---|---|---|
| `SetWinEventHook` | `git grep -i SetWinEventHook apps/ crates/` | **CLEAN** (0 occurrences in code) |
| Window Tracking / `EnumWindows` | `git grep -i EnumWindows apps/ crates/` | **CLEAN** (0 occurrences in code) |
| AppBar API (`SHAppBarMessage`) | `git grep -i SHAppBarMessage apps/ crates/` | **CLEAN** (0 occurrences in code) |
| Work Area Contraction (`SPI_SETWORKAREA`) | `git grep -i SPI_SETWORKAREA apps/ crates/` | **CLEAN** (0 occurrences in code) |
| Explorer Taskbar Modification | Review of Win32 calls | **CLEAN** (No shell hook or taskbar injection) |
| System Tray / AsterTray | `git grep -i AsterTray apps/ crates/` | **CLEAN** (0 occurrences in code) |
| Audio / Network Indicators | Review of Slint UI & crates | **CLEAN** (Only brand pill, mode badge, and status text) |
| Fullscreen Z-Order Demotion Hook | Review of Win32 calls | **CLEAN** (Deferred to M3F) |
| Multi-Monitor Hot-Plugging | Review of `platform-windows` | **CLEAN** (Restricted to primary monitor) |

---

## 10. Test Quality Audit

| Test Name | Criterion | Real APIs Exercised | Test Methodology | Audit Finding & Verdict |
|---|---|---|---|---|
| `test_ac_m3a_01_supervised_lifecycle_and_job_object` | **AC-M3A-01** | `CreateJobObjectW`, `SetInformationJobObject` | Creates in-memory Job Object and `ProcessSpec`. Does not spawn process or test IPC handshake. | **FAILED / SUPERFICIAL (P1):** Does not test actual process containment or supervisor kill-on-close. |
| `test_ac_m3a_02_native_frameless_window_styles` | **AC-M3A-02** | `CreateWindowExW`, `GetWindowLongPtrW`, `SetWindowLongPtrW`, `SetWindowPos`, `DestroyWindow` | Creates real Win32 window, calls `configure_taskbar_native_styles`, asserts all 4 base style bits. | **VERIFIED:** Real Win32 window style manipulation tested and confirmed. |
| `test_ac_m3a_03_extended_shell_styles_and_alt_tab_exclusion` | **AC-M3A-03** | `CreateWindowExW`, `GetWindowLongPtrW`, `SetWindowLongPtrW`, `SetWindowPos`, `DestroyWindow` | Creates real Win32 window, asserts `WS_EX_TOPMOST`, `WS_EX_TOOLWINDOW`, `WS_EX_NOACTIVATE`, and `!WS_EX_APPWINDOW`. | **VERIFIED:** Real Win32 extended style bits and Alt+Tab exclusion confirmed. |
| `test_ac_m3a_04_zero_desktop_work_area_modification` | **AC-M3A-04** | `SystemParametersInfoW(SPI_GETWORKAREA)` | Queries `SPI_GETWORKAREA` before and after window styling and positioning; asserts bitwise equality. | **VERIFIED:** Exact bitwise preservation of desktop work area proven. |
| `test_ac_m3a_05_signed_primary_monitor_placement` | **AC-M3A-05** | `MonitorFromPoint`, `GetMonitorInfoW` | Validates primary display centering, top alignment, negative coordinate space math, and 150% DPI scaling. | **VERIFIED:** Mathematical and platform placement correctness proven. |
| `test_ac_m3a_06_dwm_backdrop_realization_and_fallback` | **AC-M3A-06** | `DwmSetWindowAttribute` | Calls `apply_dark_mode_and_backdrop(hwnd, DwmBackdropType::Mica)` on Win32 window; asserts `Ok(())`. | **VERIFIED WITH ACCEPTED LIMITATION:** Exercises DWM API call without error; visual raster composition not checked. |
| `test_ac_m3a_07_clean_ipc_shutdown` | **AC-M3A-07** | `CreateNamedPipeW`, `ConnectNamedPipe`, `WriteFile`, `ReadFile` | Creates test Named Pipe server; connects client; sends Handshake & Shutdown envelopes; verifies Ack. | **PARTIALLY VERIFIED (P2):** Protocol framing verified, but reflects unauthenticated shutdown flaw from Finding SEC-01. |

---

## 11. Findings Ledger

| Finding ID | Severity | File & Line | Evidence | Impact | Required Corrective Action |
|---|:---:|---|---|---|---|
| **SEC-01** | **P1** | `apps/taskbar/src/main.rs:166-222` | `handle_taskbar_control_connection` processes `IpcRequest::Shutdown` on frame 1 without handshake, PID check, or authentication state tracking. | Any local process in the user session can connect to `taskbar_pipe_address()` and terminate AsterTaskbar at will. | Remove standalone control pipe server from production `apps/taskbar`, or enforce M2.1 mutual PID authentication and mandatory first-frame handshake before accepting shutdown. |
| **LIFECYCLE-01** | **P1** | `apps/taskbar/tests/taskbar_foundation.rs:84-108` | `test_ac_m3a_01` constructs an in-memory `ProcessSpec` and asserts `spec.id.as_str() == "taskbar"`; never calls `ProcessSupervisor::spawn` or executes binary. | Supervised process containment under Windows Job Object (`KILL_ON_JOB_CLOSE`) is completely unverified in automated tests. | Implement integration test spawning `taskbar` under `ProcessSupervisor`, verifying child PID registration and automatic termination upon supervisor death. |
| **INTEGRATION-01** | **P2** | `apps/shell-host/src/main.rs:61-75` | `ShellHostDaemon` exposes `register_component` and `spawn_component`, but its default `run()` loop does not register or spawn `apps/taskbar`. | Running `shell-host` daemon does not bring up the taskbar companion window. | Wire `taskbar` registration and spawning into `shell-host` startup sequence as part of Milestone M3B. |
| **TEST-01** | **P2** | `apps/taskbar/tests/taskbar_foundation.rs:41-81` | Tests AC-M3A-02, AC-M3A-03, AC-M3A-04, AC-M3A-06 use a synthetic Win32 `STATIC` window rather than the Slint-created Winit window. | Runtime interaction between Winit event loop and native styling relies on non-blocking timer in `main.rs` that is not directly verified in headless CI. | Document this verification boundary; validate interactively on Windows desktop session. |
| **WINDOW-01** | **P3** | `crates/platform-windows/src/window.rs:423-426` | `configure_taskbar_native_styles` computes `target_style = current_style | WS_POPUP.0 | ...` without explicitly masking `& !(WS_CAPTION.0 | WS_THICKFRAME.0)`. | Defense-in-depth style gap if applied to arbitrary framed windows (harmless here because Slint sets `no-frame: true;`). | Explicitly strip `WS_CAPTION` and `WS_THICKFRAME` bits in `configure_taskbar_native_styles`. |
| **DOC-01** | **P3** | `docs/architecture/M3_TASKBAR_ARCHITECTURE.md` | Architecture specification does not document the dedicated taskbar control pipe endpoint (`taskbar_pipe_address()`). | Architecture documentation drift. | Align architecture documentation or decommission control pipe in favor of supervisor-mediated control. |

---

## 12. Acceptance Criteria Verdicts

| Criterion | Requirement Summary | Audit Verdict | Justification |
|---|---|:---:|---|
| **AC-M3A-01** | Supervised Child Lifecycle & Job Object | **REJECTED** | Test does not spawn child process or test Job Object kill-on-close containment. |
| **AC-M3A-02** | Native Frameless Window Styles | **VERIFIED** | Real Win32 HWND verified with `WS_POPUP`, `WS_VISIBLE`, `WS_CLIPSIBLINGS`, `WS_CLIPCHILDREN`. |
| **AC-M3A-03** | Extended Shell Styles & Alt+Tab Exclusion | **VERIFIED** | Real Win32 HWND verified with `WS_EX_TOPMOST`, `WS_EX_TOOLWINDOW`, `WS_EX_NOACTIVATE`, `!WS_EX_APPWINDOW`. |
| **AC-M3A-04** | Zero Desktop Work-Area Modification | **VERIFIED** | Real `SPI_GETWORKAREA` queried before and after; bitwise identical bounds confirmed. |
| **AC-M3A-05** | Signed Primary Monitor Placement | **VERIFIED** | Primary monitor selection, horizontal centering, top bounds alignment, signed math, and DPI scaling verified. |
| **AC-M3A-06** | DWM Mica Realization & Fallback | **VERIFIED** | Win32 DWM APIs executed with immersive dark mode and Mica; dark background fallback verified in Slint. |
| **AC-M3A-07** | Clean IPC Shutdown & Restart Coordination | **CONDITIONALLY ACCEPTED** | Protocol framing and shutdown mechanics verified, but security vulnerability SEC-01 must be remediated. |

---

## 13. Final Audit Verdict & Corrective Guidance

### **VERDICT: `REJECT M3A`**

### Verdict Justification:
While Milestone M3A delivers impressive Win32 windowing, clean Slint visual design, robust signed coordinate mathematics, and zero work-area contraction, it fails two critical engineering criteria:
1. **Security Boundary Violation (Finding SEC-01):** The taskbar control pipe violates AsterShell's core IPC security architecture by accepting unauthenticated `Shutdown` requests from any process in the user session without PID verification or handshake.
2. **Acceptance Test Deficiency (Finding LIFECYCLE-01):** Criterion AC-M3A-01 is claimed as passed, but the automated test only tests in-memory struct properties without spawning the process or verifying Windows Job Object containment.

Under AsterShell engineering guidelines, **approval requires that all critical security boundaries are verified, no unresolved P0 or P1 issues remain, and supervisor process containment is adequately proven**.

### Recommended Action Plan — Milestone M3A.1 Corrective Pass:
Before Pull Request #1 can be approved and merged into `main`, a narrowly scoped corrective milestone (**M3A.1: Taskbar IPC Security & Process Containment Remediation**) must be executed on `feat/m3a-taskbar-window-foundation`:

1. **[REMEDY-SEC-01] Secure or Eliminate the Control Pipe:**
   - *Option A (Recommended):* Remove the standalone Named Pipe server from `apps/taskbar`. Route all lifecycle and shutdown signaling exclusively through `shell-host` supervisor via the authenticated primary IPC pipe (`default_pipe_address()`).
   - *Option B:* If a dedicated taskbar control pipe is strictly necessary, apply the complete M2.1 security pattern: enforce mandatory frame-1 handshake, query connecting PID via `GetNamedPipeClientProcessId`, authenticate PID against supervisor or parent process, and reject all unauthenticated requests.
2. **[REMEDY-LIFECYCLE-01] Real Integration Test for AC-M3A-01:**
   - Author a genuine integration test that spawns the `taskbar` binary using `ProcessSupervisor`, verifies that the child PID is registered in `SupervisedPidRegistry`, verifies that the child is assigned to the `JobObject`, and verifies that terminating the supervisor immediately terminates the child process.
3. **[REMEDY-CI] Re-run Quality Gates & Update Audit:**
   - Commit fixes to `feat/m3a-taskbar-window-foundation`, verify all 4 quality gates pass on Windows CI, and conduct an M3A.1 close-out review.

**PR #1 must remain OPEN and UNMERGED until Milestone M3A.1 is completed.**
