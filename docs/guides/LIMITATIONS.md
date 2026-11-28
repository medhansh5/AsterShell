# AsterShell — Current M3 Limitations

- **Companion Mode Only:** AsterShell runs alongside Windows Explorer; it does not dock via `SHAppBarMessage` (`ABM_NEW`), does not modify `SPI_SETWORKAREA`, and does not replace `explorer.exe` (planned for Milestone 6).
- **Third-Party Tray Icons:** Third-party notification-area icons continue to be hosted by Windows Explorer; AsterShell hosts only its own GUID-identified tray icon.
- **UIPI Elevation Boundary:** Non-elevated `taskbar.exe` cannot force-focus elevated (Administrator) windows due to Windows UIPI.
- **Application Icons:** Taskbar items currently render clean two-letter monograms rather than extracted `.ico`/HICON bitmaps.
- **Pinned Apps Editing:** Pinned apps are configured via `%APPDATA%\AsterShell\config.toml` (`[[taskbar.pinned_apps]]`); interactive drag-and-drop pinning UI is not yet implemented.
- **Future Milestones:** `start-menu` (M4), `desktop` (M5), and full shell replacement (M6) remain scaffolded future work.
