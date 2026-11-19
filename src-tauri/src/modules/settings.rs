use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub last_project: Option<String>,
    pub history: Vec<String>,
    pub selected_ant_id: Option<String>,
    pub selected_jdk_id: Option<String>,
    #[serde(default)]
    pub java_home: Option<String>,
    #[serde(default)]
    pub ant_home: Option<String>,
    #[serde(default)]
    pub open_tabs: Vec<String>,
    #[serde(default)]
    pub active_tab_index: usize,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            last_project: None,
            history: Vec::new(),
            selected_ant_id: None,
            selected_jdk_id: None,
            java_home: None,
            ant_home: None,
            open_tabs: Vec::new(),
            active_tab_index: 0,
        }
    }
}

pub fn get_appdata_dir() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        PathBuf::from(appdata).join("batch_helper")
    } else if let Ok(userprofile) = std::env::var("USERPROFILE") {
        PathBuf::from(userprofile)
            .join("AppData")
            .join("Roaming")
            .join("batch_helper")
    } else {
        PathBuf::from(".").join(".batch_helper")
    }
}

pub fn get_settings_file_path() -> PathBuf {
    get_appdata_dir().join("history.json")
}

pub fn load_settings() -> AppSettings {
    let path = get_settings_file_path();
    let legacy_path = get_appdata_dir().join("settings.json");

    if !path.exists() && legacy_path.exists() {
        if let Ok(content) = fs::read_to_string(&legacy_path) {
            if let Ok(migrated) = serde_json::from_str::<AppSettings>(&content) {
                let _ = save_settings(&migrated);
                let _ = fs::remove_file(&legacy_path);
                return migrated;
            }
        }
    }

    if !path.exists() {
        let default_settings = AppSettings::default();
        let _ = save_settings(&default_settings);
        return default_settings;
    }

    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_else(|_| {
            let default_settings = AppSettings::default();
            let _ = save_settings(&default_settings);
            default_settings
        }),
        Err(_) => AppSettings::default(),
    }
}

pub fn save_settings(settings: &AppSettings) -> Result<(), String> {
    let dir = get_appdata_dir();
    if !dir.exists() {
        fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create AppData directory: {}", e))?;
    }

    let path = get_settings_file_path();
    let json_str = serde_json::to_string_pretty(settings)
        .map_err(|e| format!("Failed to serialize history: {}", e))?;

    fs::write(&path, json_str)
        .map_err(|e| format!("Failed to write history.json to {}: {}", path.display(), e))?;

    Ok(())
}

pub fn add_project_to_history(project_path: &str) -> Result<AppSettings, String> {
    let clean_path = project_path.trim().trim_matches('"').to_string();
    if clean_path.is_empty() {
        return Ok(load_settings());
    }

    let mut settings = load_settings();
    settings.last_project = Some(clean_path.clone());

    settings
        .history
        .retain(|p| p.to_lowercase() != clean_path.to_lowercase());
    settings.history.insert(0, clean_path);

    if settings.history.len() > 20 {
        settings.history.truncate(20);
    }

    save_settings(&settings)?;
    Ok(settings)
}

pub fn remove_project_from_history(project_path: &str) -> Result<AppSettings, String> {
    let clean_path = project_path.trim().trim_matches('"');
    let mut settings = load_settings();

    if clean_path.is_empty() {
        return Ok(settings);
    }

    let clean_lower = clean_path.to_lowercase();
    settings
        .history
        .retain(|p| p.trim().to_lowercase() != clean_lower);

    if let Some(ref lp) = settings.last_project {
        if lp.trim().to_lowercase() == clean_lower {
            settings.last_project = settings.history.first().cloned();
        }
    }

    save_settings(&settings)?;
    Ok(settings)
}

pub fn save_open_tabs_state(
    open_tabs: Vec<String>,
    active_tab_index: usize,
) -> Result<AppSettings, String> {
    let mut settings = load_settings();
    settings.open_tabs = open_tabs;
    settings.active_tab_index = active_tab_index;
    if let Some(active_proj) = settings.open_tabs.get(active_tab_index) {
        if !active_proj.trim().is_empty() {
            settings.last_project = Some(active_proj.clone());
        }
    }
    save_settings(&settings)?;
    Ok(settings)
}

pub fn save_java_home_setting(java_home: &str) -> Result<AppSettings, String> {
    let mut settings = load_settings();
    settings.java_home = Some(java_home.trim().to_string());
    save_settings(&settings)?;
    Ok(settings)
}

pub fn save_ant_home_setting(ant_home: &str) -> Result<AppSettings, String> {
    let mut settings = load_settings();
    settings.ant_home = Some(ant_home.trim().to_string());
    save_settings(&settings)?;
    Ok(settings)
}
