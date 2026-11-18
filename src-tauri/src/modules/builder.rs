use crate::modules::environment::{decode_console_bytes, encode_console_bytes};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tauri::{AppHandle, Emitter};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatFileInfo {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub modified_time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildLogEvent {
    pub tab_id: String,
    pub line: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildResult {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub message: String,
    pub build_xml_dir: String,
    pub target: String,
    pub properties_file: Option<String>,
    pub base_directory: Option<String>,
    pub bat_folder: Option<String>,
    pub bat_files: Vec<BatFileInfo>,
    pub log_file_path: Option<String>,
    pub log_folder_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostBuildInspection {
    pub properties_file: Option<String>,
    pub base_directory: Option<String>,
    pub bat_folder: Option<String>,
    pub bat_files: Vec<BatFileInfo>,
    pub log_file_path: Option<String>,
    pub log_folder_path: Option<String>,
}

/// Find any OneDrive Desktop directories under USERPROFILE
fn find_onedrive_desktops() -> Vec<PathBuf> {
    let mut desktops = Vec::new();
    if let Ok(user_profile) = std::env::var("USERPROFILE") {
        let up_path = Path::new(&user_profile);
        if let Ok(entries) = fs::read_dir(up_path) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with("OneDrive") {
                        let desk = p.join("Desktop");
                        if desk.is_dir() {
                            desktops.push(desk);
                        }
                    }
                }
            }
        }
    }
    desktops
}

/// Helper to locate build directory containing build.xml
pub fn find_build_dir(project_path: &str) -> Option<PathBuf> {
    let base = Path::new(project_path.trim().trim_matches('"'));
    let in_build = base.join("build").join("build.xml");
    if in_build.is_file() {
        return Some(base.join("build"));
    }
    let in_root = base.join("build.xml");
    if in_root.is_file() {
        return Some(base.to_path_buf());
    }
    None
}

/// Extract properties file name from target or build.xml
pub fn find_properties_for_target(build_dir: &Path, target: &str) -> Option<PathBuf> {
    let build_xml_path = build_dir.join("build.xml");
    if let Ok(bytes) = fs::read(&build_xml_path) {
        let content = decode_console_bytes(&bytes);
        let mut in_target = false;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("<target") && trimmed.contains(&format!("name=\"{}\"", target)) {
                in_target = true;
            } else if in_target && trimmed.starts_with("</target>") {
                break;
            }

            if in_target {
                if let Some(prop_file) = extract_xml_attribute(trimmed, "file") {
                    if prop_file.ends_with(".properties") {
                        let candidate = build_dir.join(&prop_file);
                        if candidate.is_file() {
                            return Some(candidate);
                        }
                    }
                }
            }
        }
    }

    let prefix = target
        .strip_suffix("-package")
        .or_else(|| target.strip_suffix("-pkg"))
        .unwrap_or(target);

    let candidates = vec![
        format!("{}.properties", target),
        format!("{}.properties", prefix),
        format!("{}.properties", prefix.replace('-', "_")),
    ];

    let check_folders = vec![
        build_dir.join("filters"),
        build_dir.join("filter"),
        build_dir.to_path_buf(),
        build_dir.parent().map(|p| p.join("build/filters")).unwrap_or_default(),
        build_dir.parent().map(|p| p.join("filters")).unwrap_or_default(),
    ];

    for folder in check_folders {
        if folder.is_dir() {
            for c in &candidates {
                let candidate_path = folder.join(c);
                if candidate_path.is_file() {
                    return Some(candidate_path);
                }
            }
        }
    }

    None
}

fn extract_xml_attribute(line: &str, attr: &str) -> Option<String> {
    let key = format!("{}=\"", attr);
    if let Some(idx) = line.find(&key) {
        let after = &line[idx + key.len()..];
        if let Some(end_idx) = after.find('"') {
            return Some(after[..end_idx].to_string());
        }
    }
    None
}

