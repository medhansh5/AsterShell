//! AsterShell System Tray & Notification-Area Integration
//!
//! Manages AsterShell's own persistent system tray icon in the Windows notification area
//! using `Shell_NotifyIconW` with a constant GUID, `NOTIFYICON_VERSION_4`, automatic
//! `TaskbarCreated` Explorer-restart recovery, and a native Win32 context menu with
//! `SetForegroundWindow` + `WM_NULL` focus-dismissal handling.
//!
//! In independent companion mode, Windows Explorer continues hosting third-party
//! notification-area icons; AsterShell never hooks or rehosts third-party tray icons.

use astercore::{AsterError, Result};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;

/// Constant GUID identifying AsterShell's tray icon across restarts.
/// {A57E254E-1140-4B89-9C1A-2026A57E0001}
pub const ASTERSHELL_TRAY_GUID_U128: u128 = 0xa57e254e_1140_4b89_9c1a_2026a57e0001;

/// Actions emitted by user interaction with the AsterShell system tray icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    /// Left-click / primary activation of the tray icon.
    OpenSearch,
    /// Context menu "Toggle Taskbar".
    ToggleTaskbar,
    /// Context menu "Exit AsterShell".
    Exit,
}

/// Handle to the active AsterShell system tray icon and its background message-pump thread.
///
/// Dropping this handle removes the tray icon (`NIM_DELETE`), destroys the hidden listener
/// window, and joins the background thread cleanly.
pub struct TrayIconHandle {
    #[cfg(windows)]
    worker_thread_id: u32,
    join_handle: Option<JoinHandle<()>>,
}

impl TrayIconHandle {
    /// Spawns AsterShell's persistent system tray icon and returns `(TrayIconHandle, Receiver<TrayAction>)`.
    pub fn start(tooltip: &str) -> Result<(Self, Receiver<TrayAction>)> {
        let (tx, rx) = mpsc::channel();

        #[cfg(windows)]
        {
            let tooltip_owned = tooltip.to_string();
            let (ready_tx, ready_rx) = mpsc::channel();
            let handle = std::thread::Builder::new()
                .name("astershell-tray-listener".to_string())
                .spawn(move || {
                    win32::run_tray_worker(&tooltip_owned, tx, ready_tx);
                })
                .map_err(|e| {
                    AsterError::Platform(format!("Failed to spawn tray listener thread: {}", e))
                })?;

            let thread_id = ready_rx.recv().map_err(|e| {
                AsterError::Platform(format!("Tray listener thread failed to initialize: {}", e))
            })??;

            tracing::info!(
                worker_thread_id = thread_id,
                "AsterShell system tray icon registered (NOTIFYICON_VERSION_4)"
            );

            Ok((
                Self {
                    worker_thread_id: thread_id,
                    join_handle: Some(handle),
                },
                rx,
            ))
        }

        #[cfg(not(windows))]
        {
            let _ = (tooltip, tx);
            Ok((Self { join_handle: None }, rx))
        }
    }
}

impl Drop for TrayIconHandle {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            win32::stop_tray_worker(self.worker_thread_id);
        }

        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
        tracing::debug!("AsterShell system tray icon removed cleanly");
    }
}

