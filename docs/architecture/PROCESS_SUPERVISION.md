# AsterShell — Process Supervision & Lifecycle Management

This document defines the process ownership model, supervisor lifecycle, startup sequence, health monitoring, and crash recovery semantics implemented in `process-supervisor` and orchestrated by `shell-host`.

---

## 1. Process Hierarchy & Ownership

```
                        [ Windows NT Subsystem ]
                                   |
                                   v
                         +-------------------+
                         |    shell-host     | <--- Win32 Job Object Owner
                         +---------+---------+
                                   |
           +-----------------------+-----------------------+
           |                       |                       |
           v                       v                       v
     +-----------+           +-----------+           +-----------+
     |  taskbar  |           | search-ui |           |start-menu |
     +-----------+           +-----------+           +-----------+
```

### Process Ownership Guarantees:
- **`shell-host`** is the root supervisor process. It creates and owns the Win32 Job Object `AsterShellSessionJob`.
- Every child process (`taskbar`, `search-ui`, `start-menu`, `desktop`) is spawned using `CreateProcessW` with the flag `CREATE_SUSPENDED`, assigned to the Job Object via `AssignProcessToJobObject`, and then resumed.
- **Kernel-Enforced Cleanup:** `JOBOBJECT_EXTENDED_LIMIT_INFORMATION` is configured with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`. If `shell-host` terminates for any reason (clean exit, crash, or `taskkill`), the Windows kernel automatically terminates all child processes in the Job Object, eliminating zombie shell windows.

---

## 2. Startup Sequence & Dependency Ordering

To prevent race conditions, IPC deadlocks, and visual flickering during startup, processes follow a strict staged startup sequence:

```
[Phase 1: Supervisor Core]
  1. shell-host starts.
  2. Initialize logging (tracing-subscriber) and diagnostics.
  3. Load configuration (configuration crate).
  4. Create Named Pipe IPC server (ipc crate).
  5. Initialize background workers (WSearch connection pool, application catalog index).

[Phase 2: Platform Registration]
  6. Register global hotkeys (Alt + Space for search, Win key hooks if full shell mode).
  7. Start window-tracker WinEvent hook listener.

[Phase 3: Surface Initialization]
  8. Spawn desktop process (renders wallpaper and base desktop layer).
  9. Wait for IPC handshake from desktop (timeout: 2,000 ms).

[Phase 4: Shell Controls]
  10. Spawn taskbar process.
  11. Taskbar registers system AppBar (SHAppBarMessage ABM_NEW) and docks.
  12. Taskbar connects to IPC and receives initial window list.

[Phase 5: Overlay Pre-Warming]
  13. Spawn search-ui in hidden mode (pre-warmed for instant sub-50ms activation on Alt+Space).
  14. Spawn start-menu in hidden mode.
```

---

## 3. Crash Detection & Restart Policy

### Health Monitoring Mechanisms
1. **Windows Wait Handles:** `shell-host` registers process handles with `RegisterWaitForSingleObject` or polls them via an asynchronous `tokio` event loop using `tokio::process::Child`.
2. **IPC Heartbeat:** Every child process sends a lightweight ping frame over the IPC pipe every 5 seconds. If no frame is received for 15 seconds, the child is flagged as unresponsive.

### Restart State Machine & Policy
```
  +------------+       Crash Event        +---------------+
  |  RUNNING   | -----------------------> | EVALUATING    |
  +------------+                          +-------+-------+
        ^                                         |
        | Restart Success                         | Under limit (< 3 crashes / 10s)
        +-----------------------------------------+
        |                                         |
        | Backoff Elapsed                         | Over limit (>= 3 crashes / 10s)
        |                                         v
  +-----+------+      Backoff Delay       +---------------+
  | RESTARTING | <----------------------- | BACKOFF_PAUSED|
  +------------+                          +-------+-------+
                                                  |
                                                  | >= 5 crashes / 60s
                                                  v
                                          +---------------+
                                          | CIRCUIT_BROKE |
                                          +---------------+
```

- **Transient Failure:** Re-spawned immediately.
- **Backoff Schedule:** 1s, 2s, 4s, 8s, up to 30s max delay.
- **Circuit Breaker:** If 5 crashes occur within 60 seconds, the child is halted and an alert is recorded in the diagnostics log. The user can manually restart it from the settings tray menu.

---

## 4. Graceful Shutdown & Teardown (M3A.2 Verified)

1. **Broadcast Shutdown Notice:** `shell-host` sends an `IpcResponse::ShutdownNotice` frame across all active authenticated named-pipe client connections and terminates listener acceptance.
2. **Child Voluntary Teardown:**
   - `taskbar` operates as a floating companion (zero work area modification) and cleanly terminates its Slint UI event loop and winit window upon receiving `ShutdownNotice` or detecting pipe EOF.
   - `desktop` closes rendering surfaces and unhooks DWM backdrops.
   - `search-ui` closes local SQLite connection handles.
3. **Termination Grace Period:**
   - `shell-host` allows a bounded grace period (1,500 ms) for supervised child processes to exit voluntarily.
   - Any child process remaining alive after the timeout expires is forcefully terminated via `child.start_kill()`.
4. **Kernel Job Object Containment:**
   - If the supervisor process itself crashes, is killed, or drops its handles, the Windows kernel terminates all enclosed child processes automatically via `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
   - Verified through authoritative `QueryInformationJobObject` and `IsProcessInJob` Win32 tests without manual `TerminateProcess` intervention.
