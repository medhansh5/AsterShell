# AsterShell — Milestone 3: Implementation Plan & Staged Roadmap

**Milestone:** M3 — Custom Taskbar & System Tray Integration  
**Status:** DRAFT SPECIFICATION (APPROVED FOR M3 PREFLIGHT)  
**Target Platform:** Windows 11 (Build 22000+)  
**Baseline Commit:** `31a1a09fa5ab993c873133b891b28f7c3641dcdc`

---

## 1. Milestone Mission & Execution Strategy

The objective of Milestone 3 is to construct AsterShell's first custom, multi-monitor taskbar (`AsterTaskbar`) and persistent system tray integration subsystem (`AsterTray`).

To ensure architectural integrity, avoid scope creep, and maintain continuous automated verification in GitHub Actions CI, Milestone 3 is decomposed into six strictly sequential, independently auditable engineering stages:

```
+-------------------------------------------------------------------------------------------------------+
|                                    M3 STAGED IMPLEMENTATION ROADMAP                                   |
+-------------------------------------------------------------------------------------------------------+
|                                                                                                       |
|  [ M3A: Window Foundation & Supervisor Lifecycle ]                                                    |
|  * HWND creation, WS_POPUP / WS_EX_TOOLWINDOW / WS_EX_NOACTIVATE styles, DWM backdrops.                |
|  * Process supervision under shell-host, Job Object integration, mutual PID authentication.          |
|                                                                                                       |
|  [ M3B: Monitor Topology, Dynamic DPI & Positioning ]                                                 |
|  * EnumDisplayMonitors discovery, per-monitor window instantiation, WM_DISPLAYCHANGE handling.        |
|  * Per-Monitor V2 DPI scaling, physical coordinate mapping, multi-display repositioning.             |
|                                                                                                       |
|  [ M3C: Running Application Discovery & Activation ]                                                  |
|  * crates/window-tracker: SetWinEventHook listener thread, 7-stage HWND filtering pipeline.           |
|  * Window switching: ShowWindow(SW_RESTORE), SetForegroundWindow, AllowSetForegroundWindow.           |
|                                                                                                       |
|  [ M3D: AsterShell-Owned System Tray Infrastructure ]                                                 |
|  * crates/platform-windows/src/tray.rs: Shell_NotifyIconW with NOTIFYICON_VERSION_4 and GUID.          |
|  * TaskbarCreated broadcast listener, native Win32 context menu with focus fix.                       |
|                                                                                                       |
|  [ M3E: Taskbar Interactions, Clock & System Indicators ]                                             |
|  * Slint UI design system: Floating island layout, pinned apps, active indicator pills.              |
|  * Zero-polling indicators: Minute-synced clock, CoreAudio volume, Network List Manager, Battery.      |
|                                                                                                       |
|  [ M3F: Integration, Performance, Reliability & Release Audit ]                                       |
|  * End-to-end integration testing, idle CPU/memory profiling, crash resilience verification.          |
|  * Independent exit audit and GitHub release readiness.                                              |
|                                                                                                       |
+-------------------------------------------------------------------------------------------------------+
```

---

## 2. Measurable Performance Targets & Verification Methodology

Every performance claim in AsterShell must be verifiable and reproducible. The following matrix defines M3 targets, classification, and empirical measurement methodologies:

