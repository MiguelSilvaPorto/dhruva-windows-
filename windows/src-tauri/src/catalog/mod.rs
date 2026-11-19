mod lnk;
pub mod icons;

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub use icons as app_icons;
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppEntry {
    pub id: String,
    pub name: String,
    pub path: String,
    pub target: String,
    pub args: String,
    pub aumid: Option<String>,
    pub icon_source: String,
    pub builtin: bool,
}

pub fn scan_apps() -> Vec<AppEntry> {
    let mut apps: Vec<AppEntry> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for dir in start_menu_dirs() {
        collect_links(&dir, 0, &mut apps, &mut seen);
    }

    add_builtin_fallbacks(&mut apps);
    dedupe_builtins(&mut apps);
    scan_installed_apps(&mut apps);

    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps
}

fn start_menu_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(program_data) = std::env::var_os("ProgramData") {
        dirs.push(
            PathBuf::from(program_data)
                .join("Microsoft\\Windows\\Start Menu\\Programs"),
        );
    }
    if let Some(app_data) = std::env::var_os("APPDATA") {
        dirs.push(
            PathBuf::from(app_data)
                .join("Microsoft\\Windows\\Start Menu\\Programs"),
        );
    }
    dirs
}

fn collect_links(dir: &Path, depth: u32, apps: &mut Vec<AppEntry>, seen: &mut HashSet<String>) {
    if depth > 6 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        if file_type.is_dir() {
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.eq_ignore_ascii_case("Startup"))
                .unwrap_or(false)
            {
                continue;
            }
            collect_links(&path, depth + 1, apps, seen);
            continue;
        }

        let ext = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default()
            .to_lowercase();
        if ext != "lnk" {
            continue;
        }

        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
            .trim()
            .replace(" - Atalho", "")
            .replace(" - Shortcut", "");
        if name.is_empty() || is_junk_name(&name) {
            continue;
        }

        let info = lnk::resolve(&path).unwrap_or_default();
        let aumid = extract_aumid(&info.args, &info.target);
        let target = info.target.trim().to_string();

        if aumid.is_none() && target.is_empty() {
            // A shortcut with no target and no app id is usually broken.
            continue;
        }

        // Folder/documentation shortcuts that merely open Explorer are not apps.
        let lower_name = name.to_lowercase();
        let target_lower = normalize_path(&target);
        if target_lower.ends_with("\\explorer.exe")
            && !lower_name.contains("explorer")
            && !lower_name.contains("gerenciador")
        {
            continue;
        }

        let key = if let Some(ref id) = aumid {
            format!("aumid:{}", id.to_lowercase())
        } else {
            format!("target:{}", normalize_path(&target))
        };
        if !seen.insert(key) {
            continue;
        }

        apps.push(AppEntry {
            id: app_id(&name, &target, aumid.as_deref()),
            name,
            path: path.to_string_lossy().to_string(),
            target,
            args: info.args,
            aumid,
            icon_source: path.to_string_lossy().to_string(),
            builtin: false,
        });
    }
}

