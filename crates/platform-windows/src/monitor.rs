//! AsterShell Multi-Monitor Topology and Placement Subsystem
//!
//! Provides Win32 display monitor discovery, DPI-aware coordinate mapping,
//! signed coordinate bounding, and dynamic display change handling.

use crate::window::{get_window_dpi, position_taskbar_window, scale_to_physical_pixels, Rect};
use astercore::{AsterError, Result};
use std::fmt;

/// Represents an enumerated Win32 display monitor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorInfo {
    pub hmonitor: isize,
    pub device_name: String,
    pub is_primary: bool,
    pub monitor_rect: Rect,
    pub work_rect: Rect,
    pub dpi: u32,
}

impl fmt::Display for MonitorInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Monitor [{}]: primary={}, dpi={}, monitor=({}x{}+{}+{}), work=({}x{}+{}+{})",
            self.device_name,
            self.is_primary,
            self.dpi,
            self.monitor_rect.width(),
            self.monitor_rect.height(),
            self.monitor_rect.left,
            self.monitor_rect.top,
            self.work_rect.width(),
            self.work_rect.height(),
            self.work_rect.left,
            self.work_rect.top,
        )
    }
}

/// Enumerates all active display monitors attached to the desktop system.
pub fn enumerate_monitors() -> Result<Vec<MonitorInfo>> {
    #[cfg(windows)]
    {
        use std::mem::size_of;
        use windows::Win32::Foundation::{BOOL, LPARAM, RECT};
        use windows::Win32::Graphics::Gdi::{
            EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
        };
        use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};

        const MONITORINFOF_PRIMARY: u32 = 1;

        let mut monitors = Vec::new();

        unsafe extern "system" fn enum_proc(
            hmon: HMONITOR,
            _hdc: HDC,
            _rect: *mut RECT,
            lparam: LPARAM,
        ) -> BOOL {
            let list = &mut *(lparam.0 as *mut Vec<MonitorInfo>);

            let mut mi: MONITORINFOEXW = std::mem::zeroed();
            mi.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;

            if GetMonitorInfoW(hmon, &mut mi as *mut _ as _).as_bool() {
                let is_primary = (mi.monitorInfo.dwFlags & MONITORINFOF_PRIMARY) != 0;

                let mut len = 0;
                while len < mi.szDevice.len() && mi.szDevice[len] != 0 {
                    len += 1;
                }
                let device_name = String::from_utf16_lossy(&mi.szDevice[..len]);

                let mut dpi_x = 96u32;
                let mut dpi_y = 96u32;
                let _ = GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
                let dpi = if dpi_x > 0 { dpi_x } else { 96 };

                list.push(MonitorInfo {
                    hmonitor: hmon.0 as isize,
                    device_name,
                    is_primary,
                    monitor_rect: Rect {
                        left: mi.monitorInfo.rcMonitor.left,
                        top: mi.monitorInfo.rcMonitor.top,
                        right: mi.monitorInfo.rcMonitor.right,
                        bottom: mi.monitorInfo.rcMonitor.bottom,
                    },
                    work_rect: Rect {
                        left: mi.monitorInfo.rcWork.left,
                        top: mi.monitorInfo.rcWork.top,
                        right: mi.monitorInfo.rcWork.right,
                        bottom: mi.monitorInfo.rcWork.bottom,
                    },
                    dpi,
                });
            }

            BOOL(1) // Continue enumeration
        }

        unsafe {
            let lparam = LPARAM(&mut monitors as *mut _ as isize);
            let _ = EnumDisplayMonitors(None, None, Some(enum_proc), lparam);
        }

        if monitors.is_empty() {
            let primary_work = crate::window::get_primary_monitor_work_area()?;
            monitors.push(MonitorInfo {
                hmonitor: 0,
                device_name: r"\\.\DISPLAY1".to_string(),
                is_primary: true,
                monitor_rect: primary_work,
                work_rect: primary_work,
                dpi: 96,
            });
        }

        Ok(monitors)
    }

    #[cfg(not(windows))]
    {
        Ok(vec![MonitorInfo {
            hmonitor: 0,
            device_name: "DISPLAY1".to_string(),
            is_primary: true,
            monitor_rect: Rect {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
            work_rect: Rect {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1040,
            },
            dpi: 96,
        }])
    }
}

/// Finds the primary display monitor.
pub fn find_primary_monitor() -> Result<MonitorInfo> {
    let monitors = enumerate_monitors()?;
    if let Some(m) = monitors.iter().find(|m| m.is_primary) {
        return Ok(m.clone());
    }

    monitors
        .into_iter()
        .next()
        .ok_or_else(|| AsterError::Platform("No display monitors discovered on system".into()))
}

