# AsterShell — Troubleshooting Guide

- **Taskbar window does not appear:** Check `%APPDATA%\AsterShell\config.toml` (`taskbar.enabled = true`) and ensure `taskbar.exe` is in the same directory as `shell-host.exe` (or built in `target\debug` / `target\release`).
- **Tray icon missing after Explorer crash:** AsterShell listens for the `TaskbarCreated` broadcast on a hidden top-level `WS_OVERLAPPED` window and automatically re-registers via `Shell_NotifyIconW`. If Explorer is not running, the tray icon cannot be displayed until Explorer starts.
- **Cannot activate an elevated window:** Windows User Interface Privilege Isolation (UIPI) prevents non-elevated `taskbar.exe` from raising elevated (Administrator) windows via `SetForegroundWindow`. Run both at the same integrity level if needed.
- **Audio indicator shows unavailable or stale:** Verify a default render audio endpoint is enabled in Windows Sound settings. `IMMNotificationClient` automatically rebinds when the default output device changes.
- **High memory during `cargo test`:** Always pass `-j 2` (`cargo test --workspace --locked -j 2`) when compiling/running Slint + Win32 tests concurrently on Windows hosts.
