# AsterShell — Windows API & COM Interface Catalog

This catalog details the specific Win32 APIs, COM interfaces, WinRT namespaces, and corresponding Rust crates required for AsterShell's core capabilities.

---

## 1. Window Management & Desktop Shell APIs

| Category | API / Interface | Header / Typelib | Rust Crate (`windows` feature) | Purpose in AsterShell |
| :--- | :--- | :--- | :--- | :--- |
| **Window Enumeration** | `EnumWindows`, `EnumDesktopWindows` | `winuser.h` | `Win32_UI_WindowsAndMessaging` | Enumerate all top-level application windows on startup. |
| **Window Metadata** | `GetWindowTextW`, `GetClassNameW`, `GetWindowRect` | `winuser.h` | `Win32_UI_WindowsAndMessaging` | Query titles, classes, and bounds of running windows. |
| **WinEvent Hooks** | `SetWinEventHook`, `UnhookWinEvent` | `winuser.h` | `Win32_UI_Accessibility` | Real-time notifications for window creation, destruction, focus change, and title updates. |
| **Shell Hooks** | `RegisterShellHookWindow`, `DeregisterShellHookWindow` | `winuser.h` | `Win32_UI_WindowsAndMessaging` | Receive shell broadcast messages (`HSHELL_WINDOWCREATED`, `HSHELL_WINDOWDESTROYED`, `HSHELL_WINDOWACTIVATED`). |
| **AppBar Registration** | `SHAppBarMessage` | `shellapi.h` | `Win32_UI_Shell` | Register the taskbar as an official Windows AppBar (`ABM_NEW`, `ABM_SETPOS`, `ABM_REMOVE`), reserving desktop workspace. |
| **Live Previews** | `DwmRegisterThumbnail`, `DwmUpdateThumbnailProperties`, `DwmUnregisterThumbnail` | `dwmapi.h` | `Win32_Graphics_Dwm` | Render live, GPU-accelerated window thumbnails in taskbar hover cards. |
| **Backdrop Effects** | `DwmSetWindowAttribute` (`DWMWA_SYSTEMBACKDROP_TYPE`, `DWMWA_USE_IMMERSIVE_DARK_MODE`) | `dwmapi.h` | `Win32_Graphics_Dwm` | Enable native Windows 11 Mica, Mica Alt, or Acrylic blur on AsterShell windows. |
| **Desktop Hooking** | `SetShellWindow` | `winuser.h` | `Win32_UI_WindowsAndMessaging` | Designate the AsterShell desktop process as the root desktop shell window (Full Shell mode). |

---

## 2. Application Discovery & Launching

| Category | API / Interface | Header / Typelib | Rust Crate (`windows` feature) | Purpose in AsterShell |
| :--- | :--- | :--- | :--- | :--- |
| **Shell Namespace** | `SHGetKnownFolderItem` (`FOLDERID_AppsFolder`) | `shlobj.h` | `Win32_UI_Shell` | Access the unified virtual folder containing all installed Win32, MSIX, and UWP apps. |
| **Folder Enumeration** | `IShellFolder`, `IEnumIDList`, `STRRET` | `shobjidl.h` | `Win32_UI_Shell` | Traverse items in `shell:AppsFolder`, retrieving display names and parsing paths. |
| **Item Properties** | `IShellItem2::GetPropertyStore`, `PROPERTYKEY` | `shobjidl.h` | `Win32_UI_Shell_PropertiesSystem` | Extract `PKEY_AppUserModel_ID`, `PKEY_ItemNameDisplay`, and application icon paths. |
| **Win32 Execution** | `ShellExecuteExW` | `shellapi.h` | `Win32_UI_Shell` | Launch standard Win32 executables, documents, and shortcuts with appropriate verb and elevation handling. |
| **UWP/MSIX Launch** | `IApplicationActivationManager::ActivateApplication` | `shobjidl.h` | `Win32_UI_Shell` | Launch modern packaged Windows apps by passing their Application User Model ID (AUMID). |
| **App Paths Registry** | `RegOpenKeyExW`, `RegQueryValueExW` | `winreg.h` | `Win32_System_Registry` | Scan registered Win32 executables in `SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths`. |

---

## 3. Windows Search & Indexing

| Category | API / Interface | Header / Typelib | Rust Crate (`windows` feature) | Purpose in AsterShell |
| :--- | :--- | :--- | :--- | :--- |
| **OLE DB Query** | `IDBInitialize`, `IDBCreateSession`, `ICommandText` | `oledb.h` | Custom COM bindings or `ADODB` | Execute SQL queries against `Search.CollatorDSO` provider targeting `SystemIndex`. |
| **Search Management** | `ISearchManager`, `ISearchCatalogManager` | `searchapi.h` | `Win32_System_Search` | Query catalog status, indexing state, and crawl health. |
| **Search Helper** | `ISearchQueryHelper` | `searchapi.h` | `Win32_System_Search` | Generate dialect-compliant SQL queries and apply file scope restrictions. |
| **USN Journal** | `FSCTL_QUERY_USN_JOURNAL`, `FSCTL_READ_USN_JOURNAL` | `winioctl.h` | `Win32_System_Ioctl` | Read NTFS change journal for instant, low-overhead incremental filesystem indexing. |

---

## 4. Multi-Monitor & DPI Awareness

| Category | API / Interface | Header / Typelib | Rust Crate (`windows` feature) | Purpose in AsterShell |
| :--- | :--- | :--- | :--- | :--- |
| **DPI Awareness** | `SetProcessDpiAwarenessContext` (`DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2`) | `winuser.h` | `Win32_UI_HiDpi` | Ensure crisp rendering across displays with different scaling factors. |
| **Monitor Bounds** | `EnumDisplayMonitors`, `GetMonitorInfoW` | `winuser.h` | `Win32_Graphics_Gdi` | Discover all active displays, resolutions, and work areas. |
| **DPI Queries** | `GetDpiForMonitor`, `GetDpiForWindow` | `shellscalingapi.h` | `Win32_UI_HiDpi` | Query precise scaling factor to render pixel-perfect taskbar and search overlays. |

---

## 5. Process Supervision & IPC

| Category | API / Interface | Header / Typelib | Rust Crate (`windows` feature) | Purpose in AsterShell |
| :--- | :--- | :--- | :--- | :--- |
| **Job Objects** | `CreateJobObjectW`, `AssignProcessToJobObject` | `jobapi2.h` | `Win32_System_JobObjects` | Enforce clean process lifecycle: terminating `shell-host` terminates all child UI components cleanly. |
| **Named Pipes** | `CreateNamedPipeW`, `ConnectNamedPipe` | `winbase.h` | `Win32_System_Pipes` | High-throughput, low-latency inter-process communication bus with security descriptors. |
| **Process Wait** | `WaitForSingleObject`, `RegisterWaitForSingleObject` | `synchapi.h` | `Win32_System_Threading` | Non-blocking detection of child process crashes for instant automated restarts. |