| Metric | Hard Requirement | Engineering Target | Aspirational Target | Measurement Methodology |
| :--- | :--- | :--- | :--- | :--- |
| **Cold Startup-to-Visible** | `< 120 ms` | `< 80 ms` | `< 50 ms` | High-resolution timestamp from `apps/taskbar` process launch to first `WM_PAINT` completion. |
| **Taskbar Click Latency** | `< 30 ms` | `< 16 ms` (1 frame) | `< 8 ms` | Latency from `WM_LBUTTONDOWN` to Slint UI state update emission. |
| **Application Switch Latency** | `< 50 ms` | `< 25 ms` | `< 15 ms` | Latency from click on taskbar item to `EVENT_SYSTEM_FOREGROUND` receipt for target HWND. |
| **WinEvent Pipeline Latency** | `< 15 ms` | `< 5 ms` | `< 2 ms` | Timestamp difference between WinEvent hook callback invocation and `WindowListUpdate` delivery to UI. |
| **Idle CPU Usage** | `< 0.5%` | `< 0.1%` | `0.0%` (0 wakeups) | Average Windows Performance Monitor (`perfmon`) or `GetProcessTimes` sampling over 5-minute idle. |
| **Memory Footprint (RSS)** | `< 50 MB` | `< 35 MB` | `< 25 MB` | Working Set Private memory reported by `GetProcessMemoryInfo` after 10 minutes runtime. |
| **Monitor Change Recovery** | `< 100 ms` | `< 50 ms` | `< 20 ms` | Timestamp from `WM_DISPLAYCHANGE` receipt to all taskbar windows repositioned and repainted. |
| **Explorer Crash Recovery** | `< 50 ms` | `< 20 ms` | `< 10 ms` | Timestamp from `TaskbarCreated` message receipt to `Shell_NotifyIconW(NIM_ADD)` completion. |
| **Supervisor Crash Recovery** | `< 500 ms` | `< 250 ms` | `< 150 ms` | Timestamp from `apps/taskbar` process termination to new instance visible on screen. |

---

## 3. Detailed Milestone Decomposition

### 3.1 Milestone 3A: Native Taskbar Window Foundation & Lifecycle

- **Objective:** Establish the native Win32 window foundation for `apps/taskbar`, configure window styles, and bind it to the `shell-host` supervisor under strict companion mode.
- **Scope:**
  - Implement native window creation via Slint/Winit with `WS_POPUP`, `WS_CLIPSIBLINGS`, `WS_CLIPCHILDREN`.
  - Apply extended styles: `WS_EX_TOPMOST`, `WS_EX_TOOLWINDOW`, `WS_EX_NOACTIVATE`.
  - Implement DWM backdrop configuration (Mica / Acrylic) with automatic fallback to dark opaque/translucent styling if unsupported.
  - Implement signed `i32` primary-monitor coordinate placement.
  - Implement `apps/taskbar` supervisor client: mutual PID authentication and handshake with `shell-host`.
  - Handle clean shutdown signals (`IpcRequest::Shutdown`) and Windows Job Object termination.
  - **Explicit Negative Scope:** Strictly no AppBar registration (`SHAppBarMessage`), no work-area modification, no WinEvent hooks, no tray icons, and no telemetry indicators.
- **Dependencies:** `crates/platform-windows`, `crates/ipc`, `apps/shell-host`.
- **Files & Crates Affected:**
  - `apps/taskbar/src/main.rs`
  - `apps/taskbar/ui/taskbar.slint`
  - `apps/taskbar/Cargo.toml`
  - `crates/platform-windows/src/window.rs`
  - `crates/ipc/src/lib.rs`

#### Detailed Behavioral Acceptance Criteria:

##### Automated Test Suite (Headless / CI Verifiable)
1. **AC-M3A-01 (Supervised Lifecycle & Job Object Binding):**
   - *What must work:* `apps/taskbar` spawns under `shell-host`, authenticates its PID over Named Pipe IPC (`HandshakeAck`), and attaches to the supervisor Job Object.
   - *Test Method:* Automated integration test in `tests/supervisor_taskbar_integration.rs`. Terminate supervisor; assert child process terminates automatically.
   - *Expected Result:* Child attaches in < 100 ms; child exits cleanly when supervisor dies.
   - *Failure Conditions:* Child orphans, fails handshake, or remains alive after supervisor termination.
2. **AC-M3A-02 (Native Frameless Window Styles):**
   - *What must work:* HWND is created with `WS_POPUP | WS_VISIBLE | WS_CLIPSIBLINGS | WS_CLIPCHILDREN`.
   - *Test Method:* Automated Win32 test querying `GetWindowLongPtrW(hwnd, GWL_STYLE)`.
   - *Expected Result:* Style bitmask matches exactly; no titlebar or window borders present.
   - *Failure Conditions:* Titlebar present, resizable frame present, missing clipping bits.
3. **AC-M3A-03 (Extended Shell Styles & Alt+Tab Exclusion):**
   - *What must work:* HWND has `WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`.
   - *Test Method:* Automated Win32 test querying `GetWindowLongPtrW(hwnd, GWL_EXSTYLE)`.
   - *Expected Result:* `WS_EX_TOOLWINDOW` and `WS_EX_NOACTIVATE` bits set. Window does not register as a taskbar application or Alt+Tab entry.
   - *Failure Conditions:* Window appears in Alt+Tab; missing extended style bits.
