use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;
use std::time::Duration;
use windows::core::{BOOL, HSTRING, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmRegisterThumbnail, DwmUnregisterThumbnail, DwmUpdateThumbnailProperties,
    DWM_THUMBNAIL_PROPERTIES, DWM_TNP_OPACITY, DWM_TNP_RECTDESTINATION,
    DWM_TNP_SOURCECLIENTAREAONLY, DWM_TNP_VISIBLE,
};
use windows::Win32::Graphics::Gdi::{
    CreateRoundRectRgn, DeleteObject, SetWindowRgn, HGDIOBJ,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, PeekMessageW, RegisterClassW, SetWindowPos,
    ShowWindow, TranslateMessage, HWND_TOPMOST, MSG, PM_REMOVE, SW_HIDE, SW_SHOWNOACTIVATE,
    SWP_NOACTIVATE, SWP_SHOWWINDOW, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_POPUP,
};

enum Command {
    Show {
        source: isize,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        reply: Sender<bool>,
    },
    Hide,
}

static SENDER: Mutex<Option<Sender<Command>>> = Mutex::new(None);

fn sender() -> Option<Sender<Command>> {
    let mut slot = SENDER.lock().ok()?;
    if let Some(tx) = slot.as_ref() {
        return Some(tx.clone());
    }

    let (tx, rx) = mpsc::channel();
    *slot = Some(tx.clone());
    std::thread::spawn(move || run(rx));
    Some(tx)
}

/// Shows a live DWM thumbnail of `source` in a click-through topmost window.
/// Returns false when DWM or the window cannot be created.
pub fn show(source: i64, x: i32, y: i32, width: i32, height: i32) -> bool {
    let Some(tx) = sender() else {
        return false;
    };

    let (reply_tx, reply_rx) = mpsc::channel();
    if tx
        .send(Command::Show {
            source: source as isize,
            x,
            y,
            width,
            height,
            reply: reply_tx,
        })
        .is_err()
    {
        return false;
    }

    reply_rx
        .recv_timeout(Duration::from_millis(1500))
        .unwrap_or(false)
}

pub fn hide() {
    if let Some(tx) = sender() {
        let _ = tx.send(Command::Hide);
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

fn run(rx: Receiver<Command>) {
    unsafe {
        let Ok(module) = GetModuleHandleW(None) else {
            return;
        };
        let class = HSTRING::from("DhruvaMediaPreviewWindow");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: HINSTANCE(module.0),
            lpszClassName: PCWSTR(class.as_ptr()),
            ..Default::default()
        };
        if RegisterClassW(&wc) == 0 {
            log::warn!("media thumbnail: RegisterClassW failed");
            return;
        }

        let hwnd = match CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_TRANSPARENT,
            PCWSTR(class.as_ptr()),
            PCWSTR::null(),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            Some(HINSTANCE(module.0)),
            None,
        ) {
            Ok(hwnd) => hwnd,
            Err(error) => {
                log::warn!("media thumbnail: CreateWindowExW failed: {error}");
                return;
            }
        };
        let _ = ShowWindow(hwnd, SW_HIDE);

        let mut thumbnail: isize = 0;
        let mut source: isize = 0;

        loop {
            let mut message = MSG::default();
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }

            match rx.recv_timeout(Duration::from_millis(16)) {
                Ok(Command::Show {
                    source: new_source,
                    x,
                    y,
                    width,
                    height,
                    reply,
                }) => {
                    let shown =
                        show_inner(hwnd, &mut thumbnail, &mut source, new_source, x, y, width, height);
                    let _ = reply.send(shown);
                }
                Ok(Command::Hide) => hide_inner(hwnd, &mut thumbnail, &mut source),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        if thumbnail != 0 {
            let _ = DwmUnregisterThumbnail(thumbnail);
        }
    }
}

unsafe fn show_inner(
    hwnd: HWND,
    thumbnail: &mut isize,
    source: &mut isize,
    new_source: isize,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> bool {
    if new_source == 0 || width <= 0 || height <= 0 {
        return false;
    }

    if *thumbnail == 0 || *source != new_source {
        if *thumbnail != 0 {
            let _ = DwmUnregisterThumbnail(*thumbnail);
            *thumbnail = 0;
        }
        match DwmRegisterThumbnail(hwnd, HWND(new_source as *mut _)) {
            Ok(registered) => {
                *thumbnail = registered;
                *source = new_source;
            }
            Err(error) => {
                log::warn!("media thumbnail: DwmRegisterThumbnail failed: {error}");
                return false;
            }
        }
    }

    let region = CreateRoundRectRgn(0, 0, width + 1, height + 1, 24, 24);
    if region.is_invalid() {
        return false;
    }
    if SetWindowRgn(hwnd, Some(region), true) == 0 {
        let _ = DeleteObject(HGDIOBJ(region.0));
    }

    if let Err(error) = SetWindowPos(
        hwnd,
        Some(HWND_TOPMOST),
        x,
        y,
        width,
        height,
        SWP_NOACTIVATE | SWP_SHOWWINDOW,
    ) {
        log::warn!("media thumbnail: SetWindowPos failed: {error}");
        return false;
    }
    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);

    let properties = DWM_THUMBNAIL_PROPERTIES {
        dwFlags: DWM_TNP_RECTDESTINATION
            | DWM_TNP_VISIBLE
            | DWM_TNP_SOURCECLIENTAREAONLY
            | DWM_TNP_OPACITY,
        rcDestination: RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        },
        opacity: 255,
        fVisible: BOOL(1),
        fSourceClientAreaOnly: BOOL(1),
        ..Default::default()
    };

    match DwmUpdateThumbnailProperties(*thumbnail, &properties) {
        Ok(()) => true,
        Err(error) => {
            log::warn!("media thumbnail: DwmUpdateThumbnailProperties failed: {error}");
            false
        }
    }
}

unsafe fn hide_inner(hwnd: HWND, thumbnail: &mut isize, source: &mut isize) {
    let _ = ShowWindow(hwnd, SW_HIDE);
    if *thumbnail != 0 {
        let _ = DwmUnregisterThumbnail(*thumbnail);
        *thumbnail = 0;
    }
    *source = 0;
}
