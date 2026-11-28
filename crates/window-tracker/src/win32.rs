//! Win32 Window Inspection, Safe Activation, and Hook-Before-Snapshot WinEvent Worker.

use crate::filter::{should_track_attributes, WindowAttributes};
use crate::{WindowEvent, WindowInfo};
use astercore::{AsterError, Result};
use std::cell::RefCell;
use std::collections::HashMap;
use std::mem::size_of;
use std::sync::mpsc::Sender;
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::Graphics::Gdi::{MonitorFromWindow, MONITOR_DEFAULTTONEAREST};
use windows::Win32::System::Threading::{
    GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, EnumWindows, GetClassNameW, GetForegroundWindow, GetMessageW, GetWindow,
    GetWindowLongPtrW, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindow,
    IsWindowVisible, PostMessageW, PostThreadMessageW, SetForegroundWindow, ShowWindow,
    TranslateMessage, GWL_EXSTYLE, GWL_STYLE, GW_OWNER, MSG, OBJID_WINDOW, SW_MINIMIZE, SW_RESTORE,
    WM_CLOSE, WM_QUIT, WS_CHILD, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};

const EVENT_SYSTEM_FOREGROUND: u32 = 0x0003;
const EVENT_SYSTEM_MINIMIZESTART: u32 = 0x0016;
const EVENT_SYSTEM_MINIMIZEEND: u32 = 0x0017;
const EVENT_OBJECT_CREATE: u32 = 0x8000;
const EVENT_OBJECT_DESTROY: u32 = 0x8001;
const EVENT_OBJECT_SHOW: u32 = 0x8002;
const EVENT_OBJECT_HIDE: u32 = 0x8003;
const EVENT_OBJECT_NAMECHANGE: u32 = 0x800C;
const EVENT_OBJECT_CLOAKED: u32 = 0x8017;
const EVENT_OBJECT_UNCLOAKED: u32 = 0x8018;

const WINEVENT_OUTOFCONTEXT: u32 = 0x0000;
const WINEVENT_SKIPOWNPROCESS: u32 = 0x0002;

struct WorkerState {
    tx: Sender<WindowEvent>,
    tracked: HashMap<isize, WindowInfo>,
    focused_hwnd: Option<isize>,
}

thread_local! {
    static WORKER_STATE: RefCell<Option<WorkerState>> = const { RefCell::new(None) };
}

fn get_window_text_string(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    let len = unsafe { GetWindowTextW(hwnd, &mut buf) };
    if len > 0 {
        String::from_utf16_lossy(&buf[..len as usize])
    } else {
        String::new()
    }
}

fn get_window_class_string(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, &mut buf) };
    if len > 0 {
        String::from_utf16_lossy(&buf[..len as usize])
    } else {
        String::new()
    }
}

fn query_process_executable(pid: u32) -> (String, String) {
    if pid == 0 {
        return (String::new(), String::new());
    }

    unsafe {
        if let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            let mut buf = [0u16; 1024];
            let mut size = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_FORMAT(0),
                PWSTR(buf.as_mut_ptr()),
                &mut size,
            );
            let _ = CloseHandle(handle);

            if ok.is_ok() && size > 0 {
                let full_path = String::from_utf16_lossy(&buf[..size as usize]);
                let process_name = std::path::Path::new(&full_path)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| full_path.clone());
                return (process_name, full_path);
            }
        }
    }

    (String::new(), String::new())
}

