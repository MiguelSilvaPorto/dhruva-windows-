use std::path::Path;
use windows::core::{Interface, PCWSTR};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    STGM_READ,
};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink, SLGP_RAWPATH};

#[derive(Default)]
pub struct LnkInfo {
    pub target: String,
    pub args: String,
}

pub fn resolve(path: &Path) -> Option<LnkInfo> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let link: IShellLinkW =
            CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let persist: IPersistFile = link.cast().ok()?;

        let wide: Vec<u16> = path
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        persist.Load(PCWSTR(wide.as_ptr()), STGM_READ).ok()?;

        let mut target_buf = [0u16; 2048];
        let target = match link.GetPath(
            &mut target_buf,
            std::ptr::null_mut(),
            SLGP_RAWPATH.0 as u32,
        ) {
            Ok(()) => trim_nul(&target_buf),
            Err(_) => String::new(),
        };

        let mut args_buf = [0u16; 4096];
        let args = match link.GetArguments(&mut args_buf) {
            Ok(()) => trim_nul(&args_buf),
            Err(_) => String::new(),
        };

        Some(LnkInfo { target, args })
    }
}

fn trim_nul(buffer: &[u16]) -> String {
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}
