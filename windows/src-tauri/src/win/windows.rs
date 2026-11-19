use serde::Serialize;
use std::collections::HashMap;
use std::sync::OnceLock;
use windows::core::{GUID, BOOL, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    AttachThreadInput, GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Shell::IVirtualDesktopManager;
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, EnumWindows, GetClassNameW, GetForegroundWindow, GetWindow,
    GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible, PostMessageW, SetForegroundWindow, ShowWindow, SwitchToThisWindow, GWL_EXSTYLE,
    GW_OWNER, SW_MINIMIZE, SW_RESTORE, WM_CLOSE, WS_EX_TOOLWINDOW,
};

const VIRTUAL_DESKTOP_MANAGER_CLSID: GUID =
    GUID::from_u128(0xAA509086_5CA9_4C25_8F95_589D3C07B48A);

thread_local! {
    static VDM: OnceLock<Option<IVirtualDesktopManager>> = OnceLock::new();
}

fn virtual_desktop_manager() -> Option<IVirtualDesktopManager> {
    VDM.with(|cell| {
        cell.get_or_init(|| unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            CoCreateInstance(&VIRTUAL_DESKTOP_MANAGER_CLSID, None, CLSCTX_ALL).ok()
        })
        .clone()
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowInfo {
    pub hwnd: i64,
    pub title: String,
    pub exe: String,
    pub pid: u32,
    pub minimized: bool,
    pub focused: bool,
}

const SKIP_CLASSES: &[&str] = &[
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "Windows.UI.Core.CoreWindow",
    "ApplicationManager_DesktopShellWindow",
    "DV2ControlHost",
    "MsgrIMEWindowClass",
    "SysShadow",
    "TaskListThumbnailWnd",
    "ForegroundStaging",
    "Shell_InputSwitchTopLevelWindow",
    "MultitaskingViewFrame",
    "XamlExplorerHostIslandWindow",
];

struct EnumContext {
    items: Vec<WindowInfo>,
    exe_cache: HashMap<u32, String>,
    process_names: HashMap<u32, String>,
    foreground: HWND,
    own_pid: u32,
    vdm: Option<IVirtualDesktopManager>,
}

pub fn list() -> Vec<WindowInfo> {
    let vdm = virtual_desktop_manager();

    let mut context = EnumContext {
        items: Vec::new(),
        exe_cache: HashMap::new(),
        process_names: process_names(),
        foreground: unsafe { GetForegroundWindow() },
        own_pid: std::process::id(),
        vdm,
    };

    unsafe {
        let _ = EnumWindows(
            Some(enum_window),
            LPARAM(&mut context as *mut EnumContext as isize),
        );
    }

    context.items
}

unsafe extern "system" fn enum_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let context = &mut *(lparam.0 as *mut EnumContext);
    if let Some(window) = inspect(hwnd, context) {
        context.items.push(window);
    }
    BOOL(1)
}

unsafe fn inspect(hwnd: HWND, context: &mut EnumContext) -> Option<WindowInfo> {
    if !IsWindowVisible(hwnd).as_bool() {
        return None;
    }

    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid as *mut u32));
    if pid == 0 || pid == context.own_pid {
        return None;
    }

    if let Ok(owner) = GetWindow(hwnd, GW_OWNER) {
        if !owner.is_invalid() {
            return None;
        }
    }

    let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
    if ex_style & WS_EX_TOOLWINDOW.0 != 0 {
        return None;
    }

    if let Some(vdm) = &context.vdm {
        if let Ok(on_current) = vdm.IsWindowOnCurrentVirtualDesktop(hwnd) {
            if !on_current.as_bool() {
                return None;
            }
        }
    }

    if GetWindowTextLengthW(hwnd) == 0 {
        return None;
    }

    let mut title_buf = [0u16; 512];
    let title_len = GetWindowTextW(hwnd, &mut title_buf).max(0) as usize;
    let title = String::from_utf16_lossy(&title_buf[..title_len.min(title_buf.len())]);
    if title.trim().is_empty() {
        return None;
    }

    let mut class_buf = [0u16; 256];
    let class_len = GetClassNameW(hwnd, &mut class_buf).max(0) as usize;
    let class = String::from_utf16_lossy(&class_buf[..class_len.min(class_buf.len())]);
    if SKIP_CLASSES.iter().any(|name| name.eq_ignore_ascii_case(&class)) {
        return None;
    }

    let mut cloaked = 0u32;
    let cloaked_result = DwmGetWindowAttribute(
        hwnd,
        DWMWA_CLOAKED,
        &mut cloaked as *mut u32 as *mut core::ffi::c_void,
        std::mem::size_of::<u32>() as u32,
    );
    if cloaked_result.is_ok() && cloaked != 0 {
        return None;
    }

    let exe = context
        .exe_cache
        .entry(pid)
        .or_insert_with(|| exe_for_pid(pid, &context.process_names))
        .clone();
    if exe.is_empty() {
        return None;
    }

    Some(WindowInfo {
        hwnd: hwnd.0 as isize as i64,
        title,
        exe,
        pid,
        minimized: IsIconic(hwnd).as_bool(),
        focused: hwnd == context.foreground,
    })
}

