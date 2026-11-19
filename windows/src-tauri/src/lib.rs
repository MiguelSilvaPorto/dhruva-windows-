mod catalog;
mod commands;
mod settings;
mod win;

use commands::AppState;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{
    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
};

pub fn run() {
    init_logging();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            let _ = commands::open_settings(app.clone());
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let handle = app.handle().clone();

            win::taskbar::cleanup_stale();

            let mut current_settings = settings::load();
            let apps = catalog::scan_apps();

            if !current_settings.pinned_initialized {
                current_settings.pinned = catalog::default_pinned(&apps);
                current_settings.pinned_initialized = true;
                let _ = settings::save(&current_settings);
            }

            let light = win::theme::is_light();
            let (dock_work, dock_full) = win::cursor_monitor_metrics();
            app.manage(AppState {
                settings: Mutex::new(current_settings.clone()),
                apps: Mutex::new(apps),
                theme_light: Mutex::new(light),
                dock_work,
                dock_full,
            });

            let monitors = win::monitors();
            if monitors.is_empty() {
                create_dock(app, "dock", dock_work)?;
            } else {
                for (index, monitor) in monitors.iter().enumerate() {
                    let label = if index == 0 {
                        "dock".to_string()
                    } else {
                        format!("dock-{index}")
                    };
                    create_dock(app, &label, monitor.work)?;
                }
            }
            create_settings_window(app, dock_work)?;
            create_launcher_window(app)?;
            win::virtual_desktops::follow_desktops(app.handle().clone());
            win::hotkey::install(handle.clone());

            if current_settings.hide_taskbar {
                if let Err(error) = win::taskbar::hide_all() {
                    log::warn!("could not hide the Windows taskbar: {error}");
                } else {
                    schedule_relayout(handle.clone(), 350);
                }
            }

            register_panic_shortcut(&handle);

            start_theme_poller(handle.clone());
            start_window_poller(handle.clone());
            start_foreground_watcher();
            start_taskbar_watcher(handle.clone());
            start_session_watcher(handle.clone());
            start_outside_click_watcher(handle.clone());

            // Never leave the user with an invisible dock if the frontend fails
            // to call dock_ready for any reason.
            let fallback = handle.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(6));
                for (label, window) in fallback.webview_windows() {
                    if !(label == "dock" || label.starts_with("dock-")) {
                        continue;
                    }
                    if !window.is_visible().unwrap_or(true) {
                        if let Some(hwnd) = win::hwnd_of(&window) {
                            commands::apply_no_activate(hwnd);
                        }
                        let _ = window.show();
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::get_icon,
            commands::get_exe_icon,
            commands::get_trash_icon,
            commands::launch_app,
            commands::focus_window,
            commands::minimize_window,
            commands::close_window,
            commands::list_windows,
            commands::window_preview,
            commands::pick_folder,
            commands::open_folder,
            commands::open_path,
            commands::system_status,
            commands::background_apps,
            commands::background_activate,
            commands::background_terminate,
            commands::media_status,
            commands::media_art,
            commands::media_control,
            commands::wifi_status,
            commands::wifi_set_enabled,
            commands::wifi_connect,
            commands::wifi_disconnect,
            commands::wifi_forget,
            commands::focus_dock,
            commands::release_dock_focus,
            commands::open_start_menu,
            commands::open_launcher,
            commands::close_launcher,
            commands::open_task_view,
            commands::new_virtual_desktop,
            commands::close_virtual_desktop,
            commands::switch_virtual_desktop,
            commands::virtual_desktops,
            commands::switch_virtual_desktop_to,
            commands::open_action_center,
            commands::open_settings_page,
            commands::open_trash,
            commands::empty_trash,
            commands::save_settings,
            commands::reset_settings,
            commands::set_taskbar_hidden,
            commands::open_settings,
            commands::close_settings,
            commands::resize_dock,
            commands::set_dock_reservation,
            commands::set_dock_region,
            commands::dock_ready,
            commands::quit_app,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Dhruva")
        .run(|app, event| {
            if matches!(
                event,
                tauri::RunEvent::Exit | tauri::RunEvent::ExitRequested { .. }
            ) {
                win::appbar::release_all();
                win::taskbar::show();
                let _ = app;
            }
        });
}

fn create_dock(
    app: &mut tauri::App,
    label: &str,
    work: (i32, i32, i32, i32),
) -> tauri::Result<tauri::WebviewWindow> {
    let window = WebviewWindowBuilder::new(
        app,
        label,
        WebviewUrl::App("index.html".into()),
    )
    .title("Dhruva")
    .inner_size(800.0, 640.0)
    .transparent(true)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .focused(false)
    .shadow(false)
    .visible(false)
    .build()?;

    if let Some(hwnd) = win::hwnd_of(&window) {
        commands::apply_no_activate(hwnd);

        let size = window.inner_size().unwrap_or_default();
        win::region::set_strip(hwnd, size.height as i32, 2);

        let scale = window.scale_factor().unwrap_or(1.0);
        let (work_x, work_y, work_w, work_h) = work;
        let width = (800.0 * scale) as i32;
        let height = (640.0 * scale) as i32;
        let x = work_x + ((work_w - width) / 2).max(0);
        let y = work_y + (work_h - height).max(0);
        let _ = window.set_size(tauri::PhysicalSize::new(width as u32, height as u32));
        let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
    }

    Ok(window)
}

fn create_launcher_window(app: &mut tauri::App) -> tauri::Result<tauri::WebviewWindow> {
    let window = WebviewWindowBuilder::new(
        app,
        "launcher",
        WebviewUrl::App("launcher.html".into()),
    )
    .title("Dhruva — Aplicativos")
    .inner_size(800.0, 600.0)
    .transparent(true)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .focused(false)
    .shadow(false)
    .visible(false)
    .build()?;

    if let Some(hwnd) = win::hwnd_of(&window) {
        commands::apply_tool_window(hwnd);
    }

    let handle = window.clone();
    window.on_window_event(move |event| {
        match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = handle.hide();
            }
            tauri::WindowEvent::Focused(false) => {
                let _ = handle.hide();
            }
            _ => {}
        }
    });

    Ok(window)
}

