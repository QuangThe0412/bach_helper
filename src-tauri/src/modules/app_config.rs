use crate::modules::settings::get_appdata_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub const DEFAULT_PS_WIDTH: u32 = 1000;
pub const DEFAULT_PS_HEIGHT: u32 = 650;
pub const MIN_PS_WIDTH: u32 = 320;
pub const MAX_PS_WIDTH: u32 = 3840;
pub const MIN_PS_HEIGHT: u32 = 240;
pub const MAX_PS_HEIGHT: u32 = 2160;

/// User configuration persisted in %APPDATA%\batch_helper\setting.json.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct UserSetting {
    pub ps_window_width: u32,
    pub ps_window_height: u32,
    pub auto_capture_clipboard: bool,
}

impl Default for UserSetting {
    fn default() -> Self {
        Self {
            ps_window_width: DEFAULT_PS_WIDTH,
            ps_window_height: DEFAULT_PS_HEIGHT,
            auto_capture_clipboard: true,
        }
    }
}

/// Clamp user settings within safe window bounds.
pub fn sanitize_setting(setting: &UserSetting) -> UserSetting {
    UserSetting {
        ps_window_width: setting.ps_window_width.clamp(MIN_PS_WIDTH, MAX_PS_WIDTH),
        ps_window_height: setting.ps_window_height.clamp(MIN_PS_HEIGHT, MAX_PS_HEIGHT),
        auto_capture_clipboard: setting.auto_capture_clipboard,
    }
}

/// Return path to %APPDATA%\batch_helper\setting.json.
pub fn get_setting_file_path() -> PathBuf {
    get_appdata_dir().join("setting.json")
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

/// Save sanitized user setting to setting.json.
pub fn save_user_setting(setting: &UserSetting) -> Result<UserSetting, String> {
    let sanitized = sanitize_setting(setting);
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
