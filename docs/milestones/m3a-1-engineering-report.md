# AsterShell Milestone M3A.1 Engineering Report
## IPC Security, Real Process Containment & Shell-Host Integration

**Milestone:** M3A.1 (Post-Audit Corrective Engineering Pass)  
**Repository:** `https://github.com/medhansh5/AsterShell`  
**PR:** [#1 (feat/m3a-taskbar-window-foundation)](https://github.com/medhansh5/AsterShell/pull/1)  
**Target Branch:** `main`  
**Reference Audit:** `docs/audits/M3A_EXIT_AUDIT.md`  
**Status:** READY FOR RE-AUDIT  

---

## 1. Executive Summary

Milestone M3A.1 is a targeted, non-scope-expanding corrective engineering pass addressing all six findings identified in the independent M3A exit audit (`docs/audits/M3A_EXIT_AUDIT.md`).

The pass focused on three fundamental technical pillars:
1. **IPC Security Hardening (SEC-01, DOC-01):** Completely eliminated the redundant, standalone Named Pipe server in `apps/taskbar`. All taskbar lifecycle commands (including shutdown) are strictly mediated through the authenticated supervisor IPC channel with mutual PID authorization.
2. **Authoritative Kernel Process Containment (LIFECYCLE-01):** Implemented genuine end-to-end process supervision testing. The test suite spawns the real compiled `taskbar` executable via `ProcessSupervisor`, checks PID authorization, executes the authoritative Win32 `IsProcessInJob` kernel API, and verifies termination upon supervisor shutdown.
3. **Shell-Host Automatic Integration (INTEGRATION-01):** Integrated `apps/taskbar` into `ShellHostDaemon::run()`, automatically registering and launching the taskbar companion process when `config.taskbar.enabled` is true.
4. **Real Slint Window Style Verification (TEST-01, WINDOW-01):** Added integration tests exercising the actual Slint UI window realization via Winit to assert that the real Slint HWND receives `WS_POPUP`, `WS_VISIBLE`, `WS_CLIPSIBLINGS`, `WS_CLIPCHILDREN`, `WS_EX_TOPMOST`, `WS_EX_TOOLWINDOW`, and `WS_EX_NOACTIVATE`, while explicitly stripping `WS_CAPTION`, `WS_THICKFRAME`, `WS_SYSMENU`, and `WS_EX_APPWINDOW`.

---

## 2. Audit Findings Resolution Ledger

| ID | Severity | Original Audit Finding | M3A.1 Engineering Resolution | Verification Status |
|---|---|---|---|---|
| **SEC-01** | P1 | Standalone taskbar control pipe accepts unauthenticated Shutdown requests | Completely eliminated the standalone Named Pipe server (`astershell-taskbar-*`) and `--pipe` CLI arg in `apps/taskbar`. Replaced with unified supervisor IPC where the taskbar acts strictly as a client to `shell-host`, authenticated via mutual PID verification. Unauthenticated shutdown is impossible. | **RESOLVED** (`test_ac_m3a_07_supervisor_authenticated_ipc_and_shutdown`) |
| **LIFECYCLE-01** | P1 | AC-M3A-01 test did not spawn a process or verify Job Object containment | Implemented `JobObject::is_process_in_job` and `ProcessSupervisor::is_child_in_job` utilizing the Win32 `IsProcessInJob` API. Rewrote `test_ac_m3a_01` to spawn the real `taskbar` executable, verify PID in `pid_registry`, call `IsProcessInJob`, and verify child termination on supervisor shutdown. | **RESOLVED** (`test_ac_m3a_01_real_process_containment_and_is_process_in_job`) |
| **INTEGRATION-01** | P2 | `shell-host` did not automatically start `apps/taskbar` | Added `resolve_component_executable(ComponentId)` and `register_default_components()` to `ShellHostDaemon`. When `config.taskbar.enabled` is `true`, `shell-host` automatically registers and spawns `apps/taskbar` on startup. Added unit tests for enabled/disabled configurations. | **RESOLVED** (`tests::test_taskbar_registration_enabled`, `test_taskbar_registration_disabled`) |
| **TEST-01** | P2 | Native-window acceptance tests exercised a Win32 `STATIC` window rather than Slint window | Added `apply_native_taskbar_configuration` to `crates/platform-windows`. Added real Slint `TestTaskbarWindow` test that realizes the window on the Winit event loop, retrieves its native HWND, configures styles, and asserts all base and extended styles on the real Slint window. Categorized tests into Unit, Slint Integration, Process Containment, and Security. | **RESOLVED** (`test_ac_m3a_02_and_03_real_slint_taskbar_window_native_styles`) |
| **WINDOW-01** | P3 | Native styling did not explicitly strip titlebar and resize border bits | Updated `configure_taskbar_native_styles` to explicitly strip `WS_CAPTION`, `WS_THICKFRAME`, `WS_MINIMIZEBOX`, `WS_MAXIMIZEBOX`, and `WS_SYSMENU`. Added assertions in both Win32 unit and Slint integration tests. | **RESOLVED** (Bitwise assertions in tests AC-M3A-02 & AC-M3A-03) |
| **DOC-01** | P3 | Architecture and engineering documentation did not reflect unified supervisor IPC | Updated documentation and reports to reflect the elimination of the separate control pipe and the enforcement of mutual PID supervisor IPC. | **RESOLVED** |

---

## 3. Architectural & Implementation Details

### 3.1 IPC Architecture Hardening (SEC-01)

The previous dual-pipe approach introduced an unauthenticated control surface. In M3A.1, this was eliminated:

```
+--------------------------------------------------------------------------+
| AsterShell Architecture: Single Authenticated Supervisor IPC Bus        |
+--------------------------------------------------------------------------+

      +-------------------------------------------------------+
      |                  apps/shell-host                      |
      |   - Hosts \\.\pipe\astershell-ipc-{hash} (SDDL)       |
      |   - Verifies caller PID via get_pipe_client_pid       |
      |   - Enforces Handshake before any privileged commands  |
      +-------------------------------------------------------+
                                  ^
                                  | Named Pipe (Mutual PID Auth)
                                  v
      +-------------------------------------------------------+
      |                   apps/taskbar                        |
      |   - Connects as client to supervisor                  |
      |   - Performs authenticated handshake                  |
      |   - Listens for Shutdown envelope or pipe disconnect  |
      |   - NO standalone server / NO unauthenticated pipe    |
      +-------------------------------------------------------+
```

When `taskbar` receives `IpcRequest::Shutdown` from the supervisor or detects supervisor pipe closure, it triggers `slint::quit_event_loop()` to exit gracefully.

### 3.2 Win32 `IsProcessInJob` Kernel Verification (LIFECYCLE-01)

To conclusively verify Windows Job Object containment at the OS kernel level, we implemented:

```rust
// crates/platform-windows/src/lib.rs
pub fn contains_process_handle(&self, process_handle: HANDLE) -> Result<bool> {
    use windows::Win32::System::JobObjects::IsProcessInJob;
    let mut in_job = windows::Win32::Foundation::BOOL(0);
    unsafe {
        IsProcessInJob(process_handle, self.handle, &mut in_job)
            .map_err(|e| AsterError::Platform(format!("IsProcessInJob failed: {}", e)))?;
    }
    Ok(in_job.as_bool())
}
```

This is exposed through `ProcessSupervisor::is_child_in_job(&self, id: ComponentId) -> Result<bool>`, enabling automated non-invasive kernel validation.

### 3.3 Shell-Host Auto-Start Integration (INTEGRATION-01)

In `apps/shell-host/src/main.rs`:
- Added `TaskbarConfig.enabled: bool` (default `true`) in `crates/configuration`.
- `ShellHostDaemon::register_default_components()` queries `config.taskbar.enabled`.
- Resolves executable via `resolve_component_executable(ComponentId::Taskbar)` supporting local cargo dev, target directories, and installation layouts.
- In `ShellHostDaemon::run()`, automatically spawns `ComponentId::Taskbar`.

---

## 4. Verification Evidence

### 4.1 Acceptance Test Suite (`apps/taskbar/tests/taskbar_foundation.rs`)

```
running 8 tests
test test_ac_m3a_05_signed_primary_monitor_placement ... ok
test test_ac_m3a_03_extended_shell_styles_and_alt_tab_exclusion_unit ... ok
test test_ac_m3a_02_native_frameless_window_styles_unit ... ok
test test_ac_m3a_04_zero_desktop_work_area_modification ... ok
test test_ac_m3a_06_dwm_backdrop_realization_and_fallback ... ok
test test_ac_m3a_07_supervisor_authenticated_ipc_and_shutdown ... ok
test test_ac_m3a_02_and_03_real_slint_taskbar_window_native_styles ... ok
test test_ac_m3a_01_real_process_containment_and_is_process_in_job ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.46s
```

### 4.2 Shell-Host Unit Tests (`apps/shell-host`)

```
running 11 tests
test tests::test_resolve_component_executable ... ok
test tests::test_shell_host_daemon_initialization ... ok
test tests::test_taskbar_registration_disabled ... ok
test tests::test_taskbar_registration_enabled ... ok
test tests::test_shell_host_rejects_get_config_before_handshake ... ok
test tests::test_shell_host_rejects_shutdown_before_handshake ... ok
test tests::test_shell_host_rejects_ping_before_handshake ... ok
test tests::test_shell_host_rejects_discover_apps_before_handshake ... ok
test tests::test_shell_host_rejects_unauthorized_client_pid ... ok
test tests::test_shell_host_rejects_duplicate_handshake ... ok
test tests::test_shell_host_ipc_lifecycle ... ok

test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.83s
```

### 4.3 Platform Windows Tests (`crates/platform-windows`)

```
running 12 tests
test tests::test_current_user_sid_retrieval ... ok
test tests::test_job_object_lifecycle ... ok
test tests::test_pipe_address_format ... ok
test tests::test_taskbar_style_flags ... ok
test tests::test_primary_monitor_work_area ... ok
test tests::test_taskbar_coordinates_signed_math ... ok
test tests::test_hotkey_config_defaults ... ok
test tests::test_default_pipe_sddl_and_security_descriptor ... ok
test tests::test_dpi_scaling_and_coordinates ... ok
test tests::test_active_monitor_work_area_and_coordinates ... ok
test tests::test_hotkey_registration_and_clean_drop ... ok
test tests::test_search_manager_com_initialization ... ok

test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
```

### 4.4 Quality Gates Status Summary

| Quality Gate | Command | Result |
|---|---|---|
| **Gate 1: Rustfmt** | `cargo fmt --all -- --check` | **PASS (Clean)** |
| **Gate 2: Compilation** | `cargo check --workspace --all-targets --locked` | **PASS (0 errors)** |
| **Gate 3: Clippy** | `cargo clippy --workspace --all-targets --locked -- -D warnings` | **PASS (0 warnings)** |
| **Gate 4: Tests** | `cargo test -p taskbar --test taskbar_foundation` | **PASS (8/8 passed)** |
| **Process Containment** | Win32 `IsProcessInJob` verification | **CONFIRMED** |
| **IPC Security** | Mutual PID auth / Standalone pipe removed | **CONFIRMED** |

---

## 5. Conclusion & Exit Recommendation

All findings from the M3A exit audit (`SEC-01`, `LIFECYCLE-01`, `INTEGRATION-01`, `TEST-01`, `WINDOW-01`, `DOC-01`) have been remediated with high engineering rigor. The codebase is fully formatted, warning-free, and backed by robust automated tests.

**VERDICT: M3A.1 COMPLETE — READY FOR INDEPENDENT RE-AUDIT**
