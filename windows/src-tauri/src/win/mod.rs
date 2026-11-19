use ::windows::core::BOOL;
use ::windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use ::windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, MonitorFromPoint, MonitorFromRect, MonitorFromWindow, HDC,
    HMONITOR, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use ::windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetSystemMetrics, SystemParametersInfoW, SM_CXSCREEN, SM_CYSCREEN,
    SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
};

pub mod appbar;
pub mod accessibility;
pub mod hotkey;
pub mod media;
pub mod preview;
pub mod processes;
pub mod region;
pub mod shell;
pub mod taskbar;
pub mod theme;
pub mod util;
pub mod virtual_desktops;
pub mod wifi;
pub mod windows;

#[derive(Debug, Clone, Copy)]
pub struct MonitorRects {
    pub work: (i32, i32, i32, i32),
    pub full: (i32, i32, i32, i32),
    pub primary: bool,
}

pub fn work_area() -> (i32, i32, i32, i32) {
    let mut rect = RECT::default();
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut rect as *mut RECT as *mut core::ffi::c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
    }
    if rect.right <= rect.left || rect.bottom <= rect.top {
        (0, 0, 1920, 1080)
    } else {
        (rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top)
    }
}

pub fn primary_monitor_rect() -> (i32, i32, i32, i32) {
    unsafe {
        let width = GetSystemMetrics(SM_CXSCREEN);
        let height = GetSystemMetrics(SM_CYSCREEN);
        if width > 0 && height > 0 {
            (0, 0, width, height)
        } else {
            (0, 0, 1920, 1080)
        }
    }
}

fn rect_tuple(rect: RECT) -> (i32, i32, i32, i32) {
    (rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top)
}

fn info_for(monitor: HMONITOR) -> Option<MonitorRects> {
    unsafe {
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            rcMonitor: RECT::default(),
            rcWork: RECT::default(),
            dwFlags: 0,
        };

        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return None;
        }

        Some(MonitorRects {
            work: rect_tuple(info.rcWork),
            full: rect_tuple(info.rcMonitor),
            primary: (info.dwFlags & 1) != 0,
        })
    }
}

unsafe extern "system" fn enum_monitor(
    monitor: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    lparam: LPARAM,
) -> BOOL {
    let monitors = &mut *(lparam.0 as *mut Vec<MonitorRects>);
    if let Some(info) = info_for(monitor) {
        monitors.push(info);
    }
    BOOL(1)
}

/// Every monitor, sorted left to right.
pub fn monitors() -> Vec<MonitorRects> {
    let mut result: Vec<MonitorRects> = Vec::new();

    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(enum_monitor),
            LPARAM(&mut result as *mut Vec<MonitorRects> as isize),
        );
    }

    result.sort_by_key(|monitor| monitor.full.0);

    if result.is_empty() {
        let (work, full) = cursor_monitor_metrics();
        result.push(MonitorRects {
            work,
            full,
            primary: true,
        });
    }

    result
}

/// Work area and full rect of the monitor under the cursor.
pub fn cursor_monitor_metrics() -> ((i32, i32, i32, i32), (i32, i32, i32, i32)) {
    unsafe {
        let mut point = POINT::default();
        if GetCursorPos(&mut point).is_err() {
            return (work_area(), primary_monitor_rect());
        }

        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        if let Some(info) = info_for(monitor) {
            (info.work, info.full)
        } else {
            (work_area(), primary_monitor_rect())
        }
    }
}

/// Work area of the monitor that contains this full monitor rectangle.
pub fn work_area_for(full: (i32, i32, i32, i32)) -> Option<(i32, i32, i32, i32)> {
    let (x, y, width, height) = full;
    let rect = RECT {
        left: x,
        top: y,
        right: x + width,
        bottom: y + height,
    };

    unsafe {
        let monitor = MonitorFromRect(&rect, MONITOR_DEFAULTTONEAREST);
        info_for(monitor).map(|info| info.work)
    }
}

pub fn window_monitor_metrics(
    window: &tauri::WebviewWindow,
) -> ((i32, i32, i32, i32), (i32, i32, i32, i32)) {
    if let Some(hwnd) = hwnd_of(window) {
        unsafe {
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            if let Some(info) = info_for(monitor) {
                return (info.work, info.full);
            }
        }
    }

    cursor_monitor_metrics()
}

pub fn hwnd_of(window: &tauri::WebviewWindow) -> Option<HWND> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = window.window_handle().ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(h) => Some(HWND(h.hwnd.get() as *mut core::ffi::c_void)),
        _ => None,
    }
}

pub fn hwnd_from_i64(value: i64) -> HWND {
    HWND(value as *mut core::ffi::c_void)
}
