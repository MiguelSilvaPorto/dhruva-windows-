use crate::catalog::{self, AppEntry};
use crate::settings::{self, Settings};
use crate::win;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, IsIconic, IsWindowVisible, SetForegroundWindow,
};

#[repr(C)]
#[derive(Default)]
struct SystemPowerStatus {
    ac_line_status: u8,
    battery_flag: u8,
    battery_life_percent: u8,
    system_status_flag: u8,
    battery_life_time: u32,
    battery_full_life_time: u32,
}

#[link(name = "kernel32")]
extern "system" {
    fn GetSystemPowerStatus(status: *mut SystemPowerStatus) -> i32;
}

#[link(name = "user32")]
extern "system" {
    fn keybd_event(vk: u8, scan: u8, flags: u32, extra: usize);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStatus {
    pub battery_percent: Option<u32>,
    pub charging: bool,
}

pub struct AppState {
    pub settings: Mutex<Settings>,
    pub apps: Mutex<Vec<AppEntry>>,
    pub theme_light: Mutex<bool>,
    pub dock_work: (i32, i32, i32, i32),
    pub dock_full: (i32, i32, i32, i32),
}

/// Last foreground window owned by a real application. Windows can leave a
/// hidden Start/Search window as the focus fallback when the launcher hides,
/// so that memory is used to hand focus back to what the user was using.
static LAST_REAL_FOREGROUND: AtomicIsize = AtomicIsize::new(0);

pub fn remember_foreground() {
    unsafe {
        let foreground = GetForegroundWindow();
        if foreground.0.is_null() {
            return;
        }

        let mut pid = 0u32;
        GetWindowThreadProcessId(foreground, Some(&mut pid));
        if pid == 0 || pid == std::process::id() {
            return;
        }
        if !IsWindowVisible(foreground).as_bool() || IsIconic(foreground).as_bool() {
            return;
        }
        if win::hotkey::is_start_surface_pid(pid) {
            return;
        }

        LAST_REAL_FOREGROUND.store(foreground.0 as isize, Ordering::SeqCst);
    }
}

fn last_real_foreground() -> Option<HWND> {
    let value = LAST_REAL_FOREGROUND.load(Ordering::SeqCst);
    if value == 0 {
        return None;
    }
    let hwnd = HWND(value as *mut core::ffi::c_void);
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return None;
        }
    }
    Some(hwnd)
}

