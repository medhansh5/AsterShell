# AsterShell Project Roadmap & Milestone Criteria

## Milestone 0: Architecture, Reverse Engineering & Environment Baseline (Completed)
- [x] Environment inspection: Windows 11 Build 26300, Rust 1.100.0, Cargo 1.100.0.
- [x] REA skill inspection and fallback strategy verification.
- [x] Empirical investigation of Windows 11 shell decoupling (`explorer.exe`, `SearchHost.exe`, `StartMenuExperienceHost.exe`).
- [x] Verification of `Search.CollatorDSO` latency (~7ms warm query).
- [x] Verification of `shell:AppsFolder` application discovery (137 applications).
- [x] 21-crate Cargo workspace established with clean unit tests passing.
- [x] Architecture specifications, ADR-0001, and reverse engineering ledgers published.

---

## Milestone 1: IPC Core, Process Supervision & Hybrid Search Prototype (Next)
**Objective:** Deliver the persistent supervisor daemon and a functional Spotlight-style search overlay.

### Explicit Acceptance Criteria:
1. **Supervisor Engine (`shell-host`, `process-supervisor`):**
   - Win32 Job Object successfully created with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
   - Child process crash detection implemented with exponential backoff and circuit breaker.
   - Named Pipe server accepting connections at `\\.\pipe\astershell-ipc-{hash}` with SID DACL.
2. **Versioned IPC Protocol (`ipc`):**
   - Framing header (`"ASTS"` magic, version 1, payload length) verified with unit tests.
   - Postcard binary serialization benchmarks showing roundtrip latency < 0.5 ms.
3. **Application Catalog (`application-catalog`):**
   - In-memory SQLite FTS5 database populating all 130+ applications from `shell:AppsFolder` in < 250 ms.
   - Instant prefix and fuzzy match queries completing in < 5 ms.
4. **Hybrid Search Engine (`search-core`, `search-providers`):**
   - Parallel query fan-out with cancellation token on new keystroke.
   - Connection pool to `Search.CollatorDSO` pre-warmed at startup.
   - Integrated arithmetic evaluator returning math results in < 1 ms.
5. **Search Overlay Prototype (`search-ui`):**
   - Global hotkey `Alt + Space` registers and brings overlay into foreground in < 30 ms.
   - Slint UI rendering results with keyboard navigation (Up, Down, Enter, Esc).
   - End-to-end p95 warm query-to-render latency verified under 100 ms.

---

## Milestone 2: Taskbar, Window Tracking & DWM Thumbnails
**Objective:** Deliver modern docked taskbar with real-time window tracking and live previews.

### Acceptance Criteria:
1. `window-tracker` registers asynchronous WinEvent hooks for window create, destroy, focus, and title change.
2. `taskbar` registers as an official Windows AppBar (`SHAppBarMessage` `ABM_NEW`, `ABM_SETPOS`) without clipping maximized windows.
3. DWM live thumbnail handles (`DwmRegisterThumbnail`) render smooth 60fps window hover cards.
4. Proper multi-monitor display enumeration and per-monitor DPI scaling.

---

## Milestone 3: Start Menu & App Launcher
**Objective:** Complete modern Start flyout with pinned grid, recent files, and categorized apps.

---

## Milestone 4: Desktop Canvas & Workspace Management
**Objective:** Dedicated wallpaper canvas, desktop grid, and window tiling gestures.

---

## Milestone 5: Full Shell Replacement & Recovery Watchdog
**Objective:** Safe per-user shell configuration (`HKCU\...\Winlogon\Shell`) with automated Explorer fallback.
