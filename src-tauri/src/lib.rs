pub mod modules;

use modules::environment::{self, CommandExecutionResult, SystemEnvironmentInfo};
use modules::project::{self, ProjectScanResult};
use modules::ant_manager::{self, AntScanResult, AntTestResult, ExtractAntResult};
use modules::jdk_manager::{self, JavaTestResult, JdkScanResult};

#[tauri::command]
async fn pick_folder() -> Result<Option<String>, String> {
    let folder = rfd::AsyncFileDialog::new()
        .set_title("Select Java Ant Project Directory")
        .pick_folder()
        .await;
    Ok(folder.map(|f| f.path().to_string_lossy().to_string()))
}

#[tauri::command]
async fn check_system_environment() -> Result<SystemEnvironmentInfo, String> {
    Ok(environment::check_system_environment())
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
            scan_project_dir,
            get_available_ants,
            extract_ant,
            test_ant,
            get_available_jdks,
            test_java,
            run_custom_cmd,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
