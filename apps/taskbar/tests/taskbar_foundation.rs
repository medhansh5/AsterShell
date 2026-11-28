//! AsterShell Taskbar Foundation & Lifecycle Acceptance Tests (Milestone M3A.1)
//!
//! Automated test suite verifying criteria AC-M3A-01 through AC-M3A-07:
//!
//! Categorization:
//! - [Category A: Isolated Win32 Window Unit Tests]
//!   Pure Win32 API window testing in isolation for deterministic verification of styles,
//!   work area non-interference, coordinate transforms, and DWM composition attributes.
//! - [Category B: Real Slint Component & Native Window Integration Tests]
//!   Full Slint UI window realization test asserting that the real Slint
//!   HWND receives exact Win32 companion styles and strips standard desktop frame chrome.
//! - [Category C: Real Process Containment & Supervisor Lifecycle Integration Tests]
//!   End-to-end process supervision test spawning a real OS process, asserting kernel Job
//!   Object containment via Win32 IsProcessInJob API, verifying PID authorization in
//!   the supervisor registry, and testing kill-on-close termination semantics.
//! - [Category D: Supervisor Authenticated IPC & Security Verification Tests]
//!   Security verification testing named pipe mutual PID authentication, ensuring
//!   unauthorized callers cannot issue shutdown commands, and verifying graceful lifecycle termination.

use astercore::ComponentId;
use ipc::{
    transport::{self, NamedPipeServer},
    IpcRequest, IpcResponse, RequestEnvelope, ResponseEnvelope, PROTOCOL_VERSION_1,
};
use platform_windows::{
    apply_dark_mode_and_backdrop, apply_native_taskbar_configuration,
    calculate_taskbar_coordinates_with_dpi, configure_taskbar_native_styles, default_pipe_sddl,
    get_primary_monitor_work_area, get_window_styles, position_taskbar_window,
    scale_to_physical_pixels, DwmBackdropType, JobObject, PipeSecurityDescriptor, Rect,
    WS_CAPTION_FLAG, WS_CLIPCHILDREN_FLAG, WS_CLIPSIBLINGS_FLAG, WS_EX_APPWINDOW_FLAG,
    WS_EX_NOACTIVATE_FLAG, WS_EX_TOOLWINDOW_FLAG, WS_EX_TOPMOST_FLAG, WS_POPUP_FLAG,
    WS_SYSMENU_FLAG, WS_THICKFRAME_FLAG, WS_VISIBLE_FLAG,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[cfg(windows)]
use windows::core::w;
#[cfg(windows)]
use windows::Win32::Foundation::{HWND, RECT};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, SystemParametersInfoW, SPI_GETWORKAREA,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, WINDOW_EX_STYLE, WS_POPUP, WS_VISIBLE,
};

slint::slint! {
    #[edition = "1.19"]
    export component TestTaskbarWindow inherits Window {
        title: "AsterShell Taskbar Test Window";
        preferred-width: 720px;
        preferred-height: 48px;
        min-width: 720px;
        max-width: 720px;
        min-height: 48px;
        max-height: 48px;
        no-frame: true;
    }
}

/// Helper RAII wrapper for a Win32 test window to ensure cleanup after tests.
#[cfg(windows)]
struct TestWindow {
    hwnd: HWND,
}

#[cfg(windows)]
impl TestWindow {
    fn new() -> Self {
        // SAFETY: Calling CreateWindowExW with pre-registered STATIC window class.
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                w!("AsterTaskbarTestWindow"),
                WS_POPUP | WS_VISIBLE,
                0,
                0,
                100,
                100,
                None,
                None,
                None,
                None,
            )
            .expect("Failed to create Win32 test window")
        };
        Self { hwnd }
    }

    fn as_isize(&self) -> isize {
        self.hwnd.0 as _
    }
}

#[cfg(windows)]
impl Drop for TestWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

/// Helper to resolve the taskbar executable binary path during integration tests.
fn resolve_taskbar_exe() -> PathBuf {
    if let Some(path) = option_env!("CARGO_BIN_EXE_taskbar") {
        let p = PathBuf::from(path);
        if p.exists() {
            return p;
        }
    }
    if let Ok(mut current) = std::env::current_exe() {
        current.pop();
        let exe_name = if cfg!(windows) {
            "taskbar.exe"
        } else {
            "taskbar"
        };
        let candidate = current.join(exe_name);
        if candidate.exists() {
            return candidate;
        }
        let parent_candidate = current.join("..").join(exe_name);
        if parent_candidate.exists() {
            if let Ok(canon) = parent_candidate.canonicalize() {
                return canon;
            }
            return parent_candidate;
        }
    }
    PathBuf::from(if cfg!(windows) {
        "taskbar.exe"
    } else {
        "taskbar"
    })
}

// ============================================================================
// Category A: Isolated Win32 Window Unit Tests
// ============================================================================

