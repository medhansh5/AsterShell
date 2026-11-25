//! AsterShell Global Hotkey Subsystem
//!
//! Provides isolated, thread-based Win32 hotkey registration (`RegisterHotKey`)
//! without UI thread blocking or polling.

use std::fmt;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::mpsc;
use std::thread;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HotkeyError {
    #[error("Hotkey is already registered by another application or process")]
    AlreadyRegistered,
    #[error("Failed to register hotkey: {0}")]
    RegistrationFailed(String),
    #[error("Hotkey worker thread failed to initialize: {0}")]
    ThreadInitFailed(String),
}

/// Configuration descriptor for a global hotkey shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HotkeyConfig {
    pub alt: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub win: bool,
    pub no_repeat: bool,
    pub vk_code: u32,
}

impl HotkeyConfig {
    /// Standard Alt + Space shortcut configuration for Spotlight search launcher.
    pub fn alt_space() -> Self {
        Self {
            alt: true,
            ctrl: false,
            shift: false,
            win: false,
            no_repeat: true,
            vk_code: 0x20, // VK_SPACE
        }
    }

    /// Custom shortcut constructor.
    pub fn new(vk_code: u32, alt: bool, ctrl: bool, shift: bool, win: bool) -> Self {
        Self {
            alt,
            ctrl,
            shift,
            win,
            no_repeat: true,
            vk_code,
        }
    }
}

static NEXT_HOTKEY_ID: AtomicI32 = AtomicI32::new(1001);

/// RAII handle managing the lifetime of a registered Win32 global hotkey.
///
/// Dropping this handle cleanly unregisters the hotkey and terminates the worker message pump.
pub struct GlobalHotkeyHandle {
    thread_handle: Option<thread::JoinHandle<()>>,
    pub thread_id: u32,
    pub hotkey_id: i32,
}

impl fmt::Debug for GlobalHotkeyHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GlobalHotkeyHandle")
            .field("thread_id", &self.thread_id)
            .field("hotkey_id", &self.hotkey_id)
            .finish()
    }
}

impl Drop for GlobalHotkeyHandle {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            use windows::Win32::Foundation::{LPARAM, WPARAM};
            use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};

            if self.thread_id != 0 {
                // SAFETY: PostThreadMessageW posts WM_QUIT to the hotkey message loop thread.
                unsafe {
                    let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
                }
            }
        }

        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }

        tracing::debug!(hotkey_id = self.hotkey_id, "Global hotkey cleanly released");
    }
}

/// Registers a global hotkey on a dedicated background message pump thread.
///
/// When the hotkey is triggered by the operating system, `callback` is invoked.
pub fn register_global_hotkey<F>(
    config: HotkeyConfig,
    callback: F,
) -> Result<GlobalHotkeyHandle, HotkeyError>
where
    F: Fn() + Send + 'static,
{
    let hotkey_id = NEXT_HOTKEY_ID.fetch_add(1, Ordering::SeqCst);

    #[cfg(windows)]
    {
        use windows::Win32::Foundation::WIN32_ERROR;
        use windows::Win32::System::Threading::GetCurrentThreadId;
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL,
            MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            GetMessageW, PeekMessageW, MSG, PM_NOREMOVE, WM_HOTKEY,
        };

        let (init_tx, init_rx) = mpsc::sync_channel::<Result<u32, HotkeyError>>(1);

        let thread_handle = thread::Builder::new()
            .name(format!("astershell-hotkey-{}", hotkey_id))
            .spawn(move || {
                let tid = unsafe { GetCurrentThreadId() };

                // Force message queue creation for this thread before registering hotkey
                let mut msg = MSG::default();
                unsafe {
                    let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
                }

                // Construct Win32 modifiers
                let mut modifiers = HOT_KEY_MODIFIERS(0);
                if config.alt {
                    modifiers |= MOD_ALT;
                }
                if config.ctrl {
                    modifiers |= MOD_CONTROL;
                }
                if config.shift {
                    modifiers |= MOD_SHIFT;
                }
                if config.win {
                    modifiers |= MOD_WIN;
                }
                if config.no_repeat {
                    modifiers |= MOD_NOREPEAT;
                }

                // SAFETY: Registering hotkey associated with this thread's message queue (hWnd = None).
                let reg_res = unsafe { RegisterHotKey(None, hotkey_id, modifiers, config.vk_code) };

                match reg_res {
                    Ok(()) => {
                        let _ = init_tx.send(Ok(tid));
                    }
                    Err(e) => {
                        let err_code = WIN32_ERROR(e.code().0 as u32);
                        let err = if err_code == WIN32_ERROR(1422) {
                            // ERROR_HOTKEY_ALREADY_REGISTERED
                            HotkeyError::AlreadyRegistered
                        } else {
                            HotkeyError::RegistrationFailed(format!("Code {:?}: {}", e.code(), e))
                        };
                        let _ = init_tx.send(Err(err));
                        return;
                    }
                }

                tracing::info!(
                    hotkey_id,
                    thread_id = tid,
                    "Global hotkey registered successfully"
                );

                // Message pump: processes WM_HOTKEY until WM_QUIT is received
                let mut msg = MSG::default();
                // SAFETY: GetMessageW blocks until a message arrives for this thread.
                while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
                    if msg.message == WM_HOTKEY && msg.wParam.0 as i32 == hotkey_id {
                        callback();
                    }
                }

                // SAFETY: Cleanly unregister the hotkey before thread exits.
                unsafe {
                    let _ = UnregisterHotKey(None, hotkey_id);
                }
                tracing::debug!(hotkey_id, "Hotkey message pump loop exited");
            })
            .map_err(|e| HotkeyError::ThreadInitFailed(e.to_string()))?;

        let thread_id = init_rx
            .recv()
            .map_err(|e| HotkeyError::ThreadInitFailed(e.to_string()))??;

        Ok(GlobalHotkeyHandle {
            thread_handle: Some(thread_handle),
            thread_id,
            hotkey_id,
        })
    }

    #[cfg(not(windows))]
    {
        let _ = (config, callback);
        Ok(GlobalHotkeyHandle {
            thread_handle: None,
            thread_id: 0,
            hotkey_id,
        })
    }
}
