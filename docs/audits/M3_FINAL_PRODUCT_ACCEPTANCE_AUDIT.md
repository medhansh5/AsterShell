# AsterShell — M3 Final Product Acceptance Audit

**Repository:** `https://github.com/medhansh5/AsterShell.git` (`medhansh5/AsterShell`)  
**Branch:** `feat/m3-daily-driver-completion`  
**Pull Request:** [#2](https://github.com/medhansh5/AsterShell/pull/2) (`OPEN`, `mergedAt: null`)  
**Exact Audited Commit SHA:** `b143bd7811a7b88215bdaa2d4232533f0d5836f5`  
**Base Branch:** `main` (`a1fc2dc8cbbd14926f4ed6d6ec2e408b742b50d8`)  
**Final Verdict:** **`CONDITIONAL APPROVAL — MANUAL VALIDATION REQUIRED`**

---

## 1. Repository & PR State

- **Working Tree:** Clean (`nothing to commit, working tree clean` prior to creating the two audit documents).
- **Remote Sync:** Local `feat/m3-daily-driver-completion` matches `origin/feat/m3-daily-driver-completion` at `b143bd7811a7b88215bdaa2d4232533f0d5836f5`.
- **PR #2 State:** `OPEN`, unmerged (`mergedAt: null`), `headRefOid: b143bd7811a7b88215bdaa2d4232533f0d5836f5`.
- **GitHub Actions CI:** Run [`46105955936`](https://github.com/medhansh5/AsterShell/actions/runs/46105955936/job/140858618077) (`Windows Quality Gates`) — `status: completed`, `conclusion: success`, `headSha: b143bd7811a7b88215bdaa2d4232533f0d5836f5`.

---

## 2. Scope & Methodology

This independent product acceptance audit inspected the complete M3 codebase (`apps/taskbar`, `apps/shell-host`, `apps/search-ui`, `crates/platform-windows`, `crates/window-tracker`, `crates/shell-api`, `crates/ipc`, `crates/process-supervisor`, `crates/configuration`), all project documentation, and executed all workspace build, lint, and test gates on exact HEAD `b143bd7811a7b88215bdaa2d4232533f0d5836f5`.

---

## 3. Automated Verification Results (Exact HEAD `b143bd7811a7b88215bdaa2d4232533f0d5836f5`)

| Check | Command | Exit Code | Result |
|---|---|---:|---|
| Formatting | `cargo fmt --all -- --check` | `0` | **PASS** |
| Compilation | `cargo check --workspace --all-targets --locked` | `0` | **PASS** |
| Strict Linter | `cargo clippy --workspace --all-targets --locked -- -D warnings` | `0` | **PASS** (0 warnings) |
| Workspace Tests | `cargo test --workspace --locked -j 2` | `0` | **PASS** (**140 passed, 0 failed, 0 ignored**) |

Targeted suites verified within the 140 passing tests:
- `platform-windows`: 26/26 passed (`monitor::tests::*`, `audio::tests::*`, `network::tests::*`, `power::tests::*`, `tray::tests::*`, `test_job_object_lifecycle`, `test_default_pipe_sddl_and_security_descriptor`).
- `window-tracker`: 11/11 passed (7-stage filter, `test_live_enum_windows_snapshot`, `test_tracker_start_snapshot_and_clean_drop`, `test_state_machine_idempotent_upsert_and_remove`).
- `taskbar` unit + `taskbar_foundation` + `m3_daily_driver_integration`: 6 + 11 + 3 = 20/20 passed (real `JobObject` `KILL_ON_JOB_CLOSE` containment, authenticated IPC shutdown, real Slint `TestTaskbarWindow` native styles, `HwndHandleTable` 64-bit HWND preservation, work-area invariant).
- `shell-host`: 14/14 passed (`test_tray_action_dispatch_all_handlers`, mutual PID auth, child-cannot-shutdown-host, IPC lifecycle).
- `configuration`: 5/5 passed (`test_pinned_apps_validation_and_backward_compat`, corrupt fallback, atomic roundtrip).
- `shell-api`: 10/10 passed; `ipc`: 6/6 passed; `process-supervisor`: 4/4 passed; `search-*`: 27/27 passed; `application-catalog`/`astercore`/others: 17/17 passed.

---

## 4. Manual Validation Status

**0 of 18** interactive desktop acceptance tests in [M3_FINAL_MANUAL_ACCEPTANCE.md](file:///C:/Users/medha/.gemini/antigravity/scratch/AsterShell/docs/audits/M3_FINAL_MANUAL_ACCEPTANCE.md) were executed during this automated/read-only audit. Hardware-dependent and interactive scenarios (multi-monitor hot-plug, DPI scaling drag, Explorer restart `TaskbarCreated`, physical audio output switching, sleep/resume, and 30+ minute soak) remain `Not Tested` and require execution on the user's Windows 11 desktop.

---

## 5. Resource & Stability Evidence

- Short-duration release measurements (`scripts/measure_idle_resources.ps1`, 10s @ 1s intervals on Windows 11 Pro Build 26300):
  - `shell-host.exe`: `12.23 MB` RSS, `2.27 MB` private, `151 -> 151` handles, `18` threads, `0%` idle CPU.
  - `taskbar.exe`: `70.26 MB` RSS, `46.12 MB` private, `416 -> 416` handles, `33` threads, `0%` idle CPU.
  - 3-cycle start/stop: `0` orphaned `taskbar.exe` processes after `shell-host.exe` termination.
- **Limitation:** No 30+ minute soak test has been run yet; long-term handle/memory stability is not yet empirically proven.

---

## 6. Findings Summary

- **P0 (Critical):** 0
- **P1 (High):** 0
- **P2 (Medium):** 1
  - **`F-P2-01`** (`apps/taskbar/src/main.rs` lines 651–665): `ToggleTaskbarNotice` toggles `main_window.show()`/`hide()` only; in `MonitorTargetMode::All`, `EXTRA_SURFACES` are not toggled, and `apply_taskbar_native_styles` is not re-applied upon `win.show()`. *Reproduction:* Configure `monitor = "all"`, click tray `Toggle Taskbar` twice. *Impact:* Secondary monitor surfaces stay visible while primary hides. *Blocks PR #2 merge?* No (non-blocking for default `"primary"` mode; recommended follow-up).
- **P3 (Low):** 3
  - **`F-P3-01`** (`crates/platform-windows/src/tray.rs`): `TaskbarCreated` Explorer-restart recovery is verified by code inspection only (no automated Explorer-kill test). *Blocks merge?* No.
  - **`F-P3-02`** (`crates/platform-windows/src/{audio,power}.rs`): Sleep/resume and physical audio endpoint switching require manual desktop validation. *Blocks merge?* No.
  - **`F-P3-03`** (`apps/shell-host/src/main.rs` lines 258–263): `launch_spotlight_search` spawns `search-ui.exe` outside `ProcessSupervisor` / `JobObject`, so an open Spotlight window persists if `shell-host` exits. *Blocks merge?* No.

---

## 7. Documentation Review

Verified `README.md`, `ARCHITECTURE.md`, `ROADMAP.md`, `docs/guides/{BUILD_AND_RUN,TROUBLESHOOTING,MANUAL_VALIDATION_CHECKLIST,LIMITATIONS}.md`, and `docs/audits/*`. All documents accurately describe companion mode, zero AppBar/work-area mutation, Explorer ownership of third-party tray icons, multi-monitor `"all"` behavior, search integration, and current limitations.

---

## 8. Remaining Risks & Final Verdict

- **Remaining Risks:** Untested multi-hour desktop soak, physical multi-monitor hot-plug, Explorer restart recovery, sleep/resume clock/power transitions, and `F-P2-01` multi-surface toggle behavior.
- **Final Verdict:** **`CONDITIONAL APPROVAL — MANUAL VALIDATION REQUIRED`**
  - Code inspection, security/boundary invariants (zero `SHAppBarMessage`, zero `SPI_SETWORKAREA`, zero `AttachThreadInput`), and all 140 automated tests + Windows CI (`46105955936`) pass on exact HEAD `b143bd7811a7b88215bdaa2d4232533f0d5836f5`. Merge of PR #2 is conditioned on completing the manual desktop acceptance checklist in [M3_FINAL_MANUAL_ACCEPTANCE.md](file:///C:/Users/medha/.gemini/antigravity/scratch/AsterShell/docs/audits/M3_FINAL_MANUAL_ACCEPTANCE.md).

---

## 9. Post-Acceptance Remediation Record (`F-P2-01` & `F-P3-03`)

### 9.1 `F-P2-01` — Multi-Surface Toggle & Native Style Restoration (`RESOLVED`)
- **Original Finding:** `ToggleTaskbarNotice` toggled `main_window.show()`/`hide()` only, omitting `EXTRA_SURFACES` in `MonitorTargetMode::All` and omitting native Win32 style re-application on restore.
- **Root Cause:** Visibility was tracked via an isolated `AtomicBool` (`is_visible`) on `main_window` only; `reconcile_extra_surfaces` unconditionally called `extra_win.show()` for newly added monitors even while hidden, and `toggle_cb` did not invoke `apply_taskbar_native_styles`.
- **Files Changed:**
  - `apps/taskbar/src/main.rs`
  - `apps/taskbar/tests/taskbar_foundation.rs`
- **Fix Details:**
  - Added authoritative `TaskbarVisibilityState` (`visible: bool`, `surfaces: Vec<String>`) with `toggle()`, `reconcile()`, `mark_destroyed()`, and `surface_visibility_targets()`.
  - Added `apply_visibility_to_all_surfaces(...)` synchronizing `main_window` and every surface in `EXTRA_SURFACES`, and reapplying `apply_taskbar_native_styles(win, target, false, None)` on restore without recreating HWNDs or duplicating subclasses.
  - Updated `reconcile_extra_surfaces` so newly attached monitors respect `TaskbarVisibilityState.visible` (staying hidden when toggled off), and guarded `toggle_cb` during shutdown (`!is_running`).
- **Regression Tests Added:**
  - Unit tests (`apps/taskbar/src/main.rs`): `test_visibility_single_monitor_and_repeated_toggles`, `test_visibility_all_monitor_sync_and_topology_while_hidden`, `test_visibility_missing_or_destroyed_surface_handling`.
  - Win32 integration test (`apps/taskbar/tests/taskbar_foundation.rs`): `test_f_p2_01_win32_hide_restore_preserves_native_styles` (`ShowWindow(SW_HIDE)` -> `ShowWindow(SW_SHOWNA)` -> `configure_taskbar_native_styles` -> `get_window_styles` readback).
- **Verification Evidence:** `cargo fmt --all -- --check` (exit `0`), `cargo check --workspace --all-targets --locked` (exit `0`), `cargo clippy --workspace --all-targets --locked -- -D warnings` (exit `0`), `cargo test --workspace --locked -j 2` (exit `0`, **144 passed, 0 failed, 0 ignored**).
- **Final Status:** **`RESOLVED`** (0 P0, 0 P1, 0 P2, 3 P3 remaining).

### 9.2 `F-P3-03` — `launch_spotlight_search` Lifetime Investigation (`DOCUMENTED — INTENTIONAL DESIGN`)
- **Finding:** `launch_spotlight_search()` in `apps/shell-host/src/main.rs` spawns `search-ui.exe` via `std::process::Command::new(&exe).spawn()` outside `ProcessSupervisor` / `JobObject`.
- **Investigation Outcome:**
  - `ProcessSupervisor` is designed for persistent background daemons (`ComponentId::Taskbar`) with automatic crash-restart policies and circuit breakers.
  - `search-ui.exe` is an on-demand interactive overlay (also launched directly via `Alt+Space` or taskbar search pill) that exits or hides on `Esc`/blur. Registering it in `ProcessSupervisor` would trigger unintended restart loops whenever the user closes Spotlight.
  - **Limitation:** Because `search-ui.exe` is not bound to `shell-host`'s `JobObject`, an open `search-ui.exe` window persists if `shell-host.exe` exits, and `search-ui.exe` crashes are not auto-restarted by `shell-host`. This independent lifetime is intentional and remains P3 (non-blocking).
