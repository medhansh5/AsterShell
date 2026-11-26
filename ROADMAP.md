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

## Milestone 1A: Core Infrastructure & Process Supervision (Completed)
- [x] Windows Job Object integration (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`).
- [x] Persistent `shell-host` daemon with child process supervisor.
- [x] Versioned Named Pipe IPC bus (`\\.\pipe\astershell-ipc-{hash}`) with custom framing header (`ASTS`).
- [x] Hierarchical configuration engine and structured diagnostics logging.
- [x] Independent exit audit completed: `APPROVE M1B WITH CONDITIONS`.

---

## Milestone 1B: Search Infrastructure & Hybrid Indexing (Completed)
- [x] In-memory SQLite FTS5 application index with prefix and fuzzy matching.
- [x] Windows Search OLE DB provider (`Search.CollatorDSO`) with connection pooling and query sanitization.
- [x] Integrated math evaluation provider.
- [x] Unified parallel search orchestrator with cancellation tokens and scoring.
- [x] Independent exit audit completed: `APPROVE M2 WITH CONDITIONS`.

---

## Milestone 2: Spotlight Interface (`search-ui`) (Completed)
- [x] Slint hardware-accelerated search overlay with keyboard navigation.
- [x] Global hotkey registration (`Alt + Space`) and foreground elevation.
- [x] DWM Mica / Acrylic system backdrops and dark mode styling.
- [x] Safe application activation abstraction (`shell-api`).
- [x] Independent exit audit completed: `APPROVE M2.1 WITH CONDITIONS`.

---

## Milestone 2.1: Security & Correctness Remediation + GitHub Publication (Completed)
- [x] SEC-01: Enforcement of strict mutual PID authentication in `shell-host` IPC listener.
- [x] UI-01: Generation-aware search event delivery eliminating race conditions.
- [x] LAUNCH-01: Packaged MSIX/UWP application launch via validated AUMID / `shell:AppsFolder`.
- [x] Published to GitHub (`https://github.com/medhansh5/AsterShell`) with CI/CD passing on Windows runners.
- [x] Independent post-publication audit completed: `APPROVE M3 ARCHITECTURE SPECIFICATION`.

---

## Milestone 3: Custom Taskbar & System Tray Integration (Current Stage — Architecture Approved)
**Objective:** Deliver AsterShell's first custom, multi-monitor taskbar (`AsterTaskbar`) and persistent system tray subsystem (`AsterTray`) in companion mode.

### Planned Sub-Stages:
- [ ] **M3A: Native Taskbar Window Foundation & Lifecycle:** Slint window creation, `WS_POPUP`, `WS_EX_TOOLWINDOW`, `WS_EX_NOACTIVATE`, DWM Mica backdrops, supervisor binding.
- [ ] **M3B: Monitor Topology, Dynamic DPI & Positioning:** `EnumDisplayMonitors`, per-monitor taskbar instances, `WM_DISPLAYCHANGE`, Per-Monitor V2 DPI scaling.
- [ ] **M3C: Running Application Discovery & Activation:** `crates/window-tracker` STA WinEvent hook thread, 7-stage HWND filtering, foreground window switching.
- [ ] **M3D: AsterShell-Owned System Tray Infrastructure:** `Shell_NotifyIconW` with `NOTIFYICON_VERSION_4` and GUID identity, `TaskbarCreated` recovery, Win32 context menu.
- [ ] **M3E: Taskbar Interactions, Clock & System Indicators:** Floating island UI layout, minute-synced clock, zero-polling CoreAudio volume, Network List Manager, battery status.
- [ ] **M3F: Integration, Performance, Reliability & Release Audit:** End-to-end stress tests, memory/CPU profiling, GitHub Actions CI verification, exit audit.

---

## Milestone 4: Start Menu & App Launcher (Future)
**Objective:** Complete modern Start flyout with pinned grid, recent files, and categorized apps.

---

## Milestone 5: Desktop Canvas & Workspace Management (Future)
**Objective:** Dedicated wallpaper canvas, desktop grid, and window tiling gestures.

---

## Milestone 6: Full Shell Replacement & Recovery Watchdog (Future)
**Objective:** Safe per-user shell configuration (`HKCU\...\Winlogon\Shell`) with automated Explorer fallback.
