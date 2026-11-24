# AsterShell — Reverse Engineering Evidence Ledger

This ledger records every piece of empirical evidence collected during the Phase 0 investigation on Microsoft Windows 11 Build 26300.

---

## Evidence Ledger Table

| Evidence ID | Category | Target / Component | Command / Inspection Performed | Observed Outcome / Metrics | Classification | Architectural Impact |
| :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| **EV-001** | Tooling | Toolchain Verification | `rustc --version; cargo --version; git --version` | Rust 1.100.0, Cargo 1.100.0, Git 2.56.0 installed. | **[FACT]** | Development toolchain is modern and ready for Cargo workspace compilation. |
| **EV-002** | Skill | REA Integration | `Get-ChildItem ~\AppData\...\reverse-engineer-anything` | REA v63 skill markdown and reference guides present. No local binary engines or active MCP. | **[FACT]** | Investigation properly falls back to native Windows SDK, dynamic inspection, and static analysis. |
| **EV-003** | System | OS Version & Session | `[Environment]::OSVersion; Get-CimInstance Win32_OperatingSystem` | Windows 11 Pro, Build 26300, 64-bit. Interactive Session ID 1 (`WinSta0\Default`). | **[FACT]** | Confirms target is modern Windows 11 24H2 / Canary branch with decoupled shell. |
| **EV-004** | Process | Shell Subsystem | `Get-Process explorer, SearchHost, StartMenuExperienceHost, ShellExperienceHost` | Found distinct active PIDs: `explorer.exe` (11320), `SearchHost.exe` (13204), `StartMenuExperienceHost.exe` (13240). | **[FACT]** | Proves Windows 11 shell is multi-process. Confirms AsterShell's multi-process supervisor design is sound. |
| **EV-005** | Modules | Explorer Architecture | `(Get-Process explorer).Modules \| Where-Object ModuleName -match "taskbar\|xaml\|shell"` | Loaded `Taskbar.dll`, `Taskbar.View.dll`, `SystemTray.dll`, `SearchUx.UI.dll`, `Microsoft.UI.Xaml.dll`. | **[FACT]** | Proves modern taskbar UI is loaded from `MicrosoftWindows.Client.Core` AppX package, not legacy `shell32.dll` controls. |
| **EV-006** | Search | Windows Search Indexer | Query `Search.CollatorDSO` via ADODB COM object | Cold query: 1,510 ms. Warm queries: 9, 8, 7, 7, 6, 5, 7, 7, 7 ms (avg 7.2 ms). | **[FACT]** | Validates hybrid search feasibility: warm queries achieve ~7 ms, easily satisfying p95 < 100 ms target. |
| **EV-007** | Catalog | Application Discovery | Enumerate `shell:AppsFolder` via `Shell.Application` COM | Discovered 137 applications across Win32 executables, AUMID packages, and PWAs. | **[FACT]** | Validates `shell:AppsFolder` as unified data source for AsterShell's `application-catalog`. |
| **EV-008** | Registry | Winlogon Shell Configuration | Inspect `HKLM` and `HKCU` Winlogon `Shell` keys | `HKLM` has `explorer.exe`. `HKCU` is absent/unconfigured. | **[FACT]** | Confirms per-user shell override in `HKCU` is available for safe, non-elevated full shell replacement testing. |
| **EV-009** | Remote | GitHub Repository Check | `gh repo view AsterShell` | Repo `medhansh5/AsterShell` does not exist on remote. Authentication valid. | **[FACT]** | Clean slate confirmed. No risk of accidental repository overwrites. |
