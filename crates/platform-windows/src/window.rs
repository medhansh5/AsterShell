//! AsterShell Window Management and DWM Backdrop Subsystem
//!
//! Provides Win32 window positioning, multi-monitor query relative to cursor,
//! and DWM backdrop styling (Mica / Acrylic / Dark Mode).

use astercore::{AsterError, Result};

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