/// Inspects a Win32 HWND against the 7-stage filter pipeline.
/// Returns `Some(WindowInfo)` if the window is a trackable user application window.
pub fn inspect_window(hwnd_raw: isize, foreground_hwnd: Option<isize>) -> Option<WindowInfo> {
    if hwnd_raw == 0 {
        return None;
    }

    let hwnd = HWND(hwnd_raw as _);
    unsafe {
        let is_valid = IsWindow(hwnd).as_bool();
        if !is_valid {
            return None;
        }

        // Skip windows owned by the current process (matching WINEVENT_SKIPOWNPROCESS)
        // before calling GetWindowTextW, which issues a synchronous cross-thread
        // WM_GETTEXT SendMessage on same-process HWNDs and would deadlock if the UI
        // thread is waiting for WindowTracker::start() before entering its event loop.
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == std::process::id() {
            return None;
        }

        let is_visible = IsWindowVisible(hwnd).as_bool();
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;

        let is_child = (style & WS_CHILD.0) != 0;
        let is_tool_window = (ex_style & WS_EX_TOOLWINDOW.0) != 0;
        let is_app_window = (ex_style & WS_EX_APPWINDOW.0) != 0;

        let owner = GetWindow(hwnd, GW_OWNER).unwrap_or(HWND(std::ptr::null_mut()));
        let has_owner = !owner.0.is_null();

        let mut cloaked: u32 = 0;
        let _ = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut _ as _,
            size_of::<u32>() as u32,
        );
        let is_cloaked = cloaked != 0;

        let is_iconic = IsIconic(hwnd).as_bool();

        let mut rect = RECT::default();
        let _ = GetWindowRect(hwnd, &mut rect);
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;

        let class_name = get_window_class_string(hwnd);
        let title = get_window_text_string(hwnd);

        let attrs = WindowAttributes {
            is_valid,
            is_visible,
            is_child,
            is_tool_window,
            is_app_window,
            has_owner,
            is_cloaked,
            is_iconic,
            width,
            height,
            class_name: class_name.clone(),
            title: title.clone(),
        };

        if !should_track_attributes(&attrs) {
            return None;
        }

        let (process_name, executable_path) = query_process_executable(pid);

        let hmon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let is_focused = foreground_hwnd == Some(hwnd_raw);

        Some(WindowInfo {
            hwnd: hwnd_raw,
            title: if title.is_empty() {
                process_name.clone()
            } else {
                title
            },
            class_name,
            pid,
            process_name,
            executable_path,
            is_minimized: is_iconic,
            is_focused,
            monitor_hmonitor: hmon.0 as isize,
        })
    }
}

/// Enumerates all currently trackable top-level application windows.
pub fn enumerate_trackable_windows() -> Result<Vec<WindowInfo>> {
    let fg = unsafe { GetForegroundWindow() };
    let fg_raw = if fg.0.is_null() {
        None
    } else {
        Some(fg.0 as isize)
    };

    struct EnumContext {
        windows: Vec<WindowInfo>,
        fg_raw: Option<isize>,
    }

    let mut ctx = EnumContext {
        windows: Vec::new(),
        fg_raw,
    };

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = &mut *(lparam.0 as *mut EnumContext);
        if let Some(info) = inspect_window(hwnd.0 as isize, ctx.fg_raw) {
            ctx.windows.push(info);
        }
        BOOL(1)
    }

    unsafe {
        let _ = EnumWindows(Some(enum_proc), LPARAM(&mut ctx as *mut _ as isize));
    }

    Ok(ctx.windows)
}

/// Safely activates and restores (if minimized) a target window without using `AttachThreadInput`.
pub fn activate_window(hwnd_raw: isize) -> Result<()> {
    let hwnd = HWND(hwnd_raw as _);
    unsafe {
        if !IsWindow(hwnd).as_bool() {
            return Err(AsterError::Platform(format!(
                "Cannot activate invalid HWND {}",
                hwnd_raw
            )));
        }

        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }

        let ok = SetForegroundWindow(hwnd);
        if !ok.as_bool() {
            tracing::debug!(
                hwnd = hwnd_raw,
                "SetForegroundWindow returned false (Windows focus lock); window restored"
            );
        }
    }
    Ok(())
}

/// Minimizes a target window.
pub fn minimize_window(hwnd_raw: isize) -> Result<()> {
    let hwnd = HWND(hwnd_raw as _);
    unsafe {
        if !IsWindow(hwnd).as_bool() {
            return Err(AsterError::Platform(format!(
                "Cannot minimize invalid HWND {}",
                hwnd_raw
            )));
        }
        let _ = ShowWindow(hwnd, SW_MINIMIZE);
    }
    Ok(())
}

/// Requests a graceful close (`WM_CLOSE`) for a target window.
pub fn close_window(hwnd_raw: isize) -> Result<()> {
    let hwnd = HWND(hwnd_raw as _);
    unsafe {
        if !IsWindow(hwnd).as_bool() {
            return Err(AsterError::Platform(format!(
                "Cannot close invalid HWND {}",
                hwnd_raw
            )));
        }
        PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0))
            .map_err(|e| AsterError::Platform(format!("PostMessageW WM_CLOSE failed: {}", e)))?;
    }
    Ok(())
}

