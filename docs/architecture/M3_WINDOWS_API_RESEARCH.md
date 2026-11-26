# AsterShell — Milestone 3: Windows Platform API Research & Feasibility Specification

**Milestone:** M3 — Custom Taskbar & System Tray Integration  
**Component:** `AsterTaskbar`, `AsterTray` & `crates/platform-windows`  
**Status:** DRAFT SPECIFICATION (APPROVED FOR M3 PREFLIGHT)  
**Target Platform:** Windows 11 (Build 22000+ / Architecture: x86_64, aarch64)  
**Baseline Commit:** `31a1a09fa5ab993c873133b891b28f7c3641dcdc`

---

## 1. Executive Summary

This research specification documents the empirical investigation, technical constraints, Win32/COM/WinRT API signatures, threading invariants, and memory safety rules governing AsterShell's custom taskbar and system tray architecture.

Every architectural assertion in M3 is grounded in documented Microsoft Windows platform specifications. This document proves the feasibility of:
1. Creating and managing multi-monitor companion taskbar windows without stealing user focus.
2. Event-driven tracking of top-level application windows using out-of-process WinEvent hooks.
3. Activating and switching running applications while complying with Windows 11 foreground lockdown restrictions.
4. Managing AsterShell-owned system notification icons with GUID-based persistence and Explorer restart resilience.
5. Capturing real-time system indicators (Clock, Audio, Network, Battery) with zero polling overhead.

---

## 2. Windowing, Docking & Taskbar Behavior

### 2.1 Window Creation & Native Styles
AsterTaskbar windows are created either via Slint's underlying winit backend or directly via `CreateWindowExW`.

#### Win32 Window Styles (`GWL_STYLE`):
```c
DWORD dwStyle = WS_POPUP | WS_VISIBLE | WS_CLIPSIBLINGS | WS_CLIPCHILDREN;
```
- `WS_POPUP`: Strips standard title bars, sizing frames, minimize/maximize buttons, and system menu borders.
- `WS_VISIBLE`: Ensures immediate visibility upon creation.
- `WS_CLIPSIBLINGS` & `WS_CLIPCHILDREN`: Prevents visual tearing and invalidates only damaged rectangular regions during repaint.