#[cfg(windows)]
mod win32 {
    use super::*;
    use std::cell::RefCell;
    use std::mem::size_of;
    use windows::core::{GUID, HSTRING, PCWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Shell::{
        Shell_NotifyIconW, NIF_GUID, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE,
        NIM_SETVERSION, NOTIFYICONDATAW, NOTIFYICON_VERSION_4,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
        DispatchMessageW, GetCursorPos, GetMessageW, LoadIconW, PostMessageW, PostThreadMessageW,
        RegisterClassW, RegisterWindowMessageW, SetForegroundWindow, TrackPopupMenuEx,
        TranslateMessage, UnregisterClassW, CW_USEDEFAULT, IDI_APPLICATION, MF_SEPARATOR,
        MF_STRING, MSG, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RIGHTBUTTON, WM_APP, WM_COMMAND,
        WM_CONTEXTMENU, WM_LBUTTONUP, WM_NULL, WM_QUIT, WM_RBUTTONUP, WNDCLASSW, WS_OVERLAPPED,
    };

    const WM_TRAY_CALLBACK: u32 = WM_APP + 42;
    const CMD_OPEN_SEARCH: usize = 1001;
    const CMD_TOGGLE_TASKBAR: usize = 1002;
    const CMD_EXIT_ASTERSHELL: usize = 1003;

    struct TrayThreadState {
        tx: Sender<TrayAction>,
        tooltip: String,
        taskbar_created_msg: u32,
        nid: NOTIFYICONDATAW,
    }

    thread_local! {
        static TRAY_STATE: RefCell<Option<TrayThreadState>> = const { RefCell::new(None) };
    }

    fn build_notify_icon_data(hwnd: HWND, tooltip: &str) -> NOTIFYICONDATAW {
        let mut nid = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_GUID,
            uCallbackMessage: WM_TRAY_CALLBACK,
            hIcon: unsafe { LoadIconW(None, IDI_APPLICATION).unwrap_or_default() },
            guidItem: GUID::from_u128(ASTERSHELL_TRAY_GUID_U128),
            ..Default::default()
        };

        let utf16: Vec<u16> = tooltip.encode_utf16().collect();
        let copy_len = utf16.len().min(nid.szTip.len().saturating_sub(1));
        nid.szTip[..copy_len].copy_from_slice(&utf16[..copy_len]);
        nid.szTip[copy_len] = 0;

        nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        nid
    }

    fn register_or_restore_tray_icon(nid: &mut NOTIFYICONDATAW) -> bool {
        unsafe {
            let added = Shell_NotifyIconW(NIM_ADD, nid).as_bool();
            if added {
                nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;
                let _ = Shell_NotifyIconW(NIM_SETVERSION, nid);
            }
            added
        }
    }

    unsafe fn show_tray_context_menu(hwnd: HWND) {
        let Ok(hmenu) = CreatePopupMenu() else {
            return;
        };

        let search_label = HSTRING::from("Open Search");
        let toggle_label = HSTRING::from("Toggle Taskbar");
        let exit_label = HSTRING::from("Exit AsterShell");

        let _ = AppendMenuW(
            hmenu,
            MF_STRING,
            CMD_OPEN_SEARCH,
            PCWSTR(search_label.as_ptr()),
        );
        let _ = AppendMenuW(
            hmenu,
            MF_STRING,
            CMD_TOGGLE_TASKBAR,
            PCWSTR(toggle_label.as_ptr()),
        );
        let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(
            hmenu,
            MF_STRING,
            CMD_EXIT_ASTERSHELL,
            PCWSTR(exit_label.as_ptr()),
        );

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);

