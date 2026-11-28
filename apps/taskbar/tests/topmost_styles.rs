//! Real Slint/winit HWND Topmost Z-Order & Extended Styles Integration Test
//!
//! Runs in its own test binary process so `slint::run_event_loop()` has a clean
//! winit EventLoop on Windows.

use platform_windows::{
    apply_native_taskbar_configuration, get_window_styles, WS_EX_APPWINDOW_FLAG,
    WS_EX_NOACTIVATE_FLAG, WS_EX_TOOLWINDOW_FLAG, WS_EX_TOPMOST_FLAG, WS_POPUP_FLAG,
};
use std::sync::{Arc, Mutex};
use std::time::Duration;

slint::slint! {
    #[edition = "1.19"]
    export component TopmostTestWindow inherits Window {
        title: "AsterShell Topmost Integration Test";
        preferred-width: 720px;
        preferred-height: 48px;
        min-width: 720px;
        max-width: 720px;
        min-height: 48px;
        max-height: 48px;
        no-frame: true;
    }
}

#[cfg(windows)]
fn is_hwnd_above_in_zorder(
    upper: windows::Win32::Foundation::HWND,
    lower: windows::Win32::Foundation::HWND,
) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{GetTopWindow, GetWindow, GW_HWNDNEXT};

    let mut cur = unsafe { GetTopWindow(None) }.ok();
    let mut seen_upper = false;
    while let Some(h) = cur {
        if h == upper {
            seen_upper = true;
        } else if h == lower {
            return seen_upper;
        }
        cur = unsafe { GetWindow(h, GW_HWNDNEXT) }.ok();
    }
    false
}

#[cfg(windows)]
#[derive(Debug)]
struct TopmostProbeResult {
    before_style: u32,
    before_ex_style: u32,
    before_above_normal: bool,
    after_style: u32,
    after_ex_style: u32,
    after_above_normal: bool,
    had_other_foreground: bool,
    stole_foreground: bool,
}

