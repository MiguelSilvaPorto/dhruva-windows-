use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::AppHandle;
use windows::core::{BOOL, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, VK_ESCAPE, VK_LWIN,
    VK_RWIN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, EnumWindows, GetClassNameW, GetForegroundWindow, GetMessageW,
    GetWindowThreadProcessId, IsWindowVisible, PostMessageW, SetWindowsHookExW, ShowWindow,
    TranslateMessage, HC_ACTION, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, LLKHF_INJECTED, MSG, SW_HIDE,
    WH_KEYBOARD_LL, WM_CLOSE, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

static WIN_DOWN: AtomicBool = AtomicBool::new(false);
static WIN_DOWN_AT: AtomicU64 = AtomicU64::new(0);
static COMBO: AtomicBool = AtomicBool::new(false);
static CHORD_INTENT: AtomicBool = AtomicBool::new(false);

/// A Win key-up lost to a focus change or a stuck key would make every later
/// key look like a Win chord and freeze normal typing. The state is dropped
/// after this long without a Win-up.
const WIN_STICKY_MS: u64 = 1500;

/// Launcher toggles run one at a time, in press order. Spawning a thread per
/// key-up lets two quick taps both read the same visibility state, which
/// desyncs the launcher and can leave focus on a hidden shell window.
static LAUNCHER_TX: Mutex<Option<Sender<LauncherRequest>>> = Mutex::new(None);

#[derive(Clone, Copy)]
enum LauncherRequest {
    Toggle,
    Open,
}

/// Marks the events this hook injects itself. Other injected events are
/// treated as real input so automation and remappers still go through the
/// launcher instead of opening the native Start menu.
pub const DHURVA_INJECTED: usize = 0x44_48_52_55;

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
        keybd_event(vk as u8, scan as u8, flags, DHURVA_INJECTED);
    }
}

