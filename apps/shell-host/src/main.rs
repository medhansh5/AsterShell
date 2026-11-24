//! AsterShell Shell Host Daemon and Process Supervisor

use astercore::{ComponentId, Result};
use configuration::{default_config_path, load_config_from_path, Config};
use diagnostics::{init_diagnostics, DiagnosticConfig};
use ipc::{
    transport::{self, NamedPipeServer},
    IpcRequest, IpcResponse, RequestEnvelope, ResponseEnvelope, PROTOCOL_VERSION_1,
};
use platform_windows::{default_pipe_address, JobObject};
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
        let config_path = default_config_path();
        let config = load_config_from_path(&config_path)?;

        let job_object = JobObject::create("AsterShellHostJob")?;
        let supervisor = ProcessSupervisor::new(job_object);
        let pipe_address = default_pipe_address();
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        Ok(Self {
            config,
            supervisor,
            pipe_address,
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

        // Spawn IPC Named Pipe Listener
        let ipc_task = tokio::spawn(async move {
            let mut is_first = true;
            while is_running.load(Ordering::SeqCst) {
                #[cfg(windows)]
                {
                    match transport::create_pipe_server(&pipe_addr, is_first) {
                        Ok(server) => {
                            is_first = false;
                            let s_tx = shutdown_tx.clone();
                            let cfg_json = config_json.clone();

                            tokio::select! {
                                res = server.connect() => {
                                    if res.is_ok() {
                                        tokio::spawn(handle_client_connection(server, s_tx, cfg_json));
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
                    break;
                }
            }
            tracing::info!("IPC server listener terminated cleanly");
        });

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

        // Shutdown supervised child processes cleanly
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

#[cfg(windows)]
async fn handle_client_connection(
    mut stream: NamedPipeServer,
    shutdown_tx: watch::Sender<bool>,
    config_json: String,
) {
    loop {
        let req_res: Result<RequestEnvelope> = transport::read_framed_message(&mut stream).await;
        let req = match req_res {
            Ok(r) => r,
            Err(e) => {
                tracing::debug!("Client disconnected or invalid frame: {}", e);
                break;
            }
        };

        let is_shutdown = matches!(req.request, IpcRequest::Shutdown);

        let response = match req.request {
            IpcRequest::Handshake {
                client_version,
                ref client_name,
            } => {
                tracing::info!(client = %client_name, client_version, "IPC Client connected");
                IpcResponse::HandshakeAck {
                    negotiated_version: PROTOCOL_VERSION_1,
                    server_pid: std::process::id(),
                }
            }
            IpcRequest::Ping { timestamp_ms } => IpcResponse::Pong { timestamp_ms },
            IpcRequest::GetConfig => IpcResponse::ConfigJson(config_json.clone()),
            IpcRequest::DiscoverApps => {
                let apps = application_catalog::discover_installed_applications();
                IpcResponse::DiscoveredApps(apps)
            }
            IpcRequest::Shutdown => {
                tracing::warn!("Client requested shell host shutdown");
                IpcResponse::Ack
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
        let mut daemon = ShellHostDaemon::new().expect("Failed to initialize daemon");
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
                client_name: "test-runner".into(),
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

        // 4. Shutdown request
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
}