fn restore_previous_focus(launcher: Option<isize>) {
    std::thread::spawn(move || unsafe {
        let launcher = launcher.map(|value| HWND(value as *mut core::ffi::c_void));
        let target = last_real_foreground();

        for attempt in 0..20 {
            let foreground = GetForegroundWindow();
            let mut pid = 0u32;
            GetWindowThreadProcessId(foreground, Some(&mut pid));

            let is_launcher = launcher.map(|value| value == foreground).unwrap_or(false);
            if !foreground.0.is_null()
                && !is_launcher
                && !win::hotkey::is_start_surface_pid(pid)
            {
                // A real window took focus (for example the app the user just
                // launched): never steal it.
                return;
            }

            if let Some(target) = target {
                if win::windows::focus(target) {
                    return;
                }
            }

            if attempt >= 1 && win::hotkey::is_start_surface_pid(pid) {
                // The shell surface grabbed focus before the hand-off. Closing
                // it makes Windows return focus to the window it covered.
                win::hotkey::dismiss_start_surface();
                if target.is_none() {
                    return;
                }
            }

            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    });
}

fn hide_launcher(window: &tauri::WebviewWindow, restore_focus: bool) {
    let launcher = win::hwnd_of(window).map(|hwnd| hwnd.0 as isize);

    if restore_focus {
        // Hand focus over while this process still owns the foreground, then
        // hide. Hiding first lets Windows activate a hidden Start/Search
        // window as the fallback, which is what the user then sees.
        if let Some(target) = last_real_foreground() {
            unsafe {
                let _ = SetForegroundWindow(target);
            }
        }
    }

    let _ = window.hide();

    if restore_focus {
        restore_previous_focus(launcher);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub settings: Settings,
    pub apps: Vec<AppEntry>,
    pub theme: String,
    pub taskbar_hidden: bool,
    pub version: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashIcon {
    pub icon: Option<String>,
    pub full: bool,
}

#[derive(Deserialize)]
pub struct RegionRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub radius: f64,
}

#[tauri::command]
pub fn bootstrap(state: State<'_, AppState>) -> Bootstrap {
    let settings = state.settings.lock().unwrap().clone();
    let apps = state.apps.lock().unwrap().clone();
    let light = *state.theme_light.lock().unwrap();

    Bootstrap {
        settings,
        apps,
        theme: if light { "light" } else { "dark" }.to_string(),
        taskbar_hidden: win::taskbar::is_hidden_by_us(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

#[tauri::command]
pub fn get_icon(state: State<'_, AppState>, app_id: String) -> Option<String> {
    let app = state
        .apps
        .lock()
        .ok()?
        .iter()
        .find(|app| app.id == app_id)
        .cloned()?;
    catalog::app_icons::for_app(&app)
}

#[tauri::command]
pub fn get_exe_icon(path: String) -> Option<String> {
    catalog::app_icons::for_exe(&path)
}

#[tauri::command]
pub fn get_trash_icon() -> TrashIcon {
    TrashIcon {
        icon: catalog::app_icons::trash(),
        full: win::shell::recycle_bin_full(),
    }
}

#[tauri::command]
pub fn launch_app(state: State<'_, AppState>, app_id: String) -> Result<(), String> {
    let app = find_app(&state, &app_id)?;
    let target = if !app.path.is_empty() {
        app.path.clone()
    } else {
        app.target.clone()
    };
    win::shell::execute(&target)
}

#[tauri::command]
pub fn focus_window(hwnd: i64) -> bool {
    win::windows::focus(win::hwnd_from_i64(hwnd))
}

#[tauri::command]
pub fn minimize_window(hwnd: i64) {
    win::windows::minimize(win::hwnd_from_i64(hwnd));
}

#[tauri::command]
pub fn close_window(hwnd: i64) {
    win::windows::close(win::hwnd_from_i64(hwnd));
}

#[tauri::command]
pub fn list_windows() -> Vec<win::windows::WindowInfo> {
    win::windows::list()
}

#[tauri::command]
pub async fn window_preview(hwnd: i64, width: u32, height: u32) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        win::preview::capture(hwnd, width, height)
    })
    .await
    .ok()
    .flatten()
}

#[tauri::command]
pub async fn window_preview_jpeg(hwnd: i64, width: u32, height: u32) -> Option<String> {
    // The lock screen suspends window painting; frames would be blank.
    if win::windows::session_is_locked() {
        return None;
    }

    tauri::async_runtime::spawn_blocking(move || {
        win::preview::capture_jpeg(hwnd, width, height)
    })
    .await
    .ok()
    .flatten()
}

#[tauri::command]
pub async fn pick_folder() -> Option<String> {
    tauri::async_runtime::spawn_blocking(win::shell::pick_folder)
        .await
        .ok()
        .flatten()
}

#[tauri::command]
pub fn open_folder(path: String, new_window: bool) -> Result<(), String> {
    win::shell::open_folder(&path, new_window)
}

#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    win::shell::execute(&path)
}

#[tauri::command]
pub fn background_apps() -> Vec<win::processes::BackgroundApp> {
    win::processes::list()
}

#[tauri::command]
pub fn background_activate(hwnd: Option<i64>, exe: String) -> Result<(), String> {
    if let Some(hwnd) = hwnd {
        win::processes::activate(hwnd);
        return Ok(());
    }

    if exe.trim().is_empty() {
        return Err("Aplicativo sem janela e sem executável.".to_string());
    }

    win::shell::execute(&exe)
}

#[tauri::command]
pub fn background_terminate(pid: u32) -> Result<(), String> {
    win::processes::terminate(pid)
}

#[tauri::command]
pub fn media_status() -> win::media::MediaStatus {
    win::media::status()
}

#[tauri::command]
pub fn media_art() -> Option<String> {
    win::media::art()
}

#[tauri::command]
pub async fn media_preview_start(
    title: Option<String>,
    app: Option<String>,
) -> Option<win::media::PreviewSession> {
    tauri::async_runtime::spawn_blocking(move || win::media::preview_start(title, app))
        .await
        .ok()
        .flatten()
}

#[tauri::command]
pub async fn media_preview_stop(selection: win::accessibility::TabSelection) -> bool {
    tauri::async_runtime::spawn_blocking(move || win::accessibility::restore_tab(&selection))
        .await
        .ok()
        .unwrap_or(false)
}

#[tauri::command]
pub fn set_media_thumbnail(
    window: tauri::WebviewWindow,
    source: i64,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    visible: bool,
) -> bool {
    if !visible || source == 0 {
        win::thumbnail::hide();
        return true;
    }

    let scale = window.scale_factor().unwrap_or(1.0);
    let position = window
        .outer_position()
        .unwrap_or(tauri::PhysicalPosition::new(0, 0));
    let px = position.x + ((x + 1.0) * scale).round() as i32;
    let py = position.y + ((y + 1.0) * scale).round() as i32;
    let pw = ((width - 2.0) * scale).round().max(8.0) as i32;
    let ph = ((height - 2.0) * scale).round().max(8.0) as i32;

    win::thumbnail::show(source, px, py, pw, ph)
}

#[tauri::command]
pub async fn media_audio() -> Option<win::audio::AudioState> {
    tauri::async_runtime::spawn_blocking(win::audio::state)
        .await
        .ok()
        .flatten()
}

#[tauri::command]
pub async fn media_timeline(allow_switch: Option<bool>) -> Option<win::accessibility::TimelineSample> {
    tauri::async_runtime::spawn_blocking(move || {
        win::media::timeline(allow_switch.unwrap_or(false))
    })
    .await
    .ok()
    .flatten()
}

#[tauri::command]
pub async fn set_media_volume(volume: f32) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || win::audio::set_volume(volume))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn set_media_muted(muted: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || win::audio::set_muted(muted))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub fn media_control(action: String, position_ms: Option<u64>) -> Result<(), String> {
    win::media::control(&action, position_ms)
}