4. **AC-M3A-04 (Zero Desktop Work-Area Modification):**
   - *What must work:* Desktop work area (`SPI_SETWORKAREA`) is left completely unmodified.
   - *Test Method:* Automated test queries `SystemParametersInfoW(SPI_GETWORKAREA)` immediately before taskbar launch and immediately after window realization.
   - *Expected Result:* `rcWork` rectangle is bit-for-bit identical before and after taskbar instantiation.
   - *Failure Conditions:* `rcWork` contracts or expands; `SHAppBarMessage` called.
5. **AC-M3A-05 (Signed Primary Monitor Placement):**
   - *What must work:* Taskbar window positions at the top of the primary monitor using signed `i32` physical pixel coordinates.
   - *Test Method:* Query `GetWindowRect(hwnd)` and compare against `GetMonitorInfoW(hmonitor_primary)`.
   - *Expected Result:* Window X matches `rcMonitor.left + offset`, Window Y matches `rcMonitor.top`.
   - *Failure Conditions:* Negative coordinates clamped to 0; window placed off-screen or misplaced.
6. **AC-M3A-06 (DWM Backdrop Realization & Graceful Fallback):**
   - *What must work:* Applies `DWMWA_SYSTEMBACKDROP_TYPE` (Mica) and `DWMWA_USE_IMMERSIVE_DARK_MODE`. If DWM fails or runs on Windows 10/Server, falls back cleanly to Slint background color without crashing.
   - *Test Method:* Automated unit test mocking unsupported DWM returns; assert `Ok(())` fallback.
   - *Expected Result:* Returns `Ok(())` in all environments; Mica active on Windows 11.
   - *Failure Conditions:* Panic or unhandled error on older Windows or Server builds.
7. **AC-M3A-07 (Clean IPC Shutdown & Restart):**
   - *What must work:* Exits cleanly within 500 ms when receiving `IpcRequest::Shutdown`.
   - *Test Method:* Integration test issuing `IpcRequest::Shutdown` over pipe.
   - *Expected Result:* Process exits with code 0; all resources freed.
   - *Failure Conditions:* Process hangs, leaks handles, or requires forceful kill.

##### Manual Desktop Validation (Interactive Windows 11 Session)
1. **MV-M3A-01 (No Unintended Foreground Activation):**
   - Click a text document in Notepad or VS Code to place typing cursor.
   - Click the AsterTaskbar background or placeholder item.
   - *Expected Result:* Notepad/VS Code remains the active foreground window; typing cursor continues blinking without focus theft.
2. **MV-M3A-02 (Explorer Taskbar Coexistence):**
   - Launch AsterTaskbar while Windows 11 Explorer taskbar is visible at bottom.
   - *Expected Result:* Windows 11 taskbar remains 100% visible, interactive, unclipped, and fully responsive to Start menu and system tray clicks.
3. **MV-M3A-03 (Alt+Tab Verification):**
   - Press `Alt+Tab`.
   - *Expected Result:* AsterTaskbar does NOT appear as an entry in the Alt+Tab switcher.
4. **MV-M3A-04 (Topmost & Fullscreen Yielding):**
   - Launch a fullscreen video (F11 in browser).
   - *Expected Result:* AsterTaskbar yields Z-order (`HWND_BOTTOM`) and does not occlude fullscreen playback.

- **Exit Criteria:** All 7 automated CI tests pass 100% green; all 4 manual desktop tests verified on Windows 11.

---

### 3.2 Milestone 3B: Monitor Topology, Dynamic DPI & Positioning

- **Objective:** Deliver robust multi-monitor discovery, per-monitor window instantiation, and dynamic Per-Monitor V2 DPI scaling.
- **Scope:**
  - Implement display monitor enumeration (`EnumDisplayMonitors` / `GetMonitorInfoW`).
  - Implement per-monitor Slint `TaskbarWindow` component instantiation.
  - Implement hidden system broadcast listener window (`HWND_MESSAGE`).
  - Handle `WM_DISPLAYCHANGE`, `WM_DPICHANGED`, and `WM_SETTINGCHANGE`.
  - Support configurable positioning (top edge `ABE_TOP`, floating offset, primary-only vs all-monitors).
