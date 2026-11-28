# AsterShell Architecture Specification

## 1. Architectural Philosophy & Principles

AsterShell is engineered from the ground up as a native, resilient, modular Windows desktop shell in Rust. It adheres to six core architectural principles:

1. **Supervised Fault Isolation:** No single component failure (e.g., a search rendering glitch or a window-thumbnail crash) may compromise the shell host or other running applications.
2. **Minimal In-Process Blast Radius:** UI frontends and low-level platform trackers execute in distinct operating system processes managed by a persistent supervisor (`shell-host`).
3. **Explicit, Versioned IPC Contracts:** All cross-process communication occurs over local Windows Named Pipes using strictly typed, version-negotiated message envelopes.
4. **Zero Proprietary Hooks:** Rely exclusively on documented Win32, COM, and WinRT APIs. Never patch OS kernel binaries, inject DLLs into third-party processes, or depend on brittle internal Microsoft GUIDs.
5. **Separation of Platform and UI:** Windows platform bindings (`platform-windows`) remain completely decoupled from the UI rendering layer (`ui-components`, Slint/Win32).
6. **Sub-100ms Perceived Performance:** The flagship search and launcher components must maintain warm query-to-result latency under 100 ms (p95 target).

---

## 2. High-Level System Topology

```
+-------------------------------------------------------------------------------------------------------+
|                                              ASTERSHELL                                               |
+-------------------------------------------------------------------------------------------------------+
|                                                                                                       |
|                                       +-----------------------+                                       |
|                                       |      shell-host       |                                       |
|                                       | (Supervisor / Daemon) |                                       |
|                                       +-----------+-----------+                                       |
|                                                   |                                                   |
|                        +--------------------------+--------------------------+                        |
|                        |  Windows Job Object (Shared Process Tree Lifetime)  |                        |
|                        +--------------------------+--------------------------+                        |
|                                                   |                                                   |
|                         Named Pipe IPC Bus (`\\.\pipe\astershell-ipc-{user}`)                         |
|     +---------------------+-----------------------+-----------------------+---------------------+     |
|     |                     |                       |                       |                     |     |
|     v                     v                       v                       v                     v     |
| +-------+           +-----------+           +-----------+           +-----------+         +-------+   |
| |taskbar|           | search-ui |           |start-menu |           |  desktop  |         |file-  |   |
| | (UI)  |           | (Overlay) |           | (Launcher)|           | (Surface) |         |manager|   |
| +---+---+           +-----+-----+           +-----+-----+           +-----+-----+         +---+---+   |
|     |                     |                       |                       |                   |       |
+-----+---------------------+-----------------------+-----------------------+-------------------+-------+
|                                         SHARED RUST LIBRARIES                                         |
+-------------------------------------------------------------------------------------------------------+
|  astercore             | Platform-agnostic core types, error models, and async primitives.            |
|  platform-windows      | Safe, idiomatic Rust abstractions over Win32, COM, WinRT, and DWM APIs.     |
|  shell-api             | Unified application activation, shortcut resolution, and shell verbs.        |
|  ipc                   | Versioned framing, serialization, and async Windows named-pipe client/server.|
|  process-supervisor    | Windows Job Objects, health monitoring, crash telemetry, and backoff logic.   |
|  window-tracker        | Real-time WinEvent hooks, Z-order tracking, and DWM live thumbnail handles.  |
|  search-core           | Search orchestrator, query classification, deduplication, and scoring.       |
|  search-providers      | Provider implementations: Windows Search OLE DB, SQLite FTS5, Calculator.    |
|  search-index          | Embedded SQLite index for metadata, user history, and custom fast paths.     |
|  application-catalog   | Automatic discovery and indexing of Win32, packaged MSIX/UWP, and PWAs.     |
|  filesystem            | Fast directory traversal, metadata extraction, and USN journal reader.       |
|  configuration         | Hierarchical configuration engine with live hot-reloading and schema checks.  |
|  ui-components         | Reusable design tokens, modern themes, and layout primitives.                |
|  diagnostics           | Structured tracing (`tracing-subscriber`), telemetry, and performance spans. |
+-------------------------------------------------------------------------------------------------------+
```