/// Resolve valid JAVA_HOME directory with bin/javac.exe
pub fn resolve_active_java_home(java_home_opt: Option<&str>) -> Option<String> {
    if let Some(jh) = java_home_opt {
        let clean = jh.trim().trim_matches('"');
        if !clean.is_empty() && Path::new(clean).join("bin").join("javac.exe").is_file() {
            return Some(clean.to_string());
        }
    }
    if let Ok(env_jh) = std::env::var("JAVA_HOME") {
        let clean = env_jh.trim().trim_matches('"');
        if !clean.is_empty() && Path::new(clean).join("bin").join("javac.exe").is_file() {
            return Some(clean.to_string());
        }
    }
    let scan = crate::modules::jdk_manager::scan_available_jdks();
    if let Some(rec) = scan
        .jdks
        .iter()
        .find(|j| j.is_recommended && j.javac_executable.is_some())
    {
        return Some(rec.root_path.clone());
    }
    if let Some(any_jdk) = scan.jdks.iter().find(|j| j.javac_executable.is_some()) {
        return Some(any_jdk.root_path.clone());
    }
    None
}

/// Check and fix javac executable path in build.xml if invalid or mismatched
pub fn fix_build_xml_java_path(build_dir: &Path, active_java_home: &str) -> Option<String> {
    let build_xml_path = build_dir.join("build.xml");
    let bytes = fs::read(&build_xml_path).ok()?;
    let was_utf8 = std::str::from_utf8(&bytes).is_ok();
    let content = decode_console_bytes(&bytes);

    let desired_javac = Path::new(active_java_home)
        .join("bin")
        .join("javac")
        .to_string_lossy()
        .to_string();

    let mut updated_lines = Vec::new();
    let mut changed_to: Option<String> = None;

    for line in content.lines() {
        let mut new_line = line.to_string();
        if let Some(cur_exe) = extract_xml_attribute(line, "executable") {
            if cur_exe.to_lowercase().contains("javac") {
                let cur_path = Path::new(&cur_exe);
                let cur_exists = cur_path.is_file()
                    || Path::new(&format!("{}.exe", cur_exe)).is_file();

                if !cur_exists || !cur_exe.eq_ignore_ascii_case(&desired_javac) {
                    let old_attr = format!("executable=\"{}\"", cur_exe);
                    let new_attr = format!("executable=\"{}\"", desired_javac);
                    new_line = line.replace(&old_attr, &new_attr);
                    changed_to = Some(desired_javac.clone());
                }
            }
        }
        updated_lines.push(new_line);
    }

    if changed_to.is_some() {
        let line_ending = if content.contains("\r\n") { "\r\n" } else { "\n" };
        let mut final_text = updated_lines.join(line_ending);
        if content.ends_with('\n') {
            final_text.push_str(line_ending);
        }
        let encoded = encode_console_bytes(&final_text, was_utf8);
        let _ = fs::write(&build_xml_path, encoded);
    }

    changed_to
}

