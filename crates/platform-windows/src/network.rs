//! AsterShell Network Connectivity Query & Event Notification Subsystem
//!
//! Queries Windows Network List Manager (`INetworkListManager`) and subscribes to
//! `INetworkListManagerEvents` via COM connection points for zero-polling connectivity
//! state updates.

use astercore::Result;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;

/// Represents the current system network connectivity state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkStatus {
    pub is_connected: bool,
    pub has_internet: bool,
    pub label: String,
}

impl Default for NetworkStatus {
    fn default() -> Self {
        Self {
            is_connected: false,
            has_internet: false,
            label: "Offline".to_string(),
        }
    }
}

/// Queries current Windows Network List Manager connectivity.
pub fn query_network_status() -> Result<NetworkStatus> {
    #[cfg(windows)]
    {
        win32::query_network_status_win32()
    }
    #[cfg(not(windows))]
    {
        Ok(NetworkStatus {
            is_connected: true,
            has_internet: true,
            label: "Connected".to_string(),
        })
    }
}

/// Handle to an active `INetworkListManagerEvents` subscription.
pub struct NetworkMonitorHandle {
    #[cfg(windows)]
    stop_tx: Option<Sender<()>>,
    join_handle: Option<JoinHandle<()>>,
}

impl NetworkMonitorHandle {
    /// Starts an event-driven Network List Manager monitor emitting `NetworkStatus` on changes.
    pub fn start() -> Result<(Self, Receiver<NetworkStatus>)> {
        let (tx, rx) = mpsc::channel();

        #[cfg(windows)]
        {
            let (stop_tx, stop_rx) = mpsc::channel();
            let handle = std::thread::Builder::new()
                .name("astershell-network-monitor".to_string())
                .spawn(move || {
                    win32::run_network_monitor(tx, stop_rx);
                })
                .map_err(|e| {
                    astercore::AsterError::Platform(format!(
                        "Failed to spawn network monitor thread: {}",
                        e
                    ))
                })?;

            Ok((
                Self {
                    stop_tx: Some(stop_tx),
                    join_handle: Some(handle),
                },
                rx,
            ))
        }

        #[cfg(not(windows))]
        {
            let _ = tx.send(NetworkStatus::default());
            Ok((Self { join_handle: None }, rx))
        }
    }
}

impl Drop for NetworkMonitorHandle {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            if let Some(tx) = self.stop_tx.take() {
                let _ = tx.send(());
            }
        }
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(windows)]
mod win32 {
    use super::*;
    use windows::core::{implement, Interface};
    use windows::Win32::Networking::NetworkListManager::{
        INetworkListManager, INetworkListManagerEvents, INetworkListManagerEvents_Impl,
        NetworkListManager, NLM_CONNECTIVITY, NLM_CONNECTIVITY_DISCONNECTED,
        NLM_CONNECTIVITY_IPV4_INTERNET, NLM_CONNECTIVITY_IPV6_INTERNET,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, IConnectionPointContainer, CLSCTX_ALL,
        COINIT_MULTITHREADED,
    };

    fn status_from_connectivity(conn: NLM_CONNECTIVITY) -> NetworkStatus {
        let has_internet =
            (conn.0 & (NLM_CONNECTIVITY_IPV4_INTERNET.0 | NLM_CONNECTIVITY_IPV6_INTERNET.0)) != 0;
        let is_connected = conn.0 != NLM_CONNECTIVITY_DISCONNECTED.0;
        let label = if has_internet {
            "Online".to_string()
        } else if is_connected {
            "Local".to_string()
        } else {
            "Offline".to_string()
        };
        NetworkStatus {
            is_connected,
            has_internet,
            label,
        }
    }

    #[implement(INetworkListManagerEvents)]
    struct NetworkEventsSink {
        tx: Sender<NetworkStatus>,
    }

    impl INetworkListManagerEvents_Impl for NetworkEventsSink_Impl {
        fn ConnectivityChanged(
            &self,
            newconnectivity: NLM_CONNECTIVITY,
        ) -> windows::core::Result<()> {
            let _ = self.tx.send(status_from_connectivity(newconnectivity));
            Ok(())
        }
    }

    pub fn query_network_status_win32() -> Result<NetworkStatus> {
        unsafe {
            let com_init = CoInitializeEx(None, COINIT_MULTITHREADED);
            let res = match CoCreateInstance::<_, INetworkListManager>(
                &NetworkListManager,
                None,
                CLSCTX_ALL,
            ) {
                Ok(nlm) => {
                    let conn = nlm
                        .GetConnectivity()
                        .unwrap_or(NLM_CONNECTIVITY_DISCONNECTED);
                    Ok(status_from_connectivity(conn))
                }
                Err(e) => {
                    tracing::debug!(error = %e, "NetworkListManager query failed");
                    Ok(NetworkStatus::default())
                }
            };
            if com_init.is_ok() {
                CoUninitialize();
            }
            res
        }
    }

    pub fn run_network_monitor(tx: Sender<NetworkStatus>, stop_rx: Receiver<()>) {
        unsafe {
            let com_init = CoInitializeEx(None, COINIT_MULTITHREADED);

            if let Ok(nlm) =
                CoCreateInstance::<_, INetworkListManager>(&NetworkListManager, None, CLSCTX_ALL)
            {
                let conn = nlm
                    .GetConnectivity()
                    .unwrap_or(NLM_CONNECTIVITY_DISCONNECTED);
                let _ = tx.send(status_from_connectivity(conn));

                if let Ok(cpc) = nlm.cast::<IConnectionPointContainer>() {
                    if let Ok(cp) = cpc.FindConnectionPoint(&INetworkListManagerEvents::IID) {
                        let sink: INetworkListManagerEvents =
                            NetworkEventsSink { tx: tx.clone() }.into();
                        if let Ok(cookie) = cp.Advise(&sink) {
                            let _ = stop_rx.recv();
                            let _ = cp.Unadvise(cookie);
                        } else {
                            let _ = stop_rx.recv();
                        }
                    } else {
                        let _ = stop_rx.recv();
                    }
                } else {
                    let _ = stop_rx.recv();
                }
            } else {
                let _ = tx.send(NetworkStatus::default());
                let _ = stop_rx.recv();
            }

            if com_init.is_ok() {
                CoUninitialize();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_network_status_succeeds() {
        let status = query_network_status().expect("query_network_status should not fail");
        assert!(!status.label.is_empty());
    }

    #[test]
    fn test_network_monitor_start_and_clean_drop() {
        let (handle, rx) = NetworkMonitorHandle::start().expect("NetworkMonitorHandle::start");
        let initial = rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("Must receive initial network status");
        assert!(!initial.label.is_empty());
        drop(handle);
    }
}