/// Resolves a display monitor target string to a specific `MonitorInfo`.
///
/// Target format:
/// - `"primary"` (default, case-insensitive)
/// - Index as integer string (`"0"`, `"1"`, etc.)
/// - Device name matching or containing (`"\\\\.\\DISPLAY1"`, `"DISPLAY2"`, etc.)
///
/// Falls back to the primary monitor if the target cannot be matched.
pub fn find_monitor_by_target(target: &str) -> Result<MonitorInfo> {
    let trimmed = target.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("primary") {
        return find_primary_monitor();
    }

    let monitors = enumerate_monitors()?;
    if monitors.is_empty() {
        return Err(AsterError::Platform("No display monitors available".into()));
    }

    // Check by numerical index
    if let Ok(idx) = trimmed.parse::<usize>() {
        if idx < monitors.len() {
            return Ok(monitors[idx].clone());
        } else {
            tracing::warn!(
                requested_index = idx,
                available_count = monitors.len(),
                "Requested monitor index out of bounds; falling back to primary"
            );
            return find_primary_monitor();
        }
    }

    // Check by exact or partial device name match
    if let Some(m) = monitors.iter().find(|m| {
        m.device_name.eq_ignore_ascii_case(trimmed)
            || m.device_name
                .to_lowercase()
                .ends_with(&trimmed.to_lowercase())
    }) {
        return Ok(m.clone());
    }

    tracing::warn!(
        target = trimmed,
        "Specified monitor target not found; falling back to primary monitor"
    );
    find_primary_monitor()
}

/// Computes the horizontal center and top placement coordinates (X, Y) and physical dimensions
/// for the floating companion taskbar window on the given monitor using signed `i32` coordinates.
///
/// Ensures correct placement even on monitors positioned with negative virtual coordinates
/// (e.g., secondary monitor situated to the left or above the primary monitor).
pub fn calculate_monitor_taskbar_coordinates(
    monitor: &MonitorInfo,
    logical_width: i32,
    logical_height: i32,
    override_dpi: Option<u32>,
) -> (i32, i32, i32, i32) {
    let effective_dpi = override_dpi.unwrap_or(monitor.dpi);
    let physical_width = scale_to_physical_pixels(logical_width, effective_dpi);
    let physical_height = scale_to_physical_pixels(logical_height, effective_dpi);

    let x = monitor.work_rect.left + (monitor.work_rect.width() - physical_width) / 2;
    let y = monitor.work_rect.top;

    (x, y, physical_width, physical_height)
}

/// Repositions the given taskbar window on the target monitor.
pub fn reposition_taskbar_on_monitor(
    hwnd: isize,
    monitor_target: &str,
    logical_width: i32,
    logical_height: i32,
) -> Result<MonitorInfo> {
    let monitor = find_monitor_by_target(monitor_target)?;
    let win_dpi = get_window_dpi(hwnd);
    let dpi = if win_dpi > 0 { win_dpi } else { monitor.dpi };

    let (x, y, phys_w, phys_h) =
        calculate_monitor_taskbar_coordinates(&monitor, logical_width, logical_height, Some(dpi));

    tracing::info!(
        hwnd,
        monitor_device = %monitor.device_name,
        is_primary = monitor.is_primary,
        x,
        y,
        phys_w,
        phys_h,
        dpi,
        "Positioning taskbar window on target monitor"
    );

    position_taskbar_window(hwnd, x, y, phys_w, phys_h)?;
    Ok(monitor)
}

/// Context stored for the subclass window procedure.
struct SubclassContext {
    monitor_target: String,
    logical_width: i32,
    logical_height: i32,
}

const SUBCLASS_ID_DISPLAY_CHANGE: usize = 0xAA01;

