use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::Shell::{SHAppBarMessage, APPBARDATA};

use super::taskbar;

const ABM_NEW: u32 = 0x0000_0000;
const ABM_REMOVE: u32 = 0x0000_0001;
const ABM_QUERYPOS: u32 = 0x0000_0002;
const ABM_SETPOS: u32 = 0x0000_0003;
const ABE_BOTTOM: u32 = 0x0000_0003;
const WM_APP_CALLBACK: u32 = 0x8001;

#[derive(Clone, Copy)]
struct Reservation {
    monitor: (i32, i32, i32, i32),
    height: i32,
}

static RESERVED: OnceLock<Mutex<HashMap<isize, Reservation>>> = OnceLock::new();

fn reserved() -> &'static Mutex<HashMap<isize, Reservation>> {
    RESERVED.get_or_init(|| Mutex::new(HashMap::new()))
}

fn key_of(hwnd: HWND) -> isize {
    hwnd.0 as isize
}

fn data_for(hwnd: HWND) -> APPBARDATA {
    APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        hWnd: hwnd,
        uCallbackMessage: WM_APP_CALLBACK,
        ..Default::default()
    }
}

/// Bottom edge the dock should use on this monitor: above a visible bottom
/// taskbar, otherwise the screen bottom. Our own reservations are ignored
/// here; the appbar is always anchored to the screen edge, so re-applying it
/// cannot walk it off-screen.
pub fn base_bottom(monitor: (i32, i32, i32, i32)) -> i32 {
    if !taskbar::is_hidden_by_us() {
        if let Some(top) = taskbar::visible_top_for_monitor(monitor) {
            return top;
        }
    }

    monitor.1 + monitor.3
}

fn set_pos(hwnd: HWND, reservation: Reservation, bottom: i32) -> Result<(), String> {
    let height = reservation.height.max(8);
    let (x, _, width, _) = reservation.monitor;

    unsafe {
        let mut data = data_for(hwnd);
        data.uEdge = ABE_BOTTOM;
        data.rc = RECT {
            left: x,
            top: bottom - height,
            right: x + width,
            bottom,
        };

        let _ = SHAppBarMessage(ABM_QUERYPOS, &mut data);

        // Keep the strip anchored to the dock, even if the shell offered a
        // different rectangle because of another appbar.
        data.rc.top = bottom - height;
        data.rc.bottom = bottom;

        let result = SHAppBarMessage(ABM_SETPOS, &mut data);
        log::debug!(
            "appbar setpos hwnd={:?} edge={} rc=({},{}..{}, {}) result={} work={:?}",
            hwnd,
            data.uEdge,
            data.rc.left,
            data.rc.top,
            data.rc.right,
            data.rc.bottom,
            result,
            super::work_area_for(reservation.monitor)
        );
        if result == 0 {
            return Err("ABM_SETPOS failed".to_string());
        }
    }

    Ok(())
}

/// Reserves a full-width strip at the bottom of the dock's monitor. Windows
/// then keeps maximized windows above it instead of letting them run under
/// the bar.
pub fn reserve(hwnd: HWND, monitor: (i32, i32, i32, i32), height: i32) {
    let reservation = Reservation {
        monitor,
        height: height.max(8),
    };

    // The bottom has to be measured before the new height enters the map.
    let bottom = base_bottom(monitor);
    log::debug!(
        "appbar reserve hwnd={:?} monitor={:?} height={} bottom={}",
        hwnd,
        monitor,
        reservation.height,
        bottom
    );
    let key = key_of(hwnd);
    let existed = reserved()
        .lock()
        .unwrap()
        .insert(key, reservation)
        .is_some();

    if !existed {
        unsafe {
            let mut data = data_for(hwnd);
            if SHAppBarMessage(ABM_NEW, &mut data) == 0 {
                log::warn!("ABM_NEW failed for the dock appbar");
            }
        }
    }

    if let Err(error) = set_pos(hwnd, reservation, bottom) {
        log::warn!("could not reserve the dock area: {error}");
    }
}

/// Re-applies every reservation after the taskbar is shown or hidden.
pub fn refresh() {
    let entries: Vec<(isize, Reservation)> = reserved()
        .lock()
        .unwrap()
        .iter()
        .map(|(key, reservation)| (*key, *reservation))
        .collect();

    for (key, reservation) in entries {
        let bottom = base_bottom(reservation.monitor);
        let hwnd = HWND(key as *mut core::ffi::c_void);
        if let Err(error) = set_pos(hwnd, reservation, bottom) {
            log::warn!("could not refresh the dock appbar: {error}");
        }
    }
}

/// Registers the appbars again after Explorer restarted and forgot them.
pub fn recreate() {
    let entries: Vec<(isize, Reservation)> = reserved()
        .lock()
        .unwrap()
        .iter()
        .map(|(key, reservation)| (*key, *reservation))
        .collect();

    for (key, reservation) in entries {
        let hwnd = HWND(key as *mut core::ffi::c_void);
        let bottom = base_bottom(reservation.monitor);

        unsafe {
            let mut data = data_for(hwnd);
            let _ = SHAppBarMessage(ABM_REMOVE, &mut data);
            if SHAppBarMessage(ABM_NEW, &mut data) == 0 {
                log::warn!("ABM_NEW failed while recreating the dock appbar");
            }
        }

        if let Err(error) = set_pos(hwnd, reservation, bottom) {
            log::warn!("could not recreate the dock appbar: {error}");
        }
    }
}

/// Gives the reserved strip back. Called when auto-hide is enabled, and on
/// exit before the taskbar is restored.
pub fn release(hwnd: HWND) {
    let key = key_of(hwnd);
    let existed = reserved().lock().unwrap().remove(&key).is_some();
    if !existed {
        return;
    }

    unsafe {
        let mut data = data_for(hwnd);
        let _ = SHAppBarMessage(ABM_REMOVE, &mut data);
    }
}

pub fn release_all() {
    let entries: Vec<isize> = reserved().lock().unwrap().keys().copied().collect();
    for key in entries {
        release(HWND(key as *mut core::ffi::c_void));
    }
}
