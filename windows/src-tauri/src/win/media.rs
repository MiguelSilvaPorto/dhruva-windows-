use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as SessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
};
use windows::Storage::Streams::DataReader;
use windows::Win32::System::Com::{
    CoInitializeEx, COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaStatus {
    pub has_session: bool,
    pub playing: bool,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub app: Option<String>,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub art_key: Option<String>,
    pub can_play_pause: bool,
    pub can_next: bool,
    pub can_previous: bool,
    pub can_seek: bool,
}

impl MediaStatus {
    fn empty() -> Self {
        Self {
            has_session: false,
            playing: false,
            title: None,
            artist: None,
            album: None,
            app: None,
            position_ms: 0,
            duration_ms: 0,
            art_key: None,
            can_play_pause: false,
            can_next: false,
            can_previous: false,
            can_seek: false,
        }
    }
}

fn init_com() {
    unsafe {
        // Some players expose themselves through WinRT only after COM is up.
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

fn manager() -> Option<SessionManager> {
    SessionManager::RequestAsync().ok()?.join().ok()
}

fn session() -> Option<Session> {
    let manager = manager()?;

    if let Ok(current) = manager.GetCurrentSession() {
        return Some(current);
    }

    let sessions = manager.GetSessions().ok()?;
    let count = sessions.Size().ok()?;
    for index in 0..count {
        if let Ok(session) = sessions.GetAt(index) {
            return Some(session);
        }
    }

    None
}

fn text(value: windows::core::Result<windows::core::HSTRING>) -> Option<String> {
    value
        .ok()
        .map(|value| value.to_string())
        .filter(|value| !value.trim().is_empty())
}

fn friendly_app(app_id: &str) -> String {
    let lower = app_id.to_lowercase();

    for (needle, name) in [
        ("spotify", "Spotify"),
        ("msedge", "Edge"),
        ("chrome", "Chrome"),
        ("firefox", "Firefox"),
        ("brave", "Brave"),
        ("opera", "Opera"),
        ("vlc", "VLC"),
        ("wmplayer", "Windows Media Player"),
        ("zunemusic", "Groove Música"),
        ("mediaplayer", "Mídia do Windows"),
        ("foobar", "foobar2000"),
        ("deezer", "Deezer"),
        ("tidal", "TIDAL"),
        ("amazonmusic", "Amazon Music"),
        ("youtubemusic", "YouTube Music"),
    ] {
        if lower.contains(needle) {
            return name.to_string();
        }
    }

    let base = app_id.split('!').next().unwrap_or(app_id);
    let base = base.split('_').next().unwrap_or(base);
    base.to_string()
}

fn ticks_to_ms(ticks: i64) -> u64 {
    if ticks <= 0 {
        0
    } else {
        (ticks / 10_000) as u64
    }
}

pub fn status() -> MediaStatus {
    init_com();
    let Some(session) = session() else {
        return MediaStatus::empty();
    };

    let media = session
        .TryGetMediaPropertiesAsync()
        .ok()
        .and_then(|operation| operation.join().ok());

    let title = media
        .as_ref()
        .and_then(|media| text(media.Title()));
    let artist = media
        .as_ref()
        .and_then(|media| text(media.Artist()).or_else(|| text(media.AlbumArtist())));
    let album = media
        .as_ref()
        .and_then(|media| text(media.AlbumTitle()));

    let app = session
        .SourceAppUserModelId()
        .ok()
        .map(|id| friendly_app(&id.to_string()));

    let playback = session.GetPlaybackInfo().ok();
    let playing = playback
        .as_ref()
        .and_then(|playback| playback.PlaybackStatus().ok())
        .map(|status| status == PlaybackStatus::Playing)
        .unwrap_or(false);

    let controls = playback
        .as_ref()
        .and_then(|playback| playback.Controls().ok());
    let can_play_pause = controls
        .as_ref()
        .map(|controls| {
            controls.IsPlayEnabled().unwrap_or(false)
                || controls.IsPauseEnabled().unwrap_or(false)
        })
        .unwrap_or(true);
    let can_next = controls
        .as_ref()
        .map(|controls| controls.IsNextEnabled().unwrap_or(false))
        .unwrap_or(false);
    let can_previous = controls
        .as_ref()
        .map(|controls| controls.IsPreviousEnabled().unwrap_or(false))
        .unwrap_or(false);
    let can_seek = controls
        .as_ref()
        .map(|controls| controls.IsPlaybackPositionEnabled().unwrap_or(false))
        .unwrap_or(false);

    let (position_ms, duration_ms) = session
        .GetTimelineProperties()
        .ok()
        .map(|timeline| {
            let start = timeline.StartTime().map(|time| time.Duration).unwrap_or(0);
            let end = timeline.EndTime().map(|time| time.Duration).unwrap_or(0);
            let position = timeline.Position().map(|time| time.Duration).unwrap_or(0);
            (ticks_to_ms(position), ticks_to_ms(end - start))
        })
        .unwrap_or((0, 0));

    let art_key = Some(format!(
        "{}|{}|{}|{}",
        app.as_deref().unwrap_or_default(),
        title.as_deref().unwrap_or_default(),
        artist.as_deref().unwrap_or_default(),
        album.as_deref().unwrap_or_default(),
    ));

    MediaStatus {
        has_session: true,
        playing,
        title,
        artist,
        album,
        app,
        position_ms,
        duration_ms,
        art_key,
        can_play_pause,
        can_next,
        can_previous,
        can_seek,
    }
}

fn thumbnail_bytes(session: &Session) -> Option<(String, Vec<u8>)> {
    let media = session
        .TryGetMediaPropertiesAsync()
        .ok()?
        .join()
        .ok()?;
    let thumbnail = media.Thumbnail().ok()?;
    let stream = thumbnail.OpenReadAsync().ok()?.join().ok()?;

    let size = stream.Size().ok()?;
    if size == 0 || size > 8_000_000 {
        return None;
    }

    let input = stream.GetInputStreamAt(0).ok()?;
    let reader = DataReader::CreateDataReader(&input).ok()?;
    let loaded = reader.LoadAsync(size as u32).ok()?.join().ok()?;
    if loaded == 0 {
        return None;
    }

    let mut bytes = vec![0u8; loaded as usize];
    reader.ReadBytes(&mut bytes).ok()?;

    let content_type = stream
        .ContentType()
        .ok()
        .map(|value| value.to_string())
        .filter(|value| value.starts_with("image/"))
        .unwrap_or_else(|| "image/png".to_string());

    Some((content_type, bytes))
}

fn art_inner() -> Option<String> {
    let session = session()?;
    let (content_type, bytes) = thumbnail_bytes(&session)?;
    Some(format!(
        "data:{content_type};base64,{}",
        STANDARD.encode(bytes)
    ))
}

static ART_BUSY: AtomicBool = AtomicBool::new(false);

/// Album art of the current session. Kept as a separate command so the
/// one-second status polling does not ship the image over IPC every time.
/// Some players block on the thumbnail stream, so this runs on its own STA
/// thread with a timeout; the panel simply falls back to a music note.
pub fn art() -> Option<String> {
    if ART_BUSY.swap(true, Ordering::SeqCst) {
        return None;
    }

    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let result = art_inner();
        ART_BUSY.store(false, Ordering::SeqCst);
        let _ = sender.send(result);
    });

    receiver
        .recv_timeout(Duration::from_secs(5))
        .ok()
        .flatten()
}

pub fn control(action: &str, position_ms: Option<u64>) -> Result<(), String> {
    init_com();
    let session = session().ok_or_else(|| "Nenhuma mídia tocando.".to_string())?;

    let result = match action {
        "playPause" => session.TryTogglePlayPauseAsync(),
        "next" => session.TrySkipNextAsync(),
        "previous" => session.TrySkipPreviousAsync(),
        "play" => session.TryPlayAsync(),
        "pause" => session.TryPauseAsync(),
        "seek" => session.TryChangePlaybackPositionAsync(
            position_ms.unwrap_or(0).saturating_mul(10_000) as i64,
        ),
        _ => return Err("Comando de mídia desconhecido.".to_string()),
    };

    let accepted = result
        .map_err(|error| error.message().to_string())?
        .join()
        .unwrap_or(false);

    if accepted {
        Ok(())
    } else {
        Err("O aplicativo de mídia não aceitou o comando.".to_string())
    }
}