fn process_names() -> HashMap<u32, String> {
    let mut map = HashMap::new();

    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return map;
        };

        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        if Process32FirstW(snapshot, &mut entry as *mut PROCESSENTRY32W).is_ok() {
            loop {
                let end = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                map.insert(
                    entry.th32ProcessID,
                    String::from_utf16_lossy(&entry.szExeFile[..end]),
                );

                if Process32NextW(snapshot, &mut entry as *mut PROCESSENTRY32W).is_err() {
                    break;
                }
            }
        }

        let _ = CloseHandle(snapshot);
    }

    map
}

fn exe_for_pid(pid: u32, names: &HashMap<u32, String>) -> String {
    unsafe {
        if let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            let mut buffer = [0u16; 1024];
            let mut size = buffer.len() as u32;
            let ok = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                PWSTR(buffer.as_mut_ptr()),
                &mut size,
            )
            .is_ok();
            let _ = CloseHandle(handle);

            if ok && size > 0 {
                return String::from_utf16_lossy(&buffer[..size as usize]);
            }
        }
    }

    names.get(&pid).cloned().unwrap_or_default()
}

pub fn session_is_locked() -> bool {
    unsafe {
        let foreground = GetForegroundWindow();
        if foreground.is_invalid() {
            return true;
        }

        let mut pid = 0u32;
        GetWindowThreadProcessId(foreground, Some(&mut pid as *mut u32));
        if pid == 0 {
            return true;
        }

        if let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            let mut buffer = [0u16; 1024];
            let mut size = buffer.len() as u32;
            let ok = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                PWSTR(buffer.as_mut_ptr()),
                &mut size,
            )
            .is_ok();
            let _ = CloseHandle(handle);

            if ok && size > 0 {
                let exe = String::from_utf16_lossy(&buffer[..size as usize]).to_lowercase();
                return exe.ends_with("\\lockapp.exe") || exe.ends_with("\\logonui.exe");
            }
        }

        false
    }
}

pub fn focus(hwnd: HWND) -> bool {
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        if GetForegroundWindow() == hwnd {
            return true;
        }

        let foreground = GetForegroundWindow();
        let foreground_thread = GetWindowThreadProcessId(foreground, None);
        let target_thread = GetWindowThreadProcessId(hwnd, None);
        let current_thread = GetCurrentThreadId();
        let attach = foreground_thread != 0 && target_thread != 0;

        if attach {
            let _ = AttachThreadInput(current_thread, foreground_thread, true);
            let _ = AttachThreadInput(current_thread, target_thread, true);
        }

        let _ = SetForegroundWindow(hwnd);

        if GetForegroundWindow() != hwnd {
            // SetForegroundWindow can be denied when the dock is not the
            // foreground process (e.g. a preview brought forward on hover).
            // SwitchToThisWindow follows the Alt+Tab activation path.
            let _ = BringWindowToTop(hwnd);
            SwitchToThisWindow(hwnd, true);
        }

        if attach {
            let _ = AttachThreadInput(current_thread, foreground_thread, false);
            let _ = AttachThreadInput(current_thread, target_thread, false);
        }

        for _ in 0..10 {
            if GetForegroundWindow() == hwnd {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        false
    }
}

pub fn minimize(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_MINIMIZE);
    }
}

pub fn close(hwnd: HWND) {
    unsafe {
        let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}
