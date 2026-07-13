use ccs2keil_core::{
    BuildValidationReport, ConversionReport, ConversionRequest, ProjectInspection, ResourceInfo,
};
use std::path::Path;

#[tauri::command]
fn validate_resources(
    sdk_path: String,
    pack_path: String,
    ccs_path: String,
    keil_path: String,
) -> Result<ResourceInfo, String> {
    ccs2keil_core::validate_toolchains(Path::new(&ccs_path), Path::new(&keil_path))?;
    ccs2keil_core::validate_resources(Path::new(&sdk_path), Path::new(&pack_path))
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
    project_path: String,
    ccs_path: String,
    keil_path: String,
    ccs_in_place: bool,
) -> Result<BuildValidationReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        ccs2keil_core::validate_project_build(
            Path::new(&project_path),
            Path::new(&ccs_path),
            Path::new(&keil_path),
            ccs_in_place,
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
