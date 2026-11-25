//! AsterShell Spotlight Search Launcher (`search-ui`)
//!
//! High-performance, keyboard-first desktop search overlay built with Slint,
//! integrating federated search providers, Win32 global hotkeys, DWM styling,
//! safe launch execution, and persistent supervisor IPC authentication.

use astercore::ComponentId;
use diagnostics::{init_diagnostics, DiagnosticConfig};
use platform_windows::{
    apply_dark_mode_and_backdrop, calculate_spotlight_coordinates_with_dpi, get_window_dpi,
    position_window, register_global_hotkey, DwmBackdropType, HotkeyConfig,
};
use search_core::{SearchEvent, SearchOrchestrator, SearchResult};
use search_providers::{ApplicationDiscoveryProvider, CalculatorProvider, WindowsSearchProvider};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

slint::slint! {
    #[edition = "1.19"]
    import { ScrollView } from "std-widgets.slint";

    export struct SearchItemData {
        id: string,
        title: string,
        subtitle: string,
        category: string,
        badge: string,
        score: float,
    }

    export component SpotlightWindow inherits Window {
        title: "AsterShell Spotlight";
        preferred-width: 640px;
        preferred-height: 480px;
        min-width: 640px;
        max-width: 640px;
        min-height: 480px;
        max-height: 480px;
        background: rgba(20, 20, 24, 0.94);
        no-frame: true;

        in-out property <string> search_text: "";
        in-out property <int> selected_index: 0;
        in-out property <[SearchItemData]> results: [];
        in-out property <bool> is_searching: false;
        in-out property <string> status_message: "Ready";
        in-out property <string> stats_text: "0 results";

        callback search_input_changed(string);
        callback item_activated(int);
        callback dismiss_requested();
        callback move_selection(int);

        forward-focus: search_input;

        VerticalLayout {
            padding: 12px;
            spacing: 8px;

            // Search Header Box
            Rectangle {
                height: 52px;
                background: #1e1e24;
                border-radius: 10px;
                border-width: 1px;
                border-color: #2e2e38;

                HorizontalLayout {
                    padding-left: 14px;
                    padding-right: 14px;
                    spacing: 10px;
                    alignment: center;

                    // Magnifier Icon
                    Text {
                        text: "🔍";
                        font-size: 16px;
                        vertical-alignment: center;
                    }

                    // Search Input Text Field
                    search_input := TextInput {
                        text <=> root.search_text;
                        font-size: 16px;
                        color: #f4f4f6;
                        selection-background-color: #3b82f6;
                        vertical-alignment: center;

                        edited => {
                            root.search_input_changed(self.text);
                        }

                        key-pressed(event) => {
                            if (event.text == Key.DownArrow) {
                                root.move_selection(1);
                                return accept;
                            }
                            if (event.text == Key.UpArrow) {
                                root.move_selection(-1);
                                return accept;
                            }
                            if (event.text == Key.Return) {
                                root.item_activated(root.selected_index);
                                return accept;
                            }
                            if (event.text == Key.Escape) {
                                root.dismiss_requested();
                                return accept;
                            }
                            return reject;
                        }
                    }

                    // ESC Dismiss Badge
                    Rectangle {
                        width: 38px;
                        height: 22px;
                        background: #272730;
                        border-radius: 4px;
                        border-width: 1px;
                        border-color: #3a3a46;

                        Text {
                            text: "ESC";
                            font-size: 10px;
                            color: #a0a0ab;
                            horizontal-alignment: center;
                            vertical-alignment: center;
                        }
                    }
                }
            }

            // Results List Container
            Rectangle {
                vertical-stretch: 1;
                background: #18181c;
                border-radius: 10px;
                border-width: 1px;
                border-color: rgb(38, 38, 46);
                clip: true;

                if root.results.length == 0: VerticalLayout {
                    alignment: center;
                    spacing: 8px;

                    Text {
                        text: root.search_text == "" ? "AsterShell Spotlight" : "No matching results found";
                        font-size: 15px;
                        font-weight: 600;
                        color: #71717a;
                        horizontal-alignment: center;
                    }

                    Text {
                        text: root.search_text == "" ? "Start typing to search apps, files, or evaluate calculations" : "Try searching for a different keyword or calculation";
                        font-size: 12px;
                        color: #52525b;
                        horizontal-alignment: center;
                    }
                }

                if root.results.length > 0: ScrollView {
                    VerticalLayout {
                        spacing: 2px;
                        padding: 4px;

                        for item[index] in root.results: Rectangle {
                            height: 48px;
                            background: root.selected_index == index ? #262632 : (touch.has-hover ? #1c1c24 : #00000000);
                            border-radius: 6px;

                            touch := TouchArea {
                                clicked => {
                                    root.selected_index = index;
                                    root.item_activated(index);
                                }
                            }

                            HorizontalLayout {
                                padding-left: 10px;
                                padding-right: 12px;
                                spacing: 10px;
                                alignment: center;

                                // Left selection indicator bar
                                Rectangle {
                                    width: 3px;
                                    height: 24px;
                                    border-radius: 2px;
                                    background: root.selected_index == index ? #3b82f6 : #00000000;
                                }

                                // Category Badge
                                Rectangle {
                                    width: 44px;
                                    height: 22px;
                                    border-radius: 4px;
                                    background: item.badge == "APP" ? #1e3a8a : (item.badge == "CALC" ? #78350f : (item.badge == "DOC" ? #064e3b : #27272a));

                                    Text {
                                        text: item.badge;
                                        font-size: 10px;
                                        font-weight: 700;
                                        color: item.badge == "APP" ? #93c5fd : (item.badge == "CALC" ? #fde68a : (item.badge == "DOC" ? #a7f3d0 : #d4d4d8));
                                        horizontal-alignment: center;
                                        vertical-alignment: center;
                                    }
                                }

                                // Title & Subtitle
                                VerticalLayout {
                                    horizontal-stretch: 1;
                                    spacing: 2px;
                                    alignment: center;

                                    Text {
                                        text: item.title;
                                        font-size: 13px;
                                        font-weight: 600;
                                        color: root.selected_index == index ? #ffffff : #e4e4e7;
                                        overflow: elide;
                                    }

                                    Text {
                                        text: item.subtitle;
                                        font-size: 11px;
                                        color: #71717a;
                                        overflow: elide;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Footer Status Bar
            Rectangle {
                height: 28px;
                background: #16161a;
                border-radius: 6px;

                HorizontalLayout {
                    padding-left: 10px;
                    padding-right: 10px;
                    alignment: center;

                    Text {
                        text: "↑↓ Navigate   ↵ Open   Esc Dismiss   Alt+Space Toggle";
                        font-size: 11px;
                        color: #71717a;
                        vertical-alignment: center;
                    }

                    Rectangle { horizontal-stretch: 1; }

                    Text {
                        text: root.stats_text;
                        font-size: 11px;
                        color: #71717a;
                        vertical-alignment: center;
                    }
                }
            }
        }
    }
}

/// Converts a `SearchResult` model into a Slint-renderable `SearchItemData`.
pub fn convert_result_to_item(res: &SearchResult) -> SearchItemData {
    let badge = match res.category {
        search_core::SearchResultCategory::Application => "APP",
        search_core::SearchResultCategory::Calculation => "CALC",
        search_core::SearchResultCategory::Document => "DOC",
        search_core::SearchResultCategory::File => "FILE",
        search_core::SearchResultCategory::Folder => "DIR",
        search_core::SearchResultCategory::Command => "CMD",
        search_core::SearchResultCategory::Setting => "SET",
        search_core::SearchResultCategory::Web => "WEB",
        search_core::SearchResultCategory::Custom(_) => "ITEM",
    };

    SearchItemData {
        id: res.id.clone().into(),
        title: res.title.clone().into(),
        subtitle: res.subtitle.clone().into(),
        category: format!("{:?}", res.category).into(),
        badge: badge.into(),
        score: res.score,
    }
}

pub fn create_search_orchestrator() -> Arc<tokio::sync::Mutex<SearchOrchestrator>> {
    let mut orchestrator = SearchOrchestrator::new();
    orchestrator.register_provider(Arc::new(ApplicationDiscoveryProvider::new()));
    orchestrator.register_provider(Arc::new(CalculatorProvider::new()));
    orchestrator.register_provider(Arc::new(WindowsSearchProvider::new()));
    Arc::new(tokio::sync::Mutex::new(orchestrator))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_diagnostics(DiagnosticConfig {
        process_name: ComponentId::SearchUi.as_str(),
        default_level: "info",
    });

    tracing::info!("AsterShell Spotlight search-ui initializing...");

    // Create background multi-thread Tokio runtime for search and IPC tasks
    let tokio_rt = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to initialize background Tokio runtime"),
    );

    let orchestrator = create_search_orchestrator();

    // Create Slint Window
    let main_window = SpotlightWindow::new()?;
    let window_weak = main_window.as_weak();

    // Stored live search results matching current Slint view
    let current_results: Arc<Mutex<Vec<SearchResult>>> = Arc::new(Mutex::new(Vec::new()));
    let is_window_visible = Arc::new(AtomicBool::new(true));
    let active_generation = Arc::new(AtomicU64::new(0));

    // Handle search input changes
    {
        let orch = orchestrator.clone();
        let rt = tokio_rt.clone();
        let win_weak = window_weak.clone();
        let cur_res_ref = current_results.clone();
        let active_gen = active_generation.clone();

        main_window.on_search_input_changed(move |query_str| {
            let query = query_str.to_string();
            let orch_clone = orch.clone();
            let win_weak_inner = win_weak.clone();
            let cur_res_inner = cur_res_ref.clone();
            let active_gen_inner = active_gen.clone();

            if query.trim().is_empty() {
                // Invalidate any inflight query generation
                active_gen_inner.fetch_add(1, Ordering::SeqCst);
                if let Ok(mut l) = cur_res_inner.lock() {
                    l.clear();
                }
                if let Some(win) = win_weak_inner.upgrade() {
                    win.set_results(ModelRc::new(VecModel::default()));
                    win.set_selected_index(0);
                    win.set_stats_text("0 results".into());
                    win.set_is_searching(false);
                }
                return;
            }

            if let Some(win) = win_weak_inner.upgrade() {
                win.set_is_searching(true);
            }

            let active_gen_task = active_gen_inner.clone();
            rt.spawn(async move {
                let start = Instant::now();
                let (gen, mut rx) = {
                    let locked = orch_clone.lock().await;
                    locked.execute_search(&query).await
                };
                active_gen_task.store(gen, Ordering::SeqCst);

                while let Some(event) = rx.recv().await {
                    let event_gen = event.generation();
                    let cur_active = active_gen_task.load(Ordering::SeqCst);
                    if event_gen != cur_active {
                        tracing::debug!(
                            event_gen,
                            active_gen = cur_active,
                            "Discarding stale search event for superseded generation"
                        );
                        if event_gen < cur_active {
                            break;
                        }
                        continue;
                    }

                    match event {
                        SearchEvent::ResultsUpdated { results, .. } => {
                            let duration_ms = start.elapsed().as_secs_f32() * 1000.0;
                            let results_count = results.len();
                            let items: Vec<SearchItemData> =
                                results.iter().map(convert_result_to_item).collect();

                            let win_weak_cb = win_weak_inner.clone();
                            let cur_res_cb = cur_res_inner.clone();
                            let results_saved = results.clone();
                            let active_gen_ui = active_gen_task.clone();

                            let _ = slint::invoke_from_event_loop(move || {
                                if active_gen_ui.load(Ordering::SeqCst) != event_gen {
                                    return;
                                }
                                if let Some(win) = win_weak_cb.upgrade() {
                                    if let Ok(mut l) = cur_res_cb.lock() {
                                        *l = results_saved;
                                    }
                                    win.set_results(ModelRc::new(VecModel::from(items)));
                                    win.set_stats_text(
                                        format!("{} results ({:.1} ms)", results_count, duration_ms)
                                            .into(),
                                    );
                                }
                            });
                        }
                        SearchEvent::SearchCompleted { total_results, duration_ms, .. } => {
                            let win_weak_cb = win_weak_inner.clone();
                            let active_gen_ui = active_gen_task.clone();
                            let _ = slint::invoke_from_event_loop(move || {
                                if active_gen_ui.load(Ordering::SeqCst) != event_gen {
                                    return;
                                }
                                if let Some(win) = win_weak_cb.upgrade() {
                                    win.set_is_searching(false);
                                    win.set_stats_text(
                                        format!("{} results ({} ms)", total_results, duration_ms).into(),
                                    );
                                }
                            });
                            break;
                        }
                        SearchEvent::SearchFailed { provider_id, error, .. } => {
                            tracing::warn!(provider = ?provider_id, error = %error, "Search execution error");
                        }
                        _ => {}
                    }
                }
            });
        });
    }

    // Handle selection movement (Arrow keys)
    {
        let win_weak = window_weak.clone();
        let cur_res_ref = current_results.clone();
        main_window.on_move_selection(move |delta| {
            if let Some(win) = win_weak.upgrade() {
                let total = cur_res_ref.lock().map(|l| l.len()).unwrap_or(0) as i32;
                if total > 0 {
                    let cur = win.get_selected_index();
                    let next = (cur + delta).clamp(0, total - 1);
                    win.set_selected_index(next);
                }
            }
        });
    }

    // Handle item activation (Enter / Click)
    {
        let cur_res_ref = current_results.clone();
        let win_weak = window_weak.clone();
        main_window.on_item_activated(move |index| {
            let res_opt = cur_res_ref.lock().ok().and_then(|l| l.get(index as usize).cloned());

            if let Some(res) = res_opt {
                tracing::info!(id = %res.id, title = %res.title, "Activating selected search result");

                match shell_api::launch_target(&res.launch_target) {
                    Ok(()) => {
                        // Launch succeeded, clear search text and hide window
                        if let Some(win) = win_weak.upgrade() {
                            win.set_search_text("".into());
                            let _ = win.hide();
                        }
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to launch target");
                        if let Some(win) = win_weak.upgrade() {
                            win.set_status_message(format!("Launch error: {}", e).into());
                        }
                    }
                }
            }
        });
    }

    // Handle Escape / dismiss requested
    {
        let win_weak = window_weak.clone();
        let is_visible = is_window_visible.clone();
        main_window.on_dismiss_requested(move || {
            if let Some(win) = win_weak.upgrade() {
                is_visible.store(false, Ordering::SeqCst);
                let _ = win.hide();
            }
        });
    }

    // Show window initially
    main_window.show()?;

    // Configure Win32 HWND: positioning, DWM backdrop, dark mode
    let win_weak_hwnd = window_weak.clone();
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::SingleShot,
        std::time::Duration::from_millis(50),
        move || {
            if let Some(win) = win_weak_hwnd.upgrade() {
                use slint::winit_030::winit::raw_window_handle::{
                    HasWindowHandle, RawWindowHandle,
                };
                use slint::winit_030::WinitWindowAccessor;

                let hwnd_opt = win
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
                    let _ = apply_dark_mode_and_backdrop(hwnd, DwmBackdropType::Mica);
                    let dpi = get_window_dpi(hwnd);
                    if let Ok((x, y, phys_w, phys_h)) =
                        calculate_spotlight_coordinates_with_dpi(640, 480, dpi)
                    {
                        let _ = position_window(hwnd, x, y, phys_w, phys_h);
                    }
                }
            }
        },
    );

    // Register Global Alt + Space Hotkey
    let win_weak_hotkey = window_weak.clone();
    let is_visible_hotkey = is_window_visible.clone();
    let hotkey_handle_res = register_global_hotkey(HotkeyConfig::alt_space(), move || {
        let win_weak = win_weak_hotkey.clone();
        let is_vis = is_visible_hotkey.clone();

        let _ = slint::invoke_from_event_loop(move || {
            if let Some(win) = win_weak.upgrade() {
                let currently_visible = is_vis.load(Ordering::SeqCst);
                if currently_visible {
                    is_vis.store(false, Ordering::SeqCst);
                    let _ = win.hide();
                } else {
                    is_vis.store(true, Ordering::SeqCst);
                    let _ = win.show();
                    // Recalculate coordinates on cursor monitor upon showing
                    use slint::winit_030::winit::raw_window_handle::{
                        HasWindowHandle, RawWindowHandle,
                    };
                    use slint::winit_030::WinitWindowAccessor;

                    let hwnd_opt = win
                        .window()
                        .with_winit_window(|w| {
                            w.window_handle().ok().and_then(|h| match h.as_raw() {
                                RawWindowHandle::Win32(w32) => Some(w32.hwnd.get()),
                                _ => None,
                            })
                        })
                        .flatten();

                    if let Some(hwnd) = hwnd_opt {
                        let dpi = get_window_dpi(hwnd);
                        if let Ok((x, y, phys_w, phys_h)) =
                            calculate_spotlight_coordinates_with_dpi(640, 480, dpi)
                        {
                            let _ = position_window(hwnd, x, y, phys_w, phys_h);
                        }
                    }
                }
            }
        });
    });

    match hotkey_handle_res {
        Ok(_handle) => {
            tracing::info!("Global Alt+Space hotkey registered successfully");
        }
        Err(e) => {
            tracing::warn!(error = %e, "Global hotkey registration warning: application remains usable via window");
        }
    }

    // Connect to supervisor via Named Pipe IPC (background task)
    {
        let rt = tokio_rt.clone();
        rt.spawn(async move {
            let pipe_addr = platform_windows::default_pipe_address();
            match ipc::transport::connect_pipe_client(&pipe_addr).await {
                Ok(mut client) => {
                    let req = ipc::RequestEnvelope {
                        request_id: 1,
                        request: ipc::IpcRequest::Handshake {
                            client_version: ipc::PROTOCOL_VERSION_1,
                            client_name: "search-ui".into(),
                        },
                    };
                    if let Ok(()) = ipc::transport::write_framed_message(&mut client, &req).await {
                        let resp_res: Result<ipc::ResponseEnvelope, _> =
                            ipc::transport::read_framed_message(&mut client).await;
                        match resp_res {
                            Ok(resp) => {
                                tracing::info!(response = ?resp.response, "Supervised IPC Handshake confirmed");
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "IPC Handshake response failed");
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::info!(error = %e, "Supervisor host offline; running search-ui in standalone mode");
                }
            }
        });
    }

    tracing::info!("Starting Slint UI event loop...");
    slint::run_event_loop()?;
    tracing::info!("AsterShell search-ui exited cleanly.");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use search_core::LaunchTarget;

    #[test]
    fn test_result_conversion_to_slint_item() {
        let res = SearchResult::new(
            "app:notepad",
            "Notepad",
            "C:\\Windows\\notepad.exe",
            search_core::SearchResultCategory::Application,
            LaunchTarget::Executable {
                path: "C:\\Windows\\notepad.exe".into(),
                arguments: vec![],
                working_directory: None,
            },
            "app-discovery",
            0.98,
        );

        let item = convert_result_to_item(&res);
        assert_eq!(item.id.as_str(), "app:notepad");
        assert_eq!(item.title.as_str(), "Notepad");
        assert_eq!(item.badge.as_str(), "APP");
        assert_eq!(item.score, 0.98);
    }

    #[test]
    fn test_calculator_conversion_badge() {
        let res = SearchResult::new(
            "calc:42",
            "42",
            "2 * 21",
            search_core::SearchResultCategory::Calculation,
            LaunchTarget::CopyToClipboard { text: "42".into() },
            "calculator",
            1.0,
        );

        let item = convert_result_to_item(&res);
        assert_eq!(item.badge.as_str(), "CALC");
        assert_eq!(item.title.as_str(), "42");
    }

    #[test]
    fn test_orchestrator_initialization() {
        let orch = create_search_orchestrator();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let locked = orch.lock().await;
            let (gen, _rx) = locked.execute_search("calc").await;
            assert_eq!(gen, 1);
        });
    }

    #[test]
    fn test_generation_filtering_discards_stale_events() {
        let active_gen = Arc::new(AtomicU64::new(5));

        // Event from generation 4 (stale)
        let stale_event = SearchEvent::ResultsUpdated {
            generation: 4,
            results: vec![],
            is_final: false,
        };
        assert_ne!(
            stale_event.generation(),
            active_gen.load(Ordering::SeqCst),
            "Stale generation must not match active generation"
        );

        // Event from generation 5 (current)
        let current_event = SearchEvent::ResultsUpdated {
            generation: 5,
            results: vec![],
            is_final: false,
        };
        assert_eq!(
            current_event.generation(),
            active_gen.load(Ordering::SeqCst),
            "Current generation must match active generation"
        );
    }

    #[test]
    fn test_empty_query_increments_generation() {
        let active_gen = Arc::new(AtomicU64::new(10));
        let prev = active_gen.fetch_add(1, Ordering::SeqCst);
        assert_eq!(prev, 10);
        assert_eq!(active_gen.load(Ordering::SeqCst), 11);

        // Older event is now strictly less than active_gen
        let old_event = SearchEvent::SearchCompleted {
            generation: 10,
            total_results: 5,
            duration_ms: 10,
        };
        assert!(old_event.generation() < active_gen.load(Ordering::SeqCst));
    }
}
