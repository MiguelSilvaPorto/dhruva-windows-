use super::AppEntry;
use crate::settings;
use base64::{engine::general_purpose::STANDARD, Engine};
use image::{DynamicImage, ImageFormat, RgbaImage};
use sha2::{Digest, Sha256};
use std::ffi::c_void;
use std::fs;
use std::io::Cursor;
use windows::core::PCWSTR;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, SelectObject, BITMAP,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
};

const ICON_SIZE: i32 = 256;
const TRASH_CLSID: &str = "::{645FF040-5081-101B-9F08-00AA002F954E}";

pub fn for_app(app: &AppEntry) -> Option<String> {
    let mut sources: Vec<&str> = Vec::new();
    if app.aumid.is_some() {
        sources.push(app.icon_source.as_str());
        sources.push(app.target.as_str());
    } else {
        sources.push(app.target.as_str());
        sources.push(app.icon_source.as_str());
    }
    sources.push(app.path.as_str());

    for source in sources {
        if source.trim().is_empty()
            || source.starts_with("ms-")
            || source.starts_with("shell:")
            || source.starts_with("http")
        {
            continue;
        }
        if let Some(icon) = for_source(source, &cache_name(source)) {
            return Some(icon);
        }
    }
    None
}

pub fn for_exe(path: &str) -> Option<String> {
    for_source(path, &cache_name(path))
}

pub fn trash() -> Option<String> {
    for_source(TRASH_CLSID, "trash")
}

pub fn for_source(source: &str, name: &str) -> Option<String> {
    let dir = settings::local_dir().join("icons");
    let cache = dir.join(format!("{name}.png"));

    if let Ok(bytes) = fs::read(&cache) {
        if !bytes.is_empty() {
            return Some(data_url(&bytes));
        }
    }

    let png = extract_png(source)?;
    if fs::create_dir_all(&dir).is_ok() {
        let _ = fs::write(&cache, &png);
    }
    Some(data_url(&png))
}

fn cache_name(source: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source.to_lowercase().as_bytes());
    // Version the cache key with the requested size: older builds stored
    // 128 px versions, and the shell may have scaled a low-resolution icon.
    hasher.update(format!("|{ICON_SIZE}|v2").as_bytes());
    let digest = hasher.finalize();
    super::hex(&digest[..10])
}

fn data_url(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", STANDARD.encode(png))
}

fn extract_png(source: &str) -> Option<Vec<u8>> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let wide: Vec<u16> = source.encode_utf16().chain(std::iter::once(0)).collect();
        let factory: IShellItemImageFactory =
            SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None).ok()?;

        let flags = SIIGBF(SIIGBF_ICONONLY.0 | SIIGBF_BIGGERSIZEOK.0);
        let bitmap = factory
            .GetImage(windows::Win32::Foundation::SIZE {
                cx: ICON_SIZE,
                cy: ICON_SIZE,
            }, flags)
            .ok()?;

        let png = bitmap_to_png(bitmap);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        png
    }
}

unsafe fn bitmap_to_png(bitmap: windows::Win32::Graphics::Gdi::HBITMAP) -> Option<Vec<u8>> {
    let mut info = BITMAP::default();
    let got = GetObjectW(
        HGDIOBJ(bitmap.0),
        std::mem::size_of::<BITMAP>() as i32,
        Some(&mut info as *mut BITMAP as *mut c_void),
    );
    if got == 0 || info.bmWidth <= 0 || info.bmHeight <= 0 {
        return None;
    }

    let width = info.bmWidth;
    let height = info.bmHeight.abs();

    let mut bmi = BITMAPINFO::default();
    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = width;
    bmi.bmiHeader.biHeight = -height;
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB.0;

    let mut pixels = vec![0u8; (width * height * 4) as usize];

    let dc = CreateCompatibleDC(None);
    let old = SelectObject(dc, HGDIOBJ(bitmap.0));
    let lines = GetDIBits(
        dc,
        bitmap,
        0,
        height as u32,
        Some(pixels.as_mut_ptr() as *mut c_void),
        &mut bmi,
        DIB_RGB_COLORS,
    );
    SelectObject(dc, old);
    let _ = DeleteDC(dc);

    if lines == 0 {
        return None;
    }

    let mut has_alpha = false;
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
        if pixel[3] != 0 {
            has_alpha = true;
        }
    }
    if !has_alpha {
        for pixel in pixels.chunks_exact_mut(4) {
            pixel[3] = 255;
        }
    }

    let image = RgbaImage::from_raw(width as u32, height as u32, pixels)?;
    let mut out = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut out, ImageFormat::Png)
        .ok()?;
    Some(out.into_inner())
}
