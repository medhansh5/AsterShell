//! AsterShell M3F Daily-Driver End-to-End Integration, Reliability & Performance Tests.

use platform_windows::{
    calculate_monitor_taskbar_coordinates, enumerate_monitors, find_monitor_by_target,
    get_primary_monitor_work_area, query_master_volume, query_network_status, query_power_status,
    AudioMonitorHandle, NetworkMonitorHandle, PowerMonitorHandle, TrayIconHandle,
};
use std::time::{Duration, Instant};
use window_tracker::{
    enumerate_windows, WindowEvent, WindowInfo, WindowTracker, WindowTrackerState,
};

#[test]
fn test_m3f_work_area_invariant_preserved() {
    let before = get_primary_monitor_work_area().expect("work area before");
    let monitors = enumerate_monitors().expect("enumerate monitors");
    assert!(!monitors.is_empty());

    let primary = find_monitor_by_target("primary").expect("find primary monitor");
    let (x, y, w, h) = calculate_monitor_taskbar_coordinates(&primary, 720, 48, None);
    assert!(w > 0 && h > 0);
    assert_eq!(y, primary.work_rect.top);
    assert!(x >= primary.work_rect.left);

    let after = get_primary_monitor_work_area().expect("work area after");
    assert_eq!(
        before, after,
        "Floating companion mode must never modify the Windows desktop work area"
    );
}

#[test]
fn test_m3f_window_tracker_soak_and_latency() {
    let t0 = Instant::now();
    let initial = enumerate_windows().expect("enumerate_windows");
    let enum_elapsed = t0.elapsed();
    println!(
        "[M3F PERF] enumerate_windows() returned {} windows in {:?}",
        initial.len(),
        enum_elapsed
    );
    assert!(
        enum_elapsed < Duration::from_secs(2),
        "Window enumeration exceeded 2s budget: {:?}",
        enum_elapsed
    );

    let t1 = Instant::now();
    let (tracker, rx) = WindowTracker::start().expect("WindowTracker::start");
    let first_ev = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("Snapshot event");
    let start_elapsed = t1.elapsed();
    println!(
        "[M3F PERF] WindowTracker::start() + Snapshot in {:?}",
        start_elapsed
    );
    assert!(matches!(first_ev, WindowEvent::Snapshot(_)));
    drop(tracker);

    // 1,000-cycle rapid event soak on WindowTrackerState
    let t2 = Instant::now();
    let mut state = WindowTrackerState::new();
    state.apply_event(first_ev);
    for i in 0..1000isize {
        let hwnd = 900_000 + (i % 50);
        state.apply_event(WindowEvent::WindowAdded(WindowInfo {
            hwnd,
            title: format!("App {}", i),
            class_name: "SoakClass".to_string(),
            pid: 10_000 + (i as u32 % 50),
            process_name: "soak".to_string(),
            executable_path: r"C:\soak.exe".to_string(),
            is_minimized: i % 3 == 0,
            is_focused: i % 2 == 0,
            monitor_hmonitor: 1,
        }));
        state.apply_event(WindowEvent::FocusChanged(Some(hwnd)));
        if i % 5 == 0 {
            state.apply_event(WindowEvent::WindowRemoved(hwnd));
        }
    }
    let soak_elapsed = t2.elapsed();
    println!(
        "[M3F PERF] 1,000-cycle WindowTrackerState soak completed in {:?}",
        soak_elapsed
    );
    assert!(
        soak_elapsed < Duration::from_millis(250),
        "State soak took too long: {:?}",
        soak_elapsed
    );
}

#[test]
fn test_m3f_system_indicators_and_tray_lifecycle() {
    let t0 = Instant::now();
    let vol = query_master_volume().expect("query_master_volume");
    let net = query_network_status().expect("query_network_status");
    let pwr = query_power_status().expect("query_power_status");
    let query_elapsed = t0.elapsed();
    println!(
        "[M3F PERF] System indicator queries (audio={}%, net={}, battery={}%): {:?}",
        vol.volume_percent, net.label, pwr.percent, query_elapsed
    );

    let (audio_h, audio_rx) = AudioMonitorHandle::start().expect("AudioMonitorHandle");
    let (net_h, net_rx) = NetworkMonitorHandle::start().expect("NetworkMonitorHandle");
    let (pwr_h, pwr_rx) = PowerMonitorHandle::start().expect("PowerMonitorHandle");
    let (tray_h, _tray_rx) = TrayIconHandle::start("AsterShell M3F Soak").expect("TrayIconHandle");

    assert!(audio_rx.recv_timeout(Duration::from_secs(2)).is_ok());
    assert!(net_rx.recv_timeout(Duration::from_secs(2)).is_ok());
    assert!(pwr_rx.recv_timeout(Duration::from_secs(2)).is_ok());

    drop(tray_h);
    drop(pwr_h);
    drop(net_h);
    drop(audio_h);
}
