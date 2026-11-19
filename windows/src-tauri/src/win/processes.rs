use serde::Serialize;
use std::collections::HashMap;
use std::sync::OnceLock;
use windows::core::{GUID, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
    TH32CS_SNAPPROCESS,
};
use windows::Win32::System::ProcessStatus::{
    K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
    PROCESS_VM_READ,
};
use windows::Win32::UI::Shell::IVirtualDesktopManager;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindow, GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible, GWL_EXSTYLE, GW_OWNER, WS_EX_TOOLWINDOW,
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
pub struct BackgroundApp {
    pub key: String,
    pub name: String,
    pub exe: String,
    pub pid: u32,
    pub hwnd: Option<i64>,
    pub title: Option<String>,
    pub windows: u32,
    pub processes: u32,
    pub memory: u64,
    pub state: String,
}

#[derive(Debug, Clone)]
struct WindowEntry {
    hwnd: isize,
    title: String,
    visible: bool,
    minimized: bool,
}

struct WindowScan {
    windows: HashMap<u32, Vec<WindowEntry>>,
    vdm: Option<IVirtualDesktopManager>,
}

struct Group {
    name: String,
    exe: String,
    pids: Vec<u32>,
    memory: u64,
    windows: Vec<WindowEntry>,
    active: bool,
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

const BLOCKED: &[&str] = &[
    "applicationframehost",
    "backgroundtaskhost",
    "conhost",
    "csrss",
    "ctfmon",
    "dllhost",
    "dwm",
    "explorer",
    "fontdrvhost",
    "lsass",
    "msedgewebview2",
    "openconsole",
    "registry",
    "runtimebroker",
    "searchhost",
    "searchprotocolhost",
    "securityhealthsystray",
    "services",
    "shellexperiencehost",
    "sihost",
    "smartscreen",
    "smss",
    "spoolsv",
    "startmenuexperiencehost",
    "svchost",
    "system",
    "taskhostw",
    "textinputhost",
    "wininit",
    "winlogon",
    "wmiprvse",
    "wslhost",
    "wslservice",
    "crashpad_handler",
    "werfault",
    "googleupdate",
    "microsoftedgeupdate",
    "onedrivestandaloneupdater",
    "adobearm",
    "jusched",
    "browse",
    "node",
    "python",
    "pythonw",
    "powershell",
    "pwsh",
    "cmd",
    "wscript",
    "cscript",
    "mshta",
    "rundll32",
    "crashhelper",
    "crashpad",
    "copilot-runtime",
    "vctip",
    "filecoauth",
    "olkbg",
    "musnotifyicon",
    "rtkauduservice64",
    "wavessvc64",
    "igfxem",
    "avp",
    "avpui",
    "geminiapplauncher",
    "geminicrashpadhandler",
    "onedrive.sync.service",
    "setup_wm",
    "screensketch",
    "systemsettings",
    "notas-ti",
    "portal-host",
    "opencode-pty",
    "git",
    "git-remote-https",
    "git-lfs",
    "ssh",
    "scp",
    "sftp",
    "bash",
    "sh",
    "mintty",
    "curl",
    "wget",
    "puttyng",
    "docker",
    "docker-compose",
    "kubectl",
    "helm",
];

fn wide_string(buffer: &[u16]) -> String {
    let end = buffer
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
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
        Some(String::from_utf16_lossy(&buffer[..size as usize]))
    }
}

fn process_memory(pid: u32) -> u64 {
    unsafe {
        let Ok(handle) = OpenProcess(
            PROCESS_QUERY_INFORMATION | PROCESS_VM_READ,
            false,
            pid,
        ) else {
            return 0;
        };

        let mut counters = PROCESS_MEMORY_COUNTERS::default();
        let ok = K32GetProcessMemoryInfo(
            handle,
            &mut counters,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )
        .as_bool();
        let _ = CloseHandle(handle);

        if ok {
            counters.WorkingSetSize as u64
        } else {
            0
        }
    }
}

fn process_session(pid: u32) -> Option<u32> {
    unsafe {
        let mut session = 0u32;
        ProcessIdToSessionId(pid, &mut session).ok()?;
        Some(session)
    }
}

