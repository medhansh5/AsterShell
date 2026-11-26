//! AsterShell Floating Companion Taskbar (`taskbar`)
//!
//! Milestone M3A: Native Taskbar Window Foundation & Lifecycle.
//! Supervised floating companion window built with Slint and Win32 APIs,
//! supporting non-activating topmost presentation, DWM Mica composition,
//! signed physical placement at the top of the primary monitor, zero work-area
//! contraction, and dual IPC lifecycle coordination.

use astercore::{ComponentId, Result};
use diagnostics::{init_diagnostics, DiagnosticConfig};
use ipc::{
    transport::{self, NamedPipeServer},
    IpcRequest, IpcResponse, RequestEnvelope, ResponseEnvelope, PROTOCOL_VERSION_1,
};
use platform_windows::{
    apply_dark_mode_and_backdrop, calculate_taskbar_coordinates_with_dpi,
    configure_taskbar_native_styles, default_pipe_address, get_window_dpi, position_taskbar_window,
    taskbar_pipe_address, DwmBackdropType,
};
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

/// Applies native Win32 window styles, DWM Mica backdrop, and primary-monitor positioning.
pub fn apply_native_taskbar_configuration(win: &TaskbarWindow) -> Option<isize> {
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
        // 1. DWM Immersive Dark Mode and Mica backdrop with fallback
        if let Err(e) = apply_dark_mode_and_backdrop(hwnd, DwmBackdropType::Mica) {
            tracing::warn!(error = %e, "DWM Mica application returned fallback");
        }

        // 2. Configure native styles: WS_POPUP, WS_EX_TOPMOST, WS_EX_TOOLWINDOW, WS_EX_NOACTIVATE, !WS_EX_APPWINDOW
        if let Err(e) = configure_taskbar_native_styles(hwnd) {
            tracing::warn!(error = %e, "Native taskbar styles configuration failed");
        }

        // 3. Query DPI and calculate centered top coordinates on primary monitor
        let dpi = get_window_dpi(hwnd);
        if let Ok((x, y, phys_w, phys_h)) = calculate_taskbar_coordinates_with_dpi(720, 48, dpi) {
            if let Err(e) = position_taskbar_window(hwnd, x, y, phys_w, phys_h) {
                tracing::warn!(error = %e, "Failed to position taskbar window");
            }
        }
    }

    hwnd_opt
}

