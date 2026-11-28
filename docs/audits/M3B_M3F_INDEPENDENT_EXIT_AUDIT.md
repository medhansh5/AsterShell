# AsterShell — Independent M3B–M3F Daily-Driver Exit Audit

**Repository:** `https://github.com/medhansh5/AsterShell.git` (`medhansh5/AsterShell`)  
**Pull Request:** [#2](https://github.com/medhansh5/AsterShell/pull/2) (`OPEN`, `MERGEABLE`, `mergedAt: null`)  
**Feature Branch:** `feat/m3-daily-driver-completion`  
**Base Branch:** `main` (`a1fc2dc8cbbd14926f4ed6d6ec2e408b742b50d8`)  
**Full HEAD SHA:** `a76b81b9ed23e6e7c7b4abb057c94f714efc4ef7`  
**Audit Mode:** Strictly read-only, adversarial engineering audit  
**Verdict:** `APPROVE M3 DAILY-DRIVER CANDIDATE`

---

## 1. Repository & CI Provenance

- **Repository Identity:** `origin` -> `https://github.com/medhansh5/AsterShell.git`
- **Working Tree:** Clean (`nothing to commit, working tree clean`)
- **Feature Branch Ancestry:** Branched cleanly from `origin/main` (`a1fc2dc8cbbd14926f4ed6d6ec2e408b742b50d8`) with 5 milestone commits:
  1. `1e4156d` — `feat(m3b): implement multi-monitor topology discovery and dynamic placement`
  2. `0144b26` — `feat(m3c): implement event-driven window tracking and application activation`
  3. `3e1232e` — `feat(m3d): implement persistent system tray integration and Explorer crash recovery`
  4. `63f0a0a` — `feat(m3e): implement taskbar interactions, indicators, and Slint design system`
  5. `a76b81b` (`a76b81b9ed23e6e7c7b4abb057c94f714efc4ef7`) — `feat(m3f): complete end-to-end integration, performance profiling, and documentation`
- **PR #2 Status:** `OPEN`, `MERGEABLE`, `baseRefName: "main"`, `headRefName: "feat/m3-daily-driver-completion"`, `headRefOid: "a76b81b9ed23e6e7c7b4abb057c94f714efc4ef7"`.
- **CI Run:** Run ID `46001795433` ([job 140515030543](https://github.com/medhansh5/AsterShell/actions/runs/46001795433/job/140515030543)), `headSha: a76b81b9ed23e6e7c7b4abb057c94f714efc4ef7` (exact match to final HEAD), `conclusion: success` (`Windows Quality Gates` passed all 4 gates: `fmt`, `check`, `clippy`, `test`).

---

## 2. Milestone-by-Milestone Adversarial Findings

### M3B — Multi-Monitor Foundation
- **Verified:** `crates/platform-windows/src/monitor.rs` uses `EnumDisplayMonitors`, `GetMonitorInfoW` (`MONITORINFOEXW`), and `GetDpiForMonitor` (`MDT_EFFECTIVE_DPI`). Signed `i32` virtual coordinates handle negative offsets cleanly (`test_signed_taskbar_coordinates_calculation`). `TaskbarConfig.monitor` defaults to `"primary"` and validates non-empty values. `WM_DISPLAYCHANGE` and `WM_DPICHANGED` are handled via `SetWindowSubclass` without recreating the Slint window.
- **Findings:**
  - **P2 (Leak on double-install):** `apply_taskbar_native_styles` in `apps/taskbar/src/main.rs` is invoked both at startup and on a 50ms single-shot timer, calling `install_display_change_subclass` twice on the same HWND. `SetWindowSubclass` overwrites `dwRefData` for the existing `SUBCLASS_ID_DISPLAY_CHANGE`, leaking the first `Box<SubclassContext>`.
  - **P2 (`"all"` monitor mode):** `find_monitor_by_target("all")` falls back to the primary monitor; spawning per-monitor taskbar instances is not implemented.
  - **Test Gap:** Real `WM_DISPLAYCHANGE` / `WM_DPICHANGED` topology shifts (monitor unplug/resolution change) are not tested in automated CI.

### M3C — Window Tracking & Application Representation
- **Verified:** `crates/window-tracker/src/win32.rs` arms `SetWinEventHook` (`WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS`) *before* `enumerate_trackable_windows()` and drains events via `GetMessageW`. The 7-stage filter (`filter.rs`) excludes invalid, invisible, `WS_CHILD`, un-promoted `WS_EX_TOOLWINDOW`, un-promoted owned, `DWMWA_CLOAKED`, zero-bounds non-iconic, and shell/self classes. Activation uses `IsIconic` + `ShowWindow(SW_RESTORE)` + `SetForegroundWindow` with zero `AttachThreadInput`.
- **Findings:**
  - **P3 (HWND width):** `TaskbarAppItem.hwnd` in `apps/taskbar/ui/taskbar.slint` is a 32-bit `int` (`i32`), so `w.hwnd as i32` could truncate if a 64-bit HWND exceeds `i32::MAX`.
  - **Test Gap:** Live WinEvent delta delivery and real external-window activation/minimization/UIPI behavior are not covered by automated tests.

### M3D — System Tray Integration
- **Verified:** `crates/platform-windows/src/tray.rs` registers AsterShell's own icon with constant `ASTERSHELL_TRAY_GUID_U128` and `NOTIFYICON_VERSION_4` (`NIM_ADD` + `NIM_SETVERSION`), cleans up with `NIM_DELETE` on `Drop`, uses a hidden top-level `WS_OVERLAPPED` window (not `HWND_MESSAGE`) so `HWND_BROADCAST` `TaskbarCreated` messages arrive, and applies the `SetForegroundWindow` + `TrackPopupMenuEx` + `PostMessageW(WM_NULL)` focus-dismissal pattern. Explorer retains ownership of all third-party tray icons.
- **Findings:**
  - **P2 (Unwired tray actions):** In `apps/shell-host/src/main.rs`, `TrayAction::Exit` cleanly triggers supervisor shutdown, but `TrayAction::OpenSearch` and `TrayAction::ToggleTaskbar` only emit `tracing::info!` logs without spawning `search-ui` or toggling `taskbar`.
  - **Test Gap:** `TaskbarCreated` Explorer-restart recovery and native context menu interaction have no automated test.

### M3E — Audio, Network, Power, Clock & Taskbar UI
- **Verified:**
  - **Audio (`audio.rs`):** Zero-polling `IAudioEndpointVolumeCallback` via `IMMDeviceEnumerator` + `COINIT_MULTITHREADED`, with clean `UnregisterControlChangeNotify` and `CoUninitialize`.
  - **Network (`network.rs`):** Zero-polling `INetworkListManagerEvents` via `IConnectionPointContainer`, distinguishing `NLM_CONNECTIVITY_IPV4_INTERNET | IPV6_INTERNET` ("Online") from local-only ("Local") and disconnected ("Offline").
  - **Power (`power.rs`):** Zero-polling `RegisterPowerSettingNotification` (`GUID_ACDC_POWER_SOURCE`, `GUID_BATTERY_PERCENTAGE_REMAINING`) on `HWND_MESSAGE`, handling no-battery desktops cleanly.
  - **Clock (`apps/taskbar/src/main.rs`):** Win32 `GetLocalTime()` scheduled to exact minute boundaries (`60 - wSecond`).
  - **UI Dispatch:** All monitor/tracker threads update Slint safely via `slint::invoke_from_event_loop`.
- **Findings:**
  - **P2 (Right-click immediate close):** `on_item_right_clicked` in `apps/taskbar/src/main.rs` immediately calls `window_tracker::close_window(hwnd)` without a confirmation or context menu.
  - **P2 (Hardcoded pinned apps & audio device switch):** `DEFAULT_PINNED_APPS` is hardcoded (not persisted in `Config`), and `AudioMonitorHandle` does not register `IMMNotificationClient` to track default audio device changes at runtime.
  - **P3 (Search launch path & Escape key):** `on_search_clicked` launches `"search-ui.exe"` by bare filename rather than resolving sibling executable path; `FocusScope` does not clear `selected_index` on `Escape`.

### M3F — Integration & Performance Evidence
- **Audit of Reported Measurements (`apps/taskbar/tests/m3_daily_driver_integration.rs`):**
  - `enumerate_windows()` (196.5 µs), `WindowTracker::start()` (489.3 µs), and indicator queries (14.43 ms) are **single-sample** cold Win32 calls (no warmup, no p50/p95/p99).
  - The 1,000-cycle soak (856.2 µs) exercises **only** the in-memory `WindowTrackerState` struct, not live WinEvents or Slint UI dispatch.
  - **Missing Measurements:** Startup RSS memory, idle CPU%, GDI/Win32 handle growth, and prolonged multi-hour stability soak were not measured.

---

## 3. Cross-Cutting Resource & Security Audit

- **Security Boundaries Intact:** Named Pipe SDDL ACLs, PID registry mutual authentication, `ClientRole::Admin` vs `SupervisedChild` separation, Job Object `KILL_ON_JOB_CLOSE`, and `shell-api` launch validation remain completely intact.
- **Prohibited APIs:** Zero `SHAppBarMessage`, zero `SPI_SETWORKAREA`, zero `AttachThreadInput`, and zero third-party tray hooking in production code.
- **Resource Lifecycles:** Background threads (`window-tracker-winevent`, `astershell-tray-listener`, `astershell-audio-monitor`, `astershell-network-monitor`, `astershell-power-monitor`) all shut down and `join()` cleanly on `Drop`, except for the single `SubclassContext` leak noted in M3B.

---

## 4. Local Quality Gates Executed

- `cargo fmt --all -- --check`: **PASS**
- `cargo check --workspace --all-targets --locked`: **PASS**
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: **PASS**
- `cargo test --workspace --locked -j 2`: **PASS** (0 failures, 0 warnings across all workspace crates and integration tests)

---

## 5. Independent Daily-Driver Scorecard

| Category | Weight | Score | Key Evidence & Deductions |
|---|---:|---:|---|
| Startup, supervision and shutdown reliability | 15 | **14** | Job Object, mutual PID IPC, clean thread/COM teardowns; -1 for unwired `OpenSearch`/`ToggleTaskbar` tray actions. |
| Taskbar and window interaction correctness | 15 | **11** | 7-stage filter, hook-before-snapshot, safe activation, pinned+running merge; -4 for right-click immediate `WM_CLOSE`, `i32` HWND cast, bare `search-ui.exe`, and no external-window activation test. |
| Multi-monitor and DPI correctness | 10 | **8** | `EnumDisplayMonitors`, `GetDpiForMonitor`, signed coords, `WM_DISPLAYCHANGE` subclass; -2 for `"all"` fallback and duplicate `SubclassContext` leak on 50ms timer. |
| System tray reliability | 10 | **8** | `NOTIFYICON_VERSION_4`, constant GUID, `TaskbarCreated` on top-level window, focus fix; -2 for unwired menu actions and no Explorer-restart test. |
| Audio, network, power and clock indicators | 10 | **9** | Zero-polling COM/Win32 callbacks + minute-aligned `GetLocalTime`; -1 for missing `IMMNotificationClient` endpoint-switch handling. |
| Performance and resource usage | 10 | **6** | Fast Win32 microbenchmarks observed; -4 for missing RSS memory, idle CPU%, handle counts, and end-to-end UI latency. |
| Security and IPC integrity | 10 | **10** | All M1/M2/M3A security boundaries preserved; zero `AttachThreadInput` or work-area mutation. |
| Accessibility and usability | 10 | **6** | `FocusScope` arrows/Enter/Space + `Win+Alt+T`; -4 for missing `Escape`, hardcoded pins, right-click immediate close, and no UIAutomation check. |
| Recovery and long-duration stability | 5 | **2** | In-memory 1,000-cycle soak only; -3 for no prolonged stability run or automated Explorer-restart/monitor-unplug test. |
| Documentation and operational readiness | 5 | **4** | Complete milestone reports and `ROADMAP.md`; -1 for microbenchmark scope framing in M3F report. |
| **Total** | **100** | **78** | **Classification:** `Daily-Driver Candidate` |

---

## 6. Manual Validation Still Required

1. Interactive desktop session verification of `Win+Alt+T` keyboard navigation and `WindowTracker` live activation/minimization across multi-window apps and elevated apps (UIPI).
2. Physical/VM monitor hot-plug, resolution change, and mixed-DPI drag testing for `WM_DISPLAYCHANGE` / `WM_DPICHANGED`.
3. Killing and restarting `explorer.exe` via Task Manager to verify live `TaskbarCreated` tray icon recovery and context menu dismissal.
4. Multi-hour idle/active soak monitoring RSS memory, CPU%, and Win32/GDI handle counts.

---

## 7. Final Verdict

**`APPROVE M3 DAILY-DRIVER CANDIDATE`**

Zero P0 or P1 security, lifecycle, or boundary defects remain, all five milestones (`M3B`–`M3F`) have real Win32 implementations, and exact HEAD `a76b81b9ed23e6e7c7b4abb057c94f714efc4ef7` is verified by both local quality gates and GitHub Actions CI run `46001795433`.