#[tauri::command]
pub fn wifi_status(include_networks: Option<bool>) -> win::wifi::WifiStatus {
    win::wifi::status(include_networks.unwrap_or(true))
}

#[tauri::command]
pub fn wifi_set_enabled(enabled: bool) -> Result<(), String> {
    win::wifi::set_enabled(enabled)
}

#[tauri::command]
pub fn wifi_connect(ssid: String, password: Option<String>) -> Result<(), String> {
    win::wifi::connect(&ssid, password.as_deref())
}

#[tauri::command]
pub fn wifi_disconnect() -> Result<(), String> {
    win::wifi::disconnect()
}

#[tauri::command]
pub fn wifi_forget(profile: String) -> Result<(), String> {
    win::wifi::forget(&profile)
}

#[tauri::command]
pub fn system_status() -> SystemStatus {
    let mut status = SystemPowerStatus::default();
    let ok = unsafe { GetSystemPowerStatus(&mut status) } != 0;

    if !ok {
        return SystemStatus {
            battery_percent: None,
            charging: false,
        };
    }

    let battery_percent = if status.battery_life_percent == 255 {
        None
    } else {
        Some(status.battery_life_percent as u32)
    };
    let charging = status.ac_line_status == 1 || (status.battery_flag & 0x08) != 0;

    SystemStatus {
        battery_percent,
        charging,
    }
}

#[tauri::command]
pub fn open_start_menu() {
    const VK_LWIN: u8 = 0x5b;
    const KEYEVENTF_KEYUP: u32 = 0x0002;

    unsafe {
        keybd_event(VK_LWIN, 0, 0, 0);
        std::thread::sleep(std::time::Duration::from_millis(30));
        keybd_event(VK_LWIN, 0, KEYEVENTF_KEYUP, 0);
    }
}

/// Opens the Windows Action Center (notifications + calendar) with Win+A.
fn show_launcher_on(
    app: &AppHandle,
    monitor: (i32, i32, i32, i32),
) -> Result<(), String> {
    if win::windows::session_is_locked() {
        return Err("A sessao do Windows esta bloqueada".to_string());
    }

    let launcher = app
        .get_webview_window("launcher")
        .ok_or_else(|| "janela do launcher indisponivel".to_string())?;

    remember_foreground();

    let (x, y, width, height) = monitor;
    launcher
        .set_size(tauri::PhysicalSize::new(
            width.max(320) as u32,
            height.max(240) as u32,
        ))
        .map_err(|error| error.to_string())?;
    launcher
        .set_position(tauri::PhysicalPosition::new(x, y))
        .map_err(|error| error.to_string())?;
    launcher.show().map_err(|error| error.to_string())?;

    if let Some(hwnd) = win::hwnd_of(&launcher) {
        win::windows::focus(hwnd);
    }

    let _ = launcher.set_focus();
    let _ = launcher.emit("launcher-open", ());
    Ok(())
}