/// Check and fix project paths & javaHome in the target's .properties file
pub fn fix_properties_file_paths(
    prop_file: &Path,
    project_path: &str,
    active_java_home: Option<&str>,
) -> bool {
    let bytes = match fs::read(prop_file) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let was_utf8 = std::str::from_utf8(&bytes).is_ok();
    let content = decode_console_bytes(&bytes);

    let proj_slash = project_path
        .trim()
        .trim_matches('"')
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();

    let java_slash = active_java_home.map(|jh| {
        jh.trim()
            .trim_matches('"')
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_string()
    });

    // Detect old project root prefix from baseDirectory or pom.log4j.path.value
    let mut old_proj_prefix: Option<String> = None;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.starts_with('!') {
            continue;
        }
        if let Some(idx) = trimmed.find('=') {
            let key = trimmed[..idx].trim();
            let val = trimmed[idx + 1..].trim().replace('\\', "/");
            if key == "filter.shellscript.baseDirectory.value" {
                let clean_val = val.trim_end_matches('/');
                if let Some(prefix) = clean_val.strip_suffix("/target") {
                    old_proj_prefix = Some(prefix.to_string());
                    break;
                }
            } else if key == "pom.log4j.path.value" && old_proj_prefix.is_none() {
                if let Some(t_idx) = val.find("/target/") {
                    old_proj_prefix = Some(val[..t_idx].to_string());
                }
            }
        }
    }

    let mut changed = false;
    let mut updated_lines = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
            updated_lines.push(line.to_string());
            continue;
        }

        if let Some(idx) = line.find('=') {
            let key_raw = &line[..idx];
            let key = key_raw.trim();
            let val_raw = &line[idx + 1..];
            let val = val_raw.trim();

            let mut new_val: Option<String> = None;

            if key == "filter.shellscript.javaHome.value" || key.ends_with(".javaHome.value") {
                if let Some(ref js) = java_slash {
                    if val != js {
                        new_val = Some(js.clone());
                    }
                }
            } else if key == "filter.shellscript.baseDirectory.value" {
                let desired = format!("{}/target", proj_slash);
                if val != desired {
                    new_val = Some(desired);
                }
            } else if key == "pom.log4j.path.value" {
                let desired = format!("{}/target/log/", proj_slash);
                if val != desired {
                    new_val = Some(desired);
                }
            } else if key == "filter.resources.propertiesLocation" {
                let desired = format!("{}/target/batch/properties", proj_slash);
                if val != desired {
                    new_val = Some(desired);
                }
            } else if key == "filter.common.errmailBatchPath.value"
                || key == "filter.common.errmailWebPath.value"
            {
                let desired = format!("{}/target/batch/properties/mdware_xml", proj_slash);
                if val != desired {
                    new_val = Some(desired);
                }
            } else {
                let val_norm = val.replace('\\', "/");
                if let Some(t_idx) = val_norm.find("/target/") {
                    let suffix = &val_norm[t_idx..];
                    let desired = format!("{}{}", proj_slash, suffix);
                    if val != desired {
                        new_val = Some(desired);
                    }
                } else if let Some(ref old_p) = old_proj_prefix {
                    if !old_p.is_empty() && val_norm.starts_with(old_p) {
                        let suffix = &val_norm[old_p.len()..];
                        let desired = format!("{}{}", proj_slash, suffix);
                        if val != desired {
                            new_val = Some(desired);
                        }
                    }
                }
            }

            if let Some(nv) = new_val {
                changed = true;
                updated_lines.push(format!("{}={}", key_raw, nv));
            } else {
                updated_lines.push(line.to_string());
            }
        } else {
            updated_lines.push(line.to_string());
        }
    }

    if changed {
        let line_ending = if content.contains("\r\n") { "\r\n" } else { "\n" };
        let mut final_text = updated_lines.join(line_ending);
        if content.ends_with('\n') {
            final_text.push_str(line_ending);
        }
        // Save as UTF-8 if it contains non-ASCII project paths or was UTF-8, preserving compatibility
        let save_utf8 = was_utf8 || !proj_slash.is_ascii();
        let encoded = encode_console_bytes(&final_text, save_utf8);
        let _ = fs::write(prop_file, encoded);
    }

    changed
}

/// Read a key from a .properties file
pub fn extract_property_value(properties_file: &Path, target_key: &str) -> Option<String> {
    let content = fs::read(properties_file).ok()?;
    let text = decode_console_bytes(&content);

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.starts_with('!') {
            continue;
        }
        if let Some(idx) = trimmed.find('=') {
            let key = trimmed[..idx].trim();
            let val = trimmed[idx + 1..].trim();
            if key == target_key {
                return Some(val.to_string());
            }
        }
    }
    None
}

/// Read properties file and find key `filter.shellscript.baseDirectory.value=`
pub fn extract_base_directory(properties_file: &Path) -> Option<String> {
    extract_property_value(properties_file, "filter.shellscript.baseDirectory.value")
}

