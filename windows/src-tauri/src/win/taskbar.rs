use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use windows::core::{w, BOOL, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::UI::Shell::{SHAppBarMessage, APPBARDATA};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowW, GetClassNameW, GetWindowRect, IsWindowVisible, ShowWindow, SW_HIDE,
    SW_SHOW, SW_SHOWNA,
};

use crate::settings;

const ABM_GETSTATE: u32 = 0x0000_0004;
const ABM_SETSTATE: u32 = 0x0000_000A;
const ABS_AUTOHIDE: u32 = 0x0000_0001;
const ABS_ALWAYSONTOP: u32 = 0x0000_0002;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct SavedTaskbar {
    state: u32,
    #[serde(default)]
    monitor: Option<(i32, i32, i32, i32)>,
}

fn state_path() -> PathBuf {
    settings::local_dir().join("taskbar_state.json")
}

pub fn taskbars() -> Vec<HWND> {
    let mut handles: Vec<HWND> = Vec::new();

    unsafe {
        if let Ok(primary) = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()) {
            if !primary.is_invalid() {
                handles.push(primary);
            }
        }

        let _ = EnumWindows(
            Some(enum_taskbars),
            LPARAM(&mut handles as *mut Vec<HWND> as isize),
        );
    }

    handles
}

unsafe extern "system" fn enum_taskbars(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let handles = &mut *(lparam.0 as *mut Vec<HWND>);
    let mut class_buf = [0u16; 256];
    let len = GetClassNameW(hwnd, &mut class_buf).max(0) as usize;
    let class = String::from_utf16_lossy(&class_buf[..len.min(class_buf.len())]);

    if class.eq_ignore_ascii_case("Shell_SecondaryTrayWnd")
        && !handles.iter().any(|handle| *handle == hwnd)
    {
        handles.push(hwnd);
    }

    BOOL(1)
}

fn window_on_monitor(hwnd: HWND, monitor: (i32, i32, i32, i32)) -> bool {
    unsafe {
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return false;
        }

        let center_x = (rect.left + rect.right) / 2;
        let center_y = (rect.top + rect.bottom) / 2;
        let (x, y, width, height) = monitor;

        center_x >= x && center_x < x + width && center_y >= y && center_y < y + height
    }
}

fn bars_on_monitor(monitor: (i32, i32, i32, i32)) -> Vec<HWND> {
    let selected: Vec<HWND> = taskbars()
        .into_iter()
        .filter(|bar| window_on_monitor(*bar, monitor))
        .collect();

    if selected.is_empty() {
        taskbars()
    } else {
        selected
    }
}

pub fn is_hidden_by_us() -> bool {
    state_path().exists()
}

fn get_state(hwnd: HWND) -> u32 {
    unsafe {
        let mut data = APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            hWnd: hwnd,
            ..Default::default()
        };
        SHAppBarMessage(ABM_GETSTATE, &mut data) as u32
    }
}

fn set_state(hwnd: HWND, state: u32) {
    unsafe {
        let mut data = APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            hWnd: hwnd,
            lParam: LPARAM(state as isize),
            ..Default::default()
        };
        SHAppBarMessage(ABM_SETSTATE, &mut data);
    }
}

pub fn hide_all() -> Result<(), String> {
    let bars = taskbars();
    if bars.is_empty() {
        return Err("barra de tarefas do Windows nao encontrada".to_string());
    }

    if !state_path().exists() {
        let saved = SavedTaskbar {
            state: get_state(bars[0]),
            monitor: None,
        };
        if let Some(parent) = state_path().parent() {
            let _ = fs::create_dir_all(parent);
        }
        let body = serde_json::to_vec(&saved).map_err(|e| e.to_string())?;
        fs::write(state_path(), body).map_err(|e| e.to_string())?;
    }

    for bar in bars {
        set_state(bar, ABS_AUTOHIDE | ABS_ALWAYSONTOP);
        unsafe {
            let _ = ShowWindow(bar, SW_HIDE);
        }
    }

    Ok(())
}

/// Top edge of the visible taskbar on this monitor, when it sits at the bottom.
pub fn visible_top_for_monitor(monitor: (i32, i32, i32, i32)) -> Option<i32> {
    for bar in bars_on_monitor(monitor) {
        unsafe {
            if !IsWindowVisible(bar).as_bool() {
                continue;
            }

            let mut rect = RECT::default();
            if GetWindowRect(bar, &mut rect).is_err() {
                continue;
            }

            let (x, y, width, height) = monitor;
            let within_width = rect.left >= x && rect.right <= x + width;
            let at_bottom = rect.top >= y + height / 2 && rect.bottom <= y + height + 2;
            if within_width && at_bottom && rect.top < rect.bottom {
                return Some(rect.top);
            }
        }
    }

    None
}

pub fn show() {
    let saved = fs::read_to_string(state_path())
        .ok()
        .and_then(|text| serde_json::from_str::<SavedTaskbar>(&text).ok());

    let all = taskbars();
    let bars = match saved.and_then(|saved| saved.monitor) {
        Some(monitor) => all
            .into_iter()
            .filter(|bar| window_on_monitor(*bar, monitor))
            .collect::<Vec<HWND>>(),
        None => all,
    };

    for bar in bars {
        if let Some(saved) = saved {
            set_state(bar, saved.state);
        }

        unsafe {
            let _ = ShowWindow(bar, SW_SHOW);
            let _ = ShowWindow(bar, SW_SHOWNA);
        }
    }

    let _ = fs::remove_file(state_path());
}

/// Restores a taskbar left hidden by a previous crashed session.
pub fn cleanup_stale() {
    if state_path().exists() {
        log::warn!("restoring taskbar left hidden by a previous session");
        show();
    }
}

static LAST_BARS: OnceLock<Mutex<Vec<isize>>> = OnceLock::new();

fn last_bars() -> &'static Mutex<Vec<isize>> {
    LAST_BARS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Re-hides the taskbar if Explorer recreated or re-showed it.
pub fn reconcile_all() {
    if !state_path().exists() {
        return;
    }

    let bars = taskbars();
    let keys: Vec<isize> = bars.iter().map(|bar| bar.0 as isize).collect();
    let has_bars = !bars.is_empty();
    let changed = {
        let mut last = last_bars().lock().unwrap();
        if *last != keys {
            *last = keys;
            true
        } else {
            false
        }
    };

    for bar in bars {
        unsafe {
            if IsWindowVisible(bar).as_bool() {
                set_state(bar, ABS_AUTOHIDE | ABS_ALWAYSONTOP);
                let _ = ShowWindow(bar, SW_HIDE);
            }
        }
    }

    // A new set of taskbar windows means Explorer restarted: the shell
    // forgot the dock's appbars, so register them again.
    if changed && has_bars {
        super::appbar::recreate();
    }
}


