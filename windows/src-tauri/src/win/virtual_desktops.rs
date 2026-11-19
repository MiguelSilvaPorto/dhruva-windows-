use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use windows::core::GUID;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::IVirtualDesktopManager;
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

const MAIN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\VirtualDesktops";
const VIRTUAL_DESKTOP_MANAGER_CLSID: GUID =
    GUID::from_u128(0xAA509086_5CA9_4C25_8F95_589D3C07B48A);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualDesktopInfo {
    pub index: usize,
    pub name: String,
    pub current: bool,
}

pub fn list() -> Vec<VirtualDesktopInfo> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    let ids = hkcu
        .open_subkey(MAIN_KEY)
        .ok()
        .and_then(|key| key.get_raw_value("VirtualDesktopIDs").ok())
        .map(|value| value.bytes)
        .unwrap_or_default();

    let current = current_desktop(&hkcu);

    let mut desktops = Vec::new();
    for (index, chunk) in ids.chunks_exact(16).enumerate() {
        let name = desktop_name(&hkcu, chunk)
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| format!("Área de trabalho {}", index + 1));

        desktops.push(VirtualDesktopInfo {
            index,
            name,
            current: current.as_deref() == Some(chunk),
        });
    }

    if desktops.is_empty() {
        desktops.push(VirtualDesktopInfo {
            index: 0,
            name: "Área de trabalho 1".into(),
            current: true,
        });
    }

    desktops
}

fn current_desktop(hkcu: &RegKey) -> Option<Vec<u8>> {
    if let Some(key) = hkcu.open_subkey(MAIN_KEY).ok() {
        if let Ok(value) = key.get_raw_value("CurrentVirtualDesktop") {
            if value.bytes.len() == 16 {
                return Some(value.bytes);
            }
        }
    }

    let session = current_session_id();
    let path = format!(
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\SessionInfo\{session}\VirtualDesktops"
    );
    hkcu.open_subkey(path)
        .ok()
        .and_then(|key| key.get_raw_value("CurrentVirtualDesktop").ok())
        .filter(|value| value.bytes.len() == 16)
        .map(|value| value.bytes)
}

fn current_session_id() -> u32 {
    unsafe {
        use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
        use windows::Win32::System::Threading::GetCurrentProcessId;

        let mut session = 1u32;
        let _ = ProcessIdToSessionId(GetCurrentProcessId(), &mut session);
        session
    }
}

fn guid_from_bytes(bytes: &[u8]) -> Option<GUID> {
    if bytes.len() < 16 {
        return None;
    }

    let data1 = u32::from_le_bytes(bytes[0..4].try_into().ok()?);
    let data2 = u16::from_le_bytes(bytes[4..6].try_into().ok()?);
    let data3 = u16::from_le_bytes(bytes[6..8].try_into().ok()?);
    Some(GUID::from_values(
        data1,
        data2,
        data3,
        [
            bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14],
            bytes[15],
        ],
    ))
}

pub fn current_guid() -> Option<GUID> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    guid_from_bytes(&current_desktop(&hkcu)?)
}

/// Keeps the dock and the launcher on whichever virtual desktop is active.
/// Windows hides normal windows that belong to another desktop, so moving the
/// host windows is what makes the same dock appear on every workspace.
pub fn follow_desktops(app: AppHandle) {
    std::thread::spawn(move || unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let Ok(manager): Result<IVirtualDesktopManager, _> =
            CoCreateInstance(&VIRTUAL_DESKTOP_MANAGER_CLSID, None, CLSCTX_ALL)
        else {
            log::warn!("virtual desktop manager unavailable; dock will not follow workspaces");
            return;
        };

        let mut last: Option<GUID> = None;
        loop {
            std::thread::sleep(Duration::from_millis(180));

            let current = {
                let by_registry = current_guid();
                if by_registry.is_some() {
                    by_registry
                } else {
                    let foreground = GetForegroundWindow();
                    if foreground.is_invalid() {
                        None
                    } else {
                        manager.GetWindowDesktopId(foreground).ok()
                    }
                }
            };

            let Some(guid) = current else {
                continue;
            };
            if last.as_ref() == Some(&guid) {
                continue;
            }
            last = Some(guid);

            for (label, window) in app.webview_windows() {
                if label != "launcher" && label != "settings" && !label.starts_with("dock") {
                    continue;
                }
                let Ok(raw) = window.hwnd() else {
                    continue;
                };
                let hwnd = HWND(raw.0);
                let _ = manager.MoveWindowToDesktop(hwnd, &guid);
            }
        }
    });
}

fn desktop_name(hkcu: &RegKey, id: &[u8]) -> Option<String> {
    let guid = guid_string(id)?;
    let path = format!(r"{MAIN_KEY}\Desktops\{guid}");
    hkcu.open_subkey(path)
        .ok()
        .and_then(|key| key.get_value::<String, _>("Name").ok())
}

fn guid_string(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 16 {
        return None;
    }

    let data1 = u32::from_le_bytes(bytes[0..4].try_into().ok()?);
    let data2 = u16::from_le_bytes(bytes[4..6].try_into().ok()?);
    let data3 = u16::from_le_bytes(bytes[6..8].try_into().ok()?);

    Some(format!(
        "{{{data1:08X}-{data2:04X}-{data3:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    ))
}
