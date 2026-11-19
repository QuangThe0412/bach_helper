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
    pub bat_folder: Option<String>,
    pub bat_files: Vec<BatFileInfo>,
    pub batch_log_name: Option<String>,
    pub app_log_name: Option<String>,
    pub log_file_path: Option<String>,
    pub app_log_file_path: Option<String>,
    pub log_folder_path: Option<String>,
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
        return Err(format!("Missing bin/{} in JDK directory: {}", java_name, root.display()));
    }
    if !javac_bin.is_file() {
        return Err(format!("Missing bin/{} compiler in JDK directory: {}", javac_name, root.display()));
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
                    if seg_str.is_ascii()
                        && !seg_str.bytes().any(|b| b.is_ascii_whitespace())
                    {
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
fn resolve_long_path_best_effort(path: &Path) -> PathBuf {
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
        let encoded = encode_text(&final_text, save_encoding)
            .or_else(|_| encode_text(&final_text, TextEncoding::Utf8))
            .unwrap_or_else(|_| final_text.as_bytes().to_vec());
        let _ = fs::write(&build_xml_path, encoded);
    }

    changed_to
}

/// Check and fix project paths & all Java Home keys in the target's .properties file.
/// Uses Windows 8.3 short paths for space/non-ASCII segments and `\uXXXX` escaping so Java's
/// `Properties.load(InputStream)` & Ant `<filterset>` never corrupt Japanese characters into `????`.
pub fn fix_properties_file_paths(
    prop_file: &Path,
    project_path: &str,
    active_java_home: Option<&str>,
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

    let java_slash = active_java_home
        .and_then(|jh| validate_java_home_dir(jh).ok())
        .map(|valid_jh| {
            let safe_jh = to_batch_safe_path(&valid_jh.to_string_lossy())
                .trim()
                .trim_matches('"')
                .replace('\\', "/")
                .trim_end_matches('/')
                .to_string();
            escape_properties_value(&safe_jh)
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
            let val_raw_un = unescape_properties_value(trimmed[idx + 1..].trim());
            let val = val_raw_un.replace('\\', "/");
            if key == "filter.shellscript.baseDirectory.value" {
                let clean_val = val.trim_end_matches('/');
                if clean_val.to_lowercase().ends_with("/target") {
                    let prefix_len = clean_val.len() - "/target".len();
                    old_proj_prefix = Some(clean_val[..prefix_len].to_string());
                    break;
                }
            } else if key == "pom.log4j.path.value" && old_proj_prefix.is_none() {
                let val_lower = val.to_lowercase();
                if let Some(t_idx) = val_lower.find("/target/") {
                    if val.is_char_boundary(t_idx) {
                        old_proj_prefix = Some(val[..t_idx].to_string());
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

            if is_java_home_property_key(key) {
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
                let val_decoded = unescape_properties_value(val);
                let val_norm = val_decoded.replace('\\', "/");
                let val_lower = val_norm.to_lowercase();
                if let Some(t_idx) = val_lower.find("/target/") {
                    if val_norm.is_char_boundary(t_idx) {
                        let suffix = escape_properties_value(&val_norm[t_idx..]);
                        let desired = format!("{}{}", proj_slash, suffix);
                        if val != desired {
                            new_val = Some(desired);
                        }
                    }
                } else if let Some(ref old_p) = old_proj_prefix {
                    if !old_p.is_empty() {
                        let old_lower = old_p.to_lowercase();
                        if val_lower.starts_with(&old_lower) && val_norm.is_char_boundary(old_p.len())
                        {
                            let suffix = escape_properties_value(&val_norm[old_p.len()..]);
                            let desired = format!("{}{}", proj_slash, suffix);
                            if val != desired {
                                new_val = Some(desired);
                            }
                        } else if !val.is_ascii() {
                            let escaped = escape_properties_value(val);
                            if val != escaped {
                                new_val = Some(escaped);
                            }
                        }
                    }
                } else if !val.is_ascii() {
                    let escaped = escape_properties_value(val);
                    if val != escaped {
                        new_val = Some(escaped);
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
        let encoded = encode_text(&final_text, orig_encoding)
            .or_else(|_| encode_text(&final_text, TextEncoding::Utf8))
            .unwrap_or_else(|_| final_text.as_bytes().to_vec());
        let _ = fs::write(prop_file, encoded);
    }

    changed
}

/// Apply explicit Auto Config to build.xml javac path and target .properties paths
pub fn run_auto_config(
    project_path: &str,
    target: &str,
    java_home: Option<&str>,
) -> Result<AutoConfigResult, String> {
    let build_dir = find_build_dir(project_path).ok_or_else(|| {
        format!(
            "Cannot find build.xml in '{}' or its 'build' subfolder",
            project_path
        )
    })?;

    let safe_proj = to_batch_safe_path(project_path)
        .trim()
        .trim_matches('"')
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();

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
        if fix_properties_file_paths(pp, project_path, active_java_home.as_deref()) {
            properties_updated = true;
            let prop_name = pp
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "properties".to_string());
            if let Some(ref jh) = active_java_home {
                let safe_jh = to_batch_safe_path(jh).replace('\\', "/");
                changes.push(format!("{} paths -> {}, JavaHome -> {}", prop_name, safe_proj, safe_jh));
            } else {
                changes.push(format!("{} paths -> {}", prop_name, safe_proj));
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
    let clean_proj = project_path.trim().trim_matches('"').replace('/', "\\");
    let base_proj = Path::new(&clean_proj);
    let source_batch_dir = base_proj.join("batch");

    let mut raw_dirs: Vec<PathBuf> = Vec::new();

    if let Some(prop_path) = properties_file {
        if let Some(log4j_dir) = extract_property_value(prop_path, "pom.log4j.path.value") {
            let clean_slash = log4j_dir.trim().replace('\\', "/");
            if !clean_slash.is_empty() {
                let candidate = PathBuf::from(clean_slash.replace('/', "\\"));
                if is_path_within_dir(&candidate, base_proj) {
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
                            let replaced = clean_slash.replace(&std_desk, od_str.trim_end_matches('/'));
                            let od_candidate = PathBuf::from(replaced.replace('/', "\\"));
                            if is_path_within_dir(&od_candidate, base_proj) {
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
            if is_path_within_dir(&direct, base_proj)
                && !paths_equal_ci(&direct, base_proj)
                && !paths_equal_ci(&direct, &source_batch_dir)
            {
                raw_dirs.push(direct.join("log"));
            }
        }
    }

    raw_dirs.push(base_proj.join("target").join("log"));

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
    let candidate_dirs = resolve_target_log_dirs(project_path, properties_file, base_dir_str);

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
                    let appender_orig = &key["log4j.appender.".len().."log4j.appender.".len() + dot_rel];
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
    let clean_proj = project_path.trim().trim_matches('"').replace('/', "\\");
    if clean_proj.is_empty() {
        return (None, None);
    }
    let base_proj = Path::new(&clean_proj);
    let source_batch_dir = base_proj.join("batch");

    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Some(base_str) = base_dir_str {
        let clean_base = base_str.trim().replace('/', "\\");
        let clean_trimmed = clean_base.trim_end_matches('\\');
        if !clean_trimmed.is_empty() {
            let bdir = PathBuf::from(clean_trimmed);
            if is_path_within_dir(&bdir, base_proj)
                && !paths_equal_ci(&bdir, base_proj)
                && !paths_equal_ci(&bdir, &source_batch_dir)
            {
                candidates.push(bdir.join("batch").join("properties").join("stcliblog4j.properties"));
                candidates.push(bdir.join("properties").join("stcliblog4j.properties"));
                candidates.push(bdir.join("stcliblog4j.properties"));
            }
        }
    }

    candidates.push(
        base_proj
            .join("target")
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

    for root_dir in [base_proj.join("target"), base_proj.join("batch")] {
        if root_dir.is_dir() {
            for entry in walkdir::WalkDir::new(&root_dir).max_depth(4).into_iter().flatten() {
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
        let ext: Vec<u16> = OsStr::new(".txt").encode_wide().chain(std::iter::once(0)).collect();
        let verb: Vec<u16> = OsStr::new("open").encode_wide().chain(std::iter::once(0)).collect();

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
        let op_w: Vec<u16> = OsStr::new("open").encode_wide().chain(std::iter::once(0)).collect();
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
        candidate_dirs.extend(resolve_target_log_dirs(
            proj_str,
            prop_file.as_deref(),
            base_dir.as_deref(),
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
            proj_opt
                .as_ref()
                .map(|p| Path::new(p).join("target").join("log"))
        })
        .unwrap_or_else(|| deduped[0].clone());

    let expected_path = expected_dir
        .join(clean_name)
        .to_string_lossy()
        .replace('/', "\\");

    Err(format!("Log file not found:\n{}", expected_path))
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
pub fn scan_bat_files(project_path: &str, base_dir_str: Option<&str>) -> (Option<PathBuf>, Vec<BatFileInfo>) {
    let clean_proj_str = project_path.trim().trim_matches('"').replace('/', "\\");
    let base_proj = Path::new(&clean_proj_str);
    let source_batch_dir = base_proj.join("batch");
    let source_batfiles_dir = source_batch_dir.join("batfiles");
    let proj_has_tilde = clean_proj_str.contains('~');

    // Always prefer the selected project's own target folders first
    let mut candidate_folders: Vec<PathBuf> = vec![
        base_proj.join("target").join("batch").join("batfiles"),
        base_proj.join("target").join("batfiles"),
    ];

    // Only accept properties base_dir candidates that reside strictly inside the selected project
    if let Some(base_str) = base_dir_str {
        let base_norm = base_str.trim().replace('/', "\\");
        let clean_base = base_norm.trim_end_matches('\\');
        if !clean_base.is_empty() {
            let direct_path = PathBuf::from(clean_base);
            if is_path_within_dir(&direct_path, base_proj)
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
                            if is_path_within_dir(&p, base_proj)
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
        if paths_equal_ci(&folder, &source_batfiles_dir) || paths_equal_ci(&folder, &source_batch_dir) {
            continue;
        }
        if folder.is_dir() {
            let display_dir = if !proj_has_tilde && folder.to_string_lossy().contains('~') {
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

    // Check nested directories inside target (e.g., target/20-honban-psp/hansoku/batch/batfiles)
    let target_root = base_proj.join("target");
    if target_root.is_dir() {
        for entry in walkdir::WalkDir::new(&target_root)
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
                    let display_dir = if !proj_has_tilde && dir_path.to_string_lossy().contains('~') {
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
    let (batch_log_name, app_log_name) =
        discover_configured_log_files(project_path, base_dir.as_deref());

    let (log_file_path, batch_log_folder) = if let Some(ref b_name) = batch_log_name {
        resolve_log_file(project_path, prop_file.as_deref(), base_dir.as_deref(), b_name)
    } else {
        (None, None)
    };

    let (app_log_file_path, app_log_folder) = if let Some(ref a_name) = app_log_name {
        resolve_log_file(project_path, prop_file.as_deref(), base_dir.as_deref(), a_name)
    } else {
        (None, None)
    };

    let log_folder_path = batch_log_folder.or(app_log_folder).or_else(|| {
        resolve_target_log_dirs(project_path, prop_file.as_deref(), base_dir.as_deref())
            .into_iter()
            .find(|d| d.is_dir())
            .map(|d| d.to_string_lossy().replace('/', "\\"))
    });

    PostBuildInspection {
        properties_file: prop_file.map(|p| p.to_string_lossy().to_string()),
        base_directory: base_dir,
        bat_folder: bat_folder.map(|p| p.to_string_lossy().to_string()),
        bat_files,
        batch_log_name,
        app_log_name,
        log_file_path,
        app_log_file_path,
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
    _java_home: Option<String>,
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
        let (xml_str, _) = detect_and_decode_text(&xml_bytes);
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

/// Build PowerShell script to resize console, run `.\<file_name>`, and optionally capture to clipboard.
pub fn build_powershell_run_script(
    parent_dir: &str,
    file_name: &str,
    setting: &crate::modules::app_config::UserSetting,
) -> String {
    let safe_dir = parent_dir.replace('\'', "''");
    let safe_file = file_name.replace('\'', "''");
    let width = setting.ps_window_width;
    let height = setting.ps_window_height;

    let capture_block = if setting.auto_capture_clipboard {
        r#"
try {
    Start-Sleep -Milliseconds 300
    $hwnd = [Win32Cap]::GetConsoleWindow()
    if ($hwnd -ne [IntPtr]::Zero) {
        $winRect = New-Object Win32Cap+RECT
        [Win32Cap]::GetWindowRect($hwnd, [ref]$winRect) | Out-Null
        $fullW = $winRect.Right - $winRect.Left
        $fullH = $winRect.Bottom - $winRect.Top

        $dwmRect = New-Object Win32Cap+RECT
        $hr = [Win32Cap]::DwmGetWindowAttribute($hwnd, 9, [ref]$dwmRect, [System.Runtime.InteropServices.Marshal]::SizeOf($dwmRect))
        if ($hr -eq 0 -and ($dwmRect.Right - $dwmRect.Left) -gt 0 -and ($dwmRect.Bottom - $dwmRect.Top) -gt 0) {
            $capRect = $dwmRect
        } else {
            $capRect = $winRect
        }

        $capW = $capRect.Right - $capRect.Left
        $capH = $capRect.Bottom - $capRect.Top

        if ($fullW -gt 0 -and $fullH -gt 0 -and $capW -gt 0 -and $capH -gt 0) {
            $finalBmp = $null
            $tmpBmp = New-Object System.Drawing.Bitmap($fullW, $fullH)
            $tmpG = [System.Drawing.Graphics]::FromImage($tmpBmp)
            $hdc = $tmpG.GetHdc()
            $pwOk = $false
            try {
                $pwOk = [Win32Cap]::PrintWindow($hwnd, $hdc, 2)
            } finally {
                $tmpG.ReleaseHdc($hdc)
                $tmpG.Dispose()
            }

            if ($pwOk) {
                $cropX = [Math]::Max(0, $capRect.Left - $winRect.Left)
                $cropY = [Math]::Max(0, $capRect.Top - $winRect.Top)
                $cropW = [Math]::Min($capW, $fullW - $cropX)
                $cropH = [Math]::Min($capH, $fullH - $cropY)
                if ($cropW -gt 0 -and $cropH -gt 0) {
                    $pts = @(
                        @([Math]::Min($fullW - 1, $cropX + 20), [Math]::Min($fullH - 1, $cropY + 12)),
                        @([Math]::Min($fullW - 1, $cropX + [int]($cropW / 2)), [Math]::Min($fullH - 1, $cropY + [int]($cropH / 2))),
                        @([Math]::Min($fullW - 1, $cropX + [int]($cropW / 3)), [Math]::Min($fullH - 1, $cropY + [int]($cropH / 3))),
                        @([Math]::Min($fullW - 1, $cropX + [int](($cropW * 2) / 3)), [Math]::Min($fullH - 1, $cropY + [int]($cropH / 2))),
                        @([Math]::Min($fullW - 1, $cropX + $cropW - 20), [Math]::Min($fullH - 1, $cropY + $cropH - 20))
                    )
                    $hasContent = $false
                    foreach ($pt in $pts) {
                        $px = $tmpBmp.GetPixel([Math]::Max(0, $pt[0]), [Math]::Max(0, $pt[1]))
                        if ($px.A -ne 0 -or $px.R -ne 0 -or $px.G -ne 0 -or $px.B -ne 0) {
                            $hasContent = $true
                            break
                        }
                    }
                    if ($hasContent) {
                        $finalBmp = New-Object System.Drawing.Bitmap($cropW, $cropH)
                        $fg = [System.Drawing.Graphics]::FromImage($finalBmp)
                        $srcRect = New-Object System.Drawing.Rectangle($cropX, $cropY, $cropW, $cropH)
                        $dstRect = New-Object System.Drawing.Rectangle(0, 0, $cropW, $cropH)
                        $fg.DrawImage($tmpBmp, $dstRect, $srcRect, [System.Drawing.GraphicsUnit]::Pixel)
                        $fg.Dispose()
                    }
                }
            }
            $tmpBmp.Dispose()

            if ($null -eq $finalBmp) {
                [Win32Cap]::SetForegroundWindow($hwnd) | Out-Null
                Start-Sleep -Milliseconds 150
                $finalBmp = New-Object System.Drawing.Bitmap($capW, $capH)
                $sg = [System.Drawing.Graphics]::FromImage($finalBmp)
                $sz = New-Object System.Drawing.Size($capW, $capH)
                $sg.CopyFromScreen($capRect.Left, $capRect.Top, 0, 0, $sz, [System.Drawing.CopyPixelOperation]::SourceCopy)
                $sg.Dispose()
            }

            if ($null -ne $finalBmp) {
                for ($i = 0; $i -lt 3; $i++) {
                    try {
                        [System.Windows.Forms.Clipboard]::SetImage($finalBmp)
                        break
                    } catch {
                        Start-Sleep -Milliseconds 100
                    }
                }
                $finalBmp.Dispose()
            }
        }
    }
} catch {}
"#
    } else {
        ""
    };

    let post_run_block = if setting.auto_capture_clipboard {
        format!(
            r#"$global:__bh_cap_done = $false
function global:prompt {{
    $p = 'PS ' + (Get-Location).Path + '>'
    if (-not $global:__bh_cap_done) {{
        $global:__bh_cap_done = $true
        Write-Host -NoNewline $p
        {capture_block}
        return ' '
    }}
    return ($p + ' ')
}}"#,
            capture_block = capture_block
        )
    } else {
        String::new()
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

    [DllImport("kernel32.dll")]
    public static extern IntPtr GetConsoleWindow();

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);

    [DllImport("user32.dll")]
    public static extern bool MoveWindow(IntPtr hWnd, int X, int Y, int nWidth, int nHeight, bool bRepaint);

    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdcBlt, uint nFlags);

    [DllImport("user32.dll")]
    public static extern bool SetProcessDPIAware();

    [DllImport("user32.dll")]
    public static extern IntPtr SetProcessDpiAwarenessContext(IntPtr dpiContext);

    [DllImport("dwmapi.dll")]
    public static extern int DwmGetWindowAttribute(IntPtr hwnd, int dwAttribute, out RECT pvAttribute, int cbAttribute);
}}
'@
try {{ [Win32Cap]::SetProcessDpiAwarenessContext([IntPtr](-4)) | Out-Null }} catch {{ try {{ [Win32Cap]::SetProcessDPIAware() | Out-Null }} catch {{}} }}
$Host.UI.RawUI.WindowTitle = '{safe_file}'
try {{
    $hwnd = [Win32Cap]::GetConsoleWindow()
    if ($hwnd -ne [IntPtr]::Zero) {{
        $maxSize = $Host.UI.RawUI.MaxPhysicalWindowSize
        $buf = $Host.UI.RawUI.BufferSize
        $buf.Width = [Math]::Max($buf.Width, [Math]::Max($maxSize.Width, 300))
        $buf.Height = [Math]::Max($buf.Height, 3000)
        $Host.UI.RawUI.BufferSize = $buf

        $workArea = [System.Windows.Forms.Screen]::PrimaryScreen.WorkingArea
        $x = $workArea.Left + [Math]::Max(0, [int](($workArea.Width - {width}) / 2))
        $y = $workArea.Top + [Math]::Max(0, [int](($workArea.Height - {height}) / 2))
        [Win32Cap]::MoveWindow($hwnd, $x, $y, {width}, {height}, $true) | Out-Null
        Start-Sleep -Milliseconds 100

        $winSize = $Host.UI.RawUI.WindowSize
        $newBuf = $Host.UI.RawUI.BufferSize
        $newBuf.Width = $winSize.Width
        $newBuf.Height = [Math]::Max($newBuf.Height, $winSize.Height)
        $Host.UI.RawUI.BufferSize = $newBuf
    }}
}} catch {{}}
Set-Location -LiteralPath '{safe_dir}'
Clear-Host
Write-Host ('PS ' + (Get-Location).Path + '> .\{safe_file}')
& '.\{safe_file}'
{post_run_block}
"#,
        safe_file = safe_file,
        safe_dir = safe_dir,
        width = width,
        height = height,
        post_run_block = post_run_block
    )
}

/// Launch a .bat file in a dedicated Win32 console PowerShell window (`.\<file.bat>`).
pub fn execute_bat_in_powershell(path: &str) -> Result<(), String> {
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

    let setting = crate::modules::app_config::load_user_setting();
    let script = build_powershell_run_script(&parent_str, &file_name, &setting);
    let encoded = encode_utf16le_base64(&script);

    // Prefer launching via conhost.exe so GetConsoleWindow() always resolves to the dedicated visible window
    let conhost_res = Command::new("conhost.exe")
        .args([
            "powershell.exe",
            "-NoProfile",
            "-NoExit",
            "-STA",
            "-EncodedCommand",
            &encoded,
        ])
        .current_dir(parent)
        .spawn();

    if conhost_res.is_ok() {
        return Ok(());
    }

    #[cfg(windows)]
    {
        const CREATE_NEW_CONSOLE: u32 = 0x00000010;
        Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NoExit",
                "-STA",
                "-EncodedCommand",
                &encoded,
            ])
            .current_dir(parent)
            .creation_flags(CREATE_NEW_CONSOLE)
            .spawn()
            .map_err(|e| format!("Failed to launch PowerShell window: {}", e))?;
    }

    #[cfg(not(windows))]
    {
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-NoExit",
                "-STA",
                "-EncodedCommand",
                &encoded,
            ])
            .current_dir(parent)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Execute a single `.bat` script sequentially and capture decoded stdout/stderr/exit status (for Group Run).
pub fn execute_bat_group_step(path: &str) -> BatGroupStepResult {
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
        };
    }

    let parent = p.parent().unwrap_or(p);

    #[cfg(windows)]
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let mut cmd = Command::new("cmd.exe");
    cmd.args(["/c", &name]);
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
                path: clean_path,
                name,
                success,
                exit_code,
                duration_ms: start.elapsed().as_millis() as u64,
                stdout,
                stderr,
                error_message,
            }
        }
        Err(e) => {
            let err_msg = format!("Failed to execute '{}': {}", name, e);
            BatGroupStepResult {
                path: clean_path,
                name,
                success: false,
                exit_code: None,
                duration_ms: start.elapsed().as_millis() as u64,
                stdout: String::new(),
                stderr: String::new(),
                error_message: Some(err_msg),
            }
        }
    }
}
