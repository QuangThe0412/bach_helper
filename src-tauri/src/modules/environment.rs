use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandExecutionResult {
    pub command: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemEnvironmentInfo {
    // Raw CMD outputs
    pub ant_version_cmd: CommandExecutionResult,
    pub where_ant_cmd: CommandExecutionResult,
    pub echo_ant_home_cmd: CommandExecutionResult,
    pub java_version_cmd: CommandExecutionResult,
    pub where_java_cmd: CommandExecutionResult,
    pub echo_java_home_cmd: CommandExecutionResult,
    pub javac_version_cmd: CommandExecutionResult,
    pub where_javac_cmd: CommandExecutionResult,

    // High level parsed summary
    pub ant_installed: bool,
    pub ant_version: Option<String>,
    pub ant_executables: Vec<String>,
    pub ant_where_path: Option<String>,
    pub ant_home: Option<String>,

    pub java_installed: bool,
    pub java_version: Option<String>,
    pub java_executables: Vec<String>,
    pub java_where_path: Option<String>,
    pub java_home: Option<String>,

    pub javac_installed: bool,
    pub javac_version: Option<String>,
    pub javac_executables: Vec<String>,
    pub javac_where_path: Option<String>,
}

/// Decode bytes from Windows CMD (handles UTF-8 and Shift-JIS/CP932 seamlessly)
pub fn decode_console_bytes(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    if let Ok(utf8) = std::str::from_utf8(bytes) {
        return utf8.trim_end_matches(&['\r', '\n'][..]).to_string();
    }
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn MultiByteToWideChar(
                CodePage: u32,
                dwFlags: u32,
                lpMultiByteStr: *const u8,
                cbMultiByte: i32,
                lpWideCharStr: *mut u16,
                cchWideChar: i32,
            ) -> i32;
        }
        unsafe {
            let len = MultiByteToWideChar(
                0, // CP_ACP (System default ANSI codepage, e.g. CP932 on Japanese Windows)
                0,
                bytes.as_ptr(),
                bytes.len() as i32,
                std::ptr::null_mut(),
                0,
            );
            if len > 0 {
                let mut wide = vec![0u16; len as usize];
                MultiByteToWideChar(
                    0,
                    0,
                    bytes.as_ptr(),
                    bytes.len() as i32,
                    wide.as_mut_ptr(),
                    len,
                );
                return String::from_utf16_lossy(&wide)
                    .trim_end_matches(&['\r', '\n'][..])
                    .to_string();
            }
        }
    }
    String::from_utf8_lossy(bytes)
        .trim_end_matches(&['\r', '\n'][..])
        .to_string()
}

pub fn encode_console_bytes(text: &str, as_utf8: bool) -> Vec<u8> {
    if as_utf8 {
        return text.as_bytes().to_vec();
    }
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn WideCharToMultiByte(
                CodePage: u32,
                dwFlags: u32,
                lpWideCharStr: *const u16,
                cchWideChar: i32,
                lpMultiByteStr: *mut u8,
                cbMultiByte: i32,
                lpDefaultChar: *const u8,
                lpUsedDefaultChar: *mut i32,
            ) -> i32;
        }
        let wide: Vec<u16> = text.encode_utf16().collect();
        if wide.is_empty() {
            return Vec::new();
        }
        unsafe {
            let len = WideCharToMultiByte(
                0,
                0,
                wide.as_ptr(),
                wide.len() as i32,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
                std::ptr::null_mut(),
            );
            if len > 0 {
                let mut buf = vec![0u8; len as usize];
                WideCharToMultiByte(
                    0,
                    0,
                    wide.as_ptr(),
                    wide.len() as i32,
                    buf.as_mut_ptr(),
                    len,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                );
                return buf;
            }
        }
    }
    text.as_bytes().to_vec()
}

