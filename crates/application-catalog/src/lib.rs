//! AsterShell Application Discovery Subsystem

use astercore::{AppDiscoverySource, DiscoveredApp};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Enumerates all installed applications using documented Windows Start Menu directories and shell namespaces.
pub fn discover_installed_applications() -> Vec<DiscoveredApp> {
    let mut apps = Vec::new();
    let mut seen_names = HashSet::new();

    // 1. Scan System Start Menu Programs
    let system_start = PathBuf::from(r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs");
    if system_start.exists() {
        scan_directory_shortcuts(
            &system_start,
            AppDiscoverySource::StartMenuShortcutSystem,
            &mut seen_names,
            &mut apps,
        );
    }

    // 2. Scan Current User Start Menu Programs
    if let Ok(appdata) = std::env::var("APPDATA") {
        let user_start = PathBuf::from(appdata)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs");

        if user_start.exists() {
            scan_directory_shortcuts(
                &user_start,
                AppDiscoverySource::StartMenuShortcutUser,
                &mut seen_names,
                &mut apps,
            );
        }
    }

    tracing::info!(
        count = apps.len(),
        "Discovered installed Windows applications"
    );
    apps
}

fn scan_directory_shortcuts(
    dir: &Path,
    source: AppDiscoverySource,
    seen: &mut HashSet<String>,
    results: &mut Vec<DiscoveredApp>,
) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan_directory_shortcuts(&path, source, seen, results);
            } else if let Some(ext) = path.extension() {
                let ext_str = ext.to_string_lossy().to_lowercase();
                if ext_str == "lnk" || ext_str == "url" || ext_str == "exe" {
                    if let Some(stem) = path.file_stem() {
                        let name = stem.to_string_lossy().trim().to_string();
                        // Ignore uninstallers and readme files
                        let lower = name.to_lowercase();
                        if lower.contains("uninstall")
                            || lower.contains("readme")
                            || lower.contains("help")
                        {
                            continue;
                        }

                        if seen.insert(lower.clone()) {
                            let id = format!("{}:{}", lower, path.display());
                            results.push(DiscoveredApp {
                                id,
                                name,
                                executable_or_aumid: path.to_string_lossy().to_string(),
                                icon_location: None,
                                is_packaged: false,
                                source,
                            });
                        }
                    }
                }
            }
        }
    }
}

pub fn crate_name() -> &'static str {
    "application-catalog"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_application_discovery_returns_usable_results() {
        let apps = discover_installed_applications();
        #[cfg(windows)]
        {
            assert!(
                !apps.is_empty(),
                "Application discovery must find at least one installed application on Windows"
            );
            println!("Found {} applications on Windows system.", apps.len());
            for app in apps.iter().take(5) {
                println!("  - {} ({:?})", app.name, app.source);
                assert!(!app.name.is_empty());
                assert!(!app.executable_or_aumid.is_empty());
            }
        }
    }
}
