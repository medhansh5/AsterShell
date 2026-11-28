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

/// Interpreted target mode from `TaskbarConfig.monitor`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonitorTargetMode {
    Primary,
    All,
    Specific(String),
}

impl MonitorTargetMode {
    pub fn parse(target: &str) -> Self {
        let trimmed = target.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("primary") {
            Self::Primary
        } else if trimmed.eq_ignore_ascii_case("all") {
            Self::All
        } else {
            Self::Specific(trimmed.to_string())
        }
    }
}

/// Result of reconciling active taskbar surfaces against current display topology.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorSurfaceDiff {
    pub to_add: Vec<MonitorInfo>,
    pub to_keep: Vec<MonitorInfo>,
    pub to_remove: Vec<String>,
}

/// Resolves the list of target `MonitorInfo` entries from an available monitor list.
///
/// - `"all"` returns every unique active monitor.
/// - `"primary"` returns the primary monitor.
/// - Numeric index or device name returns the matching monitor, falling back to primary if absent.
pub fn resolve_target_monitors(target: &str, available: &[MonitorInfo]) -> Vec<MonitorInfo> {
    if available.is_empty() {
        return Vec::new();
    }

    let primary = available
        .iter()
        .find(|m| m.is_primary)
        .cloned()
        .unwrap_or_else(|| available[0].clone());

    match MonitorTargetMode::parse(target) {
        MonitorTargetMode::Primary => vec![primary],
        MonitorTargetMode::All => {
            let mut seen = std::collections::HashSet::new();
            let mut out = Vec::new();
            for m in available {
                if seen.insert(m.device_name.to_ascii_lowercase()) {
                    out.push(m.clone());
                }
            }
            out
        }
        MonitorTargetMode::Specific(spec) => {
            if let Ok(idx) = spec.parse::<usize>() {
                if idx < available.len() {
                    return vec![available[idx].clone()];
                }
                tracing::warn!(
                    requested_index = idx,
                    available_count = available.len(),
                    "Requested monitor index out of bounds; falling back to primary"
                );
                return vec![primary];
            }

            if let Some(m) = available.iter().find(|m| {
                m.device_name.eq_ignore_ascii_case(&spec)
                    || m.device_name.to_lowercase().ends_with(&spec.to_lowercase())
            }) {
                return vec![m.clone()];
            }

            tracing::warn!(
                target = %spec,
                "Specified monitor target not found; falling back to primary monitor"
            );
            vec![primary]
        }
    }
}

/// Computes which monitor surfaces should be added, kept, or removed on topology change.
pub fn reconcile_monitor_surfaces(
    current_devices: &[String],
    desired: &[MonitorInfo],
) -> MonitorSurfaceDiff {
    let mut to_add = Vec::new();
    let mut to_keep = Vec::new();
    let mut to_remove = Vec::new();

    for m in desired {
        if current_devices
            .iter()
            .any(|d| d.eq_ignore_ascii_case(&m.device_name))
        {
            to_keep.push(m.clone());
        } else {
            to_add.push(m.clone());
        }
    }

    for dev in current_devices {
        if !desired
            .iter()
            .any(|m| m.device_name.eq_ignore_ascii_case(dev))
        {
            to_remove.push(dev.clone());
        }
    }

    MonitorSurfaceDiff {
        to_add,
        to_keep,
        to_remove,
    }
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
    let monitors = enumerate_monitors()?;
    resolve_target_monitors(target, &monitors)
        .into_iter()
        .next()
        .ok_or_else(|| AsterError::Platform("No display monitors available".into()))
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
    on_display_change: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
    drop_counter: Option<std::sync::Arc<std::sync::atomic::AtomicUsize>>,
}

impl Drop for SubclassContext {
    fn drop(&mut self) {
        if let Some(counter) = &self.drop_counter {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }
}

const SUBCLASS_ID_DISPLAY_CHANGE: usize = 0xAA01;

#[cfg(windows)]
fn subclass_registry(
) -> &'static std::sync::Mutex<std::collections::HashMap<isize, std::sync::Arc<SubclassContext>>> {
    static REGISTRY: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<isize, std::sync::Arc<SubclassContext>>>,
    > = std::sync::OnceLock::new();
    REGISTRY.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

#[cfg(windows)]
unsafe extern "system" fn display_change_subclass_proc(
    hwnd: windows::Win32::Foundation::HWND,
    umsg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
    uidsubclass: usize,
    _dwrefdata: usize,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass};
    use windows::Win32::UI::WindowsAndMessaging::{WM_DISPLAYCHANGE, WM_DPICHANGED, WM_NCDESTROY};

