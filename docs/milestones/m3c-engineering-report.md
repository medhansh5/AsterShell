# Milestone M3C Engineering Report — Window Tracking & Application Representation

**Milestone:** M3C (Window Tracking & Application Representation)  
**Branch:** `feat/m3-daily-driver-completion`  
**Status:** Complete & Verified

---

## 1. Summary of Implemented Changes

Milestone M3C implements event-driven top-level window discovery, a 7-stage window filtering pipeline, safe window activation/minimization/closing without `AttachThreadInput`, and live taskbar synchronization.

### 1.1 `crates/window-tracker/src/filter.rs`
- Implemented `WindowAttributes` and `should_track_attributes(...)` enforcing the 7-stage filter pipeline:
  1. Valid HWND (`IsWindow`)
  2. Visible (`IsWindowVisible`)
  3. Not a child window (`!WS_CHILD`)
  4. Not a tool window unless `WS_EX_APPWINDOW`
  5. Not owned unless `WS_EX_APPWINDOW`
  6. Not DWM cloaked (`DWMWA_CLOAKED == 0`)
  7. Non-empty bounding rect (unless minimized/`IsIconic`) and not an excluded shell class (`Shell_TrayWnd`, `Shell_SecondaryTrayWnd`, `Progman`, `WorkerW`, `NotifyIconOverflowWindow`, `TopLevelWindowForOverflowXamlIsland`, `Windows.UI.Core.CoreWindow`, `ApplicationFrameWindow_Hidden`, or AsterShell's own companion windows).

### 1.2 `crates/window-tracker/src/win32.rs` & `crates/window-tracker/src/lib.rs`
- Implemented `WindowInfo`, `WindowEvent`, and idempotent `WindowTrackerState`.
- Implemented `WindowTracker::start()` spawning a dedicated STA background thread (`window-tracker-winevent`) with strict **hook-before-snapshot** ordering: arms `SetWinEventHook` hooks (`WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS`) *before* taking the initial `EnumWindows` snapshot, then drains queued and live events via `GetMessageW`.
- Implemented clean `Drop` teardown via `PostThreadMessageW(WM_QUIT)`, `UnhookWinEvent`, and `join()`.
- Implemented `activate_window`, `minimize_window`, `toggle_window`, and `close_window` without `AttachThreadInput`.

### 1.3 `apps/taskbar/src/main.rs`
- Connected `WindowTracker::start()` to `TaskbarWindow`, updating live tracked window state on the Slint event loop.

---

## 2. Verification Results

All workspace quality gates passed after the final M3C edit:
- `cargo fmt --all -- --check`: PASS
- `cargo check --workspace --all-targets --locked`: PASS
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: PASS
- `cargo test --workspace --locked -j 2`: PASS (11/11 tests in `window-tracker`, plus all existing workspace tests).
