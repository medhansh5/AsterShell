//! AsterShell Window Management and DWM Backdrop Subsystem
//!
//! Provides Win32 window positioning, multi-monitor query relative to cursor,
//! and DWM backdrop styling (Mica / Acrylic / Dark Mode).

pub use astercore::{AsterError, Result};

pub const WS_POPUP_FLAG: u32 = 0x8000_0000;
pub const WS_VISIBLE_FLAG: u32 = 0x1000_0000;
pub const WS_CLIPSIBLINGS_FLAG: u32 = 0x0400_0000;
pub const WS_CLIPCHILDREN_FLAG: u32 = 0x0200_0000;
pub const WS_CAPTION_FLAG: u32 = 0x00C0_0000;
pub const WS_THICKFRAME_FLAG: u32 = 0x0004_0000;
pub const WS_SYSMENU_FLAG: u32 = 0x0008_0000;
pub const WS_EX_TOPMOST_FLAG: u32 = 0x0000_0008;
pub const WS_EX_TOOLWINDOW_FLAG: u32 = 0x0000_0080;
pub const WS_EX_NOACTIVATE_FLAG: u32 = 0x0800_0000;
pub const WS_EX_APPWINDOW_FLAG: u32 = 0x0004_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DwmBackdropType {
    Auto = 0,
    None = 1,
    Mica = 2,
    Acrylic = 3,
    MicaAlt = 4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

/// Applies immersive dark mode and optional DWM system backdrop to a window HWND.
pub fn apply_dark_mode_and_backdrop(hwnd: isize, backdrop: DwmBackdropType) -> Result<()> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::{BOOL, HWND};
        use windows::Win32::Graphics::Dwm::{
            DwmSetWindowAttribute, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE,
            DWMWA_WINDOW_CORNER_PREFERENCE, DWM_WINDOW_CORNER_PREFERENCE,
        };

        let handle = HWND(hwnd as _);

        // 1. Apply Immersive Dark Mode
        let dark_mode = BOOL(1);
        unsafe {
            let _ = DwmSetWindowAttribute(
                handle,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &dark_mode as *const _ as _,
                std::mem::size_of::<BOOL>() as u32,
            );
        }

        // 2. Apply Rounded Corners (DWMWCP_ROUND = 2)
        let corner_pref = 2u32; // DWMWCP_ROUND
        unsafe {
            let _ = DwmSetWindowAttribute(
                handle,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &corner_pref as *const _ as _,
                std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
            );
        }

        // 3. Apply System Backdrop (Mica / Acrylic)
        let backdrop_val = backdrop as u32;
        unsafe {
            let res = DwmSetWindowAttribute(
                handle,
                DWMWA_SYSTEMBACKDROP_TYPE,
                &backdrop_val as *const _ as _,
                std::mem::size_of::<u32>() as u32,
            );
            if let Err(e) = res {
                tracing::warn!(
                    hwnd,
                    error = %e,
                    "DWM system backdrop attribute not supported on this Windows build, using fallback"
                );
            }
        }

        Ok(())
    }

    #[cfg(not(windows))]
    {
        let _ = (hwnd, backdrop);
        Ok(())
    }
}

