use serde::{Deserialize, Serialize};
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
    pub ant_home: Option<String>,

    pub java_installed: bool,
    pub java_version: Option<String>,
    pub java_executables: Vec<String>,
    pub java_home: Option<String>,

    pub javac_installed: bool,
    pub javac_version: Option<String>,
    pub javac_executables: Vec<String>,
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
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
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

pub fn check_system_environment() -> SystemEnvironmentInfo {
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

    // Parse Ant
    let ant_installed = ant_version_cmd.success && !ant_version_cmd.stdout.is_empty();
    let ant_version = if ant_installed {
        ant_version_cmd
            .stdout
            .lines()
            .find(|line| line.contains("Apache Ant"))
            .map(|s| s.trim().to_string())
            .or_else(|| ant_version_cmd.stdout.lines().next().map(|s| s.trim().to_string()))
    } else {
        None
    };

    let ant_executables = parse_paths(&where_ant_cmd);
    let ant_home = parse_env_var(&echo_ant_home_cmd, "%ANT_HOME%");

    // Parse Java
    // Note: java -version outputs to stderr in many Java versions!
    let java_output = if !java_version_cmd.stdout.is_empty() {
        &java_version_cmd.stdout
    } else {
        &java_version_cmd.stderr
    };
    let java_installed = java_version_cmd.success
        && (java_output.contains("version")
            || java_output.contains("OpenJDK")
            || java_output.contains("Java(TM)"));
    let java_version = if java_installed {
        java_output.lines().next().map(|s| s.trim().to_string())
    } else {
        None
    };

    let java_executables = parse_paths(&where_java_cmd);
    let java_home = parse_env_var(&echo_java_home_cmd, "%JAVA_HOME%");

    // Parse Javac
    let javac_output = if !javac_version_cmd.stdout.is_empty() {
        &javac_version_cmd.stdout
    } else {
        &javac_version_cmd.stderr
    };
    let javac_installed = javac_version_cmd.success && javac_output.contains("javac");
    let javac_version = if javac_installed {
        javac_output.lines().next().map(|s| s.trim().to_string())
    } else {
        None
    };
    let javac_executables = parse_paths(&where_javac_cmd);

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
        ant_home,
        java_installed,
        java_version,
        java_executables,
        java_home,
        javac_installed,
        javac_version,
        javac_executables,
    }
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

