use base64::{engine::general_purpose::STANDARD, Engine};
use image::{imageops::FilterType, DynamicImage, ImageFormat, RgbaImage};
use std::ffi::c_void;
use std::io::Cursor;
use std::mem::size_of;
use std::sync::mpsc;
use std::time::Duration;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC,
    SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, IsIconic};

const CAPTURE_TIMEOUT: Duration = Duration::from_millis(1600);

pub fn capture(hwnd: i64, max_width: u32, max_height: u32) -> Option<String> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(capture_inner(hwnd, max_width, max_height));
    });
    receiver.recv_timeout(CAPTURE_TIMEOUT).ok().flatten()
}

fn capture_inner(hwnd: i64, max_width: u32, max_height: u32) -> Option<String> {
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
                bitmap,
                hwnd,
                width,
                height,
                window_rect,
                max_width,
                max_height,
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
    bitmap: windows::Win32::Graphics::Gdi::HBITMAP,
    hwnd: HWND,
    width: i32,
    height: i32,
    window_rect: RECT,
    max_width: u32,
    max_height: u32,
) -> Option<String> {
    unsafe {
        let mut info = BITMAPINFO::default();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };

        let mut pixels = vec![0u8; (width * height * 4) as usize];
        let lines = GetDIBits(
            memory,
            bitmap,
            0,
            height as u32,
            Some(pixels.as_mut_ptr() as *mut c_void),
            &mut info,
            DIB_RGB_COLORS,
        );
        if lines == 0 {
            return None;
        }

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

        let mut rgba = vec![0u8; (crop_width * crop_height * 4) as usize];
        for y in 0..crop_height {
            for x in 0..crop_width {
                let source = (((top + y) * width + (left + x)) * 4) as usize;
                let target = ((y * crop_width + x) * 4) as usize;
                rgba[target] = pixels[source + 2];
                rgba[target + 1] = pixels[source + 1];
                rgba[target + 2] = pixels[source];
                rgba[target + 3] = 255;
            }
        }

        let image = RgbaImage::from_raw(crop_width as u32, crop_height as u32, rgba)?;
        let scale = (max_width as f64 / crop_width as f64)
            .min(max_height as f64 / crop_height as f64)
            .clamp(0.05, 1.5);
        let target_width = ((crop_width as f64 * scale).round() as u32).max(1);
        let target_height = ((crop_height as f64 * scale).round() as u32).max(1);

        let resized =
            DynamicImage::ImageRgba8(image).resize(target_width, target_height, FilterType::Triangle);

        let mut png = Cursor::new(Vec::new());
        resized.write_to(&mut png, ImageFormat::Png).ok()?;
        Some(format!(
            "data:image/png;base64,{}",
            STANDARD.encode(png.into_inner())
        ))
    }
}