fn add_builtin_fallbacks(apps: &mut Vec<AppEntry>) {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".to_string());
    let local = std::env::var("LOCALAPPDATA").unwrap_or_default();

    let settings = PathBuf::from(&windir)
        .join("ImmersiveControlPanel")
        .join("SystemSettings.exe");
    let notepad = PathBuf::from(&windir).join("System32").join("notepad.exe");
    let calc = PathBuf::from(&windir).join("System32").join("calc.exe");
    let cmd = PathBuf::from(&windir).join("System32").join("cmd.exe");
    let wt = PathBuf::from(&local)
        .join("Microsoft\\WindowsApps")
        .join("wt.exe");

    ensure_explorer(apps);

    if wt.exists() {
        // wt.exe is only an execution alias and carries the generic Windows
        // application icon. The packaged executable has the real Terminal
        // icon, which the shell can extract at 128 px.
        let terminal_exe = terminal_package_exe();
        let icon = terminal_exe.clone().unwrap_or_else(|| wt.clone());
        let before = apps.len();

        push_builtin(
            apps,
            "Terminal",
            &wt,
            &icon,
            &["wt.exe", "windowsterminal"],
            &[],
        );

        if apps.len() > before {
            if let Some(exe) = terminal_exe {
                if let Some(app) = apps.last_mut() {
                    // Matches the real process and gives the icon extractor
                    // the executable that owns the artwork.
                    app.target = exe.to_string_lossy().to_string();
                }
            }
        }
    } else {
        push_builtin(apps, "Prompt de Comando", &cmd, &cmd, &["cmd.exe"], &[]);
    }

    push_builtin(
        apps,
        "Configurações",
        Path::new("ms-settings:"),
        &settings,
        &["systemsettings.exe", "immersivecontrolpanel"],
        &["windows.immersivecontrolpanel"],
    );

    push_builtin(
        apps,
        "Bloco de Notas",
        &notepad,
        &notepad,
        &["notepad.exe"],
        &[],
    );

    if calc.exists() {
        push_builtin(apps, "Calculadora", &calc, &calc, &["calc.exe"], &["windowscalculator"]);
    }
}

fn ensure_explorer(apps: &mut Vec<AppEntry>) {
    let already_there = apps.iter().any(|app| {
        let name = app.name.to_lowercase();
        name == "explorer"
            || name == "file explorer"
            || name == "windows explorer"
            || name.contains("gerenciador de arquivos")
    });
    if already_there {
        return;
    }

    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".to_string());
    let explorer = PathBuf::from(windir).join("explorer.exe");
    let path = explorer.to_string_lossy().to_string();

    apps.push(AppEntry {
        id: "builtin:explorer".to_string(),
        name: "Explorer".to_string(),
        path: path.clone(),
        target: path.clone(),
        args: String::new(),
        aumid: None,
        icon_source: path,
        builtin: true,
    });
}

fn push_builtin(
    apps: &mut Vec<AppEntry>,
    name: &str,
    launch_path: &Path,
    icon_source: &Path,
    target_fragments: &[&str],
    aumid_fragments: &[&str],
) {
    let exists = apps.iter().any(|app| {
        let hay = format!(
            "{} {} {}",
            app.target,
            app.aumid.as_deref().unwrap_or_default(),
            app.name
        )
        .to_lowercase();
        target_fragments.iter().any(|fragment| hay.contains(fragment))
            || aumid_fragments.iter().any(|fragment| hay.contains(fragment))
    });
    if exists {
        return;
    }

    let launch = launch_path.to_string_lossy().to_string();
    let icon = icon_source.to_string_lossy().to_string();
    let id = format!("builtin:{}", slug(name));

    apps.push(AppEntry {
        id,
        name: name.to_string(),
        path: launch.clone(),
        target: if launch_path.is_absolute() {
            launch.clone()
        } else {
            icon.clone()
        },
        args: String::new(),
        aumid: None,
        icon_source: icon,
        builtin: true,
    });
}

fn dedupe_builtins(apps: &mut Vec<AppEntry>) {
    let mut seen = HashSet::new();
    apps.retain(|app| {
        let key = if let Some(aumid) = &app.aumid {
            format!("aumid:{}", aumid.to_lowercase())
        } else if !app.target.is_empty() {
            format!("target:{}", normalize_path(&app.target))
        } else {
            format!("path:{}", normalize_path(&app.path))
        };
        seen.insert(key)
    });
}