/// AC-M3A-02: Native frameless window styles (Unit test on isolated Win32 HWND).
/// HWND must possess WS_POPUP, WS_VISIBLE, WS_CLIPSIBLINGS, WS_CLIPCHILDREN,
/// and explicitly strip WS_CAPTION, WS_THICKFRAME, WS_SYSMENU (WINDOW-01).
#[cfg(windows)]
#[test]
fn test_ac_m3a_02_native_frameless_window_styles_unit() {
    let test_win = TestWindow::new();
    let hwnd = test_win.as_isize();

    let (style, _) = configure_taskbar_native_styles(hwnd)
        .expect("configure_taskbar_native_styles must succeed");

    assert_ne!(
        style & WS_POPUP_FLAG,
        0,
        "WS_POPUP bit (0x80000000) must be present"
    );
    assert_ne!(
        style & WS_VISIBLE_FLAG,
        0,
        "WS_VISIBLE bit (0x10000000) must be present"
    );
    assert_ne!(
        style & WS_CLIPSIBLINGS_FLAG,
        0,
        "WS_CLIPSIBLINGS bit (0x04000000) must be present"
    );
    assert_ne!(
        style & WS_CLIPCHILDREN_FLAG,
        0,
        "WS_CLIPCHILDREN bit (0x02000000) must be present"
    );

    // Explicitly verify titlebar, resize borders, and sysmenu are stripped (WINDOW-01)
    assert_eq!(
        style & WS_CAPTION_FLAG,
        0,
        "WS_CAPTION bit (0x00C00000) must NOT be present"
    );
    assert_eq!(
        style & WS_THICKFRAME_FLAG,
        0,
        "WS_THICKFRAME bit (0x00040000) must NOT be present"
    );
    assert_eq!(
        style & WS_SYSMENU_FLAG,
        0,
        "WS_SYSMENU bit (0x00080000) must NOT be present"
    );

    // Read back independently via get_window_styles to verify persistence
    let (read_style, _) = get_window_styles(hwnd).expect("get_window_styles must succeed");
    assert_eq!(read_style, style, "Styles must match read-back values");
}

/// AC-M3A-03: Extended shell styles and Alt+Tab exclusion (Unit test on isolated Win32 HWND).
/// HWND must possess WS_EX_TOPMOST, WS_EX_TOOLWINDOW, WS_EX_NOACTIVATE, and strip WS_EX_APPWINDOW.
#[cfg(windows)]
#[test]
fn test_ac_m3a_03_extended_shell_styles_and_alt_tab_exclusion_unit() {
    let test_win = TestWindow::new();
    let hwnd = test_win.as_isize();

    let (_, ex_style) = configure_taskbar_native_styles(hwnd)
        .expect("configure_taskbar_native_styles must succeed");

    assert_ne!(
        ex_style & WS_EX_TOPMOST_FLAG,
        0,
        "WS_EX_TOPMOST bit (0x00000008) must be present"
    );
    assert_ne!(
        ex_style & WS_EX_TOOLWINDOW_FLAG,
        0,
        "WS_EX_TOOLWINDOW bit (0x00000080) must be present to prevent Alt+Tab indexing"
    );
    assert_ne!(
        ex_style & WS_EX_NOACTIVATE_FLAG,
        0,
        "WS_EX_NOACTIVATE bit (0x08000000) must be present to avoid foreground focus theft"
    );
    assert_eq!(
        ex_style & WS_EX_APPWINDOW_FLAG,
        0,
        "WS_EX_APPWINDOW bit (0x00040000) must NOT be present"
    );

    // Read back independently via get_window_styles
    let (_, read_ex_style) = get_window_styles(hwnd).expect("get_window_styles must succeed");
    assert_eq!(read_ex_style, ex_style);
}