#[tauri::command]
pub fn open_launcher(
    app: AppHandle,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    let (_, full) = win::window_monitor_metrics(&window);
    show_launcher_on(&app, full)
}

#[tauri::command]
pub fn close_launcher(window: tauri::WebviewWindow, restore_focus: Option<bool>) {
    hide_launcher(&window, restore_focus.unwrap_or(true));
}

pub fn toggle_launcher_cursor(app: &AppHandle) {
    if win::windows::session_is_locked() {
        return;
    }

    let Some(launcher) = app.get_webview_window("launcher") else {
        return;
    };

    if launcher.is_visible().unwrap_or(false) {
        hide_launcher(&launcher, true);
    } else {
        let (_, full) = win::cursor_monitor_metrics();
        let _ = show_launcher_on(app, full);
    }
}

/// Opens the launcher without toggling it closed. Used by the Win+Q and
/// Win+S shortcuts, which Windows would otherwise use for its own search.
pub fn open_launcher_cursor(app: &AppHandle) {
    if win::windows::session_is_locked() {
        return;
    }

    let Some(launcher) = app.get_webview_window("launcher") else {
        return;
    };

    if launcher.is_visible().unwrap_or(false) {
        if let Some(hwnd) = win::hwnd_of(&launcher) {
            win::windows::focus(hwnd);
        }
        let _ = launcher.set_focus();
        let _ = launcher.emit("launcher-open", ());
    } else {
        let (_, full) = win::cursor_monitor_metrics();
        let _ = show_launcher_on(app, full);
    }
}

const VK_CONTROL: u8 = 0x11;
const VK_LWIN: u8 = 0x5b;
const VK_TAB: u8 = 0x09;
const VK_D: u8 = 0x44;
const VK_F4: u8 = 0x73;
const VK_LEFT: u8 = 0x25;
const VK_RIGHT: u8 = 0x27;

fn send_combo(keys: &[u8]) {
    const KEYEVENTF_KEYUP: u32 = 0x0002;
    let marker = win::hotkey::DHURVA_INJECTED;

    unsafe {
        for key in keys {
            keybd_event(*key, 0, 0, marker);
            std::thread::sleep(std::time::Duration::from_millis(22));
        }
        for key in keys.iter().rev() {
            keybd_event(*key, 0, KEYEVENTF_KEYUP, marker);
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
    }
}

#[tauri::command]
pub fn open_task_view() {
    send_combo(&[VK_LWIN, VK_TAB]);
}

#[tauri::command]
pub fn new_virtual_desktop() {
    send_combo(&[VK_CONTROL, VK_LWIN, VK_D]);
}

#[tauri::command]
pub fn close_virtual_desktop() {
    send_combo(&[VK_CONTROL, VK_LWIN, VK_F4]);
}

#[tauri::command]
pub fn switch_virtual_desktop(direction: String) {
    let key = if direction == "left" {
        VK_LEFT
    } else {
        VK_RIGHT
    };
    send_combo(&[VK_CONTROL, VK_LWIN, key]);
}

#[tauri::command]
pub fn virtual_desktops() -> Vec<win::virtual_desktops::VirtualDesktopInfo> {
    win::virtual_desktops::list()
}

#[tauri::command]
pub async fn switch_virtual_desktop_to(index: usize) {
    tauri::async_runtime::spawn_blocking(move || {
        let desktops = win::virtual_desktops::list();
        let Some(current) = desktops.iter().position(|desktop| desktop.current) else {
            return;
        };
        if index >= desktops.len() || index == current {
            return;
        }

        let key = if index > current { VK_RIGHT } else { VK_LEFT };
        let steps = current.abs_diff(index);
        for step in 0..steps {
            send_combo(&[VK_CONTROL, VK_LWIN, key]);
            if step + 1 < steps {
                std::thread::sleep(std::time::Duration::from_millis(180));
            }
        }
    })
    .await
    .ok();
}

#[tauri::command]
pub fn open_action_center() {
    const VK_LWIN: u8 = 0x5b;
    const VK_A: u8 = 0x41;
    const KEYEVENTF_KEYUP: u32 = 0x0002;

    unsafe {
        keybd_event(VK_LWIN, 0, 0, 0);
        std::thread::sleep(std::time::Duration::from_millis(20));
        keybd_event(VK_A, 0, 0, 0);
        std::thread::sleep(std::time::Duration::from_millis(35));
        keybd_event(VK_A, 0, KEYEVENTF_KEYUP, 0);
        keybd_event(VK_LWIN, 0, KEYEVENTF_KEYUP, 0);
    }
}

#[tauri::command]
pub fn open_settings_page(page: String) -> Result<(), String> {
    let uri = match page.as_str() {
        "network" => "ms-settings:network-status",
        "sound" => "ms-settings:sound",
        "battery" => "ms-settings:batterysaver",
        _ => return Err(format!("pagina de configuracoes desconhecida: {page}")),
    };

    win::shell::execute(uri)
}

#[tauri::command]
pub fn open_trash() -> Result<(), String> {
    win::shell::open_recycle_bin()
}

#[tauri::command]
pub fn empty_trash() -> Result<(), String> {
    win::shell::empty_recycle_bin()
}

#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    mut settings: Settings,
) -> Result<Settings, String> {
    settings.normalize();
    let previous = state.settings.lock().unwrap().clone();

    if settings.hide_taskbar != previous.hide_taskbar {
        if settings.hide_taskbar {
            win::taskbar::hide_all()?;
        } else {
            win::taskbar::show();
        }
        win::appbar::refresh();
    }

    settings::save(&settings)?;
    *state.settings.lock().unwrap() = settings.clone();
    let _ = app.emit("settings-changed", &settings);
    schedule_relayout(&app);
    Ok(settings)
}

