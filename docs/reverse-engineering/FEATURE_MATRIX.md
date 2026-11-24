# AsterShell — Explorer Feature Matrix & Feasibility Analysis

This matrix contrasts the native Windows 11 Shell (`explorer.exe` and companion hosts) with the planned AsterShell implementation. Feasibility grades indicate public API support, undocumented dependencies, and engineering risk.

## Feasibility Grading Key
- **🟢 Full Public Support:** Well-documented, officially supported Win32/COM/WinRT APIs exist in the Windows SDK.
- **🟡 Partial / Constrained:** Can be implemented using public APIs, but requires workarounds, polling, or lacks private OS hooks.
- **🔴 Proprietary / Undocumented:** Relies on private Microsoft interfaces (`twinui.pcshell.dll`, undocumented COM GUIDs, or OS token privileges).

---

## 1. Feature Comparison & Feasibility Matrix

| Subsystem / Feature | Windows 11 Implementation | AsterShell Implementation | Public Feasibility | AsterShell Approach & Strategy |
| :--- | :--- | :--- | :---: | :--- |
| **Taskbar: Window Tracking** | `Taskbar.dll` hooked into WinEvent & ShellHooks | `window-tracker` crate with WinEvent hooks | 🟢 Full | Use `SetWinEventHook` (`EVENT_SYSTEM_FOREGROUND`, `EVENT_OBJECT_CREATE`, `EVENT_OBJECT_DESTROY`, `EVENT_OBJECT_LOCATIONCHANGE`) and `EnumWindows`. |
| **Taskbar: AppBar Docking** | Registers as native system AppBar via `SHAppBarMessage` | Native AppBar registration via `SHAppBarMessage` | 🟢 Full | Registers `ABM_NEW`, sets work area with `ABM_SETPOS`, preventing maximized windows from overlapping AsterShell taskbar. |
| **Taskbar: Live Window Previews** | DWM Thumbnail API (`DwmRegisterThumbnail`) | DWM Live Thumbnail rendering in companion window | 🟢 Full | Bind target HWND to AsterShell preview surface with `DwmRegisterThumbnail` and `DwmUpdateThumbnailProperties`. Low overhead GPU blit. |
| **Taskbar: Progress & Badges** | `ITaskbarList3` / `ITaskbarList4` | Emulate receiver of `ITaskbarList4` messages | 🟡 Constrained | Windows apps send progress to `Shell_TrayWnd`. In full shell replacement, AsterShell registers `Shell_TrayWnd` class or listens to shell broadcast messages. |
| **System Tray: Third-Party Icons** | `Shell_NotifyIcon` (`NIM_ADD`, `NIM_MODIFY`, `NIM_DELETE`) | `platform-windows` tray proxy / listener | 🟡 Constrained | Third-party apps communicate with `Shell_TrayWnd` via `WM_COPYDATA` and `Shell_NotifyIcon`. Recreating this requires hosting the tray message receiver window. |
| **System Tray: Windows Quick Settings** | `ControlCenter.exe` / `ShellExperienceHost.exe` | Custom Modern Quick Settings in Rust | 🟢 Full | Build custom native quick settings (Audio endpoints via `CoreAudio` COM, Wi-Fi via `WlanQueryInterface`, Bluetooth via WinRT `BluetoothAdapter`). |
| **Start Menu: App Catalog** | XAML Island hosted by `StartMenuExperienceHost.exe` | `application-catalog` crate reading `shell:AppsFolder` | 🟢 Full | Query `shell:AppsFolder` via COM `IShellFolder` to retrieve all 130+ packaged MSIX, UWP, and Win32 applications with their display names, icons, and AUMIDs. |
| **Start Menu: Execution** | `IApplicationActivationManager` & `ShellExecuteExW` | Unified launcher in `shell-api` | 🟢 Full | Execute Win32 executables via `ShellExecuteExW` and packaged WinRT/MSIX apps via `IApplicationActivationManager::ActivateApplication`. |
| **Search: Indexed Files** | `SearchHost.exe` talking to `WSearch` service | `search-providers` querying OLE DB `Search.CollatorDSO` | 🟢 Full | Query `SystemIndex` over OLE DB provider. Empirical warm query latency measured at **~7 ms**. |
| **Search: Application Launching** | Integrated Start catalog in Search UI | Instant fuzzy match against SQLite in-memory catalog | 🟢 Full | Sub-millisecond SQLite FTS5 / trigram index over pre-indexed application catalog. Zero disk I/O on keystroke. |
| **Search: Math & Evaluation** | Cloud Bing query + local calculator | Embedded fast evaluation engine (`qalculate` or Rust AST evaluator) | 🟢 Full | Pure local Rust math engine executing in < 1 ms. No network dependency. |
| **Desktop: Wallpaper & Icons** | `Progman` / `WorkerW` host with `SHELLDLL_DefView` | Dedicated desktop window layer beneath all windows | 🟢 Full | Create HWND placed directly above `Progman` (companion mode) or registered as the main desktop window via `SetShellWindow`. |
| **Desktop: Virtual Desktops** | Undocumented `IVirtualDesktopManagerInternal` COM | `IVirtualDesktopManager` (public) / custom workspaces | 🟡 Constrained | Public `IVirtualDesktopManager` only checks if a window is on the active desktop. Switching virtual desktops requires documented keystroke simulation or internal COM wrappers. |
| **Window Management: Snapping** | Windows 11 Snap Layouts (`SnapLayout.dll`) | Built-in tiling & snap overlay in `window-tracker` | 🟢 Full | Track mouse drag to screen edges and trigger proportional window resizing via `SetWindowPos` (`SWP_NOACTIVATE | SWP_NOZORDER`). |
| **File Explorer: Namespace** | Win32 namespace with `IShellFolder` & virtual folders | `filesystem` crate with native Win32 `IExplorerBrowser` / custom UI | 🟢 Full | Combine raw fast Win32 filesystem enumeration (`FindFirstFileExW`) with COM shell properties (`IShellItem2::GetPropertyStore`). |

---

## 2. Platform Boundaries & Intentional Exclusions

### What AsterShell Will NOT Attempt
1. **Private `twinui.pcshell.dll` Hooking:** AsterShell will not hook or inject into proprietary internal Windows Immersive Shell binaries. All behavior is built upon clean, documented Win32 and COM boundaries.
2. **Pixel-Perfect Windows 11 Clone:** The visual language of AsterShell is an original, clean, modern aesthetic with restrained motion and responsive layouts, avoiding Microsoft's visual regressions and advertising widgets.
3. **Indiscriminate Disk Crawling:** AsterShell will never crawl entire physical drives with brute-force directory walks. It strictly balances the OS `WSearch` indexer with targeted SQLite metadata.
