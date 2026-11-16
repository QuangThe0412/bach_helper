use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AntOption {
    pub id: String,
    pub name: String,
    pub source_type: String, // "system" | "bundled_zip" | "bundled_extracted" | "custom"
    pub version: Option<String>,
    pub root_path: String,
    pub executable_path: Option<String>,
    pub is_ready_to_use: bool,
    pub status_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AntScanResult {
    pub current_system_ant: Option<AntOption>,
    pub bundled_ants: Vec<AntOption>,
    pub available_options: Vec<AntOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractAntResult {
    pub success: bool,
    pub message: String,
    pub extracted_dir: Option<String>,
    pub executable_path: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AntTestResult {
    pub success: bool,
    pub executable_path: String,
    pub version_output: String,
    pub exit_code: i32,
    pub error_message: Option<String>,
}

pub fn scan_available_ants() -> AntScanResult {
    let mut available_options = Vec::new();

    // 1. Check system Ant via 'where ant' and 'ant -version'
    let system_ant = detect_system_ant();
    if let Some(ref sys_ant) = system_ant {
        available_options.push(sys_ant.clone());
    }

    // 2. Scan 'libs' folder for bundled Ant
    let bundled_ants = scan_libs_for_ant();
    for bundled in &bundled_ants {
        available_options.push(bundled.clone());
    }

    AntScanResult {
        current_system_ant: system_ant,
        bundled_ants,
        available_options,
    }
}

fn detect_system_ant() -> Option<AntOption> {
    #[cfg(windows)]
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let mut cmd = Command::new("cmd.exe");
    cmd.args(["/c", "where ant"]);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let where_res = cmd.output().ok()?;
    if !where_res.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&where_res.stdout).to_string();
    let first_exe = stdout
        .lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty() && (l.ends_with(".bat") || l.ends_with(".cmd") || !l.contains('.')))?;

    // Try get version
    let mut ver_cmd = Command::new("cmd.exe");
    ver_cmd.args(["/c", "ant -version"]);
    #[cfg(windows)]
    ver_cmd.creation_flags(CREATE_NO_WINDOW);

    let version = ver_cmd
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .and_then(|out| {
            out.lines()
                .find(|l| l.contains("Apache Ant"))
                .map(|s| s.to_string())
        });

    let exe_path_buf = PathBuf::from(first_exe);
    let root_path = exe_path_buf
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| first_exe.to_string());

    Some(AntOption {
        id: "system_ant".to_string(),
        name: format!("System Ant ({})", version.as_deref().unwrap_or("Installed")),
        source_type: "system".to_string(),
        version: version.clone(),
        root_path,
        executable_path: Some(first_exe.to_string()),
        is_ready_to_use: true,
        status_message: format!(
            "Installed on System PATH: {}",
            version.unwrap_or_else(|| "Detected".to_string())
        ),
    })
}

fn scan_libs_for_ant() -> Vec<AntOption> {
    let mut list = Vec::new();
    let libs_path = find_libs_dir();

    if !libs_path.exists() {
        return list;
    }

    // 1. Look for zip files like apache-ant-*-bin.zip
    if let Ok(entries) = fs::read_dir(&libs_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = path.file_name().unwrap_or_default().to_string_lossy();

            if file_name.starts_with("apache-ant-") && file_name.ends_with(".zip") {
                // Check if already extracted in libs
                let extracted_name = file_name.trim_end_matches("-bin.zip").trim_end_matches(".zip");
                let extracted_dir = libs_path.join(extracted_name);

                let (is_extracted, exe_path) = check_extracted_ant(&extracted_dir);

                let version_str = file_name
                    .strip_prefix("apache-ant-")
                    .and_then(|s| s.strip_suffix("-bin.zip").or_else(|| s.strip_suffix(".zip")))
                    .unwrap_or("1.10.18");

                if is_extracted {
                    list.push(AntOption {
                        id: format!("bundled_extracted_{}", version_str),
                        name: format!("Bundled Apache Ant {} (Extracted)", version_str),
                        source_type: "bundled_extracted".to_string(),
                        version: Some(version_str.to_string()),
                        root_path: extracted_dir.to_string_lossy().to_string(),
                        executable_path: exe_path,
                        is_ready_to_use: true,
                        status_message: "Extracted and ready for use in libs/".to_string(),
                    });
                } else {
                    list.push(AntOption {
                        id: format!("bundled_zip_{}", version_str),
                        name: format!("Bundled Apache Ant {} (ZIP in libs)", version_str),
                        source_type: "bundled_zip".to_string(),
                        version: Some(version_str.to_string()),
                        root_path: path.to_string_lossy().to_string(),
                        executable_path: None,
                        is_ready_to_use: false,
                        status_message: "Archive available in libs. Click 'Extract & Setup' to enable.".to_string(),
                    });
                }
            }
        }
    }

    list
}

fn check_extracted_ant(dir: &Path) -> (bool, Option<String>) {
    if !dir.is_dir() {
        return (false, None);
    }

    // Look for bin/ant.bat or bin/ant
    let ant_bat = dir.join("bin").join("ant.bat");
    if ant_bat.is_file() {
        return (true, Some(ant_bat.to_string_lossy().to_string()));
    }

    let ant_cmd = dir.join("bin").join("ant.cmd");
    if ant_cmd.is_file() {
        return (true, Some(ant_cmd.to_string_lossy().to_string()));
    }

    let ant_sh = dir.join("bin").join("ant");
    if ant_sh.is_file() {
        return (true, Some(ant_sh.to_string_lossy().to_string()));
    }

    (false, None)
}

