# AsterShell — Build & Run Guide

## Prerequisites
- **OS:** Windows 11 (Build 22000+, tested on Build 26300)
- **Toolchain:** Rust stable (`1.80+`, tested on `1.100.0`) + MSVC Build Tools
- **Git:** 2.40+

## Build & Verify
```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked -j 2
```

## Run in Companion Mode
```powershell
cargo build --release -p shell-host -p taskbar -p search-ui
.\target\release\shell-host.exe
```
`shell-host` automatically starts `taskbar.exe` under its Windows Job Object (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`), registers the persistent system tray icon (`NOTIFYICON_VERSION_4`), and binds the authenticated Named Pipe IPC server.

## Configuration (`%APPDATA%\AsterShell\config.toml`)
- `taskbar.monitor`: `"primary"` (default), `"all"` (one floating companion surface per active display), numeric index (`"0"`, `"1"`), or device name (`"\\\\.\\DISPLAY2"`).
- `taskbar.pinned_apps`: List of `{ process_key, title, monogram, launch_target }` entries validated against duplicates and unsafe shell metacharacters.
