use ccs2keil_core::{ConversionReport, ConversionRequest, ProjectInspection, ResourceInfo};
use std::path::Path;

#[tauri::command]
fn validate_resources(sdk_path: String, pack_path: String) -> Result<ResourceInfo, String> {
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            validate_resources,
            inspect_project,
            convert_project
        ])
        .run(tauri::generate_context!())
        .expect("启动 CCS2KEIL 失败");
}
