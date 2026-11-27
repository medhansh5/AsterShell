//! AsterShell Floating Companion Taskbar (`taskbar`)
//!
//! Supervised floating companion window built with Slint and Win32 APIs,
//! supporting non-activating topmost presentation, DWM Mica composition,
//! multi-monitor placement, event-driven window tracking, pinned & running
//! app interactions, zero-polling system indicators, and keyboard navigation.

use astercore::{ComponentId, Result};
use diagnostics::{init_diagnostics, DiagnosticConfig};
use ipc::{
    transport, IpcRequest, IpcResponse, RequestEnvelope, ResponseEnvelope, PROTOCOL_VERSION_1,
};
use platform_windows::{
    default_pipe_address, register_global_hotkey, AudioMonitorHandle, HotkeyConfig,
    NetworkMonitorHandle, PowerMonitorHandle,
};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use window_tracker::WindowInfo;

slint::slint! {
    #[edition = "1.19"]
    export { TaskbarWindow, TaskbarAppItem } from "ui/taskbar.slint";
}

#[derive(Debug, Clone)]
struct PinnedApp {
    process_key: &'static str,
    title: &'static str,
    monogram: &'static str,
    launch_target: &'static str,
}

const DEFAULT_PINNED_APPS: &[PinnedApp] = &[
    PinnedApp {
        process_key: "explorer",
        title: "File Explorer",
        monogram: "FE",
        launch_target: "explorer.exe",
    },
    PinnedApp {
        process_key: "windowsterminal",
        title: "Terminal",
        monogram: "WT",
        launch_target: "wt.exe",
    },
    PinnedApp {
        process_key: "msedge",
        title: "Edge",
        monogram: "ED",
        launch_target: "msedge.exe",
    },
    PinnedApp {
        process_key: "notepad",
        title: "Notepad",
        monogram: "NP",
        launch_target: "notepad.exe",
    },
];

fn make_monogram(name: &str) -> String {
    let clean: String = name.chars().filter(|c| c.is_alphanumeric()).collect();
    let mut chars = clean.chars();
    match (chars.next(), chars.next()) {
        (Some(a), Some(b)) => format!("{}{}", a.to_ascii_uppercase(), b.to_ascii_uppercase()),
        (Some(a), None) => a.to_ascii_uppercase().to_string(),
        _ => "AP".to_string(),
    }
}

/// Builds the combined list of pinned apps and live running windows for the taskbar strip.
pub fn build_taskbar_app_items(windows: &[WindowInfo]) -> Vec<TaskbarAppItem> {
    let mut items = Vec::new();
    let mut matched_hwnds = std::collections::HashSet::new();

    for pinned in DEFAULT_PINNED_APPS {
        let matching_win = windows.iter().find(|w| {
            w.process_name.eq_ignore_ascii_case(pinned.process_key)
                && !matched_hwnds.contains(&w.hwnd)
        });

        if let Some(w) = matching_win {
            matched_hwnds.insert(w.hwnd);
            items.push(TaskbarAppItem {
                hwnd: w.hwnd as i32,
                title: w.title.clone().into(),
                monogram: pinned.monogram.into(),
                is_focused: w.is_focused,
                is_minimized: w.is_minimized,
                is_pinned: true,
                launch_target: if w.executable_path.is_empty() {
                    pinned.launch_target.into()
                } else {
                    w.executable_path.clone().into()
                },
            });
        } else {
            items.push(TaskbarAppItem {
                hwnd: 0,
                title: pinned.title.into(),
                monogram: pinned.monogram.into(),
                is_focused: false,
                is_minimized: false,
                is_pinned: true,
                launch_target: pinned.launch_target.into(),
            });
        }
    }

    for w in windows {
        if matched_hwnds.contains(&w.hwnd) {
            continue;
        }
        let label = if !w.process_name.is_empty() {
            &w.process_name
        } else {
            &w.title
        };
        items.push(TaskbarAppItem {
            hwnd: w.hwnd as i32,
            title: w.title.clone().into(),
            monogram: make_monogram(label).into(),
            is_focused: w.is_focused,
            is_minimized: w.is_minimized,
            is_pinned: false,
            launch_target: w.executable_path.clone().into(),
        });
    }

    items
}