/// Handles incoming IPC control requests from supervisor or test runner on the taskbar control pipe.
#[cfg(windows)]
async fn handle_taskbar_control_connection(
    mut stream: NamedPipeServer,
    is_running: Arc<AtomicBool>,
) {
    loop {
        let req_res: Result<RequestEnvelope> = transport::read_framed_message(&mut stream).await;
        let req = match req_res {
            Ok(r) => r,
            Err(_) => break,
        };

        let is_shutdown = matches!(req.request, IpcRequest::Shutdown);

        let response = match req.request {
            IpcRequest::Handshake {
                client_version,
                client_name,
            } => {
                tracing::info!(
                    client = %client_name,
                    client_version,
                    "Taskbar control connection received handshake"
                );
                IpcResponse::HandshakeAck {
                    negotiated_version: PROTOCOL_VERSION_1,
                    server_pid: std::process::id(),
                }
            }
            IpcRequest::Ping { timestamp_ms } => IpcResponse::Pong { timestamp_ms },
            IpcRequest::Shutdown => {
                tracing::info!("Received Shutdown command on taskbar control pipe");
                is_running.store(false, Ordering::SeqCst);
                let _ = slint::invoke_from_event_loop(|| {
                    let _ = slint::quit_event_loop();
                });
                IpcResponse::Ack
            }
            _ => IpcResponse::Error("Unsupported operation on taskbar control pipe".into()),
        };

        let resp_envelope = ResponseEnvelope {
            request_id: req.request_id,
            response,
        };

        if transport::write_framed_message(&mut stream, &resp_envelope)
            .await
            .is_err()
        {
            break;
        }

        if is_shutdown {
            break;
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
    let mut control_pipe = taskbar_pipe_address();
    let mut supervisor_pipe = default_pipe_address();
    let mut connect_supervisor = true;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--pipe" if i + 1 < args.len() => {
                control_pipe = args[i + 1].clone();
                i += 1;
            }
            "--supervisor-pipe" if i + 1 < args.len() => {
                supervisor_pipe = args[i + 1].clone();
                i += 1;
            }
            "--no-supervisor" => {
                connect_supervisor = false;
            }
            _ => {}
        }
        i += 1;
    }

    let is_running = Arc::new(AtomicBool::new(true));

    // 1. Spawn IPC Control Pipe Server
    {
        let is_running_ctrl = is_running.clone();
        let pipe_addr = control_pipe.clone();
        tokio::spawn(async move {
            let mut is_first = true;
            while is_running_ctrl.load(Ordering::SeqCst) {
                #[cfg(windows)]
                {
                    let mut pipe_sec =
                        platform_windows::default_pipe_sddl().ok().and_then(|sddl| {
                            platform_windows::PipeSecurityDescriptor::from_sddl(&sddl).ok()
                        });
                    let sa_ptr = pipe_sec.as_mut().map(|s| s.as_raw_mut_ptr());

                    match transport::create_pipe_server_with_security(&pipe_addr, is_first, sa_ptr)
                    {
                        Ok(server) => {
                            is_first = false;
                            let running = is_running_ctrl.clone();
                            if server.connect().await.is_ok() {
                                tokio::spawn(handle_taskbar_control_connection(server, running));
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "Failed to create taskbar control pipe instance");
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                    }
                }

                #[cfg(not(windows))]
                {
                    let _ = pipe_addr;
                    break;
                }
            }
        });
    }

    // 2. Connect to ShellHost Supervisor via Named Pipe (AC-M3A-01)
    if connect_supervisor {
        let is_running_sup = is_running.clone();
        let sup_addr = supervisor_pipe.clone();
        tokio::spawn(async move {
            tracing::info!(supervisor_pipe = %sup_addr, "Connecting to supervisor IPC...");
            match transport::connect_pipe_client(&sup_addr).await {
                Ok(mut client) => {
                    let req = RequestEnvelope {
                        request_id: 1,
                        request: IpcRequest::Handshake {
                            client_version: PROTOCOL_VERSION_1,
                            client_name: "taskbar".to_string(),
                        },
                    };
                    if transport::write_framed_message(&mut client, &req)
                        .await
                        .is_ok()
                    {
                        let resp_res: Result<ResponseEnvelope> =
                            transport::read_framed_message(&mut client).await;
                        match resp_res {
                            Ok(resp) => {
                                tracing::info!(
                                    response = ?resp.response,
                                    "Taskbar supervisor mutual handshake confirmed"
                                );

                                // Read loop: wait for supervisor shutdown or pipe break
                                while is_running_sup.load(Ordering::SeqCst) {
                                    let incoming: Result<RequestEnvelope> =
                                        transport::read_framed_message(&mut client).await;
                                    match incoming {
                                        Ok(env) if matches!(env.request, IpcRequest::Shutdown) => {
                                            tracing::info!(
                                                "Supervisor requested taskbar shutdown via pipe"
                                            );
                                            is_running_sup.store(false, Ordering::SeqCst);
                                            let _ = slint::invoke_from_event_loop(|| {
                                                let _ = slint::quit_event_loop();
                                            });
                                            break;
                                        }
                                        Err(_) => {
                                            tracing::info!(
                                                "Supervisor pipe closed; terminating taskbar lifecycle"
                                            );
                                            is_running_sup.store(false, Ordering::SeqCst);
                                            let _ = slint::invoke_from_event_loop(|| {
                                                let _ = slint::quit_event_loop();
                                            });
                                            break;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "Failed to read supervisor handshake response");
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::info!(
                        error = %e,
                        "Supervisor offline or unreachable; continuing in standalone companion mode"
                    );
                }
            }
        });
    }

    // 3. Handle Ctrl+C signal
    {
        let is_running_ctrl = is_running.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                tracing::info!("Ctrl+C signal received, commencing graceful shutdown");
                is_running_ctrl.store(false, Ordering::SeqCst);
                let _ = slint::invoke_from_event_loop(|| {
                    let _ = slint::quit_event_loop();
                });
            }
        });
    }

    // 4. Instantiate and configure Slint Window
    let main_window = TaskbarWindow::new().map_err(|e| {
        astercore::AsterError::Platform(format!("Failed to create TaskbarWindow: {}", e))
    })?;

    main_window.show().map_err(|e| {
        astercore::AsterError::Platform(format!("Failed to show TaskbarWindow: {}", e))
    })?;

    // Initial native Win32 configuration
    apply_native_taskbar_configuration(&main_window);

    // Re-verify styles and placement on short timer once Winit message pump starts
    let win_weak = main_window.as_weak();
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::SingleShot,
        Duration::from_millis(50),
        move || {
            if let Some(win) = win_weak.upgrade() {
                apply_native_taskbar_configuration(&win);
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

    #[test]
    fn test_taskbar_pipe_address_format() {
        let pipe = taskbar_pipe_address();
        assert!(pipe.starts_with(r"\\.\pipe\astershell-taskbar-"));
    }
}
