use crate::modules::encoding::{
    detect_and_decode_text, detect_xml_declared_encoding, encode_text, escape_properties_value,
    unescape_properties_value, TextEncoding,
};
use crate::modules::environment::decode_console_bytes;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Emitter};

pub const WINDOWS_CMD_LINE_LIMIT: usize = 8191;
pub const WINDOWS_CMD_LINE_WARN: usize = 7600;
pub const WINDOWS_MAX_PATH_LIMIT: usize = 260;
pub const WINDOWS_MAX_PATH_WARN: usize = 235;

static PS_SCRIPT_SEQ: AtomicU64 = AtomicU64::new(1);

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
    pub default_target_path: Option<String>,
    pub effective_target_path: Option<String>,
    pub is_default_target: bool,
    pub bat_folder: Option<String>,
    pub bat_files: Vec<BatFileInfo>,
    pub batch_log_name: Option<String>,
    pub app_log_name: Option<String>,
    pub log_file_path: Option<String>,
    pub app_log_file_path: Option<String>,
    pub log_folder_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostBuildInspection {
    pub properties_file: Option<String>,
    pub base_directory: Option<String>,
    pub default_target_path: Option<String>,
    pub effective_target_path: Option<String>,
    pub is_default_target: bool,
    pub bat_folder: Option<String>,
    pub bat_files: Vec<BatFileInfo>,
    pub batch_log_name: Option<String>,
    pub app_log_name: Option<String>,
    pub log_file_path: Option<String>,
    pub app_log_file_path: Option<String>,
    pub log_folder_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AntTargetProperties {
    pub target_prop_name: String,
    pub build_classes_prop_name: String,
    pub default_target_path: String,
    pub env_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetPathValidation {
    pub is_valid: bool,
    pub is_default_target: bool,
    pub default_target_path: String,
    pub effective_target_path: String,
    pub batch_safe_target_path: String,
    pub effective_target_len: usize,
    pub batch_safe_target_len: usize,
    pub cmd_limit: usize,
    pub max_path_limit: usize,
    pub estimated_cmd_line_len: usize,
    pub estimated_longest_file_path_len: usize,
    pub max_safe_target_len: Option<usize>,
    pub offending_script: Option<String>,
    pub offending_line_number: Option<usize>,
    pub jar_reference_count: usize,
    pub severity: String,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetTargetPathResult {
    pub custom_target_path: Option<String>,
    pub effective_target_path: String,
    pub default_target_path: String,
    pub is_default_target: bool,
    pub properties_updated: bool,
    pub properties_file: Option<String>,
    pub validation: TargetPathValidation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoConfigResult {
    pub success: bool,
    pub build_xml_updated: bool,
    pub build_xml_javac_path: Option<String>,
    pub resolved_java_home: Option<String>,
    pub properties_file: Option<String>,
    pub properties_updated: bool,
    pub batch_safe_project_path: String,
    pub changes: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatGroupStepResult {
    pub path: String,
    pub name: String,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub stdout: String,
    pub stderr: String,
    pub error_message: Option<String>,
    pub screenshot_path: Option<String>,
    pub screenshot_saved: bool,
    pub screenshot_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatLaunchResult {
    pub launched: bool,
    pub screenshot_path: Option<String>,
    pub screenshot_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatExecutionFinishedEvent {
    pub tab_id: String,
    pub path: String,
    pub bat_name: String,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub screenshot_saved: bool,
    pub screenshot_path: Option<String>,
    pub screenshot_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClearLogsResult {
    pub cleared_files: Vec<String>,
    pub failed_files: Vec<String>,
    pub log_dirs_checked: Vec<String>,
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
        let (content, _) = detect_and_decode_text(&bytes);
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
        build_dir
            .parent()
            .map(|p| p.join("build/filters"))
            .unwrap_or_default(),
        build_dir
            .parent()
            .map(|p| p.join("filters"))
            .unwrap_or_default(),
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

/// Validate a candidate JAVA_HOME directory (handles JDK root, bin folder, or java/javac exe paths).
pub fn validate_java_home_dir(raw_path: &str) -> Result<PathBuf, String> {
    let trimmed = raw_path.trim().trim_matches('"');
    if trimmed.is_empty() {
        return Err("Empty JAVA_HOME path".to_string());
    }

    #[cfg(windows)]
    let normalized = trimmed.replace('/', "\\");
    #[cfg(not(windows))]
    let normalized = trimmed.to_string();

    let mut root = PathBuf::from(&normalized);
    if root.is_file() {
        let parent = root.parent().unwrap_or(&root).to_path_buf();
        if parent
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.eq_ignore_ascii_case("bin"))
            .unwrap_or(false)
        {
            root = parent.parent().unwrap_or(&parent).to_path_buf();
        } else {
            root = parent;
        }
    } else if root
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.eq_ignore_ascii_case("bin"))
        .unwrap_or(false)
    {
        if let Some(p) = root.parent() {
            root = p.to_path_buf();
        }
    }

    if !root.is_dir() {
        return Err(format!("JDK directory does not exist: {}", root.display()));
    }

    #[cfg(windows)]
    let (java_name, javac_name) = ("java.exe", "javac.exe");
    #[cfg(not(windows))]
    let (java_name, javac_name) = ("java", "javac");

    let java_bin = root.join("bin").join(java_name);
    let javac_bin = root.join("bin").join(javac_name);

    if !java_bin.is_file() {
        return Err(format!(
            "Missing bin/{} in JDK directory: {}",
            java_name,
            root.display()
        ));
    }
    if !javac_bin.is_file() {
        return Err(format!(
            "Missing bin/{} compiler in JDK directory: {}",
            javac_name,
            root.display()
        ));
    }

    Ok(root)
}

/// Match property keys that represent a Java Home directory across all known Ant filter templates.
pub fn is_java_home_property_key(key: &str) -> bool {
    let k = key.trim().to_lowercase();
    k == "java_home"
        || k == "javahome"
        || k.contains("javahome")
        || k.contains("java_home")
        || k.contains("java.home")
        || k.ends_with(".jdk.home")
        || k.ends_with(".jdkhome")
}

/// Resolve effective JAVA_HOME directory prioritized by active app/system config and validated with javac.
pub fn resolve_active_java_home(java_home_opt: Option<&str>) -> Option<String> {
    // Sync process environment with persisted history.json & Windows HKCU\Environment first
    crate::modules::environment::apply_persisted_environment();

    if let Some(jh) = java_home_opt {
        if let Ok(valid) = validate_java_home_dir(jh) {
            return Some(valid.to_string_lossy().to_string());
        }
    }

    let settings = crate::modules::settings::load_settings();
    if let Some(ref persisted_jh) = settings.java_home {
        if let Ok(valid) = validate_java_home_dir(persisted_jh) {
            return Some(valid.to_string_lossy().to_string());
        }
    }

    if let Ok(env_jh) = std::env::var("JAVA_HOME") {
        if let Ok(valid) = validate_java_home_dir(&env_jh) {
            return Some(valid.to_string_lossy().to_string());
        }
    }

    // Check effective java executable resolved by PATH
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let mut cmd = Command::new("cmd.exe");
        cmd.args(["/c", "where java"]);
        cmd.creation_flags(CREATE_NO_WINDOW);
        if let Ok(out) = cmd.output() {
            if out.status.success() {
                let text = decode_console_bytes(&out.stdout);
                for line in text.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() || trimmed.to_lowercase().contains("javapath") {
                        continue;
                    }
                    if let Ok(valid) = validate_java_home_dir(trimmed) {
                        return Some(valid.to_string_lossy().to_string());
                    }
                }
            }
        }
    }

    let scan = crate::modules::jdk_manager::scan_available_jdks();
    if let Some(rec) = scan
        .jdks
        .iter()
        .find(|j| j.is_recommended && j.javac_executable.is_some())
    {
        if let Ok(valid) = validate_java_home_dir(&rec.root_path) {
            return Some(valid.to_string_lossy().to_string());
        }
    }
    if let Some(any_jdk) = scan.jdks.iter().find(|j| j.javac_executable.is_some()) {
        if let Ok(valid) = validate_java_home_dir(&any_jdk.root_path) {
            return Some(valid.to_string_lossy().to_string());
        }
    }
    None
}

/// Convert path segments that contain spaces or non-ASCII characters to Windows 8.3 short names.
/// Pure-ASCII, space-free segments stay readable in full form.
pub fn to_batch_safe_path(path: &str) -> String {
    let clean = path.trim().trim_matches('"');
    if clean.is_empty() {
        return String::new();
    }

    #[cfg(windows)]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use std::path::Component;

        #[link(name = "kernel32")]
        extern "system" {
            fn GetShortPathNameW(
                lpszLongPath: *const u16,
                lpszShortPath: *mut u16,
                cchBuffer: u32,
            ) -> u32;
        }

        fn query_short_path(target: &Path) -> Option<PathBuf> {
            let wide: Vec<u16> = OsStr::new(target.as_os_str())
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            unsafe {
                let needed = GetShortPathNameW(wide.as_ptr(), std::ptr::null_mut(), 0);
                if needed == 0 {
                    return None;
                }
                let mut buf = vec![0u16; needed as usize];
                let written = GetShortPathNameW(wide.as_ptr(), buf.as_mut_ptr(), needed);
                if written == 0 {
                    return None;
                }
                let end = buf.iter().position(|&c| c == 0).unwrap_or(written as usize);
                let s = String::from_utf16_lossy(&buf[..end]);
                if s.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(s))
                }
            }
        }

        let clean_win = clean.replace('/', "\\");
        let mut real_prefix = PathBuf::new();
        let mut safe_path = PathBuf::new();

        for comp in Path::new(&clean_win).components() {
            match comp {
                Component::Prefix(p) => {
                    real_prefix.push(p.as_os_str());
                    safe_path.push(p.as_os_str());
                }
                Component::RootDir => {
                    real_prefix.push(comp.as_os_str());
                    safe_path.push(comp.as_os_str());
                }
                Component::CurDir => {
                    real_prefix.push(".");
                    safe_path.push(".");
                }
                Component::ParentDir => {
                    real_prefix.push("..");
                    safe_path.push("..");
                }
                Component::Normal(seg) => {
                    real_prefix.push(seg);
                    let seg_str = seg.to_string_lossy();
                    if seg_str.is_ascii() && !seg_str.bytes().any(|b| b.is_ascii_whitespace()) {
                        safe_path.push(seg);
                    } else {
                        let short_probe = if safe_path.as_os_str().is_empty() {
                            real_prefix.clone()
                        } else {
                            safe_path.join(seg)
                        };
                        let resolved = query_short_path(&short_probe)
                            .or_else(|| query_short_path(&real_prefix));
                        if let Some(short_full) = resolved {
                            if let Some(short_seg) = short_full.file_name() {
                                let short_str = short_seg.to_string_lossy();
                                if short_str.is_ascii()
                                    && !short_str.bytes().any(|b| b.is_ascii_whitespace())
                                {
                                    safe_path.push(short_seg);
                                } else {
                                    safe_path.push(seg);
                                }
                            } else {
                                safe_path.push(seg);
                            }
                        } else {
                            safe_path.push(seg);
                        }
                    }
                }
            }
        }

        let out = safe_path.to_string_lossy().to_string();
        if out.is_empty() {
            clean_win
        } else {
            out
        }
    }

    #[cfg(not(windows))]
    {
        clean.to_string()
    }
}

/// Expand any Windows 8.3 short (`~1`) segments back to their full readable long path for display.
pub fn to_display_long_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        #[link(name = "kernel32")]
        extern "system" {
            fn GetLongPathNameW(
                lpszShortPath: *const u16,
                lpszLongPath: *mut u16,
                cchBuffer: u32,
            ) -> u32;
        }

        let wide: Vec<u16> = OsStr::new(path.as_os_str())
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let needed = GetLongPathNameW(wide.as_ptr(), std::ptr::null_mut(), 0);
            if needed == 0 {
                return path.to_path_buf();
            }
            let mut buf = vec![0u16; needed as usize];
            let written = GetLongPathNameW(wide.as_ptr(), buf.as_mut_ptr(), needed);
            if written == 0 {
                return path.to_path_buf();
            }
            let end = buf.iter().position(|&c| c == 0).unwrap_or(written as usize);
            let s = String::from_utf16_lossy(&buf[..end]);
            if s.is_empty() {
                path.to_path_buf()
            } else {
                PathBuf::from(s)
            }
        }
    }

    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

/// Expand an existing or non-existent path by resolving its deepest existing ancestor to a long path.
pub fn resolve_long_path_best_effort(path: &Path) -> PathBuf {
    if path.exists() {
        return to_display_long_path(path);
    }

    let mut tail_parts: Vec<std::ffi::OsString> = Vec::new();
    let mut cursor = path;
    while !cursor.exists() {
        if let Some(name) = cursor.file_name() {
            tail_parts.push(name.to_os_string());
        } else {
            break;
        }
        if let Some(parent) = cursor.parent() {
            cursor = parent;
        } else {
            break;
        }
    }

    let mut base = if cursor.exists() {
        to_display_long_path(cursor)
    } else {
        cursor.to_path_buf()
    };

    for part in tail_parts.into_iter().rev() {
        base.push(part);
    }
    base
}

/// Check whether `candidate` resides inside `root` (or equals `root`), accounting for 8.3 short names.
pub fn is_path_within_dir(candidate: &Path, root: &Path) -> bool {
    let cand_long = resolve_long_path_best_effort(candidate);
    let root_long = resolve_long_path_best_effort(root);
    let cand_key = normalize_path_key(&cand_long);
    let root_key = normalize_path_key(&root_long);
    if root_key.is_empty() || cand_key.is_empty() {
        return false;
    }
    cand_key == root_key || cand_key.starts_with(&format!("{}\\", root_key))
}

/// Check and fix javac executable path in build.xml if invalid or mismatched (encoding-safe).
pub fn fix_build_xml_java_path(build_dir: &Path, active_java_home: &str) -> Option<String> {
    let valid_java_home = validate_java_home_dir(active_java_home).ok()?;
    let build_xml_path = build_dir.join("build.xml");
    let bytes = fs::read(&build_xml_path).ok()?;
    let (content, orig_encoding) = detect_and_decode_text(&bytes);
    let save_encoding = detect_xml_declared_encoding(&content, orig_encoding);

    let safe_java_home = to_batch_safe_path(&valid_java_home.to_string_lossy());
    let desired_javac = Path::new(&safe_java_home)
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
                let cur_exists =
                    cur_path.is_file() || Path::new(&format!("{}.exe", cur_exe)).is_file();

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
        let line_ending = if content.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let mut final_text = updated_lines.join(line_ending);
        if content.ends_with('\n') {
            final_text.push_str(line_ending);
        }
        let encoded = encode_text(&final_text, save_encoding)
            .or_else(|_| encode_text(&final_text, TextEncoding::Utf8))
            .unwrap_or_else(|_| final_text.as_bytes().to_vec());
        let _ = fs::write(&build_xml_path, encoded);
    }

    changed_to
}

fn lexical_normalize_path(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::Prefix(p) => out.push(p.as_os_str()),
            Component::RootDir => out.push(comp.as_os_str()),
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
            Component::Normal(seg) => out.push(seg),
        }
    }
    out
}

fn format_display_windows_path(path: &Path) -> String {
    let s = path.to_string_lossy().replace('/', "\\");
    let is_drive_root = s.len() == 3 && s.as_bytes().get(1) == Some(&b':') && s.ends_with('\\');
    if is_drive_root {
        s
    } else if s.starts_with(r"\\") {
        let rest = s.trim_end_matches('\\');
        if rest.len() > 2 {
            rest.to_string()
        } else {
            s
        }
    } else {
        s.trim_end_matches('\\').to_string()
    }
}