fn request_launcher(request: LauncherRequest) {
    if let Ok(slot) = LAUNCHER_TX.lock() {
        if let Some(tx) = slot.as_ref() {
            let _ = tx.send(request);
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

fn reset_win_state() {
    WIN_DOWN.store(false, Ordering::SeqCst);
    WIN_DOWN_AT.store(0, Ordering::SeqCst);
    COMBO.store(false, Ordering::SeqCst);
    CHORD_INTENT.store(false, Ordering::SeqCst);
    let _ = clear_pending();
}

fn win_state_is_stale() -> bool {
    WIN_DOWN.load(Ordering::SeqCst)
        && now_ms().saturating_sub(WIN_DOWN_AT.load(Ordering::SeqCst)) > WIN_STICKY_MS
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
        // chord, and the Escape that dismisses a shell flyout) must reach
        // Windows untouched. Only the physical keyboard drives the state
        // machine; injected input from automation never gets stuck here.
        if info.flags.0 & LLKHF_INJECTED.0 != 0 {
            return CallNextHookEx(None, code, wparam, lparam);
        }

        match message {
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                if is_win {
                    // Windows must never see the physical Windows key. It is
                    // only replayed as a full chord when another key follows.
                    if !WIN_DOWN.swap(true, Ordering::SeqCst) {
                        WIN_DOWN_AT.store(now_ms(), Ordering::SeqCst);
                        COMBO.store(false, Ordering::SeqCst);
                        CHORD_INTENT.store(false, Ordering::SeqCst);
                        let _ = clear_pending();
                    }
                    return LRESULT(1);
                }

                if !WIN_DOWN.load(Ordering::SeqCst) {
                    return CallNextHookEx(None, code, wparam, lparam);
                }

                // Recover from a Win key-up that was lost (focus change,
                // session lock, another hook). Without this, every later key
                // would be replayed as a Win chord and typing would freeze.
                if win_state_is_stale() {
                    reset_win_state();
                    return CallNextHookEx(None, code, wparam, lparam);
                }

                CHORD_INTENT.store(true, Ordering::SeqCst);

                // Win+Q and Win+S are the native search shortcuts. Dhruva is
                // the search surface, so they open/focus the launcher instead
                // of letting Windows show its own search flyout.
                if matches!(vk, 0x51 | 0x53) {
                    let has_modifier = PENDING_MODS
                        .lock()
                        .map(|pending| !pending.is_empty())
                        .unwrap_or(false);
                    if !has_modifier && !COMBO.swap(true, Ordering::SeqCst) {
                        request_launcher(LauncherRequest::Open);
                        return LRESULT(1);
                    }
                }

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
                    keybd_event(VK_LWIN.0 as u8, 0, KEYBD_EVENT_FLAGS(0), DHURVA_INJECTED);
                }
                for modifier in pending {
                    inject_key(modifier.vk, modifier.scan, modifier.extended);
                }
                inject_key(vk, scan, extended);
                unsafe {
                    keybd_event(VK_LWIN.0 as u8, 0, KEYEVENTF_KEYUP, DHURVA_INJECTED);
                }

                // The physical key is swallowed; the injected copy is the
                // one Windows receives. Its physical key-up still passes and
                // releases the injected key.
                return LRESULT(1);
            }
            WM_KEYUP | WM_SYSKEYUP => {
                if is_win {
                    let combo = COMBO.swap(false, Ordering::SeqCst);
                    let intent = CHORD_INTENT.swap(false, Ordering::SeqCst);
                    let had_pending = clear_pending();
                    WIN_DOWN.store(false, Ordering::SeqCst);
                    WIN_DOWN_AT.store(0, Ordering::SeqCst);

                    if !combo && !intent && !had_pending {
                        request_launcher(LauncherRequest::Toggle);
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

fn is_start_surface_path(path: &str) -> bool {
    path.ends_with("startmenuexperiencehost.exe")
        || path.ends_with("searchapp.exe")
        || path.ends_with("searchhost.exe")
}

/// True for the native Start menu and Windows Search surfaces.
pub fn is_start_surface_pid(pid: u32) -> bool {
    process_path(pid)
        .map(|path| is_start_surface_path(&path))
        .unwrap_or(false)
}

unsafe extern "system" fn enum_start_surface(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let found = &mut *(lparam.0 as *mut Vec<HWND>);
    if !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1);
    }

    // Cheap class filter first: the Start menu and the Search flyout are
    // XAML CoreWindows. Opening a process for every visible window every
    // 120 ms would be far more expensive than this check.
    let mut class = [0u16; 64];
    let len = GetClassNameW(hwnd, &mut class);
    let class = String::from_utf16_lossy(&class[..len.max(0) as usize]);
    if !class.starts_with("Windows.UI.") {
        return BOOL(1);
    }

    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == 0 {
        return BOOL(1);
    }

    if is_start_surface_pid(pid) {
        found.push(hwnd);
    }

    BOOL(1)
}

/// Dismisses the native Start/Search flyout when it currently owns focus.
pub fn dismiss_start_surface() -> bool {
    unsafe {
        let foreground = GetForegroundWindow();
        if foreground.is_invalid() {
            return false;
        }

        let mut pid = 0u32;
        GetWindowThreadProcessId(foreground, Some(&mut pid));
        if !is_start_surface_pid(pid) {
            return false;
        }

        keybd_event(
            VK_ESCAPE.0 as u8,
            0,
            KEYBD_EVENT_FLAGS(0),
            DHURVA_INJECTED,
        );
        keybd_event(
            VK_ESCAPE.0 as u8,
            0,
            KEYEVENTF_KEYUP,
            DHURVA_INJECTED,
        );
        true
    }
}

fn close_start_menu_forever() {
    std::thread::spawn(|| unsafe {
        loop {
            std::thread::sleep(Duration::from_millis(120));

            let mut found: Vec<HWND> = Vec::new();
            let _ = EnumWindows(
                Some(enum_start_surface),
                LPARAM(&mut found as *mut Vec<HWND> as isize),
            );

            for hwnd in found {
                if GetForegroundWindow() == hwnd {
                    // Safety net: if Windows ever manages to open its Start
                    // menu or search flyout, dismiss it before it can stay
                    // on screen.
                    keybd_event(
                        VK_ESCAPE.0 as u8,
                        0,
                        KEYBD_EVENT_FLAGS(0),
                        DHURVA_INJECTED,
                    );
                    keybd_event(
                        VK_ESCAPE.0 as u8,
                        0,
                        KEYEVENTF_KEYUP,
                        DHURVA_INJECTED,
                    );
                }

                // Even when it is behind the launcher, it must not be revealed
                // when the launcher closes.
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
        }
    });
}

pub fn process_path(pid: u32) -> Option<String> {
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
    let (launcher_tx, launcher_rx) = mpsc::channel::<LauncherRequest>();
    if let Ok(mut slot) = LAUNCHER_TX.lock() {
        *slot = Some(launcher_tx);
    }
    let launcher_app = app.clone();
    std::thread::spawn(move || {
        while let Ok(request) = launcher_rx.recv() {
            // Window visibility and foreground changes must run on the Tauri
            // main thread; doing them from this worker makes the focus
            // hand-off fail and lets the shell keep a hidden Start/Search
            // window as the foreground.
            let app = launcher_app.clone();
            let _ = launcher_app.run_on_main_thread(move || match request {
                LauncherRequest::Toggle => {
                    crate::commands::toggle_launcher_cursor(&app);
                }
                LauncherRequest::Open => {
                    crate::commands::open_launcher_cursor(&app);
                }
            });
        }
    });

    close_start_menu_forever();

    // If a Win key-up never arrives, release the state even with no further
    // input, so the keyboard is never left in "Win is held" mode.
    std::thread::spawn(|| loop {
        std::thread::sleep(Duration::from_millis(250));
        if win_state_is_stale() {
            reset_win_state();
        }
    });

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
