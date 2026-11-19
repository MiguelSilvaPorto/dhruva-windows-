use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::LPARAM;
use windows::Win32::UI::Shell::{
    SHQueryRecycleBinW, SHEmptyRecycleBinW, SHQUERYRBINFO, SHERB_NOPROGRESSUI, ShellExecuteW,
    BIF_EDITBOX, BIF_NEWDIALOGSTYLE, BIF_RETURNONLYFSDIRS, BROWSEINFOW, ILFree, SHBrowseForFolderW,
    SHGetPathFromIDListW,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use super::util::to_wide;

pub fn execute(target: &str) -> Result<(), String> {
    let verb = to_wide("open");
    let file = to_wide(target);
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };

    if result.0 as isize <= 32 {
        Err(format!("nao foi possivel abrir: {target}"))
    } else {
        Ok(())
    }
}

pub fn open_folder(path: &str, new_window: bool) -> Result<(), String> {
    if !std::path::Path::new(path).is_dir() {
        return Err(format!("pasta nao encontrada: {path}"));
    }

    if new_window {
        std::process::Command::new("explorer.exe")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    } else {
        execute(path)
    }
}

pub fn pick_folder() -> Option<String> {
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
        );

        let title = to_wide("Adicionar pasta à dock");
        let mut display = [0u16; 260];
        let owner = windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow();
        let info = BROWSEINFOW {
            hwndOwner: owner,
            pidlRoot: std::ptr::null_mut(),
            pszDisplayName: PWSTR(display.as_mut_ptr()),
            lpszTitle: PCWSTR(title.as_ptr()),
            ulFlags: BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE | BIF_EDITBOX,
            lpfn: None,
            lParam: LPARAM(0),
            iImage: 0,
        };

        let pidl = SHBrowseForFolderW(&info);
        if pidl.is_null() {
            return None;
        }

        let mut path = [0u16; 260];
        let ok = SHGetPathFromIDListW(pidl, &mut path).as_bool();
        ILFree(Some(pidl as *const _));

        if !ok {
            return None;
        }

        let len = path.iter().position(|value| *value == 0).unwrap_or(path.len());
        Some(String::from_utf16_lossy(&path[..len]))
    }
}

pub fn recycle_bin_full() -> bool {
    unsafe {
        let root = to_wide("C:\\");
        let mut info = SHQUERYRBINFO {
            cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32,
            i64Size: 0,
            i64NumItems: 0,
        };
        let result = SHQueryRecycleBinW(PCWSTR(root.as_ptr()), &mut info);
        result.is_ok() && info.i64NumItems > 0
    }
}

pub fn open_recycle_bin() -> Result<(), String> {
    execute("shell:RecycleBinFolder")
}

pub fn empty_recycle_bin() -> Result<(), String> {
    unsafe {
        let _ = SHEmptyRecycleBinW(None, PCWSTR::null(), SHERB_NOPROGRESSUI);
    }
    Ok(())
}