pub fn execute_exe_version(exe: &str) -> CommandExecutionResult {
    #[cfg(windows)]
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let clean_exe = exe.trim().trim_matches('"');
    let mut cmd = Command::new("cmd.exe");
    cmd.args(["/c", clean_exe, "-version"]);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    match cmd.output() {
        Ok(output) => {
            let stdout = decode_console_bytes(&output.stdout).trim().to_string();
            let stderr = decode_console_bytes(&output.stderr).trim().to_string();
            let exit_code = output.status.code().unwrap_or(-1);
            CommandExecutionResult {
                command: format!("{} -version", clean_exe),
                exit_code,
                stdout,
                stderr,
                success: output.status.success(),
            }
        }
        Err(e) => CommandExecutionResult {
            command: format!("{} -version", clean_exe),
            exit_code: -1,
            stdout: String::new(),
            stderr: e.to_string(),
            success: false,
        },
    }
}

pub fn execute_cmd(command: &str) -> CommandExecutionResult {
    #[cfg(windows)]
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let mut cmd = Command::new("cmd.exe");
    cmd.args(["/c", command]);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    match cmd.output() {
        Ok(output) => {
            let stdout = decode_console_bytes(&output.stdout).trim().to_string();
            let stderr = decode_console_bytes(&output.stderr).trim().to_string();
            let exit_code = output.status.code().unwrap_or(-1);
            CommandExecutionResult {
                command: command.to_string(),
                exit_code,
                stdout,
                stderr,
                success: output.status.success(),
            }
        }
        Err(e) => CommandExecutionResult {
            command: command.to_string(),
            exit_code: -1,
            stdout: String::new(),
            stderr: e.to_string(),
            success: false,
        },
    }
}

