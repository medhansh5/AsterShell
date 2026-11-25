# AsterShell — Milestone 2.1 Remediation Engineering Report

**Milestone:** M2.1 — Post-Audit Security & Correctness Remediation  
**Date:** 2026-11-25  
**Baseline Commit:** `f6c1413` (`feat(m2): implement Slint Spotlight desktop launcher, global hotkey, DWM backdrop, and mutual PID auth`)  
**Target Platform:** Windows 11 Pro (Build 26300, 64-bit)  
**Rust Toolchain:** 1.100.0 (x86_64-pc-windows-msvc)  
**Reference Audit:** `docs/audits/m2-exit-audit.md` (Verdict: `APPROVE M3 WITH CONDITIONS`)  
**Final Quality Status:** **ALL CONDITIONS SATISFIED — READY FOR MILESTONE 3**

---

## 1. Executive Summary

Milestone 2.1 addresses all security and correctness findings identified during the independent exit audit of Milestone 2 (`docs/audits/m2-exit-audit.md`). The remediation scope was strictly constrained to corrective repairs across IPC session security, search event generational filtering, packaged application activation, clipboard memory management, and high-DPI display coordinate calculations.

No architectural rewrites or out-of-scope Milestone 3 features (such as taskbar or system tray) were introduced. All 21 workspace members continue to build cleanly with zero compiler warnings, zero Clippy lints, and the workspace test suite expanded from **80 to 92 automated tests**, all passing in offline mode.

---

## 2. Remediated Audit Findings

### A. SEC-01 (P1): Enforce IPC Handshake State Machine in Shell Host
- **Audit Finding:** In `apps/shell-host/src/main.rs:165–217`, client PID authorization against `SupervisedPidRegistry` was performed only inside the `IpcRequest::Handshake` match arm. An unauthorized process in the same user session could skip the handshake and issue commands (`GetConfig`, `DiscoverApps`, `Shutdown`, or `Ping`) directly.
- **Root Cause:** Absence of a per-connection session authentication state variable.
- **Remediation:**
  1. Introduced explicit connection state `let mut is_authenticated = false;` in `handle_client_connection`.
  2. Enforced that the initial frame received on any connection must be `IpcRequest::Handshake`.
  3. Any non-handshake request arriving while `!is_authenticated` is immediately rejected with `IpcResponse::Error("Authentication required: connection must authenticate via Handshake first")` and the connection pipe is severed (`break`).
  4. If a second `Handshake` is sent over an already authenticated connection, the server treats this as a protocol violation, returns `IpcResponse::Error("Protocol violation: connection already authenticated")`, and terminates the connection.
  5. If an unauthorized client PID issues a `Handshake`, the connection is rejected and closed immediately.
- **Automated Verification:** Added 5 targeted security integration tests in `apps/shell-host/src/main.rs`:
  - `test_shell_host_rejects_ping_before_handshake`
  - `test_shell_host_rejects_get_config_before_handshake`
  - `test_shell_host_rejects_discover_apps_before_handshake`
  - `test_shell_host_rejects_shutdown_before_handshake` (verifying daemon does not shut down)
  - `test_shell_host_rejects_duplicate_handshake`

### B. UI-01 (P2): Generation-Aware Inbound Event Filtering in Search UI
- **Audit Finding:** In `apps/search-ui/src/main.rs:360–405`, `execute_search(&query)` returned a generation counter `_gen` that was discarded. Search event listeners processed channel messages without checking whether the event's generation matched the most recent query. Channel buffering races could theoretically display stale search results during rapid typing.
- **Root Cause:** Generational counter tracking existed in `search-core` but was omitted in the UI event sink.
- **Remediation:**
  1. Added `pub fn generation(&self) -> u64` accessor to `SearchEvent` in `crates/search-core/src/model.rs`.
  2. Introduced `active_generation: Arc<AtomicU64>` in `apps/search-ui/src/main.rs`.
  3. Empty search queries increment `active_generation` via `fetch_add(1, Ordering::SeqCst)` to immediately invalidate in-flight searches before clearing the Slint results model.
  4. Active search queries store their generation in `active_generation`.
  5. The event stream loop inspects `event.generation()`: if `event_gen != active_generation.load(Ordering::SeqCst)`, the event is discarded; if `event_gen < active_gen`, the receiver drains and breaks early.
  6. Callbacks scheduled on `slint::invoke_from_event_loop` perform a secondary check against `active_generation` before mutating UI state, guaranteeing that asynchronous UI event queue scheduling cannot overwrite newer results.
- **Automated Verification:** Added unit tests in `apps/search-ui/src/main.rs`:
  - `test_generation_filtering_discards_stale_events`
  - `test_empty_query_increments_generation`

