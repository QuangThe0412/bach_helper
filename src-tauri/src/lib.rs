pub mod modules;

use modules::ant_manager::{self, AntScanResult, AntTestResult, ExtractAntResult};
use modules::builder::{self, BatFileInfo, BuildResult, PostBuildInspection};
use modules::environment::{self, CommandExecutionResult, SystemEnvironmentInfo};
use modules::jdk_manager::{self, JavaTestResult, JdkScanResult};
use modules::project::{self, ProjectScanResult};
use modules::settings::{self, AppSettings};
use std::path::Path;
use tauri::AppHandle;

#[tauri::command]
async fn pick_folder() -> Result<Option<String>, String> {
    let folder = rfd::AsyncFileDialog::new()
        .set_title("Select Directory")
        .pick_folder()
        .await;
    Ok(folder.map(|f| f.path().to_string_lossy().to_string()))
}

#[tauri::command]
async fn check_system_environment() -> Result<SystemEnvironmentInfo, String> {
    Ok(environment::check_system_environment())
}

#[tauri::command]
async fn set_ant_env_quick(ant_path: String) -> Result<SystemEnvironmentInfo, String> {
    environment::set_ant_environment_quick(&ant_path)
}

#[tauri::command]
async fn set_java_env_quick(java_path: String) -> Result<SystemEnvironmentInfo, String> {
    environment::set_java_environment_quick(&java_path)
}

#[tauri::command]
async fn scan_project_dir(path: String) -> Result<ProjectScanResult, String> {
    Ok(project::scan_project(&path))
}

#[tauri::command]
async fn get_available_ants() -> Result<AntScanResult, String> {
    Ok(ant_manager::scan_available_ants())
}

#[tauri::command]
async fn extract_ant(zip_path: String) -> Result<ExtractAntResult, String> {
    Ok(ant_manager::extract_bundled_ant(&zip_path))
}

#[tauri::command]
async fn test_ant(executable_path: String) -> Result<AntTestResult, String> {
    Ok(ant_manager::test_ant_executable(&executable_path))
}

#[tauri::command]
async fn get_available_jdks() -> Result<JdkScanResult, String> {
    Ok(jdk_manager::scan_available_jdks())
}

#[tauri::command]
async fn test_java(executable_path: String) -> Result<JavaTestResult, String> {
    Ok(jdk_manager::test_java_executable(&executable_path))
}

#[tauri::command]
async fn run_custom_cmd(command: String) -> Result<CommandExecutionResult, String> {
    Ok(environment::execute_cmd(&command))
}

#[tauri::command]
async fn get_settings() -> Result<AppSettings, String> {
    Ok(settings::load_settings())
}

#[tauri::command]
async fn save_settings(new_settings: AppSettings) -> Result<(), String> {
    settings::save_settings(&new_settings)
}

#[tauri::command]
async fn add_project_history(path: String) -> Result<AppSettings, String> {
    settings::add_project_to_history(&path)
}

#[tauri::command]
async fn save_tabs_state(open_tabs: Vec<String>, active_tab_index: usize) -> Result<AppSettings, String> {
    settings::save_open_tabs_state(open_tabs, active_tab_index)
}

#[tauri::command]
async fn get_settings_path() -> Result<String, String> {
    Ok(settings::get_settings_file_path().to_string_lossy().to_string())
}

