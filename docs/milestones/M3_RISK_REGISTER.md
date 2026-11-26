# AsterShell — Milestone 3: Technical Risk Register & Failure Mode Analysis

**Milestone:** M3 — Custom Taskbar & System Tray Integration  
**Component:** Full System (`AsterTaskbar`, `AsterTray`, `window-tracker`, `shell-host`)  
**Status:** DRAFT SPECIFICATION (APPROVED FOR M3 PREFLIGHT)  
**Target Platform:** Windows 11 (Build 22000+)  
**Baseline Commit:** `31a1a09fa5ab993c873133b891b28f7c3641dcdc`

---

## 1. Risk Matrix Overview

The following matrix categorizes all identified technical risks across Severity and Likelihood:

```
+---------------------------------------------------------------------------------------+
| LIKELIHOOD \ SEVERITY | LOW                 | MEDIUM              | HIGH              |
+-----------------------+---------------------+---------------------+-------------------+
| HIGH                  |                     | RISK-04 (DPI Drift) | RISK-01 (WinEvent)|
| MEDIUM                | RISK-09 (Telemetry) | RISK-03 (Focus Lock)| RISK-02 (Coexist) |
| LOW                   | RISK-08 (Security)  | RISK-07 (Explorer)  | RISK-05 (Slint TDR|
+---------------------------------------------------------------------------------------+
```

---

## 2. Comprehensive Risk Register

### RISK-01: WinEvent Hook Event Flooding & Thread Starvation
- **Description:** During rapid window resizing, dragging, or heavy multi-window compiling/gaming, Windows can fire hundreds of `EVENT_OBJECT_LOCATIONCHANGE` or `EVENT_OBJECT_NAMECHANGE` events per second. If unthrottled, processing every event could saturate the message loop, spike CPU usage, and cause UI stuttering.
- **Severity:** HIGH  
- **Likelihood:** HIGH  
- **Impact:** Slint UI frame drops, latency spikes, and degraded system responsiveness.
- **Mitigation Strategy:**
  1. **Strict Event Filtering:** Explicitly omit `EVENT_OBJECT_LOCATIONCHANGE` from hook registration. Only subscribe to discrete lifecycle events (`CREATE`, `DESTROY`, `SHOW`, `HIDE`, `FOREGROUND`, `CLOAKED`, `UNCLOAKED`, `NAMECHANGE`).
  2. **Debouncing Window Name Updates:** Apply a 50ms trailing debounce on `EVENT_OBJECT_NAMECHANGE` before updating the in-memory window cache.
  3. **Dedicated STA Thread:** Run the WinEvent hook on a dedicated worker thread with its own message pump, completely isolated from the main UI thread.
  4. **Batch Channel Dispatch:** Send batched window list diffs to the UI thread via crossbeam channel with a maximum refresh frequency of 60 Hz.
- **Fallback:** If an event flood is detected (> 200 events in 100ms), temporarily enter a coalesced reconciliation mode where the worker thread discards intermediate events and performs a single clean reconciliation pass.

---

### RISK-02: Work-Area Fighting Between AsterTaskbar & Windows Explorer
- **Description:** If AsterTaskbar registers as a formal Windows AppBar (`SHAppBarMessage` `ABM_NEW`, `ABM_SETPOS`) while Explorer's taskbar is also running as an AppBar, race conditions or conflicting `SPI_SETWORKAREA` calculations can cause desktop window bouncing, infinite resize loops, or clipped maximized windows.
- **Severity:** HIGH  
- **Likelihood:** MEDIUM  
- **Impact:** Desktop visual flickering, maximized windows jumping between coordinates, unstable work area.
- **Mitigation Strategy:**
  1. **Strict M3A Floating Companion Mode:** In M3A, AsterTaskbar strictly operates as a floating companion window (`WS_EX_TOPMOST`) with zero work-area contraction. Calling `SHAppBarMessage` (`ABM_NEW`, `ABM_QUERYPOS`, `ABM_SETPOS`) is strictly **prohibited** in M3.
  2. **Work Area Untouched:** Desktop `SPI_SETWORKAREA` is preserved 100% without modification, completely eliminating any work-area fighting or oscillation with Explorer.
  3. **Future Edge Mode Deferral:** Edge-anchored AppBar docking is formally deferred to Milestone 6 (Full Shell Replacement) when Explorer is replaced.