/// Retrieves the working area rectangle of the monitor containing the mouse cursor.
///
/// Falls back to the primary monitor work area or system screen metrics
/// if running in a non-interactive session or if cursor access is restricted.
pub fn get_active_monitor_work_area() -> Result<Rect> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::{POINT, RECT};
        use windows::Win32::Graphics::Gdi::{
            GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            GetCursorPos, GetSystemMetrics, SystemParametersInfoW, SM_CXSCREEN, SM_CYSCREEN,
            SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
        };

        let mut cursor_pos = POINT::default();
        let cursor_res = unsafe { GetCursorPos(&mut cursor_pos) };

        if cursor_res.is_ok() {
            let hmonitor = unsafe { MonitorFromPoint(cursor_pos, MONITOR_DEFAULTTONEAREST) };
            if !hmonitor.is_invalid() {
                let mut mi = MONITORINFO {
                    cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                    ..Default::default()
                };

                let ok = unsafe { GetMonitorInfoW(hmonitor, &mut mi) };
                if ok.as_bool() {
                    return Ok(Rect {
                        left: mi.rcWork.left,
                        top: mi.rcWork.top,
                        right: mi.rcWork.right,
                        bottom: mi.rcWork.bottom,
                    });
                }
            }
        }

        // Fallback 1: SystemParametersInfoW for primary monitor work area (excludes taskbar)
        let mut work_rect = RECT::default();
        let spi_res = unsafe {
            SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                Some(&mut work_rect as *mut _ as *mut std::ffi::c_void),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
        };

        if spi_res.is_ok()
            && (work_rect.right > work_rect.left)
            && (work_rect.bottom > work_rect.top)
        {
            return Ok(Rect {
                left: work_rect.left,
                top: work_rect.top,
                right: work_rect.right,
                bottom: work_rect.bottom,
            });
        }

        // Fallback 2: GetSystemMetrics screen resolution
        let cx = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let cy = unsafe { GetSystemMetrics(SM_CYSCREEN) };
        if cx > 0 && cy > 0 {
            return Ok(Rect {
                left: 0,
                top: 0,
                right: cx,
                bottom: cy,
            });
        }

        // Fallback 3: Standard 1080p fallback
        Ok(Rect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        })
    }

    #[cfg(not(windows))]
    {
        Ok(Rect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        })
    }
}

/// Queries the effective DPI for a given Win32 window HWND.
/// Returns 96 if unable to query or on non-Windows platforms.
pub fn get_window_dpi(hwnd: isize) -> u32 {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::HiDpi::GetDpiForWindow;
        unsafe {
            let dpi = GetDpiForWindow(HWND(hwnd as _));
            if dpi == 0 {
                96
            } else {
                dpi
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        96
    }
}

/// Scales a logical pixel dimension to physical device pixels based on the given DPI.
pub fn scale_to_physical_pixels(logical_size: i32, dpi: u32) -> i32 {
    let effective_dpi = if dpi == 0 { 96 } else { dpi };
    (logical_size * effective_dpi as i32) / 96
}

/// Computes the centered (X, Y) coordinate and physical dimensions for a spotlight window
/// taking into account the window's effective DPI on the target monitor.
/// Places the window horizontally centered and ~20% from the top of the active monitor's work area.
pub fn calculate_spotlight_coordinates_with_dpi(
    logical_width: i32,
    logical_height: i32,
    dpi: u32,
) -> Result<(i32, i32, i32, i32)> {
    let work = get_active_monitor_work_area()?;
    let physical_width = scale_to_physical_pixels(logical_width, dpi);
    let physical_height = scale_to_physical_pixels(logical_height, dpi);

    let x = work.left + (work.width() - physical_width) / 2;
    let y = work.top + (work.height() * 20 / 100);
    Ok((x, y, physical_width, physical_height))
}

/// Computes the centered (X, Y) coordinate for a spotlight window of given dimensions
/// located horizontally centered and placed at ~20% from the top of the active monitor's work area.
pub fn calculate_spotlight_coordinates(
    window_width: i32,
    _window_height: i32,
) -> Result<(i32, i32)> {
    let work = get_active_monitor_work_area()?;
    let x = work.left + (work.width() - window_width) / 2;
    let y = work.top + (work.height() * 20 / 100);
    Ok((x, y))
}

/// Moves and sizes a Win32 window to the specified coordinates without stealing focus unless requested.
pub fn position_window(hwnd: isize, x: i32, y: i32, width: i32, height: i32) -> Result<()> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER};

        unsafe {
            SetWindowPos(
                HWND(hwnd as _),
                None,
                x,
                y,
                width,
                height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            )
            .map_err(|e| AsterError::Platform(format!("SetWindowPos failed: {}", e)))?;
        }
    }

    #[cfg(not(windows))]
    {
        let _ = (hwnd, x, y, width, height);
    }

    Ok(())
}