unsafe extern "system" fn win_event_proc(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    _id_child: i32,
    _id_event_thread: u32,
    _dwms_event_time: u32,
) {
    if id_object != OBJID_WINDOW.0 || hwnd.0.is_null() {
        return;
    }

    let hwnd_raw = hwnd.0 as isize;

    WORKER_STATE.with(|cell| {
        let mut borrow = cell.borrow_mut();
        let Some(state) = borrow.as_mut() else {
            return;
        };

        match event {
            EVENT_SYSTEM_FOREGROUND => {
                state.focused_hwnd = Some(hwnd_raw);
                for (k, info) in state.tracked.iter_mut() {
                    let should_focus = *k == hwnd_raw;
                    if info.is_focused != should_focus {
                        info.is_focused = should_focus;
                        let _ = state.tx.send(WindowEvent::WindowUpdated(info.clone()));
                    }
                }
                let _ = state.tx.send(WindowEvent::FocusChanged(Some(hwnd_raw)));
            }
            EVENT_OBJECT_DESTROY | EVENT_OBJECT_HIDE | EVENT_OBJECT_CLOAKED => {
                if state.tracked.remove(&hwnd_raw).is_some() {
                    let _ = state.tx.send(WindowEvent::WindowRemoved(hwnd_raw));
                }
            }
            EVENT_OBJECT_CREATE
            | EVENT_OBJECT_SHOW
            | EVENT_OBJECT_UNCLOAKED
            | EVENT_OBJECT_NAMECHANGE
            | EVENT_SYSTEM_MINIMIZESTART
            | EVENT_SYSTEM_MINIMIZEEND => {
                if let Some(info) = inspect_window(hwnd_raw, state.focused_hwnd) {
                    let is_new = !state.tracked.contains_key(&hwnd_raw);
                    let changed = state.tracked.get(&hwnd_raw) != Some(&info);
                    if is_new {
                        state.tracked.insert(hwnd_raw, info.clone());
                        let _ = state.tx.send(WindowEvent::WindowAdded(info));
                    } else if changed {
                        state.tracked.insert(hwnd_raw, info.clone());
                        let _ = state.tx.send(WindowEvent::WindowUpdated(info));
                    }
                } else if state.tracked.remove(&hwnd_raw).is_some() {
                    let _ = state.tx.send(WindowEvent::WindowRemoved(hwnd_raw));
                }
            }
            _ => {}
        }
    });
}

/// Runs the STA WinEvent hook worker on the current thread.
///
/// Enforces strict hook-before-snapshot ordering:
/// 1. Arms `SetWinEventHook` hooks first so no events during enumeration are lost.
/// 2. Takes the initial `EnumWindows` snapshot and emits `WindowEvent::Snapshot`.
/// 3. Runs the Win32 message pump until `WM_QUIT`.
pub fn run_win_event_worker(tx: Sender<WindowEvent>, ready_tx: Sender<Result<u32>>) {
    let thread_id = unsafe { GetCurrentThreadId() };

    let mut hooks: Vec<HWINEVENTHOOK> = Vec::new();
    let ranges = [
        (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
        (EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND),
        (EVENT_OBJECT_CREATE, EVENT_OBJECT_HIDE),
        (EVENT_OBJECT_NAMECHANGE, EVENT_OBJECT_NAMECHANGE),
        (EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED),
    ];

    // Step 1: Arm WinEvent hooks BEFORE initial snapshot
    for (min_ev, max_ev) in ranges {
        let hook = unsafe {
            SetWinEventHook(
                min_ev,
                max_ev,
                None,
                Some(win_event_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            )
        };
        if !hook.is_invalid() {
            hooks.push(hook);
        }
    }

    if hooks.is_empty() {
        let _ = ready_tx.send(Err(AsterError::Platform(
            "Failed to register any WinEvent hooks".into(),
        )));
        return;
    }

    // Step 2: Take initial EnumWindows snapshot after hooks are armed
    let initial_windows = enumerate_trackable_windows().unwrap_or_default();
    let mut tracked_map = HashMap::new();
    let mut fg_hwnd = None;
    for w in &initial_windows {
        if w.is_focused {
            fg_hwnd = Some(w.hwnd);
        }
        tracked_map.insert(w.hwnd, w.clone());
    }

    WORKER_STATE.with(|cell| {
        *cell.borrow_mut() = Some(WorkerState {
            tx: tx.clone(),
            tracked: tracked_map,
            focused_hwnd: fg_hwnd,
        });
    });

    let _ = tx.send(WindowEvent::Snapshot(initial_windows));
    let _ = ready_tx.send(Ok(thread_id));

    // Step 3: Run Win32 message loop to drain queued events and listen for live updates
    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    // Cleanup hooks and thread-local state
    for hook in hooks {
        unsafe {
            let _ = UnhookWinEvent(hook);
        }
    }
    WORKER_STATE.with(|cell| {
        *cell.borrow_mut() = None;
    });
}

/// Signals the WinEvent worker thread to exit its message loop cleanly.
pub fn stop_win_event_worker(thread_id: u32) {
    unsafe {
        let _ = PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
    }
}
