//! AsterShell Shell Host Daemon and Process Supervisor

use astercore::{ComponentId, Result};
use configuration::{default_config_path, load_config_from_path, Config};
use diagnostics::{init_diagnostics, DiagnosticConfig};
use ipc::{
    transport::{self, NamedPipeServer},
    IpcRequest, IpcResponse, RequestEnvelope, ResponseEnvelope, PROTOCOL_VERSION_1,
};
use platform_windows::{default_pipe_address, JobObject, TrayAction, TrayIconHandle};
use process_supervisor::ProcessSupervisor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;

pub struct ShellHostDaemon {
    config: Config,
    supervisor: ProcessSupervisor,
    pipe_address: String,
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
    is_running: Arc<AtomicBool>,
}

impl ShellHostDaemon {
    pub fn new() -> Result<Self> {
        Self::new_with_pipe(&default_pipe_address())
    }

    pub fn new_with_pipe(pipe_address: &str) -> Result<Self> {
        let config_path = default_config_path();
        let config = load_config_from_path(&config_path)?;

        let job_object = JobObject::create("AsterShellHostJob")?;
        let supervisor = ProcessSupervisor::new(job_object);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        Ok(Self {
            config,
            supervisor,
            pipe_address: pipe_address.to_string(),
            shutdown_tx,
            shutdown_rx,
            is_running: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn pipe_address(&self) -> &str {
        &self.pipe_address
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn authorize_pid(&self, pid: u32) {
        self.supervisor.pid_registry().authorize(pid);
    }

    pub fn authorize_admin_pid(&self, pid: u32) {
        self.supervisor.pid_registry().authorize_admin(pid);
    }

    pub fn supervisor(&self) -> &ProcessSupervisor {
        &self.supervisor
    }

    pub fn supervisor_mut(&mut self) -> &mut ProcessSupervisor {
        &mut self.supervisor
    }

    pub fn register_component(&mut self, spec: process_supervisor::ProcessSpec) {
        self.supervisor.register(spec);
    }

    pub fn spawn_component(&mut self, id: ComponentId) -> Result<u32> {
        self.supervisor.spawn(id)
    }

    pub fn register_default_components(&mut self) {
        if self.config.taskbar.enabled {
            let taskbar_exe = resolve_component_executable(ComponentId::Taskbar);
            let spec = process_supervisor::ProcessSpec {
                id: ComponentId::Taskbar,
                executable: taskbar_exe,
                args: vec!["--supervisor-pipe".to_string(), self.pipe_address.clone()],
                restart_policy: self.config.supervisor.restart_policy.clone(),
            };
            self.supervisor.register(spec);
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        self.is_running.store(true, Ordering::SeqCst);
        tracing::info!(
            pipe = %self.pipe_address,
            companion_mode = self.config.general.companion_mode,
            "AsterShell Shell Host supervisor daemon starting..."
        );

        let shutdown_tx = self.shutdown_tx.clone();
        let mut shutdown_rx_ipc = self.shutdown_rx.clone();
        let pipe_addr = self.pipe_address.clone();
        let is_running = self.is_running.clone();
        let config_json = serde_json::to_string(&self.config).unwrap_or_default();
        let pid_registry = self.supervisor.pid_registry();

        // 1. Channel to deterministically signal when the IPC listener is bound and ready to accept connections
        let (listener_ready_tx, listener_ready_rx) = tokio::sync::oneshot::channel::<()>();
        let (toggle_taskbar_tx, _) = watch::channel(0u64);

        // 2. Spawn IPC Named Pipe Listener
        let toggle_for_ipc = toggle_taskbar_tx.clone();
        let ipc_task = tokio::spawn(async move {
            let mut is_first = true;
            let mut ready_sender = Some(listener_ready_tx);

            while is_running.load(Ordering::SeqCst) {
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
                            // Signal that the listener endpoint is bound and ready
                            if let Some(tx) = ready_sender.take() {
                                let _ = tx.send(());
                            }

                            let s_tx = shutdown_tx.clone();
                            let s_rx = shutdown_rx_ipc.clone();
                            let t_rx = toggle_for_ipc.subscribe();
                            let cfg_json = config_json.clone();
                            let registry = pid_registry.clone();

                            tokio::select! {
                                res = server.connect() => {
                                    if res.is_ok() {
                                        tokio::spawn(handle_client_connection(server, s_tx, s_rx, t_rx, cfg_json, registry));
                                    }
                                }
                                _ = shutdown_rx_ipc.changed() => {
                                    tracing::debug!("IPC listener received shutdown watch signal");
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to create IPC server instance: {}", e);
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                    }
                }

                #[cfg(not(windows))]
                {
                    let _ = pipe_addr;
                    if let Some(tx) = ready_sender.take() {
                        let _ = tx.send(());
                    }
                    break;
                }
            }
            tracing::info!("IPC server listener terminated cleanly");
        });

        // 3. Wait deterministically for IPC listener readiness before spawning child components (INTEGRATION-01)
        if listener_ready_rx.await.is_err() {
            tracing::warn!("IPC listener readiness channel closed prematurely");
        } else {
            tracing::info!(pipe = %self.pipe_address, "IPC server endpoint bound and ready for connections");
        }

        // 4. Register default configured components (INTEGRATION-01)
        self.register_default_components();

        // 5. Automatically spawn enabled components
        if self.config.taskbar.enabled {
            match self.supervisor.spawn(ComponentId::Taskbar) {
                Ok(pid) => {
                    tracing::info!(pid, "Supervised taskbar companion spawned successfully");
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Taskbar companion auto-start skipped (executable unavailable or headless)");
                }
            }
        }

        // 6. Start persistent system tray icon with Explorer TaskbarCreated recovery (M3D)
        let _tray_handle = match TrayIconHandle::start("AsterShell Companion") {
            Ok((handle, tray_rx)) => {
                let shutdown_tx_tray = self.shutdown_tx.clone();
                let toggle_tx_tray = toggle_taskbar_tx.clone();
                std::thread::Builder::new()
                    .name("shell-host-tray-events".to_string())
                    .spawn(move || {
                        while let Ok(action) = tray_rx.recv() {
                            let is_exit = matches!(action, TrayAction::Exit);
                            if let Err(e) = dispatch_tray_action(
                                action,
                                &shutdown_tx_tray,
                                &toggle_tx_tray,
                                &launch_spotlight_search,
                            ) {
                                tracing::warn!(error = %e, "Tray action dispatch failed");
                            }
                            if is_exit {
                                break;
                            }
                        }
                    })
                    .ok();
                Some(handle)
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to initialize system tray icon; continuing headless");
                None
            }
        };

        // Supervisor main loop waiting for signal
        let mut shutdown_rx = self.shutdown_rx.clone();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Ctrl+C interrupt received. Commencing graceful shutdown...");
            }
            _ = shutdown_rx.changed() => {
                tracing::info!("Shutdown signal received from IPC watch channel.");
            }
        }

        self.is_running.store(false, Ordering::SeqCst);
        let _ = self.shutdown_tx.send(true);

        // Shutdown supervised child processes cleanly with bounded grace period
        self.supervisor
            .shutdown_all(Duration::from_millis(1500))
            .await?;
        let _ = ipc_task.await;

        tracing::info!("AsterShell Shell Host daemon shutdown complete.");
        Ok(())
    }

    pub fn trigger_shutdown(&self) {
        self.is_running.store(false, Ordering::SeqCst);
        let _ = self.shutdown_tx.send(true);
    }
}

fn launch_spotlight_search() -> Result<()> {
    let exe = resolve_component_executable(ComponentId::SearchUi);
    std::process::Command::new(&exe).spawn().map_err(|e| {
        astercore::AsterError::Platform(format!("Failed to launch {}: {}", exe.display(), e))
    })?;
    Ok(())
}

pub fn dispatch_tray_action(
    action: TrayAction,
    shutdown_tx: &watch::Sender<bool>,
    toggle_tx: &watch::Sender<u64>,
    open_search_fn: &dyn Fn() -> Result<()>,
) -> Result<()> {
    match action {
        TrayAction::OpenSearch => {
            tracing::info!("System tray action: OpenSearch");
            open_search_fn()
        }
        TrayAction::ToggleTaskbar => {
            tracing::info!("System tray action: ToggleTaskbar");
            let next = toggle_tx.borrow().wrapping_add(1);
            let _ = toggle_tx.send(next);
            Ok(())
        }
        TrayAction::Exit => {
            tracing::info!("System tray action: Exit AsterShell requested");
            let _ = shutdown_tx.send(true);
            Ok(())
        }
    }
}

#[cfg(windows)]
async fn handle_client_connection(
    mut stream: NamedPipeServer,
    shutdown_tx: watch::Sender<bool>,
    mut shutdown_rx: watch::Receiver<bool>,
    mut toggle_rx: watch::Receiver<u64>,
    config_json: String,
    pid_registry: process_supervisor::SupervisedPidRegistry,
) {
    let mut is_authenticated = false;
    let mut client_role = process_supervisor::ClientRole::SupervisedChild;
    let mut client_pid = 0u32;

    loop {
        let mut is_shutdown = false;

        tokio::select! {
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    tracing::info!(client_pid, "Host shutting down; sending ShutdownNotice to client and closing pipe");
                    let notice = ResponseEnvelope {
                        request_id: 0,
                        response: IpcResponse::ShutdownNotice,
                    };
                    let _ = transport::write_framed_message(&mut stream, &notice).await;
                    break;
                }
            }
            _ = toggle_rx.changed() => {
                if is_authenticated {
                    let notice = ResponseEnvelope {
                        request_id: 0,
                        response: IpcResponse::ToggleTaskbarNotice,
                    };
                    let _ = transport::write_framed_message(&mut stream, &notice).await;
                }
            }
            req_res = transport::read_framed_message::<_, RequestEnvelope>(&mut stream) => {
                let req = match req_res {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::debug!("Client disconnected or invalid frame: {}", e);
                        break;
                    }
                };

                let response = match req.request {
                    IpcRequest::Handshake {
                        client_version,
                        ref client_name,
                    } => {
                        if is_authenticated {
                            tracing::warn!(
                                client = %client_name,
                                "Protocol violation: connection already authenticated"
                            );
                            let resp_envelope = ResponseEnvelope {
                                request_id: req.request_id,
                                response: IpcResponse::Error(
                                    "Protocol violation: connection already authenticated".to_string(),
                                ),
                            };
                            let _ = transport::write_framed_message(&mut stream, &resp_envelope).await;
                            break;
                        }

                        let pid = platform_windows::get_pipe_client_pid(&stream).unwrap_or(0);
                        if !pid_registry.is_authorized(pid) {
                            tracing::warn!(
                                client = %client_name,
                                client_pid = pid,
                                "Mutual authentication failed: PID {} is not registered as a supervised child",
                                pid
                            );
                            let resp_envelope = ResponseEnvelope {
                                request_id: req.request_id,
                                response: IpcResponse::Error(format!(
                                    "Mutual authentication failed: PID {} is not a supervised child process",
                                    pid
                                )),
                            };
                            let _ = transport::write_framed_message(&mut stream, &resp_envelope).await;
                            break;
                        }

                        client_pid = pid;
                        client_role = pid_registry.get_role(pid).unwrap_or(process_supervisor::ClientRole::SupervisedChild);
                        is_authenticated = true;
                        tracing::info!(
                            client = %client_name,
                            client_version,
                            client_pid,
                            ?client_role,
                            "IPC Client authenticated against supervised registry and connected"
                        );
                        IpcResponse::HandshakeAck {
                            negotiated_version: PROTOCOL_VERSION_1,
                            server_pid: std::process::id(),
                        }
                    }
                    _ if !is_authenticated => {
                        tracing::warn!(
                            "Unauthenticated request rejected: connection must authenticate via Handshake first"
                        );
                        let resp_envelope = ResponseEnvelope {
                            request_id: req.request_id,
                            response: IpcResponse::Error(
                                "Authentication required: connection must authenticate via Handshake first"
                                    .to_string(),
                            ),
                        };
                        let _ = transport::write_framed_message(&mut stream, &resp_envelope).await;
                        break;
                    }
                    IpcRequest::Ping { timestamp_ms } => IpcResponse::Pong { timestamp_ms },
                    IpcRequest::GetConfig => IpcResponse::ConfigJson(config_json.clone()),
                    IpcRequest::DiscoverApps => {
                        let apps = application_catalog::discover_installed_applications();
                        IpcResponse::DiscoveredApps(apps)
                    }
                    IpcRequest::Shutdown => {
                        if client_role == process_supervisor::ClientRole::AdminController {
                            tracing::warn!("Admin controller requested shell host shutdown");
                            is_shutdown = true;
                            IpcResponse::Ack
                        } else {
                            tracing::warn!(
                                client_pid,
                                "Unauthorized shutdown attempt rejected from supervised child"
                            );
                            IpcResponse::Error(
                                "Permission denied: supervised child processes are not authorized to trigger host shutdown"
                                    .to_string(),
                            )
                        }
                    }
                };

                let resp_envelope = ResponseEnvelope {
                    request_id: req.request_id,
                    response,
                };

                if let Err(e) = transport::write_framed_message(&mut stream, &resp_envelope).await {
                    tracing::error!("Failed to send IPC response: {}", e);
                    break;
                }

                if is_shutdown {
                    let _ = shutdown_tx.send(true);
                    break;
                }
            }
        }
    }
}

