//! AsterShell Core Definitions and Types

use serde::{Deserialize, Serialize};
use std::fmt;

/// Unique identifier for each AsterShell managed process component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ComponentId {
    ShellHost,
    Taskbar,
    SearchUi,
    StartMenu,
    Desktop,
    FileManager,
    Settings,
}

impl ComponentId {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ShellHost => "shell-host",
            Self::Taskbar => "taskbar",
            Self::SearchUi => "search-ui",
            Self::StartMenu => "start-menu",
            Self::Desktop => "desktop",
            Self::FileManager => "file-manager",
            Self::Settings => "settings",
        }
    }
}

impl fmt::Display for ComponentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Operational state of a supervised process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessStatus {
    Uninitialized,
    Starting,
    Running,
    Crashed { exit_code: Option<u32> },
    BackoffPaused { retry_in_ms: u64 },
    Stopped,
    CircuitBroken { consecutive_crashes: u32 },
}

/// Restart policy parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RestartPolicy {
    pub max_retries_in_window: u32,
    pub window_secs: u64,
    pub initial_backoff_ms: u64,
    pub max_backoff_ms: u64,
    pub circuit_breaker_threshold: u32,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            max_retries_in_window: 3,
            window_secs: 10,
            initial_backoff_ms: 1000,
            max_backoff_ms: 30000,
            circuit_breaker_threshold: 5,
        }
    }
}

/// Metadata for discovered applications.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredApp {
    pub id: String,
    pub name: String,
    pub executable_or_aumid: String,
    pub icon_location: Option<String>,
    pub is_packaged: bool,
    pub source: AppDiscoverySource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppDiscoverySource {
    AppsFolderCom,
    StartMenuShortcutSystem,
    StartMenuShortcutUser,
    AppPathsRegistry,
}

/// Common AsterShell error types.
#[derive(thiserror::Error, Debug)]
pub enum AsterError {
    #[error("Platform error: {0}")]
    Platform(String),

    #[error("IPC communication error: {0}")]
    Ipc(String),

    #[error("Process supervision error: {0}")]
    Supervisor(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Application discovery error: {0}")]
    Discovery(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Protocol version mismatch (expected: {expected}, found: {found})")]
    ProtocolVersionMismatch { expected: u16, found: u16 },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, AsterError>;

pub fn crate_name() -> &'static str {
    "astercore"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_id_string_conversion() {
        assert_eq!(ComponentId::ShellHost.as_str(), "shell-host");
        assert_eq!(ComponentId::SearchUi.as_str(), "search-ui");
        assert_eq!(format!("{}", ComponentId::Taskbar), "taskbar");
    }

    #[test]
    fn test_default_restart_policy() {
        let policy = RestartPolicy::default();
        assert_eq!(policy.max_retries_in_window, 3);
        assert_eq!(policy.initial_backoff_ms, 1000);
        assert_eq!(policy.circuit_breaker_threshold, 5);
    }

    #[test]
    fn test_discovered_app_serialization() {
        let app = DiscoveredApp {
            id: "calc".into(),
            name: "Calculator".into(),
            executable_or_aumid: "Microsoft.WindowsCalculator".into(),
            icon_location: None,
            is_packaged: true,
            source: AppDiscoverySource::AppsFolderCom,
        };
        assert!(app.is_packaged);
    }
}