#[cfg(windows)]
#[test]
fn test_real_slint_winit_hwnd_topmost_zorder_and_hide_restore() {
    use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use slint::winit_030::winit::window::WindowLevel;
    use slint::winit_030::WinitWindowAccessor;
    use slint::ComponentHandle;
    use windows::core::w;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, GetForegroundWindow, SetForegroundWindow, WINDOW_EX_STYLE,
        WS_OVERLAPPEDWINDOW, WS_VISIBLE,
    };

    // Create a normal non-topmost reference window to verify real Z-order placement.
    let normal_hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("STATIC"),
            w!("AsterNormalZOrderRef"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            0,
            0,
            120,
            120,
            None,
            None,
            None,
            None,
        )
        .expect("CreateWindowExW for normal reference window must succeed")
    };

    let win = TopmostTestWindow::new().expect("TopmostTestWindow::new must succeed");
    win.show().expect("TopmostTestWindow::show must succeed");

    let result_slot: Arc<Mutex<Option<TopmostProbeResult>>> = Arc::new(Mutex::new(None));
    let result_clone = result_slot.clone();
    let win_weak = win.as_weak();
    let phase = Arc::new(Mutex::new(0u8));
    let before_state = Arc::new(Mutex::new(None::<(u32, u32, bool)>));
    let had_other_fg = Arc::new(Mutex::new(false));

    let timer = slint::Timer::default();
    let timer_phase = phase.clone();
    let before_clone = before_state.clone();
    let had_other_fg_cb = had_other_fg.clone();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(25),
        move || {
            let Some(w) = win_weak.upgrade() else {
                let _ = slint::quit_event_loop();
                return;
            };

            let mut current_phase = timer_phase.lock().unwrap();
            match *current_phase {
                0 => {
                    // Phase 0: Wait for winit_window realization, apply AlwaysOnTop + native styles, verify before hide.
                    let hwnd_opt = w
                        .window()
                        .with_winit_window(|winit_window| {
                            winit_window.set_window_level(WindowLevel::AlwaysOnTop);
                            match winit_window.window_handle() {
                                Ok(handle) => match handle.as_raw() {
                                    RawWindowHandle::Win32(win32) => Some(win32.hwnd.get()),
                                    _ => None,
                                },
                                Err(_) => None,
                            }
                        })
                        .flatten();

                    if let Some(hwnd_raw) = hwnd_opt {
                        let _ = apply_native_taskbar_configuration(hwnd_raw)
                            .expect("apply_native_taskbar_configuration before hide must succeed");
                        let (style, ex_style) =
                            get_window_styles(hwnd_raw).expect("get_window_styles before hide");
                        let hwnd = HWND(hwnd_raw as _);
                        let above = is_hwnd_above_in_zorder(hwnd, normal_hwnd);
                        *before_clone.lock().unwrap() = Some((style, ex_style, above));

                        // Trigger hide and give foreground to normal_hwnd before testing restore
                        let _ = w.hide();
                        unsafe {
                            let _ = SetForegroundWindow(normal_hwnd);
                        }
                        *current_phase = 1;
                    }
                }
                1 => {
                    // Phase 1: Record foreground owner before restore, then restore via show() and re-sync AlwaysOnTop + native styles.
                    let fg_before = unsafe { GetForegroundWindow() };
                    let own_hwnd = w
                        .window()
                        .with_winit_window(|winit_window| match winit_window.window_handle() {
                            Ok(handle) => match handle.as_raw() {
                                RawWindowHandle::Win32(win32) => Some(HWND(win32.hwnd.get() as _)),
                                _ => None,
                            },
                            Err(_) => None,
                        })
                        .flatten();
                    *had_other_fg_cb.lock().unwrap() =
                        !fg_before.0.is_null() && Some(fg_before) != own_hwnd;

                    let _ = w.show();
                    let _ = w.window().with_winit_window(|winit_window| {
                        winit_window.set_window_level(WindowLevel::AlwaysOnTop);
                        if let Ok(handle) = winit_window.window_handle() {
                            if let RawWindowHandle::Win32(win32) = handle.as_raw() {
                                let _ = apply_native_taskbar_configuration(win32.hwnd.get());
                            }
                        }
                    });
                    *current_phase = 2;
                }
                _ => {
                    // Phase 2: Post-restore verification after event loop processes show().
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

                    if let Some(hwnd_raw) = hwnd_opt {
                        let (after_style, after_ex_style) =
                            get_window_styles(hwnd_raw).expect("get_window_styles after restore");
                        let hwnd = HWND(hwnd_raw as _);
                        let after_above_normal = is_hwnd_above_in_zorder(hwnd, normal_hwnd);
                        let fg = unsafe { GetForegroundWindow() };
                        let (before_style, before_ex_style, before_above_normal) =
                            before_clone.lock().unwrap().take().unwrap();

                        *result_clone.lock().unwrap() = Some(TopmostProbeResult {
                            before_style,
                            before_ex_style,
                            before_above_normal,
                            after_style,
                            after_ex_style,
                            after_above_normal,
                            had_other_foreground: *had_other_fg_cb.lock().unwrap(),
                            stole_foreground: fg == hwnd,
                        });
                    }
                    let _ = slint::quit_event_loop();
                }
            }
        },
    );

    slint::run_event_loop_until_quit().expect("slint::run_event_loop_until_quit must succeed");

    unsafe {
        let _ = DestroyWindow(normal_hwnd);
    }

    let res = result_slot
        .lock()
        .unwrap()
        .take()
        .expect("Real Slint/winit window must complete topmost probe");

    for (label, style, ex_style, above) in [
        (
            "before_hide",
            res.before_style,
            res.before_ex_style,
            res.before_above_normal,
        ),
        (
            "after_restore",
            res.after_style,
            res.after_ex_style,
            res.after_above_normal,
        ),
    ] {
        assert_ne!(style & WS_POPUP_FLAG, 0, "{}: WS_POPUP must be set", label);
        assert_ne!(
            ex_style & WS_EX_TOPMOST_FLAG,
            0,
            "{}: WS_EX_TOPMOST must be set",
            label
        );
        assert_ne!(
            ex_style & WS_EX_TOOLWINDOW_FLAG,
            0,
            "{}: WS_EX_TOOLWINDOW must be set",
            label
        );
        assert_ne!(
            ex_style & WS_EX_NOACTIVATE_FLAG,
            0,
            "{}: WS_EX_NOACTIVATE must be set",
            label
        );
        assert_eq!(
            ex_style & WS_EX_APPWINDOW_FLAG,
            0,
            "{}: WS_EX_APPWINDOW must NOT be set",
            label
        );
        assert!(
            above,
            "{}: Slint/winit HWND must be above normal non-topmost HWND in Z-order",
            label
        );
    }

    if res.had_other_foreground {
        assert!(
            !res.stole_foreground,
            "Topmost taskbar HWND must not steal foreground focus on show/restore"
        );
    }
}
