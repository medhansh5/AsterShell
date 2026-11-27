# AsterShell Milestone M3A.2 Engineering Report
## IPC Lifecycle Direction, Kernel Kill-on-Close Proof & Startup Race Elimination

**Milestone:** M3A.2 (Post-Audit Corrective Engineering Pass)  
**Repository:** `https://github.com/medhansh5/AsterShell`  
**PR:** [#1 (feat/m3a-taskbar-window-foundation)](https://github.com/medhansh5/AsterShell/pull/1)  
**Target Branch:** `main`  
**Baseline Commit:** `ec26bc9da027a3e8e87a5a77202df2e75a8b5b66`  
**Reference Audit:** `docs/audits/M3A_1_EXIT_AUDIT.md`  
**Status:** M3A.2 COMPLETE — READY FOR INDEPENDENT RE-AUDIT  

---

## 1. Executive Summary

Milestone M3A.2 is an adversarial, precision corrective engineering pass that directly remediates all four blocking findings identified in the independent M3A.1 exit audit (`docs/audits/M3A_1_EXIT_AUDIT.md`):

1. **IPC Lifecycle Protocol & Security Hardening (SEC-01 - P1):**
   - Corrected the IPC message flow direction. The supervisor connection handler in `apps/shell-host` now issues an explicit `IpcResponse::ShutdownNotice` across active client streams upon host shutdown.
   - Introduced role-based authorization in `SupervisedPidRegistry` (`ClientRole::SupervisedChild` vs `ClientRole::AdminController`). Supervised child processes (such as `taskbar`) are explicitly prohibited from executing host-level shutdown (`IpcResponse::Error("Permission denied: ...")`), eliminating the privilege escalation vector.
   - Replaced the dead client `RequestEnvelope` loop in `apps/taskbar` with an active `ResponseEnvelope` supervisor listener.
2. **Authoritative Kernel Kill-on-Close Proof (LIFECYCLE-01 - P1):**
   - Implemented `JobObject::query_kill_on_close` utilizing Win32 `QueryInformationJobObject` with `JobObjectExtendedLimitInformation` to authoritatively verify `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` at the OS kernel level.
   - Replaced manual termination in `test_ac_m3a_01` with genuine kernel handle closure. The test launches a live persistent child process, queries `IsProcessInJob`, verifies active process execution (`STILL_ACTIVE`), and closes the final Job Object handle via `supervisor.into_job_object()` and `drop(job_object)` without calling `child.start_kill()`, `TerminateProcess`, or `shutdown_all()`.
   - Windows kernel termination on handle closure is independently observed via `WaitForSingleObject` returning `WAIT_OBJECT_0`.
3. **Deterministic Startup Ordering & Reconnection State Machine (INTEGRATION-01 - P2):**
   - Refactored `ShellHostDaemon::run()` to establish and bind the named pipe server instance and signal readiness over an asynchronous oneshot channel *prior* to registering or spawning child processes, eliminating the startup connection race.
   - Implemented `run_supervisor_connection_loop` in `apps/taskbar`, featuring bounded exponential backoff with pseudo-jitter, fresh handshake re-authentication upon reconnect, graceful shutdown on `ShutdownNotice` or pipe EOF, and prompt cancellation of pending retry timers.
4. **Documentation and Pull Request Refresh (DOC-01 - P3):**
   - Authored this M3A.2 engineering report.
   - Updated `IPC_PROTOCOL_SPEC.md` and `PROCESS_SUPERVISION.md` to document the verified single-pipe authenticated architecture and role-based lifecycle.
   - Refreshed PR #1 metadata on GitHub.

---

## 2. Audit Findings Resolution Ledger

| Finding ID | Severity | M3A.1 Audit Finding | M3A.2 Engineering Resolution | Verification Test | Status |
|---|---|---|---|---|---|
| **SEC-01** | **P1** | Supervisor-to-taskbar shutdown direction was inverted/dead code; supervised children could shut down shell-host daemon. | Added `ShutdownNotice` to `IpcResponse`. `shell-host` sends `ShutdownNotice` upon host shutdown. Role-based authorization (`ClientRole`) added to `SupervisedPidRegistry`: child `Shutdown` requests rejected with `Permission denied`. Dead loop removed from `taskbar`. | `test_ac_m3a_07_supervisor_authenticated_ipc_and_shutdown`, `test_ac_m3a_07_supervised_child_cannot_shutdown_host`, `test_ac_m3a_08_taskbar_graceful_shutdown_on_supervisor_exit` | ✅ **RESOLVED** |
| **LIFECYCLE-01** | **P1** | `test_ac_m3a_01` used `start_kill()`, failing to prove kernel `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` behavior. | Added `JobObject::query_kill_on_close` (`QueryInformationJobObject`). Updated `test_ac_m3a_01` to close the final Job Object handle via `into_job_object()` while child is running, observing kernel kill via `WaitForSingleObject` without direct termination APIs. | `test_ac_m3a_01_real_process_containment_and_is_process_in_job` | ✅ **RESOLVED** |
| **INTEGRATION-01** | **P2** | Taskbar launched before IPC server listener was ready; connection failed and did not retry. | IPC server binds and signals readiness via oneshot channel before child spawn. Added `run_supervisor_connection_loop` with bounded exponential backoff, jitter, fresh handshake re-auth, and cancellation. | `test_supervisor_connection_loop_lifecycle_and_cancellation`, `test_reconnect_backoff_shutdown_cancellation` | ✅ **RESOLVED** |
| **DOC-01** | **P3** | PR #1 description and documentation contained obsolete dual-pipe references. | Updated `IPC_PROTOCOL_SPEC.md`, `PROCESS_SUPERVISION.md`, PR #1 description, and authored M3A.2 engineering report. | Documentation review & GitHub PR verification | ✅ **RESOLVED** |

---

## 3. Technical Architecture & Implementation

### 3.1 Role-Based Security Authorization Model (SEC-01)

`SupervisedPidRegistry` now tracks the authorization role for every registered PID:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientRole {
    SupervisedChild,
    AdminController,
}
```

- When `shell-host` spawns managed components (e.g. `ComponentId::Taskbar`), its PID is registered with `ClientRole::SupervisedChild`.
- In `handle_client_connection`, when a client sends `IpcRequest::Shutdown`:
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
- Any unauthorized or supervised child attempting host shutdown is explicitly rejected, preventing privilege escalation.

### 3.2 Supervisor-to-Child Lifecycle Sequence (SEC-01)

The lifecycle sequence during host shutdown operates cleanly without protocol inversions:

```
ShellHostDaemon                  handle_client_connection                 Taskbar Process
      |                                     |                                    |
      | --- shutdown_tx.send(true) -------> |                                    |
      |                                     | --- IpcResponse::ShutdownNotice -> |
      |                                     |                                    | (Breaks Slint loop)
      |                                     | <--- Pipe Disconnect / EOF ------- | (Exits code 0)
      |                                     |                                    +
      | (Wait bounded grace period: 1500ms) |
      | (Child exited cleanly; no kill)     |
      v                                     v
```

- If a child process is non-responsive or deadlocked, the supervisor's `shutdown_all` timeout expires and falls back to `child.start_kill()`.
- Verified in `test_ac_m3a_08_taskbar_graceful_shutdown_on_supervisor_exit` and `test_ac_m3a_09_non_responsive_taskbar_forcibly_terminated_on_timeout`.

### 3.3 Authoritative Kernel Kill-on-Close Proof (LIFECYCLE-01)

`JobObject` in `crates/platform-windows` now provides:
```rust
pub fn query_kill_on_close(&self) -> Result<bool> {
    // Queries QueryInformationJobObject with JobObjectExtendedLimitInformation
    // Verifies (info.BasicLimitInformation.LimitFlags & JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE) != 0
}
```

In `test_ac_m3a_01`:
1. **Real Process Spawn:** Spawns `taskbar.exe` with `--test-child --no-supervisor`.
2. **Kernel Job Membership:** Win32 `IsProcessInJob` returns `true`.
3. **Kernel Limit Query:** Win32 `QueryInformationJobObject` confirms `KILL_ON_JOB_CLOSE`.
4. **Kernel Termination:** Closes the sole Job Object handle via `supervisor.into_job_object()` and `drop(job_object)`.
5. **Observation:** `WaitForSingleObject(proc_handle, 3000)` returns `WAIT_OBJECT_0` without invoking `child.start_kill()`, `TerminateProcess`, or `shutdown_all()`.

### 3.4 Startup Readiness & Reconnection State Machine (INTEGRATION-01)

In `ShellHostDaemon::run()`:
1. `create_pipe_server_with_security` binds the initial named pipe server instance.
2. `listener_ready_tx.send(())` fires.
3. `listener_ready_rx.await` unblocks before `register_default_components()` and `supervisor.spawn(ComponentId::Taskbar)` are called.
4. When `taskbar.exe` executes, the supervisor pipe is guaranteed to be ready.

In `apps/taskbar` (`run_supervisor_connection_loop`):
- Connection attempts use bounded exponential backoff (`delay = (100ms * 2^min(attempt, 5) + jitter).min(2000ms)`).
- On transport drop, taskbar breaks to the outer loop and re-authenticates with a fresh `Handshake`.
- Shutdown notifications interrupt backoff sleeps immediately.

---

## 4. Verification Evidence & Quality Gates

### 4.1 Local Quality Gates Summary

All quality gates passed with zero warnings and zero regressions:

1. **Formatting:** `cargo fmt --all -- --check` (0 diffs).
2. **Compilation:** `cargo check --workspace --all-targets --locked -j 1` (clean).
3. **Linting:** `cargo clippy --workspace --all-targets --locked -j 1 -- -D warnings` (0 warnings).
4. **Taskbar Foundation Suite:** `cargo test -j 1 -p taskbar --test taskbar_foundation` (11/11 passed, 2.08s).
5. **Taskbar Binary Suite:** `cargo test -j 1 -p taskbar --bin taskbar` (3/3 passed, 0.28s).
6. **Shell-Host Suite:** `cargo test -j 1 -p shell-host` (13/13 passed, 1.72s).
7. **Supervisor Suite:** `cargo test -j 1 -p process-supervisor` (4/4 passed, 0.00s).
8. **Platform Suite:** `cargo test -j 1 -p platform-windows` (12/12 passed, 0.01s).
9. **Full Workspace Suite:** `cargo test --workspace --locked -j 1` (All 13 crates passed, 34.87s).

---

## 5. Scope & Boundary Discipline

- Strictly within M3A window foundation and lifecycle scope.
- Zero AppBar calls (`SHAppBarMessage`), zero work area modifications.
- Zero system tray or multi-monitor tracking features.
- No changes to M1/M2/M2.1 subsystems.

---

## 6. Conclusion

All four audit findings from `docs/audits/M3A_1_EXIT_AUDIT.md` are definitively resolved with rigorous code and kernel verification.

**Milestone M3A.2 is COMPLETE and ready for independent re-audit.**
