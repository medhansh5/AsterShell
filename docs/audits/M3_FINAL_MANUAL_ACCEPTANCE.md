# AsterShell M3 — Final Manual Windows 11 Acceptance Checklist

**Audited Commit SHA:** `77d5df87f7db70b5798165bead19104abc88165f`  
**Mode:** Independent Companion Mode (`shell-host.exe` + `taskbar.exe` + `search-ui.exe`)

## Test Environment
- **OS:** Microsoft Windows 11 Pro Build 26300 (64-bit)
- **CPU / RAM:** 12th Gen Intel(R) Core(TM) i5-12450H | 15.68 GB RAM
- **Displays (1):** `\\.\DISPLAY1` (`Primary=True`, `Bounds={X=0,Y=0,Width=1536,Height=864}`, `WorkingArea={X=0,Y=0,Width=1536,Height=816}`, 125% scaling)
- **Audio Devices:** Intel Smart Sound Bluetooth Audio, NVIDIA High Definition Audio, Realtek High Definition Audio, Intel Smart Sound Digital Microphones, NVIDIA Virtual Audio Device (`[OK]`)
- **Network:** `Mannu_5G` (Wi-Fi, `IPv4=Internet`, `IPv6=Internet`); `Tailscale` (`NoTraffic`)
- **Config:** `C:\Users\medha\AppData\Roaming\AsterShell\config.toml` (`Exists=True`)
- **30-Minute Soak Script:** [scripts/run_30min_soak.ps1](file:///C:/Users/medha/.gemini/antigravity/scratch/AsterShell/scripts/run_30min_soak.ps1)

---

| # | Test Item | Steps | Expected Result | Actual Result | Status | Notes / Evidence |
|---:|---|---|---|---|---|---|
| 1 | **Clean Boot / Live Launch** | Launch `target\release\shell-host.exe` on Windows 11. | `shell-host.exe` starts, spawns `taskbar.exe` under its Job Object, and registers tray icon with no errors. | `shell-host.exe` (`PID 21268` / `15508`) spawned `taskbar.exe` (`PID 21408` / `21000`); `FocusChanged=False`. | `Pass` | Boot-time login shortcut not tested; live release launch verified. |
| 2 | **Taskbar Appearance & Positioning** | Inspect `taskbar.exe` HWND, styles, and primary work area. | Visible floating companion window (`WS_POPUP`, `WS_EX_TOPMOST`, `WS_EX_TOOLWINDOW`, `WS_EX_NOACTIVATE`), work area preserved. | `AsterShell Taskbar` (`hwnd=0x330744`) `ex=0x08000098` (`TOPMOST=True`, `TOOL=True`, `NOACTIVATE=True`, `APPWIN=False`), `above_before=True` above Notepad; preserved after hide/restore (`exAfter=0x08000098`, `above_after=True`). | `Pass` | Live Test C verified real `TaskbarWindow` HWND topmost styles & Z-order. |
| 3 | **Alt+Space Search Activation** | Press `Alt+Space`, type `notepad` / `2+2`, press `Enter` / `Esc`. | Spotlight overlay appears centered without permanent focus steal; results <100ms; `Esc` dismisses. | _Requires interactive user input_ | `Not Tested` | |
| 4 | **Open & Close Multiple Apps** | Open 3+ applications (Notepad, Terminal, Edge) and close them. | Taskbar app strip dynamically adds and removes running app pills. | _Requires interactive user input_ | `Not Tested` | |
| 5 | **Running-Window Tracking** | Switch focus between windows and change window titles. | Focused window shows `#3b82f6` active pill; unfocused running windows show `#9ca3af` dot. | _Requires interactive user input_ | `Not Tested` | |
| 6 | **Minimize, Restore & Close Windows** | Left-click pills; right-click pill and test `Activate / Restore`, `Minimize`, `Close Window`, `Esc`. | Left-click toggles minimize/restore; right-click opens native context menu; `Esc` dismisses safely. | _Requires interactive user input_ | `Not Tested` | |
| 7 | **Tray Context-Menu Actions** | Right-click AsterShell tray icon and test `Open Search`, `Toggle Taskbar`, `Exit AsterShell`. | `Open Search` launches `search-ui.exe`; `Toggle Taskbar` hides/shows taskbar; `Exit AsterShell` exits cleanly. | _Requires interactive user input_ | `Not Tested` | |
| 8 | **Explorer Restart & Tray Recovery** | Restart `explorer.exe` in Task Manager while `shell-host.exe` is running. | AsterShell tray icon recovers via `TaskbarCreated`; third-party icons remain owned by Explorer. | _Requires user authorization (disruptive)_ | `Not Tested` | |
| 9 | **External Monitor Connect/Disconnect** | With `taskbar.monitor = "all"`, connect and disconnect an external monitor. | Secondary taskbar surface spawns and closes cleanly; primary remains visible. | _Only 1 monitor connected (`\\.\DISPLAY1`)_ | `Not Tested` | |
| 10 | **Display Scaling & Resolution Change** | Change display scaling (`100%` <-> `150%`) and resolution in Windows Settings. | Taskbar recalculates physical size/position via `WM_DISPLAYCHANGE` / `WM_DPICHANGED`. | _Requires user authorization (disruptive)_ | `Not Tested` | |
| 11 | **Virtual Monitor Arrangements (Left/Right/Above)** | Arrange secondary monitor left, right, and above primary. | Taskbar centers within each monitor's `rcWork` including negative coordinates. | _Only 1 monitor connected_ | `Not Tested` | |
| 12 | **Default Audio Output Switching** | Play audio, change volume/mute, switch default output device. | Volume/mute updates; `IMMNotificationClient` rebinds without polling. | _Requires user authorization / physical listening_ | `Not Tested` | |
| 13 | **Network Connectivity Transitions** | Disconnect Wi-Fi/Ethernet, then reconnect. | Network indicator transitions `Online` -> `Offline` -> `Online`. | _Requires user authorization (disruptive)_ | `Not Tested` | |
| 14 | **Suspend & Resume (Sleep)** | Put machine to Sleep for 2+ minutes, then wake. | Clock and indicators refresh cleanly; no frozen threads. | _Requires user authorization (disruptive)_ | `Not Tested` | |
| 15 | **Restart `shell-host` & Job Object Cleanup** | Terminate `shell-host.exe` and relaunch it. | Windows Job Object immediately terminates `taskbar.exe` (0 orphans); relaunch restores session. | `hostPid=9288` stopped -> `orphaned_taskbar_after_host_stop=0`; clean relaunch verified. | `Pass` | Live Test B verified. |
| 16 | **Terminate Supervised Child & Recovery** | Terminate `taskbar.exe` while `shell-host.exe` is running. | `ProcessSupervisor` detects exit and restarts `taskbar.exe` per `RestartPolicy`. | `hostPid=9288`, `tbPid1=13016` killed -> recovered `tbPid2=13420`, `tbCount=1`, `recovered=True`. | `Pass` | `F-P1-01` remediated & live Test A verified. |
| 17 | **Configuration Persistence After Restart** | Verify `%APPDATA%\AsterShell\config.toml` across `shell-host.exe` launches. | Config file preserved and loaded without corruption. | `Exists=True` and preserved across test launches. | `Pass` | Custom TOML pin-edit restart still recommended. |
| 18 | **Extended Soak (30+ Minutes)** | Run `.\scripts\run_30min_soak.ps1` for 30 minutes (0, 5, 10, 15, 20, 25, 30 min samples). | Idle CPU ~0%; RSS and handles remain bounded without monotonic growth; 0 orphans on exit. | _30-minute soak not yet elapsed_ | `Not Tested` | Script updated for restart tracking at `scripts/run_30min_soak.ps1`. |

---

## Repeatable Verification Procedures (Tests A, B, C & All-Monitor Mode)

- **Test A (Crash Recovery):** Start `.\target\release\shell-host.exe`, record `taskbar.exe` PID, run `Stop-Process -Id <tbPid> -Force`, wait 2s, and confirm a single new `taskbar.exe` process is running with a different PID.
- **Test B (Job Object Shutdown):** Stop `shell-host.exe` (`Stop-Process -Id <hostPid> -Force`), wait 1s, and confirm `@(Get-Process -Name taskbar -ErrorAction SilentlyContinue).Count -eq 0`.
- **Test C (Real `TaskbarWindow` HWND Topmost & Z-Order):** Start `.\target\release\taskbar.exe --no-supervisor`, locate the visible `"AsterShell Taskbar"` HWND, verify `GWL_EXSTYLE` has `WS_EX_TOPMOST` (`0x00000008`), `WS_EX_TOOLWINDOW` (`0x00000080`), `WS_EX_NOACTIVATE` (`0x08000000`), and `!WS_EX_APPWINDOW` (`0x00040000`), and verify Z-order above a newly opened Notepad window both before and after `ShowWindow(SW_HIDE)` / `ShowWindow(SW_SHOWNA)`.
- **All-Monitor Mode (`monitor = "all"`):** Set `monitor = "all"` under `[taskbar]` in `%APPDATA%\AsterShell\config.toml`, launch `shell-host.exe`, verify one `"AsterShell Taskbar"` HWND per active monitor with `WS_EX_TOPMOST`, and test `Toggle Taskbar` from the tray menu.