    let hwnd_raw = hwnd.0 as isize;

    match umsg {
        WM_DISPLAYCHANGE | WM_DPICHANGED => {
            let ctx_opt = subclass_registry()
                .lock()
                .ok()
                .and_then(|map| map.get(&hwnd_raw).cloned());

            if let Some(ctx) = ctx_opt {
                tracing::info!(
                    msg = umsg,
                    target = %ctx.monitor_target,
                    "Display topology or DPI change notification received; recalculating placement"
                );

                let _ = reposition_taskbar_on_monitor(
                    hwnd_raw,
                    &ctx.monitor_target,
                    ctx.logical_width,
                    ctx.logical_height,
                );
                if let Some(cb) = &ctx.on_display_change {
                    cb();
                }
            }
            DefSubclassProc(hwnd, umsg, wparam, lparam)
        }
        WM_NCDESTROY => {
            tracing::debug!("Taskbar window destroying; cleaning up display change subclass");
            let _ = RemoveWindowSubclass(hwnd, Some(display_change_subclass_proc), uidsubclass);
            if let Ok(mut map) = subclass_registry().lock() {
                map.remove(&hwnd_raw);
            }
            DefSubclassProc(hwnd, umsg, wparam, lparam)
        }
        _ => DefSubclassProc(hwnd, umsg, wparam, lparam),
    }
}

/// Installs (or atomically updates without leaking) a Win32 window subclass on the taskbar HWND
/// to intercept `WM_DISPLAYCHANGE` and `WM_DPICHANGED` messages.
pub fn install_display_change_subclass(
    hwnd: isize,
    monitor_target: &str,
    logical_width: i32,
    logical_height: i32,
) -> Result<()> {
    install_display_change_subclass_with_callback(
        hwnd,
        monitor_target,
        logical_width,
        logical_height,
        None,
        None,
    )
}

/// Installs or updates the display-change subclass with an optional notification callback.
pub fn install_display_change_subclass_with_callback(
    hwnd: isize,
    monitor_target: &str,
    logical_width: i32,
    logical_height: i32,
    on_display_change: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
    drop_counter: Option<std::sync::Arc<std::sync::atomic::AtomicUsize>>,
) -> Result<()> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::Shell::SetWindowSubclass;

        let handle = HWND(hwnd as _);
        let ctx = std::sync::Arc::new(SubclassContext {
            monitor_target: monitor_target.to_string(),
            logical_width,
            logical_height,
            on_display_change,
            drop_counter,
        });

        let mut map = subclass_registry()
            .lock()
            .map_err(|_| AsterError::Platform("Subclass registry lock poisoned".into()))?;

        let already_installed = map.contains_key(&hwnd);
        map.insert(hwnd, ctx);

        if !already_installed {
            let ok = unsafe {
                SetWindowSubclass(
                    handle,
                    Some(display_change_subclass_proc),
                    SUBCLASS_ID_DISPLAY_CHANGE,
                    0,
                )
            };

            if !ok.as_bool() {
                map.remove(&hwnd);
                return Err(AsterError::Platform(
                    "Failed to install SetWindowSubclass for display change monitoring".into(),
                ));
            }
        }

        tracing::info!(
            hwnd,
            monitor_target,
            replaced_existing = already_installed,
            "Installed display change subclass handler on taskbar window"
        );
        Ok(())
    }

    #[cfg(not(windows))]
    {
        let _ = (
            hwnd,
            monitor_target,
            logical_width,
            logical_height,
            on_display_change,
            drop_counter,
        );
        Ok(())
    }
}