pub fn default_pinned(apps: &[AppEntry]) -> Vec<String> {
    let mut pinned: Vec<String> = Vec::new();

    let mut add = |candidate: Option<&AppEntry>| {
        if let Some(app) = candidate {
            if !pinned.iter().any(|id| id == &app.id) {
                pinned.push(app.id.clone());
            }
        }
    };

    add(find_app_by_name(
        apps,
        &["explorer", "file explorer", "windows explorer", "gerenciador de arquivos"],
    ));
    add(default_browser_hint()
        .and_then(|hint| find_app(apps, &[hint.as_str()]))
        .or_else(|| find_app(apps, &["msedge", "chrome", "firefox", "brave"])));
    add(
        find_app_by_name(apps, &["terminal"])
            .or_else(|| find_app(apps, &["windowsterminal", "wt.exe", "cmd.exe"])),
    );
    add(find_app(
        apps,
        &["systemsettings.exe", "immersivecontrolpanel", "configuracoes", "configurações"],
    ));
    add(find_app(apps, &["notepad.exe", "bloco de notas"]));
    add(find_app(apps, &["windowscalculator", "calc.exe", "calculadora"]));
    add(find_app(apps, &["windowsstore", "microsoft store", "loja"]));
    add(find_app(apps, &["windows.photos", "fotos"]));

    if pinned.len() < 5 {
        for app in apps {
            if pinned.len() >= 6 {
                break;
            }
            if app.name.len() > 40 || app.target.is_empty() {
                continue;
            }
            if !pinned.iter().any(|id| id == &app.id) {
                pinned.push(app.id.clone());
            }
        }
    }

    pinned
}

fn find_app<'a>(apps: &'a [AppEntry], needles: &[&str]) -> Option<&'a AppEntry> {
    apps.iter().find(|app| {
        let hay = format!(
            "{} {} {}",
            app.name,
            app.target,
            app.aumid.as_deref().unwrap_or_default()
        )
        .to_lowercase();
        needles.iter().any(|needle| hay.contains(&needle.to_lowercase()))
    })
}

/// Path to the packaged Windows Terminal executable. The Start Menu only
/// exposes the wt.exe execution alias, whose icon is the generic one.
fn terminal_package_exe() -> Option<PathBuf> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let packages = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(
            "Software\\Classes\\Local Settings\\Software\\Microsoft\\Windows\\CurrentVersion\\AppModel\\Repository\\Packages",
        )
        .ok()?;

    for name in packages.enum_keys().flatten() {
        if !name
            .to_lowercase()
            .starts_with("microsoft.windowsterminal_")
        {
            continue;
        }

        let Ok(package) = packages.open_subkey(&name) else {
            continue;
        };
        let Ok(root) = package.get_value::<String, _>("PackageRootFolder") else {
            continue;
        };

        let exe = PathBuf::from(root).join("WindowsTerminal.exe");
        if exe.is_file() {
            return Some(exe);
        }
    }

    None
}

fn find_app_by_name<'a>(apps: &'a [AppEntry], needles: &[&str]) -> Option<&'a AppEntry> {
    apps.iter().find(|app| {
        let name = app.name.to_lowercase();
        needles.iter().any(|needle| name.contains(&needle.to_lowercase()))
    })
}

fn default_browser_hint() -> Option<String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(
            "Software\\Microsoft\\Windows\\Shell\\Associations\\UrlAssociations\\http\\UserChoice",
        )
        .ok()?;
    let prog_id: String = key.get_value("ProgId").ok()?;
    let prog_id = prog_id.to_lowercase();

    if prog_id.contains("edge") {
        Some("msedge".to_string())
    } else if prog_id.contains("chrome") {
        Some("chrome".to_string())
    } else if prog_id.contains("firefox") {
        Some("firefox".to_string())
    } else if prog_id.contains("brave") {
        Some("brave".to_string())
    } else if prog_id.contains("opera") {
        Some("opera".to_string())
    } else {
        Some(prog_id)
    }
}

fn extract_aumid(args: &str, target: &str) -> Option<String> {
    let haystack = format!("{} {}", args, target);
    let lower = haystack.to_lowercase();
    let marker = "shell:appsfolder";
    let index = lower.find(marker)?;
    let rest = &haystack[index + marker.len()..];
    let rest = rest.trim_start_matches(['\\', '/', ' ']);
    let aumid = rest.split_whitespace().next().unwrap_or_default().trim();
    if aumid.is_empty() {
        None
    } else {
        Some(aumid.to_string())
    }
}

