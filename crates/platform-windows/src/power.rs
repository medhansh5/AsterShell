//! AsterShell Power & Battery Status Query & Notification Subsystem
//!
//! Queries `GetSystemPowerStatus` and registers `RegisterPowerSettingNotification`
//! on a hidden message window for zero-polling AC/battery state updates.

use astercore::Result;
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;

/// Represents current system power and battery state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PowerStatus {
    pub has_battery: bool,
    pub percent: u8,
    pub is_charging: bool,
    pub is_ac_online: bool,
}

impl Default for PowerStatus {
    fn default() -> Self {
        Self {
            has_battery: false,
            percent: 100,
            is_charging: false,
            is_ac_online: true,
        }
    }
}

/// Queries current system power/battery status via `GetSystemPowerStatus`.
pub fn query_power_status() -> Result<PowerStatus> {
    #[cfg(windows)]
    {
        win32::query_power_status_win32()
    }
    #[cfg(not(windows))]
    {
        Ok(PowerStatus::default())
    }
}

/// Handle to an active `RegisterPowerSettingNotification` listener thread.
pub struct PowerMonitorHandle {
    #[cfg(windows)]
    worker_thread_id: u32,
    join_handle: Option<JoinHandle<()>>,
}

impl PowerMonitorHandle {
    /// Starts an event-driven power monitor emitting `PowerStatus` on changes.
    pub fn start() -> Result<(Self, Receiver<PowerStatus>)> {
        let (tx, rx) = mpsc::channel();

        #[cfg(windows)]
        {
            let (ready_tx, ready_rx) = mpsc::channel();
            let handle = std::thread::Builder::new()
                .name("astershell-power-monitor".to_string())
                .spawn(move || {
                    win32::run_power_monitor(tx, ready_tx);
                })
                .map_err(|e| {
                    astercore::AsterError::Platform(format!(
                        "Failed to spawn power monitor thread: {}",
                        e
                    ))
                })?;

            let thread_id = ready_rx.recv().map_err(|e| {
                astercore::AsterError::Platform(format!(
                    "Power monitor thread failed during initialization: {}",
                    e
                ))
            })??;

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
            let _ = tx.send(PowerStatus::default());
            Ok((Self { join_handle: None }, rx))
        }
    }
}

impl Drop for PowerMonitorHandle {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            win32::stop_power_monitor(self.worker_thread_id);
        }
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(windows)]
mod win32 {
    use super::*;
    use std::cell::RefCell;
    use std::sync::mpsc::Sender;
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::Power::{
        GetSystemPowerStatus, RegisterPowerSettingNotification, UnregisterPowerSettingNotification,
        SYSTEM_POWER_STATUS,
    };
    use windows::Win32::System::SystemServices::{
        GUID_ACDC_POWER_SOURCE, GUID_BATTERY_PERCENTAGE_REMAINING,
    };
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
        PostThreadMessageW, RegisterClassW, TranslateMessage, UnregisterClassW, CW_USEDEFAULT,
        DEVICE_NOTIFY_WINDOW_HANDLE, HWND_MESSAGE, MSG, WM_POWERBROADCAST, WM_QUIT, WNDCLASSW,
    };

    thread_local! {
        static POWER_TX: RefCell<Option<Sender<PowerStatus>>> = const { RefCell::new(None) };
    }

    pub fn query_power_status_win32() -> Result<PowerStatus> {
        let mut sps = SYSTEM_POWER_STATUS::default();
        unsafe {
            GetSystemPowerStatus(&mut sps).map_err(|e| {
                astercore::AsterError::Platform(format!("GetSystemPowerStatus failed: {}", e))
            })?;
        }

        // BatteryFlag bit 128 indicates no system battery
        let has_battery = (sps.BatteryFlag & 128) == 0 && sps.BatteryFlag != 255;
        let is_charging = (sps.BatteryFlag & 8) != 0;
        let is_ac_online = sps.ACLineStatus == 1;
        let percent = if sps.BatteryLifePercent <= 100 {
            sps.BatteryLifePercent
        } else {
            100
        };

        Ok(PowerStatus {
            has_battery,
            percent,
            is_charging,
            is_ac_online,
        })
    }

    unsafe extern "system" fn power_wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_POWERBROADCAST {
            if let Ok(status) = query_power_status_win32() {
                POWER_TX.with(|cell| {
                    if let Some(tx) = cell.borrow().as_ref() {
                        let _ = tx.send(status);
                    }
                });
            }
            return LRESULT(1);
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }

    pub fn run_power_monitor(tx: Sender<PowerStatus>, ready_tx: Sender<Result<u32>>) {
        let thread_id = unsafe { GetCurrentThreadId() };
        let class_name = HSTRING::from("AsterShellPowerListenerWindow");

        let wc = WNDCLASSW {
            lpfnWndProc: Some(power_wnd_proc),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };

        unsafe {
            let atom = RegisterClassW(&wc);
            if atom == 0 {
                let _ = ready_tx.send(Err(astercore::AsterError::Platform(
                    "RegisterClassW failed for AsterShellPowerListenerWindow".into(),
                )));
                return;
            }

            let hwnd = match CreateWindowExW(
                Default::default(),
                PCWSTR(class_name.as_ptr()),
                PCWSTR(HSTRING::from("AsterShell Power Listener").as_ptr()),
                Default::default(),
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                0,
                0,
                HWND_MESSAGE,
                None,
                None,
                None,
            ) {
                Ok(h) => h,
                Err(e) => {
                    let _ = UnregisterClassW(PCWSTR(class_name.as_ptr()), None);
                    let _ = ready_tx.send(Err(astercore::AsterError::Platform(format!(
                        "CreateWindowExW failed for power listener: {}",
                        e
                    ))));
                    return;
                }
            };

            let h_acdc = RegisterPowerSettingNotification(
                windows::Win32::Foundation::HANDLE(hwnd.0),
                &GUID_ACDC_POWER_SOURCE,
                DEVICE_NOTIFY_WINDOW_HANDLE,
            )
            .ok();
            let h_batt = RegisterPowerSettingNotification(
                windows::Win32::Foundation::HANDLE(hwnd.0),
                &GUID_BATTERY_PERCENTAGE_REMAINING,
                DEVICE_NOTIFY_WINDOW_HANDLE,
            )
            .ok();

            let initial = query_power_status_win32().unwrap_or_default();
            let _ = tx.send(initial);

            POWER_TX.with(|cell| {
                *cell.borrow_mut() = Some(tx);
            });
            let _ = ready_tx.send(Ok(thread_id));

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            if let Some(h) = h_acdc {
                let _ = UnregisterPowerSettingNotification(h);
            }
            if let Some(h) = h_batt {
                let _ = UnregisterPowerSettingNotification(h);
            }
            POWER_TX.with(|cell| {
                *cell.borrow_mut() = None;
            });
            let _ = DestroyWindow(hwnd);
            let _ = UnregisterClassW(PCWSTR(class_name.as_ptr()), None);
        }
    }

    pub fn stop_power_monitor(thread_id: u32) {
        unsafe {
            let _ = PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_power_status_returns_valid_percent() {
        let status = query_power_status().expect("query_power_status should succeed");
        assert!(status.percent <= 100);
    }

    #[test]
    fn test_power_monitor_start_and_clean_drop() {
        let (handle, rx) = PowerMonitorHandle::start().expect("PowerMonitorHandle::start");
        let initial = rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("Must receive initial power status");
        assert!(initial.percent <= 100);
        drop(handle);
    }
}