- **Dependencies:** M3A baseline.
- **Files & Crates Affected:**
  - `crates/platform-windows/src/monitor.rs`
  - `crates/platform-windows/src/lib.rs`
  - `apps/taskbar/src/main.rs`
  - `apps/taskbar/src/monitor_manager.rs`
- **Acceptance Criteria:**
  1. Multi-monitor systems instantiate distinct, correctly sized taskbar instances on every active monitor.
  2. Disconnecting or connecting a display dynamically destroys or spawns instances in < 50 ms.
  3. Changing display resolution or DPI updates taskbar geometry and vector rendering with zero blur.
  4. Primary monitor designation changes (`MONITORINFOF_PRIMARY`) reassign the primary status area seamlessly.
- **Tests:**
  - Unit tests: Multi-monitor coordinate bounding math, DPI scaling formulas.
  - Integration tests: Monitor enumeration mock tests, DPI conversion tests.
- **Exit Criteria:** Multi-monitor and DPI scaling verified across varied display scales (100%, 125%, 150%, 200%).

---

### 3.3 Milestone 3C: Running Application Discovery & Activation

- **Objective:** Implement the real-time event-driven window tracking engine (`crates/window-tracker`) and reliable application switching.
- **Scope:**
  - Implement dedicated STA WinEvent Hook thread with native message pump.
  - Register `SetWinEventHook` for creation, destruction, show, hide, title change, minimize, cloaking, and foreground events.
  - Implement the 7-stage HWND comprehensive filtering pipeline.
  - Implement thread-safe window cache emitting diffs via lock-free channels to the UI thread.
  - Implement application activation (`ShowWindow(SW_RESTORE)` + `SetForegroundWindow`).
  - Extract application icons (Win32 `WM_GETICON`, `GetClassLongPtrW`, `ExtractIconExW`, and AUMID resolution).
- **Dependencies:** M3A, M3B baselines.
- **Files & Crates Affected:**
  - `crates/window-tracker/src/lib.rs`
  - `crates/window-tracker/src/filter.rs`
  - `crates/window-tracker/src/hook.rs`
  - `crates/window-tracker/src/icons.rs`
  - `apps/taskbar/src/main.rs`
- **Acceptance Criteria:**
  1. Window list populates open applications upon startup in < 30 ms.
  2. Launching or closing applications updates the taskbar in < 5 ms via events (zero polling).
  3. Cloaked windows on other virtual desktops are correctly excluded.
  4. Clicking a taskbar button restores and brings the target application to foreground smoothly.
  5. Clicking an already active foreground application minimizes it.
- **Tests:**
  - Unit tests: HWND style filtering logic, virtual desktop cloaking checks.
  - Integration tests: Window event pipeline benchmarks, icon extraction fallbacks.
- **Exit Criteria:** Event-driven window tracking verified without dropped events; idle CPU remains under 0.1%.

---

### 3.4 Milestone 3D: AsterShell-Owned System Tray Infrastructure

- **Objective:** Build AsterShell's persistent system notification area presence and crash recovery subsystem.
- **Scope:**
  - Implement `crates/platform-windows/src/tray.rs` wrapping `Shell_NotifyIconW`.
  - Register AsterShell tray icon using `NOTIFYICON_VERSION_4` and constant GUID.
  - Implement listener window for tray callbacks (`WM_LBUTTONUP`, `WM_RBUTTONUP`, `NIN_KEYSELECT`).
  - Implement native Win32 popup context menu with focus fix.
  - Implement `TaskbarCreated` broadcast listener for Explorer crash recovery.
  - Integrate tray management into `apps/shell-host`.
- **Dependencies:** `crates/platform-windows`, `apps/shell-host`.
- **Files & Crates Affected:**
  - `crates/platform-windows/src/tray.rs`
  - `apps/shell-host/src/main.rs`
  - `apps/shell-host/src/tray_manager.rs`
- **Acceptance Criteria:**
  1. AsterShell icon appears in the Windows system tray upon supervisor startup.
  2. Left-clicking toggles the Spotlight launcher; right-clicking opens the native context menu.
  3. Context menu dismisses cleanly when clicking away (focus fix verified).
  4. Killing and restarting `explorer.exe` restores the AsterShell tray icon in < 20 ms.
