# AsterShell — Windows 11 Shell Component Architecture Analysis

This document details the observed internal architecture of the modern Windows 11 Shell and defines how AsterShell decomposes those capabilities into a modular, multi-process Rust architecture.

---

## 1. Observed Windows 11 Shell Topology

Modern Windows 11 does **not** run as a single monolithic `explorer.exe`. It operates as a multi-process federated subsystem:

```
+---------------------------------------------------------------------------------------+
|                                    WINDOWS 11 OS                                      |
+---------------------------------------------------------------------------------------+
|                                                                                       |
|   +------------------------------------+   +--------------------------------------+   |
|   |         EXPLORER.EXE (Win32)       |   |         SearchHost.exe (UWP/CBS)     |   |
|   |  - Win32 Host Window (Progman)     |   |  - XAML Search UI Engine             |   |
|   |  - AppBar & Shell_TrayWnd HWND     |   |  - Edge WebView2 Host                |   |
|   |  - Shell Namespace Dispatcher      |   |  - Connects to WSearch Service       |   |
|   |  - In-Process Host for Taskbar.dll |   +--------------------------------------+   |
|   |  - File Explorer Window Instances  |                                              |
|   +-----------------+------------------+   +--------------------------------------+   |
|                     |                      |    StartMenuExperienceHost.exe       |   |
|                     |                      |  - Modern Start UI / Pins Grid       |   |
|                     |                      |  - AppX XAML Islands Surface         |   |
|                     v                      +--------------------------------------+   |
|   +------------------------------------+                                              |
|   |      WSearch (Windows Service)     |   +--------------------------------------+   |
|   |  - Indexing Engine (ESE database)  |   |        ShellExperienceHost.exe       |   |
|   |  - OLE DB Provider (CollatorDSO)   |   |  - Action Center / Flyouts           |   |
|   +------------------------------------+   +--------------------------------------+   |
|                                                                                       |
+---------------------------------------------------------------------------------------+
```

### Component Analysis:

1. **`explorer.exe` (Core Shell Host):**
   - **Role:** Registers the desktop window (`Progman`), primary system tray (`Shell_TrayWnd`), and secondary taskbars (`Shell_SecondaryTrayWnd`).
   - **Modern Windows 11 Pivot:** In Build 26300, `explorer.exe` loads `Taskbar.dll`, `Taskbar.View.dll`, and `SystemTray.dll` from the `MicrosoftWindows.Client.Core_cw5n1h2txyewy` package, delegating rendering to WinUI/XAML while retaining legacy HWND message loops for backward compatibility.
2. **`SearchHost.exe` (Windows Search Host):**
   - **Role:** Houses the modern Windows 11 search flyout. It is an isolated modern application that queries `WSearch` in the background and renders local and web results.
3. **`StartMenuExperienceHost.exe` (Start Menu):**
   - **Role:** Completely separated out-of-process XAML application activated via Shell RPC and global hotkey hooks (`Win` key).
4. **`WSearch` (Windows Search Indexer Service):**
   - **Role:** Background Windows NT service hosting an Extensible Storage Engine (ESE / JET Blue) database that continuously monitors file change events (USN journal / ReadDirectoryChangesW) and exposes an OLE DB interface `Search.CollatorDSO`.

---

## 2. AsterShell Decoupled Architecture Mapping

AsterShell replaces this tightly coupled, fragile Windows model with an explicit, resilient multi-process architecture orchestrated by `shell-host`:

```
+---------------------------------------------------------------------------------------+
|                                    ASTERSHELL SYSTEM                                  |
+---------------------------------------------------------------------------------------+
|                                                                                       |
|                                +----------------------+                               |
|                                |      shell-host      |                               |
|                                | (Process Supervisor) |                               |
|                                +----------+-----------+                               |
|                                           |                                           |
|                      +--------------------+--------------------+                      |
|                      | Named Pipe IPC Bus (Versioned Protocol) |                      |
|                      +--------------------+--------------------+                      |
|                                           |                                           |
|       +-------------------+---------------+-------------------+-------------------+   |
|       |                   |                                   |                   |   |
|       v                   v                                   v                   v   |
| +-----------+     +---------------+                   +---------------+     +-------+ |
| |  taskbar  |     |   search-ui   |                   |  start-menu   |     |desktop| |
| | (AppBar & |     | (Spotlight-   |                   | (Application  |     | (Root | |
| |  Tracker) |     |   Search)     |                   |  Launcher)    |     |Layer) | |
| +-----------+     +-------+-------+                   +---------------+     +-------+ |
|                           |                                                           |
|                           v                                                           |
|             +----------------------------+                                            |
|             |        search-core         |                                            |
|             | (Federated Hybrid Engine)  |                                            |
|             +--------------+-------------+                                            |
|                            |                                                          |
|             +--------------+-------------+                                            |
|             |                            |                                            |
|             v                            v                                            |
|     +---------------+            +---------------+                                    |
|     |    WSearch    |            |  SQLite FTS5  |                                    |
|     |  (Win32 OLEDB)|            | (Custom Index)|                                    |
|     +---------------+            +---------------+                                    |
+---------------------------------------------------------------------------------------+
```

### Decoupling Guarantees:
- **Fault Isolation:** If `search-ui` encounters an unexpected parsing issue or `taskbar` renders an unusual glyph, only that specific process restarts. The `shell-host` supervisor detects process termination via Windows job objects and `WaitForSingleObject` and restores it within 50 ms.
- **Independent Evolution:** The UI rendering engine (Slint) can be updated or reconfigured without modifying low-level Win32 window-tracking hooks.
- **Zero Explorer Mutation:** During companion mode, AsterShell does not overwrite `explorer.exe` or hook private internal memory offsets.