/// Retrieves the working area rectangle of the primary monitor using signed `i32` coordinates.
///
/// Queries `MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY)` and `GetMonitorInfoW`.
/// Falls back to `SystemParametersInfoW(SPI_GETWORKAREA)` and standard fallback resolutions.
pub fn get_primary_monitor_work_area() -> Result<Rect> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::{POINT, RECT};
        use windows::Win32::Graphics::Gdi::{
            GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTOPRIMARY,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            GetSystemMetrics, SystemParametersInfoW, SM_CXSCREEN, SM_CYSCREEN, SPI_GETWORKAREA,
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
        };

        let origin = POINT { x: 0, y: 0 };
        let hmonitor = unsafe { MonitorFromPoint(origin, MONITOR_DEFAULTTOPRIMARY) };
        if !hmonitor.is_invalid() {
            let mut mi = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };

            let ok = unsafe { GetMonitorInfoW(hmonitor, &mut mi) };
            if ok.as_bool() {
                return Ok(Rect {
                    left: mi.rcWork.left,
                    top: mi.rcWork.top,
                    right: mi.rcWork.right,
                    bottom: mi.rcWork.bottom,
                });
            }
        }

        // Fallback 1: SystemParametersInfoW for primary monitor work area (excludes taskbar)
        let mut work_rect = RECT::default();
        let spi_res = unsafe {
            SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                Some(&mut work_rect as *mut _ as *mut std::ffi::c_void),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
        };

        if spi_res.is_ok()
            && (work_rect.right > work_rect.left)
            && (work_rect.bottom > work_rect.top)
        {
            return Ok(Rect {
                left: work_rect.left,
                top: work_rect.top,
                right: work_rect.right,
                bottom: work_rect.bottom,
            });
        }

        // Fallback 2: GetSystemMetrics screen resolution
        let cx = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let cy = unsafe { GetSystemMetrics(SM_CYSCREEN) };
        if cx > 0 && cy > 0 {
            return Ok(Rect {
                left: 0,
                top: 0,
                right: cx,
                bottom: cy,
            });
        }

        // Fallback 3: Standard 1080p fallback
        Ok(Rect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        })
    }

    #[cfg(not(windows))]
    {
        Ok(Rect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        })
    }
}

/// Retrieves the Win32 window style and extended style for a given HWND.
///
/// Returns `(style, ex_style)` as `(u32, u32)`.
pub fn get_window_styles(hwnd: isize) -> Result<(u32, u32)> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, GWL_EXSTYLE, GWL_STYLE};

        let handle = HWND(hwnd as _);
        let style = unsafe { GetWindowLongPtrW(handle, GWL_STYLE) } as u32;
        let ex_style = unsafe { GetWindowLongPtrW(handle, GWL_EXSTYLE) } as u32;

        Ok((style, ex_style))
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        Ok((0, 0))
    }
}