- **Fallback:** If an external process alters the work area, AsterTaskbar reads `SPI_GETWORKAREA` passively and remains floating above application windows.

---

### RISK-03: Application Activation Rejection via Windows Foreground Lockdown & UIPI
- **Description:** Calling `SetForegroundWindow(target_hwnd)` when a user clicks a taskbar button may be rejected by Windows (`LockSetForegroundWindow`) or blocked by User Interface Privilege Isolation (UIPI) when targeting elevated windows.
- **Severity:** MEDIUM  
- **Likelihood:** MEDIUM  
- **Impact:** Target window fails to activate or merely flashes in the taskbar.
- **Mitigation Strategy:**
  1. **Input Event Privilege:** Ensure AsterTaskbar processes activation strictly within the mouse click handler (`WM_LBUTTONUP`). Windows explicitly grants foreground activation privilege to the process that received the last physical input event.
  2. **Restore Iconic Windows:** Always check `IsIconic(target_hwnd)` and issue `ShowWindow(target_hwnd, SW_RESTORE)` prior to calling `SetForegroundWindow`.
  3. **UIPI Boundary Recognition:** Acknowledge that standard-user shells cannot elevate focus to Administrator windows under Windows UIPI rules; handle flashing as documented platform behavior.
  4. **Strict Ban on `AttachThreadInput`:** Never use `AttachThreadInput` as a shortcut; it causes deadlocks if the target window hangs.
- **Fallback:** If `SetForegroundWindow` returns false, invoke `AllowSetForegroundWindow(target_pid)` and flash the AsterTaskbar button to provide clear visual feedback to the user.

---

### RISK-04: Multi-Monitor DPI Coordinate Drift & Geometry Desynchronization
- **Description:** In multi-monitor configurations with mixed DPI scales (e.g., 4K at 150% alongside 1080p at 100%), window coordinates spanning the virtual desktop can experience rounding errors, pixel gaps, or misplaced taskbar windows during monitor connect/disconnect events.
- **Severity:** MEDIUM  
- **Likelihood:** HIGH  
- **Impact:** Misaligned taskbar, blurred icons, or off-screen window placement.
- **Mitigation Strategy:**
  1. **PerMonitorV2 Awareness:** Explicitly declare `DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2` in the application manifest.
  2. **Signed `i32` Coordinate Math:** All coordinate calculations, monitor bounds, and offsets must strictly use signed 32-bit integers (`i32`) to properly support negative offsets on monitors positioned left of or above the primary display.
  3. **Physical Coordinate Calculation:** Compute all window coordinates using integer physical device pixels derived from `MONITORINFOEXW.rcMonitor` rather than logical scaled floats.
  4. **Dynamic `WM_DPICHANGED` Handling:** Handle `WM_DPICHANGED` by reading the suggested rectangle in `lParam` and updating physical dimensions via `SetWindowPos`.
- **Fallback:** In the event of a coordinate mismatch, re-query all monitors via `EnumDisplayMonitors` and snap taskbar windows to the exact physical bounding box reported by `GetMonitorInfoW`.

---

### RISK-05: Slint Multi-Window Rendering Stall or GPU Driver Timeout (TDR)
- **Description:** Instantiating multiple Slint `Window` instances across multiple monitors on the main UI thread could encounter a GPU driver reset (TDR) or frame pacing stutter if rendering surfaces are recreated simultaneously during resolution changes.
- **Severity:** HIGH  
- **Likelihood:** LOW  
- **Impact:** Taskbar UI freezes or crashes.
- **Mitigation Strategy:**
  1. **Supervised Process Isolation:** `apps/taskbar` executes in a separate process under `shell-host`. A UI crash has zero blast radius on the supervisor daemon or search launcher.
  2. **Supervisor Watchdog:** `shell-host` detects child process termination via Windows Job Object and async wait handles, automatically restarting `apps/taskbar` within 200 ms with exponential backoff.
  3. **Low Complexity Shaders:** AsterTaskbar UI relies on simple geometric rectangles, vector glyphs, and native DWM Mica backdrops rather than heavy client-side blur shaders.
- **Fallback:** If Slint GPU backend fails, fall back to Slint software renderer (`backend-femtovg` software fallback).

---

