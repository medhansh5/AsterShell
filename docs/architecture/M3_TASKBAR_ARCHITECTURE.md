# AsterShell — Milestone 3: Taskbar Architecture Specification (`AsterTaskbar`)

**Milestone:** M3 — Custom Taskbar & System Tray Integration  
**Component:** `AsterTaskbar` (`apps/taskbar` & `crates/window-tracker`)  
**Status:** DRAFT SPECIFICATION (APPROVED FOR M3 PREFLIGHT)  
**Target Platform:** Windows 11 (Build 22000+)  
**Baseline Commit:** `31a1a09fa5ab993c873133b891b28f7c3641dcdc`

---

## 1. Executive Summary & Mission

AsterShell is a native Rust shell companion designed for Windows 11. Its overarching objective is to provide an ultra-performant, modular, and elegant desktop shell companion without prematurely replacing or destabilizing Windows Explorer.

Milestone 3 introduces **AsterTaskbar**: an independent, hardware-accelerated, multi-monitor custom taskbar window paired with an event-driven window tracking engine (`crates/window-tracker`). AsterTaskbar delivers:
1. Pinned and active application launch management.
2. Real-time, event-driven tracking of open application windows.
3. Smooth application switching and activation complying with Windows 11 foreground rules.
4. Integrated system status indicators (Clock/Date, Audio Volume, Network Connectivity, Battery Status).
5. Native multi-monitor positioning and per-monitor dynamic DPI scaling (Per-Monitor V2).
6. Non-destructive coexistence alongside the standard Windows 11 Explorer taskbar in companion mode.

AsterTaskbar strictly adheres to documented Win32, COM, and DWM APIs. It executes zero binary patches, introduces zero code injection into Explorer or foreign processes, avoids undocumented shell exports, and maintains strict process isolation under the AsterShell supervisor (`apps/shell-host`).

---

## 2. Architectural Principles & Isolation Model

AsterTaskbar aligns directly with the foundational principles established in `ARCHITECTURE.md`:

```
+---------------------------------------------------------------------------------------------------------+
|                                        ASTERSHELL SUPERVISOR BUS                                        |
+---------------------------------------------------------------------------------------------------------+
|                                                                                                         |
|                                       +-------------------------+                                       |
|                                       |       shell-host        |                                       |
|                                       |  (Supervisor / Broker)  |                                       |
|                                       +------------+------------+                                       |
|                                                    |                                                    |
|                                         Windows Job Object Tree                                         |
|                                                    |                                                    |
|                   Named Pipe IPC (`\\.\pipe\astershell-ipc-{hash}`) [SDDL Mutual Auth]                  |
|                                                    |                                                    |
|         +------------------------------------------+------------------------------------------+         |
|         |                                                                                     |         |
|         v                                                                                     v         |
| +-----------------------------------------------+                   +---------------------------------+ |
| |                 apps/taskbar                  |                   |         apps/search-ui          | |
| |       (Supervised Custom Taskbar UI)          |                   |     (Spotlight Search Overlay)  | |
| |                                               |                   +---------------------------------+ |
| |  +-----------------------------------------+  |                                                       |
| |  | Main UI Thread (Slint Event Loop)       |  |                                                       |
| |  | - TaskbarWindow Component Instances     |  |                                                       |
| |  | - User Input & Animation Dispatch       |  |                                                       |
| |  +--------------------+--------------------+  |                                                       |
| |                       ^                       |                                                       |
| |             Lock-free Channel                 |                                                       |
| |                       v                       |                                                       |
| |  +-----------------------------------------+  |                                                       |
| |  | WinEvent Hook Worker Thread (STA)       |  |                                                       |
| |  | - SetWinEventHook (WINEVENT_OUTOFCONTEXT)  |                                                       |
| |  | - Window Lifecycle State Machine        |  |                                                       |
| |  | - HWND Filtering Pipeline               |  |                                                       |
| |  +--------------------+--------------------+  |                                                       |
| |                       ^                       |                                                       |
| |             State Synchronization             |                                                       |
| |                       v                       |                                                       |
| |  +-----------------------------------------+  |                                                       |
| |  | System Telemetry Worker Thread (MTA)    |  |                                                       |
| |  | - CoreAudio Volume Event Sinks          |  |                                                       |
| |  | - Network List Manager Event Sinks      |  |                                                       |
| |  | - Win32 Battery Status Poller/Listener  |  |                                                       |
| |  +-----------------------------------------+  |                                                       |
| +-----------------------------------------------+                                                       |
+---------------------------------------------------------------------------------------------------------+
```

