# AsterShell — Reverse Engineering Investigation Plan

## 1. Executive Summary & Objective

The objective of this investigation is to deconstruct the runtime architecture, COM interfaces, windowing contracts, and undocumented integration points of the Windows 11 Shell subsystem (`explorer.exe`, `SearchHost.exe`, `StartMenuExperienceHost.exe`, and associated system packages). This empirical foundation ensures that AsterShell can implement a modern, high-performance, native Rust shell companion and optional full shell replacement without destabilizing the host operating system.

## 2. Tooling & Environment Verification

### 2.1 Target Environment
- **Operating System:** Microsoft Windows 11 Pro (Build 26300, 64-bit)
- **Host Session:** Interactive Desktop Session (Session ID 1, `WinSta0\Default`)
- **Runtime Toolchains:** Rust 1.100.0 (LLVM 19), Cargo 1.100.0, Git 2.56.0
- **Windows SDK Interfaces:** Rust `windows` and `windows-sys` crates

### 2.2 REA Skill Tooling Status & Fallback Strategy
- **Installed Skill:** `reverse-engineer-anything` (REA v63 by morluto) located at `~\AppData\Local\Programs\antigravity\.agents\skills\reverse-engineer-anything`.
- **Tooling Diagnostic:** The skill instructions specify that installing the skill supplies methodology guides but does not automatically bind an MCP server or inject binary decompilers into an active session. Attempts to invoke external CLI wrappers (`npx rea`) without active local decompilation engines (Ghidra, Hopper, IDA Pro) were properly diagnosed.
- **Investigation Strategy:** Per REA recovery protocol and client constraints, we employ legitimate static analysis of Windows SDK headers, runtime process/module inspection, COM type library enumeration, and empirical performance micro-benchmarking using native Windows subsystem APIs. No system binaries were modified, patched, or compromised.

---

## 3. Investigation Scope & Targets

The investigation is divided into 10 key technical domains:

| Domain | Target Component / Binary | Key Windows APIs & Interfaces |
| :--- | :--- | :--- |
| **1. Core Shell Process** | `explorer.exe` (Win32 Host) | `RegisterShellHookWindow`, `SetShellWindow`, `Shell_TrayWnd` |
| **2. Taskbar Subsystem** | `Taskbar.dll`, `Taskbar.View.dll` | `SHAppBarMessage` (`ABM_NEW`, `ABM_SETPOS`), DWM Thumbnails |
| **3. Start Menu Experience** | `StartMenuExperienceHost.exe` | AppX Package Activation, AUMID discovery, XAML Islands |
| **4. Windows Search Indexer** | `SearchHost.exe`, `WSearch` service | OLE DB `Search.CollatorDSO`, `ISearchManager`, `ISearchQueryHelper` |
| **5. Desktop & Shell Namespace** | `Progman`, `WorkerW`, `SHELL32.dll` | `IShellFolder`, `IShellItem`, `FOLDERID_Desktop`, `SHChangeNotify` |
| **6. Window Tracking** | `USER32.dll`, `DWMAPI.dll` | `SetWinEventHook`, `GetGUIThreadInfo`, `DwmRegisterThumbnail` |
| **7. Notification Area (Tray)** | `SystemTray.dll`, `Shell32.dll` | `Shell_NotifyIconGetRect`, undocumented `ITrayNotify` COM interface |
| **8. Shell COM Interfaces** | COM subsystem (`ole32.dll`) | `IApplicationActivationManager`, `ITaskbarList4`, `IShellLinkW` |
| **9. Application Discovery** | `shell:AppsFolder`, Registry | `FOLDERID_AppsFolder`, `IEnumIDList`, `App Paths` registry keys |
| **10. File Explorer Integration** | `explorer.exe`, `windows.storage.dll`| `IExplorerBrowser`, `IShellItemArray`, `IContextMenu` |

---

## 4. Methodology & Evidence Grading

Every finding in this investigation is cataloged under a strict tri-tier classification:
1. **[FACT] Observed Facts:** Verified directly via empirical runtime execution, API responses, process memory inspections, or official Microsoft documentation.
2. **[INFERENCE] Inferred Behavior:** Deduced from exported symbol tables, interface definitions, COM GUIDs, and architectural patterns.
3. **[HYPOTHESIS] Unverified Hypotheses:** Plausible architectural models requiring isolated sandbox verification or deep symbol tracing in future milestones.

---

## 5. Safety, Boundaries & Compliance

- **Zero-Modification Policy:** No system binaries, registry defaults, or running operating system components shall be modified during Phase 0.
- **Companion Mode Priority:** All initial prototypes must execute concurrently alongside `explorer.exe` without stealing system hooks or breaking existing shell windows.
- **Recovery Assurance:** Clear fallback procedures are established to guarantee instant return to standard Windows Explorer functionality if any anomalous behavior occurs.
