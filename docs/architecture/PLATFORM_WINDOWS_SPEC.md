# AsterShell — Windows Platform Integration Layer Specification

This document details the low-level Win32, COM, and WinRT platform bindings, memory safety rules, and architectural patterns implemented in `platform-windows`, `window-tracker`, and `shell-api`.

---

## 1. Rust Ecosystem & Crate Strategy

AsterShell uses official Microsoft Windows bindings for Rust:
- **`windows` crate (v0.58+):** Primary interface for COM interfaces (`IShellFolder`, `IApplicationActivationManager`), DWM graphics (`DwmRegisterThumbnail`), and WinRT integrations.
- **`windows-sys` crate:** High-throughput, low-overhead C-compatible bindings for performance-critical Win32 message pump routines, hook callbacks, and raw struct manipulations.

### Unsafe Code Isolation & Safety Policy:
1. **Zero Unchecked FFI in Business Logic:** All raw Win32 pointer manipulation and COM vtable calls must be wrapped inside safe, idiomatic Rust structs in `platform-windows` or `shell-api`.
2. **RAII Resource Management:** Every Win32 handle (`HWND`, `HANDLE`, `HKEY`, `HHOOK`) must be encapsulated within a Rust wrapper implementing `Drop` (e.g., `CloseHandle`, `UnhookWinEvent`, `RegCloseKey`, `DeregisterShellHookWindow`).
3. **Explicit Documentation:** Every `unsafe` block must include a `// SAFETY:` invariant comment justifying why pointer alignment, buffer size, and lifetime safety are guaranteed.

---

## 2. Core Subsystem Implementations

### 2.1 Window Tracking (`window-tracker`)
- **Mechanism:** Asynchronous WinEvent Hook listening via `SetWinEventHook` with `WINEVENT_OUTOFCONTEXT`.
- **Target Events:**
  - `EVENT_SYSTEM_FOREGROUND`: Detects active window focus change.
  - `EVENT_OBJECT_CREATE` / `EVENT_OBJECT_DESTROY`: Detects window creation and destruction.
  - `EVENT_OBJECT_NAMECHANGE`: Updates window titles in taskbar button labels.
  - `EVENT_SYSTEM_MINIMIZESTART` / `EVENT_SYSTEM_MINIMIZEEND`: Updates minimized state.
- **Filtering:** Filters out tooltips, menu popups, off-screen helper windows, and cloaked DWM windows (`DWMWA_CLOAKED`).

### 2.2 System AppBar Docking (`platform-windows`)
- **Mechanism:** Registers AsterShell taskbar as a native system AppBar via `SHAppBarMessage`.
- **Operations:**
  - `ABM_NEW`: Registers the taskbar HWND.
  - `ABM_SETPOS`: Sets edge docking (`ABE_BOTTOM`, `ABE_TOP`) and size, reducing the desktop `SPI_SETWORKAREA` so maximized application windows dock cleanly against the taskbar without clipping or overlapping.
  - `ABM_REMOVE`: Restores original work area upon clean exit.

### 2.3 Live Window Previews via DWM (`platform-windows`)
- **Mechanism:** Direct GPU composition via `DwmRegisterThumbnail` and `DwmUpdateThumbnailProperties`.
- **Implementation:**
  - Generates a lightweight child HWND in the taskbar process.
  - Calls `DwmRegisterThumbnail(hwnd_dest, hwnd_source_app, &mut hthumbnail)`.
  - Configures destination rect, opacity, and visible region.
  - Automatically unregisters thumbnail handle when hover card dismisses.

### 2.4 Multi-Monitor & Per-Monitor V2 DPI Handling
- **Mechanism:** Process manifests specify `PerMonitorV2` DPI awareness.
- **Dynamic Resizing:** Listens to `WM_DPICHANGED` messages.
- **Multi-Monitor:** Enumerates all active displays with `EnumDisplayMonitors`. Dispatches independent taskbar instances or mirrors across secondary monitors (`Shell_SecondaryTrayWnd` emulation).

---

## 3. Platform Limitations & Undocumented Boundaries

| Subsystem | Public Support Level | Documented Platform Limitation | AsterShell Mitigation |
| :--- | :---: | :--- | :--- |
| **System Tray Icons** | Constrained | Windows third-party tray icons send `WM_COPYDATA` messages expecting `Shell_TrayWnd` to receive them. In companion mode, Explorer retains this. | In companion mode, AsterShell leaves tray icons in Explorer. In full shell mode, AsterShell provides a tray proxy listener. |
| **Quick Settings Flyout** | Proprietary | Action Center / Quick Settings are implemented inside internal `ControlCenter.exe` with proprietary XAML hooks. | AsterShell builds its own modern Quick Settings flyout using public Windows audio (`CoreAudio` COM), Wi-Fi (`Wlan` API), and power APIs. |
| **Virtual Desktops** | Proprietary | `IVirtualDesktopManager` is read-only. Switching virtual desktops has no public API in the Windows SDK. | In Phase 0/1, AsterShell manages window grouping within its own taskbar workspaces, avoiding unstable private GUIDs. |
| **Windows Notification Toast Center** | Proprietary | Microsoft toasts are dispatched via Windows Notification Service (WNS) and stored in proprietary SQLite action center databases. | AsterShell registers as an official WinRT toast listener via `ToastNotificationManagerCompat` where available. |