### 2.1 Process Separation Rationale
- **Why not host the taskbar inside `shell-host`?**  
  `shell-host` is the critical system watchdog, configuration broker, and crash supervisor. If the UI rendering pipeline encounters a graphics driver timeout (TDR) or an invalid layout edge case, in-process UI hosting would crash the supervisor daemon, collapsing the entire shell suite. Isolating `apps/taskbar` into its own supervised child process guarantees zero blast radius to core daemons.
- **Why not separate `window-tracker` into its own child process?**  
  Window tracking generates hundreds of fine-grained events during active window manipulation (window drags, focus shifts, title changes, minimization). Marshalling every single WinEvent across Named Pipe IPC introduces serialization overhead, inter-process latency, and context switching. Embedding `crates/window-tracker` as an internal worker thread within `apps/taskbar` allows lock-free in-memory ring-buffer dispatch directly to the UI thread, achieving sub-5ms UI update latency with zero IPC saturation.

---

## 3. Thread Architecture & Message Pump Ownership

To guarantee responsive UI rendering while processing real-time OS events, `apps/taskbar` employs a tripartite threading model:

```
+-------------------------------------------------------------------------------------------------------+
|                                       apps/taskbar PROCESS SPACE                                      |
+-------------------------------------------------------------------------------------------------------+
|                                                                                                       |
|  [ Thread 1: Main UI Thread (Slint / Winit) ]                                                         |
|  * Owns the Slint UI event loop (`slint::run_event_loop()`).                                          |
|  * Manages native HWND instances for each monitor taskbar.                                            |
|  * Handles mouse clicks, hover animations, and application activation requests.                      |
|  * Consumes state change messages from internal worker channels (`crossbeam-channel`).                |
|                                                                                                       |
|  [ Thread 2: WinEvent Hook Worker Thread (STA) ]                                                      |
|  * Initialized as Single-Threaded Apartment (`CoInitializeEx(NULL, COINIT_APARTMENTTHREADED)`).       |
|  * Owns a dedicated standard Win32 message pump (`GetMessageW` / `DispatchMessageW`).                |
|  * Registers OS hooks via `SetWinEventHook(..., WINEVENT_OUTOFCONTEXT)`.                              |
|  * Runs HWND filtering pipeline to eliminate toolbars, cloaked windows, and invisible dialogs.        |
|  * Emits reconciled `WindowListUpdate` snapshots to Thread 1.                                         |
|                                                                                                       |
|  [ Thread 3: Platform Telemetry & Indicator Thread (MTA) ]                                            |
|  * Initialized as Multi-Threaded Apartment (`CoInitializeEx(NULL, COINIT_MULTITHREADED)`).            |
|  * Connects COM event sinks: `IAudioEndpointVolumeCallback` and `INetworkListManagerEvents`.          |
|  * Listens to power status changes via `RegisterPowerSettingNotification`.                           |
|  * Emits `TelemetryUpdate` payloads (volume, battery %, network status) to Thread 1.                 |
|                                                                                                       |
|  [ Thread 4: Async Supervisor IPC Worker (Tokio Runtime) ]                                           |
|  * Lightweight tokio async task managing the named pipe client connection to `shell-host`.            |
|  * Performs mutual PID authentication and heartbeat exchanges.                                       |
|  * Receives configuration updates (hot-reloading layout, pinned apps, edge position).                 |
|                                                                                                       |
+-------------------------------------------------------------------------------------------------------+
```

### 3.1 Windows Message Pump Ownership
1. **Slint/Winit Message Pump:** Winit controls the primary message pump on Thread 1. It processes `WM_PAINT`, `WM_MOUSEMOVE`, `WM_LBUTTONDOWN`, and mouse capture.
2. **WinEvent Hook Message Pump:** Windows requires a message pump on any thread that registers `SetWinEventHook` with `WINEVENT_OUTOFCONTEXT`. Thread 2 executes:
   ```rust
   let mut msg = std::mem::zeroed();
   while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
       TranslateMessage(&msg);
       DispatchMessageW(&msg);
   }
   ```
