# AsterShell — Manual Windows Validation Checklist

Use this checklist in an interactive Windows 11 desktop session before promoting from **Daily-Driver Candidate** to **Validated Daily Driver**:

- [ ] **Multi-Monitor & DPI:** Set `taskbar.monitor = "all"`, connect/disconnect a secondary monitor, and change display scaling (100% <-> 150%) to verify `WM_DISPLAYCHANGE` / `WM_DPICHANGED` surface creation, removal, and repositioning.
- [ ] **Window Tracking & Activation:** Open multiple apps (including minimized and multi-window apps), click taskbar pills to restore/minimize, right-click to verify native menu (`Activate / Restore`, `Minimize`, `Close Window`, `Escape` dismissal, click-outside dismissal), and test `Win+Alt+T` + Left/Right/Enter/Escape keyboard navigation.
- [ ] **System Tray & Explorer Restart:** Right-click AsterShell's tray icon (`Open Search`, `Toggle Taskbar`, `Exit AsterShell`), and restart `explorer.exe` via Task Manager to confirm `TaskbarCreated` icon recovery while third-party tray icons remain managed by Explorer.
- [ ] **Audio Endpoint Switching:** Play audio, switch default output device (Speakers <-> Headphones/Bluetooth) in Windows Sound settings, and verify volume/mute updates without polling.
- [ ] **Network & Power:** Toggle Wi-Fi/Ethernet and plug/unplug AC power (on laptop) to verify instant indicator updates.
- [ ] **Extended Soak:** Run `shell-host.exe` + `taskbar.exe` for >= 4 hours of normal desktop work and verify flat RSS memory and handle counts.