/// Configures native Win32 window styles for the AsterTaskbar floating companion window.
///
/// Ensures:
/// - Base: `WS_POPUP | WS_VISIBLE | WS_CLIPSIBLINGS | WS_CLIPCHILDREN`
/// - Extended: `WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`
/// - Strips: `WS_EX_APPWINDOW`
///
/// Flushes the style changes via `SetWindowPos` with `SWP_FRAMECHANGED` without activating or moving the window.
/// Returns the verified `(style, ex_style)` read back after application.
pub fn configure_taskbar_native_styles(hwnd: isize) -> Result<(u32, u32)> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, GWL_STYLE,
            HWND_TOPMOST, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
            WS_CAPTION, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_EX_APPWINDOW, WS_EX_NOACTIVATE,
            WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU,
            WS_THICKFRAME, WS_VISIBLE,
        };

        let handle = HWND(hwnd as _);

        // 1. Read existing styles
        let current_style = unsafe { GetWindowLongPtrW(handle, GWL_STYLE) } as u32;
        let current_ex_style = unsafe { GetWindowLongPtrW(handle, GWL_EXSTYLE) } as u32;

        // 2. Compute target base style: ensure POPUP, VISIBLE, CLIPSIBLINGS, CLIPCHILDREN are set,
        // and explicitly strip title-bar, resize-borders, and sysmenu frames (WINDOW-01).
        let unwanted_frame_bits =
            WS_CAPTION.0 | WS_THICKFRAME.0 | WS_MINIMIZEBOX.0 | WS_MAXIMIZEBOX.0 | WS_SYSMENU.0;
        let target_style =
            (current_style | WS_POPUP.0 | WS_VISIBLE.0 | WS_CLIPSIBLINGS.0 | WS_CLIPCHILDREN.0)
                & !unwanted_frame_bits;

        // 3. Compute target extended style: add TOPMOST, TOOLWINDOW, NOACTIVATE; strip APPWINDOW
        let target_ex_style =
            (current_ex_style | WS_EX_TOPMOST.0 | WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0)
                & !WS_EX_APPWINDOW.0;

        // 4. Apply via SetWindowLongPtrW
        unsafe {
            SetWindowLongPtrW(handle, GWL_STYLE, target_style as isize);
            SetWindowLongPtrW(handle, GWL_EXSTYLE, target_ex_style as isize);
        }

        // 5. Force frame update and HWND_TOPMOST without moving or stealing focus.
        // Note: SetWindowLongPtrW(GWL_EXSTYLE) cannot set WS_EX_TOPMOST; only SetWindowPos(HWND_TOPMOST) does.
        unsafe {
            use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOWNA};
            let _ = ShowWindow(handle, SW_SHOWNA);
            if let Err(e) = SetWindowPos(
                handle,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED | SWP_SHOWWINDOW,
            ) {
                tracing::warn!(
                    hwnd,
                    error = %e,
                    "SetWindowPos(HWND_TOPMOST) failed in configure_taskbar_native_styles"
                );
            }
        }

        // 6. Read back verified styles
        let verified_style = unsafe { GetWindowLongPtrW(handle, GWL_STYLE) } as u32;
        let mut verified_ex_style = unsafe { GetWindowLongPtrW(handle, GWL_EXSTYLE) } as u32;

        if (verified_ex_style & WS_EX_TOPMOST.0) == 0 {
            unsafe {
                if let Err(e) = SetWindowPos(
                    handle,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                ) {
                    tracing::warn!(
                        hwnd,
                        error = %e,
                        "Retry SetWindowPos(HWND_TOPMOST) failed in configure_taskbar_native_styles"
                    );
                }
            }
            verified_ex_style = unsafe { GetWindowLongPtrW(handle, GWL_EXSTYLE) } as u32;
        }

        tracing::info!(
            hwnd,
            style = format_args!("{:#010x}", verified_style),
            ex_style = format_args!("{:#010x}", verified_ex_style),
            "Taskbar native window styles configured and verified"
        );

        Ok((verified_style, verified_ex_style))
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        Ok((0, 0))
    }
}

/// Computes the horizontal center and top placement coordinates (X, Y) and physical dimensions
/// for the floating companion taskbar window on the primary monitor using signed `i32` coordinates.
///
/// Places the window horizontally centered at the top of the primary monitor's work area:
/// - `x = work.left + (work.width() - physical_width) / 2`
/// - `y = work.top`
pub fn calculate_taskbar_coordinates_with_dpi(
    logical_width: i32,
    logical_height: i32,
    dpi: u32,
) -> Result<(i32, i32, i32, i32)> {
    let work = get_primary_monitor_work_area()?;
    let physical_width = scale_to_physical_pixels(logical_width, dpi);
    let physical_height = scale_to_physical_pixels(logical_height, dpi);

    let x = work.left + (work.width() - physical_width) / 2;
    let y = work.top;
    Ok((x, y, physical_width, physical_height))
}

/// Positions and sizes the taskbar window at the specified coordinates, ensuring it remains
/// `HWND_TOPMOST`, shown, and does not activate or steal focus.
pub fn position_taskbar_window(hwnd: isize, x: i32, y: i32, width: i32, height: i32) -> Result<()> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_SHOWWINDOW,
        };

        unsafe {
            SetWindowPos(
                HWND(hwnd as _),
                HWND_TOPMOST,
                x,
                y,
                width,
                height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
            .map_err(|e| {
                AsterError::Platform(format!(
                    "position_taskbar_window SetWindowPos failed: {}",
                    e
                ))
            })?;
        }
    }

    #[cfg(not(windows))]
    {
        let _ = (hwnd, x, y, width, height);
    }

    Ok(())
}

