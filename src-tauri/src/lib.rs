pub mod modules;

use modules::ant_manager::{self, AntScanResult, AntTestResult, ExtractAntResult};
use modules::app_config::{self, UserSetting};
use modules::builder::{
    self, AutoConfigResult, BatFileInfo, BatGroupStepResult, BuildResult, PostBuildInspection,
};
use modules::encoding;
use modules::environment::{self, CommandExecutionResult, SystemEnvironmentInfo};
use modules::jdk_manager::{self, JavaTestResult, JdkScanResult};
use modules::project::{self, ProjectScanResult};
use modules::settings::{self, AppSettings};
use serde::Serialize;
use std::path::Path;
use tauri::{AppHandle, Emitter, Manager};

const CMD_SIZE_PICKER_LABEL: &str = "cmd-size-picker";

#[derive(Debug, Clone, Serialize)]
struct CmdSizePayload {
    width: u32,
    height: u32,
}

fn clamp_cmd_size(width: u32, height: u32) -> (u32, u32) {
    (
        width.clamp(app_config::MIN_PS_WIDTH, app_config::MAX_PS_WIDTH),
        height.clamp(app_config::MIN_PS_HEIGHT, app_config::MAX_PS_HEIGHT),
    )
}

fn apply_target_outer_size(win: &tauri::WebviewWindow, target_w: u32, target_h: u32) {
    let (clamped_w, clamped_h) = clamp_cmd_size(target_w, target_h);
    for _ in 0..2 {
        if let (Ok(outer), Ok(inner)) = (win.outer_size(), win.inner_size()) {
            let chrome_w = outer.width.saturating_sub(inner.width);
            let chrome_h = outer.height.saturating_sub(inner.height);
            let desired_inner_w = clamped_w.saturating_sub(chrome_w).max(240);
            let desired_inner_h = clamped_h.saturating_sub(chrome_h).max(160);
            let _ = win.set_size(tauri::Size::Physical(tauri::PhysicalSize::new(
                desired_inner_w,
                desired_inner_h,
            )));
        }
    }
}

fn read_clamped_outer_size(win: &tauri::WebviewWindow) -> Result<CmdSizePayload, String> {
    let outer = win
        .outer_size()
        .map_err(|e| format!("Failed to read window size: {}", e))?;
    let (width, height) = clamp_cmd_size(outer.width, outer.height);
    Ok(CmdSizePayload { width, height })
}

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
async fn remove_project_history(path: String) -> Result<AppSettings, String> {
    settings::remove_project_from_history(&path)
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
async fn get_user_setting() -> Result<UserSetting, String> {
    Ok(app_config::load_user_setting())
}

#[tauri::command]
async fn save_user_setting(setting: UserSetting) -> Result<UserSetting, String> {
    app_config::save_user_setting(&setting)
}

#[tauri::command]
async fn get_user_setting_path() -> Result<String, String> {
    Ok(app_config::get_setting_file_path().to_string_lossy().to_string())
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
async fn run_auto_config(
    project_path: String,
    target: String,
    java_home: Option<String>,
) -> Result<AutoConfigResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        builder::run_auto_config(&project_path, &target, java_home.as_deref())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
async fn read_text_file(path: String) -> Result<String, String> {
    let clean = path.trim().trim_matches('"').replace('/', "\\");
    let p = Path::new(&clean);
    if !p.is_file() {
        return Err(format!("File does not exist: {}", clean));
    }
    let meta = std::fs::metadata(p).map_err(|e| format!("Failed to stat file: {}", e))?;
    const MAX_PREVIEW_BYTES: u64 = 2 * 1024 * 1024;
    if meta.len() > MAX_PREVIEW_BYTES {
        return Err(format!(
            "File is too large to preview ({} bytes, max 2 MB): {}",
            meta.len(),
            clean
        ));
    }
    let bytes = std::fs::read(p).map_err(|e| format!("Failed to read file: {}", e))?;
    let (decoded, _) = encoding::detect_and_decode_text(&bytes);
    Ok(decoded)
}

#[tauri::command]
async fn open_file_default(path: String) -> Result<String, String> {
    builder::open_file_with_os_default(&path)
}

#[tauri::command]
async fn open_file_in_text_editor(path: String) -> Result<String, String> {
    builder::open_file_with_default_text_editor(&path)
}

#[tauri::command]
async fn execute_bat_group_step(path: String) -> Result<BatGroupStepResult, String> {
    tauri::async_runtime::spawn_blocking(move || builder::execute_bat_group_step(&path))
        .await
        .map_err(|e| format!("Task join error: {}", e))
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
    log_folder_path: Option<String>,
    project_path: Option<String>,
    target: Option<String>,
    file_name: Option<String>,
) -> Result<String, String> {
    let fname = file_name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "No log file is configured in log4j properties for this project.".to_string())?;
    builder::open_target_log_file(
        project_path.as_deref(),
        target.as_deref(),
        log_folder_path.as_deref(),
        &fname,
    )
}

#[tauri::command]
async fn execute_bat_file(path: String) -> Result<(), String> {
    builder::execute_bat_in_powershell(&path)
}

#[tauri::command]
async fn open_cmd_size_picker(
    app: AppHandle,
    width: u32,
    height: u32,
) -> Result<CmdSizePayload, String> {
    let (w, h) = clamp_cmd_size(width, height);

    if let Some(existing) = app.get_webview_window(CMD_SIZE_PICKER_LABEL) {
        apply_target_outer_size(&existing, w, h);
        let _ = existing.unminimize();
        let _ = existing.show();
        let _ = existing.set_focus();
        let payload = read_clamped_outer_size(&existing)?;
        let _ = app.emit_to(CMD_SIZE_PICKER_LABEL, "cmd-size-picker-resized", payload.clone());
        return Ok(payload);
    }

    let win = tauri::WebviewWindowBuilder::new(
        &app,
        CMD_SIZE_PICKER_LABEL,
        tauri::WebviewUrl::App("index.html".into()),
    )
    .title("Command Prompt - Size Preview (Drag/Resize & Click DONE)")
    .resizable(true)
    .decorations(true)
    .min_inner_size(300.0, 200.0)
    .inner_size(w.min(1920) as f64, h.min(1080) as f64)
    .visible(false)
    .build()
    .map_err(|e| format!("Failed to create simulated CMD window: {}", e))?;

    apply_target_outer_size(&win, w, h);
    let _ = win.center();

    let app_for_event = app.clone();
    win.on_window_event(move |event| match event {
        tauri::WindowEvent::Resized(_) => {
            if let Some(w_ref) = app_for_event.get_webview_window(CMD_SIZE_PICKER_LABEL) {
                if let Ok(payload) = read_clamped_outer_size(&w_ref) {
                    let _ = app_for_event.emit_to(
                        CMD_SIZE_PICKER_LABEL,
                        "cmd-size-picker-resized",
                        payload,
                    );
                }
            }
        }
        tauri::WindowEvent::Destroyed => {
            if let Some(main_win) = app_for_event.get_webview_window("main") {
                let _ = main_win.set_focus();
            }
        }
        _ => {}
    });

    let app_fallback = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(1000));
        if let Some(w_ref) = app_fallback.get_webview_window(CMD_SIZE_PICKER_LABEL) {
            if !w_ref.is_visible().unwrap_or(true) {
                let _ = w_ref.show();
                let _ = w_ref.set_focus();
            }
        }
    });

    read_clamped_outer_size(&win)
}