/// Formats local wall-clock time (`HH:MM`), date (`DD MMM`), and returns seconds until next minute boundary.
pub fn format_local_clock() -> (String, String, u64) {
    #[cfg(windows)]
    {
        use windows::Win32::System::SystemInformation::GetLocalTime;
        let st = unsafe { GetLocalTime() };
        let month_name = match st.wMonth {
            1 => "Jan",
            2 => "Feb",
            3 => "Mar",
            4 => "Apr",
            5 => "May",
            6 => "Jun",
            7 => "Jul",
            8 => "Aug",
            9 => "Sep",
            10 => "Oct",
            11 => "Nov",
            12 => "Dec",
            _ => "",
        };
        let time_str = format!("{:02}:{:02}", st.wHour, st.wMinute);
        let date_str = format!("{} {}", st.wDay, month_name);
        let secs_remaining = (60u64).saturating_sub(st.wSecond as u64).max(1);
        (time_str, date_str, secs_remaining)
    }

    #[cfg(not(windows))]
    {
        ("12:00".to_string(), "Today".to_string(), 60)
    }
}

/// Applies native Win32 window styles, DWM Mica backdrop, target-monitor positioning,
/// and installs dynamic display change / DPI change subclass monitoring.
pub fn apply_taskbar_native_styles(win: &TaskbarWindow, monitor_target: &str) -> Option<isize> {
    use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use slint::winit_030::WinitWindowAccessor;

    let hwnd_opt = win
        .window()
        .with_winit_window(|winit_window| match winit_window.window_handle() {
            Ok(handle) => match handle.as_raw() {
                RawWindowHandle::Win32(win32) => Some(win32.hwnd.get()),
                _ => None,
            },
            Err(_) => None,
        })
        .flatten();

    if let Some(hwnd) = hwnd_opt {
        if let Err(e) = platform_windows::apply_dark_mode_and_backdrop(
            hwnd,
            platform_windows::DwmBackdropType::Mica,
        ) {
            tracing::warn!(error = %e, "DWM Mica application returned fallback");
        }

        if let Err(e) = platform_windows::configure_taskbar_native_styles(hwnd) {
            tracing::warn!(error = %e, "Failed to configure native taskbar window styles");
        }

        if let Err(e) =
            platform_windows::reposition_taskbar_on_monitor(hwnd, monitor_target, 720, 48)
        {
            tracing::warn!(
                error = %e,
                monitor_target,
                "Failed to position taskbar on target monitor"
            );
        }

        if let Err(e) =
            platform_windows::install_display_change_subclass(hwnd, monitor_target, 720, 48)
        {
            tracing::warn!(error = %e, "Failed to install display change subclass");
        }
    }

    hwnd_opt
}

fn launch_app_target(target: &str) {
    if target.trim().is_empty() {
        return;
    }
    let launch = if target.contains(':') && !target.contains('\\') {
        shell_api::LaunchTarget::Uri {
            uri: target.to_string(),
        }
    } else {
        shell_api::LaunchTarget::Executable {
            path: target.to_string(),
            arguments: Vec::new(),
            working_directory: None,
        }
    };

    if let Err(e) = shell_api::launch_target(&launch) {
        tracing::warn!(target, error = %e, "Failed to launch taskbar app target");
    }
}

