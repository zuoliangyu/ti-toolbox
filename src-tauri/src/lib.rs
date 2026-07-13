use ccs2keil_core::{
    BuildValidationReport, ConversionReport, ConversionRequest, ProjectInspection,
    ValidatedResources,
};
use std::path::Path;
use tauri::Emitter;

#[tauri::command]
fn validate_resources(
    sdk_path: String,
    pack_path: String,
    ccs_path: String,
    keil_path: String,
    search_depth: u8,
) -> Result<ValidatedResources, String> {
    ccs2keil_core::validate_development_resources(
        Path::new(&sdk_path),
        Path::new(&pack_path),
        Path::new(&ccs_path),
        Path::new(&keil_path),
        search_depth,
    )
}

#[tauri::command]
fn inspect_project(project_path: String) -> Result<ProjectInspection, String> {
    ccs2keil_core::inspect_project(Path::new(&project_path))
}

#[tauri::command]
fn convert_project(request: ConversionRequest) -> Result<ConversionReport, String> {
    ccs2keil_core::convert_project(&request)
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
        ccs2keil_core::validate_project_build_with_progress(
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
    ccs2keil_core::cleanup_validation_copy(Path::new(&path))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            validate_resources,
            inspect_project,
            convert_project,
            validate_project_build,
            cleanup_validation_copy
        ])
        .run(tauri::generate_context!())
        .expect("启动 CCS2KEIL 失败");
}