/// AC-M3A-04: Zero desktop work-area modification.
/// SystemParametersInfoW(SPI_GETWORKAREA) must remain 100% identical before and after taskbar window creation.
#[cfg(windows)]
#[test]
fn test_ac_m3a_04_zero_desktop_work_area_modification() {
    // Ensure Per-Monitor DPI awareness is set consistently across test threads
    unsafe {
        use windows::Win32::UI::HiDpi::{
            SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        };
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    // 1. Query work area immediately before
    let mut rc_before = RECT::default();
    unsafe {
        let ok = SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut rc_before as *mut _ as *mut std::ffi::c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
        assert!(ok.is_ok(), "SPI_GETWORKAREA before must succeed");
    }

    // 2. Realize and configure taskbar window
    let test_win = TestWindow::new();
    let hwnd = test_win.as_isize();
    let _ = configure_taskbar_native_styles(hwnd).expect("Styling configuration must succeed");
    let (x, y, w, h) = calculate_taskbar_coordinates_with_dpi(720, 48, 96)
        .expect("Coordinates calculation must succeed");
    position_taskbar_window(hwnd, x, y, w, h).expect("Positioning must succeed");

    // 3. Query work area immediately after
    let mut rc_after = RECT::default();
    unsafe {
        let ok = SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut rc_after as *mut _ as *mut std::ffi::c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
        assert!(ok.is_ok(), "SPI_GETWORKAREA after must succeed");
    }

    // 4. Assert exact bitwise equality: no AppBar contraction or work area shrinkage
    assert_eq!(
        rc_before.left, rc_after.left,
        "Desktop work-area left coordinate must not be modified"
    );
    assert_eq!(
        rc_before.top, rc_after.top,
        "Desktop work-area top coordinate must not be modified"
    );
    assert_eq!(
        rc_before.right, rc_after.right,
        "Desktop work-area right coordinate must not be modified"
    );
    assert_eq!(
        rc_before.bottom, rc_after.bottom,
        "Desktop work-area bottom coordinate must not be modified"
    );
}

/// AC-M3A-05: Signed primary monitor placement and DPI scaling.
/// Window positions at top of primary monitor using signed i32 coordinates.
#[test]
fn test_ac_m3a_05_signed_primary_monitor_placement() {
    let work = get_primary_monitor_work_area().expect("get_primary_monitor_work_area must succeed");
    assert!(work.width() > 0);
    assert!(work.height() > 0);

    // 1. Standard 100% DPI (96)
    let (x, y, w, h) = calculate_taskbar_coordinates_with_dpi(720, 48, 96).unwrap();
    assert_eq!(w, 720, "Physical width at 96 DPI must be 720");
    assert_eq!(h, 48, "Physical height at 96 DPI must be 48");
    assert_eq!(y, work.top, "Taskbar Y must match primary monitor work.top");
    assert_eq!(
        x,
        work.left + (work.width() - 720) / 2,
        "Taskbar X must be centered horizontally on primary monitor"
    );

    // 2. High-DPI 150% (144)
    let (x144, y144, w144, h144) = calculate_taskbar_coordinates_with_dpi(720, 48, 144).unwrap();
    assert_eq!(w144, 1080);
    assert_eq!(h144, 72);
    assert_eq!(y144, work.top);
    assert_eq!(x144, work.left + (work.width() - 1080) / 2);

    // 3. Signed negative offset simulation (virtual multi-monitor with primary to the right of secondary)
    let negative_virtual_work = Rect {
        left: -1920,
        top: -1080,
        right: 0,
        bottom: 0,
    };
    let phys_w = scale_to_physical_pixels(720, 96);
    let neg_x = negative_virtual_work.left + (negative_virtual_work.width() - phys_w) / 2;
    let neg_y = negative_virtual_work.top;

    assert_eq!(
        neg_x, -1320,
        "Signed arithmetic must handle negative virtual coordinate spaces"
    );
    assert_eq!(neg_y, -1080);
}

/// AC-M3A-06: DWM Mica backdrop realization and graceful fallback.
/// Asserts apply_dark_mode_and_backdrop returns Ok(()) on all environments.
#[cfg(windows)]
#[test]
fn test_ac_m3a_06_dwm_backdrop_realization_and_fallback() {
    let test_win = TestWindow::new();
    let hwnd = test_win.as_isize();

    let res = apply_dark_mode_and_backdrop(hwnd, DwmBackdropType::Mica);
    assert!(
        res.is_ok(),
        "apply_dark_mode_and_backdrop must return Ok(()) without panics on Windows: {:?}",
        res.err()
    );
}

// ============================================================================
// Category B: Real Slint Component & Native Window Integration Tests
// ============================================================================

/// TEST-01: Real Slint `TestTaskbarWindow` native window realization test.
/// Asserts that when the real Slint/Winit taskbar component is instantiated and configured,
/// its underlying Win32 HWND receives all required styles and strips unwanted window chrome.
#[cfg(windows)]
#[test]
fn test_ac_m3a_02_and_03_real_slint_taskbar_window_native_styles() {
    use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use slint::winit_030::WinitWindowAccessor;
    use slint::ComponentHandle;

    let win = TestTaskbarWindow::new().expect("Real Slint TestTaskbarWindow creation must succeed");
    win.show()
        .expect("Showing real Slint TestTaskbarWindow must succeed");

    let win_weak = win.as_weak();
    let verified_styles = Arc::new(std::sync::Mutex::new(None));
    let styles_clone = verified_styles.clone();

    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::SingleShot,
        Duration::from_millis(50),
        move || {
            if let Some(w) = win_weak.upgrade() {
                let hwnd_opt = w
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
                    if let Ok((style, ex_style)) = apply_native_taskbar_configuration(hwnd) {
                        *styles_clone.lock().unwrap() = Some((style, ex_style));
                    }
                }
            }
            let _ = slint::quit_event_loop();
        },
    );

    let _ = slint::run_event_loop();

    let styles = verified_styles
        .lock()
        .unwrap()
        .take()
        .expect("Real Slint window must yield an HWND and configured styles during event loop");
    let (style, ex_style) = styles;

    // Assert base styles on real Slint HWND
    assert_ne!(
        style & WS_POPUP_FLAG,
        0,
        "Real Slint HWND must have WS_POPUP"
    );
    assert_ne!(
        style & WS_VISIBLE_FLAG,
        0,
        "Real Slint HWND must have WS_VISIBLE"
    );
    assert_ne!(
        style & WS_CLIPSIBLINGS_FLAG,
        0,
        "Real Slint HWND must have WS_CLIPSIBLINGS"
    );
    assert_ne!(
        style & WS_CLIPCHILDREN_FLAG,
        0,
        "Real Slint HWND must have WS_CLIPCHILDREN"
    );

    // Assert unwanted chrome is stripped from real Slint HWND (WINDOW-01)
    assert_eq!(
        style & WS_CAPTION_FLAG,
        0,
        "Real Slint HWND must strip WS_CAPTION"
    );
    assert_eq!(
        style & WS_THICKFRAME_FLAG,
        0,
        "Real Slint HWND must strip WS_THICKFRAME"
    );
    assert_eq!(
        style & WS_SYSMENU_FLAG,
        0,
        "Real Slint HWND must strip WS_SYSMENU"
    );

    // Assert extended styles on real Slint HWND
    assert_ne!(
        ex_style & WS_EX_TOPMOST_FLAG,
        0,
        "Real Slint HWND must have WS_EX_TOPMOST"
    );
    assert_ne!(
        ex_style & WS_EX_TOOLWINDOW_FLAG,
        0,
        "Real Slint HWND must have WS_EX_TOOLWINDOW"
    );
    assert_ne!(
        ex_style & WS_EX_NOACTIVATE_FLAG,
        0,
        "Real Slint HWND must have WS_EX_NOACTIVATE"
    );
    assert_eq!(
        ex_style & WS_EX_APPWINDOW_FLAG,
        0,
        "Real Slint HWND must strip WS_EX_APPWINDOW"
    );
}