/// Resolve batchApp.log path and log directory
pub fn resolve_batch_app_log(
    project_path: &str,
    properties_file: Option<&Path>,
    base_dir_str: Option<&str>,
) -> (Option<String>, Option<String>) {
    let base_proj = Path::new(project_path.trim().trim_matches('"'));
    let mut candidate_dirs: Vec<PathBuf> = Vec::new();

    if let Some(prop_path) = properties_file {
        if let Some(log4j_dir) = extract_property_value(prop_path, "pom.log4j.path.value") {
            let clean = log4j_dir.trim().replace('\\', "/");
            candidate_dirs.push(PathBuf::from(&clean));

            if clean.contains("/Desktop/") {
                if let Ok(user_profile) = std::env::var("USERPROFILE") {
                    let std_desk = format!("{}/Desktop", user_profile.replace('\\', "/"));
                    for od_desk in find_onedrive_desktops() {
                        let od_str = od_desk.to_string_lossy().replace('\\', "/");
                        let replaced = clean.replace(&std_desk, &od_str);
                        candidate_dirs.push(PathBuf::from(replaced));
                    }
                }
            }
        }
    }

    if let Some(base_str) = base_dir_str {
        let clean = base_str.trim().replace('\\', "/");
        candidate_dirs.push(PathBuf::from(&clean).join("log"));
    }

    candidate_dirs.push(base_proj.join("target").join("log"));
    candidate_dirs.push(base_proj.join("log"));
    candidate_dirs.push(base_proj.join("batch").join("log"));

    let mut found_log_file: Option<String> = None;
    let mut found_log_dir: Option<String> = None;

    for dir in &candidate_dirs {
        let log_file = dir.join("batchApp.log");
        if log_file.is_file() {
            found_log_file = Some(log_file.to_string_lossy().to_string());
            found_log_dir = Some(dir.to_string_lossy().to_string());
            break;
        }
        if found_log_dir.is_none() && dir.is_dir() {
            found_log_dir = Some(dir.to_string_lossy().to_string());
        }
    }

    (found_log_file, found_log_dir)
}

/// Scan a user-specified custom folder for .bat files
pub fn scan_custom_bat_folder(folder_path: &str) -> (Option<String>, Vec<BatFileInfo>) {
    let clean = folder_path.trim().trim_matches('"');
    let base = Path::new(clean);
    if !base.exists() {
        return (None, Vec::new());
    }
    if base.is_file() {
        if let Some(parent) = base.parent() {
            let files = list_bat_files_in_dir(parent);
            return (Some(parent.to_string_lossy().to_string()), files);
        }
    }

    let candidates = vec![
        base.to_path_buf(),
        base.join("batfiles"),
        base.join("batch").join("batfiles"),
        base.join("target").join("batch").join("batfiles"),
    ];

    for c in candidates {
        if c.is_dir() {
            let files = list_bat_files_in_dir(&c);
            if !files.is_empty() {
                return (Some(c.to_string_lossy().to_string()), files);
            }
        }
    }

    (Some(base.to_string_lossy().to_string()), Vec::new())
}

/// Find batch/batfiles directory and list all .bat files
pub fn scan_bat_files(project_path: &str, base_dir_str: Option<&str>) -> (Option<PathBuf>, Vec<BatFileInfo>) {
    let mut candidate_folders: Vec<PathBuf> = Vec::new();
    let base_proj = Path::new(project_path.trim().trim_matches('"'));

    if let Some(base_str) = base_dir_str {
        let base_str_clean = base_str.trim().replace('\\', "/");
        let direct_path = PathBuf::from(&base_str_clean);

        candidate_folders.push(direct_path.join("batch").join("batfiles"));
        candidate_folders.push(direct_path.join("batfiles"));
        candidate_folders.push(direct_path.join("batch"));
        candidate_folders.push(direct_path.clone());

        if base_str_clean.contains("/Desktop/") {
            if let Ok(user_profile) = std::env::var("USERPROFILE") {
                let std_desk = format!("{}/Desktop", user_profile.replace('\\', "/"));
                for od_desk in find_onedrive_desktops() {
                    let od_str = od_desk.to_string_lossy().replace('\\', "/");
                    let replaced = base_str_clean.replace(&std_desk, &od_str);
                    let p = PathBuf::from(replaced);
                    candidate_folders.push(p.join("batch").join("batfiles"));
                    candidate_folders.push(p.join("batfiles"));
                }
            }
        }
    }

    candidate_folders.push(base_proj.join("target").join("batch").join("batfiles"));
    candidate_folders.push(base_proj.join("batch").join("batfiles"));
    candidate_folders.push(base_proj.join("target").join("batfiles"));
    candidate_folders.push(base_proj.join("batch"));

    let mut found_dir: Option<PathBuf> = None;
    let mut bat_files = Vec::new();

    for folder in candidate_folders {
        if folder.is_dir() {
            let files = list_bat_files_in_dir(&folder);
            if !files.is_empty() {
                found_dir = Some(folder);
                bat_files = files;
                break;
            } else if found_dir.is_none() {
                found_dir = Some(folder);
            }
        }
    }

    (found_dir, bat_files)
}