### RISK-06: Stale HWND Handles & Race Conditions on Window Destruction
- **Description:** A window can be destroyed by its owning application between the time `crates/window-tracker` processes an event and the time the user clicks the corresponding taskbar button, resulting in stale HWND operations or error returns.
- **Severity:** LOW  
- **Likelihood:** MEDIUM  
- **Impact:** Benign Win32 error returns (`ERROR_INVALID_WINDOW_HANDLE`).
- **Mitigation Strategy:**
  1. **Liveness Validation:** Before any window operation (`ShowWindow`, `SetForegroundWindow`), check `IsWindow(hwnd)` atomically.
  2. **Immediate Removal:** Upon receiving `EVENT_OBJECT_DESTROY`, immediately remove the HWND from both the tracker cache and UI model without waiting for roundtrips.
  3. **Safe Error Handling:** Wrap all Win32 handle operations in `platform-windows` with error-handling that gracefully ignores `ERROR_INVALID_WINDOW_HANDLE`.
- **Fallback:** If an operation fails on a dead HWND, immediately prune it from the UI list.

---

### RISK-07: Windows Explorer Restart Resulting in Vanished Tray Icons
- **Description:** When `explorer.exe` crashes or restarts, all icons in the Windows notification area are discarded by the operating system.
- **Severity:** MEDIUM  
- **Likelihood:** LOW  
- **Impact:** AsterShell system tray icon disappears until application restart.
- **Mitigation Strategy:**
  1. **`TaskbarCreated` Broadcast Interception:** Register the system broadcast message `RegisterWindowMessageW("TaskbarCreated")`.
  2. **Automatic Re-registration:** The tray management thread in `shell-host` listens for this broadcast and immediately re-issues `Shell_NotifyIconW(NIM_ADD, &nid)` and `NIM_SETVERSION`.
  3. **GUID Identity:** Because AsterShell uses `NIF_GUID`, Windows immediately restores the user's pinned tray position and preferences.
- **Fallback:** Periodic (30-second) sanity check verifying whether the tray icon is registered.

---

### RISK-08: Security: Untrusted Window Titles & IPC Spoofing
- **Description:** Untrusted applications can set malicious or excessively long window titles containing control characters, ANSI escape sequences, or injection strings. Unauthorized processes could attempt to connect to the taskbar IPC pipe.
- **Severity:** MEDIUM  
- **Likelihood:** LOW  
- **Impact:** UI rendering corruption, buffer overflow, or unauthorized IPC command execution.
- **Mitigation Strategy:**
  1. **Title Sanitization:** Truncate window titles to a maximum of 256 characters and strip all ASCII control characters (`< 0x20`) except space.
  2. **Mutual PID Authentication:** Retain the M2.1 IPC security model where `shell-host` verifies client PID against `SupervisedPidRegistry` before granting handshake acknowledgment.
  3. **Strict Pipe SDDL:** Named Pipe DACL restricts access exclusively to the current user SID and Administrators.
- **Fallback:** Reject unauthenticated connections immediately.

---

### RISK-09: Excessive CPU Wakeups from System Telemetry Polling
- **Description:** Polling volume, network, battery, and clock on a high-frequency timer (e.g. 100ms) would prevent the CPU from entering low-power C-states, reducing battery life on portable laptops.
- **Severity:** LOW  
- **Likelihood:** MEDIUM  
- **Impact:** 1–3% continuous CPU usage, battery drain.
- **Mitigation Strategy:**
  1. **100% Event-Driven Indicators:**
     - Audio: `IAudioEndpointVolumeCallback` (invoked only on volume change).
     - Network: `INetworkListManagerEvents` (invoked only on connectivity shift).
     - Battery: `RegisterPowerSettingNotification` (invoked only on power state shift).
  2. **Boundary-Synced Clock:** The clock timer computes the remaining milliseconds until the next 00-second minute boundary and sleeps until that exact moment.
- **Fallback:** Verified target: 0 wakeups per minute for indicators, 1 wakeup per minute for clock.

---

## 3. References
1. [Microsoft Learn: WinEvent Hook Performance Considerations](https://learn.microsoft.com/en-us/windows/win32/winmsg/about-winevents)
2. [Microsoft Learn: Power Management Architecture](https://learn.microsoft.com/en-us/windows/win32/power/power-management-architecture)
3. [Microsoft Learn: Guidelines for Handling High-DPI Displays](https://learn.microsoft.com/en-us/windows/win32/hidpi/high-dpi-desktop-application-development-on-windows)
