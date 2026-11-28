# AsterShell — M3 Post-Audit Remediation & Finalization Report

**Repository:** `medhansh5/AsterShell`  
**Branch:** `feat/m3-daily-driver-completion`  
**Pull Request:** [#2](https://github.com/medhansh5/AsterShell/pull/2) (`OPEN`, unmerged)  
**Baseline Audit:** [M3B_M3F_INDEPENDENT_EXIT_AUDIT.md](file:///C:/Users/medha/.gemini/antigravity/scratch/AsterShell/docs/audits/M3B_M3F_INDEPENDENT_EXIT_AUDIT.md) (Previous score: `78 / 100`)  
**Outcome:** `M3 REMEDIATION COMPLETE — READY FOR FINAL REVIEW`  
**Recalculated Score:** **91 / 100** (`Daily-Driver Candidate`)

---

## 1. Summary of Remediated Findings

| Finding | Root Cause | Actual Correction | Files Changed | Verification |
|---|---|---|---|---|
| **M3B SubclassContext leak** | `apply_taskbar_native_styles` called `SetWindowSubclass` twice (startup + 50ms timer), overwriting `dwRefData` without freeing the first `Box<SubclassContext>`. | Replaced raw pointer ownership with a thread-safe `SUBCLASS_REGISTRY` (`Mutex<HashMap<isize, Arc<SubclassContext>>>`) and added `remove_display_change_subclass` + single-install gating on the 50ms timer. | `crates/platform-windows/src/monitor.rs`, `crates/platform-windows/src/lib.rs`, `apps/taskbar/src/main.rs` | `test_install_display_change_subclass_idempotent_no_leak` (10/10 contexts dropped, 0 leaks, 0 double-free) — commit `3bc0005` |
| **M3C `i32` HWND truncation** | `TaskbarAppItem.hwnd` used Slint's 32-bit `int`, casting `isize -> i32`. | Added `HwndHandleTable` mapping 32-bit UI `item_id`s to full pointer-width `isize` HWNDs; added `is_running: bool` in `taskbar.slint`. | `apps/taskbar/ui/taskbar.slint`, `apps/taskbar/src/main.rs` | `test_hwnd_pointer_width_preserved_above_i32_max` — commit `071542e` |
| **M3B `"all"` monitor mode** | `find_monitor_by_target("all")` collapsed to primary monitor. | Implemented `MonitorTargetMode::All`, `resolve_target_monitors`, `reconcile_monitor_surfaces`, and per-monitor `TaskbarWindow` lifecycle (`EXTRA_SURFACES`) with live `WM_DISPLAYCHANGE` reconciliation. | `crates/platform-windows/src/monitor.rs`, `crates/platform-windows/src/lib.rs`, `apps/taskbar/src/main.rs` | `test_all_mode_and_surface_reconciliation` — commit `dc4b111` |
| **M3D unwired `TrayAction`s** | `TrayAction::OpenSearch` and `TrayAction::ToggleTaskbar` only logged in `shell-host`. | Added `IpcResponse::ToggleTaskbarNotice`, `dispatch_tray_action`, sibling `search-ui.exe` launch, and taskbar visibility toggle over authenticated IPC. | `crates/ipc/src/lib.rs`, `apps/shell-host/src/main.rs`, `apps/taskbar/src/main.rs` | `test_tray_action_dispatch_all_handlers` — commit `b9d3b13` |
| **M3E immediate right-click `WM_CLOSE`** | `on_item_right_clicked` closed windows directly on right-click. | Added `show_window_item_context_menu` (`CreatePopupMenu` + `SetForegroundWindow` + `TPM_RETURNCMD` + `PostMessageW(WM_NULL)`) offering `Activate / Restore`, `Minimize`, and `Close Window`, plus `Escape` selection clearing in `taskbar.slint`. | `crates/platform-windows/src/window.rs`, `crates/platform-windows/src/lib.rs`, `apps/taskbar/src/main.rs`, `apps/taskbar/ui/taskbar.slint` | `cargo test -p taskbar` + `cargo clippy` — commit `b9d3b13` |
| **M3E audio endpoint switching** | `AudioMonitorHandle` did not implement `IMMNotificationClient`. | Implemented `#[implement(IMMNotificationClient)]` (`OnDefaultDeviceChanged`, `OnDeviceRemoved`, `OnDeviceStateChanged`), unregistering old `IAudioEndpointVolumeCallback` before releasing old COM interfaces. | `crates/platform-windows/Cargo.toml`, `crates/platform-windows/src/audio.rs` | `test_endpoint_rebind_tracker_prevents_duplicate_registrations`, `test_audio_monitor_start_and_clean_drop` — commit `ba170be` |
| **M3E hardcoded pinned apps** | `DEFAULT_PINNED_APPS` was hardcoded in `apps/taskbar/src/main.rs`. | Added `PinnedAppConfig` + `TaskbarConfig.pinned_apps` with `#[serde(default = "default_pinned_apps")]` backward compatibility and `validate_pinned_apps` rejecting duplicates and command injection. | `crates/configuration/src/lib.rs`, `apps/taskbar/src/main.rs` | `test_pinned_apps_validation_and_backward_compat` — commit `c8723b6` |
| **M3F idle resource & docs** | Missing RSS/CPU/handle measurements; stale AppBar/M3 claims in `README.md` / `ARCHITECTURE.md`. | Created `scripts/measure_idle_resources.ps1`, measured `shell-host.exe` and `taskbar.exe` on Windows 11 Build 26300, updated `README.md` & `ARCHITECTURE.md`, and added `docs/guides/{BUILD_AND_RUN,TROUBLESHOOTING,MANUAL_VALIDATION_CHECKLIST,LIMITATIONS}.md`. | `scripts/measure_idle_resources.ps1`, `README.md`, `ARCHITECTURE.md`, `docs/guides/*`, `docs/audits/*` | `measure_idle_resources.ps1` + workspace quality gates |

---

## 2. Measured Idle Resource Usage (`scripts/measure_idle_resources.ps1`)

- **Environment:** Microsoft Windows 11 Pro (Build 26300, Version `10.0.26300`), 12th Gen Intel Core i5-12450H (12 logical cores), 15.68 GB RAM (physical hardware).
- **Method:** Release binaries (`target\release\shell-host.exe` auto-spawning `target\release\taskbar.exe` under Job Object), 2s warmup, 10 samples at 1s intervals (10s total), followed by 3 start/stop cycles.

| Process | RSS p50 / p95 / Peak | Private Mem (p50) | Handles (Start -> End / Peak) | Threads (Peak) | Idle CPU (p50 / Peak) |
|---|---:|---:|---:|---:|---:|
| `shell-host.exe` | 12.23 MB / 12.23 MB / 12.23 MB | 2.27 MB | 151 -> 151 / 151 | 18 | 0% / 0% |
| `taskbar.exe` | 70.26 MB / 70.26 MB / 70.26 MB | 46.12 MB | 416 -> 416 / 416 | 33 | 0% / 0% |

- **3-Cycle Start/Stop Job Object Verification:** `taskbar.exe` RSS `57.49 MB` / `47.32 MB` / `57.41 MB`, `orphaned_after_host_stop = 0` in all 3 cycles.

---

## 3. Recalculated Daily-Driver Scorecard

| Category | Weight | Before | After | Rationale |
|---|---:|---:|---:|---|
| Startup, supervision and shutdown reliability | 15 | 14 | **15** | `TrayAction` handlers wired; 3-cycle start/stop verified 0 orphans. |
| Taskbar and window interaction correctness | 15 | 11 | **13** | Right-click context menu, 64-bit `HwndHandleTable`, sibling `search-ui.exe`, validated `pinned_apps`; -2 for no automated external-window activation test. |
| Multi-monitor and DPI correctness | 10 | 8 | **9** | Leak-free `SUBCLASS_REGISTRY`, `"all"` multi-surface reconciliation; -1 for no automated monitor hot-plug test. |
| System tray reliability | 10 | 8 | **9** | All tray actions wired & tested; -1 for no automated Explorer-kill `TaskbarCreated` test. |
| Audio, network, power and clock indicators | 10 | 9 | **10** | `IMMNotificationClient` endpoint switching added with unregister-before-release. |
| Performance and resource usage | 10 | 6 | **9** | Real Windows 11 RSS, handle, thread, and idle CPU measurements recorded; -1 for 10s sample window vs multi-hour soak. |
| Security and IPC integrity | 10 | 10 | **10** | M1/M2/M3A boundaries intact + `validate_pinned_apps` injection guards. |
| Accessibility and usability | 10 | 6 | **8** | `Escape` selection clear, `Win+Alt+T`, context menu, configurable pins; -2 for no UIAutomation audit. |
| Recovery and long-duration stability | 5 | 2 | **3** | 3-cycle Job Object start/stop + 1,000-cycle state soak; -2 for no multi-hour real-world desktop soak. |
| Documentation and operational readiness | 5 | 4 | **5** | Updated `README.md`, `ARCHITECTURE.md`, and added build/troubleshooting/manual-validation/limitations guides. |
| **Total** | **100** | **78** | **91** | **Classification:** `Daily-Driver Candidate` |

---

## 4. Remaining Limitations & Manual Validation

- Interactive desktop validation per [MANUAL_VALIDATION_CHECKLIST.md](file:///C:/Users/medha/.gemini/antigravity/scratch/AsterShell/docs/guides/MANUAL_VALIDATION_CHECKLIST.md) (physical audio endpoint switching, monitor hot-plug, Explorer restart recovery, multi-hour soak) is required before classifying as a **Validated Daily Driver**.
- All limitations are documented in [LIMITATIONS.md](file:///C:/Users/medha/.gemini/antigravity/scratch/AsterShell/docs/guides/LIMITATIONS.md).
