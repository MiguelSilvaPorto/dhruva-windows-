use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, CreateRoundRectRgn, DeleteObject, SetWindowRgn, HGDIOBJ, RGN_OR,
};

/// Sets a rounded rectangular window region. The window region controls both
/// painting and hit-testing, which is what makes the transparent parts of the
/// dock click-through.
pub fn set_rounded(hwnd: HWND, x: i32, y: i32, width: i32, height: i32, radius: i32) {
    if width <= 0 || height <= 0 {
        return;
    }

    unsafe {
        let rgn = CreateRoundRectRgn(
            x,
            y,
            x + width,
            y + height,
            (radius.max(0)) * 2,
            (radius.max(0)) * 2,
        );
        if rgn.is_invalid() {
            return;
        }

        if SetWindowRgn(hwnd, Some(rgn), true) == 0 {
            let _ = DeleteObject(HGDIOBJ(rgn.0));
        }
    }
}

/// Sets a region made of several rounded rectangles joined together. Used
/// when the context menu opens: the bar and the menu stay clickable without
/// blocking the whole window.
pub fn set_rounded_multi(hwnd: HWND, rects: &[(i32, i32, i32, i32, i32)]) {
    if rects.is_empty() {
        return;
    }

    unsafe {
        let combined = CreateRectRgn(0, 0, 0, 0);
        if combined.is_invalid() {
            return;
        }

        let mut added = false;
        for &(x, y, width, height, radius) in rects {
            if width <= 0 || height <= 0 {
                continue;
            }

            let piece = CreateRoundRectRgn(
                x,
                y,
                x + width,
                y + height,
                radius.max(0) * 2,
                radius.max(0) * 2,
            );
            if piece.is_invalid() {
                continue;
            }

            let _ = CombineRgn(Some(combined), Some(combined), Some(piece), RGN_OR);
            let _ = DeleteObject(HGDIOBJ(piece.0));
            added = true;
        }

        if !added {
            let _ = DeleteObject(HGDIOBJ(combined.0));
            return;
        }

        if SetWindowRgn(hwnd, Some(combined), true) == 0 {
            log::warn!(
                "could not apply the dock region: {}",
                std::io::Error::last_os_error()
            );
            let _ = DeleteObject(HGDIOBJ(combined.0));
        }
    }
}

pub fn set_strip(hwnd: HWND, window_height: i32, strip_height: i32) {
    set_rounded(
        hwnd,
        0,
        window_height - strip_height.max(1),
        100_000,
        strip_height.max(1),
        0,
    );
}

pub fn clear(hwnd: HWND) {
    unsafe {
        let _ = SetWindowRgn(hwnd, None, true);
    }
}
