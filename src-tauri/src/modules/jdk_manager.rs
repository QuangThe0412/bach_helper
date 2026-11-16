use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::modules::ant_manager::find_libs_dir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JdkOption {
    pub id: String,
    pub name: String,
    pub version_label: String,
    pub major_version: u32,
    pub is_recommended: bool,
    pub source_type: String, // "libs_installer" | "installed_jdk" | "custom"
    pub root_path: String,
    pub java_executable: Option<String>,
    pub javac_executable: Option<String>,
    pub is_ready_to_use: bool,
    pub status_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JdkScanResult {
    pub recommended_jdk_id: Option<String>,
    pub jdks: Vec<JdkOption>,
    pub libs_installers: Vec<JdkOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JavaTestResult {
    pub success: bool,
    pub executable_path: String,
    pub version_output: String,
    pub exit_code: i32,
    pub error_message: Option<String>,
}

pub fn scan_available_jdks() -> JdkScanResult {
    let mut jdks = Vec::new();
    let mut libs_installers = Vec::new();

    // 1. Scan libs folder for JDKs & installers
    let libs_path = find_libs_dir();
    if libs_path.is_dir() {
        if let Ok(entries) = fs::read_dir(&libs_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                let file_name = path.file_name().unwrap_or_default().to_string_lossy();

                // Check MSI or installer files for JDK 25
                if file_name.contains("jdk") && (file_name.ends_with(".msi") || file_name.ends_with(".exe")) {
                    let is_jdk25 = file_name.contains("25");
                    let opt = JdkOption {
                        id: format!("installer_{}", file_name),
                        name: if is_jdk25 {
                            "OpenJDK 25 (Hotspot Installer in libs)".to_string()
                        } else {
                            format!("JDK Installer: {}", file_name)
                        },
                        version_label: if is_jdk25 { "25 (Installer)".to_string() } else { "Installer".to_string() },
                        major_version: if is_jdk25 { 25 } else { 0 },
                        is_recommended: is_jdk25,
                        source_type: "libs_installer".to_string(),
                        root_path: path.to_string_lossy().to_string(),
                        java_executable: None,
                        javac_executable: None,
                        is_ready_to_use: false,
                        status_message: format!("Installer file present in libs: {}", file_name),
                    };
                    libs_installers.push(opt.clone());
                    jdks.push(opt);
                } else if path.is_dir() && file_name.to_lowercase().contains("jdk") {
                    // Check if extracted JDK folder in libs
                    if let Some(jdk_opt) = inspect_jdk_folder(&path, "libs_extracted", file_name.contains("25")) {
                        jdks.push(jdk_opt);
                    }
                }
            }
        }
    }

    // 2. Scan standard installation locations
    let mut check_dirs = Vec::new();

    if let Ok(prog_files) = std::env::var("ProgramFiles") {
        let pf = PathBuf::from(prog_files);
        check_dirs.push(pf.join("Eclipse Adoptium"));
        check_dirs.push(pf.join("Java"));
        check_dirs.push(pf.join("Microsoft"));
        check_dirs.push(pf.join("Amazon Corretto"));
        check_dirs.push(pf.join("Zulu"));
    }

    if let Ok(prog_files_x86) = std::env::var("ProgramFiles(x86)") {
        let pf86 = PathBuf::from(prog_files_x86);
        check_dirs.push(pf86.join("Java"));
    }

    for base_dir in check_dirs {
        if !base_dir.is_dir() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(base_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let folder_name = path.file_name().unwrap_or_default().to_string_lossy();
                    let is_jdk25 = folder_name.contains("25");
                    if let Some(jdk_opt) = inspect_jdk_folder(&path, "installed_jdk", is_jdk25) {
                        // Prevent duplicates
                        if !jdks.iter().any(|j| j.root_path == jdk_opt.root_path) {
                            jdks.push(jdk_opt);
                        }
                    }
                }
            }
        }
    }

    // 3. Check system PATH via 'where java'
    #[cfg(windows)]
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let mut cmd = Command::new("cmd.exe");
    cmd.args(["/c", "where java"]);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    if let Ok(out) = cmd.output() {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            for line in stdout.lines() {
                let trimmed = line.trim();
                if trimmed.ends_with("java.exe") && !trimmed.contains("javapath") {
                    let p = PathBuf::from(trimmed);
                    if let Some(bin) = p.parent() {
                        if let Some(root) = bin.parent() {
                            let is_jdk25 = root.to_string_lossy().contains("25");
                            if let Some(jdk_opt) = inspect_jdk_folder(root, "system_path", is_jdk25) {
                                if !jdks.iter().any(|j| j.root_path == jdk_opt.root_path) {
                                    jdks.push(jdk_opt);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Sort JDKs: Recommended (JDK 25) first, then by major version descending
    jdks.sort_by(|a, b| {
        b.is_recommended
            .cmp(&a.is_recommended)
            .then_with(|| b.major_version.cmp(&a.major_version))
    });

    let recommended_jdk_id = jdks.iter().find(|j| j.is_recommended).map(|j| j.id.clone())
        .or_else(|| jdks.iter().find(|j| j.is_ready_to_use).map(|j| j.id.clone()));

    JdkScanResult {
        recommended_jdk_id,
        jdks,
        libs_installers,
    }
}

fn inspect_jdk_folder(folder: &Path, source_type: &str, is_recommended_hint: bool) -> Option<JdkOption> {
    let java_exe = folder.join("bin").join("java.exe");
    if !java_exe.is_file() {
        return None;
    }

    let javac_exe = folder.join("bin").join("javac.exe");
    let has_javac = javac_exe.is_file();

    let test_res = test_java_executable(&java_exe.to_string_lossy());
    let version_output = test_res.version_output;

    // Parse major version
    let (major_version, version_label) = parse_java_version(&version_output);
    let is_recommended = is_recommended_hint || major_version == 25;

    let folder_name = folder.file_name().unwrap_or_default().to_string_lossy();
    let display_name = format!(
        "{} ({}){}",
        folder_name,
        version_label,
        if is_recommended { " [Recommended]" } else { "" }
    );

    Some(JdkOption {
        id: format!("jdk_{}", folder.to_string_lossy().replace(['\\', '/', ':', ' '], "_")),
        name: display_name,
        version_label: version_label.clone(),
        major_version,
        is_recommended,
        source_type: source_type.to_string(),
        root_path: folder.to_string_lossy().to_string(),
        java_executable: Some(java_exe.to_string_lossy().to_string()),
        javac_executable: if has_javac {
            Some(javac_exe.to_string_lossy().to_string())
        } else {
            None
        },
        is_ready_to_use: true,
        status_message: format!(
            "Java {}{}: {}",
            version_label,
            if has_javac { " (JDK)" } else { " (JRE only)" },
            folder.display()
        ),
    })
}

fn parse_java_version(output: &str) -> (u32, String) {
    // Look for lines like:
    // openjdk version "25.0.5" 2025-07-15
    // java version "1.8.0_351"
    // openjdk version "11.0.28"
    for line in output.lines() {
        if line.contains("version") {
            if let Some(start) = line.find('"') {
                if let Some(end) = line[start + 1..].find('"') {
                    let ver_str = &line[start + 1..start + 1 + end];
                    let major = if ver_str.starts_with("1.") {
                        ver_str.chars().nth(2).and_then(|c| c.to_digit(10)).unwrap_or(8)
                    } else {
                        ver_str
                            .split('.')
                            .next()
                            .and_then(|s| s.parse::<u32>().ok())
                            .unwrap_or(0)
                    };
                    return (major, ver_str.to_string());
                }
            }
        }
    }
    (0, "Unknown".to_string())
}

pub fn test_java_executable(java_exe_path: &str) -> JavaTestResult {
    #[cfg(windows)]
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let clean_path = java_exe_path.trim().trim_matches('"');
    let mut cmd = Command::new(clean_path);
    cmd.arg("-version");
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            // Java outputs version to stderr!
            let combined = if !stderr.is_empty() {
                stderr
            } else {
                stdout
            };
            let exit_code = output.status.code().unwrap_or(-1);
            let success = output.status.success()
                && (combined.contains("version")
                    || combined.contains("OpenJDK")
                    || combined.contains("Java(TM)"));

            JavaTestResult {
                success,
                executable_path: clean_path.to_string(),
                version_output: combined,
                exit_code,
                error_message: if success {
                    None
                } else {
                    Some("Failed to verify Java version".to_string())
                },
            }
        }
        Err(e) => JavaTestResult {
            success: false,
            executable_path: clean_path.to_string(),
            version_output: String::new(),
            exit_code: -1,
            error_message: Some(e.to_string()),
        },
    }
}

