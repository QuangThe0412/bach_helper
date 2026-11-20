use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static GROUP_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NormalizedProjectPath {
    pub display_path: String,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BatchGroup {
    pub id: String,
    pub name: String,
    pub files: Vec<String>,
}

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
    #[serde(default)]
    pub project_groups: HashMap<String, Vec<BatchGroup>>,
    #[serde(default)]
    pub project_target_paths: HashMap<String, String>,
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
            project_groups: HashMap::new(),
            project_target_paths: HashMap::new(),
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
    let normalized = normalize_project_path(project_path);
    if normalized.key.is_empty() {
        return Ok(load_settings());
    }

    let mut settings = load_settings();
    settings.last_project = Some(normalized.display_path.clone());

    settings
        .history
        .retain(|p| normalize_project_key(p) != normalized.key);
    settings.history.insert(0, normalized.display_path);

    if settings.history.len() > 20 {
        settings.history.truncate(20);
    }

    save_settings(&settings)?;
    Ok(settings)
}

pub fn remove_project_from_history(project_path: &str) -> Result<AppSettings, String> {
    let clean_key = normalize_project_key(project_path);
    let mut settings = load_settings();

    if clean_key.is_empty() {
        return Ok(settings);
    }

    settings
        .history
        .retain(|p| normalize_project_key(p) != clean_key);

    if let Some(ref lp) = settings.last_project {
        if normalize_project_key(lp) == clean_key {
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

fn lexical_normalize_windows_path(raw: &str) -> PathBuf {
    use std::path::Component;

    let mut out = PathBuf::new();
    for comp in Path::new(raw).components() {
        match comp {
            Component::Prefix(prefix) => {
                let p_str = prefix.as_os_str().to_string_lossy();
                if p_str.len() == 2 && p_str.ends_with(':') {
                    out.push(p_str.to_ascii_uppercase());
                } else {
                    out.push(prefix.as_os_str());
                }
            }
            Component::RootDir => {
                out.push(comp.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                let can_pop = out
                    .components()
                    .last()
                    .map(|c| matches!(c, Component::Normal(_)))
                    .unwrap_or(false);
                if can_pop {
                    out.pop();
                }
            }
            Component::Normal(seg) => {
                out.push(seg);
            }
        }
    }
    out
}

/// Normalize a project root path into a canonical display path and case-insensitive key.
pub fn normalize_project_path(raw_path: &str) -> NormalizedProjectPath {
    let trimmed = raw_path.trim().trim_matches('"');
    if trimmed.is_empty() {
        return NormalizedProjectPath {
            display_path: String::new(),
            key: String::new(),
        };
    }

    let win_norm = trimmed.replace('/', "\\");
    let lexical = lexical_normalize_windows_path(&win_norm);

    let resolved = if lexical.exists() {
        let canonical = fs::canonicalize(&lexical).unwrap_or_else(|_| lexical.clone());
        let canon_str = canonical.to_string_lossy();
        let stripped = if let Some(unc) = canon_str.strip_prefix(r"\\?\UNC\") {
            format!(r"\\{}", unc)
        } else if let Some(local) = canon_str.strip_prefix(r"\\?\") {
            local.to_string()
        } else {
            canon_str.to_string()
        };
        crate::modules::builder::to_display_long_path(Path::new(&stripped))
    } else {
        crate::modules::builder::resolve_long_path_best_effort(&lexical)
    };

    let mut display_str = resolved.to_string_lossy().replace('/', "\\");
    let is_drive_root = display_str.len() == 3
        && display_str.as_bytes().get(1) == Some(&b':')
        && display_str.ends_with('\\');
    if !is_drive_root && !display_str.starts_with(r"\\") {
        display_str = display_str.trim_end_matches('\\').to_string();
    } else if display_str.starts_with(r"\\") {
        let rest = display_str.trim_end_matches('\\');
        if rest.len() > 2 {
            display_str = rest.to_string();
        }
    }

    let key = display_str.to_lowercase();
    NormalizedProjectPath {
        display_path: display_str,
        key,
    }
}

/// Normalize a project root path into a consistent case-insensitive lookup key.
pub fn normalize_project_key(project_path: &str) -> String {
    normalize_project_path(project_path).key
}

fn normalize_file_entry(file_path: &str) -> String {
    file_path.trim().trim_matches('"').to_string()
}

fn normalize_file_key(file_path: &str) -> String {
    file_path
        .trim()
        .trim_matches('"')
        .replace('/', "\\")
        .to_lowercase()
}

fn dedup_file_list(existing: &[String], incoming: &[String]) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut merged: Vec<String> = Vec::with_capacity(existing.len() + incoming.len());

    for raw in existing.iter().chain(incoming.iter()) {
        let clean = normalize_file_entry(raw);
        let key = normalize_file_key(&clean);
        if !clean.is_empty() && seen.insert(key) {
            merged.push(clean);
        }
    }

    merged
}

pub fn get_project_groups(project_path: &str) -> Vec<BatchGroup> {
    let key = normalize_project_key(project_path);
    if key.is_empty() {
        return Vec::new();
    }
    let settings = load_settings();
    settings.project_groups.get(&key).cloned().unwrap_or_default()
}

pub fn create_batch_group(
    project_path: &str,
    name: &str,
    files: Vec<String>,
) -> Result<Vec<BatchGroup>, String> {
    let key = normalize_project_key(project_path);
    if key.is_empty() {
        return Err("Please select a project directory before creating a group.".to_string());
    }

    let clean_name = name.trim();
    if clean_name.is_empty() {
        return Err("Group name cannot be empty.".to_string());
    }

    let clean_files = dedup_file_list(&[], &files);
    if clean_files.is_empty() {
        return Err("Please select at least one .bat file to add to the group.".to_string());
    }

    let mut settings = load_settings();
    let groups = settings.project_groups.entry(key.clone()).or_default();

    if groups
        .iter()
        .any(|g| g.name.trim().eq_ignore_ascii_case(clean_name))
    {
        return Err(format!(
            "A group named '{}' already exists in this project.",
            clean_name
        ));
    }

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = GROUP_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    let new_group = BatchGroup {
        id: format!("grp_{}_{}", nanos, seq),
        name: clean_name.to_string(),
        files: clean_files,
    };

    groups.push(new_group);
    let updated = groups.clone();
    save_settings(&settings)?;
    Ok(updated)
}

pub fn add_files_to_batch_group(
    project_path: &str,
    group_id: &str,
    files: Vec<String>,
) -> Result<Vec<BatchGroup>, String> {
    let key = normalize_project_key(project_path);
    if key.is_empty() {
        return Err("Please select a project directory first.".to_string());
    }

    let clean_id = group_id.trim();
    if clean_id.is_empty() {
        return Err("Target group ID cannot be empty.".to_string());
    }

    if files.iter().all(|f| normalize_file_entry(f).is_empty()) {
        return Err("No valid .bat files selected to add.".to_string());
    }

    let mut settings = load_settings();
    let groups = settings.project_groups.entry(key).or_default();

    let Some(target_group) = groups.iter_mut().find(|g| g.id == clean_id) else {
        return Err("Selected batch group no longer exists in this project.".to_string());
    };

    target_group.files = dedup_file_list(&target_group.files, &files);
    let updated = groups.clone();
    save_settings(&settings)?;
    Ok(updated)
}

pub fn remove_files_from_batch_group(
    project_path: &str,
    group_id: &str,
    files: Vec<String>,
) -> Result<Vec<BatchGroup>, String> {
    let key = normalize_project_key(project_path);
    if key.is_empty() {
        return Err("Please select a project directory first.".to_string());
    }

    let clean_id = group_id.trim();
    if clean_id.is_empty() {
        return Err("Target group ID cannot be empty.".to_string());
    }

    let mut remove_keys: HashSet<String> = HashSet::new();
    let mut remove_names: HashSet<String> = HashSet::new();
    for raw in &files {
        let clean = normalize_file_entry(raw);
        if clean.is_empty() {
            continue;
        }
        remove_keys.insert(normalize_file_key(&clean));
        if let Some(fname) = std::path::Path::new(&clean).file_name() {
            let lower = fname.to_string_lossy().trim().to_lowercase();
            if !lower.is_empty() {
                remove_names.insert(lower);
            }
        }
    }

    if remove_keys.is_empty() && remove_names.is_empty() {
        return Err("No valid .bat files selected to remove.".to_string());
    }

    let mut settings = load_settings();
    let Some(groups) = settings.project_groups.get_mut(&key) else {
        return Err("No batch groups found for this project.".to_string());
    };

    let Some(target_group) = groups.iter_mut().find(|g| g.id == clean_id) else {
        return Err("Selected batch group no longer exists in this project.".to_string());
    };

    target_group.files.retain(|entry| {
        let entry_key = normalize_file_key(entry);
        let entry_name = std::path::Path::new(entry.trim().trim_matches('"'))
            .file_name()
            .map(|n| n.to_string_lossy().trim().to_lowercase())
            .unwrap_or_default();
        !remove_keys.contains(&entry_key)
            && (entry_name.is_empty() || !remove_names.contains(&entry_name))
    });

    let updated = groups.clone();
    save_settings(&settings)?;
    Ok(updated)
}

pub fn delete_batch_group(project_path: &str, group_id: &str) -> Result<Vec<BatchGroup>, String> {
    let key = normalize_project_key(project_path);
    if key.is_empty() {
        return Err("Please select a project directory first.".to_string());
    }

    let clean_id = group_id.trim();
    let mut settings = load_settings();
    if let Some(groups) = settings.project_groups.get_mut(&key) {
        groups.retain(|g| g.id != clean_id);
        let updated = groups.clone();
        if updated.is_empty() {
            settings.project_groups.remove(&key);
        }
        save_settings(&settings)?;
        return Ok(updated);
    }

    Ok(Vec::new())
}

pub fn get_project_target_path(project_path: &str) -> Option<String> {
    let key = normalize_project_key(project_path);
    if key.is_empty() {
        return None;
    }
    let settings = load_settings();
    settings
        .project_target_paths
        .get(&key)
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
}

pub fn set_project_target_path(
    project_path: &str,
    target_path: Option<&str>,
) -> Result<Option<String>, String> {
    let key = normalize_project_key(project_path);
    if key.is_empty() {
        return Err("Please select a project directory before setting a target path.".to_string());
    }

    let mut settings = load_settings();
    let stored = match target_path {
        Some(raw) => {
            let trimmed = raw.trim().trim_matches('"');
            if trimmed.is_empty() {
                settings.project_target_paths.remove(&key);
                None
            } else {
                let mut norm = trimmed.replace('/', "\\");
                let is_drive_root = norm.len() == 3
                    && norm.as_bytes().get(1) == Some(&b':')
                    && norm.ends_with('\\');
                if !is_drive_root && !norm.starts_with(r"\\") {
                    norm = norm.trim_end_matches('\\').to_string();
                } else if norm.starts_with(r"\\") {
                    let rest = norm.trim_end_matches('\\');
                    if rest.len() > 2 {
                        norm = rest.to_string();
                    }
                }
                settings.project_target_paths.insert(key, norm.clone());
                Some(norm)
            }
        }
        None => {
            settings.project_target_paths.remove(&key);
            None
        }
    };

    save_settings(&settings)?;
    Ok(stored)
}