/// Inspect `build.xml` to identify the target output property (`target.dir` / `TARGET_HOME`),
/// compiled classes property (`build.dir`), resolved default target path, and optional `env.dir`.
pub fn detect_ant_target_properties(
    project_path: &str,
    target: Option<&str>,
) -> AntTargetProperties {
    let clean_proj = project_path.trim().trim_matches('"').replace('/', "\\");
    let base_proj = Path::new(&clean_proj);
    let fallback_default =
        format_display_windows_path(&lexical_normalize_path(&base_proj.join("target")));

    let mut target_prop_name = "target.dir".to_string();
    let mut build_classes_prop_name = "build.dir".to_string();
    let mut default_target_path = fallback_default;
    let mut env_dir: Option<String> = None;

    if let Some(build_dir) = find_build_dir(&clean_proj) {
        let build_xml_path = build_dir.join("build.xml");
        if let Ok(bytes) = fs::read(&build_xml_path) {
            let (content, _) = detect_and_decode_text(&bytes);
            let mut in_any_target = false;
            let mut in_requested_target = false;
            let mut matched_target_prop = false;

            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("<!--") {
                    continue;
                }
                if trimmed.starts_with("<target") {
                    in_any_target = true;
                    if let Some(req_tgt) = target {
                        let t_clean = req_tgt.trim();
                        if !t_clean.is_empty()
                            && extract_xml_attribute(trimmed, "name")
                                .map(|n| n.eq_ignore_ascii_case(t_clean))
                                .unwrap_or(false)
                        {
                            in_requested_target = true;
                        }
                    }
                    continue;
                }
                if trimmed.starts_with("</target>") {
                    in_any_target = false;
                    in_requested_target = false;
                    continue;
                }

                if trimmed.contains("<property") {
                    if let (Some(name), Some(val)) = (
                        extract_xml_attribute(trimmed, "name"),
                        extract_xml_attribute(trimmed, "value"),
                    ) {
                        let name_trimmed = name.trim();
                        let name_lower = name_trimmed.to_lowercase();
                        let val_trimmed = val.trim();
                        let val_norm_slash = val_trimmed
                            .replace('\\', "/")
                            .trim_end_matches('/')
                            .to_string();
                        let val_lower = val_norm_slash.to_lowercase();

                        if !in_any_target {
                            let is_known_target_name = matches!(
                                name_lower.as_str(),
                                "target.dir"
                                    | "target_home"
                                    | "target.home"
                                    | "dist.dir"
                                    | "target.root"
                            );
                            let is_target_val = !matched_target_prop
                                && !name_lower.contains("build")
                                && !name_lower.contains("class")
                                && (val_lower == "target" || val_lower.ends_with("/target"));

                            if is_known_target_name || is_target_val {
                                target_prop_name = name_trimmed.to_string();
                                matched_target_prop = true;
                                if !val_trimmed.contains("${") && !val_trimmed.is_empty() {
                                    let val_win = val_trimmed.replace('/', "\\");
                                    let p = Path::new(&val_win);
                                    let resolved = if p.is_absolute() {
                                        lexical_normalize_path(p)
                                    } else {
                                        lexical_normalize_path(&build_dir.join(p))
                                    };
                                    default_target_path = format_display_windows_path(&resolved);
                                }
                            } else if name_lower == "build.dir"
                                || val_lower.ends_with("/target/classes")
                                || val_lower.contains("}/classes")
                            {
                                build_classes_prop_name = name_trimmed.to_string();
                            }
                        } else if in_requested_target && name_lower == "env.dir" {
                            let cleaned_env =
                                val_trimmed.replace('\\', "/").trim_matches('/').to_string();
                            if !cleaned_env.is_empty() {
                                env_dir = Some(cleaned_env);
                            }
                        }
                    }
                }
            }
        }
    }

    AntTargetProperties {
        target_prop_name,
        build_classes_prop_name,
        default_target_path,
        env_dir,
    }
}

/// Resolve `(effective_target_dir, default_target_dir, is_default_target)`.
/// Falls back cleanly to `default_target_dir` when `custom_target_path` is empty or matches default.
pub fn resolve_effective_target_dir(
    project_path: &str,
    target: Option<&str>,
    custom_target_path: Option<&str>,
) -> (PathBuf, PathBuf, bool) {
    let ant_props = detect_ant_target_properties(project_path, target);
    let default_pb = lexical_normalize_path(Path::new(&ant_props.default_target_path));

    let Some(raw_custom) = custom_target_path else {
        return (default_pb.clone(), default_pb, true);
    };
    let trimmed = raw_custom.trim().trim_matches('"');
    if trimmed.is_empty() {
        return (default_pb.clone(), default_pb, true);
    }

    let custom_pb = lexical_normalize_path(Path::new(&trimmed.replace('/', "\\")));
    let custom_long = resolve_long_path_best_effort(&custom_pb);
    let default_long = resolve_long_path_best_effort(&default_pb);

    if paths_equal_ci(&custom_pb, &default_pb) || paths_equal_ci(&custom_long, &default_long) {
        (default_pb.clone(), default_pb, true)
    } else {
        (custom_pb, default_pb, false)
    }
}

fn find_source_set_batch_path(base_proj: &Path) -> Option<PathBuf> {
    let direct_candidates = [
        base_proj.join("batch").join("SetBatchPath.bat"),
        base_proj.join("batch").join("SetBatchPath.bat.filter"),
        base_proj
            .join("batch")
            .join("batfiles")
            .join("SetBatchPath.bat.filter"),
        base_proj
            .join("batch")
            .join("batfiles")
            .join("SetBatchPath.bat"),
    ];
    for c in direct_candidates {
        if c.is_file() {
            return Some(c);
        }
    }

    let batch_dir = base_proj.join("batch");
    if batch_dir.is_dir() {
        for entry in walkdir::WalkDir::new(&batch_dir)
            .max_depth(4)
            .into_iter()
            .flatten()
        {
            if entry.file_type().is_file() {
                let lower = entry.file_name().to_string_lossy().to_lowercase();
                if lower == "setbatchpath.bat" || lower == "setbatchpath.bat.filter" {
                    return Some(entry.path().to_path_buf());
                }
            }
        }
    }
    None
}

fn expand_cmd_percent_vars(
    line: &str,
    vars: &std::collections::HashMap<String, String>,
    undefined_to_empty: bool,
) -> String {
    let mut out = String::with_capacity(line.len());
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '%' {
            if let Some(rel_end) = chars[i + 1..].iter().position(|&c| c == '%') {
                if rel_end == 0 {
                    out.push('%');
                    i += 2;
                    continue;
                }
                let var_name: String = chars[i + 1..i + 1 + rel_end].iter().collect();
                if let Some(val) = vars.get(&var_name.to_lowercase()) {
                    out.push_str(val);
                } else if !undefined_to_empty {
                    out.push('%');
                    out.push_str(&var_name);
                    out.push('%');
                }
                i += rel_end + 2;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }

    out
}

fn extract_windows_drive_prefix(path: &Path) -> Option<String> {
    use std::path::Component;
    for comp in path.components() {
        if let Component::Prefix(p) = comp {
            let s = p.as_os_str().to_string_lossy().to_string();
            if s.len() == 2 && s.ends_with(':') {
                return Some(format!("{}\\", s));
            }
        }
    }
    None
}

fn is_protected_windows_directory(cand: &Path) -> bool {
    let cand_norm = resolve_long_path_best_effort(cand);
    let mut protected: Vec<PathBuf> = vec![
        PathBuf::from(r"C:\"),
        PathBuf::from(r"C:\Windows"),
        PathBuf::from(r"C:\Program Files"),
        PathBuf::from(r"C:\Program Files (x86)"),
        PathBuf::from(r"C:\Users"),
    ];
    for var in [
        "WINDIR",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "ProgramData",
    ] {
        if let Ok(v) = std::env::var(var) {
            if !v.trim().is_empty() {
                let p = PathBuf::from(v.trim());
                protected.push(p.clone());
                if var == "USERPROFILE" {
                    protected.push(p.join("Desktop"));
                    protected.push(p.join("Documents"));
                }
            }
        }
    }
    protected.iter().any(|p| paths_equal_ci(&cand_norm, p))
}

fn check_target_path_safety(
    effective_pb: &Path,
    effective_str: &str,
    batch_safe_target_path: &str,
    base_proj: &Path,
    is_default_target: bool,
    errors: &mut Vec<String>,
) {
    let trimmed = effective_str.trim();
    if trimmed.is_empty() {
        errors.push("Target build path cannot be empty.".to_string());
        return;
    }

    if !effective_pb.is_absolute() {
        errors.push(format!(
            "Target build path must be an absolute path (for example, C:\\bh_target\\project): '{}'.",
            effective_str
        ));
        return;
    }

    // Check normal path segments for illegal Windows filename chars or risky CMD metacharacters
    let mut seg_idx = 0usize;
    for comp in effective_pb.components() {
        if let std::path::Component::Normal(os_seg) = comp {
            let seg = os_seg.to_string_lossy();
            let bad_win_char = seg
                .chars()
                .find(|c| matches!(c, '<' | '>' | '"' | '|' | '?' | '*') || c.is_control());
            if let Some(ch) = bad_win_char {
                errors.push(format!(
                    "Target build path contains invalid Windows filename character '{}' in '{}'.",
                    ch, seg
                ));
                break;
            }
            if !is_default_target {
                let bad_cmd_char = seg
                    .chars()
                    .find(|c| matches!(c, '&' | '^' | '%' | '!' | ';' | '='));
                if let Some(ch) = bad_cmd_char {
                    errors.push(format!(
                        "Custom target build path contains CMD shell metacharacter '{}' in '{}', which can break Ant/BAT argument parsing.",
                        ch, seg
                    ));
                    break;
                }
            }
            seg_idx += 1;
        }
    }

    if seg_idx == 0 {
        errors.push(format!(
            "Target build path cannot be a drive or share root ('{}') because Ant build runs <delete dir=\"${{target.dir}}\" />.",
            effective_str
        ));
    }

    if effective_pb.is_file() {
        errors.push(format!(
            "Target build path points to an existing file instead of a directory: '{}'.",
            effective_str
        ));
    }

    #[cfg(windows)]
    if let Some(drive_root) = extract_windows_drive_prefix(effective_pb) {
        if !Path::new(&drive_root).exists() {
            errors.push(format!(
                "Drive '{}' does not exist on this system.",
                drive_root
            ));
        }
    }

    if is_protected_windows_directory(effective_pb) {
        errors.push(format!(
            "Target build path '{}' points to a protected Windows/system directory and cannot be used as Ant target output.",
            effective_str
        ));
    }

    let proj_resolved = resolve_long_path_best_effort(base_proj);
    let eff_resolved = resolve_long_path_best_effort(effective_pb);
    if is_path_within_dir(&proj_resolved, &eff_resolved) {
        errors.push(format!(
            "Target build path '{}' cannot be the project root or an ancestor folder because Ant <delete> would erase project files.",
            effective_str
        ));
    }

    for protected_sub in ["src", "build", "batch"] {
        if is_path_within_dir(&eff_resolved, &proj_resolved.join(protected_sub)) {
            errors.push(format!(
                "Target build path '{}' cannot point inside source folder '{}'.",
                effective_str, protected_sub
            ));
        }
    }

    if !is_default_target
        && (batch_safe_target_path.contains(' ') || !batch_safe_target_path.is_ascii())
    {
        errors.push(format!(
            "Custom target build path '{}' contains spaces or non-ASCII characters (batch-safe: '{}'). Choose a short ASCII path without spaces (e.g. C:\\bh_target\\batch1) so unquoted %BATCH_HOME% paths in Windows .bat scripts execute safely.",
            effective_str, batch_safe_target_path
        ));
    }
}

fn strip_case_insensitive_suffix<'a>(s: &'a str, suffix: &str) -> &'a str {
    let s_len = s.len();
    let suf_len = suffix.len();
    if s_len >= suf_len
        && s.is_char_boundary(s_len - suf_len)
        && s[s_len - suf_len..].eq_ignore_ascii_case(suffix)
    {
        &s[..s_len - suf_len]
    } else {
        s
    }
}

fn estimate_longest_output_suffix(
    base_proj: &Path,
    env_dir: Option<&str>,
    artifact_id: &str,
) -> usize {
    let env_prefix_len = env_dir
        .map(|ed| ed.trim_matches('/'))
        .filter(|s| !s.is_empty())
        .map(|s| s.encode_utf16().count() + 1)
        .unwrap_or(0);

    let mut max_env_rel: usize = "batch/SetBatchPath.bat.filter"
        .len()
        .max("log/stclibApp.log".len());
    max_env_rel = max_env_rel.max(format!("{}.jar", artifact_id).encode_utf16().count());

    let batch_dir = base_proj.join("batch");
    if batch_dir.is_dir() {
        for entry in walkdir::WalkDir::new(&batch_dir).into_iter().flatten() {
            if entry.file_type().is_file() {
                if let Ok(rel) = entry.path().strip_prefix(&batch_dir) {
                    let rel_str = rel.to_string_lossy().replace('\\', "/");
                    let lower = rel_str.to_lowercase();
                    if lower.starts_with("classes/")
                        || lower.starts_with("trush/")
                        || lower.starts_with("table_sql_oracle/")
                    {
                        continue;
                    }
                    let rel_len = if lower.starts_with("lib/") {
                        // Copied directly to <target>/<env_dir>/lib/<name>
                        rel_str.encode_utf16().count()
                    } else if lower.ends_with(".filter") {
                        "batch/".len()
                            + strip_case_insensitive_suffix(&rel_str, ".filter")
                                .encode_utf16()
                                .count()
                    } else {
                        "batch/".len() + rel_str.encode_utf16().count()
                    };
                    max_env_rel = max_env_rel.max(rel_len);
                }
            }
        }
    }

    // Compiled classes are written directly under ${build.dir} (target/classes), without env.dir
    let mut max_classes_rel: usize = 0;
    let src_dir = base_proj.join("src");
    if src_dir.is_dir() {
        for entry in walkdir::WalkDir::new(&src_dir).into_iter().flatten() {
            if entry.file_type().is_file() {
                let is_java = entry
                    .path()
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("java"))
                    .unwrap_or(false);
                if !is_java {
                    continue;
                }
                if let Ok(rel) = entry.path().strip_prefix(&src_dir) {
                    let rel_str = rel.to_string_lossy().replace('\\', "/");
                    let lower = rel_str.to_lowercase();
                    let pkg_rel = if lower.starts_with("main/java/") {
                        &rel_str["main/java/".len()..]
                    } else if lower.starts_with("test/java/") {
                        &rel_str["test/java/".len()..]
                    } else {
                        &rel_str
                    };
                    let class_rel_len = strip_case_insensitive_suffix(pkg_rel, ".java")
                        .encode_utf16()
                        .count()
                        + ".class".len();
                    max_classes_rel = max_classes_rel.max("classes/".len() + class_rel_len);
                }
            }
        }
    }

    (max_env_rel + env_prefix_len).max(max_classes_rel)
}