3. **Hidden System Broadcast Window:** To receive system-wide broadcasts (`WM_DISPLAYCHANGE`, `WM_DPICHANGED`, `WM_SETTINGCHANGE`, `WM_POWERBROADCAST`) that Slint's winit abstraction may drop or abstract away, Thread 1 creates a lightweight hidden Win32 message window (`HWND_MESSAGE`) registered to a custom AsterShell window class.

---

## 4. Taskbar Window Model & Win32 Integration

### 4.1 Window Creation and Styles
Each monitor taskbar window is created with explicit Win32 window styles:

| Style Constant | Value | Purpose |
| :--- | :--- | :--- |
| `WS_POPUP` | `0x80000000` | Frameless window with no title bar, system menu, or standard borders. |
| `WS_VISIBLE` | `0x10000000` | Initially visible window. |
| `WS_CLIPSIBLINGS` | `0x04000000` | Clips child siblings to prevent rendering tearing. |
| `WS_CLIPCHILDREN` | `0x02000000` | Excludes child window regions during parent painting. |

Extended Window Styles (`WS_EX_*`):

| Extended Style Constant | Value | Purpose |
| :--- | :--- | :--- |
| `WS_EX_TOPMOST` | `0x00000008` | Ensures the taskbar remains docked above standard desktop application windows. |
| `WS_EX_TOOLWINDOW` | `0x00000080` | **Critical:** Prevents AsterTaskbar itself from appearing as a running application on the taskbar or in the Windows Alt+Tab switcher. |
| `WS_EX_NOACTIVATE` | `0x08000000` | **Critical:** Prevents clicking the taskbar from stealing foreground focus away from the active application window unless explicitly requested. |

### 4.2 Focus & Activation Interaction State Machine
AsterTaskbar operates under a deterministic, 4-state focus interaction state machine designed to reconcile `WS_EX_NOACTIVATE` with keyboard navigation, native context menus, and Windows foreground lockdown:

```
+---------------------------------------------------------------------------------------+
| STATE 1: ORDINARY MOUSE INTERACTION (Default Non-Activating State)                    |
| - HWND style: WS_EX_NOACTIVATE.                                                       |
| - WM_MOUSEACTIVATE returns MA_NOACTIVATE.                                             |
| - Left-click on running task:                                                         |
|   * If inactive: ShowWindow(SW_RESTORE) if minimized, then SetForegroundWindow(hwnd). |
|   * If active: ShowWindow(SW_MINIMIZE) to minimize.                                   |
| - AsterTaskbar never steals keyboard focus from the active application document.      |
+---------------------------------------------------------------------------------------+
        |                                                                ^
        | Win+Alt+T Hotkey                                               | Escape / Dismiss
        v                                                                |
+---------------------------------------------------------------------------------------+
| STATE 2: KEYBOARD NAVIGATION (Active Focus Ring)                                      |
| - Captures previous foreground window: previous_fg = GetForegroundWindow().            |
| - AsterTaskbar explicitly elevates to foreground: SetForegroundWindow(taskbar_hwnd).  |
| - Arrow keys (Left/Right) traverse items; visual focus ring active (#3b82f6).         |
| - Enter/Space activates selected item.                                                |
| - Escape transitions to State 4 (Dismissal & Focus Restoration).                      |
+---------------------------------------------------------------------------------------+
        |                                                                ^
        | Right-Click or Menu Key                                        | Menu Closes
        v                                                                |
+---------------------------------------------------------------------------------------+
| STATE 3: NATIVE CONTEXT MENU INVOCATION (TrackPopupMenuEx)                            |
| - Captures previous foreground window: previous_fg = GetForegroundWindow().            |
| - Temporarily elevates taskbar: SetForegroundWindow(taskbar_hwnd).                    |
| - Invokes TrackPopupMenuEx(hmenu, TPM_RIGHTBUTTON, x, y, taskbar_hwnd, None).         |
| - Posts benign message: PostMessageW(taskbar_hwnd, WM_NULL, 0, 0) for menu release.   |
| - Transitions to State 4 upon menu exit or cancellation.                              |
+---------------------------------------------------------------------------------------+
        |
        v
+---------------------------------------------------------------------------------------+
| STATE 4: DISMISSAL & FOCUS RESTORATION                                                |
| - Evaluates previous_fg window handle validity via IsWindow(previous_fg).              |
| - Restores foreground focus: SetForegroundWindow(previous_fg).                        |
| - Resets focus ring and returns to State 1.                                           |
+---------------------------------------------------------------------------------------+
```

