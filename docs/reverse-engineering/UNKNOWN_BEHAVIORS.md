# AsterShell — Unknown Behaviors, Risks & Unverified Hypotheses

This document records architectural uncertainties, undocumented OS behaviors, and open hypotheses that require isolated experimental validation prior to production commitment.

---

## 1. Undocumented Virtual Desktop Interfaces [HYPOTHESIS]

### Uncertainty & Context
The public COM interface `IVirtualDesktopManager` (`shobjidl.h`) is strictly read-only:
- `IsWindowOnCurrentVirtualDesktop(HWND, BOOL*)`
- `GetWindowDesktopId(HWND, GUID*)`
- `MoveWindowToDesktop(HWND, REFGUID)`

It provides **no public API** to enumerate all virtual desktops, query desktop names, create new desktops, or switch between desktops. In Windows 10 and Windows 11, third-party utilities rely on internal, undocumented COM interfaces:
- `IVirtualDesktopManagerInternal`
- `IVirtualDesktopNotificationService`

### Risk
Microsoft frequently changes the IID (Interface ID) and virtual method table layout of `IVirtualDesktopManagerInternal` between Windows builds. In Windows 11 24H2 / Build 26300, the GUID is known to diverge from earlier builds.

### Resolution & Mitigation Strategy
- In Phase 0 and Phase 1, AsterShell will not bind to brittle private GUIDs.
- AsterShell will support window workspace organization natively within its own process model and rely on public `IVirtualDesktopManager` for desktop affinity checks.
- If system-wide virtual desktop switching is requested in later phases, an isolated abstraction layer with runtime GUID detection and safe fallback will be deployed.

---

## 2. System Tray (Notification Area) Ownership in Full Shell Mode [UNKNOWN]

### Uncertainty & Context
In Windows, third-party applications place icons in the notification area by calling `Shell_NotifyIconW`, which formats a `WM_COPYDATA` message sent to a top-level window registered with the class name `Shell_TrayWnd`.
- In **Companion Mode**, `explorer.exe` owns `Shell_TrayWnd` and handles tray messages normally. AsterShell displays alongside it.
- In **Full Shell Mode** (where `explorer.exe` is terminated), AsterShell must create its own window of class `Shell_TrayWnd` and register internal tray messages.

### Risk
Certain system components and older Win32 tray utilities make undocumented assumptions about the thread ID or process token of `Shell_TrayWnd`, potentially rejecting registration or failing silently.

### Resolution & Mitigation Strategy
- Validate tray message reception in an isolated mock window test harness in Phase 2 before enabling full shell replacement.
- In companion mode, AsterShell avoids this risk entirely by leaving tray notification handling to the underlying OS.

---

## 3. Shell Launcher v1/v2 SKU Limitations [FACT / UNKNOWN]

### Uncertainty & Context
Microsoft documentation describes Shell Launcher as an embedded/kiosk device feature managed via WMI (`root\standardcimv2\embedded`). While supported on Windows Enterprise and IoT Enterprise, its availability on Windows 11 Pro is conditional, and it is strictly unsupported on Windows 11 Home.

### Resolution & Mitigation Strategy
- Do not make AsterShell dependent on Shell Launcher.
- Rely instead on standard per-user `HKCU:\Software\Microsoft\Windows NT\CurrentVersion\Winlogon\Shell` registry overrides, which function reliably across all Windows SKUs (Home, Pro, Enterprise, Workstation).

---

## 4. DWM Live Thumbnail Lifetime & Occlusion [HYPOTHESIS]

### Uncertainty & Context
`DwmRegisterThumbnail` allows an application window to display a real-time GPU-composed preview of another top-level HWND.
- It is hypothesized that if the source window is minimized (`IsIconic(HWND) == TRUE`), DWM stops compositing frames and retains the last known buffer or renders a blank frame.

### Resolution & Mitigation Strategy
- The `window-tracker` component will monitor window state. When an application is minimized, AsterShell can blend the cached window icon with the last known thumbnail or request a temporary window restoration preview if configured.
