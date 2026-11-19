use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Dhruva")
}

pub fn local_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Dhruva")
}

pub fn settings_path() -> PathBuf {
    data_dir().join("settings.json")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub schema_version: u32,
    pub theme: String,
    pub icon_size: u32,
    pub icon_spacing: u32,
    pub dock_margin: u32,
    pub dock_padding_x: u32,
    pub dock_padding_y: u32,
    pub zoom_factor: f64,
    pub zoom_rise: u32,
    pub auto_hide: bool,
    pub hide_delay: u32,
    pub reveal_delay: u32,
    pub hide_taskbar: bool,
    pub show_trash: bool,
    pub pinned: Vec<String>,
    pub pinned_initialized: bool,
    pub dock_order: Vec<String>,
    pub folders: Vec<String>,
    pub dock_template: String,
    pub launcher_template: String,
    pub animations: bool,
    pub animation_speed: f64,
    pub zoom_smoothness: f64,
    pub dock_radius: u32,
    pub launcher_width: u32,
    pub launcher_top: u32,
    pub launcher_radius: u32,
    pub launcher_blur: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            theme: "auto".to_string(),
            icon_size: 40,
            icon_spacing: 6,
            dock_margin: 6,
            dock_padding_x: 10,
            dock_padding_y: 6,
            zoom_factor: 1.6,
            zoom_rise: 16,
            auto_hide: false,
            hide_delay: 800,
            reveal_delay: 150,
            hide_taskbar: true,
            show_trash: true,
            pinned: Vec::new(),
            pinned_initialized: false,
            dock_order: Vec::new(),
            folders: Vec::new(),
            dock_template: "custom".to_string(),
            launcher_template: "spotlight".to_string(),
            animations: true,
            animation_speed: 1.0,
            zoom_smoothness: 0.26,
            dock_radius: 14,
            launcher_width: 640,
            launcher_top: 12,
            launcher_radius: 20,
            launcher_blur: 36,
        }
    }
}

impl Settings {
    pub fn normalize(&mut self) {
        self.schema_version = 1;
        if !matches!(self.theme.as_str(), "auto" | "light" | "dark") {
            self.theme = "auto".to_string();
        }
        self.icon_size = self.icon_size.clamp(28, 80);
        self.icon_spacing = self.icon_spacing.clamp(0, 24);
        self.dock_margin = self.dock_margin.clamp(0, 64);
        self.dock_padding_x = self.dock_padding_x.clamp(0, 40);
        self.dock_padding_y = self.dock_padding_y.clamp(0, 40);
        self.zoom_factor = self.zoom_factor.clamp(1.0, 3.0);
        self.zoom_rise = self.zoom_rise.clamp(0, 60);
        self.hide_delay = self.hide_delay.clamp(100, 5000);
        self.reveal_delay = self.reveal_delay.clamp(0, 2000);

        let mut seen_folders = std::collections::HashSet::new();
        self.folders = std::mem::take(&mut self.folders)
            .into_iter()
            .map(|path| path.trim().to_string())
            .filter(|path| !path.is_empty())
            .filter(|path| seen_folders.insert(path.to_lowercase()))
            .collect();

        if !matches!(
            self.dock_template.as_str(),
            "custom" | "macos" | "windows11" | "cosmic" | "minimal"
        ) {
            self.dock_template = "custom".to_string();
        }
        if !matches!(
            self.launcher_template.as_str(),
            "custom" | "spotlight" | "cosmic" | "compact" | "minimal"
        ) {
            self.launcher_template = "spotlight".to_string();
        }
        self.animation_speed = self.animation_speed.clamp(0.4, 2.5);
        self.zoom_smoothness = self.zoom_smoothness.clamp(0.06, 0.5);
        self.dock_radius = self.dock_radius.clamp(0, 28);
        self.launcher_width = self.launcher_width.clamp(480, 1000);
        self.launcher_top = self.launcher_top.clamp(4, 30);
        self.launcher_radius = self.launcher_radius.clamp(0, 32);
        self.launcher_blur = self.launcher_blur.clamp(0, 60);
    }
}

pub fn load() -> Settings {
    let path = settings_path();
    let existed = path.exists();
    let mut settings = fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<Settings>(&text).ok())
        .unwrap_or_default();

    // Keep development builds from hiding the developer's taskbar by surprise.
    if !existed && cfg!(debug_assertions) {
        settings.hide_taskbar = false;
    }

    settings.normalize();
    settings
}

pub fn save(settings: &Settings) -> Result<(), String> {
    let dir = data_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let tmp = dir.join("settings.json.tmp");
    let body = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(&tmp, body).map_err(|e| e.to_string())?;
    fs::rename(&tmp, settings_path()).map_err(|e| e.to_string())?;
    Ok(())
}
