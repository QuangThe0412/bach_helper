use crate::modules::settings::get_appdata_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_PS_WIDTH: u32 = 1000;
pub const DEFAULT_PS_HEIGHT: u32 = 650;
pub const MIN_PS_WIDTH: u32 = 320;
pub const MAX_PS_WIDTH: u32 = 3840;
pub const MIN_PS_HEIGHT: u32 = 240;
pub const MAX_PS_HEIGHT: u32 = 2160;

static PROBE_SEQ: AtomicU64 = AtomicU64::new(1);

/// User configuration persisted in %APPDATA%\batch_helper\setting.json.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct UserSetting {
    pub ps_window_width: u32,
    pub ps_window_height: u32,
    pub auto_capture_clipboard: bool,
    pub auto_save_image: bool,
    pub screenshot_save_dir: Option<String>,
}

impl Default for UserSetting {
    fn default() -> Self {
        Self {
            ps_window_width: DEFAULT_PS_WIDTH,
            ps_window_height: DEFAULT_PS_HEIGHT,
            auto_capture_clipboard: true,
            auto_save_image: false,
            screenshot_save_dir: None,
        }
    }
}

/// Clamp user settings within safe window bounds and normalize paths.
pub fn sanitize_setting(setting: &UserSetting) -> UserSetting {
    let clean_dir = setting
        .screenshot_save_dir
        .as_deref()
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty());

    UserSetting {
        ps_window_width: setting.ps_window_width.clamp(MIN_PS_WIDTH, MAX_PS_WIDTH),
        ps_window_height: setting.ps_window_height.clamp(MIN_PS_HEIGHT, MAX_PS_HEIGHT),
        auto_capture_clipboard: setting.auto_capture_clipboard,
        auto_save_image: setting.auto_save_image,
        screenshot_save_dir: clean_dir,
    }
}

/// Return path to %APPDATA%\batch_helper\setting.json.
pub fn get_setting_file_path() -> PathBuf {
    get_appdata_dir().join("setting.json")
}

/// Default Auto Save Image root directory: `{folder containing setting.json}/image`.
pub fn get_default_screenshot_root_dir() -> PathBuf {
    let setting_path = get_setting_file_path();
    let base_dir = setting_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(get_appdata_dir);
    base_dir.join("image")
}

/// Resolve effective Auto Save Image root directory anchored to `setting.json`'s parent directory
/// (never relative to the application's current working directory).
pub fn resolve_screenshot_root_dir(setting: &UserSetting) -> PathBuf {
    let setting_path = get_setting_file_path();
    let setting_dir = setting_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(get_appdata_dir);

    match setting
        .screenshot_save_dir
        .as_deref()
        .map(|s| s.trim().trim_matches('"'))
        .filter(|s| !s.is_empty())
    {
        Some(raw) => {
            let clean = PathBuf::from(raw.replace('/', "\\"));
            if clean.is_absolute() {
                clean
            } else {
                setting_dir.join(clean)
            }
        }
        None => setting_dir.join("image"),
    }
}

/// Replace characters disallowed in Windows filenames/directories while preserving readable Unicode.
pub fn sanitize_fs_segment(raw: &str, fallback: &str) -> String {
    let mut cleaned = String::with_capacity(raw.len());
    for ch in raw.trim().chars() {
        if matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || ch.is_control() {
            cleaned.push('_');
        } else {
            cleaned.push(ch);
        }
    }
    let trimmed = cleaned.trim().trim_end_matches(['.', ' ']).to_string();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed
    }
}

