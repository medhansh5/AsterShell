//! AsterShell Floating Companion Taskbar (`taskbar`)
//!
//! Milestone M3A: Native Taskbar Window Foundation & Lifecycle.
//! Supervised floating companion window built with Slint and Win32 APIs,
//! supporting non-activating topmost presentation, DWM Mica composition,
//! signed physical placement at the top of the primary monitor, zero work-area
//! contraction, and authenticated supervisor IPC lifecycle coordination.

use astercore::{ComponentId, Result};
use diagnostics::{init_diagnostics, DiagnosticConfig};
use ipc::{
    transport, IpcRequest, IpcResponse, RequestEnvelope, ResponseEnvelope, PROTOCOL_VERSION_1,
};
use platform_windows::default_pipe_address;
use slint::ComponentHandle;
use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

slint::slint! {
    #[edition = "1.19"]

    export component TaskbarWindow inherits Window {
        title: "AsterShell Taskbar";
        preferred-width: 720px;
        preferred-height: 48px;
        min-width: 720px;
        max-width: 720px;
        min-height: 48px;
        max-height: 48px;
        background: transparent;
        no-frame: true;

        in-out property <string> status_text: "✦ AsterShell • Floating Companion";
        in-out property <string> mode_badge: "M3A FOUNDATION";

        callback action_clicked();

        // Main floating companion capsule
        Rectangle {
            width: 100%;
            height: 100%;
            border-radius: 14px;
            background: rgba(20, 20, 26, 0.94);
            border-width: 1px;
            border-color: rgba(255, 255, 255, 0.12);

            HorizontalLayout {
                padding-left: 16px;
                padding-right: 16px;
                spacing: 12px;
                alignment: center;

                // Brand Pill / Launcher indicator
                HorizontalLayout {
                    spacing: 8px;
                    alignment: center;

                    Text {
                        text: "✦";
                        font-size: 15px;
                        color: #60a5fa;
                        vertical-alignment: center;
                    }

                    Text {
                        text: "AsterTaskbar";
                        font-size: 13px;
                        font-weight: 700;
                        color: #f4f4f6;
                        vertical-alignment: center;
                    }
                }

                // Vertical Divider
                Rectangle {
                    width: 1px;
                    height: 20px;
                    background: rgba(255, 255, 255, 0.15);
                }

                // Status Message / Companion Mode
                Text {
                    text <=> root.status_text;
                    font-size: 12px;
                    color: #94a3b8;
                    horizontal-stretch: 1;
                    vertical-alignment: center;
                    overflow: elide;
                }

                // Mode Badge
                Rectangle {
                    border-radius: 6px;
                    background: rgba(59, 130, 246, 0.18);
                    border-width: 1px;
                    border-color: rgba(59, 130, 246, 0.40);
                    height: 24px;

                    HorizontalLayout {
                        padding-left: 8px;
                        padding-right: 8px;
                        alignment: center;

                        Text {
                            text <=> root.mode_badge;
                            font-size: 10px;
                            font-weight: 600;
                            color: #93c5fd;
                            vertical-alignment: center;
                        }
                    }
                }
            }
        }
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
        // 1. DWM Immersive Dark Mode and Mica backdrop
        if let Err(e) = platform_windows::apply_dark_mode_and_backdrop(
            hwnd,
            platform_windows::DwmBackdropType::Mica,
        ) {
            tracing::warn!(error = %e, "DWM Mica application returned fallback");
        }

        // 2. Configure native styles: WS_POPUP, WS_EX_TOPMOST, WS_EX_TOOLWINDOW, WS_EX_NOACTIVATE, !WS_EX_APPWINDOW
        if let Err(e) = platform_windows::configure_taskbar_native_styles(hwnd) {
            tracing::warn!(error = %e, "Failed to configure native taskbar window styles");
        }

        // 3. Position on target monitor
        if let Err(e) =
            platform_windows::reposition_taskbar_on_monitor(hwnd, monitor_target, 720, 48)
        {
            tracing::warn!(
                error = %e,
                monitor_target,
                "Failed to position taskbar on target monitor"
            );
        }

        // 4. Install display change subclass for dynamic repositioning on WM_DISPLAYCHANGE / WM_DPICHANGED
        if let Err(e) =
            platform_windows::install_display_change_subclass(hwnd, monitor_target, 720, 48)
        {
            tracing::warn!(error = %e, "Failed to install display change subclass");
        }
    }

    hwnd_opt
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
                attempt = 0; // Reset backoff upon successful transport connection
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

                            // Active connection loop: wait for supervisor shutdown notice or connection drop
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
                                                break; // Breaks inner loop to trigger reconnection
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

        // Calculate bounded exponential backoff with jitter
        attempt = attempt.saturating_add(1);
        let backoff_multiplier = 2u64.pow(attempt.min(5));
        let jitter_ms = (std::process::id() as u64).wrapping_mul(attempt as u64) % 50;
        let delay = (initial_backoff * backoff_multiplier as u32
            + Duration::from_millis(jitter_ms))
        .min(max_backoff);
        tracing::debug!(
            delay_ms = delay.as_millis(),
            attempt,
            "Waiting before supervisor reconnect attempt"
        );

        // Interruptible retry delay
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

    tracing::info!("AsterShell Taskbar (M3A Foundation) initializing...");

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

    // In test-child mode, connect to supervisor IPC, acknowledge handshake, and wait for shutdown/pipe break
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
            } else {
                tracing::warn!("Test-child failed to connect to supervisor pipe");
            }
        } else {
            // Standalone test-child loop: stays alive in sleep loop until OS or Job Object terminates it (LIFECYCLE-01)
            loop {
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
        tracing::info!("AsterShell Taskbar test-child exited cleanly.");
        return Ok(());
    }

    let is_running = Arc::new(AtomicBool::new(true));
    let shutdown_notify = Arc::new(tokio::sync::Notify::new());

    // 1. Connect to ShellHost Supervisor via Named Pipe with reconnection (AC-M3A-01, SEC-01, INTEGRATION-01)
    if connect_supervisor {
        let is_running_sup = is_running.clone();
        let shutdown_notify_sup = shutdown_notify.clone();
        let sup_addr = supervisor_pipe.clone();
        tokio::spawn(async move {
            run_supervisor_connection_loop(sup_addr, is_running_sup, shutdown_notify_sup).await;
        });
    }

    // 2. Handle Ctrl+C signal
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

    // 3. Instantiate and configure Slint Window
    let main_window = TaskbarWindow::new().map_err(|e| {
        astercore::AsterError::Platform(format!("Failed to create TaskbarWindow: {}", e))
    })?;

    main_window.show().map_err(|e| {
        astercore::AsterError::Platform(format!("Failed to show TaskbarWindow: {}", e))
    })?;

    // Initial native Win32 configuration
    apply_taskbar_native_styles(&main_window, &monitor_target);

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

    #[cfg(windows)]
    #[tokio::test]
    async fn test_supervisor_connection_loop_lifecycle_and_cancellation() {
        let test_pipe = format!(r"\\.\pipe\astershell-test-loop-{}", std::process::id());
        let is_running = Arc::new(AtomicBool::new(true));
        let shutdown_notify = Arc::new(tokio::sync::Notify::new());

        let is_running_clone = is_running.clone();
        let shutdown_notify_clone = shutdown_notify.clone();
        let pipe_addr = test_pipe.clone();

        // 1. Start connection loop before server exists (tests initial backoff retry)
        let loop_handle = tokio::spawn(async move {
            run_supervisor_connection_loop(pipe_addr, is_running_clone, shutdown_notify_clone)
                .await;
        });

        // Allow at least one transient connection failure / backoff wait
        tokio::time::sleep(Duration::from_millis(150)).await;

        // 2. Now start server and bind
        let server = transport::create_pipe_server_with_security(&test_pipe, true, None)
            .expect("create server");

        let server_task = tokio::spawn(async move {
            if server.connect().await.is_ok() {
                let mut stream = server;
                // Read handshake
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

                    // Send ShutdownNotice to gracefully terminate connection
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

        // Cancel during backoff wait
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