#[tauri::command]
pub fn reset_settings(app: AppHandle, state: State<'_, AppState>) -> Result<Settings, String> {
    let current = state.settings.lock().unwrap().clone();
    let mut settings = Settings::default();
    settings.pinned = current.pinned;
    settings.pinned_initialized = current.pinned_initialized;
    settings.hide_taskbar = current.hide_taskbar;
    if !current.hide_taskbar {
        win::taskbar::show();
        win::appbar::refresh();
    }

    settings::save(&settings)?;
    *state.settings.lock().unwrap() = settings.clone();
    let _ = app.emit("settings-changed", &settings);
    schedule_relayout(&app);
    Ok(settings)
}

#[tauri::command]
pub fn set_taskbar_hidden(
    app: AppHandle,
    state: State<'_, AppState>,
    hidden: bool,
) -> Result<Settings, String> {
    let mut settings = state.settings.lock().unwrap().clone();
    settings.hide_taskbar = hidden;
    if hidden {
        win::taskbar::hide_all()?;
    } else {
        win::taskbar::show();
    }
    win::appbar::refresh();
    settings::save(&settings)?;
    *state.settings.lock().unwrap() = settings.clone();
    let _ = app.emit("settings-changed", &settings);
    schedule_relayout(&app);
    Ok(settings)
}

pub fn disable_taskbar_replacement(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut settings = state.settings.lock().unwrap().clone();
    settings.hide_taskbar = false;
    settings::save(&settings)?;
    win::taskbar::show();
    win::appbar::refresh();
    *state.settings.lock().unwrap() = settings.clone();
    let _ = app.emit("settings-changed", &settings);
    schedule_relayout(app);
    Ok(())
}

fn schedule_relayout(app: &AppHandle) {
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(350));
        let _ = handle.emit("relayout", ());
    });
}

#[tauri::command]
pub fn open_settings(app: AppHandle) -> Result<(), String> {
    if win::windows::session_is_locked() {
        return Err("a sessao do Windows esta bloqueada".to_string());
    }

    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }

    Err("janela de configuracoes indisponivel".to_string())
}

#[tauri::command]
pub fn close_settings(app: AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.hide();
    }
}

