use serde::Serialize;
use windows::core::Interface;
use windows::Win32::Media::Audio::{
    eConsole, eRender, IAudioSessionControl, IAudioSessionControl2, IAudioSessionManager2,
    IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED,
};

use super::util::normalize_title;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioState {
    pub volume: f32,
    pub muted: bool,
}

fn needle_for(app: &str) -> &'static str {
    let lower = app.to_lowercase();
    if lower.contains("chrome") || lower.contains("youtube") {
        "chrome.exe"
    } else if lower.contains("edge") {
        "msedge.exe"
    } else if lower.contains("firefox") {
        "firefox.exe"
    } else if lower.contains("brave") {
        "brave.exe"
    } else if lower.contains("opera") {
        "opera.exe"
    } else if lower.contains("spotify") {
        "spotify.exe"
    } else if lower.contains("vlc") {
        "vlc.exe"
    } else if lower.contains("media player") {
        "wmplayer.exe"
    } else {
        ""
    }
}

unsafe fn session_manager() -> Option<IAudioSessionManager2> {
    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    let enumerator: IMMDeviceEnumerator =
        CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
    let device = enumerator
        .GetDefaultAudioEndpoint(eRender, eConsole)
        .ok()?;
    device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None).ok()
}

unsafe fn display_name(control: &IAudioSessionControl) -> String {
    let Ok(value) = control.GetDisplayName() else {
        return String::new();
    };
    let text = value.to_string().unwrap_or_default();
    CoTaskMemFree(Some(value.0 as *const _));
    text
}

fn label_matches(label: &str, wanted: &str) -> bool {
    let label = normalize_title(label);
    if label.is_empty() || wanted.is_empty() {
        return false;
    }
    label.contains(wanted) || wanted.contains(&label)
}

unsafe fn find_session(app: &str, title: &str) -> Option<ISimpleAudioVolume> {
    let manager = session_manager()?;
    let sessions = manager.GetSessionEnumerator().ok()?;
    let count = sessions.GetCount().ok()?;
    let needle = needle_for(app);
    let wanted = normalize_title(title);
    let mut fallback: Option<ISimpleAudioVolume> = None;

    for index in 0..count as i32 {
        let Ok(control) = sessions.GetSession(index) else {
            continue;
        };
        let Ok(control2) = control.cast::<IAudioSessionControl2>() else {
            continue;
        };
        // S_OK means the session belongs to system sounds.
        if control2.IsSystemSoundsSession().0 == 0 {
            continue;
        }

        let pid = control2.GetProcessId().unwrap_or(0);
        let exe = super::hotkey::process_path(pid)
            .unwrap_or_default()
            .to_lowercase();
        if !needle.is_empty() && !exe.ends_with(needle) {
            continue;
        }

        let Ok(volume) = control.cast::<ISimpleAudioVolume>() else {
            continue;
        };

        let name = display_name(&control);
        if !wanted.is_empty() && label_matches(&name, &wanted) {
            return Some(volume);
        }
        if fallback.is_none() {
            fallback = Some(volume);
        }
    }

    fallback
}

unsafe fn current_session() -> Option<ISimpleAudioVolume> {
    let media = super::media::status();
    let title = media.title.unwrap_or_default();
    let app = media.app.unwrap_or_default();
    find_session(&app, &title)
}

pub fn state() -> Option<AudioState> {
    let volume = unsafe { current_session() }?;
    let level = unsafe { volume.GetMasterVolume().ok()? };
    let muted = unsafe { volume.GetMute().ok()?.as_bool() };
    Some(AudioState {
        volume: level.clamp(0.0, 1.0),
        muted,
    })
}

pub fn set_volume(level: f32) -> Result<(), String> {
    let volume = unsafe { current_session() }.ok_or_else(|| "Sessão de áudio não encontrada".to_string())?;
    unsafe {
        volume.SetMasterVolume(level.clamp(0.0, 1.0), std::ptr::null())
    }
    .map_err(|error| error.to_string())
}

pub fn set_muted(muted: bool) -> Result<(), String> {
    let volume = unsafe { current_session() }.ok_or_else(|| "Sessão de áudio não encontrada".to_string())?;
    unsafe { volume.SetMute(muted, std::ptr::null()) }.map_err(|error| error.to_string())
}