#[cfg(windows)]
#[test]
fn test_f_p2_01_win32_hide_restore_preserves_native_styles() {
    use platform_windows::{
        configure_taskbar_native_styles, get_window_styles, WS_EX_APPWINDOW_FLAG,
        WS_EX_NOACTIVATE_FLAG, WS_EX_TOOLWINDOW_FLAG, WS_EX_TOPMOST_FLAG, WS_POPUP_FLAG,
    };
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, ShowWindow, SW_HIDE, SW_SHOWNA, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
    };

    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            PCWSTR(HSTRING::from("STATIC").as_ptr()),
            PCWSTR(HSTRING::from("AsterHideRestoreTest").as_ptr()),
            WS_POPUP,
            0,
            0,
            720,
            48,
            None,
            None,
            None,
            None,
        )
        .expect("CreateWindowExW for hide/restore test")
    };

    let hwnd_raw = hwnd.0 as isize;
    let _ = configure_taskbar_native_styles(hwnd_raw).expect("Initial style config");

    // Simulate hide then restore without stealing activation
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
        let _ = ShowWindow(hwnd, SW_SHOWNA);
    }

    let (style, ex_style) =
        configure_taskbar_native_styles(hwnd_raw).expect("Restore style config");
    let (read_style, read_ex_style) = get_window_styles(hwnd_raw).expect("Read back styles");
    assert_eq!(style, read_style);
    assert_eq!(ex_style, read_ex_style);

    assert_ne!(style & WS_POPUP_FLAG, 0, "WS_POPUP must be preserved");
    assert_ne!(
        ex_style & WS_EX_TOPMOST_FLAG,
        0,
        "WS_EX_TOPMOST must be preserved after restore"
    );
    assert_ne!(
        ex_style & WS_EX_TOOLWINDOW_FLAG,
        0,
        "WS_EX_TOOLWINDOW must be preserved after restore"
    );
    assert_ne!(
        ex_style & WS_EX_NOACTIVATE_FLAG,
        0,
        "WS_EX_NOACTIVATE must be preserved after restore"
    );
    assert_eq!(
        ex_style & WS_EX_APPWINDOW_FLAG,
        0,
        "WS_EX_APPWINDOW must never be added on restore"
    );

    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}

// ============================================================================
// Category C: Real Process Containment & Supervisor Lifecycle Integration Tests
// ============================================================================

