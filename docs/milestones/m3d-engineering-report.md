# Milestone M3D Engineering Report — System Tray & Notification-Area Integration

**Milestone:** M3D (System Tray Integration)  
**Branch:** `feat/m3-daily-driver-completion`  
**Status:** Complete & Verified

---

## 1. Summary of Implemented Changes

Milestone M3D implements AsterShell's own persistent system tray icon in independent companion mode, with Explorer crash recovery and native context menu focus handling. Windows Explorer continues hosting third-party notification-area icons untouched.

### 1.1 `crates/platform-windows/src/tray.rs`
- Implemented `TrayIconHandle` and `TrayAction` (`OpenSearch`, `ToggleTaskbar`, `Exit`).
- Registered AsterShell's persistent tray icon via `Shell_NotifyIconW` (`NIM_ADD` + `NIM_SETVERSION` with `NOTIFYICON_VERSION_4`) and constant GUID `ASTERSHELL_TRAY_GUID_U128`.
- Created a dedicated STA listener thread with a hidden top-level `WS_OVERLAPPED` window (`AsterShellTrayListenerWindow`, intentionally non-`HWND_MESSAGE` so it receives `HWND_BROADCAST` notifications).
- Registered `RegisterWindowMessageW("TaskbarCreated")` to automatically re-add the tray icon if Explorer restarts.
- Implemented native Win32 context menu (`CreatePopupMenu` / `TrackPopupMenuEx` / `DestroyMenu`) with the required `SetForegroundWindow(hwnd)` before `TrackPopupMenuEx` and `PostMessageW(hwnd, WM_NULL, 0, 0)` after, ensuring reliable click-outside dismissal.
- Implemented clean `Drop` teardown calling `Shell_NotifyIconW(NIM_DELETE, ...)` and `PostThreadMessageW(WM_QUIT)`.

### 1.2 `apps/shell-host/src/main.rs`
- Integrated `TrayIconHandle::start("AsterShell Companion")` into `ShellHostDaemon::run()`.
- Wired `TrayAction::Exit` to trigger clean supervisor shutdown across all child processes.

---

## 2. Verification Results

All workspace quality gates passed after the final M3D edit:
- `cargo fmt --all -- --check`: PASS
- `cargo check --workspace --all-targets --locked`: PASS
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: PASS
- `cargo test --workspace --locked -j 2`: PASS (`tray::tests::test_tray_guid_constant_non_zero`, `tray::tests::test_tray_icon_lifecycle_and_clean_drop`, plus all existing workspace tests).
