//! AsterShell Shell API and Safe Launch Subsystem
//!
//! Provides safe, validated execution abstractions for search results and system activations.
//! Never invokes arbitrary shell command strings or unchecked shell processes.

use search_core::LaunchTarget;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum LaunchError {
    #[error("Target does not exist: {0}")]
    TargetNotFound(String),
    #[error("Invalid target format or disallowed URI scheme: {0}")]
    InvalidTarget(String),
    #[error("Failed to copy to system clipboard: {0}")]
    ClipboardError(String),
    #[error("Process spawn failed for '{0}': {1}")]
    SpawnFailed(String, String),
    #[error("Shell activation failed for '{0}': {1}")]
    ShellExecutionFailed(String, String),
    #[error("Unsupported shell action: {0}")]
    UnsupportedAction(String),
}

/// Validates whether a file or executable path exists on disk.
pub fn validate_file_path(path: &str) -> Result<(), LaunchError> {
    let p = Path::new(path);
    if !p.exists() {
        return Err(LaunchError::TargetNotFound(path.to_string()));
    }
    Ok(())
}

/// Validates that a URI uses an approved, safe protocol scheme.
pub fn validate_uri_scheme(uri: &str) -> Result<(), LaunchError> {
    let lower = uri.to_lowercase();
    let is_safe = lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mailto:")
        || lower.starts_with("ms-settings:")
        || lower.starts_with("file:///");

    if !is_safe {
        return Err(LaunchError::InvalidTarget(format!(
            "Disallowed or unsupported URI scheme in '{}'",
            uri
        )));
    }
    Ok(())
}

/// Copies UTF-8 text into the Windows system clipboard.
pub fn copy_to_system_clipboard(text: &str) -> Result<(), LaunchError> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::DataExchange::{
            CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
        };
        use windows::Win32::System::Memory::{
            GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE,
        };

        let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let bytes_len = utf16.len() * std::mem::size_of::<u16>();

        // SAFETY: Interacting with standard Win32 clipboard API sequence.
        unsafe {
            if let Err(e) = OpenClipboard(None) {
                return Err(LaunchError::ClipboardError(format!(
                    "Failed to open system clipboard: {}",
                    e
                )));
            }

            let _ = EmptyClipboard();

            let hmem = GlobalAlloc(GMEM_MOVEABLE, bytes_len).map_err(|e| {
                let _ = CloseClipboard();
                LaunchError::ClipboardError(format!("GlobalAlloc failed: {}", e))
            })?;

            let ptr = GlobalLock(hmem);
            if ptr.is_null() {
                let _ = CloseClipboard();
                return Err(LaunchError::ClipboardError(
                    "GlobalLock returned null".into(),
                ));
            }

            std::ptr::copy_nonoverlapping(utf16.as_ptr() as *const u8, ptr as *mut u8, bytes_len);
            let _ = GlobalUnlock(hmem);

            // CF_UNICODETEXT is standard format 13
            let set_res = SetClipboardData(13, HANDLE(hmem.0 as _));
            let _ = CloseClipboard();

            if let Err(e) = set_res {
                return Err(LaunchError::ClipboardError(format!(
                    "SetClipboardData failed: {}",
                    e
                )));
            }
        }
        tracing::info!(text_len = text.len(), "Text copied to system clipboard");
        Ok(())
    }

    #[cfg(not(windows))]
    {
        let _ = text;
        Ok(())
    }
}

/// Launches an application or document via Win32 ShellExecuteExW.
#[cfg(windows)]
fn shell_execute_open(path: &str, args: Option<&str>) -> Result<(), LaunchError> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SHELLEXECUTEINFOW};
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let h_verb = HSTRING::from("open");
    let h_file = HSTRING::from(path);
    let h_params = args.map(HSTRING::from);

    let mut exec_info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_FLAG_NO_UI,
        hwnd: HWND(std::ptr::null_mut()),
        lpVerb: PCWSTR(h_verb.as_ptr()),
        lpFile: PCWSTR(h_file.as_ptr()),
        lpParameters: h_params
            .as_ref()
            .map(|p| PCWSTR(p.as_ptr()))
            .unwrap_or(PCWSTR::null()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };

    // SAFETY: ShellExecuteExW with properly initialized SHELLEXECUTEINFOW.
    let res = unsafe { ShellExecuteExW(&mut exec_info) };
    if let Err(e) = res {
        return Err(LaunchError::ShellExecutionFailed(
            path.to_string(),
            e.to_string(),
        ));
    }

    Ok(())
}