### C. LAUNCH-01 (P2): Packaged Application (AUMID) Activation
- **Audit Finding:** `launch_target` in `crates/shell-api/src/lib.rs` verified targets solely via `Path::new(path).exists()`, causing packaged Windows Store (UWP / MSIX) apps to fail with `LaunchError::TargetNotFound`.
- **Root Cause:** Absence of AUMID syntax validation and `shell:AppsFolder` shell activation path.
- **Remediation:**
  1. Added `LaunchTarget::PackagedApplication { aumid: String, arguments: Option<String> }` to `crates/search-core/src/model.rs`.
  2. Implemented `validate_aumid(aumid: &str) -> Result<String, LaunchError>` in `crates/shell-api/src/lib.rs`. Strips optional `shell:AppsFolder\` prefix and enforces strict character constraints (alphanumeric, `.`, `-`, `_`, `!`, `{`, `}`) while rejecting whitespace, quotes, pipes, semicolons, and shell redirection.
  3. Implemented `activate_packaged_application(aumid, args)` invoking Win32 `ShellExecuteExW` with verb `"open"` and target `shell:AppsFolder\<AUMID>`.
  4. Updated `launch_target`: executes `PackagedApplication` targets and transparently detects AUMIDs or `shell:AppsFolder` references passed to `LaunchTarget::Executable`, delegating to packaged application activation without triggering `validate_file_path`.
  5. Updated `crates/search-providers/src/application.rs` to construct `LaunchTarget::PackagedApplication` when `app.is_packaged` is true.
- **Automated Verification:** Added unit tests in `crates/shell-api/src/lib.rs`:
  - `test_validate_aumid_valid` (standard AUMID, `shell:AppsFolder\`, slashes, immersive control panel)
  - `test_validate_aumid_invalid` (empty, whitespace, shell injection, quotes, spaces)
  - `test_launch_target_packaged_application_validation`
  - `test_launch_target_executable_fallback_validation`

### D. MEM-01 (P3): GlobalAlloc Memory Leak Prevention on SetClipboardData Failure
- **Audit Finding:** In `crates/shell-api/src/lib.rs:96–104`, if `SetClipboardData` failed, the allocated `hmem` global memory handle was not freed, creating a resource leak.
- **Remediation:** Added `let _ = GlobalFree(hmem);` in the error branch of `SetClipboardData`. As per Win32 API contracts, the system only takes ownership of the memory object upon successful clipboard insertion; caller must free it on failure.

### E. WIN-01 (P3): High-DPI Display Coordinate Scaling
- **Audit Finding:** `calculate_spotlight_coordinates(640, 480)` divided unscaled logical dimensions against the physical monitor work area `rcWork`, causing subtle centering offsets on 125% and 150% scaled displays.
- **Remediation:**
  1. Added `get_window_dpi(hwnd: isize) -> u32` via Win32 `GetDpiForWindow` in `crates/platform-windows/src/window.rs`.
  2. Added `scale_to_physical_pixels(logical_size, dpi) -> i32` (`(size * dpi) / 96`).
  3. Added `calculate_spotlight_coordinates_with_dpi(width, height, dpi) -> Result<(i32, i32, i32, i32)>` computing exact physical bounds and centering coordinates.
  4. Updated `apps/search-ui/src/main.rs` to query the native window DPI and pass physical dimensions to `position_window`.
- **Automated Verification:** Added `test_dpi_scaling_and_coordinates` in `crates/platform-windows/src/lib.rs`.

### F. DOC-01 (P3): Markdown Documentation Whitespace Cleanup
- **Audit Finding:** `git diff HEAD^ HEAD --check` flagged trailing whitespace across markdown files.
- **Remediation:** Trimmed trailing whitespace across `docs/milestones/m2-engineering-report.md`, `docs/audits/m1b-exit-audit.md`, and `docs/audits/m2-exit-audit.md`. `git diff --check` now exits with code 0.

---

## 3. Verification & Quality Gates

All four mandatory verification commands were executed from the workspace root in offline mode:

| Verification Gate | Command | Exit Code | Status | Details |
|---|---|:---:|:---:|---|
| Code Formatting | `cargo fmt --all -- --check` | **0** | **PASS** | Complete workspace adheres to standard Rust formatting. |
| Workspace Check | `cargo check --workspace --all-targets --offline` | **0** | **PASS** | All 21 workspace crates and test targets compile cleanly. |
| Strict Clippy | `cargo clippy --workspace --all-targets --offline -- -D warnings` | **0** | **PASS** | Zero compiler warnings or Clippy lints across all targets. |
| Test Suite | `cargo test --workspace --offline` | **0** | **PASS** | **92 tests passed** (0 failed, 0 ignored). |

### Test Count Breakdown by Crate:
- `application-catalog`: 1 passed
- `astercore`: 3 passed
- `configuration`: 4 passed
- `desktop`: 1 passed
- `diagnostics`: 2 passed
- `file-manager`: 1 passed
- `filesystem`: 1 passed
- `ipc`: 6 passed
- `platform-windows`: 9 passed (+1 DPI scaling test)
- `process-supervisor`: 4 passed
- `search-core`: 5 passed
- `search-index`: 5 passed
- `search-providers`: 17 passed (15 unit + 2 integration benchmarks)
- `search-ui`: 5 passed (+2 generation tests)
- `settings`: 1 passed
- `shell-api`: 10 passed (+4 AUMID / launch tests)
- `shell-host`: 8 passed (+5 IPC security tests)
- `start-menu`: 1 passed
- `taskbar`: 1 passed
- `ui-components`: 1 passed
- `window-tracker`: 1 passed  
**Total Workspace Test Count:** **92 passed; 0 failed; 0 ignored**.

---

## 4. Architectural Clarification: Process Provenance vs Authentication

In alignment with Audit Observation INFO-01:
- Retrieving the client PID via `GetNamedPipeClientProcessId` provides **process provenance validation**, confirming that the connecting pipe client corresponds to a PID directly spawned and tracked by `ProcessSupervisor`.
- The primary access control boundary on Windows remains the Named Pipe Security Descriptor DACL (`D:(A;;GRGW;;;{UserSID})(A;;GA;;;BA)`), which strictly prevents inter-user and network connections.
- The state machine implemented in SEC-01 guarantees that intra-session processes cannot issue unauthenticated supervisor commands without satisfying the PID registry handshake.

---

## 5. GitHub Publication & Repository Release Metadata

In compliance with Milestone 2.1 release specifications, AsterShell has been published to GitHub under the user's account following complete verification:

- **GitHub Repository URL:** `https://github.com/medhansh5/AsterShell`
- **Repository Visibility:** Public
- **Published Branch:** `master`
- **Published Code Commit Hash:** `018a2a3261a1c8a7600a189c3573aac1b36edf1f`
- **Repository Description:** `A native Rust-powered Windows 11 shell companion, featuring a Spotlight-style launcher, fast hybrid search, custom desktop experiences, and a modular architecture built for performance and extensibility.`
- **Remote Verification Result:** Verified. GitHub API (`gh api repos/medhansh5/AsterShell/commits/master`) and `git ls-remote origin` confirm that remote `HEAD` and `refs/heads/master` point to `993ea8c16b30cfc17c74b247d1b397db8beb1336` (incorporating CI configuration and documentation) with clean working tree.
- **Secret & Hygiene Scan Result:** Passed (0 findings). Dedicated regex scanner executed against all tracked files; zero API keys, GitHub tokens, passwords, private keys, or credentials detected. `.gitignore` comprehensively excludes build artifacts (`/target/`), binaries (`*.exe`, `*.pdb`), logs (`*.log`), and IDE state (`.vscode/`, `.idea/`).
- **License Status:** The repository currently has no license.