fn create_settings_window(
    app: &mut tauri::App,
    work: (i32, i32, i32, i32),
) -> tauri::Result<tauri::WebviewWindow> {
    let (work_x, work_y, work_w, work_h) = work;
    let width = 780.0;
    let height = 650.0;
    let x = work_x as f64 + ((work_w as f64 - width) / 2.0).max(0.0);
    let y = work_y as f64 + ((work_h as f64 - height) / 2.0).max(0.0);

    let window = WebviewWindowBuilder::new(
        app,
        "settings",
        WebviewUrl::App("settings.html".into()),
    )
    .title("Dhruva — Configurações")
    .inner_size(width, height)
    .min_inner_size(620.0, 480.0)
    .resizable(true)
    .position(x, y)
    .visible(false)
    .build()?;

    let settings = window.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = settings.hide();
        }
    });

    Ok(window)
}

fn register_panic_shortcut(app: &tauri::AppHandle) {
    let shortcut = Shortcut::new(
        Some(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT),
        Code::KeyD,
    );

    let handle = app.clone();
    if let Err(error) = app.global_shortcut().on_shortcut(shortcut, move |_app, _shortcut, event| {
        if event.state() == ShortcutState::Pressed {
            log::info!("restore shortcut pressed");
            let _ = commands::disable_taskbar_replacement(&handle);
        }
    }) {
        log::warn!("could not register Ctrl+Alt+Shift+D: {error}");
    }
}

fn start_theme_poller(app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        let light = win::theme::is_light();
        let changed = {
            let state = app.state::<AppState>();
            let mut last = state.theme_light.lock().unwrap();
            if *last != light {
                *last = light;
                true
            } else {
                false
            }
        };

        if changed {
            let _ = app.emit("theme-changed", if light { "light" } else { "dark" });
        }

        std::thread::sleep(Duration::from_secs(2));
    });
}

fn start_window_poller(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let mut last_json = String::new();
        let mut last_trash: Option<bool> = None;
        let mut ticks: u32 = 0;

        loop {
            let windows = win::windows::list();
            let json = serde_json::to_string(&windows).unwrap_or_default();
            if json != last_json {
                last_json = json;
                let _ = app.emit("windows-changed", &windows);
            }

            if ticks % 8 == 0 {
                let full = win::shell::recycle_bin_full();
                if last_trash != Some(full) {
                    last_trash = Some(full);
                    let _ = app.emit("trash-changed", full);
                }
            }

            ticks = ticks.wrapping_add(1);
            std::thread::sleep(Duration::from_millis(700));
        }
    });
}

fn start_foreground_watcher() {
    std::thread::spawn(|| loop {
        commands::remember_foreground();
        std::thread::sleep(Duration::from_millis(400));
    });
}

fn start_taskbar_watcher(app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(2));
        let hide = {
            let state = app.state::<AppState>();
            let settings = state.settings.lock().unwrap();
            settings.hide_taskbar
        };
        if hide {
            win::taskbar::reconcile_all();
        }
    });
}

fn start_session_watcher(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let mut was_locked = win::windows::session_is_locked();

        loop {
            std::thread::sleep(Duration::from_millis(1200));
            let locked = win::windows::session_is_locked();
            if was_locked && !locked {
                log::info!("session unlocked; revealing the dock");
                let _ = app.emit("session-unlocked", ());
            }
            was_locked = locked;
        }
    });
}

/// Closes the Wi-Fi panel and context menus when the user clicks anywhere
/// outside the dock. The dock window never activates, so it cannot rely on
/// a focus-lost event for this.
fn start_outside_click_watcher(app: tauri::AppHandle) {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetAncestor, GetCursorPos, WindowFromPoint, GA_ROOT,
    };

    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(60));

        // The low bit reports a press made since the previous poll, so even a
        // very fast click between two polls is not lost.
        let left = unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) };
        let right = unsafe { GetAsyncKeyState(VK_RBUTTON.0 as i32) };
        let pressed = (left & 1) != 0 || (right & 1) != 0;

        if pressed {
            let mut point = POINT::default();
            if unsafe { GetCursorPos(&mut point) }.is_ok() {
                log::debug!("click watcher: press at {},{}", point.x, point.y);
                let mut hit_label = String::new();

                for (label, window) in app.webview_windows() {
                    if !(label == "dock" || label.starts_with("dock-")) {
                        continue;
                    }

                    let Some(hwnd) = win::hwnd_of(&window) else {
                        continue;
                    };

                    unsafe {
                        let hit = WindowFromPoint(point);
                        if !hit.is_invalid() && GetAncestor(hit, GA_ROOT) == hwnd {
                            hit_label = label;
                            break;
                        }
                    }
                }

                log::debug!("click watcher: dismiss (source {hit_label:?})");
                let _ = app.emit("dismiss-overlays", hit_label);
            }
        }
    });
}

fn schedule_relayout(app: tauri::AppHandle, delay_ms: u64) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(delay_ms));
        let _ = app.emit("relayout", ());
    });
}

fn init_logging() {
    let dir = settings::local_dir();
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("dhruva.log");
    if let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = simplelog::WriteLogger::init(
            log::LevelFilter::Info,
            simplelog::Config::default(),
            file,
        );
    }
}