/// Installs a Win32 window subclass on the taskbar HWND to intercept `WM_DISPLAYCHANGE`
/// and `WM_DPICHANGED` messages, automatically recalculating coordinates and repositioning
/// the taskbar window without destroying UI state.
pub fn install_display_change_subclass(
    hwnd: isize,
    monitor_target: &str,
    logical_width: i32,
    logical_height: i32,
) -> Result<()> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
        use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
        use windows::Win32::UI::WindowsAndMessaging::{
            WM_DISPLAYCHANGE, WM_DPICHANGED, WM_NCDESTROY,
        };

        unsafe extern "system" fn subclass_proc(
            hwnd: HWND,
            umsg: u32,
            wparam: WPARAM,
            lparam: LPARAM,
            uidsubclass: usize,
            dwrefdata: usize,
        ) -> LRESULT {
            let ctx = &*(dwrefdata as *const SubclassContext);

            match umsg {
                WM_DISPLAYCHANGE | WM_DPICHANGED => {
                    tracing::info!(
                        msg = umsg,
                        target = %ctx.monitor_target,
                        "Display topology or DPI change notification received; recalculating placement"
                    );

                    let _ = reposition_taskbar_on_monitor(
                        hwnd.0 as isize,
                        &ctx.monitor_target,
                        ctx.logical_width,
                        ctx.logical_height,
                    );
                    DefSubclassProc(hwnd, umsg, wparam, lparam)
                }
                WM_NCDESTROY => {
                    tracing::debug!(
                        "Taskbar window destroying; cleaning up display change subclass"
                    );
                    let _ = RemoveWindowSubclass(hwnd, Some(subclass_proc), uidsubclass);
                    let _ = Box::from_raw(dwrefdata as *mut SubclassContext);
                    DefSubclassProc(hwnd, umsg, wparam, lparam)
                }
                _ => DefSubclassProc(hwnd, umsg, wparam, lparam),
            }
        }

        let ctx = Box::new(SubclassContext {
            monitor_target: monitor_target.to_string(),
            logical_width,
            logical_height,
        });
        let raw_ctx = Box::into_raw(ctx) as usize;

        let handle = HWND(hwnd as _);
        let ok = unsafe {
            SetWindowSubclass(
                handle,
                Some(subclass_proc),
                SUBCLASS_ID_DISPLAY_CHANGE,
                raw_ctx,
            )
        };

        if !ok.as_bool() {
            // Reclaim box to prevent leak on failure
            unsafe {
                let _ = Box::from_raw(raw_ctx as *mut SubclassContext);
            }
            return Err(AsterError::Platform(
                "Failed to install SetWindowSubclass for display change monitoring".into(),
            ));
        }

        tracing::info!(
            hwnd,
            monitor_target,
            "Installed display change subclass handler on taskbar window"
        );
        Ok(())
    }

    #[cfg(not(windows))]
    {
        let _ = (hwnd, monitor_target, logical_width, logical_height);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signed_taskbar_coordinates_calculation() {
        // Monitor positioned to the left of the primary monitor in virtual space
        let secondary_monitor = MonitorInfo {
            hmonitor: 123,
            device_name: r"\\.\DISPLAY2".to_string(),
            is_primary: false,
            monitor_rect: Rect {
                left: -1920,
                top: 0,
                right: 0,
                bottom: 1080,
            },
            work_rect: Rect {
                left: -1920,
                top: 0,
                right: 0,
                bottom: 1040,
            },
            dpi: 96,
        };

        let (x, y, w, h) =
            calculate_monitor_taskbar_coordinates(&secondary_monitor, 720, 48, Some(96));

        assert_eq!(w, 720);
        assert_eq!(h, 48);
        assert_eq!(y, 0);
        // Centered inside -1920 to 0:
        // x = -1920 + (1920 - 720) / 2 = -1920 + 600 = -1320
        assert_eq!(x, -1320);
    }

    #[test]
    fn test_scaled_dpi_taskbar_coordinates_calculation() {
        // Monitor with 150% scaling (144 DPI)
        let primary_4k = MonitorInfo {
            hmonitor: 456,
            device_name: r"\\.\DISPLAY1".to_string(),
            is_primary: true,
            monitor_rect: Rect {
                left: 0,
                top: 0,
                right: 3840,
                bottom: 2160,
            },
            work_rect: Rect {
                left: 0,
                top: 0,
                right: 3840,
                bottom: 2100,
            },
            dpi: 144, // 150% scale
        };

        let (x, y, w, h) = calculate_monitor_taskbar_coordinates(&primary_4k, 720, 48, None);

        // 720 * 144 / 96 = 1080
        assert_eq!(w, 1080);
        // 48 * 144 / 96 = 72
        assert_eq!(h, 72);
        assert_eq!(y, 0);
        // x = 0 + (3840 - 1080) / 2 = 1380
        assert_eq!(x, 1380);
    }

    #[test]
    fn test_monitor_target_resolution() {
        let primary = find_primary_monitor().expect("Primary monitor should be discovered");
        assert!(primary.is_primary || !primary.device_name.is_empty());

        let by_primary_str = find_monitor_by_target("primary").expect("Should find primary");
        assert_eq!(by_primary_str.device_name, primary.device_name);

        let by_idx_0 = find_monitor_by_target("0").expect("Should find index 0");
        assert!(!by_idx_0.device_name.is_empty());

        // Non-existent target gracefully falls back to primary
        let fallback =
            find_monitor_by_target("NON_EXISTENT_DISPLAY_XYZ").expect("Fallback to primary");
        assert_eq!(fallback.device_name, primary.device_name);
    }
}