#[tauri::command]
async fn ready_cmd_size_picker(app: AppHandle) -> Result<CmdSizePayload, String> {
    let win = app
        .get_webview_window(CMD_SIZE_PICKER_LABEL)
        .ok_or_else(|| "Simulated CMD size window is not open".to_string())?;
    let _ = win.show();
    let _ = win.set_focus();
    read_clamped_outer_size(&win)
}

#[tauri::command]
async fn get_cmd_size_picker_dimensions(app: AppHandle) -> Result<CmdSizePayload, String> {
    let win = app
        .get_webview_window(CMD_SIZE_PICKER_LABEL)
        .ok_or_else(|| "Simulated CMD size window is not open".to_string())?;
    read_clamped_outer_size(&win)
}

#[tauri::command]
async fn start_drag_cmd_size_picker(app: AppHandle) -> Result<(), String> {
    let win = app
        .get_webview_window(CMD_SIZE_PICKER_LABEL)
        .ok_or_else(|| "Simulated CMD size window is not open".to_string())?;
    win.start_dragging().map_err(|e| e.to_string())
}

#[tauri::command]
async fn confirm_cmd_size_picker(app: AppHandle) -> Result<CmdSizePayload, String> {
    let win = app
        .get_webview_window(CMD_SIZE_PICKER_LABEL)
        .ok_or_else(|| "Simulated CMD size window is not open".to_string())?;
    let payload = read_clamped_outer_size(&win)?;
    let _ = app.emit_to("main", "cmd-size-picker-done", payload.clone());
    let _ = win.close();
    if let Some(main_win) = app.get_webview_window("main") {
        let _ = main_win.set_focus();
    }
    Ok(payload)
}

#[tauri::command]
async fn cancel_cmd_size_picker(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(CMD_SIZE_PICKER_LABEL) {
        let _ = win.close();
    }
    if let Some(main_win) = app.get_webview_window("main") {
        let _ = main_win.set_focus();
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
        .setup(|app| {
            let app_handle = app.handle().clone();
            if let Some(main_win) = app.get_webview_window("main") {
                main_win.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { .. } = event {
                        if let Some(picker_win) = app_handle.get_webview_window(CMD_SIZE_PICKER_LABEL) {
                            let _ = picker_win.close();
                        }
                    }
                });
            }
            Ok(())
        })
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
            remove_project_history,
            save_tabs_state,
            get_settings_path,
            get_user_setting,
            save_user_setting,
            get_user_setting_path,
            start_ant_build,
            run_auto_config,
            read_text_file,
            open_file_default,
            open_file_in_text_editor,
            execute_bat_group_step,
            inspect_build_target,
            scan_custom_bat_dir,
            open_path_in_explorer,
            open_file_log,
            execute_bat_file,
            open_cmd_size_picker,
            ready_cmd_size_picker,
            get_cmd_size_picker_dimensions,
            start_drag_cmd_size_picker,
            confirm_cmd_size_picker,
            cancel_cmd_size_picker,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