/// Manages the supervisor connection lifecycle with bounded exponential backoff retry and reconnection.
pub async fn run_supervisor_connection_loop(
    pipe_address: String,
    is_running: Arc<AtomicBool>,
    shutdown_notify: Arc<tokio::sync::Notify>,
) {
    let mut attempt = 0u32;
    let initial_backoff = Duration::from_millis(100);
    let max_backoff = Duration::from_secs(2);

    while is_running.load(Ordering::SeqCst) {
        tracing::info!(supervisor_pipe = %pipe_address, attempt, "Connecting to supervisor IPC...");

        match transport::connect_pipe_client(&pipe_address).await {
            Ok(mut client) => {
                attempt = 0;
                let handshake_req = RequestEnvelope {
                    request_id: 1,
                    request: IpcRequest::Handshake {
                        client_version: PROTOCOL_VERSION_1,
                        client_name: "taskbar".to_string(),
                    },
                };

                if let Err(e) = transport::write_framed_message(&mut client, &handshake_req).await {
                    tracing::warn!(error = %e, "Failed to send supervisor handshake");
                } else {
                    let handshake_resp: Result<ResponseEnvelope> =
                        transport::read_framed_message(&mut client).await;
                    match handshake_resp {
                        Ok(resp) if matches!(resp.response, IpcResponse::HandshakeAck { .. }) => {
                            tracing::info!(
                                response = ?resp.response,
                                "Taskbar supervisor mutual handshake confirmed; entering supervised lifecycle"
                            );

                            loop {
                                tokio::select! {
                                    _ = shutdown_notify.notified() => {
                                        tracing::debug!("Shutdown notification received; closing supervisor connection");
                                        return;
                                    }
                                    incoming = transport::read_framed_message::<_, ResponseEnvelope>(&mut client) => {
                                        match incoming {
                                            Ok(env) => {
                                                if matches!(env.response, IpcResponse::ShutdownNotice) {
                                                    tracing::info!("Supervisor sent ShutdownNotice; commencing graceful taskbar termination");
                                                    is_running.store(false, Ordering::SeqCst);
                                                    let _ = slint::invoke_from_event_loop(|| {
                                                        let _ = slint::quit_event_loop();
                                                    });
                                                    return;
                                                }
                                            }
                                            Err(e) => {
                                                tracing::info!(error = %e, "Supervisor connection dropped; entering reconnection lifecycle");
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Ok(resp) => {
                            tracing::warn!(response = ?resp.response, "Supervisor handshake rejected");
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "Failed to read supervisor handshake response");
                        }
                    }
                }
            }
            Err(e) => {
                tracing::debug!(
                    error = %e,
                    attempt,
                    "Supervisor connection attempt failed; scheduling backoff retry"
                );
            }
        }

        if !is_running.load(Ordering::SeqCst) {
            break;
        }

        attempt = attempt.saturating_add(1);
        let backoff_multiplier = 2u64.pow(attempt.min(5));
        let jitter_ms = (std::process::id() as u64).wrapping_mul(attempt as u64) % 50;
        let delay = (initial_backoff * backoff_multiplier as u32
            + Duration::from_millis(jitter_ms))
        .min(max_backoff);

        tokio::select! {
            _ = shutdown_notify.notified() => {
                tracing::debug!("Shutdown notification interrupted reconnect backoff");
                break;
            }
            _ = tokio::time::sleep(delay) => {}
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    init_diagnostics(DiagnosticConfig {
        process_name: ComponentId::Taskbar.as_str(),
        default_level: "info",
    });

    tracing::info!("AsterShell Taskbar initializing...");

    let args: Vec<String> = env::args().collect();
    let mut supervisor_pipe = default_pipe_address();
    let mut connect_supervisor = true;
    let mut is_test_child = false;
    let mut cli_monitor: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--supervisor-pipe" if i + 1 < args.len() => {
                supervisor_pipe = args[i + 1].clone();
                i += 1;
            }
            "--no-supervisor" => {
                connect_supervisor = false;
            }
            "--test-child" => {
                is_test_child = true;
            }
            "--monitor" if i + 1 < args.len() => {
                cli_monitor = Some(args[i + 1].clone());
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }

    let config = configuration::load_config_from_path(&configuration::default_config_path())
        .unwrap_or_default();
    let monitor_target = cli_monitor.unwrap_or(config.taskbar.monitor);

    if is_test_child {
        tracing::info!(
            supervisor_pipe = %supervisor_pipe,
            connect_supervisor,
            "Running in test-child mode: monitoring supervisor lifecycle"
        );
        if connect_supervisor {
            if let Ok(mut client) = transport::connect_pipe_client(&supervisor_pipe).await {
                let req = RequestEnvelope {
                    request_id: 1,
                    request: IpcRequest::Handshake {
                        client_version: PROTOCOL_VERSION_1,
                        client_name: "taskbar-test-child".to_string(),
                    },
                };
                if transport::write_framed_message(&mut client, &req)
                    .await
                    .is_ok()
                {
                    let resp_res: Result<ResponseEnvelope> =
                        transport::read_framed_message(&mut client).await;
                    if let Ok(resp) = resp_res {
                        tracing::info!(response = ?resp.response, "Test-child handshake acknowledged");
                        loop {
                            let incoming: Result<ResponseEnvelope> =
                                transport::read_framed_message(&mut client).await;
                            match incoming {
                                Ok(env) if matches!(env.response, IpcResponse::ShutdownNotice) => {
                                    tracing::info!(
                                        "Test-child received ShutdownNotice; exiting cleanly"
                                    );
                                    break;
                                }
                                Err(_) => {
                                    tracing::info!(
                                        "Supervisor pipe closed; test-child exiting cleanly"
                                    );
                                    break;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        } else {
            loop {
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
        tracing::info!("AsterShell Taskbar test-child exited cleanly.");
        return Ok(());
    }

    let is_running = Arc::new(AtomicBool::new(true));
    let shutdown_notify = Arc::new(tokio::sync::Notify::new());

    if connect_supervisor {
        let is_running_sup = is_running.clone();
        let shutdown_notify_sup = shutdown_notify.clone();
        let sup_addr = supervisor_pipe.clone();
        tokio::spawn(async move {
            run_supervisor_connection_loop(sup_addr, is_running_sup, shutdown_notify_sup).await;
        });
    }

    {
        let is_running_ctrl = is_running.clone();
        let shutdown_notify_ctrl = shutdown_notify.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                tracing::info!("Ctrl+C signal received, commencing graceful shutdown");
                is_running_ctrl.store(false, Ordering::SeqCst);
                shutdown_notify_ctrl.notify_waiters();
                let _ = slint::invoke_from_event_loop(|| {
                    let _ = slint::quit_event_loop();
                });
            }
        });
    }

    let main_window = TaskbarWindow::new().map_err(|e| {
        astercore::AsterError::Platform(format!("Failed to create TaskbarWindow: {}", e))
    })?;

    // Wire UI Interaction Callbacks (M3E)
    main_window.on_search_clicked(|| {
        tracing::info!("Taskbar search pill clicked");
        launch_app_target("search-ui.exe");
    });

    main_window.on_item_clicked(|hwnd, is_focused, is_minimized, launch_target_str| {
        if hwnd != 0 {
            if let Err(e) = window_tracker::toggle_window(hwnd as isize, is_focused, is_minimized) {
                tracing::warn!(hwnd, error = %e, "Failed to toggle tracked window");
            }
        } else {
            launch_app_target(launch_target_str.as_str());
        }
    });

    main_window.on_item_middle_clicked(|launch_target_str| {
        launch_app_target(launch_target_str.as_str());
    });

    main_window.on_item_right_clicked(|hwnd, title| {
        if hwnd != 0 {
            tracing::info!(hwnd, title = %title, "Closing tracked window via taskbar right-click");
            let _ = window_tracker::close_window(hwnd as isize);
        }
    });

    main_window.on_volume_clicked(|| {
        launch_app_target("ms-settings:sound");
    });

    main_window.on_network_clicked(|| {
        launch_app_target("ms-settings:network");
    });

    main_window.show().map_err(|e| {
        astercore::AsterError::Platform(format!("Failed to show TaskbarWindow: {}", e))
    })?;

    let hwnd_opt = apply_taskbar_native_styles(&main_window, &monitor_target);

    // Register Win+Alt+T keyboard navigation hotkey (M3E)
    let _taskbar_hotkey = {
        let win_weak_hk = main_window.as_weak();
        let hwnd_for_hk = hwnd_opt;
        match register_global_hotkey(
            HotkeyConfig {
                alt: true,
                ctrl: false,
                shift: false,
                win: true,
                no_repeat: true,
                vk_code: 0x54, // 'T'
            },
            move || {
                let win_weak_inner = win_weak_hk.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(win) = win_weak_inner.upgrade() {
                        win.set_selected_index(0);
                    }
                    if let Some(hwnd) = hwnd_for_hk {
                        let _ = window_tracker::activate_window(hwnd);
                    }
                });
            },
        ) {
            Ok(h) => Some(h),
            Err(e) => {
                tracing::debug!(error = %e, "Win+Alt+T hotkey registration skipped");
                None
            }
        }
    };

    // Start Minute-Boundary Aligned Clock Task (M3E)
    {
        let (init_time, init_date, _) = format_local_clock();
        main_window.set_clock_time(init_time.into());
        main_window.set_clock_date(init_date.into());

        let win_weak_clk = main_window.as_weak();
        let is_running_clk = is_running.clone();
        let shutdown_clk = shutdown_notify.clone();
        tokio::spawn(async move {
            while is_running_clk.load(Ordering::SeqCst) {
                let (t_str, d_str, secs_until_next) = format_local_clock();
                let win_inner = win_weak_clk.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(win) = win_inner.upgrade() {
                        win.set_clock_time(t_str.into());
                        win.set_clock_date(d_str.into());
                    }
                });

                tokio::select! {
                    _ = shutdown_clk.notified() => break,
                    _ = tokio::time::sleep(Duration::from_secs(secs_until_next)) => {}
                }
            }
        });
    }

    // Start Event-Driven System Indicators (M3E)
    let _audio_monitor = match AudioMonitorHandle::start() {
        Ok((handle, rx)) => {
            let win_weak_a = main_window.as_weak();
            std::thread::Builder::new()
                .name("taskbar-audio-bridge".to_string())
                .spawn(move || {
                    while let Ok(state) = rx.recv() {
                        let win_inner = win_weak_a.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(win) = win_inner.upgrade() {
                                win.set_volume_text(format!("{}%", state.volume_percent).into());
                                win.set_is_muted(state.is_muted);
                            }
                        });
                    }
                })
                .ok();
            Some(handle)
        }
        Err(e) => {
            tracing::debug!(error = %e, "Audio monitor unavailable");
            None
        }
    };

    let _network_monitor = match NetworkMonitorHandle::start() {
        Ok((handle, rx)) => {
            let win_weak_n = main_window.as_weak();
            std::thread::Builder::new()
                .name("taskbar-network-bridge".to_string())
                .spawn(move || {
                    while let Ok(status) = rx.recv() {
                        let win_inner = win_weak_n.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(win) = win_inner.upgrade() {
                                win.set_network_label(status.label.into());
                                win.set_network_connected(status.is_connected);
                            }
                        });
                    }
                })
                .ok();
            Some(handle)
        }
        Err(e) => {
            tracing::debug!(error = %e, "Network monitor unavailable");
            None
        }
    };

    let _power_monitor = match PowerMonitorHandle::start() {
        Ok((handle, rx)) => {
            let win_weak_p = main_window.as_weak();
            std::thread::Builder::new()
                .name("taskbar-power-bridge".to_string())
                .spawn(move || {
                    while let Ok(status) = rx.recv() {
                        let win_inner = win_weak_p.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(win) = win_inner.upgrade() {
                                win.set_has_battery(status.has_battery);
                                let text = if status.is_charging {
                                    format!("+{}%", status.percent)
                                } else {
                                    format!("{}%", status.percent)
                                };
                                win.set_battery_text(text.into());
                            }
                        });
                    }
                })
                .ok();
            Some(handle)
        }
        Err(e) => {
            tracing::debug!(error = %e, "Power monitor unavailable");
            None
        }
    };

    // Start WinEvent-driven WindowTracker and bridge model to TaskbarWindow (M3C + M3E)
    let _window_tracker = match window_tracker::WindowTracker::start() {
        Ok((tracker, rx)) => {
            let win_weak_tracker = main_window.as_weak();
            std::thread::Builder::new()
                .name("taskbar-window-bridge".to_string())
                .spawn(move || {
                    let mut state = window_tracker::WindowTrackerState::new();
                    while let Ok(event) = rx.recv() {
                        state.apply_event(event);
                        let items = build_taskbar_app_items(&state.windows);
                        let win_weak_inner = win_weak_tracker.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(win) = win_weak_inner.upgrade() {
                                let model = std::rc::Rc::new(VecModel::from(items));
                                win.set_app_items(ModelRc::from(model));
                            }
                        });
                    }
                })
                .ok();
            Some(tracker)
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to start WindowTracker; continuing without window tracking");
            None
        }
    };

    // Re-verify styles and placement on short timer once Winit message pump starts
    let win_weak = main_window.as_weak();
    let target_for_timer = monitor_target.clone();
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::SingleShot,
        Duration::from_millis(50),
        move || {
            if let Some(win) = win_weak.upgrade() {
                apply_taskbar_native_styles(&win, &target_for_timer);
            }
        },
    );

    tracing::info!("Starting Slint UI event loop for AsterTaskbar...");
    slint::run_event_loop()
        .map_err(|e| astercore::AsterError::Platform(format!("Slint event loop error: {}", e)))?;

    is_running.store(false, Ordering::SeqCst);
    shutdown_notify.notify_waiters();
    tracing::info!("AsterShell Taskbar exited cleanly.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_taskbar_initialization() {
        assert_eq!(ComponentId::Taskbar.as_str(), "taskbar");
    }

    #[test]
    fn test_build_taskbar_app_items_merges_pinned_and_running() {
        let windows = vec![
            WindowInfo {
                hwnd: 1001,
                title: "Windows Terminal".to_string(),
                class_name: "CASCADIA_HOSTING_WINDOW_CLASS".to_string(),
                pid: 4200,
                process_name: "windowsterminal".to_string(),
                executable_path: r"C:\wt.exe".to_string(),
                is_minimized: false,
                is_focused: true,
                monitor_hmonitor: 1,
            },
            WindowInfo {
                hwnd: 2002,
                title: "Cargo Build".to_string(),
                class_name: "ConsoleWindowClass".to_string(),
                pid: 5100,
                process_name: "code".to_string(),
                executable_path: r"C:\code.exe".to_string(),
                is_minimized: false,
                is_focused: false,
                monitor_hmonitor: 1,
            },
        ];

        let items = build_taskbar_app_items(&windows);
        // 4 default pinned + 1 unpinned running ("code") = 5
        assert_eq!(items.len(), 5);

        let wt = items
            .iter()
            .find(|i| i.hwnd == 1001)
            .expect("Matched pinned WT");
        assert!(wt.is_pinned);
        assert!(wt.is_focused);
        assert_eq!(wt.monogram, "WT");

        let code = items
            .iter()
            .find(|i| i.hwnd == 2002)
            .expect("Unpinned code");
        assert!(!code.is_pinned);
        assert!(!code.is_focused);
        assert_eq!(code.monogram, "CO");
    }

    #[test]
    fn test_format_local_clock_valid_ranges() {
        let (time_str, date_str, secs) = format_local_clock();
        assert_eq!(time_str.len(), 5);
        assert!(time_str.contains(':'));
        assert!(!date_str.is_empty());
        assert!((1..=60).contains(&secs));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn test_supervisor_connection_loop_lifecycle_and_cancellation() {
        let test_pipe = format!(r"\\.\pipe\astershell-test-loop-{}", std::process::id());
        let is_running = Arc::new(AtomicBool::new(true));
        let shutdown_notify = Arc::new(tokio::sync::Notify::new());

        let is_running_clone = is_running.clone();
        let shutdown_notify_clone = shutdown_notify.clone();
        let pipe_addr = test_pipe.clone();

        let loop_handle = tokio::spawn(async move {
            run_supervisor_connection_loop(pipe_addr, is_running_clone, shutdown_notify_clone)
                .await;
        });

        tokio::time::sleep(Duration::from_millis(150)).await;

        let server = transport::create_pipe_server_with_security(&test_pipe, true, None)
            .expect("create server");

        let server_task = tokio::spawn(async move {
            if server.connect().await.is_ok() {
                let mut stream = server;
                let req_res: astercore::Result<RequestEnvelope> =
                    transport::read_framed_message(&mut stream).await;
                if let Ok(req) = req_res {
                    assert!(matches!(req.request, IpcRequest::Handshake { .. }));
                    let resp = ResponseEnvelope {
                        request_id: req.request_id,
                        response: IpcResponse::HandshakeAck {
                            negotiated_version: PROTOCOL_VERSION_1,
                            server_pid: std::process::id(),
                        },
                    };
                    let _ = transport::write_framed_message(&mut stream, &resp).await;

                    tokio::time::sleep(Duration::from_millis(50)).await;
                    let notice = ResponseEnvelope {
                        request_id: 0,
                        response: IpcResponse::ShutdownNotice,
                    };
                    let _ = transport::write_framed_message(&mut stream, &notice).await;
                }
            }
        });

        let _ = server_task.await;
        let _ = loop_handle.await;

        assert!(
            !is_running.load(Ordering::SeqCst),
            "is_running must be false after ShutdownNotice"
        );
    }

    #[tokio::test]
    async fn test_reconnect_backoff_shutdown_cancellation() {
        let test_pipe = format!(r"\\.\pipe\astershell-test-cancel-{}", std::process::id());
        let is_running = Arc::new(AtomicBool::new(true));
        let shutdown_notify = Arc::new(tokio::sync::Notify::new());

        let is_running_clone = is_running.clone();
        let shutdown_notify_clone = shutdown_notify.clone();
        let pipe_addr = test_pipe.clone();

        let start = std::time::Instant::now();
        let loop_handle = tokio::spawn(async move {
            run_supervisor_connection_loop(pipe_addr, is_running_clone, shutdown_notify_clone)
                .await;
        });

        tokio::time::sleep(Duration::from_millis(150)).await;
        is_running.store(false, Ordering::SeqCst);
        shutdown_notify.notify_waiters();

        let res = loop_handle.await;
        assert!(res.is_ok());
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "Cancellation must promptly abort backoff"
        );
    }
}
