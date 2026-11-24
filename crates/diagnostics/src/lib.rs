//! AsterShell Structured Logging and Diagnostics Subsystem

use std::sync::OnceLock;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

static SUBSCRIBER_INITIALIZED: OnceLock<bool> = OnceLock::new();

/// Diagnostic configuration options.
#[derive(Debug, Clone)]
pub struct DiagnosticConfig {
    pub process_name: &'static str,
    pub default_level: &'static str,
}

impl Default for DiagnosticConfig {
    fn default() -> Self {
        Self {
            process_name: "astershell",
            default_level: "info",
        }
    }
}

/// Initializes structured tracing for the calling process.
/// Safe to call multiple times (guarded by `OnceLock`).
pub fn init_diagnostics(config: DiagnosticConfig) {
    SUBSCRIBER_INITIALIZED.get_or_init(|| {
        let env_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(config.default_level));

        let fmt_layer = tracing_subscriber::fmt::layer()
            .with_target(true)
            .with_thread_ids(true)
            .with_ansi(true);

        let subscriber = tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer);

        let _ = subscriber.try_init();

        tracing::info!(
            process = config.process_name,
            version = env!("CARGO_PKG_VERSION"),
            "Diagnostics initialized successfully"
        );
        true
    });
}

/// Formats a safe diagnostic message without collecting sensitive search queries or paths.
pub fn sanitize_query_for_log(query_len: usize) -> String {
    format!("<redacted_query len={}>", query_len)
}

pub fn crate_name() -> &'static str {
    "diagnostics"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostics_init_idempotent() {
        init_diagnostics(DiagnosticConfig {
            process_name: "test-process",
            default_level: "debug",
        });
        init_diagnostics(DiagnosticConfig {
            process_name: "test-process-2",
            default_level: "info",
        });
        assert!(SUBSCRIBER_INITIALIZED.get().copied().unwrap_or(false));
    }

    #[test]
    fn test_sanitization() {
        let sanitized = sanitize_query_for_log(15);
        assert_eq!(sanitized, "<redacted_query len=15>");
    }
}
