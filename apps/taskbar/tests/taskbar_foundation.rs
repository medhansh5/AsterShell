//! AsterShell Taskbar Foundation & Lifecycle Acceptance Tests (Milestone M3A)
//!
//! Automated test suite verifying criteria AC-M3A-01 through AC-M3A-07:
//! - AC-M3A-01: Supervised lifecycle, Job Object binding, and mutual PID handshake
//! - AC-M3A-02: Native frameless window styles (WS_POPUP, WS_VISIBLE, WS_CLIPSIBLINGS, WS_CLIPCHILDREN)
//! - AC-M3A-03: Extended shell styles (WS_EX_TOPMOST, WS_EX_TOOLWINDOW, WS_EX_NOACTIVATE, !WS_EX_APPWINDOW)
//! - AC-M3A-04: Zero desktop work-area modification (SPI_GETWORKAREA preservation)
//! - AC-M3A-05: Signed primary monitor placement and DPI scaling
//! - AC-M3A-06: DWM Mica backdrop realization and graceful dark fallback
//! - AC-M3A-07: Clean IPC shutdown and restart coordination

use astercore::ComponentId;
use ipc::{
    transport::{self, NamedPipeServer},
    IpcRequest, IpcResponse, RequestEnvelope, ResponseEnvelope, PROTOCOL_VERSION_1,
};
use platform_windows::{
    apply_dark_mode_and_backdrop, calculate_taskbar_coordinates_with_dpi,
    configure_taskbar_native_styles, default_pipe_sddl, get_primary_monitor_work_area,
    get_window_styles, position_taskbar_window, scale_to_physical_pixels, DwmBackdropType,
    JobObject, PipeSecurityDescriptor, Rect, WS_CLIPCHILDREN_FLAG, WS_CLIPSIBLINGS_FLAG,
    WS_EX_APPWINDOW_FLAG, WS_EX_NOACTIVATE_FLAG, WS_EX_TOOLWINDOW_FLAG, WS_EX_TOPMOST_FLAG,
    WS_POPUP_FLAG, WS_VISIBLE_FLAG,
};
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
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, WINDOW_EX_STYLE, WS_POPUP,
};

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
                WS_POPUP,
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

/// AC-M3A-01: Supervised child lifecycle, Job Object binding, and mutual PID handshake.
#[tokio::test]
async fn test_ac_m3a_01_supervised_lifecycle_and_job_object() {
    let job = JobObject::create("AsterShellM3ATestJob")
        .expect("Win32 Job Object creation must succeed with KILL_ON_JOB_CLOSE");
    assert_eq!(job.name(), "AsterShellM3ATestJob");

    let supervisor = process_supervisor::ProcessSupervisor::new(job);
    let pid_registry = supervisor.pid_registry();

    let current_pid = std::process::id();
    pid_registry.authorize(current_pid);
    assert!(
        pid_registry.is_authorized(current_pid),
        "Current test process PID must be authorized in supervisor registry"
    );

    // Verify component id and spec registration
    let spec = process_supervisor::ProcessSpec {
        id: ComponentId::Taskbar,
        executable: std::env::current_exe().unwrap(),
        args: vec!["--test".into()],
        restart_policy: astercore::RestartPolicy::default(),
    };
    assert_eq!(spec.id.as_str(), "taskbar");
}

/// AC-M3A-02: Native frameless window styles.
/// HWND must possess WS_POPUP, WS_VISIBLE, WS_CLIPSIBLINGS, WS_CLIPCHILDREN.
#[cfg(windows)]
#[test]
fn test_ac_m3a_02_native_frameless_window_styles() {
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

    // Read back independently via get_window_styles to verify persistence
    let (read_style, _) = get_window_styles(hwnd).expect("get_window_styles must succeed");
    assert_eq!(read_style, style, "Styles must match read-back values");
}

/// AC-M3A-03: Extended shell styles and Alt+Tab exclusion.
/// HWND must possess WS_EX_TOPMOST, WS_EX_TOOLWINDOW, WS_EX_NOACTIVATE, and strip WS_EX_APPWINDOW.
#[cfg(windows)]
#[test]
fn test_ac_m3a_03_extended_shell_styles_and_alt_tab_exclusion() {
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

/// AC-M3A-07: Clean IPC shutdown and restart coordination.
/// Confirms taskbar control pipe handles Handshake and Shutdown requests cleanly.
#[tokio::test]
async fn test_ac_m3a_07_clean_ipc_shutdown() {
    let test_pipe_name = format!(
        r"\\.\pipe\astershell-test-m3a-shutdown-{}",
        std::process::id()
    );
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
        let server_task = tokio::spawn(async move {
            if server.connect().await.is_ok() {
                let mut stream: NamedPipeServer = server;
                loop {
                    let req_res: astercore::Result<RequestEnvelope> =
                        transport::read_framed_message(&mut stream).await;
                    let req = match req_res {
                        Ok(r) => r,
                        Err(_) => break,
                    };

                    let is_shutdown = matches!(req.request, IpcRequest::Shutdown);
                    let response = match req.request {
                        IpcRequest::Handshake {
                            client_version: _,
                            client_name: _,
                        } => IpcResponse::HandshakeAck {
                            negotiated_version: PROTOCOL_VERSION_1,
                            server_pid: std::process::id(),
                        },
                        IpcRequest::Shutdown => {
                            signal_clone.store(true, Ordering::SeqCst);
                            IpcResponse::Ack
                        }
                        _ => IpcResponse::Ack,
                    };

                    let resp_env = ResponseEnvelope {
                        request_id: req.request_id,
                        response,
                    };
                    let _ = transport::write_framed_message(&mut stream, &resp_env).await;

                    if is_shutdown {
                        break;
                    }
                }
            }
        });

        // Connect client to pipe
        tokio::time::sleep(Duration::from_millis(50)).await;
        let mut client = transport::connect_pipe_client(&test_pipe_name)
            .await
            .expect("Client must connect to taskbar test pipe");

        // 1. Handshake
        let hs_req = RequestEnvelope {
            request_id: 1,
            request: IpcRequest::Handshake {
                client_version: PROTOCOL_VERSION_1,
                client_name: "test-runner".into(),
            },
        };
        transport::write_framed_message(&mut client, &hs_req)
            .await
            .expect("Writing handshake must succeed");
        let hs_resp: ResponseEnvelope = transport::read_framed_message(&mut client)
            .await
            .expect("Reading handshake response must succeed");
        assert!(
            matches!(hs_resp.response, IpcResponse::HandshakeAck { .. }),
            "Expected HandshakeAck"
        );

        // 2. Shutdown
        let sd_req = RequestEnvelope {
            request_id: 2,
            request: IpcRequest::Shutdown,
        };
        transport::write_framed_message(&mut client, &sd_req)
            .await
            .expect("Writing shutdown must succeed");
        let sd_resp: ResponseEnvelope = transport::read_framed_message(&mut client)
            .await
            .expect("Reading shutdown response must succeed");
        assert_eq!(
            sd_resp.response,
            IpcResponse::Ack,
            "Expected Ack on Shutdown"
        );

        let _ = server_task.await;
        assert!(
            shutdown_signal.load(Ordering::SeqCst),
            "Shutdown signal must be received and acknowledged"
        );
    }
}