#[tauri::command]
pub fn resize_dock(
    window: tauri::WebviewWindow,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    let physical_h = (height * scale).round().max(80.0) as i32;
    let (_, full) = win::window_monitor_metrics(&window);

    // Width 0 means a full-monitor host window; the bar inside it stays
    // compact and centered.
    let (x, physical_w) = if width > 0.0 {
        let wanted = (width * scale).round().max(320.0) as i32;
        (full.0 + ((full.2 - wanted) / 2).max(0), wanted)
    } else {
        (full.0, full.2.max(320))
    };

    let bottom = win::appbar::base_bottom(full);
    let y = (bottom - physical_h).max(full.1);

    window
        .set_size(tauri::PhysicalSize::new(
            physical_w as u32,
            physical_h as u32,
        ))
        .map_err(|e| e.to_string())?;
    window
        .set_position(tauri::PhysicalPosition::new(x, y))
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn set_dock_reservation(
    window: tauri::WebviewWindow,
    height: f64,
    enabled: bool,
) {
    let Some(hwnd) = win::hwnd_of(&window) else {
        return;
    };

    if !enabled {
        win::appbar::release(hwnd);
        return;
    }

    let scale = window.scale_factor().unwrap_or(1.0);
    let physical_h = (height * scale).round() as i32;
    let (_, full) = win::window_monitor_metrics(&window);
    win::appbar::reserve(hwnd, full, physical_h);
}

#[tauri::command]
pub fn set_dock_region(
    window: tauri::WebviewWindow,
    rects: Vec<RegionRect>,
    full: Option<bool>,
) {
    let Some(hwnd) = win::hwnd_of(&window) else {
        return;
    };
    if full.unwrap_or(false) {
        win::region::clear(hwnd);
        return;
    }

    let scale = window.scale_factor().unwrap_or(1.0);
    let scaled: Vec<(i32, i32, i32, i32, i32)> = rects
        .iter()
        .map(|rect| {
            (
                (rect.x * scale).round() as i32,
                (rect.y * scale).round() as i32,
                (rect.w * scale).round() as i32,
                (rect.h * scale).round() as i32,
                (rect.radius * scale).round() as i32,
            )
        })
        .collect();

    win::region::set_rounded_multi(hwnd, &scaled);
}

#[tauri::command]
pub fn dock_ready(window: tauri::WebviewWindow) {
    let _ = window.show();
    let _ = window.set_skip_taskbar(true);
    let _ = window.set_always_on_top(true);

    if let Some(hwnd) = win::hwnd_of(&window) {
        apply_no_activate(hwnd);

        // Tauri applies its own Windows styles while showing the window. Reapply
        // the dock styles once that has settled, and refresh the appbar after
        // the window is really visible.
        let raw = hwnd.0 as isize as i64;
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(900));
            apply_no_activate(win::hwnd_from_i64(raw));
            win::appbar::refresh();
        });
    }
}

#[tauri::command]
pub fn focus_dock(window: tauri::WebviewWindow) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetForegroundWindow, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
    };

    if let Some(hwnd) = win::hwnd_of(&window) {
        unsafe {
            let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(
                hwnd,
                GWL_EXSTYLE,
                ex_style & !(WS_EX_NOACTIVATE.0 as isize),
            );
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

#[tauri::command]
pub fn release_dock_focus(window: tauri::WebviewWindow) {
    if let Some(hwnd) = win::hwnd_of(&window) {
        apply_no_activate(hwnd);
    }
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    win::appbar::release_all();
    win::taskbar::show();
    app.exit(0);
}

fn find_app(state: &State<'_, AppState>, app_id: &str) -> Result<AppEntry, String> {
    state
        .apps
        .lock()
        .map_err(|e| e.to_string())?
        .iter()
        .find(|app| app.id == app_id)
        .cloned()
        .ok_or_else(|| format!("app nao encontrado: {app_id}"))
}

/// Tool window style without NOACTIVATE, for the launcher overlay which has
/// to receive keyboard input.
pub fn apply_tool_window(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, SWP_FRAMECHANGED,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_EX_APPWINDOW,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };

    unsafe {
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let updated = (ex_style
            & !(WS_EX_APPWINDOW.0 as isize)
            & !(WS_EX_NOACTIVATE.0 as isize))
            | WS_EX_TOOLWINDOW.0 as isize;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, updated);
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}

pub fn apply_no_activate(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, SWP_FRAMECHANGED,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_EX_APPWINDOW, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW,
    };
    unsafe {
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let updated = (ex_style & !(WS_EX_APPWINDOW.0 as isize))
            | WS_EX_NOACTIVATE.0 as isize
            | WS_EX_TOOLWINDOW.0 as isize;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, updated);
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}