fn list_bat_files_in_dir(dir: &Path) -> Vec<BatFileInfo> {
    let mut list = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.to_lowercase().ends_with(".bat") {
                    let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    let modified_time = entry
                        .metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .map(|t| {
                            let dt: ChronoLite = t.into();
                            dt.to_string()
                        })
                        .unwrap_or_else(|| "Unknown".to_string());

                    list.push(BatFileInfo {
                        name,
                        path: path.to_string_lossy().to_string(),
                        size_bytes,
                        modified_time,
                    });
                }
            }
        }
    }
    list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    list
}

struct ChronoLite(u64);
impl From<std::time::SystemTime> for ChronoLite {
    fn from(t: std::time::SystemTime) -> Self {
        let secs = t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        ChronoLite(secs)
    }
}
impl std::fmt::Display for ChronoLite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let secs = self.0;
        let days = secs / 86400;
        let day_secs = secs % 86400;
        let hours = day_secs / 3600;
        let mins = (day_secs % 3600) / 60;
        let s = day_secs % 60;

        let mut year = 1970;
        let mut d = days;
        loop {
            let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
            let days_in_year = if leap { 366 } else { 365 };
            if d >= days_in_year {
                d -= days_in_year;
                year += 1;
            } else {
                break;
            }
        }
        let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let month_days = [
            31,
            if leap { 29 } else { 28 },
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ];
        let mut month = 1;
        for &md in &month_days {
            if d >= md {
                d -= md;
                month += 1;
            } else {
                break;
            }
        }
        let day = d + 1;
        write!(
            f,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            year, month, day, hours, mins, s
        )
    }
}

/// Inspect target post-build or directly
pub fn inspect_target_files(project_path: &str, target: &str) -> PostBuildInspection {
    let build_dir = find_build_dir(project_path);
    let prop_file = build_dir.as_ref().and_then(|bd| find_properties_for_target(bd, target));

    let base_dir = prop_file.as_ref().and_then(|p| extract_base_directory(p));
    let (bat_folder, bat_files) = scan_bat_files(project_path, base_dir.as_deref());
    let (log_file_path, log_folder_path) =
        resolve_batch_app_log(project_path, prop_file.as_deref(), base_dir.as_deref());

    PostBuildInspection {
        properties_file: prop_file.map(|p| p.to_string_lossy().to_string()),
        base_directory: base_dir,
        bat_folder: bat_folder.map(|p| p.to_string_lossy().to_string()),
        bat_files,
        log_file_path,
        log_folder_path,
    }
}