#### Windows UIPI Platform Security Limitation
Under Windows User Interface Privilege Isolation (UIPI), a standard-user (medium-integrity) process cannot bring a high-integrity window (e.g. Administrator Command Prompt or Task Manager) to the foreground via `SetForegroundWindow`. In such cases, Windows automatically flashes the target window button on the taskbar. This is an intentional operating system security boundary and is handled gracefully as documented platform behavior.

> [!IMPORTANT]
> **Strict Ban on `AttachThreadInput`:**  
> Casually using `AttachThreadInput` to force foreground activation is strictly prohibited. `AttachThreadInput` binds the message queues of AsterShell and the target application together. If the target application enters an unresponsive loop, deadlocks, or hangs, AsterTaskbar will immediately freeze. All window activations must rely exclusively on documented Windows activation semantics (`ShowWindow`, `SetForegroundWindow`, and `AllowSetForegroundWindow`).

### 4.3 Taskbar Operating Modes: M3A Floating Companion vs. Future Edge-Anchored

To eliminate any architectural ambiguity regarding window positioning and work-area management, AsterShell formally defines two explicitly separate operating modes:

```
MODE 1: M3A FLOATING COMPANION MODE (Active & Exclusively Implemented in M3)
+-----------------------------------------------------------------------------------+
|  [ AsterTaskbar: Floating Companion Island (WS_EX_TOPMOST / WS_EX_NOACTIVATE) ]   |
+-----------------------------------------------------------------------------------+
|                                                                                   |
|                            ACTIVE APPLICATION WINDOWS                             |
|                    (Desktop Work Area SPI_SETWORKAREA Unmodified)                 |
|                                                                                   |
+-----------------------------------------------------------------------------------+
|  [ Native Windows 11 Taskbar & System Tray (Preserved Intact at Screen Bottom) ] |
+-----------------------------------------------------------------------------------+

MODE 2: FUTURE EDGE-ANCHORED MODE (Deferred Post-M3 / Milestone 6 Optional Feature)
+-----------------------------------------------------------------------------------+
|  [ AsterTaskbar: Registered Win32 AppBar (SHAppBarMessage ABM_NEW / ABM_SETPOS) ] |
+-----------------------------------------------------------------------------------+
|  ////////////////////// DESKTOP WORK AREA (SPI_SETWORKAREA) ////////////////////// |
|                            ACTIVE APPLICATION WINDOWS                             |
|                         (Maximized windows dock cleanly)                          |
|  //////////////////////////////////////////////////////////////////////////////// |
+-----------------------------------------------------------------------------------+
|  [ Future Full Shell Replacement Mode (Explorer Replaced or Disabled) ]           |
+-----------------------------------------------------------------------------------+
```

#### Mode 1: M3A Floating Companion Mode (Active & Mandatory for M3)
- **Implementation Target:** Milestone 3 (M3A through M3F).
- **Window Model:** Floating, non-activating companion window (`WS_POPUP`, `WS_EX_TOOLWINDOW`, `WS_EX_NOACTIVATE`, `WS_EX_TOPMOST`).
- **Zero AppBar Registration:** Strictly does **NOT** register as a Win32 AppBar (`SHAppBarMessage`).
- **Zero Work-Area Modification:** Strictly does **NOT** call `ABM_NEW`, `ABM_QUERYPOS`, or `ABM_SETPOS`. The native Windows desktop work area (`SPI_SETWORKAREA`) is left 100% untouched.
- **Explorer Coexistence:** The Windows 11 Explorer taskbar and system tray remain active, interactive, and completely unsuppressed at the bottom of the screen.
- **Z-Order & Fullscreen:** Topmost by default; automatically demotes to `HWND_BOTTOM` when a fullscreen exclusive or borderless application is active on the monitor.

