//! AsterShell Configuration Management Subsystem

use astercore::{AsterError, RestartPolicy, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Root AsterShell configuration model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub version: u32,
    pub general: GeneralConfig,
    pub supervisor: SupervisorConfig,
    pub search: SearchConfig,
    pub taskbar: TaskbarConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            general: GeneralConfig::default(),
            supervisor: SupervisorConfig::default(),
            search: SearchConfig::default(),
            taskbar: TaskbarConfig::default(),
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        if self.version == 0 {
            return Err(AsterError::Config("Config version must be >= 1".into()));
        }
        if self.general.hotkey.trim().is_empty() {
            return Err(AsterError::Config("General hotkey cannot be empty".into()));
        }
        if self.search.max_results == 0 || self.search.max_results > 100 {
            return Err(AsterError::Config(
                "Search max_results must be between 1 and 100".into(),
            ));
        }
        if self.taskbar.monitor.trim().is_empty() {
            return Err(AsterError::Config(
                "Taskbar monitor target cannot be empty".into(),
            ));
        }
        validate_pinned_apps(&self.taskbar.pinned_apps)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub companion_mode: bool,
    pub hotkey: String,
    pub dark_theme: bool,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            companion_mode: true,
            hotkey: "Alt+Space".into(),
            dark_theme: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SupervisorConfig {
    pub auto_restart_crashed_children: bool,
    pub restart_policy: RestartPolicy,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            auto_restart_crashed_children: true,
            restart_policy: RestartPolicy::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchConfig {
    pub max_results: usize,
    pub debounce_ms: u64,
    pub warm_indexer_pool: bool,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            max_results: 10,
            debounce_ms: 25,
            warm_indexer_pool: true,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_monitor() -> String {
    "primary".into()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinnedAppConfig {
    pub process_key: String,
    pub title: String,
    pub monogram: String,
    pub launch_target: String,
}

pub fn default_pinned_apps() -> Vec<PinnedAppConfig> {
    vec![
        PinnedAppConfig {
            process_key: "explorer".into(),
            title: "File Explorer".into(),
            monogram: "FE".into(),
            launch_target: "explorer.exe".into(),
        },
        PinnedAppConfig {
            process_key: "windowsterminal".into(),
            title: "Terminal".into(),
            monogram: "WT".into(),
            launch_target: "wt.exe".into(),
        },
        PinnedAppConfig {
            process_key: "msedge".into(),
            title: "Edge".into(),
            monogram: "ED".into(),
            launch_target: "msedge.exe".into(),
        },
        PinnedAppConfig {
            process_key: "notepad".into(),
            title: "Notepad".into(),
            monogram: "NP".into(),
            launch_target: "notepad.exe".into(),
        },
    ]
}

/// Validates pinned application entries to prevent duplicate pins and arbitrary command injection.
pub fn validate_pinned_apps(pins: &[PinnedAppConfig]) -> Result<()> {
    let mut seen_keys = std::collections::HashSet::new();
    let mut seen_targets = std::collections::HashSet::new();
    const FORBIDDEN_CHARS: &[char] = &[';', '&', '|', '>', '<', '`', '$', '\n', '\r', '"'];

    for (idx, pin) in pins.iter().enumerate() {
        let key = pin.process_key.trim();
        let title = pin.title.trim();
        let target = pin.launch_target.trim();

        if key.is_empty() || title.is_empty() || target.is_empty() {
            return Err(AsterError::Config(format!(
                "Pinned app at index {} has empty process_key, title, or launch_target",
                idx
            )));
        }

        if target.chars().any(|c| FORBIDDEN_CHARS.contains(&c)) {
            return Err(AsterError::Config(format!(
                "Pinned app '{}' contains forbidden shell metacharacters in target '{}'",
                title, target
            )));
        }

        let lower_target = target.to_ascii_lowercase();
        if lower_target.contains(" /c ")
            || lower_target.contains(" -command ")
            || lower_target.starts_with("javascript:")
            || lower_target.starts_with("vbscript:")
            || lower_target.starts_with("data:")
        {
            return Err(AsterError::Config(format!(
                "Pinned app '{}' uses an unsafe launch target '{}'",
                title, target
            )));
        }

        if !seen_keys.insert(key.to_ascii_lowercase()) {
            return Err(AsterError::Config(format!(
                "Duplicate pinned app process_key '{}'",
                key
            )));
        }

        if !seen_targets.insert(lower_target) {
            return Err(AsterError::Config(format!(
                "Duplicate pinned app launch_target '{}'",
                target
            )));
        }
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskbarConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub position: String,
    #[serde(default = "default_monitor")]
    pub monitor: String,
    pub auto_hide: bool,
    pub live_thumbnails: bool,
    #[serde(default = "default_pinned_apps")]
    pub pinned_apps: Vec<PinnedAppConfig>,
}

impl Default for TaskbarConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            position: "bottom".into(),
            monitor: "primary".into(),
            auto_hide: false,
            live_thumbnails: true,
            pinned_apps: default_pinned_apps(),
        }
    }
}

/// Resolves standard OS configuration path for AsterShell.
pub fn default_config_path() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        PathBuf::from(appdata)
            .join("AsterShell")
            .join("config.toml")
    } else {
        PathBuf::from("config.toml")
    }
}

