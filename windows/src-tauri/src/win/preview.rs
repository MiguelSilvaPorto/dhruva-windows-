use base64::{engine::general_purpose::STANDARD, Engine};
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageFormat, RgbImage};
use std::ffi::c_void;
use std::io::Cursor;
use std::mem::size_of;
use std::sync::mpsc;
use std::time::Duration;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC,
    SelectObject, SetBrushOrgEx, SetStretchBltMode, StretchBlt, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS, HALFTONE, HGDIOBJ, SRCCOPY,
};
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, IsIconic};

/// A busy or hung window can make PrintWindow block. A preview is only worth
/// showing while the pointer is still on the icon, so fail fast instead of
/// leaving the card spinning for almost two seconds.
const CAPTURE_TIMEOUT: Duration = Duration::from_millis(1200);

/// Thumbnails are shown at 240 CSS px wide at most; JPEG keeps the IPC
/// payload small enough to arrive in a few milliseconds.
const JPEG_QUALITY: u8 = 78;

pub fn capture(hwnd: i64, max_width: u32, max_height: u32) -> Option<String> {
    capture_with(hwnd, max_width, max_height, false)
}

/// Compact frame for the live media preview; JPEG keeps the IPC payload small
/// enough to stream a few frames per second.
pub fn capture_jpeg(hwnd: i64, max_width: u32, max_height: u32) -> Option<String> {
    capture_with(hwnd, max_width, max_height, true)
}

fn capture_with(hwnd: i64, max_width: u32, max_height: u32, jpeg: bool) -> Option<String> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(capture_inner(hwnd, max_width, max_height, jpeg));
    });
    receiver.recv_timeout(CAPTURE_TIMEOUT).ok().flatten()
}

fn capture_inner(hwnd: i64, max_width: u32, max_height: u32, jpeg: bool) -> Option<String> {
    unsafe {
        let hwnd = HWND(hwnd as *mut c_void);
        if hwnd.is_invalid() || IsIconic(hwnd).as_bool() {
            return None;
        }

        let mut window_rect = RECT::default();
        GetWindowRect(hwnd, &mut window_rect).ok()?;
        let width = window_rect.right - window_rect.left;
        let height = window_rect.bottom - window_rect.top;
        if width < 48 || height < 32 {
            return None;
        }

        let screen = GetDC(None);
        if screen.is_invalid() {
            return None;
        }

        let memory = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        let old = SelectObject(memory, HGDIOBJ(bitmap.0));
        let mut captured = false;

        for flags in [2u32, 0u32] {
            if PrintWindow(hwnd, memory, PRINT_WINDOW_FLAGS(flags)).as_bool() {
                captured = true;
                break;
            }
        }

        let result = if captured {
            read_pixels(
                memory,
                hwnd,
                width,
                height,
                window_rect,
                max_width,
                max_height,
                jpeg,
            )
        } else {
            None
        };

        SelectObject(memory, old);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        let _ = DeleteDC(memory);
        ReleaseDC(None, screen);

        result
    }
}

#[allow(clippy::too_many_arguments)]
fn read_pixels(
    memory: windows::Win32::Graphics::Gdi::HDC,
    hwnd: HWND,
    width: i32,
    height: i32,
    window_rect: RECT,
    max_width: u32,
    max_height: u32,
    jpeg: bool,
) -> Option<String> {
    unsafe {
        // Drop the invisible resize border/shadow DWM keeps around a window.
        let mut frame = RECT::default();
        let has_frame = DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut frame as *mut RECT as *mut c_void,
            size_of::<RECT>() as u32,
        )
        .is_ok();

        let (left, top, crop_width, crop_height) =
            if has_frame && frame.right > frame.left && frame.bottom > frame.top {
                let left = (frame.left - window_rect.left).clamp(0, width - 1);
                let top = (frame.top - window_rect.top).clamp(0, height - 1);
                let right = (frame.right - window_rect.left).clamp(left + 1, width);
                let bottom = (frame.bottom - window_rect.top).clamp(top + 1, height);
                (left, top, right - left, bottom - top)
            } else {
                (0, 0, width, height)
            };

        let scale = (max_width as f64 / crop_width as f64)
            .min(max_height as f64 / crop_height as f64)
            .clamp(0.05, 1.5);
        let target_width = ((crop_width as f64 * scale).round() as i32).max(1);
        let target_height = ((crop_height as f64 * scale).round() as i32).max(1);

        // Crop and downscale in one GDI call, then read back only the final
        // pixels. Resizing the full frame with the image crate made every
        // preview wait on a multi-megapixel filter (about 1 s in dev builds).
        let target = CreateCompatibleDC(Some(memory));
        let target_bitmap = CreateCompatibleBitmap(memory, target_width, target_height);
        if target.is_invalid() || target_bitmap.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(target_bitmap.0));
            let _ = DeleteDC(target);
            return None;
        }
        let target_old = SelectObject(target, HGDIOBJ(target_bitmap.0));
        let _ = SetStretchBltMode(target, HALFTONE);
        let _ = SetBrushOrgEx(target, 0, 0, None);
        let stretched = StretchBlt(
            target,
            0,
            0,
            target_width,
            target_height,
            Some(memory),
            left,
            top,
            crop_width,
            crop_height,
            SRCCOPY,
        )
        .as_bool();

        let mut info = BITMAPINFO::default();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: target_width,
            biHeight: -target_height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };

        let mut pixels = vec![0u8; (target_width * target_height * 4) as usize];
        let lines = if stretched {
            GetDIBits(
                target,
                target_bitmap,
                0,
                target_height as u32,
                Some(pixels.as_mut_ptr() as *mut c_void),
                &mut info,
                DIB_RGB_COLORS,
            )
        } else {
            0
        };

        SelectObject(target, target_old);
        let _ = DeleteObject(HGDIOBJ(target_bitmap.0));
        let _ = DeleteDC(target);

        if lines == 0 {
            return None;
        }

        // BGRA (top-down DIB) to tightly packed RGB.
        let mut rgb = vec![0u8; (target_width * target_height * 3) as usize];
        for (src, dst) in pixels.chunks_exact(4).zip(rgb.chunks_exact_mut(3)) {
            dst[0] = src[2];
            dst[1] = src[1];
            dst[2] = src[0];
        }

        let image = RgbImage::from_raw(target_width as u32, target_height as u32, rgb)?;
        let image = DynamicImage::ImageRgb8(image);

        if jpeg {
            let mut output = Cursor::new(Vec::new());
            let encoder = JpegEncoder::new_with_quality(&mut output, JPEG_QUALITY);
            image.write_with_encoder(encoder).ok()?;
            return Some(format!(
                "data:image/jpeg;base64,{}",
                STANDARD.encode(output.into_inner())
            ));
        }

        let mut png = Cursor::new(Vec::new());
        image.write_to(&mut png, ImageFormat::Png).ok()?;
        Some(format!(
            "data:image/png;base64,{}",
            STANDARD.encode(png.into_inner())
        ))
    }
}
