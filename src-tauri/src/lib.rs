use std::{path::Path, process::Command};
use tauri::Emitter;
use ti_toolbox_core::{
    BuildValidationReport, ConversionReport, ConversionRequest, EnvironmentDiscovery,
    EnvironmentRequest, KeilEnvironmentDiscovery, KeilEnvironmentRequest, KeilSysConfigRequest,
    KeilSysConfigResult, ProjectInspection,
};

#[tauri::command]
async fn discover_environment(request: EnvironmentRequest) -> Result<EnvironmentDiscovery, String> {
    tauri::async_runtime::spawn_blocking(move || ti_toolbox_core::discover_environment(&request))
        .await
        .map_err(|error| format!("环境自动检测任务异常结束：{error}"))?
}

#[tauri::command]
async fn discover_keil_environment(
    request: KeilEnvironmentRequest,
) -> Result<KeilEnvironmentDiscovery, String> {
    tauri::async_runtime::spawn_blocking(move || {
        ti_toolbox_core::discover_keil_environment(&request)
    })
    .await
    .map_err(|error| format!("Keil 环境自动检测任务异常结束：{error}"))?
}

#[tauri::command]
fn configure_keil_sysconfig(request: KeilSysConfigRequest) -> Result<KeilSysConfigResult, String> {
    ti_toolbox_core::configure_keil_sysconfig(&request)
}

#[tauri::command]
fn open_pack_download(url: String) -> Result<(), String> {
    if !url.starts_with("https://www.keil.arm.com/packs/") {
        return Err("只允许打开 Keil 官方 Pack 页面".into());
    }
    Command::new("rundll32.exe")
        .args(["url.dll,FileProtocolHandler", &url])
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开 Pack 下载页面：{error}"))
}

#[tauri::command]
fn inspect_project(project_path: String) -> Result<ProjectInspection, String> {
    ti_toolbox_core::inspect_project(Path::new(&project_path))
}

#[tauri::command]
fn convert_project(request: ConversionRequest) -> Result<ConversionReport, String> {
    ti_toolbox_core::convert_project(&request)
}

#[tauri::command]
async fn validate_project_build(
    app: tauri::AppHandle,
    project_path: String,
    ccs_path: String,
    keil_path: String,
    ccs_in_place: bool,
    search_depth: u8,
    operation_id: String,
) -> Result<BuildValidationReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        ti_toolbox_core::validate_project_build_with_progress(
            Path::new(&project_path),
            Path::new(&ccs_path),
            Path::new(&keil_path),
            ccs_in_place,
            search_depth,
            |chunk| {
                let _ = app.emit("build-log", (operation_id.as_str(), chunk));
            },
        )
    })
    .await
    .map_err(|error| format!("构建验证任务异常结束：{error}"))?
}

#[tauri::command]
fn cleanup_validation_copy(path: String) -> Result<(), String> {
    ti_toolbox_core::cleanup_validation_copy(Path::new(&path))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            discover_environment,
            discover_keil_environment,
            configure_keil_sysconfig,
            open_pack_download,
            inspect_project,
            convert_project,
            validate_project_build,
            cleanup_validation_copy
        ])
        .run(tauri::generate_context!())
        .expect("启动 TI工具箱 失败");
}