        // Required Win32 focus fix so the menu dismisses cleanly when clicking outside.
        let _ = SetForegroundWindow(hwnd);
        let _ = TrackPopupMenuEx(
            hmenu,
            (TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON).0,
            pt.x,
            pt.y,
            hwnd,
            None,
        );
        let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(hmenu);
    }

    unsafe extern "system" fn tray_wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        let mut handled_taskbar_created = false;
        TRAY_STATE.with(|cell| {
            let mut borrow = cell.borrow_mut();
            if let Some(state) = borrow.as_mut() {
                if state.taskbar_created_msg != 0 && msg == state.taskbar_created_msg {
                    tracing::info!(
                        "TaskbarCreated broadcast received; restoring AsterShell tray icon"
                    );
                    state.nid = build_notify_icon_data(hwnd, &state.tooltip);
                    let _ = register_or_restore_tray_icon(&mut state.nid);
                    handled_taskbar_created = true;
                }
            }
        });
        if handled_taskbar_created {
            return LRESULT(0);
        }

        match msg {
            WM_TRAY_CALLBACK => {
                let notify_event = (lparam.0 & 0xFFFF) as u32;
                match notify_event {
                    WM_LBUTTONUP => {
                        TRAY_STATE.with(|cell| {
                            if let Some(state) = cell.borrow().as_ref() {
                                let _ = state.tx.send(TrayAction::OpenSearch);
                            }
                        });
                    }
                    WM_RBUTTONUP | WM_CONTEXTMENU => {
                        show_tray_context_menu(hwnd);
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_COMMAND => {
                let cmd_id = wparam.0 & 0xFFFF;
                TRAY_STATE.with(|cell| {
                    if let Some(state) = cell.borrow().as_ref() {
                        match cmd_id {
                            CMD_OPEN_SEARCH => {
                                let _ = state.tx.send(TrayAction::OpenSearch);
                            }
                            CMD_TOGGLE_TASKBAR => {
                                let _ = state.tx.send(TrayAction::ToggleTaskbar);
                            }
                            CMD_EXIT_ASTERSHELL => {
                                let _ = state.tx.send(TrayAction::Exit);
                            }
                            _ => {}
                        }
                    }
                });
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    pub fn run_tray_worker(tooltip: &str, tx: Sender<TrayAction>, ready_tx: Sender<Result<u32>>) {
        let thread_id = unsafe { GetCurrentThreadId() };
        let class_name = HSTRING::from("AsterShellTrayListenerWindow");

        let wc = WNDCLASSW {
            lpfnWndProc: Some(tray_wnd_proc),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };

        unsafe {
            let atom = RegisterClassW(&wc);
            if atom == 0 {
                let _ = ready_tx.send(Err(AsterError::Platform(
                    "RegisterClassW failed for AsterShellTrayListenerWindow".into(),
                )));
                return;
            }

            // Must be a hidden top-level window (not HWND_MESSAGE) to receive HWND_BROADCAST TaskbarCreated
            let hwnd_res = CreateWindowExW(
                Default::default(),
                PCWSTR(class_name.as_ptr()),
                PCWSTR(HSTRING::from("AsterShell Tray Listener").as_ptr()),
                WS_OVERLAPPED,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                0,
                0,
                None,
                None,
                None,
                None,
            );

            let hwnd = match hwnd_res {
                Ok(h) => h,
                Err(e) => {
                    let _ = UnregisterClassW(PCWSTR(class_name.as_ptr()), None);
                    let _ = ready_tx.send(Err(AsterError::Platform(format!(
                        "CreateWindowExW failed for tray listener: {}",
                        e
                    ))));
                    return;
                }
            };

            let taskbar_created_msg =
                RegisterWindowMessageW(PCWSTR(HSTRING::from("TaskbarCreated").as_ptr()));

            let mut nid = build_notify_icon_data(hwnd, tooltip);
            if !register_or_restore_tray_icon(&mut nid) {
                tracing::debug!(
                    "Initial Shell_NotifyIconW(NIM_ADD) returned false (may be headless/no Explorer shell); continuing listener"
                );
            }

            TRAY_STATE.with(|cell| {
                *cell.borrow_mut() = Some(TrayThreadState {
                    tx,
                    tooltip: tooltip.to_string(),
                    taskbar_created_msg,
                    nid,
                });
            });

            let _ = ready_tx.send(Ok(thread_id));

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            TRAY_STATE.with(|cell| {
                if let Some(state) = cell.borrow_mut().take() {
                    let _ = Shell_NotifyIconW(NIM_DELETE, &state.nid);
                }
            });

            let _ = DestroyWindow(hwnd);
            let _ = UnregisterClassW(PCWSTR(class_name.as_ptr()), None);
        }
    }

    pub fn stop_tray_worker(thread_id: u32) {
        unsafe {
            let _ = PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_tray_guid_constant_non_zero() {
        assert_ne!(ASTERSHELL_TRAY_GUID_U128, 0);
    }

    #[test]
    fn test_tray_icon_lifecycle_and_clean_drop() {
        let (tray, rx) = TrayIconHandle::start("AsterShell Companion").expect("Tray start");
        // Verify channel is open and non-blocking check works cleanly
        assert!(rx.recv_timeout(Duration::from_millis(50)).is_err());
        drop(tray);
    }
}
