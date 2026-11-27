//! AsterShell Window Tracking & Application Representation (`window-tracker`)
//!
//! Provides event-driven top-level window discovery via Win32 `SetWinEventHook`
//! and `EnumWindows` with strict hook-before-snapshot ordering, a 7-stage window
//! filtering pipeline, and safe window activation without `AttachThreadInput`.

pub mod filter;

#[cfg(windows)]
mod win32;

use astercore::Result;
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;

/// Represents a tracked top-level application window on the Windows desktop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowInfo {
    pub hwnd: isize,
    pub title: String,
    pub class_name: String,
    pub pid: u32,
    pub process_name: String,
    pub executable_path: String,
    pub is_minimized: bool,
    pub is_focused: bool,
    pub monitor_hmonitor: isize,
}

/// Incremental window lifecycle events emitted by `WindowTracker`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowEvent {
    /// Initial snapshot taken after WinEvent hooks are armed.
    Snapshot(Vec<WindowInfo>),
    /// A new trackable window appeared.
    WindowAdded(WindowInfo),
    /// An existing tracked window updated (title, focus, or minimize state).
    WindowUpdated(WindowInfo),
    /// A tracked window was destroyed, hidden, or cloaked.
    WindowRemoved(isize),
    /// Foreground focus shifted to the specified HWND (or none).
    FocusChanged(Option<isize>),
}

/// Deterministic in-memory window list state machine for taskbar synchronization.
#[derive(Debug, Default, Clone)]
pub struct WindowTrackerState {
    pub windows: Vec<WindowInfo>,
}

impl WindowTrackerState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies a `WindowEvent` idempotently, preserving insertion order.
    pub fn apply_event(&mut self, event: WindowEvent) {
        match event {
            WindowEvent::Snapshot(list) => {
                self.windows = list;
            }
            WindowEvent::WindowAdded(info) => {
                if let Some(existing) = self.windows.iter_mut().find(|w| w.hwnd == info.hwnd) {
                    *existing = info;
                } else {
                    self.windows.push(info);
                }
            }
            WindowEvent::WindowUpdated(info) => {
                if let Some(existing) = self.windows.iter_mut().find(|w| w.hwnd == info.hwnd) {
                    *existing = info;
                } else {
                    self.windows.push(info);
                }
            }
            WindowEvent::WindowRemoved(hwnd) => {
                self.windows.retain(|w| w.hwnd != hwnd);
            }
            WindowEvent::FocusChanged(focused_hwnd) => {
                for w in &mut self.windows {
                    w.is_focused = Some(w.hwnd) == focused_hwnd;
                }
            }
        }
    }
}

/// Active WinEvent-driven window tracker handle.
///
/// Dropping this handle cleanly signals the background STA thread to exit
/// (`WM_QUIT`), unhooks all WinEvent hooks, and joins the worker thread.
pub struct WindowTracker {
    #[cfg(windows)]
    worker_thread_id: u32,
    join_handle: Option<JoinHandle<()>>,
}

impl WindowTracker {
    /// Starts the window tracker on a dedicated STA background thread and returns
    /// `(WindowTracker, Receiver<WindowEvent>)`.
    ///
    /// Guarantees that `SetWinEventHook` is armed *before* the initial `EnumWindows`
    /// snapshot is captured.
    pub fn start() -> Result<(Self, Receiver<WindowEvent>)> {
        let (tx, rx) = mpsc::channel();

        #[cfg(windows)]
        {
            let (ready_tx, ready_rx) = mpsc::channel();
            let handle = std::thread::Builder::new()
                .name("window-tracker-winevent".to_string())
                .spawn(move || {
                    win32::run_win_event_worker(tx, ready_tx);
                })
                .map_err(|e| {
                    astercore::AsterError::Platform(format!(
                        "Failed to spawn window-tracker thread: {}",
                        e
                    ))
                })?;

            let thread_id = ready_rx.recv().map_err(|e| {
                astercore::AsterError::Platform(format!(
                    "Window-tracker worker failed during initialization: {}",
                    e
                ))
            })??;

            tracing::info!(
                worker_thread_id = thread_id,
                "WindowTracker started with hook-before-snapshot ordering"
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
            let _ = tx.send(WindowEvent::Snapshot(Vec::new()));
            Ok((Self { join_handle: None }, rx))
        }
    }
}

impl Drop for WindowTracker {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            win32::stop_win_event_worker(self.worker_thread_id);
        }

        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
        tracing::debug!("WindowTracker shut down cleanly");
    }
}