/// Explicitly removes the display change subclass from an HWND and reclaims its context.
pub fn remove_display_change_subclass(hwnd: isize) -> Result<bool> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::Shell::RemoveWindowSubclass;

        let mut map = subclass_registry()
            .lock()
            .map_err(|_| AsterError::Platform("Subclass registry lock poisoned".into()))?;

        if map.remove(&hwnd).is_some() {
            let handle = HWND(hwnd as _);
            unsafe {
                let _ = RemoveWindowSubclass(
                    handle,
                    Some(display_change_subclass_proc),
                    SUBCLASS_ID_DISPLAY_CHANGE,
                );
            }
            return Ok(true);
        }
        Ok(false)
    }

    #[cfg(not(windows))]
    {
        let _ = hwnd;
        Ok(false)
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

    #[cfg(windows)]
    #[test]
    fn test_install_display_change_subclass_idempotent_no_leak() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use windows::core::{HSTRING, PCWSTR};
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, HWND_MESSAGE,
        };

        let hwnd = unsafe {
            CreateWindowExW(
                Default::default(),
                PCWSTR(HSTRING::from("STATIC").as_ptr()),
                PCWSTR(HSTRING::from("AsterSubclassTest").as_ptr()),
                Default::default(),
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                None,
                None,
                None,
            )
            .expect("CreateWindowExW for test")
        };

        let drop_counter = Arc::new(AtomicUsize::new(0));
        for _ in 0..10 {
            install_display_change_subclass_with_callback(
                hwnd.0 as isize,
                "primary",
                720,
                48,
                None,
                Some(drop_counter.clone()),
            )
            .expect("install_display_change_subclass_with_callback");
        }

        // 9 prior contexts must already have been reclaimed during repeated installs
        assert_eq!(drop_counter.load(Ordering::SeqCst), 9);

        unsafe {
            let _ = DestroyWindow(hwnd);
        }

        // DestroyWindow triggers WM_NCDESTROY, reclaiming the 10th and final context
        assert_eq!(drop_counter.load(Ordering::SeqCst), 10);

        // Removing after destruction is a safe no-op (no double-free)
        assert!(!remove_display_change_subclass(hwnd.0 as isize).unwrap_or(false));
    }

    #[test]
    fn test_all_mode_and_surface_reconciliation() {
        let m1 = MonitorInfo {
            hmonitor: 1,
            device_name: r"\\.\DISPLAY1".to_string(),
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
        };
        let m2 = MonitorInfo {
            hmonitor: 2,
            device_name: r"\\.\DISPLAY2".to_string(),
            is_primary: false,
            monitor_rect: Rect {
                left: 1920,
                top: 0,
                right: 3840,
                bottom: 1080,
            },
            work_rect: Rect {
                left: 1920,
                top: 0,
                right: 3840,
                bottom: 1040,
            },
            dpi: 144,
        };

        // "all" returns both monitors (never silently collapses to primary)
        let resolved_all = resolve_target_monitors("all", &[m1.clone(), m2.clone()]);
        assert_eq!(resolved_all.len(), 2);
        assert_eq!(resolved_all[0].device_name, r"\\.\DISPLAY1");
        assert_eq!(resolved_all[1].device_name, r"\\.\DISPLAY2");

        // Monitor addition: DISPLAY1 already active, DISPLAY2 added
        let diff_add = reconcile_monitor_surfaces(&[r"\\.\DISPLAY1".to_string()], &resolved_all);
        assert_eq!(diff_add.to_keep.len(), 1);
        assert_eq!(diff_add.to_add.len(), 1);
        assert_eq!(diff_add.to_add[0].device_name, r"\\.\DISPLAY2");
        assert!(diff_add.to_remove.is_empty());

        // Monitor removal: DISPLAY2 disconnected, falls back cleanly
        let diff_remove = reconcile_monitor_surfaces(
            &[r"\\.\DISPLAY1".to_string(), r"\\.\DISPLAY2".to_string()],
            std::slice::from_ref(&m1),
        );
        assert_eq!(diff_remove.to_keep.len(), 1);
        assert!(diff_remove.to_add.is_empty());
        assert_eq!(diff_remove.to_remove, vec![r"\\.\DISPLAY2".to_string()]);

        // Specific target missing falls back to primary
        let fallback = resolve_target_monitors(r"\\.\DISPLAY2", std::slice::from_ref(&m1));
        assert_eq!(fallback.len(), 1);
        assert_eq!(fallback[0].device_name, r"\\.\DISPLAY1");
    }
}