#### Extended Window Styles (`GWL_EXSTYLE`):
```c
DWORD dwExStyle = WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
```
- `WS_EX_TOPMOST` (`0x00000008`): Places the window above all non-topmost windows in the DWM Z-order stack.
- `WS_EX_TOOLWINDOW` (`0x00000080`): Crucial shell requirement. Prevents the taskbar window itself from appearing in:
  1. The Windows taskbar (Explorer's taskbar).
  2. The AsterShell taskbar (preventing self-recursion).
  3. The Windows Alt+Tab / Task View switcher.
- `WS_EX_NOACTIVATE` (`0x08000000`): Crucial focus-protection style. Prevents the window from becoming the foreground active window when clicked with the mouse.

#### Explicit Operating Mode Boundary: M3A Floating Companion vs. Future Edge-Anchored
In M3A Floating Companion Mode, `SHAppBarMessage` (`ABM_NEW`, `ABM_QUERYPOS`, `ABM_SETPOS`) is strictly **not** invoked. The taskbar operates purely as a `WS_EX_TOPMOST` floating companion window, leaving the native Windows desktop work area (`SPI_SETWORKAREA`) 100% untouched. AppBar edge reservation is deferred to a future optional edge-anchored shell mode (Milestone 6).

### 2.2 Z-Order Management & Fullscreen Detection
- **Staying Topmost:** Managed via `SetWindowPos`:
  ```rust
  SetWindowPos(
      hwnd,
      HWND_TOPMOST,
      x, y, cx, cy,
      SWP_NOACTIVATE | SWP_SHOWWINDOW,
  );
  ```
- **Handling Fullscreen Applications:**  
  When an exclusive or borderless fullscreen application (such as a 3D game or media presentation) becomes active:
  ```rust
  // Check if foreground window occupies the entire monitor
  let mut fg_rect = RECT::default();
  GetWindowRect(GetForegroundWindow(), &mut fg_rect);
  if fg_rect.left <= mon_rect.left && fg_rect.top <= mon_rect.top
      && fg_rect.right >= mon_rect.right && fg_rect.bottom >= mon_rect.bottom
  {
      // Demote taskbar Z-order so it does not occlude the fullscreen application
      SetWindowPos(hwnd, HWND_BOTTOM, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
  }
  ```

### 2.3 Focus Protection & `WM_MOUSEACTIVATE`
If `WS_EX_NOACTIVATE` is intercepted or mishandled by the UI framework, the native window procedure handles `WM_MOUSEACTIVATE`:
```rust
// In Taskbar Window Procedure:
match msg {
    WM_MOUSEACTIVATE => {
        // Return MA_NOACTIVATE to tell Windows not to activate this window upon mouse click
        return LRESULT(MA_NOACTIVATE as _);
    }
    _ => DefWindowProcW(hwnd, msg, wparam, lparam),
}
```

#### Dual-State Focus Architecture (P1-01)
`WS_EX_NOACTIVATE` effectively prevents mouse clicks on taskbar buttons from stealing focus from active user documents. However, Win32 interactive menus and accessibility require a structured dual-state transition:
1. **Passive State (Standard Mouse Clicks):** `WS_EX_NOACTIVATE` active; clicking running task buttons directly calls `ShowWindow(SW_RESTORE)` and `SetForegroundWindow(target_hwnd)` without the taskbar ever taking keyboard focus.
2. **Elevated Active State (Context Menus & Keyboard Navigation):**
   - When the user presses `Win+Alt+T` (keyboard navigation) or right-clicks for a context menu (`TrackPopupMenuEx`), AsterTaskbar records the current foreground HWND (`previous_fg = GetForegroundWindow()`) and explicitly calls `SetForegroundWindow(taskbar_hwnd)`.
   - This elevation allows Win32 popup menus to receive keyboard events (arrow keys, Esc) and process outside-click dismissal.
   - Upon menu closure or `Escape` dismissal, AsterTaskbar immediately restores focus via `SetForegroundWindow(previous_fg)`.

### 2.4 Multi-Monitor Topology & Signed Coordinate Integrity (P2-02)
Windows uses a unified virtual desktop coordinate space. Monitor 0 (Primary) has `(0, 0)` at its top-left, while secondary monitors positioned to the left or above the primary display possess negative coordinates (e.g., `X = -1920, Y = 0`).
- **Signed Coordinate Mandatory Requirement:** All monitor coordinates, bounding boxes, and offsets across the virtual desktop coordinate space must strictly utilize signed 32-bit integers (`i32`). Treating coordinates as unsigned or assuming `(0, 0)` minimum bounds will cause off-screen window placement or integer underflow crashes.
- **Enumeration API:** `EnumDisplayMonitors(HDC(0), None, Some(enum_monitor_callback), LPARAM(ptr))`
- **Monitor Metadata:** `GetMonitorInfoW(hmonitor, &mut mi)` populates `MONITORINFOEXW`:
  - `rcMonitor`: Total monitor rectangle (signed `i32` coordinates: e.g., `-1920, 0, 0, 1080`).
  - `rcWork`: Available desktop work area excluding Explorer appbars.
  - `szDevice`: Device name string (e.g., `\\.\DISPLAY1`).
  - `dwFlags & MONITORINFOF_PRIMARY`: Boolean indicating if primary.

### 2.5 DWM System Backdrops & Corner Preferences
On Windows 11 (Build 22000+), taskbar visual styling leverages `DwmSetWindowAttribute`:
- `DWMWA_USE_IMMERSIVE_DARK_MODE` (`20`): Enables dark mode window rendering.
- `DWMWA_WINDOW_CORNER_PREFERENCE` (`33`):
  - `DWMWCP_DONOTROUND` (`1`): For edge-docked taskbars (sharp corners against screen boundaries).
  - `DWMWCP_ROUND` (`2`): For floating companion docks (rounded corners).
- `DWMWA_SYSTEMBACKDROP_TYPE` (`38`):
  - `DWMSBT_AUTO` (`0`), `DWMSBT_NONE` (`1`), `DWMSBT_MAINWINDOW` / Mica (`2`), `DWMSBT_TRANSIENTWINDOW` / Acrylic (`3`), `DWMSBT_TABBEDWINDOW` / Mica Alt (`4`).

---

## 3. Application Window Tracking & Lifecycle (`crates/window-tracker`)

### 3.1 WinEvent Hooks via `SetWinEventHook`
AsterShell uses asynchronous, out-of-context WinEvent hooks:

```rust
let hook = SetWinEventHook(
    EVENT_OBJECT_CREATE,
    EVENT_OBJECT_UNCLOAKED,
    HMODULE::default(),
    Some(win_event_proc),
    0, // All processes
    0, // All threads
    WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
);
```

#### Threading Invariant:
`SetWinEventHook` with `WINEVENT_OUTOFCONTEXT` requires a standard Win32 message pump (`GetMessageW` / `DispatchMessageW`) on the thread that called `SetWinEventHook`. WinEvent messages are delivered as synthesized `WM_NULL` or internal thread messages and dispatched to `win_event_proc`.

#### Startup Synchronization Sequence (P1-02: Race-Free Initialization)
To eliminate the race condition between initial window enumeration and event hook registration:
1. **Phase 1 (Hook Arming):** Call `SetWinEventHook` first. The OS immediately begins queuing events into the worker thread's Win32 message queue.
2. **Phase 2 (Baseline Snapshot):** Execute `EnumWindows` to snapshot all currently active windows into an initial in-memory map (`HashMap<HWND, WindowInfo>`).
3. **Phase 3 (Message Pump & Idempotent Upsert):** Enter the `GetMessageW` loop. Hook events arriving for windows already captured during enumeration are reconciled idempotently without duplicate records.
4. **Phase 4 (Initial Snapshot Dispatch):** Emit the initial consolidated window state to the main UI thread via the lock-free crossbeam channel.

#### Monitored Event Constants:

| Constant Name | Value | Purpose in AsterShell |
| :--- | :--- | :--- |
| `EVENT_SYSTEM_FOREGROUND` | `0x0003` | Fires whenever the active foreground window changes. Updates active task indicator. |
| `EVENT_OBJECT_CREATE` | `0x8000` | Fires when an HWND is created. Queues window for filtering. |
| `EVENT_OBJECT_DESTROY` | `0x8001` | Fires when an HWND is destroyed. Removes window from taskbar. |
| `EVENT_OBJECT_SHOW` | `0x8002` | Fires when a window becomes visible. |
| `EVENT_OBJECT_HIDE` | `0x8003` | Fires when a window is hidden. |
| `EVENT_SYSTEM_MINIMIZESTART` | `0x0016` | Fires when a window minimizes. Updates minimized task indicator. |
| `EVENT_SYSTEM_MINIMIZEEND` | `0x0017` | Fires when a window restores from minimized. |
| `EVENT_OBJECT_NAMECHANGE` | `0x800C` | Fires when window title changes. Updates taskbar button label. |
| `EVENT_OBJECT_CLOAKED` | `0x8017` | Fires when DWM cloaks a window (virtual desktop change). Hides task button. |
| `EVENT_OBJECT_UNCLOAKED` | `0x8018` | Fires when DWM uncloaks a window (switched to current virtual desktop). |

### 3.2 Documented Shell HWND Filter Specification
To prevent phantom buttons (tooltips, invisible helper windows, IME windows, cloaked store apps), `crates/window-tracker` implements the exact Microsoft Shell window filtering rules:

```rust
pub fn is_taskbar_window(hwnd: HWND) -> bool {
    unsafe {
        // 1. Must be a valid window handle
        if !IsWindow(hwnd).as_bool() {
            return false;
        }

        // 2. Must be visible
        if !IsWindowVisible(hwnd).as_bool() {
            return false;
        }

        // 3. Must not be a child window (must be top-level)
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        if (style & WS_CHILD) != 0 {
            return false;
        }

        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;

        // 4. Must not be a tool window, unless explicitly forced as app window
        if (ex_style & WS_EX_TOOLWINDOW) != 0 && (ex_style & WS_EX_APPWINDOW) == 0 {
            return false;
        }

        // 5. Must not be owned by another window, unless explicitly forced
        let owner = GetWindow(hwnd, GW_OWNER);
        if !owner.is_invalid() && (ex_style & WS_EX_APPWINDOW) == 0 {
            return false;
        }

        // 6. Must not be cloaked by DWM (e.g. on another virtual desktop)
        let mut cloaked: u32 = 0;
        let res = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut _ as _,
            std::mem::size_of::<u32>() as u32,
        );
        if res.is_ok() && cloaked != 0 {
            return false;
        }

        // 7. Check bounding dimensions (P1-03: bypass coordinate check if window is iconic/minimized)
        if !IsIconic(hwnd).as_bool() {
            let mut rect = RECT::default();
            if GetWindowRect(hwnd, &mut rect).is_ok() {
                if rect.right <= rect.left || rect.bottom <= rect.top {
                    return false;
                }
            }
        }

        // 8. Blacklist internal shell classes
        let mut class_buf = [0u16; 64];
        let len = GetClassNameW(hwnd, &mut class_buf);
        if len > 0 {
            let class_name = String::from_utf16_lossy(&class_buf[..len as usize]);
            if class_name == "Progman" 
                || class_name == "WorkerW" 
                || class_name == "Shell_TrayWnd" 
                || class_name == "Shell_SecondaryTrayWnd"
            {
                return false;
            }
        }

        true
    }
}
```

---

## 4. Application Window Activation & Foreground Rules

### 4.1 Windows Foreground Restrictions (`LockSetForegroundWindow`) & UIPI Limitation (P2-03)
Since Windows 98/2000, calling `SetForegroundWindow(hwnd)` is restricted to prevent background processes from stealing focus while a user is typing.

Under documented Windows rules, a process can set the foreground window ONLY if:
1. The calling process is the current foreground process.
2. The calling process was started by the foreground process.
3. The calling process received the last input event (keyboard or mouse).
4. The foreground process has permitted the caller via `AllowSetForegroundWindow`.

#### User Interface Privilege Isolation (UIPI) Platform Boundary:
Under Windows User Interface Privilege Isolation (UIPI), a standard-user (medium-integrity) process cannot bring a high-integrity window (e.g. Administrator Command Prompt or Task Manager) to the foreground via `SetForegroundWindow`. In such cases, Windows ignores the activation request and flashes the window button on the taskbar. This is an intentional operating system security boundary and is handled gracefully as documented platform behavior.

### 4.2 How AsterTaskbar Complies
When a user clicks an application button on AsterTaskbar:
1. The mouse click event (`WM_LBUTTONUP`) is directed to AsterTaskbar.
2. Because AsterTaskbar **received the physical mouse click**, the Windows subsystem recognizes AsterTaskbar as the process that received the last input event.
3. AsterTaskbar is therefore granted foreground switching privileges by Windows.
4. Calling `SetForegroundWindow(target_hwnd)` succeeds immediately.

### 4.3 Handling Minimized Windows
If `IsIconic(target_hwnd)` is true:
- Calling `SetForegroundWindow` alone will merely flash the taskbar button in Explorer.
- AsterTaskbar must first call:
  ```rust
  ShowWindow(target_hwnd, SW_RESTORE);
  SetForegroundWindow(target_hwnd);
  ```

### 4.4 The Danger of `AttachThreadInput`
Some legacy tools use `AttachThreadInput(GetWindowThreadProcessId(hwnd_fg), GetCurrentThreadId(), TRUE)` to force focus.
- **Why AsterShell strictly bans this:**  
  `AttachThreadInput` merges the message queues of both threads. If the target application enters an infinite loop, deadlocks on a COM call, or crashes, AsterTaskbar's entire message queue becomes locked, causing the taskbar to hang permanently. Furthermore, `AttachThreadInput` corrupts mouse capture state and desynchronizes global hotkeys.

---

## 5. System Notification Area (`Shell_NotifyIconW`)

### 5.1 Registration & GUID Identity
AsterShell uses `Shell_NotifyIconW` with `NOTIFYICON_VERSION_4` and GUIDs:

```rust
#[repr(C)]
pub struct NOTIFYICONDATAW {
    pub cbSize: u32,
    pub hWnd: HWND,
    pub uID: u32,
    pub uFlags: NOTIFY_ICON_FLAGS,
    pub uCallbackMessage: u32,
    pub hIcon: HICON,
    pub szTip: [u16; 128],
    pub dwState: NOTIFY_ICON_STATE,
    pub dwStateMask: NOTIFY_ICON_STATE,
    pub szInfo: [u16; 256],
    pub Anonymous: NOTIFYICONDATAW_0,
    pub szInfoTitle: [u16; 64],
    pub dwInfoFlags: NOTIFY_ICON_INFO_FLAGS,
    pub guidItem: windows::core::GUID,
    pub hBalloonIcon: HICON,
}
```

### 5.2 Callback Protocol (`NOTIFYICON_VERSION_4`)
When using `NOTIFYICON_VERSION_4`:
- `LOWORD(lParam)`: Event message (`WM_LBUTTONUP`, `WM_RBUTTONUP`, `WM_CONTEXTMENU`, `NIN_SELECT`, `NIN_KEYSELECT`).
- `HIWORD(lParam)`: Icon identifier.
- `LOWORD(wParam)`: X coordinate of the cursor in physical screen pixels.
- `HIWORD(wParam)`: Y coordinate of the cursor in physical screen pixels.

### 5.3 Explorer Crash Recovery (`TaskbarCreated`)
When `explorer.exe` crashes, Windows broadcasts:
```rust
let taskbar_created_msg = RegisterWindowMessageW(w!("TaskbarCreated"));
```
AsterTray's message pump intercepts this message and re-issues `NIM_ADD` and `NIM_SETVERSION`, guaranteeing zero lost icons after shell crashes.

---

## 6. System Status Indicators (Zero Polling Architecture)

### 6.1 Audio Volume via Core Audio COM
- **Interface:** `IAudioEndpointVolume` (part of Windows Core Audio MMDevice API).
- **Activation:**
  ```rust
  let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
  let endpoint = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
  let volume: IAudioEndpointVolume = endpoint.Activate(CLSCTX_ALL, None)?;
  ```
- **Event Sink:** Implement `IAudioEndpointVolumeCallback` and call `volume.RegisterControlChangeNotify(pCallback)`.
- **Result:** Invoked by Windows on audio thread only when volume level or mute status changes. Zero CPU polling.

### 6.2 Network Connectivity via Network List Manager COM
- **Interface:** `INetworkListManager` (`CLSID_NetworkListManager`).
- **Query:** `manager.GetConnectivity()` returns bitflags (`NLM_CONNECTIVITY_DISCONNECTED`, `NLM_CONNECTIVITY_IPV4_INTERNET`, `NLM_CONNECTIVITY_IPV6_INTERNET`).
- **Event Sink:** Connect to `IConnectionPoint` for `IID_INetworkListManagerEvents` -> `ConnectivityChanged`. Zero polling.

### 6.3 Power & Battery Status via Win32 Notifications
- **Query:** `GetSystemPowerStatus(&mut status)` retrieves `ACLineStatus` and `BatteryLifePercent`.
- **Event Notification:** `RegisterPowerSettingNotification(hwnd, &GUID_ACDC_POWER_SOURCE, DEVICE_NOTIFY_WINDOW_HANDLE)` and `&GUID_BATTERY_PERCENTAGE_REMAINING`.
- Windows posts `WM_POWERBROADCAST` messages to the window procedure. Zero polling.

---

## 7. Windows Compatibility & Version Matrix

| Platform Feature | Minimum Windows Version | Fallback Path |
| :--- | :--- | :--- |
| **DWM System Backdrop (Mica / Acrylic)** | Windows 11 Build 22000 | Fall back to opaque/semi-transparent background (`rgba`). |
| **DWM Window Corner Preference** | Windows 11 Build 22000 | System default corners. |
| **Per-Monitor V2 DPI Awareness** | Windows 10 Version 1703 (Build 15063) | System DPI scaling (`PerMonitor`). |
| **DWM Cloaked Window Detection** | Windows 8 (Build 9200) | Basic visibility check (`IsWindowVisible`). |
| **NOTIFYICON_VERSION_4 with GUIDs** | Windows 7 (Build 7600) | Legacy `(hWnd, uID)` tray registration. |
| **Core Audio Event Sinks** | Windows Vista (Build 6000) | Periodic status query. |

---

## 8. References
1. [Microsoft Learn: WinEvent Hooks](https://learn.microsoft.com/en-us/windows/win32/winmsg/about-winevents)
2. [Microsoft Learn: LockSetForegroundWindow & SetForegroundWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow)
3. [Microsoft Learn: Desktop Window Manager Attributes](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute)
4. [Microsoft Learn: High DPI Desktop Application Development on Windows](https://learn.microsoft.com/en-us/windows/win32/hidpi/high-dpi-desktop-application-development-on-windows)
5. [Microsoft Learn: Core Audio APIs](https://learn.microsoft.com/en-us/windows/win32/coreaudio/core-audio-apis-in-windows-vista)