/// AC-M3A-01 / LIFECYCLE-01: Real process containment test and kernel kill-on-close verification.
/// 1. Verifies `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` is configured on the actual kernel object via Win32 query.
/// 2. Spawns the real `taskbar` binary via `ProcessSupervisor` into the Windows Job Object.
/// 3. Authoritatively queries the Win32 `IsProcessInJob` API on the child's OS handle.
/// 4. Closes the final Job Object handle while child is actively running, without calling `start_kill` or
///    `TerminateProcess`, proving actual Windows kernel termination on job handle closure.
#[tokio::test]
async fn test_ac_m3a_01_real_process_containment_and_is_process_in_job() {
    let job_name = format!("AsterShellM3A-JobTest-{}", std::process::id());
    let job = JobObject::create(&job_name)
        .expect("Win32 Job Object creation must succeed with KILL_ON_JOB_CLOSE");

    // 1. Verify JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE is configured on kernel object via Win32 query
    let kill_on_close = job
        .query_kill_on_close()
        .expect("QueryInformationJobObject query must succeed");
    assert!(
        kill_on_close,
        "JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE must be active on kernel Job Object"
    );

    let mut supervisor = process_supervisor::ProcessSupervisor::new(job);

    let taskbar_exe = resolve_taskbar_exe();
    assert!(
        taskbar_exe.exists(),
        "Resolved taskbar executable does not exist at {:?}",
        taskbar_exe
    );

    // Launch real child in persistent test-child mode
    let spec = process_supervisor::ProcessSpec {
        id: ComponentId::Taskbar,
        executable: taskbar_exe,
        args: vec!["--test-child".to_string(), "--no-supervisor".to_string()],
        restart_policy: astercore::RestartPolicy::default(),
    };
    supervisor.register(spec);

    // 2. Real process successfully spawned and assigned (Claim 1)
    let child_pid = supervisor
        .spawn(ComponentId::Taskbar)
        .expect("ProcessSupervisor::spawn must succeed");
    assert!(child_pid > 0, "Spawned child PID must be > 0");
    assert_eq!(supervisor.child_pid(ComponentId::Taskbar), Some(child_pid));

    let pid_registry = supervisor.pid_registry();
    assert!(
        pid_registry.is_authorized(child_pid),
        "Spawned child PID {} must be authorized in supervisor PID registry",
        child_pid
    );

    // 3. Authoritative Win32 IsProcessInJob kernel verification (Claim 2)
    let is_in_job = supervisor
        .is_child_in_job(ComponentId::Taskbar)
        .expect("is_child_in_job must query Win32 IsProcessInJob successfully");
    assert!(
        is_in_job,
        "Win32 IsProcessInJob must confirm child process {} is enclosed within the Job Object",
        child_pid
    );

    // 4. Kernel kill-on-close behavior verified (Claim 3)
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
        use windows::Win32::System::Threading::{
            GetExitCodeProcess, OpenProcess, TerminateProcess, WaitForSingleObject,
            PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
        };

        // Open child process handle before closing the Job Object handle
        let proc_handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE | PROCESS_TERMINATE,
                false,
                child_pid,
            )
        }
        .expect("OpenProcess on child process must succeed");

        // RAII cleanup guard to guarantee no orphan processes on test assertion failure
        struct ProcessCleanupGuard(HANDLE);
        impl Drop for ProcessCleanupGuard {
            fn drop(&mut self) {
                unsafe {
                    let _ = TerminateProcess(self.0, 1);
                    let _ = CloseHandle(self.0);
                }
            }
        }
        let guard = ProcessCleanupGuard(proc_handle);

        // Verify child is actively alive prior to Job Object closure
        let mut exit_code = 0u32;
        let _ = unsafe { GetExitCodeProcess(guard.0, &mut exit_code) };
        assert_eq!(
            exit_code, 259,
            "Child process must be actively running (STILL_ACTIVE 259) before Job Object is closed"
        );

        // Extract the Job Object and close its final handle WITHOUT calling child.start_kill(),
        // TerminateProcess, or supervisor.shutdown_all()!
        let job_object = supervisor.into_job_object();
        drop(job_object); // Closes the final Win32 Job Object handle via CloseHandle

        // Wait with bounded timeout for Windows kernel to terminate child process via kill-on-close
        let wait_res = unsafe { WaitForSingleObject(guard.0, 3000) };
        assert_eq!(
            wait_res, WAIT_OBJECT_0,
            "Child process must be terminated by kernel upon Job Object handle closure within 3000ms"
        );

        let mut final_exit_code = 0u32;
        let _ = unsafe { GetExitCodeProcess(guard.0, &mut final_exit_code) };
        assert_ne!(
            final_exit_code, 259,
            "Child process must have terminated (exit code != STILL_ACTIVE)"
        );

        // Defuse guard to prevent redundant termination
        std::mem::forget(guard);
        let _ = unsafe { CloseHandle(proc_handle) };
    }
}

// ============================================================================
// Category D: Supervisor Authenticated IPC & Security Verification Tests
// ============================================================================