fn query_registry_user_env(var_name: &str) -> Option<String> {
    #[cfg(windows)]
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let mut cmd = Command::new("reg");
    cmd.args(["query", "HKCU\\Environment", "/v", var_name]);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    if let Ok(output) = cmd.output() {
        if output.status.success() {
            let text = decode_console_bytes(&output.stdout);
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.to_lowercase().starts_with(&var_name.to_lowercase()) {
                    // Format: JAVA_HOME    REG_SZ    D:\jdk-25.0.2
                    if let Some(idx) = trimmed.find("REG_SZ") {
                        let val = trimmed[idx + 6..].trim();
                        if !val.is_empty() {
                            return Some(val.to_string());
                        }
                    } else if let Some(idx) = trimmed.find("REG_EXPAND_SZ") {
                        let val = trimmed[idx + 13..].trim();
                        if !val.is_empty() {
                            return Some(val.to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

fn prepend_bin_to_process_path(bin_dir: &str) {
    let cur_path = std::env::var("PATH").unwrap_or_default();
    let parts: Vec<&str> = cur_path
        .split(';')
        .filter(|p| !p.trim().is_empty() && !p.trim().eq_ignore_ascii_case(bin_dir))
        .collect();
    let new_path = if parts.is_empty() {
        bin_dir.to_string()
    } else {
        format!("{};{}", bin_dir, parts.join(";"))
    };
    std::env::set_var("PATH", new_path);
}

/// Apply persisted JAVA_HOME and ANT_HOME from history.json or Windows User Registry (HKCU\Environment)
pub fn apply_persisted_environment() {
    let settings = crate::modules::settings::load_settings();

    // 1. JAVA_HOME
    let persisted_java = settings
        .java_home
        .filter(|s| !s.trim().is_empty() && Path::new(s.trim()).join("bin").join("java.exe").is_file())
        .or_else(|| {
            let reg_jh = query_registry_user_env("JAVA_HOME")?;
            if Path::new(reg_jh.trim()).join("bin").join("java.exe").is_file() {
                let _ = crate::modules::settings::save_java_home_setting(&reg_jh);
                Some(reg_jh)
            } else {
                None
            }
        });

    if let Some(jh) = persisted_java {
        let clean_jh = jh.trim().to_string();
        let bin_str = Path::new(&clean_jh).join("bin").to_string_lossy().to_string();
        std::env::set_var("JAVA_HOME", &clean_jh);
        prepend_bin_to_process_path(&bin_str);
    }

    // 2. ANT_HOME
    let persisted_ant = settings
        .ant_home
        .filter(|s| !s.trim().is_empty() && Path::new(s.trim()).join("bin").is_dir())
        .or_else(|| {
            let reg_ah = query_registry_user_env("ANT_HOME")?;
            if Path::new(reg_ah.trim()).join("bin").is_dir() {
                let _ = crate::modules::settings::save_ant_home_setting(&reg_ah);
                Some(reg_ah)
            } else {
                None
            }
        });

    if let Some(ah) = persisted_ant {
        let clean_ah = ah.trim().to_string();
        let bin_str = Path::new(&clean_ah).join("bin").to_string_lossy().to_string();
        std::env::set_var("ANT_HOME", &clean_ah);
        prepend_bin_to_process_path(&bin_str);
    }
}

pub fn check_system_environment() -> SystemEnvironmentInfo {
    // Ensure persisted JAVA_HOME and ANT_HOME are applied to process environment & PATH first
    apply_persisted_environment();

    // 1. Ant commands
    let ant_version_cmd = execute_cmd("ant -version");
    let where_ant_cmd = execute_cmd("where ant");
    let echo_ant_home_cmd = execute_cmd("echo %ANT_HOME%");

    // 2. Java commands
    let java_version_cmd = execute_cmd("java -version");
    let where_java_cmd = execute_cmd("where java");
    let echo_java_home_cmd = execute_cmd("echo %JAVA_HOME%");

    // 3. Javac commands
    let javac_version_cmd = execute_cmd("javac -version");
    let where_javac_cmd = execute_cmd("where javac");

    // Parse ANT_HOME
    let ant_home = std::env::var("ANT_HOME")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| parse_env_var(&echo_ant_home_cmd, "%ANT_HOME%"));

    // Parse where ant
    let mut ant_executables = parse_paths(&where_ant_cmd);
    if ant_executables.is_empty() {
        if let Some(ref ah) = ant_home {
            let candidate = Path::new(ah).join("bin").join("ant.bat");
            if candidate.is_file() {
                ant_executables.push(candidate.to_string_lossy().to_string());
            }
        }
    }
    let ant_where_path = ant_executables.first().cloned();

    // Parse Ant version
    let mut ant_installed = ant_version_cmd.success && !ant_version_cmd.stdout.is_empty();
    let mut ant_version = if ant_installed {
        ant_version_cmd
            .stdout
            .lines()
            .find(|line| line.contains("Apache Ant"))
            .map(|s| s.trim().to_string())
            .or_else(|| ant_version_cmd.stdout.lines().next().map(|s| s.trim().to_string()))
    } else {
        None
    };

    // Fallback: if ant_home is set and ant -version failed because PATH lacked %ANT_HOME%\bin
    if !ant_installed {
        if let Some(ref exe) = ant_where_path {
            let fallback_cmd = execute_exe_version(exe);
            if fallback_cmd.success && !fallback_cmd.stdout.is_empty() {
                ant_installed = true;
                ant_version = fallback_cmd
                    .stdout
                    .lines()
                    .find(|line| line.contains("Apache Ant"))
                    .map(|s| s.trim().to_string())
                    .or_else(|| fallback_cmd.stdout.lines().next().map(|s| s.trim().to_string()));
            }
        }
    }

    // Parse JAVA_HOME
    let java_home = std::env::var("JAVA_HOME")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| parse_env_var(&echo_java_home_cmd, "%JAVA_HOME%"));

    // Parse where java
    let mut java_executables = parse_paths(&where_java_cmd);
    if java_executables.is_empty() {
        if let Some(ref jh) = java_home {
            let candidate = Path::new(jh).join("bin").join("java.exe");
            if candidate.is_file() {
                java_executables.push(candidate.to_string_lossy().to_string());
            }
        }
    }
    let java_where_path = java_executables.first().cloned();

    // Parse Java version (java -version outputs to stderr on most JDKs)
    let java_output = if !java_version_cmd.stdout.is_empty() {
        &java_version_cmd.stdout
    } else {
        &java_version_cmd.stderr
    };
    let mut java_installed = java_version_cmd.success
        && (java_output.contains("version")
            || java_output.contains("OpenJDK")
            || java_output.contains("Java(TM)"));
    let mut java_version = if java_installed {
        java_output.lines().next().map(|s| s.trim().to_string())
    } else {
        None
    };

    if !java_installed {
        if let Some(ref exe) = java_where_path {
            let fallback_cmd = execute_exe_version(exe);
            let out = if !fallback_cmd.stdout.is_empty() {
                &fallback_cmd.stdout
            } else {
                &fallback_cmd.stderr
            };
            if fallback_cmd.success && !out.is_empty() {
                java_installed = true;
                java_version = out.lines().next().map(|s| s.trim().to_string());
            }
        }
    }

    // Parse Javac
    let mut javac_executables = parse_paths(&where_javac_cmd);
    if javac_executables.is_empty() {
        if let Some(ref jh) = java_home {
            let candidate = Path::new(jh).join("bin").join("javac.exe");
            if candidate.is_file() {
                javac_executables.push(candidate.to_string_lossy().to_string());
            }
        }
    }
    let javac_where_path = javac_executables.first().cloned();

    let javac_output = if !javac_version_cmd.stdout.is_empty() {
        &javac_version_cmd.stdout
    } else {
        &javac_version_cmd.stderr
    };
    let mut javac_installed = javac_version_cmd.success && javac_output.contains("javac");
    let mut javac_version = if javac_installed {
        javac_output.lines().next().map(|s| s.trim().to_string())
    } else {
        None
    };

    if !javac_installed {
        if let Some(ref exe) = javac_where_path {
            let fallback_cmd = execute_exe_version(exe);
            let out = if !fallback_cmd.stdout.is_empty() {
                &fallback_cmd.stdout
            } else {
                &fallback_cmd.stderr
            };
            if fallback_cmd.success && out.contains("javac") {
                javac_installed = true;
                javac_version = out.lines().next().map(|s| s.trim().to_string());
            }
        }
    }

    SystemEnvironmentInfo {
        ant_version_cmd,
        where_ant_cmd,
        echo_ant_home_cmd,
        java_version_cmd,
        where_java_cmd,
        echo_java_home_cmd,
        javac_version_cmd,
        where_javac_cmd,
        ant_installed,
        ant_version,
        ant_executables,
        ant_where_path,
        ant_home,
        java_installed,
        java_version,
        java_executables,
        java_where_path,
        java_home,
        javac_installed,
        javac_version,
        javac_executables,
        javac_where_path,
    }
}

/// Quickly configure ANT_HOME and prepend %ANT_HOME%\bin to PATH (process + Windows User Environment)
pub fn set_ant_environment_quick(ant_path_str: &str) -> Result<SystemEnvironmentInfo, String> {
    let raw_path = PathBuf::from(ant_path_str.trim().trim_matches('"'));
    if !raw_path.exists() {
        return Err(format!("Path does not exist: {}", ant_path_str));
    }

    // Determine ANT_HOME root folder
    let ant_home = if raw_path.is_file() {
        // e.g. ...\bin\ant.bat -> parent is bin, parent.parent is ANT_HOME
        raw_path
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .ok_or_else(|| "Invalid Ant executable path".to_string())?
    } else if raw_path.join("bin").join("ant.bat").is_file() || raw_path.join("bin").join("ant").is_file() {
        raw_path
    } else if raw_path.file_name().map(|n| n == "bin").unwrap_or(false) {
        raw_path.parent().unwrap_or(&raw_path).to_path_buf()
    } else {
        return Err(format!(
            "Could not find bin/ant.bat inside '{}'",
            raw_path.display()
        ));
    };

    let ant_home_str = ant_home.to_string_lossy().to_string();
    let ant_bin_str = ant_home.join("bin").to_string_lossy().to_string();

    // 1. Update current process environment immediately & save in history.json
    std::env::set_var("ANT_HOME", &ant_home_str);
    prepend_bin_to_process_path(&ant_bin_str);
    let _ = crate::modules::settings::save_ant_home_setting(&ant_home_str);

    // 2. Persist in Windows User Environment
    persist_user_env_var("ANT_HOME", &ant_home_str, &ant_bin_str)?;

    Ok(check_system_environment())
}

/// Quickly configure JAVA_HOME and prepend %JAVA_HOME%\bin to PATH (process + Windows User Environment)
pub fn set_java_environment_quick(java_path_str: &str) -> Result<SystemEnvironmentInfo, String> {
    let raw_path = PathBuf::from(java_path_str.trim().trim_matches('"'));
    if !raw_path.exists() {
        return Err(format!("Path does not exist: {}", java_path_str));
    }

    // Determine JAVA_HOME root folder
    let java_home = if raw_path.is_file() {
        raw_path
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .ok_or_else(|| "Invalid Java executable path".to_string())?
    } else if raw_path.join("bin").join("java.exe").is_file() {
        raw_path
    } else if raw_path.file_name().map(|n| n == "bin").unwrap_or(false) {
        raw_path.parent().unwrap_or(&raw_path).to_path_buf()
    } else {
        return Err(format!(
            "Could not find bin/java.exe inside '{}'",
            raw_path.display()
        ));
    };

    let java_home_str = java_home.to_string_lossy().to_string();
    let java_bin_str = java_home.join("bin").to_string_lossy().to_string();

    // 1. Update current process environment immediately & save in history.json
    std::env::set_var("JAVA_HOME", &java_home_str);
    prepend_bin_to_process_path(&java_bin_str);
    let _ = crate::modules::settings::save_java_home_setting(&java_home_str);

    // 2. Persist in Windows User Environment
    persist_user_env_var("JAVA_HOME", &java_home_str, &java_bin_str)?;

    Ok(check_system_environment())
}

fn persist_user_env_var(var_name: &str, home_val: &str, bin_val: &str) -> Result<(), String> {
    #[cfg(windows)]
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let ps_script = format!(
        r#"
        [Environment]::SetEnvironmentVariable('{var}', '{home}', 'User');
        $userPath = [Environment]::GetEnvironmentVariable('Path', 'User');
        if (-not $userPath) {{ $userPath = '' }}
        $parts = $userPath -split ';' | Where-Object {{ $_ -ne '' -and $_ -ne '{bin}' }};
        $newPath = (@('{bin}') + $parts) -join ';';
        [Environment]::SetEnvironmentVariable('Path', $newPath, 'User');
        "#,
        var = var_name.replace('\'', "''"),
        home = home_val.replace('\'', "''"),
        bin = bin_val.replace('\'', "''"),
    );

    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", &ps_script]);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let _ = cmd.output();
    Ok(())
}

fn parse_paths(cmd_res: &CommandExecutionResult) -> Vec<String> {
    if !cmd_res.success {
        return Vec::new();
    }
    cmd_res
        .stdout
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && !l.starts_with("INFO:") && !l.contains("Could not find"))
        .collect()
}

fn parse_env_var(cmd_res: &CommandExecutionResult, placeholder: &str) -> Option<String> {
    let val = cmd_res.stdout.trim();
    if val.is_empty() || val == placeholder || (val.starts_with('%') && val.ends_with('%')) {
        None
    } else {
        Some(val.to_string())
    }
}