/// Run CMD ant <target> and stream ALL logs in real-time (including Shift-JIS / CP932 lines)
pub fn run_ant_build(
    app: AppHandle,
    tab_id: String,
    project_path: String,
    target: String,
    ant_executable: Option<String>,
    java_home: Option<String>,
) -> Result<BuildResult, String> {
    let build_dir = find_build_dir(&project_path)
        .ok_or_else(|| format!("Cannot find build.xml in '{}' or its 'build' subfolder", project_path))?;

    let emit_line = |h: &AppHandle, t_id: &str, l: String| {
        let _ = h.emit(
            "build-output",
            BuildLogEvent {
                tab_id: t_id.to_string(),
                line: l,
            },
        );
    };

    // 1. Resolve active JAVA_HOME (selected JDK or auto-detected JDK 25)
    let active_java_home = resolve_active_java_home(java_home.as_deref());

    // 2. Pre-Build Auto-Fix: Check & update javac executable in build.xml
    if let Some(ref jh) = active_java_home {
        if let Some(fixed_javac) = fix_build_xml_java_path(&build_dir, jh) {
            emit_line(
                &app,
                &tab_id,
                format!("[Auto-Fix] build.xml javac -> {}", fixed_javac),
            );
        }
    }

    // 3. Pre-Build Auto-Fix: Check & update paths in the target's .properties file
    if let Some(prop_path) = find_properties_for_target(&build_dir, &target) {
        if fix_properties_file_paths(&prop_path, &project_path, active_java_home.as_deref()) {
            let prop_name = prop_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "properties".to_string());
            emit_line(
                &app,
                &tab_id,
                format!("[Auto-Fix] Updated project & JDK paths in {}", prop_name),
            );
        }
    }

    // 4. Resolve clean Ant command (prefer "ant" if available in PATH, just like manual CMD)
    let ant_exe = if let Some(exe) = ant_executable.filter(|s| !s.trim().is_empty()) {
        let clean = exe.trim().trim_matches('"').to_string();
        if clean.to_lowercase().contains("extracted_ant") {
            clean
        } else {
            "ant".to_string()
        }
    } else {
        "ant".to_string()
    };

    emit_line(&app, &tab_id, format!("> {} {}", ant_exe, target));

    #[cfg(windows)]
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let mut command = Command::new("cmd.exe");
    command.args(["/c", &ant_exe, &target]);
    command.current_dir(&build_dir);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    // Ensure ANT_OPTS sets -Dfile.encoding=MS932 if build.xml uses MS932
    // This prevents Java 18+ (JEP 400 UTF-8 default) from writing UTF-8 @argfiles when javac expects MS932
    if let Ok(xml_bytes) = fs::read(build_dir.join("build.xml")) {
        let xml_str = decode_console_bytes(&xml_bytes);
        if xml_str.contains("MS932") || xml_str.contains("Shift-JIS") || xml_str.contains("Shift_JIS") {
            let existing_opts = std::env::var("ANT_OPTS").unwrap_or_default();
            if !existing_opts.contains("file.encoding") {
                let new_opts = if existing_opts.trim().is_empty() {
                    "-Dfile.encoding=MS932".to_string()
                } else {
                    format!("{} -Dfile.encoding=MS932", existing_opts.trim())
                };
                command.env("ANT_OPTS", new_opts);
            }
        }
    }

    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to spawn build process: {}", e))?;

    let stdout = child.stdout.take().ok_or("Failed to open stdout")?;
    let stderr = child.stderr.take().ok_or("Failed to open stderr")?;

    let app_handle_out = app.clone();
    let app_handle_err = app.clone();
    let tab_id_out = tab_id.clone();
    let tab_id_err = tab_id.clone();

    let (tx_line, rx_line) = std::sync::mpsc::channel::<String>();
    let tx_line_out = tx_line.clone();
    let tx_line_err = tx_line.clone();

    let out_thread = std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut buf = Vec::with_capacity(512);
        while let Ok(n) = reader.read_until(b'\n', &mut buf) {
            if n == 0 {
                break;
            }
            let line = decode_console_bytes(&buf);
            let _ = app_handle_out.emit(
                "build-output",
                BuildLogEvent {
                    tab_id: tab_id_out.clone(),
                    line: line.clone(),
                },
            );
            let _ = tx_line_out.send(line);
            buf.clear();
        }
    });

    let err_thread = std::thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        let mut buf = Vec::with_capacity(512);
        while let Ok(n) = reader.read_until(b'\n', &mut buf) {
            if n == 0 {
                break;
            }
            let line = decode_console_bytes(&buf);
            let _ = app_handle_err.emit(
                "build-output",
                BuildLogEvent {
                    tab_id: tab_id_err.clone(),
                    line: line.clone(),
                },
            );
            let _ = tx_line_err.send(line);
            buf.clear();
        }
    });

    let status = child.wait().map_err(|e| format!("Process wait error: {}", e))?;
    let _ = out_thread.join();
    let _ = err_thread.join();

    drop(tx_line);

    let mut all_lines = Vec::new();
    while let Ok(line) = rx_line.recv() {
        all_lines.push(line);
    }

    let exit_code = status.code().unwrap_or(-1);
    let has_build_successful = all_lines.iter().any(|l| l.contains("BUILD SUCCESSFUL"));
    let has_build_failed = all_lines.iter().any(|l| l.contains("BUILD FAILED"));

    let is_success = status.success() && has_build_successful && !has_build_failed;

    let result_msg = if is_success {
        "BUILD SUCCESSFUL".to_string()
    } else {
        "BUILD FAILED".to_string()
    };

    let inspection = inspect_target_files(&project_path, &target);

    Ok(BuildResult {
        success: is_success,
        exit_code: Some(exit_code),
        message: result_msg,
        build_xml_dir: build_dir.to_string_lossy().to_string(),
        target,
        properties_file: inspection.properties_file,
        base_directory: inspection.base_directory,
        bat_folder: inspection.bat_folder,
        bat_files: inspection.bat_files,
        log_file_path: inspection.log_file_path,
        log_folder_path: inspection.log_folder_path,
    })
}
