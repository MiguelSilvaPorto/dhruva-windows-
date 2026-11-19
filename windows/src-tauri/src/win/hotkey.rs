use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::AppHandle;
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, VK_ESCAPE, VK_LWIN,
    VK_RWIN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId,
    SetWindowsHookExW, TranslateMessage, HC_ACTION, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, LLKHF_INJECTED,
    MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

static APP: OnceLock<AppHandle> = OnceLock::new();
static WIN_DOWN: AtomicBool = AtomicBool::new(false);
static COMBO: AtomicBool = AtomicBool::new(false);
static CHORD_INTENT: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy)]
struct PendingModifier {
    vk: u32,
    scan: u32,
    extended: bool,
}

static PENDING_MODS: Mutex<Vec<PendingModifier>> = Mutex::new(Vec::new());

fn is_modifier(vk: u32) -> bool {
    matches!(vk, 0x10 | 0x11 | 0x12 | 0xA0 | 0xA1 | 0xA2 | 0xA3 | 0xA4 | 0xA5)
}

fn inject_key(vk: u32, scan: u32, extended: bool) {
    let flags = if extended {
        KEYEVENTF_EXTENDEDKEY
    } else {
        KEYBD_EVENT_FLAGS(0)
    };
    unsafe {
        keybd_event(vk as u8, scan as u8, flags, 0);
    }
}

fn clear_pending() -> bool {
    PENDING_MODS
        .lock()
        .map(|mut pending| {
            let had = !pending.is_empty();
            pending.clear();
            had
        })
        .unwrap_or(false)
}

unsafe extern "system" fn keyboard_hook(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code == HC_ACTION as i32 {
        let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let is_win = info.vkCode == VK_LWIN.0 as u32 || info.vkCode == VK_RWIN.0 as u32;
        let vk = info.vkCode;
        let scan = info.scanCode;
        let extended = info.flags.0 & LLKHF_EXTENDED.0 != 0;
        let message = wparam.0 as u32;

        // Synthetic events (the Win-down/key-down/Win-up pair we send for a
        // chord) must reach Windows untouched.
        if info.flags.0 & LLKHF_INJECTED.0 != 0 {
            return CallNextHookEx(None, code, wparam, lparam);
        }

        match message {
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                if is_win {
                    // Windows must never see the physical Windows key. It is
                    // only replayed as a full chord when another key follows.
                    if !WIN_DOWN.swap(true, Ordering::SeqCst) {
                        COMBO.store(false, Ordering::SeqCst);
                        CHORD_INTENT.store(false, Ordering::SeqCst);
                        let _ = clear_pending();
                    }
                    return LRESULT(1);
                }

                if !WIN_DOWN.load(Ordering::SeqCst) {
                    return CallNextHookEx(None, code, wparam, lparam);
                }

                CHORD_INTENT.store(true, Ordering::SeqCst);

                if is_modifier(vk) {
                    // Hold modifiers until the real chord key arrives, so
                    // Win+Shift+S and Win+Ctrl+Left keep working.
                    if !COMBO.load(Ordering::SeqCst) {
                        if let Ok(mut pending) = PENDING_MODS.lock() {
                            if !pending.iter().any(|item| item.vk == vk) {
                                pending.push(PendingModifier {
                                    vk,
                                    scan,
                                    extended,
                                });
                            }
                        }
                    }
                    return LRESULT(1);
                }

                // Only one chord per Windows press. Extra keys are ignored
                // until the Windows key is released.
                if COMBO.swap(true, Ordering::SeqCst) {
                    return LRESULT(1);
                }

                let pending = PENDING_MODS
                    .lock()
                    .map(|mut list| std::mem::take(&mut *list))
                    .unwrap_or_default();

                // Replay the chord atomically from Windows' point of view:
                // Win down, held modifiers, the key, then Win up. Windows
                // sees a real shortcut and never opens Start on release.
                unsafe {
                    keybd_event(VK_LWIN.0 as u8, 0, KEYBD_EVENT_FLAGS(0), 0);
                }
                for modifier in pending {
                    inject_key(modifier.vk, modifier.scan, modifier.extended);
                }
                inject_key(vk, scan, extended);
                unsafe {
                    keybd_event(VK_LWIN.0 as u8, 0, KEYEVENTF_KEYUP, 0);
                }

                // The physical key is swallowed; the injected copy is the
                // one Windows receives. Its physical key-up still passes and
                // releases the injected key.
                return LRESULT(1);
            }
            WM_KEYUP | WM_SYSKEYUP => {
                if is_win {
                    WIN_DOWN.store(false, Ordering::SeqCst);
                    let combo = COMBO.swap(false, Ordering::SeqCst);
                    let intent = CHORD_INTENT.swap(false, Ordering::SeqCst);
                    let had_pending = clear_pending();

                    if !combo && !intent && !had_pending {
                        if let Some(app) = APP.get().cloned() {
                            std::thread::spawn(move || {
                                crate::commands::toggle_launcher_cursor(&app);
                            });
                        }
                    }

                    // Always swallow the physical Win-up. A synthetic one
                    // was already sent for chords; lone taps have no OS-side
                    // Win state at all.
                    return LRESULT(1);
                }

                if !WIN_DOWN.load(Ordering::SeqCst) {
                    return CallNextHookEx(None, code, wparam, lparam);
                }

                if is_modifier(vk) && !COMBO.load(Ordering::SeqCst) {
                    if let Ok(mut pending) = PENDING_MODS.lock() {
                        pending.retain(|item| item.vk != vk);
                    }
                    // The physical down was swallowed, so its up must be too.
                    return LRESULT(1);
                }

                // After a chord, pass key-ups so the injected keys/modifiers
                // are released.
                return CallNextHookEx(None, code, wparam, lparam);
            }
            _ => {}
        }
    }

    CallNextHookEx(None, code, wparam, lparam)
}

fn close_start_menu_forever() {
    std::thread::spawn(|| unsafe {
        loop {
            std::thread::sleep(Duration::from_millis(120));
            let hwnd = GetForegroundWindow();
            if hwnd.is_invalid() {
                continue;
            }

            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == 0 {
                continue;
            }

            let Some(path) = process_path(pid) else {
                continue;
            };
            let is_start_surface = path.ends_with("startmenuexperiencehost.exe")
                || path.ends_with("searchapp.exe")
                || path.ends_with("searchhost.exe");
            if !is_start_surface {
                continue;
            }

            // Safety net: if Windows ever manages to open its Start menu or
            // search flyout, dismiss it before it can stay on screen.
            keybd_event(VK_ESCAPE.0 as u8, 0, KEYBD_EVENT_FLAGS(0), 0);
            keybd_event(VK_ESCAPE.0 as u8, 0, KEYEVENTF_KEYUP, 0);
        }
    });
}

fn process_path(pid: u32) -> Option<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = [0u16; 1024];
        let mut size = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(handle);
        result.ok()?;
        Some(String::from_utf16_lossy(&buffer[..size as usize]).to_lowercase())
    }
}

/// Replaces a lone press of the Windows key with the Dhruva launcher while
/// replaying real Win chords. The physical Win key is never forwarded to
/// Windows, so the native Start menu cannot open.
pub fn install(app: AppHandle) {
    let _ = APP.set(app);

    close_start_menu_forever();

    std::thread::spawn(|| unsafe {
        let hook = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), None, 0) {
            Ok(hook) => hook,
            Err(error) => {
                log::warn!("could not install the Windows key hook: {error}");
                return;
            }
        };
        let _hook = hook;

        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    });
}