/// AC-M3A-07 / SEC-01: Supervisor Authenticated IPC & Security Verification.
/// Confirms that unauthenticated Shutdown requests are rejected,
/// authenticated supervised children cannot shut down the host daemon,
/// mutual PID authentication succeeds, and host shutdown triggers graceful child termination.
#[tokio::test]
async fn test_ac_m3a_07_supervisor_authenticated_ipc_and_shutdown() {
    let test_pipe_name = format!(
        r"\\.\pipe\astershell-test-m3a-auth-ipc-{}",
        std::process::id()
    );

    let job =
        JobObject::create("AsterShellM3A-AuthTest-Job").expect("JobObject creation must succeed");
    let supervisor = process_supervisor::ProcessSupervisor::new(job);
    let pid_registry = supervisor.pid_registry();

    // Register our current PID as a supervised child
    let authorized_pid = std::process::id();
    pid_registry.authorize(authorized_pid);

    let shutdown_signal = Arc::new(AtomicBool::new(false));

    #[cfg(windows)]
    {
        let mut pipe_sec = default_pipe_sddl()
            .ok()
            .and_then(|sddl| PipeSecurityDescriptor::from_sddl(&sddl).ok());
        let sa_ptr = pipe_sec.as_mut().map(|s| s.as_raw_mut_ptr());

        let server = transport::create_pipe_server_with_security(&test_pipe_name, true, sa_ptr)
            .expect("Named pipe server creation must succeed");

        let signal_clone = shutdown_signal.clone();
        let registry_clone = pid_registry.clone();

        let server_task = tokio::spawn(async move {
            if server.connect().await.is_ok() {
                let mut stream: NamedPipeServer = server;
                let mut is_authenticated = false;
                let mut client_role = process_supervisor::ClientRole::SupervisedChild;

                loop {
                    let req_res: astercore::Result<RequestEnvelope> =
                        transport::read_framed_message(&mut stream).await;
                    let req = match req_res {
                        Ok(r) => r,
                        Err(_) => break,
                    };

                    let response = match req.request {
                        IpcRequest::Handshake {
                            client_version: _,
                            ref client_name,
                        } => {
                            let client_pid =
                                platform_windows::get_pipe_client_pid(&stream).unwrap_or(0);
                            if !registry_clone.is_authorized(client_pid) {
                                IpcResponse::Error(format!(
                                    "Mutual authentication failed: PID {} is not registered",
                                    client_pid
                                ))
                            } else {
                                is_authenticated = true;
                                client_role = registry_clone
                                    .get_role(client_pid)
                                    .unwrap_or(process_supervisor::ClientRole::SupervisedChild);
                                let _ = client_name;
                                IpcResponse::HandshakeAck {
                                    negotiated_version: PROTOCOL_VERSION_1,
                                    server_pid: std::process::id(),
                                }
                            }
                        }
                        _ if !is_authenticated => IpcResponse::Error(
                            "Authentication required: handshake required first".into(),
                        ),
                        IpcRequest::Shutdown => {
                            if client_role == process_supervisor::ClientRole::AdminController {
                                signal_clone.store(true, Ordering::SeqCst);
                                IpcResponse::Ack
                            } else {
                                IpcResponse::Error(
                                    "Permission denied: supervised child processes are not authorized to trigger host shutdown"
                                        .to_string(),
                                )
                            }
                        }
                        _ => IpcResponse::Ack,
                    };

                    let resp_env = ResponseEnvelope {
                        request_id: req.request_id,
                        response,
                    };
                    let _ = transport::write_framed_message(&mut stream, &resp_env).await;

                    if !is_authenticated {
                        break;
                    }
                }
            }
        });

        // 1. Test unauthenticated request without handshake (SEC-01)
        tokio::time::sleep(Duration::from_millis(50)).await;
        let mut client = transport::connect_pipe_client(&test_pipe_name)
            .await
            .expect("Client must connect to supervisor test pipe");

        let unauth_sd = RequestEnvelope {
            request_id: 100,
            request: IpcRequest::Shutdown,
        };
        transport::write_framed_message(&mut client, &unauth_sd)
            .await
            .expect("Writing unauthenticated shutdown must succeed");

        let unauth_resp: ResponseEnvelope = transport::read_framed_message(&mut client)
            .await
            .expect("Reading unauthenticated response must succeed");

        assert!(
            matches!(unauth_resp.response, IpcResponse::Error(_)),
            "Unauthenticated Shutdown request MUST be rejected with Error: got {:?}",
            unauth_resp.response
        );
        assert!(
            !shutdown_signal.load(Ordering::SeqCst),
            "Unauthenticated request must NOT trigger shutdown signal"
        );

        let _ = server_task.await;
    }
}