/// Enumerates all currently trackable top-level application windows.
pub fn enumerate_windows() -> Result<Vec<WindowInfo>> {
    #[cfg(windows)]
    {
        win32::enumerate_trackable_windows()
    }
    #[cfg(not(windows))]
    {
        Ok(Vec::new())
    }
}

/// Restores (if minimized) and activates a window without using `AttachThreadInput`.
pub fn activate_window(hwnd: isize) -> Result<()> {
    #[cfg(windows)]
    {
        win32::activate_window(hwnd)
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        Ok(())
    }
}

/// Minimizes a target window.
pub fn minimize_window(hwnd: isize) -> Result<()> {
    #[cfg(windows)]
    {
        win32::minimize_window(hwnd)
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        Ok(())
    }
}

/// Toggles a tracked window: minimizes if currently focused and unminimized, otherwise activates.
pub fn toggle_window(hwnd: isize, is_focused: bool, is_minimized: bool) -> Result<()> {
    if is_focused && !is_minimized {
        minimize_window(hwnd)
    } else {
        activate_window(hwnd)
    }
}

/// Requests graceful closure (`WM_CLOSE`) of a tracked window.
pub fn close_window(hwnd: isize) -> Result<()> {
    #[cfg(windows)]
    {
        win32::close_window(hwnd)
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        Ok(())
    }
}

pub fn crate_name() -> &'static str {
    "window-tracker"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn mock_window(hwnd: isize, title: &str) -> WindowInfo {
        WindowInfo {
            hwnd,
            title: title.to_string(),
            class_name: "TestClass".to_string(),
            pid: 1000 + hwnd as u32,
            process_name: "app".to_string(),
            executable_path: r"C:\app.exe".to_string(),
            is_minimized: false,
            is_focused: false,
            monitor_hmonitor: 1,
        }
    }

    #[test]
    fn test_crate_name() {
        assert_eq!(crate_name(), "window-tracker");
    }

    #[test]
    fn test_state_machine_idempotent_upsert_and_remove() {
        let mut state = WindowTrackerState::new();
        state.apply_event(WindowEvent::Snapshot(vec![
            mock_window(101, "Editor"),
            mock_window(102, "Terminal"),
        ]));
        assert_eq!(state.windows.len(), 2);

        // Idempotent add of existing HWND updates instead of duplicating
        let mut updated = mock_window(101, "Editor - main.rs");
        updated.is_focused = true;
        state.apply_event(WindowEvent::WindowAdded(updated));
        assert_eq!(state.windows.len(), 2);
        assert_eq!(state.windows[0].title, "Editor - main.rs");
        assert!(state.windows[0].is_focused);

        // Focus shift updates focus flags across all windows
        state.apply_event(WindowEvent::FocusChanged(Some(102)));
        assert!(!state.windows[0].is_focused);
        assert!(state.windows[1].is_focused);

        // Rapid create/destroy
        state.apply_event(WindowEvent::WindowAdded(mock_window(103, "Temp")));
        assert_eq!(state.windows.len(), 3);
        state.apply_event(WindowEvent::WindowRemoved(103));
        state.apply_event(WindowEvent::WindowRemoved(103)); // Duplicate remove is safe
        assert_eq!(state.windows.len(), 2);
    }

    #[test]
    fn test_live_enum_windows_snapshot() {
        let windows = enumerate_windows().expect("enumerate_windows should succeed");
        // Verify no excluded shell classes leaked into snapshot
        for w in &windows {
            assert!(
                !filter::EXCLUDED_SHELL_CLASSES
                    .iter()
                    .any(|&c| c.eq_ignore_ascii_case(&w.class_name)),
                "Excluded shell class {} leaked into enumerate_windows",
                w.class_name
            );
        }
    }

    #[test]
    fn test_tracker_start_snapshot_and_clean_drop() {
        let (tracker, rx) = WindowTracker::start().expect("WindowTracker::start should succeed");
        let first_event = rx
            .recv_timeout(Duration::from_secs(3))
            .expect("Must receive initial Snapshot event immediately");

        assert!(
            matches!(first_event, WindowEvent::Snapshot(_)),
            "First event from WindowTracker must be Snapshot (hook-before-snapshot ordering)"
        );

        // Clean drop must exit STA thread promptly without hanging
        drop(tracker);
    }
}