#### Mode 2: Future Edge-Anchored Mode (Deferred Post-M3 / Milestone 6 Optional Feature)
- **Implementation Target:** Milestone 6 (Full Shell Replacement) or optional future power-user setting.
- **Window Model:** Registered Win32 AppBar reserving dedicated edge real estate (`ABE_TOP` or `ABE_BOTTOM`).
- **Work-Area Contraction:** Issues `SHAppBarMessage(ABM_NEW)` and `ABM_SETPOS` to contract `SPI_SETWORKAREA`, ensuring maximized windows do not slip beneath the bar.
- **Lifecycle & Cleanup:** Requires strict unregistration via `ABM_REMOVE` on exit to restore the user's desktop work area.
- **Explicit Scope Boundary:** **STRICTLY PROHIBITED IN M3A.** No AppBar calls will be executed in M3.

### 4.4 Fullscreen Application Behavior
When a user launches a fullscreen game, media player, or presentation:
1. `crates/window-tracker` monitors the foreground window bounding rectangle via `GetWindowRect(GetForegroundWindow())`.
2. If `rect == monitor_rect` and the window does not have taskbar borders:
   - AsterTaskbar recognizes that a fullscreen exclusive or borderless application is active on that monitor.
   - It temporarily drops its z-order below the fullscreen window (`SetWindowPos(hwnd, HWND_BOTTOM, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE)`), ensuring zero visual occlusion during gaming or playback.
3. When the fullscreen window loses focus or exits fullscreen, AsterTaskbar immediately restores `HWND_TOPMOST`.

---

## 5. Multi-Monitor Architecture & Per-Monitor V2 DPI

AsterTaskbar provides native, first-class multi-monitor support from day one.

```
                    +----------------------------------------------+
                    |           MONITOR TOPOLOGY MANAGER           |
                    +----------------------------------------------+
                                          |
                EnumDisplayMonitors / WM_DISPLAYCHANGE Broadcast
                                          |
              +---------------------------+---------------------------+
              |                                                       |
              v                                                       v
+---------------------------+                           +---------------------------+
|    PRIMARY MONITOR (0)    |                           |   SECONDARY MONITOR (1)   |
|   \\.\DISPLAY1 (100% DPI) |                           |   \\.\DISPLAY2 (150% DPI) |
+---------------------------+                           +---------------------------+
| - Full Pinned Apps Grid   |                           | - Active Running Apps     |
| - Running Window Tasks    |                           | - Secondary Clock         |
| - System Tray Indicators  |                           | - Clean Multi-Monitor UI  |
| - Launcher (Start) Button |                           +---------------------------+
+---------------------------+
```

### 5.1 Monitor Discovery & Instance Lifecycle
1. At startup, AsterTaskbar calls `EnumDisplayMonitors` via `platform-windows`.
2. For each active display (`HMONITOR`), it retrieves `MONITORINFOEXW`:
   - `rcMonitor`: Physical screen coordinates across the virtual desktop coordinate space.
   - `rcWork`: Available work area excluding Explorer appbars.
   - `dwFlags & MONITORINFOF_PRIMARY`: Identifies whether this display is the OS primary monitor.
3. **Signed Coordinate Integrity (P2-02):** All monitor bounds, virtual desktop offsets, and window placement calculations must strictly utilize signed 32-bit integers (`i32`). Secondary monitors positioned to the left of or above the primary display possess negative coordinates (e.g. `left = -1920, top = 0`). Any assumption of unsigned coordinates or `0, 0` origin will result in off-screen window placement or integer underflow.
4. AsterTaskbar instantiates a dedicated Slint `TaskbarWindow` component for each detected display:
   - **Primary Taskbar:** Displays Launcher button, pinned applications, active running windows, and the full system status area (Clock, Audio, Network, Battery).
   - **Secondary Taskbar(s):** Configurable to display either all running applications or only applications located on that specific monitor, plus a secondary clock.
5. When a monitor is disconnected or hot-plugged:
   - The hidden broadcast window intercepts `WM_DISPLAYCHANGE`.
   - AsterTaskbar re-enumerates displays, terminates instances belonging to removed handles, and instantiates instances for new displays within 15 ms.