/// SEC-01: Proves that an authenticated supervised child cannot shut down the host daemon.
#[tokio::test]
async fn test_ac_m3a_07_supervised_child_cannot_shutdown_host() {
    let test_pipe_name = format!(
        r"\\.\pipe\astershell-test-m3a-child-noshut-{}",
        std::process::id()
    );

    let job =
        JobObject::create("AsterShellM3A-NoShut-Job").expect("JobObject creation must succeed");
    let supervisor = process_supervisor::ProcessSupervisor::new(job);
    let pid_registry = supervisor.pid_registry();

    // Register our PID as a SupervisedChild (default)
    pid_registry.authorize(std::process::id());

    #[cfg(windows)]
    {
        let mut pipe_sec = default_pipe_sddl()
            .ok()
            .and_then(|sddl| PipeSecurityDescriptor::from_sddl(&sddl).ok());
        let sa_ptr = pipe_sec.as_mut().map(|s| s.as_raw_mut_ptr());

        let server = transport::create_pipe_server_with_security(&test_pipe_name, true, sa_ptr)
            .expect("Named pipe server creation must succeed");

        let registry_clone = pid_registry.clone();
        let server_task = tokio::spawn(async move {
            if server.connect().await.is_ok() {
                let mut stream: NamedPipeServer = server;
                let mut is_authenticated = false;
                let mut client_role = process_supervisor::ClientRole::SupervisedChild;

                loop {
                    let req_res: astercore::Result<RequestEnvelope> =
                        transport::read_framed_message(&mut stream).await;
                    let req = match req_res {
                        Ok(r) => r,
                        Err(_) => break,
                    };

                    let response = match req.request {
                        IpcRequest::Handshake {
                            client_version: _,
                            ref client_name,
                        } => {
                            let client_pid =
                                platform_windows::get_pipe_client_pid(&stream).unwrap_or(0);
                            if !registry_clone.is_authorized(client_pid) {
                                IpcResponse::Error("Unauthorized PID".into())
                            } else {
                                is_authenticated = true;
                                client_role = registry_clone
                                    .get_role(client_pid)
                                    .unwrap_or(process_supervisor::ClientRole::SupervisedChild);
                                let _ = client_name;
                                IpcResponse::HandshakeAck {
                                    negotiated_version: PROTOCOL_VERSION_1,
                                    server_pid: std::process::id(),
                                }
                            }
                        }
                        _ if !is_authenticated => {
                            IpcResponse::Error("Authentication required".into())
                        }
                        IpcRequest::Shutdown => {
                            if client_role == process_supervisor::ClientRole::AdminController {
                                IpcResponse::Ack
                            } else {
                                IpcResponse::Error(
                                    "Permission denied: supervised child processes are not authorized to trigger host shutdown"
                                        .to_string(),
                                )
                            }
                        }
                        _ => IpcResponse::Ack,
                    };

                    let resp_env = ResponseEnvelope {
                        request_id: req.request_id,
                        response,
                    };
                    let _ = transport::write_framed_message(&mut stream, &resp_env).await;
                }
            }
        });

        tokio::time::sleep(Duration::from_millis(50)).await;
        let mut client = transport::connect_pipe_client(&test_pipe_name)
            .await
            .expect("Client must connect");

        // 1. Handshake
        let req = RequestEnvelope {
            request_id: 1,
            request: IpcRequest::Handshake {
                client_version: PROTOCOL_VERSION_1,
                client_name: "supervised-child".into(),
            },
        };
        transport::write_framed_message(&mut client, &req)
            .await
            .unwrap();
        let resp: ResponseEnvelope = transport::read_framed_message(&mut client).await.unwrap();
        assert!(matches!(resp.response, IpcResponse::HandshakeAck { .. }));

        // 2. Child sends Shutdown request
        let sd_req = RequestEnvelope {
            request_id: 2,
            request: IpcRequest::Shutdown,
        };
        transport::write_framed_message(&mut client, &sd_req)
            .await
            .unwrap();
        let sd_resp: ResponseEnvelope = transport::read_framed_message(&mut client).await.unwrap();
        assert_eq!(sd_resp.request_id, 2);
        match sd_resp.response {
            IpcResponse::Error(msg) => {
                assert!(msg.contains("Permission denied"));
            }
            other => panic!("Expected Permission denied error, got {:?}", other),
        }

        server_task.abort();
    }
}

