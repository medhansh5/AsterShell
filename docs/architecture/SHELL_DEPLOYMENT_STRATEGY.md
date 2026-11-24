# AsterShell — Deployment Strategies & Fallback Recovery Workflow

AsterShell is engineered with a dual-deployment model designed to support both casual daily productivity and complete desktop transformation, with total user control and zero risk of desktop lockouts.

---

## 1. Strategy A: Companion Mode (Primary & Default)

In Companion Mode, Windows Explorer remains the primary OS shell, while AsterShell operates as an advanced companion suite.

```
+-----------------------------------------------------------------------------+
|                               COMPANION MODE                                |
+-----------------------------------------------------------------------------+
|                                                                             |
|   +------------------------------------+   +----------------------------+   |
|   |         EXPLORER.EXE (Active)      |   |   ASTERSHELL (Active)      |   |
|   |  - Owns Progman & Shell_TrayWnd    |   |  - shell-host (Supervisor) |   |
|   |  - System tray & legacy apps       |   |  - search-ui (Alt + Space) |   |
|   |  - File Explorer window manager    |   |  - taskbar (Optional dock) |   |
|   +------------------------------------+   +----------------------------+   |
|                                                                             |
+-----------------------------------------------------------------------------+
```

### Characteristics:
- **Zero System Modifications:** No registry keys, system files, or group policies are modified.
- **User-Controlled Startup:** Launchable manually via executable or added to standard user startup (`shell:startup`).
- **Seamless Coexistence:**
  - `search-ui` activates over any active window via `Alt + Space`.
  - If AsterShell `taskbar` is enabled, it can auto-hide or dock at the top/sides while Explorer's taskbar remains at the bottom, or Explorer's taskbar can be set to auto-hide via standard settings.
- **Trivial Teardown:** Terminating AsterShell or closing `shell-host` completely removes all AsterShell windows in < 100 ms with zero OS side effects.

---

## 2. Strategy B: Full Shell Mode (Future Milestone)

In Full Shell Mode, AsterShell entirely replaces `explorer.exe` as the default user desktop shell upon login.

```
+-----------------------------------------------------------------------------+
|                           FULL SHELL REPLACEMENT                            |
+-----------------------------------------------------------------------------+
|                                                                             |
|   +---------------------------------------------------------------------+   |
|   |                       Winlogon (User Login)                         |   |
|   +----------------------------------+----------------------------------+   |
|                                      | Spawns User Shell                    |
|                                      v                                      |
|   +---------------------------------------------------------------------+   |
|   |                       ASTERSHELL: shell-host                        |   |
|   |  - Registers Desktop Window (SetShellWindow)                        |   |
|   |  - Registers Primary AppBar (SHAppBarMessage)                       |   |
|   |  - Spawns taskbar, desktop, search-ui, start-menu                   |   |
|   +---------------------------------------------------------------------+   |
|                                                                             |
|   (explorer.exe is NOT running in background)                               |
|                                                                             |
+-----------------------------------------------------------------------------+
```

### Investigation of Windows Shell Replacement Mechanisms:

| Mechanism | Registry / Interface | SKU Support | Administrator Required | Assessment for AsterShell |
| :--- | :--- | :--- | :---: | :--- |
| **Per-User Winlogon Shell** | `HKCU\Software\Microsoft\Windows NT\CurrentVersion\Winlogon\Shell` | All SKUs (Home, Pro, Enterprise, Education) | **No** | **Recommended.** Safest, per-user, easily reverted, does not require elevation or affect other user profiles. |
| **System-Wide Winlogon Shell** | `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon\Shell` | All SKUs | **Yes** | **Not Recommended.** Affects all accounts including administrator login; high risk if shell crashes. |
| **Microsoft Shell Launcher v1/v2** | WMI `root\standardcimv2\embedded` (`WESL_UserSetting`) | Enterprise & IoT only (Pro is conditional; Home is unsupported) | **Yes** | **Rejected.** Insufficient SKU coverage; cannot be deployed on Windows Home or standard Pro installs. |

---

## 3. Verified Explorer Fallback & Emergency Recovery Workflow

To guarantee that a developer or user can never be stranded without a functional desktop shell:

### Scenario 1: Clean Fallback via Keyboard Shortcut
- In full shell mode, AsterShell registers an emergency hotkey (`Ctrl + Alt + Shift + E`).
- When invoked:
  1. `shell-host` launches `explorer.exe` with standard arguments.
  2. `shell-host` cleans up its own AppBar registrations.
  3. `shell-host` exits cleanly, handing complete desktop ownership back to Explorer.

### Scenario 2: Unhandled Shell Host Crash / Watchdog Recovery
- If `shell-host` encounters an unhandled panic in Full Shell Mode, an external independent recovery watchdog process (`astershell-watchdog.exe`) detects the termination.
- If `shell-host` fails to re-initialize within 5 seconds, the watchdog immediately spawns `explorer.exe` to restore normal Windows desktop functionality.

### Scenario 3: Manual Task Manager Recovery
If the screen is blank:
1. User presses `Ctrl + Shift + Esc` (hardcoded kernel interrupt to launch Windows Task Manager).
2. Select **Run new task**.
3. Type `explorer.exe` and press Enter. Standard Windows Shell launches instantly.

### Scenario 4: Automated Recovery Script
An emergency script `emergency-restore-explorer.bat` is installed alongside the application:
```cmd
@echo off
echo Restoring default Windows Explorer shell...
reg delete "HKCU\Software\Microsoft\Windows NT\CurrentVersion\Winlogon" /v Shell /f
taskkill /f /im astershell* >nul 2>&1
start explorer.exe
echo Explorer successfully restored.
```

---

## 4. Phase 0 Commitment

> [!IMPORTANT]
> **Zero Shell Modification Guarantee:** During Phase 0, no registry keys in `HKLM` or `HKCU` are touched, and `explorer.exe` is never terminated or replaced. All testing and prototyping is strictly restricted to Companion Mode. Full Shell Mode integration will only be implemented after explicit architectural review and milestone approval.