### 5.2 Dynamic Per-Monitor V2 DPI Scaling
- AsterShell processes are compiled with `DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2`.
- When a taskbar window is moved across monitors with differing DPI or when the user changes scale settings:
  1. Windows dispatches `WM_DPICHANGED` with `HIWORD(wParam) == new_dpi_y` and `lParam == suggested_rect`.
  2. AsterTaskbar invokes `platform_windows::scale_to_physical_pixels(logical_height, new_dpi)`.
  3. Slint re-renders vector icons, fonts, and paddings dynamically without bitmap blurring.
  4. AsterTaskbar updates its physical height and repositioning with `SetWindowPos`.

---

## 6. Real-Time Application Window Tracking (`crates/window-tracker`)

### 6.1 Event-Driven WinEvent Architecture
High-frequency polling via continuous `EnumWindows` loops wastes significant CPU resources (2–5% continuous CPU load) and degrades battery life. `crates/window-tracker` operates on an exclusively event-driven model:

```
[ Windows OS Desktop Window Manager ]
                 |
        WinEvent Hook Events
                 |
                 v
+-----------------------------------------------------------------------------------+
|                          crates/window-tracker WORKER                             |
+-----------------------------------------------------------------------------------+
|  Hook Registrations:                                                              |
|  - EVENT_SYSTEM_FOREGROUND    (0x0003) -> Focus switched to target HWND           |
|  - EVENT_OBJECT_CREATE        (0x8000) -> Top-level window instantiated           |
|  - EVENT_OBJECT_DESTROY       (0x8001) -> Top-level window destroyed              |
|  - EVENT_OBJECT_SHOW          (0x8002) -> Window shown from hidden state          |
|  - EVENT_OBJECT_HIDE          (0x8003) -> Window hidden from desktop              |
|  - EVENT_SYSTEM_MINIMIZESTART (0x0016) -> Window entered minimized state         |
|  - EVENT_SYSTEM_MINIMIZEEND   (0x0017) -> Window restored from minimized         |
|  - EVENT_OBJECT_NAMECHANGE    (0x800C) -> Window title text modified              |
|  - EVENT_OBJECT_CLOAKED       (0x8017) -> Window moved to inactive v-desktop      |
|  - EVENT_OBJECT_UNCLOAKED     (0x8018) -> Window moved to active v-desktop        |
+-----------------------------------------------------------------------------------+
                                         |
                                         v
                         [ HWND Comprehensive Filter ]
                                         |
                                         v
                       [ In-Memory WindowCache State ]
                                         |
                                         v
               [ Crossbeam Channel Update -> apps/taskbar UI ]
```

### 6.1.1 Startup Synchronization Sequence (P1-02: Race-Free Initialization)
To eliminate the race condition between initial window enumeration and event hook registration:
1. **Phase 1 (Hook Arming):** Call `SetWinEventHook` first. The OS immediately begins queuing events into the worker thread's Win32 message queue.
2. **Phase 2 (Baseline Snapshot):** Execute `EnumWindows` to snapshot all currently active windows into an initial in-memory map (`HashMap<HWND, WindowInfo>`).
3. **Phase 3 (Message Pump & Idempotent Upsert):** Enter the `GetMessageW` loop. Hook events arriving for windows already captured during enumeration are reconciled idempotently without duplicate records.
4. **Phase 4 (Initial Snapshot Dispatch):** Emit the initial consolidated window state to the main UI thread via the lock-free crossbeam channel.

### 6.2 HWND Comprehensive Filtering Pipeline
Not all top-level HWNDs represent running applications that should appear on a user's taskbar. `crates/window-tracker` executes a rigorous 7-stage filter on every candidate window:

```
Candidate HWND
      |
[ 1. IsWindow(hwnd) == TRUE ] -----------------------------------------> FALSE -> REJECT
      | TRUE
[ 2. IsWindowVisible(hwnd) == TRUE ] ----------------------------------> FALSE -> REJECT
      | TRUE
[ 3. Style Check: (GetWindowLong(GWL_STYLE) & WS_CHILD) == 0 ] --------> FALSE -> REJECT
      | TRUE
[ 4. Owner Check: GetWindow(hwnd, GW_OWNER) == NULL ] -----------------> FALSE -> (Check 4B)
      | TRUE                                                                          |
      |                                    [ 4B. (GWL_EXSTYLE & WS_EX_APPWINDOW) != 0 ]
      |                                              | TRUE                 | FALSE
      +----------------------+-----------------------+                      v
                             |                                            REJECT
[ 5. ToolWindow Check: (GWL_EXSTYLE & WS_EX_TOOLWINDOW) == 0 ] ---------> FALSE -> REJECT
      | TRUE
[ 6. DWM Cloaked Check: DwmGetWindowAttribute(DWMWA_CLOAKED) == 0 ] ----> FALSE -> REJECT (Other Virtual Desktop)
      | TRUE
[ 7. Dimensions Check (with IsIconic Exception) & Blacklist ] ---------> FAIL  -> REJECT (0x0 or Progman/Shell_TrayWnd)
      | PASS
      v
ACCEPT AS VALID TASKBAR APPLICATION WINDOW
```