/// Resolve project folder name cleanly from `project_path` or ancestor folders of `.bat` path.
pub fn extract_project_folder_name(project_path: Option<&str>, bat_path: &str) -> String {
    if let Some(raw_proj) = project_path {
        let trimmed = raw_proj.trim().trim_matches('"');
        if !trimmed.is_empty() {
            let norm = trimmed.replace('/', "\\");
            if let Some(last) = Path::new(&norm)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
            {
                return sanitize_fs_segment(&last, "default_project");
            }
        }
    }

    let norm_bat = bat_path.trim().trim_matches('"').replace('/', "\\");
    let mut cursor = Path::new(&norm_bat).parent();
    while let Some(dir) = cursor {
        if let Some(name) = dir.file_name().map(|n| n.to_string_lossy().to_string()) {
            let lower = name.to_lowercase();
            if lower != "batfiles" && lower != "batch" && lower != "target" {
                return sanitize_fs_segment(&name, "default_project");
            }
        }
        cursor = dir.parent();
    }

    "default_project".to_string()
}

/// Resolve deterministic PNG screenshot output path: `{root}/{project_name}/{bat_stem}.png`.
/// Never appends timestamps so newer runs always overwrite the same file cleanly.
pub fn resolve_screenshot_output_path(
    setting: &UserSetting,
    project_path: Option<&str>,
    bat_path: &str,
) -> PathBuf {
    let root = resolve_screenshot_root_dir(setting);

    let proj_name = extract_project_folder_name(project_path, bat_path);
    let norm_bat = bat_path.trim().trim_matches('"').replace('/', "\\");
    let stem = Path::new(&norm_bat)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "batch".to_string());
    let safe_stem = sanitize_fs_segment(&stem, "batch");

    root.join(proj_name).join(format!("{}.png", safe_stem))
}

/// Verify that `dir` can be created and written to cleanly.
pub fn validate_writable_directory(dir: &Path) -> Result<(), String> {
    if dir.as_os_str().is_empty() {
        return Err("Screenshot destination folder cannot be empty.".to_string());
    }

    if dir.is_file() {
        return Err(format!(
            "Screenshot path points to a file instead of a directory: {}",
            dir.display()
        ));
    }

    fs::create_dir_all(dir).map_err(|e| {
        format!(
            "Failed to create screenshot directory '{}': {}",
            dir.display(),
            e
        )
    })?;

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = PROBE_SEQ.fetch_add(1, Ordering::Relaxed);
    let probe = dir.join(format!(
        ".bh_write_probe_{}_{}_{}",
        std::process::id(),
        nanos,
        seq
    ));

    fs::write(&probe, b"ok").map_err(|e| {
        format!(
            "Screenshot directory '{}' is not writable: {}",
            dir.display(),
            e
        )
    })?;
    let _ = fs::remove_file(&probe);
    Ok(())
}

/// Load setting.json, automatically creating it with defaults if missing or invalid.
pub fn load_user_setting() -> UserSetting {
    let path = get_setting_file_path();
    if !path.exists() {
        let default_setting = UserSetting::default();
        let _ = save_user_setting(&default_setting);
        return default_setting;
    }

    match fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<UserSetting>(&content) {
            Ok(parsed) => sanitize_setting(&parsed),
            Err(_) => {
                let default_setting = UserSetting::default();
                let _ = save_user_setting(&default_setting);
                default_setting
            }
        },
        Err(_) => UserSetting::default(),
    }
}

/// Save sanitized user setting to setting.json (validating Auto Save Image directory when enabled).
pub fn save_user_setting(setting: &UserSetting) -> Result<UserSetting, String> {
    let sanitized = sanitize_setting(setting);
    if sanitized.auto_save_image {
        let target_root = resolve_screenshot_root_dir(&sanitized);
        validate_writable_directory(&target_root)?;
    }

    let dir = get_appdata_dir();
    if !dir.exists() {
        fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create AppData directory: {}", e))?;
    }

    let path = get_setting_file_path();
    let json_str = serde_json::to_string_pretty(&sanitized)
        .map_err(|e| format!("Failed to serialize setting.json: {}", e))?;

    fs::write(&path, json_str)
        .map_err(|e| format!("Failed to write setting.json to {}: {}", path.display(), e))?;

    Ok(sanitized)
}
