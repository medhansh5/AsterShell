# AsterShell — Reverse Engineering Behavioral Findings

This document records the empirical findings observed during static and dynamic analysis of the Windows 11 Build 26300 environment.

---

## 1. Finding 1: Modular Shell Process Architecture [FACT]

### Observed Fact
On Windows 11 Build 26300, the user shell experience is divided across four distinct processes:
1. `explorer.exe` (PID 11320, `C:\WINDOWS\Explorer.EXE`): Hosts `Progman`, desktop icons, system tray background, and in-process bridges to `Taskbar.dll`.
2. `SearchHost.exe` (PID 13204, `C:\WINDOWS\SystemApps\MicrosoftWindows.Client.CBS_cw5n1h2txyewy\SearchHost.exe`): Hosts the XAML search overlay.
3. `StartMenuExperienceHost.exe` (PID 13240, `C:\Windows\SystemApps\Microsoft.Windows.StartMenuExperienceHost_cw5n1h2txyewy\StartMenuExperienceHost.exe`): Hosts the Start menu grid.
4. `ShellExperienceHost.exe` (PID 19320): Hosts notifications and flyouts.

### Architectural Takeaway
Microsoft abandoned the monolithic `explorer.exe` model in favor of decoupled UI processes. AsterShell's multi-process architecture (`shell-host`, `taskbar`, `search-ui`, `start-menu`) directly matches this modern operating system reality while avoiding Microsoft's heavyweight XAML island runtime overhead.

---

## 2. Finding 2: Windows Search OLE DB Latency Profile [FACT]

### Observed Fact
Querying the `WSearch` service using the native OLE DB provider `Search.CollatorDSO` against `SystemIndex` demonstrated:
- **Cold Query Latency:** ~1,510 ms (includes OLE DB provider initialization, catalog attachment, and pipeline setup).
- **Warm Query Latencies:** 9 ms, 8 ms, 7 ms, 7 ms, 6 ms, 5 ms, 7 ms, 7 ms, 7 ms (average **~7.2 ms**).

### Architectural Takeaway
- Calling `Search.CollatorDSO` on an ad-hoc thread per keystroke will violate the sub-100ms latency budget due to cold start times.
- AsterShell's `search-core` must maintain a persistent, pooled OLE DB connection in a dedicated background worker thread, pre-warmed during shell startup.
- Under warm conditions, Windows Search easily meets the p95 < 100 ms requirement for file indexing without requiring AsterShell to crawl the entire disk.

---

## 3. Finding 3: Unified Application Discovery via `shell:AppsFolder` [FACT]

### Observed Fact
Traversing `shell:AppsFolder` using COM `IShellFolder` identified 137 applications, unifying:
- Modern packaged MSIX/UWP apps with Application User Model IDs (e.g., `Anysphere.Cursor`, `electron.app.Antigravity`).
- Native Win32 executables with direct file paths (e.g., `C:\Users\medha\AppData\Roaming\.minecraft\TLauncher.exe`).
- Web/PWA shortcuts (e.g., `Chrome._crx_...`).

### Architectural Takeaway
`shell:AppsFolder` is the single source of truth for all installed applications on Windows 11. Instead of maintaining disparate parsing logic for Start menu `.lnk` shortcuts, registry `App Paths`, and AppX manifests, AsterShell can synchronize with `shell:AppsFolder` to construct its in-memory SQLite application catalog.

---

## 4. Finding 4: Shell Replacement Registry Mechanisms [FACT]

### Observed Fact
- `HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon\Shell` defaults to `explorer.exe` system-wide.
- `HKCU:\Software\Microsoft\Windows NT\CurrentVersion\Winlogon\Shell` is empty by default.
- Per-user shell override in `HKCU` takes precedence over `HKLM` without requiring administrator elevation or modifying global system stability.

### Architectural Takeaway
- For **Companion Mode**, neither key is touched. AsterShell runs as a standard desktop suite.
- For **Full Shell Mode**, AsterShell can offer a safe, non-destructive per-user opt-in via `HKCU`, completely isolated from other Windows user profiles on the machine.

---

## 5. Finding 5: WinEvent Hooks vs Polling for Window Tracking [INFERENCE]

### Inferred & Supported Behavior
Polling `EnumWindows` on a high-frequency timer causes unnecessary CPU wakeups, battery drain, and stutter.
Using `SetWinEventHook` with `WINEVENT_OUTOFCONTEXT`:
- `EVENT_SYSTEM_FOREGROUND`: Detects active window switching immediately.
- `EVENT_OBJECT_CREATE` / `EVENT_OBJECT_DESTROY`: Detects application window lifecycle.
- `EVENT_OBJECT_NAMECHANGE`: Updates taskbar title strings.
- `EVENT_SYSTEM_MINIMIZESTART` / `EVENT_SYSTEM_MINIMIZEEND`: Updates minimized state.

### Architectural Takeaway
The `window-tracker` library will use an asynchronous WinEvent hook thread that streams window events across an in-memory channel to the taskbar state machine, dropping polling entirely.