#### Detailed Filter Criteria:
1. **Window Liveness:** `IsWindow(hwnd)` must be true.
2. **Visibility:** `IsWindowVisible(hwnd)` must be true.
3. **No Child Windows:** The window must not have `WS_CHILD`.
4. **Window Ownership:** The window must not be owned by another window (`GetWindow(hwnd, GW_OWNER) == NULL`), unless it explicitly declares `WS_EX_APPWINDOW` to force taskbar representation.
5. **No Tool Windows:** Windows with `WS_EX_TOOLWINDOW` are excluded unless overridden by `WS_EX_APPWINDOW`.
6. **Virtual Desktop & Cloaked State:** Modern Windows 11 virtual desktops and UWP background hosts cloak inactive windows rather than destroying them. Calling `DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &cloaked, sizeof(cloaked))` detects whether a window is on another virtual desktop (`DWM_CLOAKED_SHELL`) or suspended (`DWM_CLOAKED_APP`). Cloaked windows are filtered out of the current taskbar view.
7. **Dimensions & Minimized Window Handling (P1-03):** Windows with zero bounding area (`rect.right <= rect.left || rect.bottom <= rect.top`) are rejected, **EXCEPT** when `IsIconic(hwnd)` is true. In Win32, minimized windows report off-screen stub coordinates (e.g. `-32000, -32000`). Bounding box filtering must explicitly bypass rejection for iconic windows.
8. **System Class Exclusion:** Filter out known shell infrastructure windows:
   - `Progman` / `WorkerW` (Desktop wallpaper surfaces).
   - `Shell_TrayWnd` / `Shell_SecondaryTrayWnd` (Windows Explorer taskbars).
   - `Windows.UI.Core.CoreWindow` (Transient XAML helper windows).

### 6.3 Application Identity, Grouping & Icons
- **Win32 Applications:** Grouped by process executable path (`QueryFullProcessImageNameW`).
- **Packaged MSIX/UWP Applications:** Identified via Application User Model ID (`AppUserModelID`) queried via `SHGetPropertyStoreForWindow(hwnd, PKEY_AppUserModel_ID)`.
- **Window Icon Resolution:**
  1. Send `WM_GETICON(ICON_BIG)` / `WM_GETICON(ICON_SMALL2)`.
  2. Fall back to `GetClassLongPtrW(hwnd, GCLP_HICON)`.
  3. Fall back to extracting the icon from the executable binary via `ExtractIconExW`.
  4. Fall back to a clean default application glyph.

---

## 7. System Indicators & Telemetry Subsystem

AsterTaskbar incorporates a high-fidelity system status module in the primary taskbar:

| Indicator | Underlying Windows API | Event Mechanism | Polling Overhead |
| :--- | :--- | :--- | :--- |
| **Clock & Date** | `GetLocalTime(&st)` | Timer aligned to minute boundary: `delay = (60 - st.wSecond) * 1000 - st.wMilliseconds` | **0%** (1 wake-up per minute) |
| **Audio Volume** | CoreAudio `IAudioEndpointVolume` | Event sink: `IAudioEndpointVolumeCallback::OnNotify` | **0%** (100% event-driven) |
| **Network Status** | Network List Manager `INetworkListManager` | Event sink: `INetworkListManagerEvents::ConnectivityChanged` | **0%** (100% event-driven) |
| **Battery / Power** | `GetSystemPowerStatus` & `RegisterPowerSettingNotification` | OS notifications: `GUID_ACDC_POWER_SOURCE`, `GUID_BATTERY_PERCENTAGE_REMAINING` | **0%** (100% event-driven) |