---

## 3. Application Roles & Responsibilities

| Application Binary | Role & Responsibility | Window Type / Mode | Failure Impact |
| :--- | :--- | :--- | :--- |
| **`shell-host`** | Persistent daemon and supervisor. Manages child processes, IPC broker, hotkey registration, configuration broadcast, and crash telemetry. | Headless (System Tray status icon) | If `shell-host` terminates, the Windows Job Object terminates child processes cleanly. |
| **`taskbar`** | Floating companion taskbar. Displays configured pinned/running apps, window state, minute-synced clock, and zero-polling system indicators. | Floating `WS_POPUP` companion (`WS_EX_TOPMOST \| WS_EX_TOOLWINDOW \| WS_EX_NOACTIVATE`); zero AppBar or work-area mutation | Independent restart. Work area preserved. |
| **`search-ui`** | Fast global search overlay triggered by `Alt + Space`. Instant fuzzy application matching, indexed file search, math evaluation. | Top-level frameless popup with DWM Mica / Acrylic | Independent restart. Disappears on blur. |
| **`start-menu`** | Application launcher, categorized pinned items, power options, and user profile management. | Anchored modal flyout | Independent restart. Triggered on demand. |
| **`desktop`** | Root desktop wallpaper surface, multi-monitor canvas, desktop icon grid, and selection rectangle. | Bottom-most layered window (`HWND_BOTTOM`) | Independent restart. Does not affect open application windows. |
| **`file-manager`** | Complete modern tabbed file manager. Fast directory browsing, dual-pane navigation, and file operations. | Normal top-level Win32 window | Independent multi-instance desktop app. |
| **`settings`** | Visual configuration panel for theming, hotkeys, search providers, and shell deployment mode. | Normal top-level Win32 window | On-demand utility. Runs only when opened. |

---

## 4. Process Supervision & Crash Recovery

1. **Windows Job Object Binding:** The supervisor creates a Win32 Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`. When `shell-host` shuts down or exits, all child processes are cleaned up automatically by the Windows kernel, preventing orphaned processes.
2. **Crash Detection:** Child process handles are registered with `RegisterWaitForSingleObject` or monitored in an async supervisor loop. Process termination is detected within milliseconds.
3. **Restart Policy:**
   - **Immediate Transient Restart:** If a process terminates with a non-zero exit code, it is immediately restarted up to 3 times in 10 seconds.
   - **Exponential Backoff:** If crashes persist, restart interval backs off (1s, 2s, 4s, 8s) up to 30 seconds to prevent CPU exhaustion.
   - **Circuit Breaker:** If a child crashes 5 times consecutively within 60 seconds, it is paused and a notification is dispatched to `shell-host` diagnostics.
4. **State Restoration:** UI processes retain no persistent business state. All essential state (active windows, running apps, catalog) resides in libraries or is queried directly from Windows OS upon reconnection.

---

## 5. IPC Architecture & Message Flow

- **Transport:** Windows Named Pipes located at `\\.\pipe\astershell-ipc-{hash(username)}`.
- **Security:** Strict security descriptor granting read/write access solely to the current user token and full control to Administrators (`SDDL: D:(A;;GRGW;;;{UserSID})(A;;GA;;;BA)`). No cross-session access. Client process identity is verified using `GetNamedPipeClientProcessId`.
- **Framing:** 8-byte header:
  - `Magic`: `[u8; 4]` (`"ASTS"`)
  - `Version`: `u16` (`1`)
  - `MessageLength`: `u16` (up to 65,535 bytes per frame)
  - `Payload`: Binary message encoded via `postcard`.
- **Handshake & Negotiation:** Upon connection, the client and server exchange `HandshakeRequest` and `HandshakeResponse` negotiating maximum protocol version and supported feature flags. Client process ID is recorded and logged.