unsafe extern "system" fn enum_window(hwnd: HWND, lparam: LPARAM) -> windows::core::BOOL {
    let scan = &mut *(lparam.0 as *mut WindowScan);

    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == 0 || pid == GetCurrentProcessId() {
        return windows::core::BOOL(1);
    }

    if let Ok(owner) = GetWindow(hwnd, GW_OWNER) {
        if !owner.is_invalid() {
            return windows::core::BOOL(1);
        }
    }

    let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
    if ex_style & WS_EX_TOOLWINDOW.0 != 0 {
        return windows::core::BOOL(1);
    }

    if let Some(vdm) = &scan.vdm {
        if let Ok(on_current) = vdm.IsWindowOnCurrentVirtualDesktop(hwnd) {
            if !on_current.as_bool() {
                return windows::core::BOOL(1);
            }
        }
    }

    if GetWindowTextLengthW(hwnd) == 0 {
        return windows::core::BOOL(1);
    }

    let mut title_buffer = [0u16; 512];
    let title_len = GetWindowTextW(hwnd, &mut title_buffer).max(0) as usize;
    if title_len == 0 {
        return windows::core::BOOL(1);
    }
    let title = String::from_utf16_lossy(&title_buffer[..title_len.min(title_buffer.len())]);

    let mut class_buffer = [0u16; 256];
    let class_len = GetClassNameW(hwnd, &mut class_buffer).max(0) as usize;
    let class = String::from_utf16_lossy(&class_buffer[..class_len.min(class_buffer.len())]);
    if SKIP_CLASSES
        .iter()
        .any(|name| name.eq_ignore_ascii_case(&class))
    {
        return windows::core::BOOL(1);
    }

    scan.windows.entry(pid).or_default().push(WindowEntry {
        hwnd: hwnd.0 as isize,
        title,
        visible: IsWindowVisible(hwnd).as_bool(),
        minimized: IsIconic(hwnd).as_bool(),
    });

    windows::core::BOOL(1)
}

fn windows_by_pid() -> HashMap<u32, Vec<WindowEntry>> {
    let mut scan = WindowScan {
        windows: HashMap::new(),
        vdm: virtual_desktop_manager(),
    };
    unsafe {
        let _ = EnumWindows(
            Some(enum_window),
            LPARAM(&mut scan as *mut WindowScan as isize),
        );
    }
    scan.windows
}

fn is_system_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.contains("\\windows\\system32\\")
        || lower.contains("\\windows\\syswow64\\")
        || lower.contains("\\windows\\winsxs\\")
        || lower.contains("\\windows\\systemapps\\")
        || lower.contains("\\windows\\systemresources\\")
}

fn display_name(stem: &str) -> String {
    let name = match stem.to_lowercase().as_str() {
        "taskmgr" => "Gerenciador de Tarefas",
        "notepad" => "Bloco de Notas",
        "calc" => "Calculadora",
        "ms-teams" => "Microsoft Teams",
        "code" => "Visual Studio Code",
        "wmplayer" | "setup_wm" => "Windows Media Player",
        "mremoteng" => "mRemoteNG",
        "saplogon" => "SAP Logon",
        "thunderbird" => "Thunderbird",
        "pdfcraft" => "PdfCraft",
        "veracrypt" => "VeraCrypt",
        "anydesk" => "AnyDesk",
        "teamviewer" => "TeamViewer",
        "onedrive" => "OneDrive",
        "steam" => "Steam",
        "spotify" => "Spotify",
        "discord" => "Discord",
        "chrome" => "Google Chrome",
        "msedge" => "Microsoft Edge",
        "firefox" => "Firefox",
        "opencode" => "OpenCode",
        "claude" => "Claude",
        "microsoft.media.player" => "Media Player",
        "rustdesk" => "RustDesk",
        "handy" => "Handy",
        _ => stem,
    };
    name.to_string()
}

fn ignored_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.contains("\\appdata\\local\\temp\\")
        || lower.contains("\\node_modules\\")
        || lower.contains("\\resources\\app\\")
        || lower.contains("\\driverstore\\")
        || lower.contains("\\windowspowershell\\")
}