pub fn extract_bundled_ant(zip_path_str: &str) -> ExtractAntResult {
    let zip_path = PathBuf::from(zip_path_str.trim().trim_matches('"'));
    if !zip_path.is_file() {
        return ExtractAntResult {
            success: false,
            message: format!("Ant archive not found: {}", zip_path.display()),
            extracted_dir: None,
            executable_path: None,
            version: None,
        };
    }

    let libs_dir = zip_path.parent().unwrap_or(Path::new("."));
    let file = match fs::File::open(&zip_path) {
        Ok(f) => f,
        Err(e) => {
            return ExtractAntResult {
                success: false,
                message: format!("Failed to open zip file: {}", e),
                extracted_dir: None,
                executable_path: None,
                version: None,
            };
        }
    };

    let mut archive = match zip::ZipArchive::new(file) {
        Ok(a) => a,
        Err(e) => {
            return ExtractAntResult {
                success: false,
                message: format!("Invalid zip archive: {}", e),
                extracted_dir: None,
                executable_path: None,
                version: None,
            };
        }
    };

    // Extract all files
    for i in 0..archive.len() {
        let mut file = match archive.by_index(i) {
            Ok(f) => f,
            Err(e) => {
                return ExtractAntResult {
                    success: false,
                    message: format!("Failed to read archive entry {}: {}", i, e),
                    extracted_dir: None,
                    executable_path: None,
                    version: None,
                };
            }
        };

        let outpath = match file.enclosed_name() {
            Some(path) => libs_dir.join(path),
            None => continue,
        };

        if file.name().ends_with('/') {
            let _ = fs::create_dir_all(&outpath);
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    let _ = fs::create_dir_all(p);
                }
            }
            let mut outfile = match fs::File::create(&outpath) {
                Ok(f) => f,
                Err(e) => {
                    return ExtractAntResult {
                        success: false,
                        message: format!("Failed to create file {}: {}", outpath.display(), e),
                        extracted_dir: None,
                        executable_path: None,
                        version: None,
                    };
                }
            };
            let _ = io::copy(&mut file, &mut outfile);
        }
    }

    // Determine extracted directory
    let file_name = zip_path.file_name().unwrap_or_default().to_string_lossy();
    let folder_name = file_name.trim_end_matches("-bin.zip").trim_end_matches(".zip");
    let target_dir = libs_dir.join(folder_name);

    let (is_extracted, exe_path) = check_extracted_ant(&target_dir);

    if is_extracted {
        let test_res = exe_path.as_deref().map(test_ant_executable);
        let version = test_res.and_then(|r| {
            r.version_output
                .lines()
                .find(|l| l.contains("Apache Ant"))
                .map(|s| s.trim().to_string())
        });

        ExtractAntResult {
            success: true,
            message: format!("Apache Ant extracted successfully to {}", target_dir.display()),
            extracted_dir: Some(target_dir.to_string_lossy().to_string()),
            executable_path: exe_path,
            version,
        }
    } else {
        ExtractAntResult {
            success: false,
            message: format!("Extracted, but executable ant.bat not found in {}", target_dir.display()),
            extracted_dir: Some(target_dir.to_string_lossy().to_string()),
            executable_path: None,
            version: None,
        }
    }
}

pub fn test_ant_executable(exe_path: &str) -> AntTestResult {
    #[cfg(windows)]
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let mut cmd = Command::new("cmd.exe");
    let cmd_str = format!("\"{}\" -version", exe_path);
    cmd.args(["/c", &cmd_str]);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let combined = if !stdout.is_empty() { stdout } else { stderr };
            let exit_code = output.status.code().unwrap_or(-1);

            AntTestResult {
                success: output.status.success() && combined.contains("Apache Ant"),
                executable_path: exe_path.to_string(),
                version_output: combined,
                exit_code,
                error_message: if output.status.success() {
                    None
                } else {
                    Some("Execution returned non-zero exit code".to_string())
                },
            }
        }
        Err(e) => AntTestResult {
            success: false,
            executable_path: exe_path.to_string(),
            version_output: String::new(),
            exit_code: -1,
            error_message: Some(e.to_string()),
        },
    }
}

pub fn find_libs_dir() -> PathBuf {
    // Priority:
    // 1. Current working dir ./libs
    // 2. Relative to executable ../libs
    // 3. Absolute fallback D:\vinx_tools\bach_helper\libs
    let current_libs = Path::new("libs");
    if current_libs.is_dir() {
        return current_libs.to_path_buf();
    }

    if let Ok(exe_dir) = std::env::current_exe() {
        if let Some(parent) = exe_dir.parent() {
            let candidate1 = parent.join("libs");
            if candidate1.is_dir() {
                return candidate1;
            }
            if let Some(grandparent) = parent.parent() {
                let candidate2 = grandparent.join("libs");
                if candidate2.is_dir() {
                    return candidate2;
                }
            }
        }
    }

    PathBuf::from(r"D:\vinx_tools\bach_helper\libs")
}