/// Validate effective target build path BEFORE Ant runs, simulating Windows CMD `%VAR%` expansion
/// against `SetBatchPath.bat` / `.bat` scripts (8,191-char limit) and Windows `MAX_PATH` (260 chars).
pub fn validate_target_build_path(
    project_path: &str,
    target: &str,
    custom_target_path: Option<&str>,
    java_home: Option<&str>,
) -> TargetPathValidation {
    let clean_proj = project_path.trim().trim_matches('"').replace('/', "\\");
    let base_proj = Path::new(&clean_proj);
    let ant_props = detect_ant_target_properties(&clean_proj, Some(target));
    let (effective_pb, default_pb, is_default_target) =
        resolve_effective_target_dir(&clean_proj, Some(target), custom_target_path);

    let default_target_path = format_display_windows_path(&default_pb);
    let effective_target_path = format_display_windows_path(&effective_pb);

    let batch_safe_raw = to_batch_safe_path(&effective_target_path);
    let batch_safe_target_path = if batch_safe_raw.is_empty() {
        effective_target_path.replace('\\', "/")
    } else {
        batch_safe_raw
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_string()
    };

    let effective_target_len = effective_target_path.encode_utf16().count();
    let batch_safe_target_len = batch_safe_target_path.encode_utf16().count();

    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    check_target_path_safety(
        &effective_pb,
        &effective_target_path,
        &batch_safe_target_path,
        base_proj,
        is_default_target,
        &mut errors,
    );

    let prop_file =
        find_build_dir(&clean_proj).and_then(|bd| find_properties_for_target(&bd, target));

    let effective_java_raw = resolve_active_java_home(java_home)
        .or_else(|| {
            prop_file
                .as_ref()
                .and_then(|p| extract_property_value(p, "filter.javahome.baseDirectory.value"))
        })
        .or_else(|| {
            prop_file
                .as_ref()
                .and_then(|p| extract_property_value(p, "filter.shellscript.javaHome.value"))
        })
        .unwrap_or_else(|| "C:/jdk-25.0.2".to_string());
    let batch_safe_java_raw = to_batch_safe_path(&effective_java_raw);
    let effective_java_slash = if batch_safe_java_raw.is_empty() {
        effective_java_raw
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_string()
    } else {
        batch_safe_java_raw
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_string()
    };

    let artifact_id = prop_file
        .as_ref()
        .and_then(|p| extract_property_value(p, "artifactId"))
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            base_proj
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "batch-app".to_string());

    let mut sim_vars: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    sim_vars.insert("java_home".to_string(), effective_java_slash.clone());
    sim_vars.insert("batch_home".to_string(), batch_safe_target_path.clone());
    sim_vars.insert("batchf_home".to_string(), batch_safe_target_path.clone());

    let mut jar_reference_count: usize = 0;
    let mut peak_set_line_len: usize = 0;
    let mut set_over_line: Option<(usize, usize)> = None;
    let mut set_batch_name: Option<String> = None;

    if let Some(set_path) = find_source_set_batch_path(base_proj) {
        set_batch_name = set_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string());
        if let Ok(bytes) = fs::read(&set_path) {
            let (content, _) = detect_and_decode_text(&bytes);
            for (idx, raw_line) in content.lines().enumerate() {
                let trimmed = raw_line.trim();
                let lower = trimmed.to_lowercase();
                if trimmed.is_empty()
                    || lower == "rem"
                    || lower.starts_with("rem ")
                    || lower.starts_with("rem\t")
                    || trimmed.starts_with("::")
                {
                    continue;
                }

                let filtered = raw_line
                    .replace(
                        "@filter.shellscript.baseDirectory.value@",
                        &batch_safe_target_path,
                    )
                    .replace(
                        "@filter.javahome.baseDirectory.value@",
                        &effective_java_slash,
                    )
                    .replace("@filter.shellscript.javaHome.value@", &effective_java_slash)
                    .replace("@artifactId@", &artifact_id);

                if lower.contains("jreclasspath") && lower.contains("%batch_home%") {
                    jar_reference_count += filtered.to_lowercase().matches("%batch_home%").count();
                }

                let expanded = expand_cmd_percent_vars(&filtered, &sim_vars, true);
                let exp_len = expanded.chars().count();
                if exp_len > peak_set_line_len {
                    peak_set_line_len = exp_len;
                }
                if exp_len > WINDOWS_CMD_LINE_LIMIT && set_over_line.is_none() {
                    set_over_line = Some((idx + 1, exp_len));
                }

                let exp_trimmed = expanded.trim_start();
                if exp_trimmed.len() >= 4 && exp_trimmed[..3].eq_ignore_ascii_case("set") {
                    let after_set = &exp_trimmed[3..];
                    if after_set.starts_with(' ') || after_set.starts_with('\t') {
                        let assign = after_set.trim_start();
                        if let Some(eq_idx) = assign.find('=') {
                            let var_key = assign[..eq_idx].trim().to_lowercase();
                            let var_val = assign[eq_idx + 1..].to_string();
                            if !var_key.is_empty() {
                                sim_vars.insert(var_key, var_val);
                            }
                        }
                    }
                }
            }
        }
    }

    let mut peak_bat_line_len: usize = 0;
    let mut longest_bat_info: Option<(String, usize)> = None;
    let batch_dir = base_proj.join("batch");
    if batch_dir.is_dir() {
        for entry in walkdir::WalkDir::new(&batch_dir)
            .max_depth(4)
            .into_iter()
            .flatten()
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let fname = entry.file_name().to_string_lossy().to_string();
            let fname_lower = fname.to_lowercase();
            if !(fname_lower.ends_with(".bat") || fname_lower.ends_with(".bat.filter")) {
                continue;
            }
            if fname_lower.starts_with("setbatchpath.bat") {
                continue;
            }
            if let Ok(bytes) = fs::read(entry.path()) {
                let (content, _) = detect_and_decode_text(&bytes);
                let mut local_vars = sim_vars.clone();
                for (idx, raw_line) in content.lines().enumerate() {
                    let trimmed = raw_line.trim();
                    let lower = trimmed.to_lowercase();
                    if trimmed.is_empty()
                        || lower == "rem"
                        || lower.starts_with("rem ")
                        || lower.starts_with("rem\t")
                        || trimmed.starts_with("::")
                    {
                        continue;
                    }
                    let filtered = raw_line
                        .replace(
                            "@filter.shellscript.baseDirectory.value@",
                            &batch_safe_target_path,
                        )
                        .replace(
                            "@filter.javahome.baseDirectory.value@",
                            &effective_java_slash,
                        )
                        .replace("@filter.shellscript.javaHome.value@", &effective_java_slash)
                        .replace("@artifactId@", &artifact_id);
                    let expanded = expand_cmd_percent_vars(&filtered, &local_vars, true);
                    let exp_len = expanded.chars().count();
                    if exp_len > peak_bat_line_len {
                        peak_bat_line_len = exp_len;
                        longest_bat_info = Some((fname.clone(), idx + 1));
                    }
                    let exp_trimmed = expanded.trim_start();
                    if exp_trimmed.len() >= 4 && exp_trimmed[..3].eq_ignore_ascii_case("set") {
                        let after_set = &exp_trimmed[3..];
                        if after_set.starts_with(' ') || after_set.starts_with('\t') {
                            let assign = after_set.trim_start();
                            if let Some(eq_idx) = assign.find('=') {
                                let var_key = assign[..eq_idx].trim().to_lowercase();
                                let var_val = assign[eq_idx + 1..].to_string();
                                if !var_key.is_empty() {
                                    local_vars.insert(var_key, var_val);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let estimated_cmd_line_len = peak_set_line_len.max(peak_bat_line_len);
    let mut cmd_max_safe_target_len: Option<usize> = None;
    if jar_reference_count > 0 {
        let var_term = jar_reference_count.saturating_mul(batch_safe_target_len);
        if estimated_cmd_line_len >= var_term {
            let fixed_overhead = estimated_cmd_line_len - var_term;
            if WINDOWS_CMD_LINE_LIMIT > fixed_overhead {
                cmd_max_safe_target_len =
                    Some((WINDOWS_CMD_LINE_LIMIT - fixed_overhead) / jar_reference_count);
            } else {
                cmd_max_safe_target_len = Some(0);
            }
        }
    }

    let longest_rel_suffix =
        estimate_longest_output_suffix(base_proj, ant_props.env_dir.as_deref(), &artifact_id);
    let estimated_longest_file_path_len = effective_target_len + 1 + longest_rel_suffix;
    let max_path_safe_target_len =
        (WINDOWS_MAX_PATH_LIMIT - 1).saturating_sub(1 + longest_rel_suffix);

    let max_safe_target_len = match cmd_max_safe_target_len {
        Some(cmd_cap) => Some(cmd_cap.min(max_path_safe_target_len)),
        None => Some(max_path_safe_target_len),
    };

    let mut offending_script: Option<String> = None;
    let mut offending_line_number: Option<usize> = None;

    if estimated_cmd_line_len > WINDOWS_CMD_LINE_LIMIT {
        if let Some((line_no, line_len)) = set_over_line {
            offending_script = set_batch_name.clone();
            offending_line_number = Some(line_no);
            errors.push(format!(
                "Target path exceeds Windows CMD 8,191-character command line limit: {} expands %BATCH_HOME% across {} JAR entries (Line {} reaches {} chars; peak BAT invocation reaches {} chars > {} limit). Max safe Target Dir length is ~{} chars (current batch-safe length: {} chars).",
                set_batch_name.as_deref().unwrap_or("SetBatchPath.bat"),
                jar_reference_count,
                line_no,
                line_len,
                estimated_cmd_line_len,
                WINDOWS_CMD_LINE_LIMIT,
                max_safe_target_len.unwrap_or(0),
                batch_safe_target_len
            ));
        } else {
            if let Some((bname, bline)) = longest_bat_info {
                offending_script = Some(bname.clone());
                offending_line_number = Some(bline);
                errors.push(format!(
                    "Target path exceeds Windows CMD 8,191-character limit in '{}' line {} ({} chars > {} limit; {} %BATCH_HOME% JAR entries). Max safe Target Dir length is ~{} chars.",
                    bname,
                    bline,
                    estimated_cmd_line_len,
                    WINDOWS_CMD_LINE_LIMIT,
                    jar_reference_count,
                    max_safe_target_len.unwrap_or(0)
                ));
            } else {
                errors.push(format!(
                    "Target path produces a {} character CMD line (limit: {}). Choose a shorter Target Dir.",
                    estimated_cmd_line_len, WINDOWS_CMD_LINE_LIMIT
                ));
            }
        }
    } else if estimated_cmd_line_len >= WINDOWS_CMD_LINE_WARN {
        warnings.push(format!(
            "Estimated BAT command line is {} / {} chars ({} %BATCH_HOME% JAR entries). Consider a shorter Target Dir to keep safe headroom.",
            estimated_cmd_line_len, WINDOWS_CMD_LINE_LIMIT, jar_reference_count
        ));
    }

    if estimated_longest_file_path_len >= WINDOWS_MAX_PATH_LIMIT {
        errors.push(format!(
            "Longest generated file path under Target Dir would reach {} chars, exceeding Windows MAX_PATH (260 chars). Choose a Target Dir of at most {} chars.",
            estimated_longest_file_path_len, max_path_safe_target_len
        ));
    } else if estimated_longest_file_path_len >= WINDOWS_MAX_PATH_WARN {
        warnings.push(format!(
            "Longest generated output path approaches Windows MAX_PATH ({} / 260 chars).",
            estimated_longest_file_path_len
        ));
    }

    let is_valid = errors.is_empty();
    let severity = if !is_valid {
        "error".to_string()
    } else if !warnings.is_empty() {
        "warning".to_string()
    } else {
        "ok".to_string()
    };

    let summary = if !errors.is_empty() {
        errors.join("\n")
    } else if !warnings.is_empty() {
        warnings.join("\n")
    } else if estimated_cmd_line_len > 0 {
        format!(
            "Target path is safe (est. CMD line: {} / {} chars; max output path: {} / 260 chars).",
            estimated_cmd_line_len, WINDOWS_CMD_LINE_LIMIT, estimated_longest_file_path_len
        )
    } else {
        format!(
            "Target path is safe (max output path: {} / 260 chars).",
            estimated_longest_file_path_len
        )
    };

    TargetPathValidation {
        is_valid,
        is_default_target,
        default_target_path,
        effective_target_path,
        batch_safe_target_path,
        effective_target_len,
        batch_safe_target_len,
        cmd_limit: WINDOWS_CMD_LINE_LIMIT,
        max_path_limit: WINDOWS_MAX_PATH_LIMIT,
        estimated_cmd_line_len,
        estimated_longest_file_path_len,
        max_safe_target_len,
        offending_script,
        offending_line_number,
        jar_reference_count,
        severity,
        errors,
        warnings,
        summary,
    }
}

/// Check and fix project paths, optional custom target build path, and (optionally) Java Home keys
/// in the target's `.properties` file. Preserves default behavior when `custom_target_path` is `None`.
fn update_properties_file_paths(
    prop_file: &Path,
    project_path: &str,
    active_java_home: Option<&str>,
    custom_target_path: Option<&str>,
    update_java: bool,
) -> bool {
    let bytes = match fs::read(prop_file) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let (content, orig_encoding) = detect_and_decode_text(&bytes);

    let safe_proj_raw = to_batch_safe_path(project_path)
        .trim()
        .trim_matches('"')
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();
    let proj_slash = escape_properties_value(&safe_proj_raw);

    let effective_target_slash = match custom_target_path
        .map(|s| s.trim().trim_matches('"'))
        .filter(|s| !s.is_empty())
    {
        Some(custom_raw) => {
            let safe_t = to_batch_safe_path(custom_raw)
                .trim()
                .trim_matches('"')
                .replace('\\', "/")
                .trim_end_matches('/')
                .to_string();
            escape_properties_value(&safe_t)
        }
        None => format!("{}/target", proj_slash),
    };

    let java_slash = if update_java {
        active_java_home
            .and_then(|jh| validate_java_home_dir(jh).ok())
            .map(|valid_jh| {
                let safe_jh = to_batch_safe_path(&valid_jh.to_string_lossy())
                    .trim()
                    .trim_matches('"')
                    .replace('\\', "/")
                    .trim_end_matches('/')
                    .to_string();
                escape_properties_value(&safe_jh)
            })
    } else {
        None
    };

    // Detect old target root and old project root prefix from baseDirectory or pom.log4j.path.value
    let mut old_proj_prefix: Option<String> = None;
    let mut old_target_root: Option<String> = None;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.starts_with('!') {
            continue;
        }
        if let Some(idx) = trimmed.find('=') {
            let key = trimmed[..idx].trim();
            let val_raw_un = unescape_properties_value(trimmed[idx + 1..].trim());
            let val = val_raw_un.replace('\\', "/");
            if key == "filter.shellscript.baseDirectory.value" {
                let clean_val = val.trim_end_matches('/');
                if !clean_val.is_empty() {
                    old_target_root = Some(clean_val.to_string());
                }
                if clean_val.to_lowercase().ends_with("/target") {
                    let prefix_len = clean_val.len() - "/target".len();
                    old_proj_prefix = Some(clean_val[..prefix_len].to_string());
                }
                break;
            } else if key == "pom.log4j.path.value" {
                let clean_val = val.trim_end_matches('/');
                if old_target_root.is_none() && clean_val.to_lowercase().ends_with("/log") {
                    let root_len = clean_val.len() - "/log".len();
                    old_target_root = Some(clean_val[..root_len].to_string());
                }
                let val_lower = val.to_lowercase();
                if old_proj_prefix.is_none() {
                    if let Some(t_idx) = val_lower.find("/target/") {
                        if val.is_char_boundary(t_idx) {
                            old_proj_prefix = Some(val[..t_idx].to_string());
                        }
                    }
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

            if update_java && is_java_home_property_key(key) {
                if let Some(ref js) = java_slash {
                    if val != js {
                        new_val = Some(js.clone());
                    }
                }
            } else if key == "filter.shellscript.baseDirectory.value" {
                if val != effective_target_slash {
                    new_val = Some(effective_target_slash.clone());
                }
            } else if key == "pom.log4j.path.value" {
                let desired = format!("{}/log/", effective_target_slash);
                if val != desired {
                    new_val = Some(desired);
                }
            } else if key == "filter.resources.propertiesLocation" {
                let desired = format!("{}/batch/properties", effective_target_slash);
                if val != desired {
                    new_val = Some(desired);
                }
            } else if key == "filter.common.errmailBatchPath.value"
                || key == "filter.common.errmailWebPath.value"
            {
                let desired = format!("{}/batch/properties/mdware_xml", effective_target_slash);
                if val != desired {
                    new_val = Some(desired);
                }
            } else if !is_java_home_property_key(key) {
                let val_decoded = unescape_properties_value(val);
                let val_norm = val_decoded.replace('\\', "/");
                let val_lower = val_norm.to_lowercase();

                if let Some(ref old_t) = old_target_root {
                    let old_t_lower = old_t.to_lowercase();
                    if !old_t.is_empty()
                        && val_lower.starts_with(&old_t_lower)
                        && val_norm.is_char_boundary(old_t.len())
                    {
                        let rest = &val_norm[old_t.len()..];
                        if rest.is_empty() || rest.starts_with('/') {
                            let desired = format!(
                                "{}{}",
                                effective_target_slash,
                                escape_properties_value(rest)
                            );
                            if val != desired {
                                new_val = Some(desired);
                            }
                        }
                    }
                }

                if new_val.is_none() {
                    if let Some(t_idx) = val_lower.find("/target/") {
                        if val_norm.is_char_boundary(t_idx) {
                            let after_target = &val_norm[t_idx + "/target".len()..];
                            let suffix = escape_properties_value(after_target);
                            let desired = format!("{}{}", effective_target_slash, suffix);
                            if val != desired {
                                new_val = Some(desired);
                            }
                        }
                    } else if let Some(ref old_p) = old_proj_prefix {
                        if !old_p.is_empty() {
                            let old_lower = old_p.to_lowercase();
                            if val_lower.starts_with(&old_lower)
                                && val_norm.is_char_boundary(old_p.len())
                            {
                                let rest = &val_norm[old_p.len()..];
                                if rest.is_empty() || rest.starts_with('/') {
                                    let suffix = escape_properties_value(rest);
                                    let desired = format!("{}{}", proj_slash, suffix);
                                    if val != desired {
                                        new_val = Some(desired);
                                    }
                                }
                            } else if update_java && !val.is_ascii() {
                                let escaped = escape_properties_value(val);
                                if val != escaped {
                                    new_val = Some(escaped);
                                }
                            }
                        }
                    } else if update_java && !val.is_ascii() {
                        let escaped = escape_properties_value(val);
                        if val != escaped {
                            new_val = Some(escaped);
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
        let line_ending = if content.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let mut final_text = updated_lines.join(line_ending);
        if content.ends_with('\n') {
            final_text.push_str(line_ending);
        }
        let encoded = encode_text(&final_text, orig_encoding)
            .or_else(|_| encode_text(&final_text, TextEncoding::Utf8))
            .unwrap_or_else(|_| final_text.as_bytes().to_vec());
        let _ = fs::write(prop_file, encoded);
    }

    changed
}

/// Check and fix project paths & all Java Home keys in the target's .properties file.
/// Uses Windows 8.3 short paths for space/non-ASCII segments and `\uXXXX` escaping so Java's
/// `Properties.load(InputStream)` & Ant `<filterset>` never corrupt Japanese characters into `????`.
pub fn fix_properties_file_paths(
    prop_file: &Path,
    project_path: &str,
    active_java_home: Option<&str>,
) -> bool {
    update_properties_file_paths(prop_file, project_path, active_java_home, None, true)
}

/// Dynamically synchronize target-dependent path keys in a target `.properties` file
/// without modifying Java Home or unrelated properties.
pub fn sync_properties_target_path(
    prop_file: &Path,
    project_path: &str,
    custom_target_path: Option<&str>,
) -> bool {
    update_properties_file_paths(prop_file, project_path, None, custom_target_path, false)
}

/// Configure and persist custom target build path for a project, validate it, and sync `.properties`
/// when a non-default path is valid or when restoring default after a previous custom override.
pub fn configure_project_target_path(
    project_path: &str,
    target: Option<&str>,
    custom_target_path: Option<&str>,
    java_home: Option<&str>,
) -> Result<SetTargetPathResult, String> {
    let prev_custom = crate::modules::settings::get_project_target_path(project_path);
    let (effective_pb, default_pb, is_default) =
        resolve_effective_target_dir(project_path, target, custom_target_path);
    let default_target_path = format_display_windows_path(&default_pb);
    let effective_target_path = format_display_windows_path(&effective_pb);

    let persisted = crate::modules::settings::set_project_target_path(
        project_path,
        if is_default {
            None
        } else {
            Some(&effective_target_path)
        },
    )?;

    let validation = validate_target_build_path(
        project_path,
        target.unwrap_or(""),
        persisted.as_deref(),
        java_home,
    );

    let prop_path = find_build_dir(project_path).and_then(|bd| {
        target
            .filter(|t| !t.trim().is_empty())
            .and_then(|t| find_properties_for_target(&bd, t))
    });

    let mut properties_updated = false;
    if validation.is_valid && (!is_default || prev_custom.is_some()) {
        if let Some(ref pp) = prop_path {
            properties_updated =
                sync_properties_target_path(pp, project_path, persisted.as_deref());
        }
    }

    Ok(SetTargetPathResult {
        custom_target_path: persisted,
        effective_target_path,
        default_target_path,
        is_default_target: is_default,
        properties_updated,
        properties_file: prop_path.map(|p| p.to_string_lossy().to_string()),
        validation,
    })
}

/// Apply explicit Auto Config to build.xml javac path and target .properties paths
pub fn run_auto_config(
    project_path: &str,
    target: &str,
    java_home: Option<&str>,
) -> Result<AutoConfigResult, String> {
    run_auto_config_with_target(project_path, target, java_home, None)
}

/// Apply explicit Auto Config while preserving any configured custom target build path.
pub fn run_auto_config_with_target(
    project_path: &str,
    target: &str,
    java_home: Option<&str>,
    custom_target_path: Option<&str>,
) -> Result<AutoConfigResult, String> {
    let build_dir = find_build_dir(project_path).ok_or_else(|| {
        format!(
            "Cannot find build.xml in '{}' or its 'build' subfolder",
            project_path
        )
    })?;

    let (effective_pb, _, is_default_target) =
        resolve_effective_target_dir(project_path, Some(target), custom_target_path);
    let effective_target_str = format_display_windows_path(&effective_pb);

    let safe_proj = to_batch_safe_path(project_path)
        .trim()
        .trim_matches('"')
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();
    let custom_arg = if is_default_target {
        None
    } else {
        Some(effective_target_str.as_str())
    };
    let effective_target_slash = if is_default_target {
        format!("{}/target", safe_proj)
    } else {
        to_batch_safe_path(&effective_target_str)
            .trim()
            .trim_matches('"')
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_string()
    };

    let active_java_home = resolve_active_java_home(java_home);
    let mut changes = Vec::new();
    let mut build_xml_updated = false;
    let mut build_xml_javac_path = None;

    if let Some(ref jh) = active_java_home {
        if let Some(fixed_javac) = fix_build_xml_java_path(&build_dir, jh) {
            build_xml_updated = true;
            build_xml_javac_path = Some(fixed_javac.clone());
            changes.push(format!("build.xml javac -> {}", fixed_javac));
        }
    }

    let prop_path = find_properties_for_target(&build_dir, target);
    let mut properties_updated = false;
    let prop_str = prop_path.as_ref().map(|p| p.to_string_lossy().to_string());

    if let Some(ref pp) = prop_path {
        if update_properties_file_paths(
            pp,
            project_path,
            active_java_home.as_deref(),
            custom_arg,
            true,
        ) {
            properties_updated = true;
            let prop_name = pp
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "properties".to_string());
            if let Some(ref jh) = active_java_home {
                let safe_jh = to_batch_safe_path(jh).replace('\\', "/");
                changes.push(format!(
                    "{} target -> {}, JavaHome -> {}",
                    prop_name, effective_target_slash, safe_jh
                ));
            } else {
                changes.push(format!(
                    "{} target -> {}",
                    prop_name, effective_target_slash
                ));
            }
        }
    }

    let message = if changes.is_empty() {
        "All configuration files are already up to date.".to_string()
    } else {
        format!("Applied configuration updates:\n- {}", changes.join("\n- "))
    };

    Ok(AutoConfigResult {
        success: true,
        build_xml_updated,
        build_xml_javac_path,
        resolved_java_home: active_java_home,
        properties_file: prop_str,
        properties_updated,
        batch_safe_project_path: safe_proj,
        changes,
        message,
    })
}

/// Read a key from a .properties file (handles Shift-JIS, UTF-8, and Java `\uXXXX` escapes).
pub fn extract_property_value(properties_file: &Path, target_key: &str) -> Option<String> {
    let bytes = fs::read(properties_file).ok()?;
    let (text, _) = detect_and_decode_text(&bytes);

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.starts_with('!') {
            continue;
        }
        if let Some(idx) = trimmed.find('=') {
            let key = trimmed[..idx].trim();
            let val = trimmed[idx + 1..].trim();
            if key == target_key {
                return Some(unescape_properties_value(val));
            }
        }
    }
    None
}

/// Read properties file and find key `filter.shellscript.baseDirectory.value=`
pub fn extract_base_directory(properties_file: &Path) -> Option<String> {
    extract_property_value(properties_file, "filter.shellscript.baseDirectory.value")
}

fn is_excluded_source_log_dir(dir: &Path, base_proj: &Path) -> bool {
    paths_equal_ci(dir, &base_proj.join("log"))
        || paths_equal_ci(dir, &base_proj.join("batch").join("log"))
}

/// Collect strict target log directories (never falls back to source <project>/log or <project>/batch/log)
pub fn resolve_target_log_dirs(
    project_path: &str,
    properties_file: Option<&Path>,
    base_dir_str: Option<&str>,
) -> Vec<PathBuf> {
    resolve_target_log_dirs_with_target(project_path, properties_file, base_dir_str, None)
}

/// Target-aware log directory resolution supporting custom Target Build Paths.
pub fn resolve_target_log_dirs_with_target(
    project_path: &str,
    properties_file: Option<&Path>,
    base_dir_str: Option<&str>,
    custom_target_path: Option<&str>,
) -> Vec<PathBuf> {
    let clean_proj = project_path.trim().trim_matches('"').replace('/', "\\");
    let base_proj = Path::new(&clean_proj);
    let source_batch_dir = base_proj.join("batch");
    let (effective_target, _, is_default_target) =
        resolve_effective_target_dir(&clean_proj, None, custom_target_path);

    let mut raw_dirs: Vec<PathBuf> = Vec::new();
    raw_dirs.push(effective_target.join("log"));

    if let Some(prop_path) = properties_file {
        if let Some(log4j_dir) = extract_property_value(prop_path, "pom.log4j.path.value") {
            let clean_slash = log4j_dir.trim().replace('\\', "/");
            if !clean_slash.is_empty() {
                let candidate = PathBuf::from(clean_slash.replace('/', "\\"));
                let allowed = is_path_within_dir(&candidate, &effective_target)
                    || (is_default_target && is_path_within_dir(&candidate, base_proj));
                if allowed {
                    raw_dirs.push(candidate);
                }

                if clean_slash.contains("/Desktop/") {
                    if let Ok(user_profile) = std::env::var("USERPROFILE") {
                        let std_desk = format!(
                            "{}/Desktop",
                            user_profile.replace('\\', "/").trim_end_matches('/')
                        );
                        for od_desk in find_onedrive_desktops() {
                            let od_str = od_desk.to_string_lossy().replace('\\', "/");
                            let replaced =
                                clean_slash.replace(&std_desk, od_str.trim_end_matches('/'));
                            let od_candidate = PathBuf::from(replaced.replace('/', "\\"));
                            let od_allowed = is_path_within_dir(&od_candidate, &effective_target)
                                || (is_default_target
                                    && is_path_within_dir(&od_candidate, base_proj));
                            if od_allowed {
                                raw_dirs.push(od_candidate);
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some(base_str) = base_dir_str {
        let clean_win = base_str.trim().replace('/', "\\");
        let clean_trimmed = clean_win.trim_end_matches('\\');
        if !clean_trimmed.is_empty() {
            let direct = PathBuf::from(clean_trimmed);
            let within = is_path_within_dir(&direct, &effective_target)
                || (is_default_target && is_path_within_dir(&direct, base_proj));
            if within
                && !paths_equal_ci(&direct, base_proj)
                && !paths_equal_ci(&direct, &source_batch_dir)
            {
                raw_dirs.push(direct.join("log"));
            }
        }
    }

    if is_default_target {
        raw_dirs.push(base_proj.join("target").join("log"));
    }

    let mut unique_dirs: Vec<PathBuf> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for dir in raw_dirs {
        if is_excluded_source_log_dir(&dir, base_proj) {
            continue;
        }
        let expanded = resolve_long_path_best_effort(&dir);
        let key = normalize_path_key(&expanded);
        if key.is_empty() || !seen.insert(key) {
            continue;
        }
        unique_dirs.push(expanded);
    }

    unique_dirs
}

/// Resolve a specific log file strictly inside target log directories
pub fn resolve_log_file(
    project_path: &str,
    properties_file: Option<&Path>,
    base_dir_str: Option<&str>,
    file_name: &str,
) -> (Option<String>, Option<String>) {
    resolve_log_file_with_target(project_path, properties_file, base_dir_str, file_name, None)
}

/// Target-aware log file resolver supporting custom Target Build Paths.
pub fn resolve_log_file_with_target(
    project_path: &str,
    properties_file: Option<&Path>,
    base_dir_str: Option<&str>,
    file_name: &str,
    custom_target_path: Option<&str>,
) -> (Option<String>, Option<String>) {
    let candidate_dirs = resolve_target_log_dirs_with_target(
        project_path,
        properties_file,
        base_dir_str,
        custom_target_path,
    );

    let mut found_log_file: Option<String> = None;
    let mut found_log_dir: Option<String> = None;

    for dir in &candidate_dirs {
        let log_file = dir.join(file_name);
        if log_file.is_file() {
            found_log_file = Some(log_file.to_string_lossy().replace('/', "\\"));
            found_log_dir = Some(dir.to_string_lossy().replace('/', "\\"));
            break;
        }
        if found_log_dir.is_none() && dir.is_dir() {
            found_log_dir = Some(dir.to_string_lossy().replace('/', "\\"));
        }
    }

    (found_log_file, found_log_dir)
}

/// Resolve batchApp.log path and target log directory
pub fn resolve_batch_app_log(
    project_path: &str,
    properties_file: Option<&Path>,
    base_dir_str: Option<&str>,
) -> (Option<String>, Option<String>) {
    resolve_log_file(project_path, properties_file, base_dir_str, "batchApp.log")
}

fn parse_log4j_appender_filename(raw_val: &str) -> Option<String> {
    let trimmed = raw_val.trim();
    if trimmed.is_empty() {
        return None;
    }
    let tail = trimmed
        .rsplit(['@', '/', '\\', '}'])
        .find(|seg| !seg.trim().is_empty())
        .map(|s| s.trim())?;
    if tail.contains('.') && !tail.contains('*') {
        Some(tail.to_string())
    } else {
        None
    }
}

fn parse_log4j_config_entries(file_path: &Path) -> Vec<(String, String)> {
    let bytes = match fs::read(file_path) {
        Ok(b) => b,
        Err(_) => return Vec::new(),
    };
    let (content, _) = detect_and_decode_text(&bytes);
    let mut entries = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
            continue;
        }
        let Some(eq_idx) = trimmed.find('=') else {
            continue;
        };
        let key = trimmed[..eq_idx].trim();
        let val = trimmed[eq_idx + 1..].trim();
        let key_lower = key.to_lowercase();

        if let Some(rest) = key_lower.strip_prefix("log4j.appender.") {
            if let Some(dot_rel) = rest.find('.') {
                let suffix = &rest[dot_rel + 1..];
                if suffix == "file" {
                    let appender_orig =
                        &key["log4j.appender.".len().."log4j.appender.".len() + dot_rel];
                    if let Some(fname) = parse_log4j_appender_filename(val) {
                        entries.push((appender_orig.to_string(), fname));
                    }
                }
            }
        }
    }

    entries
}

/// Discover configured batch log and application log filenames from project log4j configs (no hardcoding).
pub fn discover_configured_log_files(
    project_path: &str,
    base_dir_str: Option<&str>,
) -> (Option<String>, Option<String>) {
    discover_configured_log_files_with_target(project_path, base_dir_str, None)
}

/// Target-aware log filename discovery from project log4j configurations.
pub fn discover_configured_log_files_with_target(
    project_path: &str,
    base_dir_str: Option<&str>,
    custom_target_path: Option<&str>,
) -> (Option<String>, Option<String>) {
    let clean_proj = project_path.trim().trim_matches('"').replace('/', "\\");
    if clean_proj.is_empty() {
        return (None, None);
    }
    let base_proj = Path::new(&clean_proj);
    let source_batch_dir = base_proj.join("batch");
    let (effective_target, _, is_default_target) =
        resolve_effective_target_dir(&clean_proj, None, custom_target_path);

    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Some(base_str) = base_dir_str {
        let clean_base = base_str.trim().replace('/', "\\");
        let clean_trimmed = clean_base.trim_end_matches('\\');
        if !clean_trimmed.is_empty() {
            let bdir = PathBuf::from(clean_trimmed);
            let within = is_path_within_dir(&bdir, &effective_target)
                || (is_default_target && is_path_within_dir(&bdir, base_proj));
            if within
                && !paths_equal_ci(&bdir, base_proj)
                && !paths_equal_ci(&bdir, &source_batch_dir)
            {
                candidates.push(
                    bdir.join("batch")
                        .join("properties")
                        .join("stcliblog4j.properties"),
                );
                candidates.push(bdir.join("properties").join("stcliblog4j.properties"));
                candidates.push(bdir.join("stcliblog4j.properties"));
            }
        }
    }

    candidates.push(
        effective_target
            .join("batch")
            .join("properties")
            .join("stcliblog4j.properties"),
    );
    candidates.push(
        base_proj
            .join("batch")
            .join("properties")
            .join("stcliblog4j.properties.filter"),
    );
    candidates.push(
        base_proj
            .join("batch")
            .join("properties")
            .join("stcliblog4j.properties"),
    );

    for root_dir in [effective_target, base_proj.join("batch")] {
        if root_dir.is_dir() {
            for entry in walkdir::WalkDir::new(&root_dir)
                .max_depth(4)
                .into_iter()
                .flatten()
            {
                if entry.file_type().is_file() {
                    let fname_lower = entry.file_name().to_string_lossy().to_lowercase();
                    if fname_lower.contains("log4j")
                        && (fname_lower.ends_with(".properties")
                            || fname_lower.ends_with(".properties.filter"))
                    {
                        candidates.push(entry.path().to_path_buf());
                    }
                }
            }
        }
    }

    let mut seen = std::collections::HashSet::new();
    for path in candidates {
        let key = normalize_path_key(&path);
        if !seen.insert(key) || !path.is_file() {
            continue;
        }
        let entries = parse_log4j_config_entries(&path);
        if entries.is_empty() {
            continue;
        }

        let batch_log = entries
            .iter()
            .find(|(app, _)| {
                let al = app.to_lowercase();
                al.contains("batch") && !al.contains("user")
            })
            .or_else(|| {
                entries.iter().find(|(_, fname)| {
                    let fl = fname.to_lowercase();
                    fl.contains("batch") && !fl.contains("user")
                })
            })
            .or_else(|| {
                entries.iter().find(|(app, fname)| {
                    !app.to_lowercase().contains("app") && !fname.to_lowercase().contains("app")
                })
            })
            .map(|(_, fname)| fname.clone());

        let app_log = entries
            .iter()
            .find(|(app, fname)| {
                let al = app.to_lowercase();
                al.contains("app") && !al.contains("batch") && Some(fname) != batch_log.as_ref()
            })
            .or_else(|| {
                entries.iter().find(|(_, fname)| {
                    let fl = fname.to_lowercase();
                    fl.contains("app") && !fl.contains("batch") && Some(fname) != batch_log.as_ref()
                })
            })
            .or_else(|| {
                entries
                    .iter()
                    .find(|(_, fname)| Some(fname) != batch_log.as_ref())
            })
            .map(|(_, fname)| fname.clone());

        return (batch_log, app_log);
    }

    (None, None)
}

/// Resolve the OS default application registered for `.txt` files (never executes `.bat`!).
pub fn resolve_default_txt_editor() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        #[link(name = "shlwapi")]
        extern "system" {
            fn AssocQueryStringW(
                flags: u32,
                str_type: u32,
                pszAssoc: *const u16,
                pszExtra: *const u16,
                pszOut: *mut u16,
                pcchOut: *mut u32,
            ) -> i32;
        }

        const ASSOCSTR_EXECUTABLE: u32 = 2;
        let ext: Vec<u16> = OsStr::new(".txt")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let verb: Vec<u16> = OsStr::new("open")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let mut len: u32 = 0;
            let _ = AssocQueryStringW(
                0,
                ASSOCSTR_EXECUTABLE,
                ext.as_ptr(),
                verb.as_ptr(),
                std::ptr::null_mut(),
                &mut len,
            );
            if len > 0 {
                let mut buf = vec![0u16; len as usize];
                let hr = AssocQueryStringW(
                    0,
                    ASSOCSTR_EXECUTABLE,
                    ext.as_ptr(),
                    verb.as_ptr(),
                    buf.as_mut_ptr(),
                    &mut len,
                );
                if hr == 0 {
                    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                    let exe = String::from_utf16_lossy(&buf[..end]).trim().to_string();
                    if !exe.is_empty() && Path::new(&exe).is_file() {
                        return Some(exe);
                    }
                }
            }
        }

        if let Ok(win_dir) = std::env::var("WINDIR") {
            let np = Path::new(&win_dir).join("notepad.exe");
            if np.is_file() {
                return Some(np.to_string_lossy().to_string());
            }
        }
        Some("notepad.exe".to_string())
    }

    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

/// Open a file purely as plain text using the OS default editor for `.txt` files.
/// Never executes `.bat` scripts and never modifies original file bytes or encoding.
pub fn open_file_with_default_text_editor(path: &str) -> Result<String, String> {
    let clean = path.trim().trim_matches('"');
    #[cfg(target_os = "windows")]
    let target_str = clean.replace('/', "\\");
    #[cfg(not(target_os = "windows"))]
    let target_str = clean.to_string();

    let p = Path::new(&target_str);
    if !p.is_file() {
        return Err(format!("File does not exist: {}", target_str));
    }

    #[cfg(target_os = "windows")]
    {
        let editor = resolve_default_txt_editor().unwrap_or_else(|| "notepad.exe".to_string());
        Command::new(&editor)
            .arg(&target_str)
            .spawn()
            .map_err(|e| format!("Failed to launch text editor '{}': {}", editor, e))?;
        return Ok(target_str);
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .args(["-t", &target_str])
            .spawn()
            .map_err(|e| format!("Failed to open file in text editor: {}", e))?;
        return Ok(target_str);
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Ok(out) = Command::new("xdg-mime")
            .args(["query", "default", "text/plain"])
            .output()
        {
            if out.status.success() {
                let desktop = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !desktop.is_empty() {
                    if Command::new("gtk-launch")
                        .args([&desktop, &target_str])
                        .spawn()
                        .is_ok()
                    {
                        return Ok(target_str);
                    }
                }
            }
        }
        Command::new("xdg-open")
            .arg(&target_str)
            .spawn()
            .map_err(|e| format!("Failed to open text editor: {}", e))?;
        return Ok(target_str);
    }
}

/// Open any file (e.g. `.properties`, `.log`) using the OS default application (Unicode path safe).
pub fn open_file_with_os_default(path: &str) -> Result<String, String> {
    let clean = path.trim().trim_matches('"');
    #[cfg(target_os = "windows")]
    let target_str = clean.replace('/', "\\");
    #[cfg(not(target_os = "windows"))]
    let target_str = clean.to_string();

    let p = Path::new(&target_str);
    if !p.is_file() {
        return Err(format!("File does not exist: {}", target_str));
    }

    #[cfg(target_os = "windows")]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        #[link(name = "shell32")]
        extern "system" {
            fn ShellExecuteW(
                hwnd: isize,
                lpOperation: *const u16,
                lpFile: *const u16,
                lpParameters: *const u16,
                lpDirectory: *const u16,
                nShowCmd: i32,
            ) -> isize;
        }

        const SW_SHOWNORMAL: i32 = 1;
        let op_w: Vec<u16> = OsStr::new("open")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let file_w: Vec<u16> = OsStr::new(&target_str)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let res = unsafe {
            ShellExecuteW(
                0,
                op_w.as_ptr(),
                file_w.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };

        if res > 32 {
            return Ok(target_str);
        }

        // Fallback when file extension (e.g. .properties) has no registered handler on Windows
        return open_file_with_default_text_editor(&target_str);
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(&target_str)
            .spawn()
            .map_err(|e| format!("Failed to open file: {}", e))?;
        return Ok(target_str);
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(&target_str)
            .spawn()
            .map_err(|e| format!("Failed to open file: {}", e))?;
        return Ok(target_str);
    }
}

/// Strictly resolve and open a target log file (never falls back to source <project>/log)
pub fn open_target_log_file(
    project_path: Option<&str>,
    target: Option<&str>,
    log_folder_path: Option<&str>,
    file_name: &str,
) -> Result<String, String> {
    open_target_log_file_with_target(project_path, target, log_folder_path, file_name, None)
}

/// Target-aware log file opener supporting custom Target Build Paths.
pub fn open_target_log_file_with_target(
    project_path: Option<&str>,
    target: Option<&str>,
    log_folder_path: Option<&str>,
    file_name: &str,
    custom_target_path: Option<&str>,
) -> Result<String, String> {
    let clean_name = file_name.trim();
    if clean_name.is_empty() {
        return Err("Log file name cannot be empty.".to_string());
    }

    let mut candidate_dirs: Vec<PathBuf> = Vec::new();
    let proj_opt = project_path
        .map(|s| s.trim().trim_matches('"').replace('/', "\\"))
        .filter(|s| !s.is_empty());

    if let Some(ref proj_str) = proj_opt {
        let build_dir = find_build_dir(proj_str);
        let prop_file = build_dir
            .as_ref()
            .and_then(|bd| target.and_then(|t| find_properties_for_target(bd, t)));
        let base_dir = prop_file.as_ref().and_then(|p| extract_base_directory(p));
        candidate_dirs.extend(resolve_target_log_dirs_with_target(
            proj_str,
            prop_file.as_deref(),
            base_dir.as_deref(),
            custom_target_path,
        ));
    }

    if let Some(dir_str) = log_folder_path {
        let clean_dir = dir_str.trim().trim_matches('"').replace('/', "\\");
        if !clean_dir.is_empty() {
            let dir_path = PathBuf::from(&clean_dir);
            let excluded = proj_opt
                .as_deref()
                .map(|p| is_excluded_source_log_dir(&dir_path, Path::new(p)))
                .unwrap_or(false);
            if !excluded {
                candidate_dirs.push(dir_path);
            }
        }
    }

    // Deduplicate case-insensitively while preserving priority order
    let mut deduped: Vec<PathBuf> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for dir in candidate_dirs {
        let key = normalize_path_key(&dir);
        if !key.is_empty() && seen.insert(key) {
            deduped.push(dir);
        }
    }

    if deduped.is_empty() {
        return Err(format!(
            "Log file not found: target\\log\\{}\nPlease select a project or run Build first.",
            clean_name
        ));
    }

    for dir in &deduped {
        let candidate = dir.join(clean_name);
        if candidate.is_file() {
            let win_path = candidate.to_string_lossy().replace('/', "\\");
            return open_file_with_os_default(&win_path);
        }
    }

    let expected_dir = deduped
        .iter()
        .find(|d| d.is_dir())
        .cloned()
        .or_else(|| {
            proj_opt.as_ref().map(|p| {
                let (eff, _, _) = resolve_effective_target_dir(p, target, custom_target_path);
                eff.join("log")
            })
        })
        .unwrap_or_else(|| deduped[0].clone());

    let expected_path = expected_dir
        .join(clean_name)
        .to_string_lossy()
        .replace('/', "\\");

    Err(format!("Log file not found:\n{}", expected_path))
}

/// Safely clear (truncate to 0 bytes) all target `.log` files for the selected project/target.
/// Never touches `.bat` scripts, app settings/history JSON files, or excluded source `<project>/log` folders.
pub fn clear_target_log_files(
    project_path: Option<&str>,
    target: Option<&str>,
    log_folder_path: Option<&str>,
    batch_log_name: Option<&str>,
    app_log_name: Option<&str>,
) -> Result<ClearLogsResult, String> {
    clear_target_log_files_with_target(
        project_path,
        target,
        log_folder_path,
        batch_log_name,
        app_log_name,
        None,
    )
}

/// Target-aware `.log` truncation helper supporting custom Target Build Paths.
pub fn clear_target_log_files_with_target(
    project_path: Option<&str>,
    target: Option<&str>,
    log_folder_path: Option<&str>,
    batch_log_name: Option<&str>,
    app_log_name: Option<&str>,
    custom_target_path: Option<&str>,
) -> Result<ClearLogsResult, String> {
    let mut candidate_dirs: Vec<PathBuf> = Vec::new();
    let proj_opt = project_path
        .map(|s| s.trim().trim_matches('"').replace('/', "\\"))
        .filter(|s| !s.is_empty());

    if let Some(ref proj_str) = proj_opt {
        let build_dir = find_build_dir(proj_str);
        let prop_file = build_dir
            .as_ref()
            .and_then(|bd| target.and_then(|t| find_properties_for_target(bd, t)));
        let base_dir = prop_file.as_ref().and_then(|p| extract_base_directory(p));
        candidate_dirs.extend(resolve_target_log_dirs_with_target(
            proj_str,
            prop_file.as_deref(),
            base_dir.as_deref(),
            custom_target_path,
        ));
    }

    if let Some(dir_str) = log_folder_path {
        let clean_dir = dir_str.trim().trim_matches('"').replace('/', "\\");
        if !clean_dir.is_empty() {
            let dir_path = PathBuf::from(&clean_dir);
            let excluded = proj_opt
                .as_deref()
                .map(|p| is_excluded_source_log_dir(&dir_path, Path::new(p)))
                .unwrap_or(false);
            let within_allowed = proj_opt
                .as_deref()
                .map(|p| {
                    let (eff, _, is_def) =
                        resolve_effective_target_dir(p, target, custom_target_path);
                    is_path_within_dir(&dir_path, &eff)
                        || (is_def && is_path_within_dir(&dir_path, Path::new(p)))
                })
                .unwrap_or(false);
            if !excluded && within_allowed {
                candidate_dirs.push(dir_path);
            }
        }
    }

    let mut deduped_dirs: Vec<PathBuf> = Vec::new();
    let mut seen_dirs = std::collections::HashSet::new();
    for dir in candidate_dirs {
        let expanded = resolve_long_path_best_effort(&dir);
        let key = normalize_path_key(&expanded);
        if !key.is_empty() && seen_dirs.insert(key) {
            deduped_dirs.push(expanded);
        }
    }

    let mut log_dirs_checked: Vec<String> = Vec::new();
    let mut candidate_files: Vec<PathBuf> = Vec::new();
    let mut seen_files = std::collections::HashSet::new();

    for dir in &deduped_dirs {
        if !dir.is_dir() {
            continue;
        }
        log_dirs_checked.push(dir.to_string_lossy().replace('/', "\\"));

        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let is_log = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.eq_ignore_ascii_case("log"))
                        .unwrap_or(false);
                    if is_log {
                        let key = normalize_path_key(&path);
                        if seen_files.insert(key) {
                            candidate_files.push(path);
                        }
                    }
                }
            }
        }

        for extra_name in [batch_log_name, app_log_name].into_iter().flatten() {
            let trimmed = extra_name.trim();
            if !trimmed.is_empty() {
                let extra_path = dir.join(trimmed);
                if extra_path.is_file() {
                    let key = normalize_path_key(&extra_path);
                    if seen_files.insert(key) {
                        candidate_files.push(extra_path);
                    }
                }
            }
        }
    }

    candidate_files.sort_by(|a, b| {
        a.to_string_lossy()
            .to_lowercase()
            .cmp(&b.to_string_lossy().to_lowercase())
    });

    let mut cleared_files = Vec::new();
    let mut failed_files = Vec::new();
    for file_path in candidate_files {
        let display = file_path.to_string_lossy().replace('/', "\\");
        match fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&file_path)
        {
            Ok(_) => cleared_files.push(display),
            Err(err) => failed_files.push(format!("{}: {}", display, err)),
        }
    }

    Ok(ClearLogsResult {
        cleared_files,
        failed_files,
        log_dirs_checked,
    })
}

fn normalize_path_key(p: &Path) -> String {
    let s = p.to_string_lossy().replace('/', "\\");
    s.trim_end_matches('\\').to_lowercase()
}

fn paths_equal_ci(a: &Path, b: &Path) -> bool {
    normalize_path_key(a) == normalize_path_key(b)
}

/// Scan a user-specified custom folder for .bat files
pub fn scan_custom_bat_folder(folder_path: &str) -> (Option<String>, Vec<BatFileInfo>) {
    let clean = folder_path.trim().trim_matches('"').replace('/', "\\");
    let base = Path::new(&clean);
    if !base.exists() {
        return (None, Vec::new());
    }
    if base.is_file() {
        if let Some(parent) = base.parent() {
            let display_dir = to_display_long_path(parent);
            let files = list_bat_files_in_dir(&display_dir);
            return (Some(display_dir.to_string_lossy().to_string()), files);
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
            let display_dir = to_display_long_path(&c);
            let files = list_bat_files_in_dir(&display_dir);
            if !files.is_empty() {
                return (Some(display_dir.to_string_lossy().to_string()), files);
            }
        }
    }

    let display_base = to_display_long_path(base);
    (Some(display_base.to_string_lossy().to_string()), Vec::new())
}

/// Find batch/batfiles strictly inside the built target folder (never fall back to source batch/batfiles).
/// Checks the selected project's own target folders first, rejects paths outside `project_path`,
/// and expands 8.3 short segments back to full readable Unicode paths when `project_path` is not short.
pub fn scan_bat_files(
    project_path: &str,
    base_dir_str: Option<&str>,
) -> (Option<PathBuf>, Vec<BatFileInfo>) {
    scan_bat_files_with_target(project_path, base_dir_str, None)
}

/// Target-aware `.bat` folder scanner supporting custom Target Build Paths.
/// When a custom target path is set, never falls back to stale `<project>/target` or source `batch/batfiles`.
pub fn scan_bat_files_with_target(
    project_path: &str,
    base_dir_str: Option<&str>,
    custom_target_path: Option<&str>,
) -> (Option<PathBuf>, Vec<BatFileInfo>) {
    let clean_proj_str = project_path.trim().trim_matches('"').replace('/', "\\");
    let base_proj = Path::new(&clean_proj_str);
    let source_batch_dir = base_proj.join("batch");
    let source_batfiles_dir = source_batch_dir.join("batfiles");
    let (effective_target, _, is_default_target) =
        resolve_effective_target_dir(&clean_proj_str, None, custom_target_path);
    let target_has_tilde = effective_target.to_string_lossy().contains('~');

    // Always prefer the active effective target folders first
    let mut candidate_folders: Vec<PathBuf> = vec![
        effective_target.join("batch").join("batfiles"),
        effective_target.join("batfiles"),
    ];

    // Only accept properties base_dir candidates that reside strictly inside the effective target (or project when default)
    if let Some(base_str) = base_dir_str {
        let base_norm = base_str.trim().replace('/', "\\");
        let clean_base = base_norm.trim_end_matches('\\');
        if !clean_base.is_empty() {
            let direct_path = PathBuf::from(clean_base);
            let within = is_path_within_dir(&direct_path, &effective_target)
                || (is_default_target && is_path_within_dir(&direct_path, base_proj));
            if within
                && !paths_equal_ci(&direct_path, base_proj)
                && !paths_equal_ci(&direct_path, &source_batch_dir)
            {
                candidate_folders.push(direct_path.join("batch").join("batfiles"));
                candidate_folders.push(direct_path.join("batfiles"));
            }

            if clean_base.to_lowercase().contains("\\desktop\\") {
                if let Ok(user_profile) = std::env::var("USERPROFILE") {
                    let std_desk = format!(
                        "{}\\Desktop",
                        user_profile.replace('/', "\\").trim_end_matches('\\')
                    );
                    if clean_base.len() >= std_desk.len()
                        && clean_base.is_char_boundary(std_desk.len())
                        && clean_base[..std_desk.len()].eq_ignore_ascii_case(&std_desk)
                    {
                        let suffix = &clean_base[std_desk.len()..];
                        for od_desk in find_onedrive_desktops() {
                            let od_str = od_desk.to_string_lossy().replace('/', "\\");
                            let replaced = format!("{}{}", od_str.trim_end_matches('\\'), suffix);
                            let p = PathBuf::from(replaced);
                            let p_within = is_path_within_dir(&p, &effective_target)
                                || (is_default_target && is_path_within_dir(&p, base_proj));
                            if p_within
                                && !paths_equal_ci(&p, base_proj)
                                && !paths_equal_ci(&p, &source_batch_dir)
                            {
                                candidate_folders.push(p.join("batch").join("batfiles"));
                                candidate_folders.push(p.join("batfiles"));
                            }
                        }
                    }
                }
            }
        }
    }

    for folder in candidate_folders {
        if paths_equal_ci(&folder, &source_batfiles_dir)
            || paths_equal_ci(&folder, &source_batch_dir)
        {
            continue;
        }
        if folder.is_dir() {
            let display_dir = if !target_has_tilde && folder.to_string_lossy().contains('~') {
                to_display_long_path(&folder)
            } else {
                folder
            };
            let files = list_bat_files_in_dir(&display_dir);
            if !files.is_empty() {
                return (Some(display_dir), files);
            }
        }
    }

    // Check nested directories inside effective target (e.g., <effective_target>/20-honban-psp/hansoku/batch/batfiles)
    if effective_target.is_dir() {
        for entry in walkdir::WalkDir::new(&effective_target)
            .max_depth(5)
            .into_iter()
            .flatten()
        {
            if entry.file_type().is_dir() {
                let is_batfiles = entry
                    .file_name()
                    .to_str()
                    .map(|s| s.eq_ignore_ascii_case("batfiles"))
                    .unwrap_or(false);
                if is_batfiles {
                    let dir_path = entry.path().to_path_buf();
                    let display_dir =
                        if !target_has_tilde && dir_path.to_string_lossy().contains('~') {
                            to_display_long_path(&dir_path)
                        } else {
                            dir_path
                        };
                    let files = list_bat_files_in_dir(&display_dir);
                    if !files.is_empty() {
                        return (Some(display_dir), files);
                    }
                }
            }
        }
    }

    (None, Vec::new())
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
        let secs = t
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
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
    inspect_target_files_with_target(project_path, target, None)
}

/// Target-aware post-build inspection supporting custom Target Build Paths.
pub fn inspect_target_files_with_target(
    project_path: &str,
    target: &str,
    custom_target_path: Option<&str>,
) -> PostBuildInspection {
    let (effective_pb, default_pb, is_default_target) =
        resolve_effective_target_dir(project_path, Some(target), custom_target_path);
    let default_target_path = format_display_windows_path(&default_pb);
    let effective_target_path = format_display_windows_path(&effective_pb);
    let eff_arg = if is_default_target {
        None
    } else {
        Some(effective_target_path.as_str())
    };

    let build_dir = find_build_dir(project_path);
    let prop_file = build_dir
        .as_ref()
        .and_then(|bd| find_properties_for_target(bd, target));

    let base_dir = prop_file.as_ref().and_then(|p| extract_base_directory(p));
    let (bat_folder, bat_files) =
        scan_bat_files_with_target(project_path, base_dir.as_deref(), eff_arg);
    let (batch_log_name, app_log_name) =
        discover_configured_log_files_with_target(project_path, base_dir.as_deref(), eff_arg);

    let (log_file_path, batch_log_folder) = if let Some(ref b_name) = batch_log_name {
        resolve_log_file_with_target(
            project_path,
            prop_file.as_deref(),
            base_dir.as_deref(),
            b_name,
            eff_arg,
        )
    } else {
        (None, None)
    };

    let (app_log_file_path, app_log_folder) = if let Some(ref a_name) = app_log_name {
        resolve_log_file_with_target(
            project_path,
            prop_file.as_deref(),
            base_dir.as_deref(),
            a_name,
            eff_arg,
        )
    } else {
        (None, None)
    };

    let log_folder_path = batch_log_folder.or(app_log_folder).or_else(|| {
        resolve_target_log_dirs_with_target(
            project_path,
            prop_file.as_deref(),
            base_dir.as_deref(),
            eff_arg,
        )
        .into_iter()
        .find(|d| d.is_dir())
        .map(|d| d.to_string_lossy().replace('/', "\\"))
    });

    PostBuildInspection {
        properties_file: prop_file.map(|p| p.to_string_lossy().to_string()),
        base_directory: base_dir,
        default_target_path: Some(default_target_path),
        effective_target_path: Some(effective_target_path),
        is_default_target,
        bat_folder: bat_folder.map(|p| p.to_string_lossy().to_string()),
        bat_files,
        batch_log_name,
        app_log_name,
        log_file_path,
        app_log_file_path,
        log_folder_path,
    }
}

/// Run pre-build target path validation, sync dynamic dependencies if a custom target path is set,
/// execute CMD `ant <target>`, and stream ALL logs in real-time (including Shift-JIS / CP932 lines).
pub fn run_ant_build(
    app: AppHandle,
    tab_id: String,
    project_path: String,
    target: String,
    ant_executable: Option<String>,
    java_home: Option<String>,
    custom_target_path: Option<String>,
) -> Result<BuildResult, String> {
    let build_dir = find_build_dir(&project_path).ok_or_else(|| {
        format!(
            "Cannot find build.xml in '{}' or its 'build' subfolder",
            project_path
        )
    })?;

    let emit_line = |h: &AppHandle, t_id: &str, l: String| {
        let _ = h.emit(
            "build-output",
            BuildLogEvent {
                tab_id: t_id.to_string(),
                line: l,
            },
        );
    };

    let validation = validate_target_build_path(
        &project_path,
        &target,
        custom_target_path.as_deref(),
        java_home.as_deref(),
    );
    if !validation.is_valid {
        emit_line(
            &app,
            &tab_id,
            format!(
                "[PRE-BUILD BLOCKED] Target path validation failed for '{}':\n{}",
                validation.effective_target_path, validation.summary
            ),
        );
        return Err(format!(
            "Pre-build path validation failed:\n{}",
            validation.summary
        ));
    }

    let ant_props = detect_ant_target_properties(&project_path, Some(&target));

    // Resolve clean Ant command (prefer "ant" if available in PATH, just like manual CMD)
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

    #[cfg(windows)]
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let mut command = Command::new("cmd.exe");
    command.arg("/c");
    command.arg(&ant_exe);

    if !validation.is_default_target {
        let eff_arg = validation.effective_target_path.as_str();
        if let Some(prop_path) = find_properties_for_target(&build_dir, &target) {
            if sync_properties_target_path(&prop_path, &project_path, Some(eff_arg)) {
                emit_line(
                    &app,
                    &tab_id,
                    format!(
                        "> Synced target path in {} -> {}",
                        prop_path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "properties".to_string()),
                        validation.batch_safe_target_path
                    ),
                );
            }
        }

        let target_slash = validation.batch_safe_target_path.as_str();
        let target_prop_arg = format!("-D{}={}", ant_props.target_prop_name, target_slash);
        let classes_prop_arg = if !ant_props
            .build_classes_prop_name
            .eq_ignore_ascii_case(&ant_props.target_prop_name)
        {
            Some(format!(
                "-D{}={}/classes",
                ant_props.build_classes_prop_name, target_slash
            ))
        } else {
            None
        };

        command.env("TARGET_HOME", target_slash);

        #[cfg(windows)]
        {
            // Quote -Dname=val via raw_arg so ant.bat does not split on '=' under cmd.exe
            command.raw_arg(format!("\"{}\"", target_prop_arg));
            if let Some(ref c_arg) = classes_prop_arg {
                command.raw_arg(format!("\"{}\"", c_arg));
            }
        }
        #[cfg(not(windows))]
        {
            command.arg(&target_prop_arg);
            if let Some(ref c_arg) = classes_prop_arg {
                command.arg(c_arg);
            }
        }
        command.arg(&target);

        let extra_display = classes_prop_arg
            .as_deref()
            .map(|c| format!(" \"{}\"", c))
            .unwrap_or_default();
        emit_line(
            &app,
            &tab_id,
            format!(
                "> {} \"{}\" {} {}",
                ant_exe,
                target_prop_arg,
                extra_display.trim_start(),
                target
            )
            .replace("  ", " "),
        );
    } else {
        command.arg(&target);
        emit_line(&app, &tab_id, format!("> {} {}", ant_exe, target));
    }

    command.current_dir(&build_dir);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    // Ensure ANT_OPTS sets -Dfile.encoding=MS932 if build.xml uses MS932
    // This prevents Java 18+ (JEP 400 UTF-8 default) from writing UTF-8 @argfiles when javac expects MS932
    if let Ok(xml_bytes) = fs::read(build_dir.join("build.xml")) {
        let (xml_str, _) = detect_and_decode_text(&xml_bytes);
        if xml_str.contains("MS932")
            || xml_str.contains("Shift-JIS")
            || xml_str.contains("Shift_JIS")
        {
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

    let status = child
        .wait()
        .map_err(|e| format!("Process wait error: {}", e))?;
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

    let inspection = inspect_target_files_with_target(
        &project_path,
        &target,
        if validation.is_default_target {
            None
        } else {
            Some(validation.effective_target_path.as_str())
        },
    );

    Ok(BuildResult {
        success: is_success,
        exit_code: Some(exit_code),
        message: result_msg,
        build_xml_dir: build_dir.to_string_lossy().to_string(),
        target,
        properties_file: inspection.properties_file,
        base_directory: inspection.base_directory,
        default_target_path: inspection.default_target_path,
        effective_target_path: inspection.effective_target_path,
        is_default_target: inspection.is_default_target,
        bat_folder: inspection.bat_folder,
        bat_files: inspection.bat_files,
        batch_log_name: inspection.batch_log_name,
        app_log_name: inspection.app_log_name,
        log_file_path: inspection.log_file_path,
        app_log_file_path: inspection.app_log_file_path,
        log_folder_path: inspection.log_folder_path,
    })
}

/// Encode a PowerShell script string as UTF-16LE Base64 for `-EncodedCommand`.
pub fn encode_utf16le_base64(script: &str) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bytes = Vec::with_capacity(script.len() * 2);
    for unit in script.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }

    let mut out = String::with_capacity(((bytes.len() + 2) / 3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;

        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((n >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(n & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn ps_escape_single(s: &str) -> String {
    s.replace('\'', "''")
}

/// Generate a unique `.ps1` script path in `%TEMP%`.
pub fn allocate_temp_ps1_path(prefix: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = PS_SCRIPT_SEQ.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "{}_{}_{}_{}.ps1",
        prefix,
        std::process::id(),
        nanos,
        seq
    ))
}

/// Write a PowerShell script as UTF-8 with BOM so PowerShell 5.1 preserves non-ASCII paths accurately.
pub fn write_ps1_with_bom(path: &Path, script: &str) -> Result<(), String> {
    let mut bytes = Vec::with_capacity(3 + script.len());
    bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    bytes.extend_from_slice(script.as_bytes());
    fs::write(path, &bytes).map_err(|e| {
        format!(
            "Failed to write PowerShell runner '{}': {}",
            path.display(),
            e
        )
    })
}

/// Append self-cleanup statement so a launched `.ps1` file removes itself from `%TEMP%`.
pub fn append_ps1_self_cleanup(script: &str, ps1_path: &Path) -> String {
    let safe_ps1 = ps_escape_single(&ps1_path.to_string_lossy());
    format!(
        "{}\r\ntry {{ Remove-Item -LiteralPath '{}' -Force -ErrorAction SilentlyContinue }} catch {{}}\r\n",
        script.trim_end(),
        safe_ps1
    )
}

/// Spawn `conhost.exe powershell.exe <args...>` so `GetConsoleWindow()` resolves cleanly,
/// with `CREATE_NEW_CONSOLE` fallback to direct `powershell.exe` if `conhost.exe` fails to start.
pub fn spawn_conhost_powershell(
    parent: &Path,
    ps_args: &[&str],
) -> Result<std::process::Child, String> {
    let mut conhost_args = Vec::with_capacity(1 + ps_args.len());
    conhost_args.push("powershell.exe");
    conhost_args.extend_from_slice(ps_args);

    // Do NOT redirect conhost.exe stdio to NUL (Stdio::null()): conhost exits immediately when stdio handles are nulled.
    match Command::new("conhost.exe")
        .args(&conhost_args)
        .current_dir(parent)
        .spawn()
    {
        Ok(child) => Ok(child),
        Err(conhost_err) => {
            #[cfg(windows)]
            {
                const CREATE_NEW_CONSOLE: u32 = 0x00000010;
                Command::new("powershell.exe")
                    .args(ps_args)
                    .current_dir(parent)
                    .creation_flags(CREATE_NEW_CONSOLE)
                    .spawn()
                    .map_err(|ps_err| {
                        format!(
                            "Failed to launch PowerShell console (conhost: {}; powershell: {})",
                            conhost_err, ps_err
                        )
                    })
            }

            #[cfg(not(windows))]
            {
                let _ = conhost_err;
                Command::new("powershell")
                    .args(ps_args)
                    .current_dir(parent)
                    .spawn()
                    .map_err(|e| e.to_string())
            }
        }
    }
}

/// Shared Win32 C# interop + Per-Monitor DPI setup + clamped monitor-centered MoveWindow preamble.
fn build_ps_win32_preamble(
    file_name: &str,
    width: u32,
    height: u32,
    monitor_anchor: Option<(i32, i32)>,
) -> String {
    let safe_file = ps_escape_single(file_name);
    let monitor_lookup_stmt = match monitor_anchor {
        Some((ax, ay)) => format!(
            r#"        $__pt = New-Object Win32Cap+POINT
        $__pt.X = {ax}
        $__pt.Y = {ay}
        $hMon = [Win32Cap]::MonitorFromPoint($__pt, 2)"#,
            ax = ax,
            ay = ay
        ),
        None => r#"        $hMon = [Win32Cap]::MonitorFromWindow($hwnd, 1)"#.to_string(),
    };

    format!(
        r#"Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

public class Win32Cap {{
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT {{
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }}

    [StructLayout(LayoutKind.Sequential)]
    public struct POINT {{
        public int X;
        public int Y;
    }}

    [StructLayout(LayoutKind.Sequential)]
    public struct MONITORINFO {{
        public int cbSize;
        public RECT rcMonitor;
        public RECT rcWork;
        public uint dwFlags;
    }}

    [DllImport("kernel32.dll")]
    public static extern IntPtr GetConsoleWindow();

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);

    [DllImport("user32.dll")]
    public static extern bool MoveWindow(IntPtr hWnd, int X, int Y, int nWidth, int nHeight, bool bRepaint);

    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool IsIconic(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

    [DllImport("user32.dll")]
    public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdcBlt, uint nFlags);

    [DllImport("user32.dll")]
    public static extern bool SetProcessDPIAware();

    [DllImport("user32.dll")]
    public static extern IntPtr SetProcessDpiAwarenessContext(IntPtr dpiContext);

    [DllImport("user32.dll")]
    public static extern IntPtr MonitorFromPoint(POINT pt, uint dwFlags);

    [DllImport("user32.dll")]
    public static extern IntPtr MonitorFromWindow(IntPtr hwnd, uint dwFlags);

    [DllImport("user32.dll")]
    public static extern bool GetMonitorInfoW(IntPtr hMonitor, ref MONITORINFO lpmi);

    [DllImport("dwmapi.dll")]
    public static extern int DwmGetWindowAttribute(IntPtr hwnd, int dwAttribute, out RECT pvAttribute, int cbAttribute);
}}
'@
try {{ [Win32Cap]::SetProcessDpiAwarenessContext([IntPtr](-4)) | Out-Null }} catch {{ try {{ [Win32Cap]::SetProcessDPIAware() | Out-Null }} catch {{}} }}
$Host.UI.RawUI.WindowTitle = '{safe_file}'
try {{
    $hwnd = [Win32Cap]::GetConsoleWindow()
    if ($hwnd -eq [IntPtr]::Zero) {{
        try {{ $hwnd = (Get-Process -Id $PID).MainWindowHandle }} catch {{}}
    }}
    if ($hwnd -ne [IntPtr]::Zero) {{
        $maxSize = $Host.UI.RawUI.MaxPhysicalWindowSize
        $buf = $Host.UI.RawUI.BufferSize
        $buf.Width = [Math]::Max($buf.Width, [Math]::Max($maxSize.Width, 300))
        $buf.Height = [Math]::Max($buf.Height, 3000)
        $Host.UI.RawUI.BufferSize = $buf

{monitor_lookup_stmt}
        $mi = New-Object Win32Cap+MONITORINFO
        $mi.cbSize = [System.Runtime.InteropServices.Marshal]::SizeOf($mi)
        $gotMi = ($hMon -ne [IntPtr]::Zero) -and ([Win32Cap]::GetMonitorInfoW($hMon, [ref]$mi))
        if ($gotMi -and ($mi.rcWork.Right - $mi.rcWork.Left) -gt 0 -and ($mi.rcWork.Bottom - $mi.rcWork.Top) -gt 0) {{
            $workLeft = $mi.rcWork.Left
            $workTop = $mi.rcWork.Top
            $workRight = $mi.rcWork.Right
            $workBottom = $mi.rcWork.Bottom
        }} else {{
            $primaryWa = [System.Windows.Forms.Screen]::PrimaryScreen.WorkingArea
            $workLeft = $primaryWa.Left
            $workTop = $primaryWa.Top
            $workRight = $primaryWa.Right
            $workBottom = $primaryWa.Bottom
        }}

        $workW = [Math]::Max(320, $workRight - $workLeft)
        $workH = [Math]::Max(240, $workBottom - $workTop)
        $targetW = [Math]::Min([Math]::Max(320, {width}), $workW)
        $targetH = [Math]::Min([Math]::Max(240, {height}), $workH)
        $x = $workLeft + [Math]::Max(0, [int](($workW - $targetW) / 2))
        $y = $workTop + [Math]::Max(0, [int](($workH - $targetH) / 2))
        [Win32Cap]::MoveWindow($hwnd, $x, $y, $targetW, $targetH, $true) | Out-Null
        Start-Sleep -Milliseconds 100

        $winSize = $Host.UI.RawUI.WindowSize
        $newBuf = $Host.UI.RawUI.BufferSize
        $newBuf.Width = $winSize.Width
        $newBuf.Height = [Math]::Max($newBuf.Height, $winSize.Height)
        $Host.UI.RawUI.BufferSize = $newBuf

        # Second pass: re-center using exact cell-snapped outer pixel size so no edge crosses monitor bounds
        $actRect = New-Object Win32Cap+RECT
        if ([Win32Cap]::GetWindowRect($hwnd, [ref]$actRect)) {{
            $actW = [Math]::Max(1, $actRect.Right - $actRect.Left)
            $actH = [Math]::Max(1, $actRect.Bottom - $actRect.Top)
            $cx = $workLeft + [int](($workW - $actW) / 2)
            $cy = $workTop + [int](($workH - $actH) / 2)
            $cx = [Math]::Max($workLeft, [Math]::Min($cx, $workRight - $actW))
            $cy = [Math]::Max($workTop, [Math]::Min($cy, $workBottom - $actH))
            [Win32Cap]::MoveWindow($hwnd, $cx, $cy, $actW, $actH, $true) | Out-Null
            Start-Sleep -Milliseconds 50
        }}
    }}
}} catch {{}}
"#,
        safe_file = safe_file,
        monitor_lookup_stmt = monitor_lookup_stmt,
        width = width,
        height = height
    )
}

fn ps_write_status_stmt(st_path: &str, expr: &str) -> String {
    let safe_st = ps_escape_single(st_path);
    format!(
        "try {{ [System.IO.File]::WriteAllText('{safe_st}', {expr}, (New-Object System.Text.UTF8Encoding($false))) }} catch {{}}",
        safe_st = safe_st,
        expr = expr
    )
}

/// Shared PowerShell bitmap capture block (clipboard copy + deterministic PNG overwrite save).
fn build_ps_capture_block(
    save_clipboard: bool,
    save_image_path: Option<&str>,
    status_file_path: Option<&str>,
) -> String {
    let clipboard_stmt = if save_clipboard {
        r#"
                for ($i = 0; $i -lt 3; $i++) {
                    try {
                        [System.Windows.Forms.Clipboard]::SetImage($finalBmp)
                        break
                    } catch {
                        Start-Sleep -Milliseconds 100
                    }
                }
"#
    } else {
        ""
    };

    let save_file_stmt = if let Some(png_path) = save_image_path {
        let safe_png = ps_escape_single(png_path);
        let status_write = status_file_path
            .map(|st| {
                format!(
                    "\n                    {}\n",
                    ps_write_status_stmt(st, "'OK'")
                )
            })
            .unwrap_or_default();
        let status_err_write = status_file_path
            .map(|st| {
                format!(
                    "\n                    {}\n",
                    ps_write_status_stmt(st, "('ERR:' + $__bh_shot_err)")
                )
            })
            .unwrap_or_default();

        format!(
            r#"
                try {{
                    $__bh_png_path = '{safe_png}'
                    $__bh_png_dir = [System.IO.Path]::GetDirectoryName($__bh_png_path)
                    if ($__bh_png_dir) {{
                        [System.IO.Directory]::CreateDirectory($__bh_png_dir) | Out-Null
                    }}
                    $__bh_fs = New-Object System.IO.FileStream($__bh_png_path, [System.IO.FileMode]::Create, [System.IO.FileAccess]::Write, [System.IO.FileShare]::ReadWrite)
                    try {{
                        $finalBmp.Save($__bh_fs, [System.Drawing.Imaging.ImageFormat]::Png)
                        $__bh_fs.Flush()
                    }} finally {{
                        $__bh_fs.Dispose()
                    }}
                    if ([System.IO.File]::Exists($__bh_png_path) -and (Get-Item -LiteralPath $__bh_png_path).Length -gt 0) {{
                        $__bh_shot_ok = $true
                        {status_write}
                    }} else {{
                        $__bh_shot_ok = $false
                        $__bh_shot_err = 'Screenshot PNG file was empty or missing after save.'
                        {status_err_write}
                    }}
                }} catch {{
                    $__bh_shot_ok = $false
                    $__bh_shot_err = $_.Exception.Message
                    if (-not $__bh_shot_err) {{ $__bh_shot_err = [string]$_ }}
                    {status_err_write}
                }}
"#,
            safe_png = safe_png,
            status_write = status_write,
            status_err_write = status_err_write
        )
    } else {
        String::new()
    };

    let no_bmp_status_write = match (save_image_path, status_file_path) {
        (Some(_), Some(st_path)) => {
            let err_stmt = ps_write_status_stmt(st_path, "('ERR:' + $__bh_shot_err)");
            format!(
                r#"
    if (-not $__bh_shot_ok -and [string]::IsNullOrEmpty($__bh_shot_err)) {{
        $__bh_shot_err = 'Failed to capture console window bitmap.'
    }}
    if (-not $__bh_shot_ok) {{
        {err_stmt}
    }}
"#,
                err_stmt = err_stmt
            )
        }
        (Some(_), None) => r#"
    if (-not $__bh_shot_ok -and [string]::IsNullOrEmpty($__bh_shot_err)) {
        $__bh_shot_err = 'Failed to capture console window bitmap.'
    }
"#
        .to_string(),
        _ => String::new(),
    };

    let catch_status_write = if let (Some(_), Some(st_path)) = (save_image_path, status_file_path) {
        format!(
            "\n    {}\n",
            ps_write_status_stmt(st_path, "('ERR:' + $__bh_shot_err)")
        )
    } else {
        String::new()
    };

    format!(
        r#"
$__bh_shot_ok = $false
$__bh_shot_err = ''
try {{
    Start-Sleep -Milliseconds 300
    $hwnd = [Win32Cap]::GetConsoleWindow()
    if ($hwnd -eq [IntPtr]::Zero) {{
        try {{ $hwnd = (Get-Process -Id $PID).MainWindowHandle }} catch {{}}
    }}
    if ($hwnd -ne [IntPtr]::Zero) {{
        if ([Win32Cap]::IsIconic($hwnd)) {{
            [Win32Cap]::ShowWindow($hwnd, 9) | Out-Null
            Start-Sleep -Milliseconds 150
        }}

        $winRect = New-Object Win32Cap+RECT
        [Win32Cap]::GetWindowRect($hwnd, [ref]$winRect) | Out-Null
        $fullW = $winRect.Right - $winRect.Left
        $fullH = $winRect.Bottom - $winRect.Top

        $dwmRect = New-Object Win32Cap+RECT
        $hr = [Win32Cap]::DwmGetWindowAttribute($hwnd, 9, [ref]$dwmRect, [System.Runtime.InteropServices.Marshal]::SizeOf($dwmRect))
        if ($hr -eq 0 -and ($dwmRect.Right - $dwmRect.Left) -gt 0 -and ($dwmRect.Bottom - $dwmRect.Top) -gt 0) {{
            $capRect = $dwmRect
        }} else {{
            $capRect = $winRect
        }}

        $capW = $capRect.Right - $capRect.Left
        $capH = $capRect.Bottom - $capRect.Top

        if ($fullW -gt 0 -and $fullH -gt 0 -and $capW -gt 0 -and $capH -gt 0) {{
            $finalBmp = $null
            foreach ($pwFlags in @(2, 0)) {{
                $tmpBmp = New-Object System.Drawing.Bitmap($fullW, $fullH)
                $tmpG = [System.Drawing.Graphics]::FromImage($tmpBmp)
                $hdc = $tmpG.GetHdc()
                $pwOk = $false
                try {{
                    $pwOk = [Win32Cap]::PrintWindow($hwnd, $hdc, [uint32]$pwFlags)
                }} finally {{
                    $tmpG.ReleaseHdc($hdc)
                    $tmpG.Dispose()
                }}

                if ($pwOk) {{
                    $cropX = [Math]::Max(0, $capRect.Left - $winRect.Left)
                    $cropY = [Math]::Max(0, $capRect.Top - $winRect.Top)
                    $cropW = [Math]::Min($capW, $fullW - $cropX)
                    $cropH = [Math]::Min($capH, $fullH - $cropY)
                    if ($cropW -gt 0 -and $cropH -gt 0) {{
                        $pts = @(
                            @([Math]::Min($fullW - 1, $cropX + 20), [Math]::Min($fullH - 1, $cropY + 12)),
                            @([Math]::Min($fullW - 1, $cropX + [int]($cropW / 2)), [Math]::Min($fullH - 1, $cropY + [int]($cropH / 2))),
                            @([Math]::Min($fullW - 1, $cropX + [int]($cropW / 3)), [Math]::Min($fullH - 1, $cropY + [int]($cropH / 3))),
                            @([Math]::Min($fullW - 1, $cropX + [int](($cropW * 2) / 3)), [Math]::Min($fullH - 1, $cropY + [int]($cropH / 2))),
                            @([Math]::Min($fullW - 1, $cropX + $cropW - 20), [Math]::Min($fullH - 1, $cropY + $cropH - 20))
                        )
                        $hasContent = $false
                        foreach ($pt in $pts) {{
                            $px = $tmpBmp.GetPixel([Math]::Max(0, $pt[0]), [Math]::Max(0, $pt[1]))
                            if ($px.A -ne 0 -or $px.R -ne 0 -or $px.G -ne 0 -or $px.B -ne 0) {{
                                $hasContent = $true
                                break
                            }}
                        }}
                        if ($hasContent) {{
                            $finalBmp = New-Object System.Drawing.Bitmap($cropW, $cropH)
                            $fg = [System.Drawing.Graphics]::FromImage($finalBmp)
                            $srcRect = New-Object System.Drawing.Rectangle($cropX, $cropY, $cropW, $cropH)
                            $dstRect = New-Object System.Drawing.Rectangle(0, 0, $cropW, $cropH)
                            $fg.DrawImage($tmpBmp, $dstRect, $srcRect, [System.Drawing.GraphicsUnit]::Pixel)
                            $fg.Dispose()
                        }}
                    }}
                }}
                $tmpBmp.Dispose()
                if ($null -ne $finalBmp) {{ break }}
            }}

            if ($null -eq $finalBmp) {{
                [Win32Cap]::SetForegroundWindow($hwnd) | Out-Null
                Start-Sleep -Milliseconds 150
                $finalBmp = New-Object System.Drawing.Bitmap($capW, $capH)
                $sg = [System.Drawing.Graphics]::FromImage($finalBmp)
                $sz = New-Object System.Drawing.Size($capW, $capH)
                $sg.CopyFromScreen($capRect.Left, $capRect.Top, 0, 0, $sz, [System.Drawing.CopyPixelOperation]::SourceCopy)
                $sg.Dispose()
            }}

            if ($null -ne $finalBmp) {{
                {clipboard_stmt}
                {save_file_stmt}
                $finalBmp.Dispose()
            }}
        }} else {{
            $__bh_shot_err = "Invalid window dimensions ($fullW x $fullH) for screenshot capture."
        }}
    }} else {{
        $__bh_shot_err = 'Console window handle (HWND) is unavailable for screenshot capture.'
    }}
    {no_bmp_status_write}
}} catch {{
    $__bh_shot_err = $_.Exception.Message
    if (-not $__bh_shot_err) {{ $__bh_shot_err = [string]$_ }}
    {catch_status_write}
}}
"#,
        clipboard_stmt = clipboard_stmt,
        save_file_stmt = save_file_stmt,
        no_bmp_status_write = no_bmp_status_write,
        catch_status_write = catch_status_write
    )
}

fn build_ps_meta_write_block(meta_file_path: &str) -> String {
    let safe_meta = ps_escape_single(meta_file_path);
    format!(
        r#"try {{
    $__metaObj = @{{
        exit_code = $__bh_exit
        screenshot_saved = [bool]$__bh_shot_ok
        screenshot_error = if ($__bh_shot_ok -or [string]::IsNullOrEmpty($__bh_shot_err)) {{ $null }} else {{ $__bh_shot_err }}
    }}
    $__json = $__metaObj | ConvertTo-Json -Compress
    $__sb = New-Object System.Text.StringBuilder
    foreach ($__ch in $__json.ToCharArray()) {{
        $__code = [int][char]$__ch
        if ($__code -le 127) {{
            [void]$__sb.Append($__ch)
        }} else {{
            [void]$__sb.AppendFormat('\u{{0:x4}}', $__code)
        }}
    }}
    [System.IO.File]::WriteAllText('{safe_meta}', $__sb.ToString(), [System.Text.Encoding]::ASCII)
}} catch {{}}"#,
        safe_meta = safe_meta
    )
}

/// Build PowerShell script to center/resize console, run `.\<file_name>`, capture optional screenshot, and report metadata.
pub fn build_powershell_run_script(
    parent_dir: &str,
    file_name: &str,
    setting: &crate::modules::app_config::UserSetting,
    save_image_path: Option<&str>,
    meta_file_path: Option<&str>,
) -> String {
    build_powershell_run_script_on_monitor(
        parent_dir,
        file_name,
        setting,
        save_image_path,
        meta_file_path,
        None,
    )
}

/// Monitor-aware PowerShell runner script builder for individual BAT execution.
pub fn build_powershell_run_script_on_monitor(
    parent_dir: &str,
    file_name: &str,
    setting: &crate::modules::app_config::UserSetting,
    save_image_path: Option<&str>,
    meta_file_path: Option<&str>,
    monitor_anchor: Option<(i32, i32)>,
) -> String {
    let safe_dir = ps_escape_single(parent_dir);
    let safe_file = ps_escape_single(file_name);
    let preamble = build_ps_win32_preamble(
        file_name,
        setting.ps_window_width,
        setting.ps_window_height,
        monitor_anchor,
    );

    let need_capture = setting.auto_capture_clipboard || save_image_path.is_some();
    let capture_block = if need_capture {
        build_ps_capture_block(setting.auto_capture_clipboard, save_image_path, None)
    } else {
        "$__bh_shot_ok = $false\n$__bh_shot_err = ''".to_string()
    };

    let meta_block = meta_file_path
        .map(build_ps_meta_write_block)
        .unwrap_or_default();

    let prompt_hook = if need_capture || meta_file_path.is_some() {
        r#"$global:__bh_cap_done = $false
function global:prompt {
    $p = 'PS ' + (Get-Location).Path + '>'
    if (-not $global:__bh_cap_done) {
        $global:__bh_cap_done = $true
        return ' '
    }
    return ($p + ' ')
}"#
    } else {
        ""
    };

    format!(
        r#"{preamble}
Set-Location -LiteralPath '{safe_dir}'
Clear-Host
Write-Host ('PS ' + (Get-Location).Path + '> .\{safe_file}')
$__bh_exit = 0
$__bh_shot_ok = $false
$__bh_shot_err = ''
$global:LASTEXITCODE = 0
try {{
    & '.\{safe_file}'
    if ($null -ne $LASTEXITCODE) {{
        $__bh_exit = [int]$LASTEXITCODE
    }} elseif (-not $?) {{
        $__bh_exit = 1
    }}
}} catch {{
    $__bh_exit = 1
    Write-Host $_.Exception.Message -ForegroundColor Red
}}
Write-Host -NoNewline ('PS ' + (Get-Location).Path + '>')
{capture_block}
{meta_block}
{prompt_hook}
"#,
        preamble = preamble,
        safe_file = safe_file,
        safe_dir = safe_dir,
        capture_block = capture_block,
        meta_block = meta_block,
        prompt_hook = prompt_hook
    )
}

/// Build sequential PowerShell script for Group Run All with Auto Save Image PNG capture.
pub fn build_powershell_group_step_script(
    parent_dir: &str,
    file_name: &str,
    setting: &crate::modules::app_config::UserSetting,
    save_image_path: &str,
    out_file_path: &str,
    meta_file_path: &str,
) -> String {
    build_powershell_group_step_script_on_monitor(
        parent_dir,
        file_name,
        setting,
        save_image_path,
        out_file_path,
        meta_file_path,
        None,
    )
}

/// Monitor-aware PowerShell script builder for sequential Group Run All steps.
pub fn build_powershell_group_step_script_on_monitor(
    parent_dir: &str,
    file_name: &str,
    setting: &crate::modules::app_config::UserSetting,
    save_image_path: &str,
    out_file_path: &str,
    meta_file_path: &str,
    monitor_anchor: Option<(i32, i32)>,
) -> String {
    let safe_dir = ps_escape_single(parent_dir);
    let safe_file = ps_escape_single(file_name);
    let safe_out = ps_escape_single(out_file_path);
    let preamble = build_ps_win32_preamble(
        file_name,
        setting.ps_window_width,
        setting.ps_window_height,
        monitor_anchor,
    );
    let capture_block =
        build_ps_capture_block(setting.auto_capture_clipboard, Some(save_image_path), None);
    let meta_block = build_ps_meta_write_block(meta_file_path);

    format!(
        r#"{preamble}
Set-Location -LiteralPath '{safe_dir}'
Clear-Host
$__bh_lines = New-Object System.Collections.Generic.List[string]
Write-Host ('PS ' + (Get-Location).Path + '> .\{safe_file}')
$__bh_exit = 0
$global:LASTEXITCODE = 0
try {{
    & '.\{safe_file}' 2>&1 | ForEach-Object {{
        $__s = $_.ToString()
        $__bh_lines.Add($__s)
        Write-Host $__s
    }}
    if ($null -ne $LASTEXITCODE) {{
        $__bh_exit = [int]$LASTEXITCODE
    }} elseif (-not $?) {{
        $__bh_exit = 1
    }}
}} catch {{
    $__errLine = $_.Exception.Message
    if ($__errLine) {{ $__bh_lines.Add($__errLine) }}
    $__bh_exit = 1
}}
Write-Host -NoNewline ('PS ' + (Get-Location).Path + '>')
{capture_block}
try {{
    [System.IO.File]::WriteAllLines('{safe_out}', $__bh_lines, (New-Object System.Text.UTF8Encoding($false)))
}} catch {{}}
{meta_block}
"#,
        preamble = preamble,
        safe_dir = safe_dir,
        safe_file = safe_file,
        capture_block = capture_block,
        safe_out = safe_out,
        meta_block = meta_block
    )
}

#[derive(Debug, Deserialize)]
struct GroupPsMeta {
    exit_code: Option<i32>,
    screenshot_saved: bool,
    screenshot_error: Option<String>,
}

/// Launch a `.bat` file in a dedicated Win32 console PowerShell window (`.\<file.bat>`).
pub fn execute_bat_in_powershell(
    app: Option<AppHandle>,
    tab_id: Option<&str>,
    path: &str,
    project_path: Option<&str>,
    monitor_anchor: Option<(i32, i32)>,
) -> Result<BatLaunchResult, String> {
    let setting = crate::modules::app_config::load_user_setting();
    execute_bat_in_powershell_with_setting(
        app,
        tab_id,
        path,
        project_path,
        &setting,
        monitor_anchor,
    )
}

/// Parameterized `.bat` launcher for deterministic testing and live execution.
pub fn execute_bat_in_powershell_with_setting(
    app: Option<AppHandle>,
    tab_id: Option<&str>,
    path: &str,
    project_path: Option<&str>,
    setting: &crate::modules::app_config::UserSetting,
    monitor_anchor: Option<(i32, i32)>,
) -> Result<BatLaunchResult, String> {
    let clean_path = path.trim().trim_matches('"').replace('/', "\\");
    let p = Path::new(&clean_path);
    if !p.is_file() {
        return Err(format!("File does not exist: {}", clean_path));
    }
    let parent = p.parent().unwrap_or(p);
    let parent_str = parent.to_string_lossy().to_string();
    let file_name = p
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or_else(|| "Invalid batch file name".to_string())?;

    let mut planned_png: Option<String> = None;
    let mut screenshot_error: Option<String> = None;

    if setting.auto_save_image {
        let target_png = crate::modules::app_config::resolve_screenshot_output_path(
            setting,
            project_path,
            &clean_path,
        );
        let parent_dir = target_png.parent().unwrap_or_else(|| Path::new("."));
        match crate::modules::app_config::validate_writable_directory(parent_dir) {
            Ok(()) => {
                planned_png = Some(target_png.to_string_lossy().replace('/', "\\"));
            }
            Err(e) => {
                screenshot_error = Some(e);
            }
        }
    }

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = PS_SCRIPT_SEQ.fetch_add(1, Ordering::Relaxed);
    let meta_file_path = std::env::temp_dir().join(format!(
        "bh_single_meta_{}_{}_{}.json",
        std::process::id(),
        nanos,
        seq
    ));
    let meta_file_str = meta_file_path.to_string_lossy().to_string();

    let base_script = build_powershell_run_script_on_monitor(
        &parent_str,
        &file_name,
        setting,
        planned_png.as_deref(),
        Some(&meta_file_str),
        monitor_anchor,
    );
    let ps1_path = allocate_temp_ps1_path("bh_bat_run");
    let full_script = append_ps1_self_cleanup(&base_script, &ps1_path);
    write_ps1_with_bom(&ps1_path, &full_script)?;

    let ps1_str = ps1_path.to_string_lossy().to_string();
    let mut child = match spawn_conhost_powershell(
        parent,
        &[
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-NoExit",
            "-STA",
            "-File",
            &ps1_str,
        ],
    ) {
        Ok(c) => c,
        Err(e) => {
            let _ = fs::remove_file(&ps1_path);
            let _ = fs::remove_file(&meta_file_path);
            return Err(e);
        }
    };

    if let Some(app_handle) = app {
        let bat_name_clone = file_name.clone();
        let path_clone = clean_path.clone();
        let tab_id_clone = tab_id.unwrap_or_default().to_string();
        let planned_png_clone = planned_png.clone();
        let initial_shot_err = screenshot_error.clone();
        let auto_save_on = setting.auto_save_image;

        std::thread::spawn(move || {
            let max_wait = std::time::Duration::from_secs(600);
            let start = std::time::Instant::now();
            let mut exited_grace_checks = 0u8;

            while start.elapsed() < max_wait {
                if meta_file_path.is_file() {
                    if let Ok(raw) = fs::read_to_string(&meta_file_path) {
                        let _ = fs::remove_file(&meta_file_path);
                        let clean_json = raw.trim_start_matches('\u{feff}').trim();
                        let parsed = serde_json::from_str::<GroupPsMeta>(clean_json).ok();
                        let exit_code = parsed.as_ref().and_then(|m| m.exit_code);
                        let success = exit_code == Some(0);

                        let (shot_saved, shot_err) = if let Some(ref png_dest) = planned_png_clone {
                            let file_non_empty =
                                fs::metadata(png_dest).map(|m| m.len() > 0).unwrap_or(false);
                            match parsed {
                                Some(ref m) => {
                                    let saved = m.screenshot_saved && file_non_empty;
                                    let err = m.screenshot_error.clone().or_else(|| {
                                        if auto_save_on && !saved {
                                            Some(
                                                "Screenshot capture did not produce a valid PNG file."
                                                    .to_string(),
                                            )
                                        } else {
                                            None
                                        }
                                    });
                                    (saved, err)
                                }
                                None => (
                                    file_non_empty,
                                    if !file_non_empty {
                                        Some("Screenshot metadata could not be parsed.".to_string())
                                    } else {
                                        None
                                    },
                                ),
                            }
                        } else {
                            (false, initial_shot_err.clone())
                        };

                        let _ = app_handle.emit_to(
                            "main",
                            "bat-execution-finished",
                            BatExecutionFinishedEvent {
                                tab_id: tab_id_clone,
                                path: path_clone,
                                bat_name: bat_name_clone,
                                success,
                                exit_code,
                                screenshot_saved: shot_saved,
                                screenshot_path: planned_png_clone,
                                screenshot_error: shot_err,
                            },
                        );
                        return;
                    }
                }

                if let Ok(Some(exit_status)) = child.try_wait() {
                    exited_grace_checks += 1;
                    if exited_grace_checks >= 4 {
                        let _ = fs::remove_file(&meta_file_path);
                        let shot_err = if planned_png_clone.is_some() {
                            Some("PowerShell runner closed before batch completion.".to_string())
                        } else {
                            initial_shot_err
                        };
                        let _ = app_handle.emit_to(
                            "main",
                            "bat-execution-finished",
                            BatExecutionFinishedEvent {
                                tab_id: tab_id_clone,
                                path: path_clone,
                                bat_name: bat_name_clone,
                                success: exit_status.success(),
                                exit_code: exit_status.code(),
                                screenshot_saved: false,
                                screenshot_path: planned_png_clone,
                                screenshot_error: shot_err,
                            },
                        );
                        return;
                    }
                }

                std::thread::sleep(std::time::Duration::from_millis(200));
            }

            let _ = fs::remove_file(&meta_file_path);
        });
    }

    Ok(BatLaunchResult {
        launched: true,
        screenshot_path: planned_png,
        screenshot_error,
    })
}

fn run_hidden_bat_cmd(
    clean_path: &str,
    name: &str,
    parent: &Path,
    start: std::time::Instant,
    screenshot_path: Option<String>,
    screenshot_error: Option<String>,
) -> BatGroupStepResult {
    #[cfg(windows)]
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let mut cmd = Command::new("cmd.exe");
    cmd.args(["/c", name]);
    cmd.current_dir(parent);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    match cmd.output() {
        Ok(output) => {
            let (stdout, _) = detect_and_decode_text(&output.stdout);
            let (stderr, _) = detect_and_decode_text(&output.stderr);
            let exit_code = output.status.code();
            let success = output.status.success();
            let error_message = if success {
                None
            } else {
                Some(format!(
                    "Exited with code {}",
                    exit_code
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                ))
            };

            BatGroupStepResult {
                path: clean_path.to_string(),
                name: name.to_string(),
                success,
                exit_code,
                duration_ms: start.elapsed().as_millis() as u64,
                stdout,
                stderr,
                error_message,
                screenshot_path,
                screenshot_saved: false,
                screenshot_error,
            }
        }
        Err(e) => {
            let err_msg = format!("Failed to execute '{}': {}", name, e);
            BatGroupStepResult {
                path: clean_path.to_string(),
                name: name.to_string(),
                success: false,
                exit_code: None,
                duration_ms: start.elapsed().as_millis() as u64,
                stdout: String::new(),
                stderr: String::new(),
                error_message: Some(err_msg),
                screenshot_path,
                screenshot_saved: false,
                screenshot_error,
            }
        }
    }
}

/// Execute a single `.bat` script sequentially and capture stdout/stderr/exit status (plus Auto Save Image if enabled).
pub fn execute_bat_group_step(
    path: &str,
    project_path: Option<&str>,
    monitor_anchor: Option<(i32, i32)>,
) -> BatGroupStepResult {
    let setting = crate::modules::app_config::load_user_setting();
    execute_bat_group_step_with_setting_on_monitor(path, project_path, &setting, monitor_anchor)
}

/// Core sequential `.bat` step executor parameterized by `UserSetting` for clean testability.
pub fn execute_bat_group_step_with_setting(
    path: &str,
    project_path: Option<&str>,
    setting: &crate::modules::app_config::UserSetting,
) -> BatGroupStepResult {
    execute_bat_group_step_with_setting_on_monitor(path, project_path, setting, None)
}

/// Core sequential `.bat` step executor with optional multi-monitor anchor coordinate.
pub fn execute_bat_group_step_with_setting_on_monitor(
    path: &str,
    project_path: Option<&str>,
    setting: &crate::modules::app_config::UserSetting,
    monitor_anchor: Option<(i32, i32)>,
) -> BatGroupStepResult {
    let start = std::time::Instant::now();
    let clean_path = path.trim().trim_matches('"').replace('/', "\\");
    let p = Path::new(&clean_path);
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| clean_path.clone());

    if !p.is_file() {
        return BatGroupStepResult {
            path: clean_path.clone(),
            name,
            success: false,
            exit_code: None,
            duration_ms: start.elapsed().as_millis() as u64,
            stdout: String::new(),
            stderr: String::new(),
            error_message: Some(format!("File does not exist: {}", clean_path)),
            screenshot_path: None,
            screenshot_saved: false,
            screenshot_error: None,
        };
    }

    let parent = p.parent().unwrap_or(p);

    if !setting.auto_save_image || cfg!(not(windows)) {
        return run_hidden_bat_cmd(&clean_path, &name, parent, start, None, None);
    }

    #[cfg(windows)]
    {
        let target_png = crate::modules::app_config::resolve_screenshot_output_path(
            setting,
            project_path,
            &clean_path,
        );
        let target_png_str = target_png.to_string_lossy().replace('/', "\\");
        let png_parent = target_png.parent().unwrap_or_else(|| Path::new("."));

        if let Err(dir_err) = crate::modules::app_config::validate_writable_directory(png_parent) {
            return run_hidden_bat_cmd(
                &clean_path,
                &name,
                parent,
                start,
                Some(target_png_str),
                Some(dir_err),
            );
        }

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let temp_dir = std::env::temp_dir();
        let out_file = temp_dir.join(format!("bh_grp_out_{}_{}.txt", std::process::id(), nanos));
        let meta_file = temp_dir.join(format!("bh_grp_meta_{}_{}.json", std::process::id(), nanos));

        let parent_str = parent.to_string_lossy().to_string();
        let out_str = out_file.to_string_lossy().to_string();
        let meta_str = meta_file.to_string_lossy().to_string();

        let script = build_powershell_group_step_script_on_monitor(
            &parent_str,
            &name,
            setting,
            &target_png_str,
            &out_str,
            &meta_str,
            monitor_anchor,
        );
        let ps1_path = allocate_temp_ps1_path("bh_grp_run");
        if let Err(ps_err) = write_ps1_with_bom(&ps1_path, &script) {
            return run_hidden_bat_cmd(
                &clean_path,
                &name,
                parent,
                start,
                Some(target_png_str),
                Some(ps_err),
            );
        }
        let ps1_str = ps1_path.to_string_lossy().to_string();

        let child_res = spawn_conhost_powershell(
            parent,
            &[
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-STA",
                "-File",
                &ps1_str,
            ],
        );

        match child_res {
            Ok(mut child) => {
                let wait_res = child.wait();
                let stdout = fs::read_to_string(&out_file)
                    .unwrap_or_default()
                    .trim_start_matches('\u{feff}')
                    .to_string();
                let meta_raw = fs::read_to_string(&meta_file)
                    .unwrap_or_default()
                    .trim_start_matches('\u{feff}')
                    .to_string();
                let _ = fs::remove_file(&ps1_path);
                let _ = fs::remove_file(&out_file);
                let _ = fs::remove_file(&meta_file);

                let meta = serde_json::from_str::<GroupPsMeta>(meta_raw.trim()).ok();
                let fallback_exit = wait_res.ok().and_then(|s| s.code());
                let exit_code = meta.as_ref().and_then(|m| m.exit_code).or(fallback_exit);
                let success = exit_code == Some(0);
                let error_message = if success {
                    None
                } else {
                    Some(format!(
                        "Exited with code {}",
                        exit_code
                            .map(|c| c.to_string())
                            .unwrap_or_else(|| "unknown".to_string())
                    ))
                };

                let file_non_empty = fs::metadata(&target_png)
                    .map(|m| m.len() > 0)
                    .unwrap_or(false);
                let (shot_saved, shot_err) = match meta {
                    Some(m) => (
                        m.screenshot_saved && file_non_empty,
                        m.screenshot_error.or_else(|| {
                            if m.screenshot_saved && !file_non_empty {
                                Some(
                                    "Screenshot PNG file was missing or empty after execution."
                                        .to_string(),
                                )
                            } else {
                                None
                            }
                        }),
                    ),
                    None => (
                        file_non_empty,
                        if file_non_empty {
                            None
                        } else {
                            Some("Screenshot capture did not complete cleanly.".to_string())
                        },
                    ),
                };

                BatGroupStepResult {
                    path: clean_path,
                    name,
                    success,
                    exit_code,
                    duration_ms: start.elapsed().as_millis() as u64,
                    stdout,
                    stderr: String::new(),
                    error_message,
                    screenshot_path: Some(target_png_str),
                    screenshot_saved: shot_saved,
                    screenshot_error: shot_err,
                }
            }
            Err(e) => {
                let _ = fs::remove_file(&ps1_path);
                let _ = fs::remove_file(&out_file);
                let _ = fs::remove_file(&meta_file);
                run_hidden_bat_cmd(
                    &clean_path,
                    &name,
                    parent,
                    start,
                    Some(target_png_str),
                    Some(format!(
                        "Failed to launch PowerShell screenshot runner: {}",
                        e
                    )),
                )
            }
        }
    }
}