/// Executes a `LaunchTarget` safely according to its specific activation rules.
pub fn launch_target(target: &LaunchTarget) -> Result<(), LaunchError> {
    match target {
        LaunchTarget::Executable {
            path,
            arguments,
            working_directory,
        } => {
            validate_file_path(path)?;

            tracing::info!(path = %path, args = ?arguments, "Launching executable target");

            let mut cmd = std::process::Command::new(path);
            cmd.args(arguments);
            if let Some(wd) = working_directory {
                cmd.current_dir(wd);
            }

            cmd.spawn()
                .map_err(|e| LaunchError::SpawnFailed(path.clone(), e.to_string()))?;

            Ok(())
        }
        LaunchTarget::Document { path } => {
            validate_file_path(path)?;

            tracing::info!(path = %path, "Opening document target");

            #[cfg(windows)]
            {
                shell_execute_open(path, None)?;
            }

            #[cfg(not(windows))]
            {
                let _ = path;
            }

            Ok(())
        }
        LaunchTarget::Uri { uri } => {
            validate_uri_scheme(uri)?;

            tracing::info!(uri = %uri, "Navigating URI target");

            #[cfg(windows)]
            {
                shell_execute_open(uri, None)?;
            }

            #[cfg(not(windows))]
            {
                let _ = uri;
            }

            Ok(())
        }
        LaunchTarget::CopyToClipboard { text } => copy_to_system_clipboard(text),
        LaunchTarget::Action { action_id } => match action_id.as_str() {
            "shell:reload" => {
                tracing::info!("Action triggered: reload shell");
                Ok(())
            }
            "shell:exit" => {
                tracing::info!("Action triggered: exit shell");
                Ok(())
            }
            other => Err(LaunchError::UnsupportedAction(other.to_string())),
        },
    }
}

pub fn crate_name() -> &'static str {
    "shell-api"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_file_path() {
        assert!(validate_file_path("C:\\Windows\\notepad.exe").is_ok());
        assert!(matches!(
            validate_file_path("C:\\nonexistent_dir_123\\nonexistent_app.exe"),
            Err(LaunchError::TargetNotFound(_))
        ));
    }

    #[test]
    fn test_validate_uri_scheme() {
        assert!(validate_uri_scheme("https://google.com").is_ok());
        assert!(validate_uri_scheme("http://localhost:8080").is_ok());
        assert!(validate_uri_scheme("mailto:user@example.com").is_ok());
        assert!(validate_uri_scheme("ms-settings:display").is_ok());

        assert!(matches!(
            validate_uri_scheme("javascript:alert(1)"),
            Err(LaunchError::InvalidTarget(_))
        ));
        assert!(matches!(
            validate_uri_scheme("powershell -Command Remove-Item"),
            Err(LaunchError::InvalidTarget(_))
        ));
    }

    #[test]
    fn test_clipboard_copy() {
        let text = "AsterShell Test Clipboard 12345";
        let res = copy_to_system_clipboard(text);
        assert!(
            res.is_ok(),
            "Clipboard copy should succeed: {:?}",
            res.err()
        );
    }

    #[test]
    fn test_launch_target_missing_executable() {
        let target = LaunchTarget::Executable {
            path: "C:\\definitely_missing_binary.exe".into(),
            arguments: vec![],
            working_directory: None,
        };
        let res = launch_target(&target);
        assert!(matches!(res, Err(LaunchError::TargetNotFound(_))));
    }

    #[test]
    fn test_launch_target_copy_to_clipboard() {
        let target = LaunchTarget::CopyToClipboard {
            text: "42.000".into(),
        };
        let res = launch_target(&target);
        assert!(res.is_ok());
    }

    #[test]
    fn test_launch_target_action() {
        let reload = LaunchTarget::Action {
            action_id: "shell:reload".into(),
        };
        assert!(launch_target(&reload).is_ok());

        let invalid = LaunchTarget::Action {
            action_id: "unknown:foo".into(),
        };
        assert!(matches!(
            launch_target(&invalid),
            Err(LaunchError::UnsupportedAction(_))
        ));
    }
}