fn best_window(windows: &[WindowEntry]) -> Option<&WindowEntry> {
    windows
        .iter()
        .find(|window| window.visible && window.minimized)
        .or_else(|| windows.iter().find(|window| window.visible))
        .or_else(|| windows.first())
}

pub fn list() -> Vec<BackgroundApp> {
    let own_pid = unsafe { GetCurrentProcessId() };
    let own_session = process_session(own_pid).unwrap_or(0);
    let windows = windows_by_pid();
    let mut groups: HashMap<String, Group> = HashMap::new();

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    let Ok(snapshot) = snapshot else {
        return Vec::new();
    };

    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };

    if unsafe { Process32FirstW(snapshot, &mut entry) }.is_ok() {
        loop {
            let pid = entry.th32ProcessID;
            if pid == 0 || pid == own_pid {
                // Next.
            } else if process_session(pid) != Some(own_session) {
                // Service or another session.
            } else if let Some(path) = process_path(pid) {
                let exe = path.to_lowercase();
                let stem = path
                    .rsplit('\\')
                    .next()
                    .unwrap_or(&path)
                    .trim_end_matches(".exe")
                    .trim_end_matches(".EXE")
                    .to_string();
                let blocked = BLOCKED
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(&stem));

                if !blocked && !ignored_path(&path) {
                    let app_windows = windows.get(&pid).cloned().unwrap_or_default();
                    let active = app_windows
                        .iter()
                        .any(|window| window.visible && !window.minimized);
                    let has_window = !app_windows.is_empty();

                    if has_window || !is_system_path(&path) {
                        let group = groups.entry(exe.clone()).or_insert_with(|| Group {
                            name: display_name(&stem),
                            exe: path.clone(),
                            pids: Vec::new(),
                            memory: 0,
                            windows: Vec::new(),
                            active: false,
                        });

                        group.pids.push(pid);
                        group.memory += process_memory(pid);
                        group.windows.extend(app_windows);
                        group.active |= active;
                    }
                }
            }

            if unsafe { Process32NextW(snapshot, &mut entry) }.is_err() {
                break;
            }
        }
    }

    unsafe {
        let _ = CloseHandle(snapshot);
    }

    let mut apps: Vec<BackgroundApp> = groups
        .into_iter()
        .filter(|(_, group)| !group.active && (!group.pids.is_empty()))
        .map(|(key, group)| {
            let best = best_window(&group.windows);
            let state = if group.windows.is_empty() {
                "no-window"
            } else if group.windows.iter().any(|window| window.minimized) {
                "minimized"
            } else {
                "hidden"
            };

            BackgroundApp {
                key,
                name: group.name,
                exe: group.exe,
                pid: group.pids.first().copied().unwrap_or(0),
                hwnd: best.map(|window| window.hwnd as i64),
                title: best.map(|window| window.title.clone()),
                windows: group.windows.len() as u32,
                processes: group.pids.len() as u32,
                memory: group.memory,
                state: state.to_string(),
            }
        })
        .collect();

    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps.truncate(60);
    apps
}

pub fn activate(hwnd: i64) {
    let hwnd = crate::win::hwnd_from_i64(hwnd);
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                hwnd,
                windows::Win32::UI::WindowsAndMessaging::SW_SHOW,
            );
        }
        if IsIconic(hwnd).as_bool() {
            let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                hwnd,
                windows::Win32::UI::WindowsAndMessaging::SW_RESTORE,
            );
        }
    }
    crate::win::windows::focus(hwnd);
}

pub fn terminate(pid: u32) -> Result<(), String> {
    if pid == 0 {
        return Err("Processo inválido.".to_string());
    }

    unsafe {
        let handle: HANDLE = OpenProcess(PROCESS_TERMINATE, false, pid)
            .map_err(|error| error.message().to_string())?;
        let result =
            windows::Win32::System::Threading::TerminateProcess(handle, 1);
        let _ = CloseHandle(handle);
        result.map_err(|error| error.message().to_string())
    }
}