/// Loads configuration from disk, validating schema and falling back to default if missing or corrupt.
pub fn load_config_from_path(path: &Path) -> Result<Config> {
    if !path.exists() {
        let default_cfg = Config::default();
        let _ = save_config_to_path(&default_cfg, path);
        return Ok(default_cfg);
    }

    let contents = std::fs::read_to_string(path)
        .map_err(|e| AsterError::Config(format!("Failed to read config: {}", e)))?;

    match toml::from_str::<Config>(&contents) {
        Ok(cfg) => {
            cfg.validate()?;
            Ok(cfg)
        }
        Err(e) => {
            tracing::warn!(
                "Failed to parse config file (error: {}). Falling back to default configuration.",
                e
            );
            Ok(Config::default())
        }
    }
}

/// Safely writes configuration to disk via atomic temp file creation and rename.
pub fn save_config_to_path(config: &Config, path: &Path) -> Result<()> {
    config.validate()?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let toml_str = toml::to_string_pretty(config)
        .map_err(|e| AsterError::Config(format!("Failed to serialize config: {}", e)))?;

    let temp_path = path.with_extension("tmp");
    std::fs::write(&temp_path, toml_str)?;
    std::fs::rename(&temp_path, path)?;

    tracing::info!("Configuration successfully saved to {:?}", path);
    Ok(())
}

pub fn crate_name() -> &'static str {
    "configuration"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_validation() {
        let config = Config::default();
        assert!(config.validate().is_ok());
        assert_eq!(config.version, 1);
        assert!(config.general.companion_mode);
        assert_eq!(config.general.hotkey, "Alt+Space");
    }

    #[test]
    fn test_invalid_config_rejected() {
        let config_bad_version = Config {
            version: 0,
            ..Default::default()
        };
        assert!(config_bad_version.validate().is_err());

        let mut config_bad_search = Config::default();
        config_bad_search.search.max_results = 0;
        assert!(config_bad_search.validate().is_err());

        let mut config_bad_monitor = Config::default();
        config_bad_monitor.taskbar.monitor = "   ".into();
        assert!(config_bad_monitor.validate().is_err());
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let temp_dir = std::env::temp_dir().join("astershell_test_config");
        let config_path = temp_dir.join("test_config.toml");

        let mut orig_config = Config::default();
        orig_config.search.max_results = 25;
        orig_config.general.hotkey = "Ctrl+Shift+P".into();

        save_config_to_path(&orig_config, &config_path).expect("Failed to save");
        let loaded = load_config_from_path(&config_path).expect("Failed to load");

        assert_eq!(loaded.search.max_results, 25);
        assert_eq!(loaded.general.hotkey, "Ctrl+Shift+P");

        // Clean up
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_corrupted_config_fallback() {
        let temp_dir = std::env::temp_dir().join("astershell_test_corrupt");
        let config_path = temp_dir.join("corrupt_config.toml");

        let _ = std::fs::create_dir_all(&temp_dir);
        std::fs::write(&config_path, "not a valid toml syntax [[[[").unwrap();

        let loaded = load_config_from_path(&config_path).expect("Should fallback to default");
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.general.hotkey, "Alt+Space");

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_pinned_apps_validation_and_backward_compat() {
        // Legacy TOML without pinned_apps field still parses with defaults
        let full_toml = toml::to_string(&Config::default()).expect("Serialize default config");
        let legacy_toml = full_toml
            .split("[[taskbar.pinned_apps]]")
            .next()
            .expect("Split before pinned_apps");
        let parsed: Config = toml::from_str(legacy_toml).expect("Legacy TOML must parse");
        assert_eq!(parsed.taskbar.pinned_apps.len(), 4);
        assert!(parsed.validate().is_ok());

        // Reject duplicate process_key
        let mut dup_cfg = Config::default();
        dup_cfg.taskbar.pinned_apps.push(PinnedAppConfig {
            process_key: "notepad".into(),
            title: "Notepad 2".into(),
            monogram: "N2".into(),
            launch_target: "notepad2.exe".into(),
        });
        assert!(dup_cfg.validate().is_err());

        // Reject command injection / unsafe targets
        for bad_target in [
            "notepad.exe & calc.exe",
            "cmd.exe /c whoami",
            "javascript:alert(1)",
        ] {
            let mut bad_cfg = Config::default();
            bad_cfg.taskbar.pinned_apps = vec![PinnedAppConfig {
                process_key: "evil".into(),
                title: "Evil".into(),
                monogram: "EV".into(),
                launch_target: bad_target.into(),
            }];
            assert!(
                bad_cfg.validate().is_err(),
                "Target '{}' must be rejected",
                bad_target
            );
        }
    }
}
