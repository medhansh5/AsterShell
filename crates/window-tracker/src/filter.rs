//! 7-Stage Top-Level Application Window Filtering Pipeline.
//!
//! Determines whether a Win32 window represents a user-facing application window
//! suitable for display in the AsterShell taskbar, filtering out hidden, owned,
//! tool, cloaked, zero-sized, and shell-reserved windows.

/// Raw attributes of a Win32 window evaluated by the 7-stage filter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowAttributes {
    pub is_valid: bool,
    pub is_visible: bool,
    pub is_child: bool,
    pub is_tool_window: bool,
    pub is_app_window: bool,
    pub has_owner: bool,
    pub is_cloaked: bool,
    pub is_iconic: bool,
    pub width: i32,
    pub height: i32,
    pub class_name: String,
    pub title: String,
}

/// Shell-reserved or system overlay window classes that must never appear as taskbar items.
pub const EXCLUDED_SHELL_CLASSES: &[&str] = &[
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "Progman",
    "WorkerW",
    "NotifyIconOverflowWindow",
    "TopLevelWindowForOverflowXamlIsland",
    "Windows.UI.Core.CoreWindow",
    "ApplicationFrameWindow_Hidden",
];

/// Evaluates the 7-stage filter pipeline against a window's attributes.
///
/// Stages:
/// 1. Valid HWND (`is_valid`)
/// 2. Visible (`is_visible`)
/// 3. Not a child window (`!is_child`)
/// 4. Not a tool window unless explicitly marked `WS_EX_APPWINDOW`
/// 5. Not owned unless explicitly marked `WS_EX_APPWINDOW`
/// 6. Not DWM cloaked (`!is_cloaked`, filters UWP suspended/other-desktop windows)
/// 7. Non-empty bounds (unless iconic/minimized) and not an excluded shell class
pub fn should_track_attributes(attrs: &WindowAttributes) -> bool {
    // Stage 1: Valid window handle
    if !attrs.is_valid {
        return false;
    }

    // Stage 2: Must be visible
    if !attrs.is_visible {
        return false;
    }

    // Stage 3: Exclude child windows (WS_CHILD)
    if attrs.is_child {
        return false;
    }

    // Stage 4: Exclude tool windows unless WS_EX_APPWINDOW is set
    if attrs.is_tool_window && !attrs.is_app_window {
        return false;
    }

    // Stage 5: Exclude owned windows unless WS_EX_APPWINDOW is set
    if attrs.has_owner && !attrs.is_app_window {
        return false;
    }

    // Stage 6: Exclude DWM cloaked windows (UWP background, virtual desktop hidden, etc.)
    if attrs.is_cloaked {
        return false;
    }

    // Stage 7a: Require non-empty bounding box unless minimized (iconic)
    if !attrs.is_iconic && (attrs.width <= 0 || attrs.height <= 0) {
        return false;
    }

    // Stage 7b: Exclude shell-reserved classes and AsterShell's own companion windows
    let trimmed_class = attrs.class_name.trim();
    if EXCLUDED_SHELL_CLASSES
        .iter()
        .any(|&excluded| excluded.eq_ignore_ascii_case(trimmed_class))
    {
        return false;
    }

    if attrs.title == "AsterShell Taskbar" || attrs.title == "AsterShell Search" {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_app_attrs() -> WindowAttributes {
        WindowAttributes {
            is_valid: true,
            is_visible: true,
            is_child: false,
            is_tool_window: false,
            is_app_window: false,
            has_owner: false,
            is_cloaked: false,
            is_iconic: false,
            width: 1280,
            height: 720,
            class_name: "Chrome_WidgetWin_1".to_string(),
            title: "Google Chrome".to_string(),
        }
    }

    #[test]
    fn test_seven_stage_filter_accepts_normal_app() {
        assert!(should_track_attributes(&valid_app_attrs()));
    }

    #[test]
    fn test_seven_stage_filter_rejects_invalid_and_invisible() {
        let mut attrs = valid_app_attrs();
        attrs.is_valid = false;
        assert!(!should_track_attributes(&attrs));

        let mut attrs = valid_app_attrs();
        attrs.is_visible = false;
        assert!(!should_track_attributes(&attrs));
    }

    #[test]
    fn test_seven_stage_filter_rejects_child_windows() {
        let mut attrs = valid_app_attrs();
        attrs.is_child = true;
        assert!(!should_track_attributes(&attrs));
    }

    #[test]
    fn test_seven_stage_filter_tool_and_owned_windows() {
        let mut tool = valid_app_attrs();
        tool.is_tool_window = true;
        assert!(!should_track_attributes(&tool));

        // Tool window with WS_EX_APPWINDOW is accepted
        tool.is_app_window = true;
        assert!(should_track_attributes(&tool));

        let mut owned = valid_app_attrs();
        owned.has_owner = true;
        assert!(!should_track_attributes(&owned));

        // Owned window with WS_EX_APPWINDOW is accepted
        owned.is_app_window = true;
        assert!(should_track_attributes(&owned));
    }

    #[test]
    fn test_seven_stage_filter_rejects_cloaked() {
        let mut attrs = valid_app_attrs();
        attrs.is_cloaked = true;
        assert!(!should_track_attributes(&attrs));
    }

    #[test]
    fn test_seven_stage_filter_bounds_and_iconic() {
        let mut zero_bounds = valid_app_attrs();
        zero_bounds.width = 0;
        zero_bounds.height = 0;
        assert!(!should_track_attributes(&zero_bounds));

        // Iconic (minimized) windows with empty/offscreen rect are still tracked
        zero_bounds.is_iconic = true;
        assert!(should_track_attributes(&zero_bounds));
    }

    #[test]
    fn test_seven_stage_filter_rejects_shell_classes_and_self() {
        for class in EXCLUDED_SHELL_CLASSES {
            let mut attrs = valid_app_attrs();
            attrs.class_name = (*class).to_string();
            assert!(
                !should_track_attributes(&attrs),
                "Shell class {} should be filtered out",
                class
            );
        }

        let mut self_taskbar = valid_app_attrs();
        self_taskbar.title = "AsterShell Taskbar".to_string();
        assert!(!should_track_attributes(&self_taskbar));
    }
}