fn app_id(name: &str, target: &str, aumid: Option<&str>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(name.to_lowercase().as_bytes());
    hasher.update(b"|");
    hasher.update(normalize_path(target).as_bytes());
    hasher.update(b"|");
    hasher.update(aumid.unwrap_or_default().to_lowercase().as_bytes());
    let digest = hasher.finalize();
    format!("app:{}", hex(&digest[..8]))
}

pub fn normalize_path(path: &str) -> String {
    path.trim()
        .replace('/', "\\")
        .trim_matches('"')
        .to_lowercase()
}

pub fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn is_junk_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    const JUNK: &[&str] = &[
        "uninstall",
        "desinstalar",
        "remove",
        "remove ",
        "readme",
        "leia-me",
        "changelog",
        "release notes",
        "license",
        "licenca",
        "licença",
        "website",
        "site da",
        "help",
        "ajuda",
        "manual",
        "documentation",
        "atualizacao",
        "atualização",
        "updater",
        "update ",
    ];
    JUNK.iter().any(|fragment| lower.contains(fragment))
}

const IGNORED_INSTALL_DIRS: &[&str] = &[
    "commonfiles",
    "windows",
    "windowsdefender",
    "windowsmediaplayer",
    "windowsphotoviewer",
    "internetexplorer",
    "applicationverifier",
    "installshield",
    "microsoft",
    "microsoftoffice",
    "microsoftvisualstudio",
    "microsoftedge",
    "microsoftteams",
    "mozillamaintenanceservice",
    "uninstall",
    "temp",
    "update",
    "backup",
    "cache",
    "resources",
    "locales",
    "swiftshader",
    "plugins",
    "drivers",
    "windowsapps",
    "windowsnt",
    "referenceassemblies",
    "dotnet",
    "nodejs",
    "python",
    "git",
    "docker",
    "kaspersky",
    "realtek",
    "intel",
    "nvidia",
    "amd",
    "waves",
    "maxxaudio",
    "sap",
    "teamviewer",
    "veracrypt",
    "defraggler",
    "crystaldiskinfo",
    "windowskits",
    "windowsappcertkit",
    "samples",
    "tools",
    "ruxim",
    "applicationverifier",
    "microsoftvisualstudiocode",
];

fn alpha_norm(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(|character| character.to_lowercase())
        .collect()
}