#[tauri::command]
async fn start_ant_build(
    app: AppHandle,
    tab_id: Option<String>,
    project_path: String,
    target: String,
    ant_executable: Option<String>,
    java_home: Option<String>,
) -> Result<BuildResult, String> {
    let tid = tab_id.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || {
        builder::run_ant_build(app, tid, project_path, target, ant_executable, java_home)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
async fn inspect_build_target(project_path: String, target: String) -> Result<PostBuildInspection, String> {
    Ok(builder::inspect_target_files(&project_path, &target))
}

#[tauri::command]
async fn scan_custom_bat_dir(folder_path: String) -> Result<(Option<String>, Vec<BatFileInfo>), String> {
    Ok(builder::scan_custom_bat_folder(&folder_path))
}

#[tauri::command]
async fn open_path_in_explorer(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    if !p.exists() {
        return Err(format!("Path does not exist: {}", path));
    }
    if p.is_dir() {
        std::process::Command::new("explorer")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    } else {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", path))
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn open_file_log(
    log_file_path: Option<String>,
    log_folder_path: Option<String>,
    project_path: Option<String>,
) -> Result<String, String> {
    let mut candidates = Vec::new();

    if let Some(ref lf) = log_file_path {
        if !lf.trim().is_empty() {
            candidates.push(std::path::PathBuf::from(lf.trim()));
        }
    }
    if let Some(ref dir) = log_folder_path {
        if !dir.trim().is_empty() {
            candidates.push(Path::new(dir.trim()).join("batchApp.log"));
        }
    }
    if let Some(ref proj) = project_path {
        if !proj.trim().is_empty() {
            let bp = Path::new(proj.trim());
            candidates.push(bp.join("target").join("log").join("batchApp.log"));
            candidates.push(bp.join("log").join("batchApp.log"));
            candidates.push(bp.join("batch").join("log").join("batchApp.log"));
        }
    }

    for candidate in &candidates {
        if candidate.is_file() {
            let path_str = candidate.to_string_lossy().to_string();
            std::process::Command::new("cmd.exe")
                .args(["/c", "start", "", &path_str])
                .spawn()
                .map_err(|e| e.to_string())?;
            return Ok(path_str);
        }
    }

    let expected = candidates
        .first()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "target/log/batchApp.log".to_string());

    Err(format!("Log file not found: {}", expected))
}

#[tauri::command]
async fn execute_bat_file(path: String) -> Result<(), String> {
    let clean_path = path.trim().trim_matches('"');
    let p = Path::new(clean_path);
    if !p.is_file() {
        return Err(format!("File does not exist: {}", clean_path));
    }
    let parent = p.parent().unwrap_or(p);
    let file_name = p
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Batch Script".to_string());

    // Launch a standalone PowerShell window with -NoExit.
    // Running the .bat file inside a child `cmd.exe /c` ensures that `exit 0` / `exit 1`
    // inside the .bat script only exits the child cmd process and NEVER closes the PowerShell window!
    let ps_cmd = format!(
        "$Host.UI.RawUI.WindowTitle = '{}'; Set-Location -LiteralPath '{}'; Write-Host 'Running: {}' -ForegroundColor Yellow; Write-Host '----------------------------------------' -ForegroundColor DarkGray; cmd.exe /c '\"{}\"'; Write-Host '----------------------------------------' -ForegroundColor DarkGray; Write-Host ('Process finished with Exit Code: ' + $LASTEXITCODE) -ForegroundColor Cyan",
        file_name.replace('\'', "''"),
        parent.to_string_lossy().replace('\'', "''"),
        clean_path.replace('\'', "''"),
        clean_path
    );

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_CONSOLE: u32 = 0x00000010;
        std::process::Command::new("powershell.exe")
            .args(["-NoExit", "-Command", &ps_cmd])
            .current_dir(parent)
            .creation_flags(CREATE_NEW_CONSOLE)
            .spawn()
            .map_err(|e| format!("Failed to launch PowerShell window: {}", e))?;
    }

    #[cfg(not(windows))]
    {
        std::process::Command::new("powershell")
            .args(["-NoExit", "-Command", &ps_cmd])
            .current_dir(parent)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            pick_folder,
            check_system_environment,
            set_ant_env_quick,
            set_java_env_quick,
            scan_project_dir,
            get_available_ants,
            extract_ant,
            test_ant,
            get_available_jdks,
            test_java,
            run_custom_cmd,
            get_settings,
            save_settings,
            add_project_history,
            save_tabs_state,
            get_settings_path,
            start_ant_build,
            inspect_build_target,
            scan_custom_bat_dir,
            open_path_in_explorer,
            open_file_log,
            execute_bat_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