/// Resolves the filesystem path to a workspace component executable.
pub fn resolve_component_executable(component: ComponentId) -> std::path::PathBuf {
    let exe_name = if cfg!(windows) {
        format!("{}.exe", component.as_str())
    } else {
        component.as_str().to_string()
    };

    if let Ok(mut current) = std::env::current_exe() {
        current.pop();
        let target = current.join(&exe_name);
        if target.exists() {
            return target;
        }
        // In cargo test or target/debug/deps, target may be in parent directory
        let parent_target = current.join("..").join(&exe_name);
        if parent_target.exists() {
            if let Ok(canon) = parent_target.canonicalize() {
                return canon;
            }
            return parent_target;
        }
    }

    std::path::PathBuf::from(exe_name)
}

#[tokio::main]
async fn main() -> Result<()> {
    init_diagnostics(DiagnosticConfig {
        process_name: ComponentId::ShellHost.as_str(),
        default_level: "info",
    });

    let mut daemon = ShellHostDaemon::new()?;
    daemon.run().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_host_daemon_initialization() {
        let daemon = ShellHostDaemon::new();
        assert!(daemon.is_ok(), "ShellHostDaemon must initialize cleanly");
        let d = daemon.unwrap();
        assert!(d.pipe_address().starts_with(r"\\.\pipe\astershell-ipc-"));
        assert_eq!(d.config().version, 1);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn test_shell_host_ipc_lifecycle() {
        let test_pipe = format!(r"\\.\pipe\astershell-test-lifecycle-{}", std::process::id());
        let mut daemon =
            ShellHostDaemon::new_with_pipe(&test_pipe).expect("Failed to initialize daemon");
        daemon.config.taskbar.enabled = false;
        daemon.authorize_admin_pid(std::process::id());
        let pipe_addr = daemon.pipe_address().to_string();

        let daemon_task = tokio::spawn(async move {
            let _ = daemon.run().await;
        });

        // Allow daemon pipe server to bind
        tokio::time::sleep(Duration::from_millis(150)).await;

        let mut client = transport::connect_pipe_client(&pipe_addr)
            .await
            .expect("Client failed to connect to daemon pipe");

        // 1. Handshake
        let req = RequestEnvelope {
            request_id: 1,
            request: IpcRequest::Handshake {
                client_version: PROTOCOL_VERSION_1,
                client_name: "admin-controller".into(),
            },
        };
        transport::write_framed_message(&mut client, &req)
            .await
            .unwrap();
        let resp: ResponseEnvelope = transport::read_framed_message(&mut client).await.unwrap();
        assert_eq!(resp.request_id, 1);
        assert!(matches!(resp.response, IpcResponse::HandshakeAck { .. }));

        // 2. Ping / Pong
        let ping_req = RequestEnvelope {
            request_id: 2,
            request: IpcRequest::Ping {
                timestamp_ms: 99999,
            },
        };
        transport::write_framed_message(&mut client, &ping_req)
            .await
            .unwrap();
        let pong_resp: ResponseEnvelope =
            transport::read_framed_message(&mut client).await.unwrap();
        assert_eq!(pong_resp.request_id, 2);
        assert_eq!(
            pong_resp.response,
            IpcResponse::Pong {
                timestamp_ms: 99999
            }
        );

        // 3. Application Discovery over IPC
        let apps_req = RequestEnvelope {
            request_id: 3,
            request: IpcRequest::DiscoverApps,
        };
        transport::write_framed_message(&mut client, &apps_req)
            .await
            .unwrap();
        let apps_resp: ResponseEnvelope =
            transport::read_framed_message(&mut client).await.unwrap();
        assert_eq!(apps_resp.request_id, 3);
        match apps_resp.response {
            IpcResponse::DiscoveredApps(apps) => {
                assert!(
                    !apps.is_empty(),
                    "Discovered apps over IPC must not be empty"
                );
            }
            other => panic!("Expected DiscoveredApps, got {:?}", other),
        }

        // 4. Shutdown request from authorized Admin Controller
        let shutdown_req = RequestEnvelope {
            request_id: 4,
            request: IpcRequest::Shutdown,
        };
        transport::write_framed_message(&mut client, &shutdown_req)
            .await
            .unwrap();
        let shutdown_resp: ResponseEnvelope =
            transport::read_framed_message(&mut client).await.unwrap();
        assert_eq!(shutdown_resp.request_id, 4);
        assert_eq!(shutdown_resp.response, IpcResponse::Ack);

        // Wait for daemon task to exit cleanly
        let daemon_res = daemon_task.await;
        assert!(daemon_res.is_ok(), "Daemon task must exit cleanly");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn test_shell_host_rejects_shutdown_from_supervised_child() {
        let test_pipe = format!(r"\\.\pipe\astershell-test-child-sd-{}", std::process::id());
        let mut daemon =
            ShellHostDaemon::new_with_pipe(&test_pipe).expect("Failed to initialize daemon");
        daemon.config.taskbar.enabled = false;
        // Authorize with ordinary SupervisedChild role
        daemon.authorize_pid(std::process::id());
        let pipe_addr = daemon.pipe_address().to_string();

        let daemon_task = tokio::spawn(async move {
            let _ = daemon.run().await;
        });

        tokio::time::sleep(Duration::from_millis(150)).await;

        let mut client = transport::connect_pipe_client(&pipe_addr)
            .await
            .expect("Client failed to connect to daemon pipe");

        // 1. Handshake as supervised child
        let handshake_req = RequestEnvelope {
            request_id: 1,
            request: IpcRequest::Handshake {
                client_version: PROTOCOL_VERSION_1,
                client_name: "taskbar-child".into(),
            },
        };
        transport::write_framed_message(&mut client, &handshake_req)
            .await
            .unwrap();
        let handshake_resp: ResponseEnvelope =
            transport::read_framed_message(&mut client).await.unwrap();
        assert!(matches!(
            handshake_resp.response,
            IpcResponse::HandshakeAck { .. }
        ));

        // 2. Child attempts Shutdown request (SEC-01)
        let child_sd_req = RequestEnvelope {
            request_id: 2,
            request: IpcRequest::Shutdown,
        };
        transport::write_framed_message(&mut client, &child_sd_req)
            .await
            .unwrap();
        let child_sd_resp: ResponseEnvelope =
            transport::read_framed_message(&mut client).await.unwrap();
        assert_eq!(child_sd_resp.request_id, 2);

        match child_sd_resp.response {
            IpcResponse::Error(msg) => {
                assert!(
                    msg.contains("Permission denied"),
                    "Expected Permission denied error for supervised child shutdown, got: {}",
                    msg
                );
            }
            other => panic!(
                "Expected IpcResponse::Error for child shutdown, got {:?}",
                other
            ),
        }

        // Verify daemon is still alive and running
        assert!(
            !daemon_task.is_finished(),
            "Daemon must remain running after unauthorized child shutdown attempt"
        );

        daemon_task.abort();
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn test_shell_host_shutdown_notice_sent_to_clients() {
        let test_pipe = format!(r"\\.\pipe\astershell-test-sd-notice-{}", std::process::id());
        let mut daemon =
            ShellHostDaemon::new_with_pipe(&test_pipe).expect("Failed to initialize daemon");
        daemon.config.taskbar.enabled = false;
        daemon.authorize_pid(std::process::id());
        let pipe_addr = daemon.pipe_address().to_string();

        let trigger_sd = daemon.shutdown_tx.clone();

        let daemon_task = tokio::spawn(async move {
            let _ = daemon.run().await;
        });

        tokio::time::sleep(Duration::from_millis(150)).await;

        let mut client = transport::connect_pipe_client(&pipe_addr)
            .await
            .expect("Client failed to connect to daemon pipe");

        // Handshake
        let handshake_req = RequestEnvelope {
            request_id: 1,
            request: IpcRequest::Handshake {
                client_version: PROTOCOL_VERSION_1,
                client_name: "test-client".into(),
            },
        };
        transport::write_framed_message(&mut client, &handshake_req)
            .await
            .unwrap();
        let handshake_resp: ResponseEnvelope =
            transport::read_framed_message(&mut client).await.unwrap();
        assert!(matches!(
            handshake_resp.response,
            IpcResponse::HandshakeAck { .. }
        ));

        // Trigger daemon shutdown
        let _ = trigger_sd.send(true);

        // Client should receive ShutdownNotice before pipe closes
        let notice_resp: ResponseEnvelope =
            transport::read_framed_message(&mut client).await.unwrap();
        assert_eq!(notice_resp.response, IpcResponse::ShutdownNotice);

        let _ = daemon_task.await;
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn test_shell_host_rejects_unauthorized_client_pid() {
        let test_pipe = format!(r"\\.\pipe\astershell-test-unauth-{}", std::process::id());
        let mut daemon =
            ShellHostDaemon::new_with_pipe(&test_pipe).expect("Failed to initialize daemon");
        daemon.config.taskbar.enabled = false;
        // Explicitly authorize a different PID so that std::process::id() is rejected
        daemon.authorize_pid(999999);
        let pipe_addr = daemon.pipe_address().to_string();

        let daemon_task = tokio::spawn(async move {
            let _ = daemon.run().await;
        });

        tokio::time::sleep(Duration::from_millis(150)).await;

        let mut client = transport::connect_pipe_client(&pipe_addr)
            .await
            .expect("Client failed to connect to daemon pipe");

        // Handshake from unauthorized client
        let req = RequestEnvelope {
            request_id: 1,
            request: IpcRequest::Handshake {
                client_version: PROTOCOL_VERSION_1,
                client_name: "unauthorized-rogue-client".into(),
            },
        };
        transport::write_framed_message(&mut client, &req)
            .await
            .unwrap();

        let resp: ResponseEnvelope = transport::read_framed_message(&mut client).await.unwrap();
        assert_eq!(resp.request_id, 1);
        match resp.response {
            IpcResponse::Error(msg) => {
                assert!(
                    msg.contains("Mutual authentication failed"),
                    "Expected mutual authentication failure message, got: {}",
                    msg
                );
            }
            other => panic!(
                "Expected IpcResponse::Error for unauthorized client, got {:?}",
                other
            ),
        }

        // Verify that server closes the pipe after rejecting
        let read_after: Result<ResponseEnvelope> =
            transport::read_framed_message(&mut client).await;
        assert!(
            read_after.is_err(),
            "Pipe stream should be disconnected by server"
        );

        // Clean up daemon
        daemon_task.abort();
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn test_shell_host_rejects_ping_before_handshake() {
        let test_pipe = format!(
            r"\\.\pipe\astershell-test-unauth-ping-{}",
            std::process::id()
        );
        let mut daemon =
            ShellHostDaemon::new_with_pipe(&test_pipe).expect("Failed to initialize daemon");
        daemon.config.taskbar.enabled = false;
        daemon.authorize_pid(std::process::id());
        let pipe_addr = daemon.pipe_address().to_string();

        let daemon_task = tokio::spawn(async move {
            let _ = daemon.run().await;
        });

        tokio::time::sleep(Duration::from_millis(150)).await;

        let mut client = transport::connect_pipe_client(&pipe_addr)
            .await
            .expect("Client failed to connect to daemon pipe");

        let req = RequestEnvelope {
            request_id: 1,
            request: IpcRequest::Ping {
                timestamp_ms: 12345,
            },
        };
        transport::write_framed_message(&mut client, &req)
            .await
            .unwrap();

        let resp: ResponseEnvelope = transport::read_framed_message(&mut client).await.unwrap();
        assert_eq!(resp.request_id, 1);
        match resp.response {
            IpcResponse::Error(msg) => {
                assert!(
                    msg.contains("Authentication required"),
                    "Expected authentication required error, got: {}",
                    msg
                );
            }
            other => panic!("Expected IpcResponse::Error, got {:?}", other),
        }

        let read_after: Result<ResponseEnvelope> =
            transport::read_framed_message(&mut client).await;
        assert!(read_after.is_err(), "Pipe should be disconnected by server");

        daemon_task.abort();
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn test_shell_host_rejects_get_config_before_handshake() {
        let test_pipe = format!(
            r"\\.\pipe\astershell-test-unauth-cfg-{}",
            std::process::id()
        );
        let mut daemon =
            ShellHostDaemon::new_with_pipe(&test_pipe).expect("Failed to initialize daemon");
        daemon.config.taskbar.enabled = false;
        daemon.authorize_pid(std::process::id());
        let pipe_addr = daemon.pipe_address().to_string();

        let daemon_task = tokio::spawn(async move {
            let _ = daemon.run().await;
        });

        tokio::time::sleep(Duration::from_millis(150)).await;

        let mut client = transport::connect_pipe_client(&pipe_addr)
            .await
            .expect("Client failed to connect to daemon pipe");

        let req = RequestEnvelope {
            request_id: 1,
            request: IpcRequest::GetConfig,
        };
        transport::write_framed_message(&mut client, &req)
            .await
            .unwrap();

        let resp: ResponseEnvelope = transport::read_framed_message(&mut client).await.unwrap();
        assert_eq!(resp.request_id, 1);
        match resp.response {
            IpcResponse::Error(msg) => {
                assert!(
                    msg.contains("Authentication required"),
                    "Expected authentication required error, got: {}",
                    msg
                );
            }
            other => panic!("Expected IpcResponse::Error, got {:?}", other),
        }

        daemon_task.abort();
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn test_shell_host_rejects_discover_apps_before_handshake() {
        let test_pipe = format!(
            r"\\.\pipe\astershell-test-unauth-apps-{}",
            std::process::id()
        );
        let mut daemon =
            ShellHostDaemon::new_with_pipe(&test_pipe).expect("Failed to initialize daemon");
        daemon.config.taskbar.enabled = false;
        daemon.authorize_pid(std::process::id());
        let pipe_addr = daemon.pipe_address().to_string();

        let daemon_task = tokio::spawn(async move {
            let _ = daemon.run().await;
        });

        tokio::time::sleep(Duration::from_millis(150)).await;

        let mut client = transport::connect_pipe_client(&pipe_addr)
            .await
            .expect("Client failed to connect to daemon pipe");

        let req = RequestEnvelope {
            request_id: 1,
            request: IpcRequest::DiscoverApps,
        };
        transport::write_framed_message(&mut client, &req)
            .await
            .unwrap();

        let resp: ResponseEnvelope = transport::read_framed_message(&mut client).await.unwrap();
        assert_eq!(resp.request_id, 1);
        match resp.response {
            IpcResponse::Error(msg) => {
                assert!(
                    msg.contains("Authentication required"),
                    "Expected authentication required error, got: {}",
                    msg
                );
            }
            other => panic!("Expected IpcResponse::Error, got {:?}", other),
        }

        daemon_task.abort();
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn test_shell_host_rejects_shutdown_before_handshake() {
        let test_pipe = format!(
            r"\\.\pipe\astershell-test-unauth-shut-{}",
            std::process::id()
        );
        let mut daemon =
            ShellHostDaemon::new_with_pipe(&test_pipe).expect("Failed to initialize daemon");
        daemon.config.taskbar.enabled = false;
        daemon.authorize_pid(std::process::id());
        let pipe_addr = daemon.pipe_address().to_string();

        let daemon_task = tokio::spawn(async move {
            let _ = daemon.run().await;
        });

        tokio::time::sleep(Duration::from_millis(150)).await;

        let mut client = transport::connect_pipe_client(&pipe_addr)
            .await
            .expect("Client failed to connect to daemon pipe");

        let req = RequestEnvelope {
            request_id: 1,
            request: IpcRequest::Shutdown,
        };
        transport::write_framed_message(&mut client, &req)
            .await
            .unwrap();

        let resp: ResponseEnvelope = transport::read_framed_message(&mut client).await.unwrap();
        assert_eq!(resp.request_id, 1);
        match resp.response {
            IpcResponse::Error(msg) => {
                assert!(
                    msg.contains("Authentication required"),
                    "Expected authentication required error, got: {}",
                    msg
                );
            }
            other => panic!("Expected IpcResponse::Error, got {:?}", other),
        }

        assert!(
            !daemon_task.is_finished(),
            "Daemon must not shut down from unauthenticated request"
        );

        daemon_task.abort();
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn test_shell_host_rejects_duplicate_handshake() {
        let test_pipe = format!(
            r"\\.\pipe\astershell-test-dup-handshake-{}",
            std::process::id()
        );
        let mut daemon =
            ShellHostDaemon::new_with_pipe(&test_pipe).expect("Failed to initialize daemon");
        daemon.config.taskbar.enabled = false;
        daemon.authorize_pid(std::process::id());
        let pipe_addr = daemon.pipe_address().to_string();

        let daemon_task = tokio::spawn(async move {
            let _ = daemon.run().await;
        });

        tokio::time::sleep(Duration::from_millis(150)).await;

        let mut client = transport::connect_pipe_client(&pipe_addr)
            .await
            .expect("Client failed to connect to daemon pipe");

        // 1st Handshake: should succeed
        let req1 = RequestEnvelope {
            request_id: 1,
            request: IpcRequest::Handshake {
                client_version: PROTOCOL_VERSION_1,
                client_name: "valid-client".into(),
            },
        };
        transport::write_framed_message(&mut client, &req1)
            .await
            .unwrap();
        let resp1: ResponseEnvelope = transport::read_framed_message(&mut client).await.unwrap();
        assert_eq!(resp1.request_id, 1);
        assert!(matches!(resp1.response, IpcResponse::HandshakeAck { .. }));

        // 2nd Handshake on same connection: protocol violation
        let req2 = RequestEnvelope {
            request_id: 2,
            request: IpcRequest::Handshake {
                client_version: PROTOCOL_VERSION_1,
                client_name: "valid-client".into(),
            },
        };
        transport::write_framed_message(&mut client, &req2)
            .await
            .unwrap();
        let resp2: ResponseEnvelope = transport::read_framed_message(&mut client).await.unwrap();
        assert_eq!(resp2.request_id, 2);
        match resp2.response {
            IpcResponse::Error(msg) => {
                assert!(
                    msg.contains("already authenticated"),
                    "Expected already authenticated error, got: {}",
                    msg
                );
            }
            other => panic!("Expected IpcResponse::Error, got {:?}", other),
        }

        let read_after: Result<ResponseEnvelope> =
            transport::read_framed_message(&mut client).await;
        assert!(
            read_after.is_err(),
            "Pipe should be closed after protocol violation"
        );

        daemon_task.abort();
    }

    #[test]
    fn test_resolve_component_executable() {
        let path = resolve_component_executable(ComponentId::Taskbar);
        assert!(path.to_string_lossy().contains("taskbar"));
    }

    #[test]
    fn test_taskbar_registration_enabled() {
        let mut daemon = ShellHostDaemon::new().expect("ShellHostDaemon::new");
        assert!(daemon.config().taskbar.enabled);
        daemon.register_default_components();
        assert!(
            daemon.supervisor().status(ComponentId::Taskbar).is_some(),
            "Taskbar must be registered when enabled in config"
        );
        let spec = daemon
            .supervisor()
            .spec(ComponentId::Taskbar)
            .expect("Taskbar spec must exist");
        assert!(
            spec.args.contains(&"--supervisor-pipe".to_string()),
            "Taskbar spec must contain --supervisor-pipe argument"
        );
    }

    #[test]
    fn test_taskbar_registration_disabled() {
        let mut daemon = ShellHostDaemon::new().expect("ShellHostDaemon::new");
        daemon.config = configuration::Config {
            taskbar: configuration::TaskbarConfig {
                enabled: false,
                ..Default::default()
            },
            ..daemon.config().clone()
        };
        daemon.register_default_components();
        assert!(
            daemon.supervisor().status(ComponentId::Taskbar).is_none(),
            "Taskbar must NOT be registered when disabled in config"
        );
    }

    #[test]
    fn test_tray_action_dispatch_all_handlers() {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let (toggle_tx, toggle_rx) = watch::channel(0u64);
        let search_count = std::sync::atomic::AtomicUsize::new(0);

        dispatch_tray_action(TrayAction::OpenSearch, &shutdown_tx, &toggle_tx, &|| {
            search_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .expect("OpenSearch dispatch");
        assert_eq!(search_count.load(Ordering::SeqCst), 1);
        assert!(!*shutdown_rx.borrow(), "OpenSearch must not shut down host");

        dispatch_tray_action(TrayAction::ToggleTaskbar, &shutdown_tx, &toggle_tx, &|| {
            Ok(())
        })
        .expect("ToggleTaskbar dispatch");
        assert_eq!(*toggle_rx.borrow(), 1);
        assert!(
            !*shutdown_rx.borrow(),
            "ToggleTaskbar must not shut down host"
        );

        // Failing OpenSearch returns error cleanly without terminating host
        let err_res =
            dispatch_tray_action(TrayAction::OpenSearch, &shutdown_tx, &toggle_tx, &|| {
                Err(astercore::AsterError::Platform("missing binary".into()))
            });
        assert!(err_res.is_err());
        assert!(!*shutdown_rx.borrow());

        dispatch_tray_action(TrayAction::Exit, &shutdown_tx, &toggle_tx, &|| Ok(()))
            .expect("Exit dispatch");
        assert!(*shutdown_rx.borrow(), "Exit must signal shutdown");
    }
}