- **Tests:**
  - Unit tests: Tray message unpacker, GUID serialization.
  - Integration tests: Tray lifecycle tests, mock `TaskbarCreated` broadcast tests.
- **Exit Criteria:** Tray icon persists cleanly through Explorer restarts with zero memory leaks.

---

### 3.5 Milestone 3E: Taskbar Interactions, Clock & System Indicators

- **Objective:** Finalize AsterTaskbar UI layout, interaction design, pinned applications, and zero-polling system status indicators.
- **Scope:**
  - Implement Slint UI design system for Concept 1 (Floating Island) and Concept 2 (Edge AppBar).
  - Implement pinned applications grid loaded from `crates/configuration`.
  - Implement active and running indicator pills/dots with 120ms transitions.
  - Implement minute-aligned digital clock widget (`GetLocalTime`).
  - Implement CoreAudio master volume event sink (`IAudioEndpointVolume`).
  - Implement Network List Manager connectivity event sink (`INetworkListManager`).
  - Implement Win32 battery and power state listener (`GetSystemPowerStatus`).
  - Implement taskbar context menus (Close, Minimize, Maximize, Unpin).
- **Dependencies:** M3A through M3D.
- **Files & Crates Affected:**
  - `apps/taskbar/ui/taskbar.slint`
  - `apps/taskbar/src/indicators.rs`
  - `apps/taskbar/src/ui_adapter.rs`
  - `crates/platform-windows/src/audio.rs`
  - `crates/platform-windows/src/network.rs`
  - `crates/platform-windows/src/power.rs`
- **Acceptance Criteria:**
  1. Slint UI renders polished floating island taskbar matching design tokens.
  2. Clock updates precisely on the minute boundary with zero drift.
  3. Changing system volume or plugging in power updates indicators in < 10 ms with zero polling.
  4. Middle-clicking a pinned application launches a new instance.
- **Tests:**
  - Unit tests: Clock alignment math, volume percentage conversions.
  - Integration tests: UI event callback tests, config reload tests.
- **Exit Criteria:** Complete, interactive taskbar rendering cleanly across all monitors.

---

### 3.6 Milestone 3F: Integration, Performance, Reliability & Release Audit

- **Objective:** Conduct end-to-end integration validation, performance profiling, stress testing, and prepare documentation for independent exit audit.
- **Scope:**
  - Measure all 9 performance metrics against hard requirements and engineering targets.
  - Perform 1-hour continuous stress test (rapid window creation/destruction, resolution shifts, monitor hot-plugging).
  - Verify zero memory leaks via `GetProcessMemoryInfo` telemetry.
  - Ensure 100% green pass on all 4 GitHub Actions CI gates (`cargo fmt`, `cargo check --locked`, `cargo clippy --locked -D warnings`, `cargo test --locked`).
  - Compile M3 Engineering Report and prepare for independent exit audit.
- **Dependencies:** All preceding stages M3A–M3E.
- **Files & Crates Affected:** Full workspace documentation and tests.
- **Acceptance Criteria:**
  1. All 9 performance requirements verified and documented.
  2. 1-hour stress test completes with 0 crashes, 0 deadlocks, and stable memory.
  3. GitHub Actions CI pipeline passes 100% green on Windows runners.
- **Exit Criteria:** Ready for independent exit audit authorization.

---

## 4. References & Documentation Links
- [M3 Taskbar Architecture Specification](file:///C:/Users/medha/.gemini/antigravity/scratch/AsterShell/docs/architecture/M3_TASKBAR_ARCHITECTURE.md)
- [M3 System Tray Architecture Specification](file:///C:/Users/medha/.gemini/antigravity/scratch/AsterShell/docs/architecture/M3_SYSTEM_TRAY_ARCHITECTURE.md)
- [M3 Windows API Research Specification](file:///C:/Users/medha/.gemini/antigravity/scratch/AsterShell/docs/architecture/M3_WINDOWS_API_RESEARCH.md)
- [M3 UI & Interaction Specification](file:///C:/Users/medha/.gemini/antigravity/scratch/AsterShell/docs/architecture/M3_UI_AND_INTERACTION_SPEC.md)