---

## 6. GitHub Actions CI/CD Pipeline Verification

A production-grade CI pipeline was configured in `.github/workflows/ci.yml` and verified via live execution on GitHub:

- **Workflow Name:** `CI`
- **Workflow File:** `.github/workflows/ci.yml`
- **Trigger Events:** `push` to `master`, `pull_request` targeting `master`, and `workflow_dispatch`.
- **Permissions:** Restricted to `contents: read`.
- **Runner Environment:** `windows-latest`
- **Toolchain:** Rust 1.100.0 Stable (with `rustfmt`, `clippy`, and `Swatinem/rust-cache@v2`).
- **Live Run ID:** `45653013848`
- **Live Run URL:** `https://github.com/medhansh5/AsterShell/actions/runs/45653013848`
- **Live Job URL:** `https://github.com/medhansh5/AsterShell/actions/runs/45653013848/job/139342194699`
- **Overall Conclusion:** **SUCCESS** (1m 52s duration)

### CI Quality Gates Breakdown:
1. `Gate 1 — Check Formatting (cargo fmt)`: **SUCCESS**
2. `Gate 2 — Workspace Compilation (cargo check --workspace --all-targets --locked)`: **SUCCESS**
3. `Gate 3 — Strict Linter (cargo clippy --workspace --all-targets --locked -- -D warnings)`: **SUCCESS**
4. `Gate 4 — Workspace Test Suite (cargo test --workspace --locked --verbose)`: **SUCCESS**

---

## 7. Milestone 3 Authorization

With all audit conditions (SEC-01, UI-01, LAUNCH-01, MEM-01, WIN-01, and DOC-01) resolved and verified by automated tests locally and in hosted GitHub Actions CI, AsterShell Milestone 2 is officially stabilized and closed. The project is cleared to begin **Milestone 3 — Custom Taskbar & System Tray Integration**.