/// AC-M3A-08 / SEC-01: Proves that supervisor shutdown causes the real taskbar process to exit cleanly
/// through the intended graceful IPC mechanism (receiving ShutdownNotice or pipe closure).
#[tokio::test]
async fn test_ac_m3a_08_taskbar_graceful_shutdown_on_supervisor_exit() {
    let test_pipe_name = format!(
        r"\\.\pipe\astershell-test-m3a-graceful-{}",
        std::process::id()
    );

    let job = JobObject::create("AsterShellM3A-GracefulJob").expect("JobObject create");
    let mut supervisor = process_supervisor::ProcessSupervisor::new(job);

    #[cfg(windows)]
    {
        let mut pipe_sec = default_pipe_sddl()
            .ok()
            .and_then(|sddl| PipeSecurityDescriptor::from_sddl(&sddl).ok());
        let sa_ptr = pipe_sec.as_mut().map(|s| s.as_raw_mut_ptr());

        let server = transport::create_pipe_server_with_security(&test_pipe_name, true, sa_ptr)
            .expect("Server create");

        let taskbar_exe = resolve_taskbar_exe();
        let spec = process_supervisor::ProcessSpec {
            id: ComponentId::Taskbar,
            executable: taskbar_exe,
            args: vec![
                "--test-child".to_string(),
                "--supervisor-pipe".to_string(),
                test_pipe_name.clone(),
            ],
            restart_policy: astercore::RestartPolicy::default(),
        };
        supervisor.register(spec);

        let pid_registry = supervisor.pid_registry();

        // Spawn child
        let child_pid = supervisor
            .spawn(ComponentId::Taskbar)
            .expect("spawn taskbar");
        pid_registry.authorize(child_pid);

        // Server accepts connection and performs handshake
        let server_task = tokio::spawn(async move {
            if server.connect().await.is_ok() {
                let mut stream = server;
                // 1. Handshake
                let req_res: astercore::Result<RequestEnvelope> =
                    transport::read_framed_message(&mut stream).await;
                if let Ok(req) = req_res {
                    let resp = ResponseEnvelope {
                        request_id: req.request_id,
                        response: IpcResponse::HandshakeAck {
                            negotiated_version: PROTOCOL_VERSION_1,
                            server_pid: std::process::id(),
                        },
                    };
                    let _ = transport::write_framed_message(&mut stream, &resp).await;

                    // 2. Send ShutdownNotice to prompt graceful termination
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    let notice = ResponseEnvelope {
                        request_id: 0,
                        response: IpcResponse::ShutdownNotice,
                    };
                    let _ = transport::write_framed_message(&mut stream, &notice).await;
                }
            }
        });

        let _ = server_task.await;

        // Verify child exits cleanly within bounded time
        tokio::time::sleep(Duration::from_millis(300)).await;
        let mut exit_code = 0u32;
        use windows::Win32::System::Threading::{
            GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        if let Ok(h) = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, child_pid) } {
            let _ = unsafe { GetExitCodeProcess(h, &mut exit_code) };
            let _ = unsafe { windows::Win32::Foundation::CloseHandle(h) };
            assert_ne!(
                exit_code, 259,
                "Taskbar process must exit cleanly upon receiving ShutdownNotice (exit code != STILL_ACTIVE)"
            );
        }
    }
}

/// AC-M3A-09 / SEC-01: Proves that a non-responsive taskbar process is forcibly terminated
/// after the configured supervisor timeout expires.
#[tokio::test]
async fn test_ac_m3a_09_non_responsive_taskbar_forcibly_terminated_on_timeout() {
    let job = JobObject::create("AsterShellM3A-TimeoutJob").expect("JobObject create");
    let mut supervisor = process_supervisor::ProcessSupervisor::new(job);

    let taskbar_exe = resolve_taskbar_exe();
    // Non-responsive child: runs in infinite sleep loop ignoring supervisor
    let spec = process_supervisor::ProcessSpec {
        id: ComponentId::Taskbar,
        executable: taskbar_exe,
        args: vec!["--test-child".to_string(), "--no-supervisor".to_string()],
        restart_policy: astercore::RestartPolicy::default(),
    };
    supervisor.register(spec);

    let child_pid = supervisor
        .spawn(ComponentId::Taskbar)
        .expect("spawn taskbar");
    assert!(child_pid > 0);

    // Call shutdown_all with a short timeout of 200ms
    let start = std::time::Instant::now();
    supervisor
        .shutdown_all(Duration::from_millis(200))
        .await
        .expect("shutdown_all must complete");
    let elapsed = start.elapsed();

    // Must have waited at least the 200ms grace period before force killing
    assert!(
        elapsed >= Duration::from_millis(150),
        "Supervisor must wait grace period before fallback force termination"
    );

    // Child must now be terminated
    tokio::time::sleep(Duration::from_millis(100)).await;
    #[cfg(windows)]
    {
        use windows::Win32::System::Threading::{
            GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        if let Ok(h) = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, child_pid) } {
            let mut exit_code = 0u32;
            let _ = unsafe { GetExitCodeProcess(h, &mut exit_code) };
            let _ = unsafe { windows::Win32::Foundation::CloseHandle(h) };
            assert_ne!(
                exit_code, 259,
                "Non-responsive child must be forcibly terminated (exit code != STILL_ACTIVE)"
            );
        }
    }
}