/// Applies native Win32 window styles, DWM Mica backdrop, and primary-monitor positioning.
pub fn apply_native_taskbar_configuration(hwnd: isize) -> Result<(u32, u32)> {
    // 1. DWM Immersive Dark Mode and Mica backdrop with fallback
    if let Err(e) = apply_dark_mode_and_backdrop(hwnd, DwmBackdropType::Mica) {
        tracing::warn!(error = %e, "DWM Mica application returned fallback");
    }

    // 2. Configure native styles: WS_POPUP, WS_EX_TOPMOST, WS_EX_TOOLWINDOW, WS_EX_NOACTIVATE, !WS_EX_APPWINDOW
    let _ = configure_taskbar_native_styles(hwnd)?;

    // 3. Query DPI and calculate centered top coordinates on primary monitor
    let dpi = get_window_dpi(hwnd);
    if let Ok((x, y, phys_w, phys_h)) = calculate_taskbar_coordinates_with_dpi(720, 48, dpi) {
        if let Err(e) = position_taskbar_window(hwnd, x, y, phys_w, phys_h) {
            tracing::warn!(error = %e, "Failed to position taskbar window");
        }
    }

    get_window_styles(hwnd)
}

/// Action selected from a taskbar item's native right-click context menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowMenuAction {
    None,
    Activate,
    Minimize,
    Close,
}

/// Displays a native Win32 context menu at the cursor for a tracked taskbar window.
///
/// Uses `SetForegroundWindow` + `TPM_RETURNCMD` + `PostMessageW(WM_NULL)` so Escape,
/// `WM_CANCELMODE`, focus loss, and click-outside dismiss the menu cleanly without
/// triggering accidental actions.
pub fn show_window_item_context_menu(owner_hwnd: isize) -> Result<WindowMenuAction> {
    #[cfg(windows)]
    {
        use windows::core::{HSTRING, PCWSTR};
        use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{
            AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, PostMessageW,
            SetForegroundWindow, TrackPopupMenuEx, MF_SEPARATOR, MF_STRING, TPM_BOTTOMALIGN,
            TPM_LEFTALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_NULL,
        };

        const CMD_ACTIVATE: usize = 1;
        const CMD_MINIMIZE: usize = 2;
        const CMD_CLOSE: usize = 3;

        let handle = HWND(owner_hwnd as _);
        unsafe {
            let hmenu = CreatePopupMenu()
                .map_err(|e| AsterError::Platform(format!("CreatePopupMenu failed: {}", e)))?;

            let activate_lbl = HSTRING::from("Activate / Restore");
            let minimize_lbl = HSTRING::from("Minimize");
            let close_lbl = HSTRING::from("Close Window");

            let _ = AppendMenuW(
                hmenu,
                MF_STRING,
                CMD_ACTIVATE,
                PCWSTR(activate_lbl.as_ptr()),
            );
            let _ = AppendMenuW(
                hmenu,
                MF_STRING,
                CMD_MINIMIZE,
                PCWSTR(minimize_lbl.as_ptr()),
            );
            let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
            let _ = AppendMenuW(hmenu, MF_STRING, CMD_CLOSE, PCWSTR(close_lbl.as_ptr()));

            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);

            let _ = SetForegroundWindow(handle);
            let cmd = TrackPopupMenuEx(
                hmenu,
                (TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD).0,
                pt.x,
                pt.y,
                handle,
                None,
            );
            let _ = PostMessageW(handle, WM_NULL, WPARAM(0), LPARAM(0));
            let _ = DestroyMenu(hmenu);

            let action = match cmd.0 as usize {
                CMD_ACTIVATE => WindowMenuAction::Activate,
                CMD_MINIMIZE => WindowMenuAction::Minimize,
                CMD_CLOSE => WindowMenuAction::Close,
                _ => WindowMenuAction::None,
            };
            Ok(action)
        }
    }

    #[cfg(not(windows))]
    {
        let _ = owner_hwnd;
        Ok(WindowMenuAction::None)
    }
}