fn clean_folder_name(value: &str) -> String {
    let cleaned = value
        .replace(" (x86)", "")
        .replace(" (x64)", "")
        .replace(" (Arm64)", "")
        .replace(" (64-bit)", "")
        .replace(" (32-bit)", "")
        .replace("Software", " ")
        .replace("Technologies", " ")
        .replace("Corporation", " ");
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_ignored_install_dir(normalized: &str) -> bool {
    IGNORED_INSTALL_DIRS
        .iter()
        .any(|ignored| normalized == *ignored || normalized.starts_with(ignored))
}

fn is_ignored_exe(normalized: &str) -> bool {
    const JUNK: &[&str] = &[
        "unins",
        "uninstall",
        "update",
        "setup",
        "install",
        "vc_redist",
        "vcredist",
        "dxsetup",
        "crash",
        "report",
        "helper",
        "elevate",
        "repair",
    ];
    JUNK.iter().any(|fragment| normalized.contains(fragment))
}

fn common_prefix(left: &str, right: &str) -> usize {
    left.chars()
        .zip(right.chars())
        .take_while(|(a, b)| a == b)
        .count()
}

fn find_matching_exe(
    dir: &Path,
    folder_norm: &str,
    depth: u32,
    budget: &mut u32,
) -> Option<(u8, PathBuf)> {
    if depth > 2 || *budget == 0 {
        return None;
    }

    let entries = fs::read_dir(dir).ok()?;
    let mut best: Option<(u8, PathBuf)> = None;

    for entry in entries.flatten() {
        if *budget == 0 {
            break;
        }
        *budget = budget.saturating_sub(1);

        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        if file_type.is_file() {
            let extension = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_lowercase();
            if extension != "exe" {
                continue;
            }

            let stem = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            let stem_norm = alpha_norm(stem);
            if stem_norm.len() < 3 || is_ignored_exe(&stem_norm) {
                continue;
            }

            let score = if stem_norm == folder_norm {
                Some(0u8)
            } else if stem_norm.contains(folder_norm) || folder_norm.contains(&stem_norm) {
                Some(1u8)
            } else if common_prefix(&stem_norm, folder_norm) >= 3 {
                Some(2u8)
            } else {
                None
            };

            if let Some(score) = score {
                if best
                    .as_ref()
                    .map(|(current, _)| score < *current)
                    .unwrap_or(true)
                {
                    best = Some((score, path));
                }
            }
        } else if file_type.is_dir() && depth < 2 {
            let child = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            let child_norm = alpha_norm(child);
            if child_norm.is_empty()
                || child_norm == "resources"
                || child_norm == "locales"
                || child_norm == "plugins"
                || child_norm == "swiftshader"
                || child_norm == "unins"
            {
                continue;
            }

            if let Some((score, found)) =
                find_matching_exe(&path, folder_norm, depth + 1, budget)
            {
                let score = (score + 1).min(3);
                if best
                    .as_ref()
                    .map(|(current, _)| score < *current)
                    .unwrap_or(true)
                {
                    best = Some((score, found));
                }
            }
        }
    }

    best
}

/// Installed programs that never registered a Start Menu shortcut (portable
/// apps, per-user installs) still show up in the launcher. The folder must
/// contain an executable whose name matches the folder, which keeps tools
/// like updaters and uninstallers out of the list.
fn scan_installed_apps(apps: &mut Vec<AppEntry>) {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(program_files) = std::env::var_os("ProgramFiles") {
        roots.push(PathBuf::from(program_files));
    }
    if let Some(program_files_x86) = std::env::var_os("ProgramFiles(x86)") {
        roots.push(PathBuf::from(program_files_x86));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(local).join("Programs"));
    }

    let mut existing_targets: HashSet<String> = apps
        .iter()
        .flat_map(|app| [&app.target, &app.path, &app.icon_source])
        .map(|value| normalize_path(value))
        .filter(|value| !value.is_empty())
        .collect();
    let mut existing_names: HashSet<String> = apps
        .iter()
        .map(|app| alpha_norm(&app.name))
        .filter(|value| !value.is_empty())
        .collect();

    let mut added = 0usize;
    for root in roots {
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };

        for entry in entries.flatten().take(180) {
            if added >= 80 {
                break;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_dir() {
                continue;
            }

            let dir = entry.path();
            let folder_name = dir
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            let folder_norm = alpha_norm(folder_name);
            if folder_norm.len() < 3 || is_ignored_install_dir(&folder_norm) {
                continue;
            }

            let name = clean_folder_name(folder_name);
            let name_norm = alpha_norm(&name);
            if name.len() < 2 || existing_names.contains(&name_norm) {
                continue;
            }

            let mut budget = 450u32;
            let Some((_, exe)) = find_matching_exe(&dir, &folder_norm, 0, &mut budget)
            else {
                continue;
            };

            let exe_path = exe.to_string_lossy().to_string();
            let target_norm = normalize_path(&exe_path);
            if existing_targets.contains(&target_norm) {
                continue;
            }

            apps.push(AppEntry {
                id: app_id(&name, &exe_path, None),
                name,
                path: exe_path.clone(),
                target: exe_path.clone(),
                args: String::new(),
                aumid: None,
                icon_source: exe_path.clone(),
                builtin: false,
            });
            existing_targets.insert(target_norm);
            existing_names.insert(name_norm);
            added += 1;
        }
    }

    if added > 0 {
        log::info!("catalog: {added} installed apps without Start Menu shortcuts");
    }
}

fn slug(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}