---

## 8. IPC Contracts with `shell-host`

AsterTaskbar connects to `shell-host` via the versioned Named Pipe IPC bus (`crates/ipc`).

### 8.1 Mutual PID Authentication
Upon connecting to `\\.\pipe\astershell-ipc-{hash}`:
1. `apps/taskbar` sends `IpcRequest::Handshake { client_version: 1, client_name: "taskbar".into() }`.
2. `shell-host` queries client PID via `GetNamedPipeClientProcessId`.
3. `shell-host` confirms the PID is present in its `SupervisedPidRegistry`.
4. Authentication succeeds with `IpcResponse::HandshakeAck`.

### 8.2 Taskbar IPC Message Extensions
To support taskbar configuration and launcher coordination, `crates/ipc` will be extended with:

```rust
pub enum TaskbarRequest {
    GetTaskbarConfig,
    ToggleLauncher,
    SetDockingEdge(DockEdge),
    ReloadPinnedApps,
}

pub enum TaskbarResponse {
    TaskbarConfig(TaskbarConfig),
    Ack,
    Error(String),
}

pub enum TaskbarEvent {
    LauncherStateChanged(bool),
    ConfigHotReloaded,
}
```

---

## 9. Architectural Decision Records (ADRs)

### ADR-0002: Process Model for AsterTaskbar & Window Tracker
- **Context:** Deciding whether taskbar UI and window tracking should run in `shell-host`, in separate child processes, or combined in a dedicated UI process.
- **Options Considered:**
  1. Monolithic: Everything inside `shell-host`.
  2. Fully Micro-Process: `taskbar` UI process + separate `window-tracker` daemon process communicating via IPC.
  3. Dedicated Supervised Process: `apps/taskbar` child process hosting `crates/window-tracker` as an internal worker thread.
- **Chosen Approach:** Option 3 (Dedicated Supervised Process).
- **Reasoning:** Option 1 violates supervisor fault isolation. Option 2 introduces massive IPC traffic overhead (hundreds of messages per second during window resize/drag). Option 3 provides perfect supervisor fault isolation while delivering zero-overhead in-memory window dispatch to the UI.
- **Alternatives Rejected:** Monolithic in `shell-host` (too risky); separate tracker daemon (unnecessary IPC latency).
- **Consequences:** `apps/taskbar` must manage internal thread safety and multi-apartment COM initialization cleanly.

### ADR-0003: Companion Mode Coexistence Strategy
- **Context:** Defining how AsterTaskbar coexists with Windows 11 Explorer taskbar without instability.
- **Options Considered:**
  1. Hide or patch Explorer's `Shell_TrayWnd`.
  2. Dock as a registered AppBar at the top edge (`ABE_TOP`).
  3. Run as a floating companion overlay dock (`WS_EX_TOPMOST` with no work-area contraction).
- **Chosen Approach:** Default to Option 3 (Floating Companion Overlay), with user-configurable support for Option 2 (Docked AppBar at `ABE_TOP`).
- **Reasoning:** Option 1 violates AsterShell core principles (zero binary patching or Explorer suppression). Option 3 provides 100% compatibility with any existing Windows taskbar setup. Option 2 provides clean window snapping for users desiring an edge dock.
- **Alternatives Rejected:** Hiding or subclassing Explorer (strictly rejected).
- **Consequences:** Maximized windows in default Mode A will expand to Explorer's work area, allowing AsterTaskbar to float cleanly above or stay docked without layout fighting.

---

## 10. References & Standards
1. [Microsoft Learn: About Window Procedures & Message Loops](https://learn.microsoft.com/en-us/windows/win32/winmsg/about-window-procedures)
2. [Microsoft Learn: SetWinEventHook Function](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwineventhook)
3. [Microsoft Learn: SHAppBarMessage Function](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shappbarmessage)
4. [Microsoft Learn: Extended Window Styles (WS_EX_*)](https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles)
5. [Microsoft Learn: Desktop Window Manager Cloaking (DWMWA_CLOAKED)](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute)
6. [Microsoft Learn: Core Audio APIs (MMDevice & EndpointVolume)](https://learn.microsoft.com/en-us/windows/win32/coreaudio/core-audio-apis-in-windows-vista)
