use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{BufRead, BufReader, Read},
    path::{Component, Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant, SystemTime},
};
use xmltree::{Element, XMLNode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectKind {
    Ccs,
    Keil,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceInfo {
    pub sdk_version: String,
    pub pack_name: String,
    pub pack_version: String,
    pub devices: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFile {
    pub path: String,
    pub group: String,
    pub file_type: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInspection {
    pub kind: ProjectKind,
    pub target_kind: ProjectKind,
    pub name: String,
    pub device: String,
    pub files: Vec<ProjectFile>,
    pub include_paths: Vec<String>,
    pub defines: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionRequest {
    pub project_path: String,
    pub sdk_path: String,
    pub pack_path: String,
    pub output_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionReport {
    pub source_kind: ProjectKind,
    pub target_kind: ProjectKind,
    pub device: String,
    pub output_path: String,
    pub generated_files: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildValidationReport {
    pub success: bool,
    pub summary: String,
    pub log: String,
    pub log_path: Option<String>,
    pub validated_project_path: Option<String>,
    pub cleanup_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidatedResources {
    pub sdk_version: String,
    pub pack_name: String,
    pub pack_version: String,
    pub devices: Vec<String>,
    pub ccs_executable: String,
    pub keil_executable: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentRequest {
    pub project_path: String,
    pub sdk_path: String,
    pub pack_path: String,
    pub ccs_path: String,
    pub keil_path: String,
    pub sysconfig_path: String,
    pub search_depth: u8,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentDiscovery {
    pub project_kind: ProjectKind,
    pub device: String,
    pub sdk_path: Option<String>,
    pub sdk_version: Option<String>,
    pub pack_path: Option<String>,
    pub pack_name: Option<String>,
    pub pack_version: Option<String>,
    pub pack_installed: bool,
    pub pack_download_url: Option<String>,
    pub ccs_path: Option<String>,
    pub ccs_executable: Option<String>,
    pub keil_path: Option<String>,
    pub keil_executable: Option<String>,
    pub sysconfig_path: Option<String>,
    pub sysconfig_executable: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeilEnvironmentRequest {
    pub sdk_path: String,
    pub keil_path: String,
    pub sysconfig_path: String,
    pub search_depth: u8,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeilEnvironmentDiscovery {
    pub sdk_path: Option<String>,
    pub sdk_version: Option<String>,
    pub keil_path: Option<String>,
    pub keil_executable: Option<String>,
    pub sysconfig_path: Option<String>,
    pub sysconfig_executable: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeilSysConfigRequest {
    pub sdk_path: String,
    pub keil_path: String,
    pub sysconfig_path: String,
    pub search_depth: u8,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeilSysConfigResult {
    pub changed: bool,
    pub slot: u8,
    pub title: String,
    pub executable: String,
    pub working_directory: String,
    pub arguments: String,
    pub updated_files: Vec<String>,
    pub backup_files: Vec<String>,
}

struct PackCandidate {
    path: PathBuf,
    name: String,
    version: String,
    installed: bool,
}

struct CommandResult {
    status: ExitStatus,
    log: String,
}

pub fn validate_toolchains(ccs_path: &Path, keil_path: &Path) -> Result<(), String> {
    locate_toolchains(ccs_path, keil_path, 2)?;
    Ok(())
}

pub fn validate_development_resources(
    sdk_path: &Path,
    pack_path: &Path,
    ccs_path: &Path,
    keil_path: &Path,
    search_depth: u8,
) -> Result<ValidatedResources, String> {
    let resources = validate_resources(sdk_path, pack_path)?;
    let (ccs, keil) = locate_toolchains(ccs_path, keil_path, search_depth)?;
    Ok(ValidatedResources {
        sdk_version: resources.sdk_version,
        pack_name: resources.pack_name,
        pack_version: resources.pack_version,
        devices: resources.devices,
        ccs_executable: ccs.to_string_lossy().into_owned(),
        keil_executable: keil.to_string_lossy().into_owned(),
    })
}

pub fn discover_environment(request: &EnvironmentRequest) -> Result<EnvironmentDiscovery, String> {
    if request.search_depth > 4 {
        return Err("工具目录搜索层级只能是 0–4".into());
    }
    let inspection = inspect_project(Path::new(&request.project_path))?;
    let mut warnings = Vec::new();

    let sdk = discover_sdk(&request.sdk_path);
    let sdk_version = sdk.as_ref().and_then(|path| validate_sdk(path).ok());
    if sdk.is_none() {
        warnings.push("未找到 MSPM0 SDK；转换前需要手动选择".into());
    }

    let ccs = discover_ccs(&request.ccs_path, request.search_depth);
    if ccs.is_none() {
        warnings.push(if inspection.kind == ProjectKind::Ccs {
            "未找到 CCS；仍可转换，但不能执行源工程构建验证".into()
        } else {
            "未找到 CCS；仍可生成目标工程，但不能执行目标构建验证".into()
        });
    }
    let keil = discover_keil(&request.keil_path, request.search_depth);
    if keil.is_none() {
        warnings.push(if inspection.kind == ProjectKind::Keil {
            "未找到 Keil；仍可转换，但不能执行源工程构建验证".into()
        } else {
            "未找到 Keil；仍可生成目标工程，但不能执行目标构建验证".into()
        });
    }

    let pack = if inspection.kind == ProjectKind::Ccs {
        discover_pack(&request.pack_path, keil.as_deref(), &inspection.device)
    } else {
        None
    };
    if inspection.kind == ProjectKind::Ccs && pack.is_none() {
        warnings.push(format!(
            "未找到支持 {} 的 Pack；请从 Keil 官网下载后手动选择",
            inspection.device
        ));
    }
    let pack_download_url = pack
        .as_ref()
        .map(|value| pack_download_url(&value.name))
        .or_else(|| {
            (inspection.kind == ProjectKind::Ccs)
                .then(|| format!("https://www.keil.arm.com/packs/?q={}", inspection.device))
        });

    let sysconfig = discover_sysconfig(&request.sysconfig_path, ccs.as_deref());
    if sysconfig.is_none() {
        warnings
            .push("未找到 SysConfig；已有生成文件不受影响，但无法一键配置 Keil SysConfig".into());
    }

    let keil_root = keil
        .as_ref()
        .and_then(|path| path.parent().and_then(Path::parent).map(Path::to_path_buf));
    let ccs_root = ccs
        .as_ref()
        .and_then(|path| path.parent().and_then(Path::parent).map(Path::to_path_buf));
    Ok(EnvironmentDiscovery {
        project_kind: inspection.kind,
        device: inspection.device,
        sdk_path: sdk.as_ref().map(|path| path_text(path)),
        sdk_version,
        pack_path: pack.as_ref().map(|value| path_text(&value.path)),
        pack_name: pack.as_ref().map(|value| value.name.clone()),
        pack_version: pack.as_ref().map(|value| value.version.clone()),
        pack_installed: pack.as_ref().is_some_and(|value| value.installed),
        pack_download_url,
        ccs_path: ccs_root.as_ref().map(|path| path_text(path)),
        ccs_executable: ccs.as_ref().map(|path| path_text(path)),
        keil_path: keil_root.as_ref().map(|path| path_text(path)),
        keil_executable: keil.as_ref().map(|path| path_text(path)),
        sysconfig_path: sysconfig.as_ref().map(|path| path_text(path)),
        sysconfig_executable: sysconfig
            .as_ref()
            .map(|path| path_text(&sysconfig_nw_executable(path))),
        warnings,
    })
}

pub fn discover_keil_environment(
    request: &KeilEnvironmentRequest,
) -> Result<KeilEnvironmentDiscovery, String> {
    if request.search_depth > 4 {
        return Err("工具目录搜索层级只能是 0–4".into());
    }
    let sdk = discover_sdk(&request.sdk_path);
    let sdk_version = sdk.as_ref().and_then(|path| validate_sdk(path).ok());
    let keil = discover_keil(&request.keil_path, request.search_depth);
    let sysconfig = discover_sysconfig(&request.sysconfig_path, None);
    let mut warnings = Vec::new();
    if sdk.is_none() {
        warnings.push("未找到 MSPM0 SDK；一键配置前需要手动选择".into());
    }
    if keil.is_none() {
        warnings.push("未找到 Keil；一键配置前需要手动选择".into());
    }
    if sysconfig.is_none() {
        warnings.push(
            "未找到带图形界面的 SysConfig（需包含 nw/nw.exe）；一键配置前需要手动选择或独立安装"
                .into(),
        );
    }
    let keil_root = keil
        .as_ref()
        .and_then(|path| path.parent().and_then(Path::parent).map(Path::to_path_buf));
    Ok(KeilEnvironmentDiscovery {
        sdk_path: sdk.as_ref().map(|path| path_text(path)),
        sdk_version,
        keil_path: keil_root.as_ref().map(|path| path_text(path)),
        keil_executable: keil.as_ref().map(|path| path_text(path)),
        sysconfig_path: sysconfig.as_ref().map(|path| path_text(path)),
        sysconfig_executable: sysconfig
            .as_ref()
            .map(|path| path_text(&sysconfig_nw_executable(path))),
        warnings,
    })
}

fn discover_sdk(selected: &str) -> Option<PathBuf> {
    if !selected.is_empty() {
        let path = PathBuf::from(selected);
        if validate_sdk(&path).is_ok() {
            return Some(path);
        }
    }
    let mut candidates = Vec::new();
    for root in [Path::new(r"C:\ti"), Path::new(r"D:\ti")] {
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if let Ok(version) = validate_sdk(&path) {
                candidates.push((version_key(&version), path));
            }
        }
    }
    candidates.sort_by(|left, right| left.0.cmp(&right.0));
    candidates.pop().map(|value| value.1)
}

fn discover_ccs(selected: &str, depth: u8) -> Option<PathBuf> {
    if !selected.is_empty() {
        if let Ok(path) = locate_ccs_server(Path::new(selected), depth) {
            return Some(path);
        }
    }
    [Path::new(r"C:\ti"), Path::new(r"D:\ti")]
        .into_iter()
        .filter(|path| path.is_dir())
        .find_map(|path| locate_ccs_server(path, 4).ok())
}

fn discover_keil(selected: &str, depth: u8) -> Option<PathBuf> {
    if !selected.is_empty() {
        if let Ok(path) = locate_uv4(Path::new(selected), depth) {
            return Some(path);
        }
    }
    let registry_path =
        query_registry_string(r"HKLM\SOFTWARE\WOW6432Node\Keil\Products\MDK", "Path")
            .or_else(|| query_registry_string(r"HKLM\SOFTWARE\Keil\Products\MDK", "Path"));
    registry_path
        .as_deref()
        .and_then(|path| Path::new(path).parent())
        .and_then(|path| locate_uv4(path, depth).ok())
        .or_else(|| {
            [Path::new(r"C:\Keil_v5"), Path::new(r"D:\Keil_v5")]
                .into_iter()
                .find_map(|path| locate_uv4(path, depth).ok())
        })
}

fn discover_pack(
    selected: &str,
    keil_executable: Option<&Path>,
    device: &str,
) -> Option<PackCandidate> {
    let selected = if !selected.is_empty() {
        let path = PathBuf::from(selected);
        if let Ok((name, version, devices)) = validate_pack(&path) {
            if devices.iter().any(|value| value == device) {
                Some(PackCandidate {
                    installed: is_installed_pack_path(&path),
                    path,
                    name,
                    version,
                })
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };
    if selected.as_ref().is_some_and(|value| value.installed) {
        return selected;
    }
    let pack_roots = keil_executable
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map(|path| {
            [
                path.join("ARM").join("PACK"),
                path.join("ARM").join("Packs"),
            ]
        })
        .unwrap_or_default();
    pack_roots
        .iter()
        .find_map(|root| find_pack_candidate(&root.join("TexasInstruments"), device, true, 4))
        .or(selected)
        .or_else(|| {
            pack_roots
                .iter()
                .find_map(|root| find_pack_candidate(&root.join(".Web"), device, false, 1))
        })
}

fn find_pack_candidate(
    root: &Path,
    device: &str,
    installed: bool,
    depth: usize,
) -> Option<PackCandidate> {
    fn visit(
        root: &Path,
        device: &str,
        installed: bool,
        depth: usize,
        candidates: &mut Vec<PackCandidate>,
    ) {
        let Ok(entries) = fs::read_dir(root) else {
            return;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() && depth > 0 {
                visit(&path, device, installed, depth - 1, candidates);
                continue;
            }
            if path
                .extension()
                .and_then(|value| value.to_str())
                .is_none_or(|value| !value.eq_ignore_ascii_case("pdsc"))
            {
                continue;
            }
            let Ok((name, version, devices)) = validate_pack(&path) else {
                continue;
            };
            if devices.iter().any(|value| value == device) {
                candidates.push(PackCandidate {
                    path: if installed {
                        path.parent().unwrap_or(root).to_path_buf()
                    } else {
                        path
                    },
                    name,
                    version,
                    installed,
                });
            }
        }
    }

    let mut candidates = Vec::new();
    visit(root, device, installed, depth, &mut candidates);
    candidates.sort_by_key(|value| version_key(&value.version));
    candidates.pop()
}

fn is_installed_pack_path(path: &Path) -> bool {
    let text = path
        .to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase();
    (text.contains(r"\arm\pack\") || text.contains(r"\arm\packs\"))
        && !text.contains(r"\pack\.")
        && !text.contains(r"\packs\.")
}

fn pack_download_url(name: &str) -> String {
    format!(
        "https://www.keil.arm.com/packs/{}-texasinstruments/boards/",
        name.to_ascii_lowercase()
    )
}

fn discover_sysconfig(selected: &str, ccs_executable: Option<&Path>) -> Option<PathBuf> {
    if !selected.is_empty() {
        if let Some(path) = normalize_sysconfig_path(Path::new(selected)) {
            return Some(path);
        }
    }
    let mut roots = Vec::new();
    if let Some(ccs) = ccs_executable {
        if let Some(root) = ccs.parent().and_then(Path::parent).and_then(Path::parent) {
            roots.push(root.to_path_buf());
        }
    }
    roots.extend([PathBuf::from(r"C:\ti"), PathBuf::from(r"D:\ti")]);
    let mut candidates = Vec::new();
    for root in roots {
        collect_sysconfig_paths(&root, 2, &mut candidates);
    }
    candidates.sort_by_key(|path| {
        version_key(
            path.file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default(),
        )
    });
    candidates.pop()
}

fn normalize_sysconfig_path(path: &Path) -> Option<PathBuf> {
    if path.is_file()
        && path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("nw.exe"))
    {
        let root = path.parent()?.parent()?;
        return root
            .join("sysconfig_cli.bat")
            .is_file()
            .then(|| root.to_path_buf());
    }
    if sysconfig_nw_executable(path).is_file() && path.join("sysconfig_cli.bat").is_file() {
        return Some(path.to_path_buf());
    }
    if path.join("nw.exe").is_file() {
        let root = path.parent()?;
        return root
            .join("sysconfig_cli.bat")
            .is_file()
            .then(|| root.to_path_buf());
    }
    None
}

fn collect_sysconfig_paths(root: &Path, depth: usize, candidates: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.starts_with("sysconfig_"))
            && sysconfig_nw_executable(&path).is_file()
            && path.join("sysconfig_cli.bat").is_file()
        {
            candidates.push(path);
        } else if depth > 0 {
            collect_sysconfig_paths(&path, depth - 1, candidates);
        }
    }
}

fn query_registry_string(key: &str, value: &str) -> Option<String> {
    let output = Command::new("reg")
        .args(["query", key, "/v", value])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| {
            let line = line.trim();
            let rest = line.strip_prefix(value)?.trim_start();
            rest.strip_prefix("REG_SZ")
                .map(|value| value.trim().to_string())
        })
}

fn version_key(value: &str) -> Vec<u32> {
    value
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect()
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn sysconfig_nw_executable(root: &Path) -> PathBuf {
    root.join("nw").join("nw.exe")
}

pub fn configure_keil_sysconfig(
    request: &KeilSysConfigRequest,
) -> Result<KeilSysConfigResult, String> {
    if request.search_depth > 4 {
        return Err("工具目录搜索层级只能是 0–4".into());
    }
    let sdk = Path::new(&request.sdk_path);
    let sdk_version = validate_sdk(sdk)?;
    locate_uv4(Path::new(&request.keil_path), request.search_depth)?;
    let sysconfig = normalize_sysconfig_path(Path::new(&request.sysconfig_path))
        .ok_or("SysConfig 路径无效：未找到 nw/nw.exe")?;
    let executable = sysconfig_nw_executable(&sysconfig);
    let command = format!("\"{}\" \"{}\"", executable.display(), sysconfig.display());
    let working_directory = path_text(sdk);
    let arguments = "--compiler keil -s \".metadata\\product.json\" \"#E\"".to_string();
    let title = format!("TI工具箱 SysConfig - SDK {sdk_version}");
    let key = keil_tool_registry_key()
        .ok_or("未找到 Keil 用户配置；请先启动一次 Keil µVision，再重新执行一键配置")?;
    let registry = query_registry_key(&key)?;
    let (slot, equivalent) =
        choose_keil_tool_slot(&registry, &command).ok_or("Keil Tools 菜单没有可用配置槽位")?;
    let (updated_files, backup_files) =
        update_sdk_keil_sysconfig_files(sdk, &sdk_version, &sysconfig)?;
    if equivalent {
        return Ok(KeilSysConfigResult {
            changed: !updated_files.is_empty(),
            slot,
            title: "已存在等效 SysConfig 工具".into(),
            executable: path_text(&executable),
            working_directory,
            arguments,
            updated_files,
            backup_files,
        });
    }

    add_registry_value(&key, &format!("Mfg{slot}"), "REG_DWORD", "2304")?;
    add_registry_value(&key, &format!("Mid{slot}"), "REG_SZ", &working_directory)?;
    add_registry_value(&key, &format!("Mex{slot}"), "REG_SZ", &command)?;
    add_registry_value(&key, &format!("Mag{slot}"), "REG_SZ", &arguments)?;
    add_registry_value(&key, &format!("Mtx{slot}"), "REG_SZ", &title)?;
    Ok(KeilSysConfigResult {
        changed: true,
        slot,
        title,
        executable: path_text(&executable),
        working_directory,
        arguments,
        updated_files,
        backup_files,
    })
}

fn update_sdk_keil_sysconfig_files(
    sdk: &Path,
    sdk_version: &str,
    sysconfig: &Path,
) -> Result<(Vec<String>, Vec<String>), String> {
    let keil = sdk.join("tools").join("keil");
    let bat_path = keil.join("syscfg.bat");
    let cfg_path = keil.join("MSPM0_SDK_syscfg_menu_import.cfg");
    let bat = fs::read_to_string(&bat_path)
        .map_err(|error| format!("无法读取 {}：{error}", bat_path.display()))?;
    let cfg = fs::read_to_string(&cfg_path)
        .map_err(|error| format!("无法读取 {}：{error}", cfg_path.display()))?;
    let (updated_bat, updated_cfg) =
        render_keil_sysconfig_files(&bat, &cfg, sdk, sdk_version, sysconfig)?;
    let mut updated_files = Vec::new();
    let mut backup_files = Vec::new();
    for (path, current, updated) in [
        (&bat_path, bat.as_str(), updated_bat.as_str()),
        (&cfg_path, cfg.as_str(), updated_cfg.as_str()),
    ] {
        if current == updated {
            continue;
        }
        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("config");
        let backup = path.with_file_name(format!("{file_name}.ti-toolbox.bak"));
        if !backup.exists() {
            fs::copy(path, &backup)
                .map_err(|error| format!("无法备份 {}：{error}", path.display()))?;
            backup_files.push(path_text(&backup));
        }
        fs::write(path, updated)
            .map_err(|error| format!("无法更新 {}：{error}", path.display()))?;
        updated_files.push(path_text(path));
    }
    Ok((updated_files, backup_files))
}

fn render_keil_sysconfig_files(
    bat: &str,
    cfg: &str,
    sdk: &Path,
    sdk_version: &str,
    sysconfig: &Path,
) -> Result<(String, String), String> {
    let sysconfig_version = sysconfig
        .file_name()
        .and_then(|value| value.to_str())
        .and_then(|value| value.strip_prefix("sysconfig_"))
        .unwrap_or("custom");
    let bat = replace_config_line(
        bat,
        "set SYSCFG_PATH=",
        &format!(
            "set SYSCFG_PATH=\"{}\"",
            sysconfig.join("sysconfig_cli.bat").display()
        ),
    )?;
    let cfg = replace_config_line(
        cfg,
        "[",
        &format!(
            "[Sysconfig v{sysconfig_version} - MSPM0 SDK v{}]",
            sdk_version.replace('.', "_")
        ),
    )?;
    let cfg = replace_config_line(
        &cfg,
        "Command=",
        &format!(
            "Command=\"{}\" \"{}\"",
            sysconfig_nw_executable(sysconfig).display(),
            sysconfig.display()
        ),
    )?;
    let cfg = replace_config_line(
        &cfg,
        "Initial Folder=",
        &format!("Initial Folder={}", sdk.display()),
    )?;
    Ok((bat, cfg))
}

fn replace_config_line(content: &str, prefix: &str, replacement: &str) -> Result<String, String> {
    let newline = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut found = false;
    let mut lines = content
        .lines()
        .map(|line| {
            if !found && line.trim_start().starts_with(prefix) {
                found = true;
                replacement
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join(newline);
    if !found {
        return Err(format!("配置文件缺少 {prefix} 配置项"));
    }
    if content.ends_with('\n') {
        lines.push_str(newline);
    }
    Ok(lines)
}

fn keil_tool_registry_key() -> Option<String> {
    ["礦ision5", "µVision5", "UVision5"]
        .into_iter()
        .map(|name| format!(r"HKCU\Software\Keil\{name}"))
        .find(|parent| {
            Command::new("reg")
                .args(["query", parent])
                .output()
                .is_ok_and(|output| output.status.success())
        })
        .map(|parent| format!(r"{parent}\ToolM"))
}

fn query_registry_key(key: &str) -> Result<String, String> {
    let output = Command::new("reg")
        .args(["query", key])
        .output()
        .map_err(|error| format!("无法读取 Keil Tools 配置：{error}"))?;
    Ok(output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default())
}

fn choose_keil_tool_slot(registry: &str, command: &str) -> Option<(u8, bool)> {
    let mut occupied = BTreeSet::new();
    let mut own_slot = None;
    let expected = normalize_registry_command(command);
    for line in registry.lines() {
        let line = line.trim();
        let Some((name, value)) = parse_registry_string_line(line) else {
            if let Some(name) = line.split_whitespace().next() {
                if let Some(slot) = tool_value_slot(name) {
                    occupied.insert(slot);
                }
            }
            continue;
        };
        let Some(slot) = tool_value_slot(name) else {
            continue;
        };
        occupied.insert(slot);
        if name.starts_with("Mex") && normalize_registry_command(value) == expected {
            return Some((slot, true));
        }
        if name.starts_with("Mtx") && value.starts_with("TI工具箱 SysConfig") {
            own_slot = Some(slot);
        }
    }
    own_slot.map(|slot| (slot, false)).or_else(|| {
        (0..=u8::MAX)
            .find(|slot| !occupied.contains(slot))
            .map(|slot| (slot, false))
    })
}

fn parse_registry_string_line(line: &str) -> Option<(&str, &str)> {
    let (name, rest) = line.split_once("REG_SZ")?;
    Some((name.trim(), rest.trim()))
}

fn tool_value_slot(name: &str) -> Option<u8> {
    ["Mfg", "Mtx", "Mid", "Mex", "Mag"]
        .into_iter()
        .find_map(|prefix| name.strip_prefix(prefix)?.parse().ok())
}

fn normalize_registry_command(value: &str) -> String {
    value
        .replace('"', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn add_registry_value(key: &str, name: &str, kind: &str, value: &str) -> Result<(), String> {
    let output = Command::new("reg")
        .args(["add", key, "/v", name, "/t", kind, "/d", value, "/f"])
        .output()
        .map_err(|error| format!("无法写入 Keil Tools 配置：{error}"))?;
    output
        .status
        .success()
        .then_some(())
        .ok_or_else(|| format!("写入 Keil Tools 配置项 {name} 失败"))
}

pub fn validate_project_build(
    project_path: &Path,
    ccs_path: &Path,
    keil_path: &Path,
    ccs_in_place: bool,
) -> Result<BuildValidationReport, String> {
    validate_project_build_with_progress(project_path, ccs_path, keil_path, ccs_in_place, 2, |_| {})
}

pub fn validate_project_build_with_progress<F>(
    project_path: &Path,
    ccs_path: &Path,
    keil_path: &Path,
    ccs_in_place: bool,
    search_depth: u8,
    mut progress: F,
) -> Result<BuildValidationReport, String>
where
    F: FnMut(&str),
{
    if search_depth > 4 {
        return Err("工具目录搜索层级只能是 0–4".into());
    }
    match detect_project(project_path)? {
        ProjectKind::Ccs => validate_ccs_build(
            ccs_path,
            project_path,
            ccs_in_place,
            search_depth,
            &mut progress,
        ),
        ProjectKind::Keil => {
            validate_keil_build(keil_path, project_path, search_depth, &mut progress)
        }
    }
}

pub fn cleanup_validation_copy(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let temp = std::env::temp_dir()
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let path = path.canonicalize().map_err(|error| error.to_string())?;
    let valid_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.starts_with("ti-toolbox-ccs-validation-"));
    if !path.starts_with(&temp) || !valid_name {
        return Err("拒绝清理非 TI工具箱 临时验证目录".into());
    }
    fs::remove_dir_all(path).map_err(|error| format!("无法清理临时验证目录：{error}"))
}

fn validate_ccs_build(
    ccs_path: &Path,
    project_path: &Path,
    ccs_in_place: bool,
    search_depth: u8,
    progress: &mut dyn FnMut(&str),
) -> Result<BuildValidationReport, String> {
    let server = locate_ccs_server(ccs_path, search_depth)?;
    let ccs_root = server
        .parent()
        .and_then(Path::parent)
        .ok_or("无法确定 CCS 安装根目录")?;
    let gmake = ccs_root.join("utils/bin/gmake.exe");
    if !gmake.is_file() {
        return Err(format!("CCS 缺少构建工具：{}", gmake.display()));
    }

    let inspection = inspect_project(project_path)?;
    let project_root = if project_path.is_dir() {
        project_path
    } else {
        project_path.parent().ok_or("无法确定 CCS 工程目录")?
    };
    let projectspec = (!project_root.join(".cproject").is_file())
        .then(|| find_project_file(project_root, "projectspec", 3))
        .transpose()?
        .flatten();
    let import_location = projectspec.as_deref().unwrap_or(project_root);
    let temp = unique_temp_dir("ccs-validation");
    let metadata = temp.join("metadata");
    let workspace = temp.join("workspace");
    fs::create_dir_all(&metadata).map_err(|error| error.to_string())?;
    fs::create_dir_all(&workspace).map_err(|error| error.to_string())?;

    let uses_temp_project = !ccs_in_place || projectspec.is_some();
    let result = (|| {
        let import_path = import_location.to_string_lossy();
        let mut import_arguments = vec!["-ccs.location", import_path.as_ref()];
        if uses_temp_project {
            import_arguments.push("-ccs.copyIntoWorkspace");
        }
        progress("===== 导入 CCS 工程 =====\n");
        let import = run_ccs(
            &server,
            &metadata,
            &workspace,
            "com.ti.ccs.apps.importProject",
            &import_arguments,
            progress,
        )?;
        if !import.status.success() {
            return Err(format!("CCS 导入工程失败：\n{}", import.log));
        }

        progress("\n===== CCS Clean Build =====\n");
        let clean = run_ccs(
            &server,
            &metadata,
            &workspace,
            "com.ti.ccs.apps.buildProject",
            &[
                "-ccs.projects",
                &inspection.name,
                "-ccs.buildType",
                "clean",
                "-ccs.listProblems",
            ],
            progress,
        )?;
        progress("\n===== CCS Full Build =====\n");
        let full = run_ccs(
            &server,
            &metadata,
            &workspace,
            "com.ti.ccs.apps.buildProject",
            &[
                "-ccs.projects",
                &inspection.name,
                "-ccs.buildType",
                "full",
                "-ccs.listProblems",
            ],
            progress,
        )?;
        let normal_success = clean.status.success() && full.status.success();
        let mut log = format!("{}\n{}", clean.log, full.log);
        let normal_success = normal_success && log.contains("0 out of 1 projects have errors");
        if !normal_success {
            return Ok(BuildValidationReport {
                success: false,
                summary: "CCS Clean + Full Build 失败".into(),
                log,
                log_path: None,
                validated_project_path: None,
                cleanup_path: None,
            });
        }

        let built_project = if uses_temp_project {
            workspace.join(&inspection.name)
        } else {
            project_root.to_path_buf()
        };
        let build_dir = find_ccs_build_dir(&built_project, 4)?
            .ok_or("CCS 构建成功，但未找到生成的 makefile/ccsObjs.opt")?;
        progress("\n===== CCS 严格链接（保留未使用 section）=====\n");
        let strict = run_strict_ccs_link(&gmake, &build_dir, &temp, progress)?;
        let CommandResult {
            status,
            log: strict_log,
        } = strict;
        log.push_str("\n\n===== CCS 严格链接（保留未使用 section）=====\n");
        log.push_str(&strict_log);
        let success = status.success();
        Ok(BuildValidationReport {
            success,
            summary: if success {
                "CCS Clean + Full Build 与严格链接均通过".into()
            } else if strict_log.contains("unresolved symbols remain") {
                "CCS 普通构建通过，但严格链接发现未定义符号".into()
            } else {
                "CCS 普通构建通过，但严格链接失败".into()
            },
            log,
            log_path: None,
            validated_project_path: (success && uses_temp_project)
                .then(|| built_project.to_string_lossy().into_owned()),
            cleanup_path: (success && uses_temp_project)
                .then(|| temp.to_string_lossy().into_owned()),
        })
    })();
    let keep_temp = result
        .as_ref()
        .is_ok_and(|report| report.success && uses_temp_project);
    if !keep_temp {
        let _ = fs::remove_dir_all(&temp);
    }
    result
}

fn run_ccs(
    server: &Path,
    metadata: &Path,
    workspace: &Path,
    application: &str,
    arguments: &[&str],
    progress: &mut dyn FnMut(&str),
) -> Result<CommandResult, String> {
    let mut command = Command::new(server);
    command
        .args(["-nosplash", "-data"])
        .arg(metadata)
        .args(["-application", application, "-ccs.launcher", "ti-toolbox"])
        .arg("-ccs.defaultImportDestination")
        .arg(workspace)
        .args(arguments);
    run_streaming_command(&mut command, progress).map_err(|error| format!("无法启动 CCS：{error}"))
}

fn run_strict_ccs_link(
    gmake: &Path,
    build_dir: &Path,
    temp: &Path,
    progress: &mut dyn FnMut(&str),
) -> Result<CommandResult, String> {
    let makefile = fs::read_to_string(build_dir.join("makefile"))
        .map_err(|error| format!("无法读取 CCS makefile：{error}"))?;
    let (patched, target, artifacts) = strict_makefile(&makefile)?;
    let strict_makefile = temp.join("strict-validation.mk");
    fs::write(&strict_makefile, patched).map_err(|error| error.to_string())?;
    let mut command = Command::new(gmake);
    command
        .args(["-f"])
        .arg(&strict_makefile)
        .arg(&target)
        .args(["-r", "-O"])
        .current_dir(build_dir);
    let output = run_streaming_command(&mut command, progress)
        .map_err(|error| format!("无法启动 CCS 严格链接：{error}"));
    let _ = fs::remove_file(&strict_makefile);
    for artifact in artifacts {
        let _ = fs::remove_file(build_dir.join(artifact));
    }
    output
}

fn strict_makefile(makefile: &str) -> Result<(String, String, Vec<String>), String> {
    let target = makefile
        .lines()
        .find_map(|line| {
            let (target, dependencies) = line.split_once(':')?;
            (dependencies.contains("$(OBJS)") && target.trim().ends_with(".out"))
                .then(|| target.trim().to_string())
        })
        .ok_or("CCS makefile 中未找到链接目标")?;
    if !makefile.contains("$(ORDERED_OBJS)") || !makefile.contains("-Wl,--rom_model") {
        return Err("CCS makefile 中未找到可复用的 TI 链接命令".into());
    }
    let stem = target.trim_end_matches(".out");
    let strict_stem = "ti-toolbox-strict-validation";
    let strict_target = format!("{strict_stem}.out");
    let map = format!("{strict_stem}.map");
    let xml = format!("{strict_stem}_linkInfo.xml");
    let patched = makefile
        .replace(&target, &strict_target)
        .replace(&format!("{stem}.map"), &map)
        .replace(&format!("{stem}_linkInfo.xml"), &xml)
        .replacen(
            "-Wl,--rom_model",
            "-Wl,--unused_section_elimination=off -Wl,--rom_model",
            1,
        );
    Ok((
        patched,
        strict_target.clone(),
        vec![strict_target, map, xml],
    ))
}

fn validate_keil_build(
    keil_path: &Path,
    project_path: &Path,
    search_depth: u8,
    progress: &mut dyn FnMut(&str),
) -> Result<BuildValidationReport, String> {
    let uv4 = locate_uv4(keil_path, search_depth)?;
    let project = if project_path.is_file() {
        project_path.to_path_buf()
    } else {
        find_project_file(project_path, "uvprojx", 3)?.ok_or("Keil 工程中未找到 .uvprojx")?
    };
    let log_path = project
        .parent()
        .unwrap_or(Path::new("."))
        .join("ti-toolbox-keil-build.log");
    let _ = fs::remove_file(&log_path);
    progress("===== Keil Build =====\n");
    Command::new(&uv4)
        .arg("-b")
        .arg(&project)
        .arg("-o")
        .arg(&log_path)
        .status()
        .map_err(|error| format!("无法启动 Keil：{error}"))?;

    let deadline = Instant::now() + Duration::from_secs(120);
    let mut emitted = 0;
    let log = loop {
        if let Ok(bytes) = fs::read(&log_path) {
            if bytes.len() < emitted {
                emitted = 0;
            }
            if bytes.len() > emitted {
                let chunk = String::from_utf8_lossy(&bytes[emitted..]);
                progress(&chunk);
                emitted = bytes.len();
            }
            let log = String::from_utf8_lossy(&bytes).into_owned();
            if log.contains("Build Time Elapsed:") {
                break log;
            }
        }
        if Instant::now() >= deadline {
            return Err("等待 Keil 构建日志超时".into());
        }
        thread::sleep(Duration::from_millis(200));
    };
    let success = keil_log_succeeded(&log);
    Ok(BuildValidationReport {
        success,
        summary: if success {
            "Keil 构建通过".into()
        } else {
            "Keil 构建失败".into()
        },
        log,
        log_path: Some(log_path.to_string_lossy().into_owned()),
        validated_project_path: None,
        cleanup_path: None,
    })
}

fn keil_log_succeeded(log: &str) -> bool {
    log.lines().any(|line| line.contains(" - 0 Error(s),"))
}

fn locate_toolchains(
    ccs_path: &Path,
    keil_path: &Path,
    search_depth: u8,
) -> Result<(PathBuf, PathBuf), String> {
    if search_depth > 4 {
        return Err("工具目录搜索层级只能是 0–4".into());
    }
    Ok((
        locate_ccs_server(ccs_path, search_depth)?,
        locate_uv4(keil_path, search_depth)?,
    ))
}

fn locate_ccs_server(selected: &Path, search_depth: u8) -> Result<PathBuf, String> {
    locate_tool(
        selected,
        "CCS",
        "ccs-serverc.exe",
        &["../eclipse/ccs-serverc.exe"],
        search_depth,
    )
}

fn locate_uv4(selected: &Path, search_depth: u8) -> Result<PathBuf, String> {
    locate_tool(selected, "Keil", "UV4.exe", &[], search_depth)
}

fn locate_tool(
    selected: &Path,
    name: &str,
    file_name: &str,
    special_candidates: &[&str],
    search_depth: u8,
) -> Result<PathBuf, String> {
    if selected.is_file() {
        return selected
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case(file_name))
            .then(|| selected.to_path_buf())
            .ok_or_else(|| format!("{name} 路径无效：请选择 {file_name} 或其上级目录"));
    }
    if !selected.is_dir() {
        return Err(format!("{name} 路径不存在：{}", selected.display()));
    }
    for candidate in special_candidates {
        let path = selected.join(candidate);
        if path.is_file() {
            return Ok(path);
        }
    }
    find_tool_bounded(selected, file_name, search_depth)?.ok_or_else(|| {
        format!(
            "{name} 路径无效：在 {} 向下 {search_depth} 级未找到 {file_name}",
            selected.display()
        )
    })
}

fn find_tool_bounded(root: &Path, file_name: &str, depth: u8) -> Result<Option<PathBuf>, String> {
    let mut directories = vec![root.to_path_buf()];
    for level in 0..=depth {
        directories.sort();
        let mut next = Vec::new();
        for directory in directories {
            let entries = match fs::read_dir(&directory) {
                Ok(entries) => entries,
                Err(error) if directory == root => {
                    return Err(format!("无法搜索工具目录 {}：{error}", directory.display()));
                }
                Err(_) => continue,
            };
            let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                if file_type.is_file()
                    && entry
                        .file_name()
                        .to_str()
                        .is_some_and(|value| value.eq_ignore_ascii_case(file_name))
                {
                    return Ok(Some(entry.path()));
                }
                if level < depth && file_type.is_dir() {
                    next.push(entry.path());
                }
            }
        }
        directories = next;
    }
    Ok(None)
}

fn find_ccs_build_dir(root: &Path, depth: usize) -> Result<Option<PathBuf>, String> {
    fn visit(
        root: &Path,
        depth: usize,
        best: &mut Option<(SystemTime, PathBuf)>,
    ) -> Result<(), String> {
        let makefile = root.join("makefile");
        if makefile.is_file() && root.join("ccsObjs.opt").is_file() {
            let modified = makefile
                .metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            if best
                .as_ref()
                .map_or(true, |(current, _)| modified > *current)
            {
                *best = Some((modified, root.to_path_buf()));
            }
        }
        if depth == 0 || !root.is_dir() {
            return Ok(());
        }
        for entry in fs::read_dir(root).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            if entry
                .file_type()
                .map_err(|error| error.to_string())?
                .is_dir()
            {
                visit(&entry.path(), depth - 1, best)?;
            }
        }
        Ok(())
    }

    let mut best = None;
    visit(root, depth, &mut best)?;
    Ok(best.map(|(_, path)| path))
}

fn unique_temp_dir(name: &str) -> PathBuf {
    let id = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!("ti-toolbox-{name}-{id}"))
}

fn run_streaming_command(
    command: &mut Command,
    progress: &mut dyn FnMut(&str),
) -> Result<CommandResult, String> {
    fn forward<R: Read + Send + 'static>(stream: R, sender: mpsc::Sender<Vec<u8>>) {
        thread::spawn(move || {
            let mut reader = BufReader::new(stream);
            loop {
                let mut chunk = Vec::new();
                match reader.read_until(b'\n', &mut chunk) {
                    Ok(0) => break,
                    Ok(_) => {
                        if sender.send(chunk).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(format!("读取构建日志失败：{error}\n").into_bytes());
                        break;
                    }
                }
            }
        });
    }

    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    let (sender, receiver) = mpsc::channel();
    forward(
        child.stdout.take().ok_or("无法读取构建标准输出")?,
        sender.clone(),
    );
    forward(
        child.stderr.take().ok_or("无法读取构建错误输出")?,
        sender.clone(),
    );
    drop(sender);

    let mut log = String::new();
    for bytes in receiver {
        let chunk = String::from_utf8_lossy(&bytes);
        progress(&chunk);
        log.push_str(&chunk);
    }
    let status = child.wait().map_err(|error| error.to_string())?;
    Ok(CommandResult { status, log })
}

pub fn convert_project(request: &ConversionRequest) -> Result<ConversionReport, String> {
    let project_path = Path::new(&request.project_path);
    let sdk_path = Path::new(&request.sdk_path);
    let pack_path = Path::new(&request.pack_path);
    let output_path = Path::new(&request.output_path);
    let inspection = inspect_project(project_path)?;
    let resources = if inspection.kind == ProjectKind::Ccs {
        let resources = validate_resources(sdk_path, pack_path)?;
        if !resources
            .devices
            .iter()
            .any(|device| device == &inspection.device)
        {
            return Err(format!("所选 Pack 不支持器件 {}", inspection.device));
        }
        Some(resources)
    } else {
        validate_sdk(sdk_path)?;
        None
    };
    let template_root = sdk_path
        .join("examples/nortos")
        .join(format!("LP_{}", inspection.device))
        .join("driverlib/empty");
    if !template_root.is_dir() {
        return Err(format!(
            "SDK 中没有 {} 的官方 NoRTOS empty 模板",
            inspection.device
        ));
    }
    ensure_output_available(output_path)?;
    let staging = staging_path(output_path);
    fs::create_dir_all(&staging).map_err(|error| format!("无法创建输出目录：{error}"))?;
    let result = (|| {
        let copied = copy_project_sources(project_path, &inspection, &staging)?;
        match inspection.kind {
            ProjectKind::Ccs => generate_keil(
                &inspection,
                resources.as_ref().unwrap(),
                sdk_path,
                &template_root,
                &staging,
                &copied,
            ),
            ProjectKind::Keil => generate_ccs(&inspection, &template_root, &staging, &copied),
        }
    })();
    let (mut generated_files, mut warnings) = match result {
        Ok(result) => result,
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
    };
    generated_files.extend(list_relative_files(&staging)?);
    dedup(&mut generated_files);
    warnings.splice(0..0, inspection.warnings.clone());
    if output_path.exists() {
        fs::remove_dir(output_path).map_err(|error| format!("无法使用空输出目录：{error}"))?;
    }
    fs::rename(&staging, output_path).map_err(|error| format!("无法完成输出：{error}"))?;
    Ok(ConversionReport {
        source_kind: inspection.kind,
        target_kind: inspection.target_kind,
        device: inspection.device,
        output_path: output_path.to_string_lossy().into_owned(),
        generated_files,
        warnings,
    })
}

#[derive(Debug, Clone)]
struct CopiedFile {
    relative: String,
    group: String,
    file_type: String,
}

fn ensure_output_available(output: &Path) -> Result<(), String> {
    if !output.exists() {
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("无法创建输出目录：{error}"))?;
        }
        return Ok(());
    }
    if !output.is_dir() {
        return Err("输出路径已存在且不是目录".into());
    }
    if fs::read_dir(output)
        .map_err(|error| error.to_string())?
        .next()
        .is_some()
    {
        return Err("输出目录必须为空，工具不会覆盖已有文件".into());
    }
    Ok(())
}

fn staging_path(output: &Path) -> PathBuf {
    let id = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    let name = output
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("project");
    output
        .parent()
        .unwrap_or(Path::new("."))
        .join(format!(".{name}.ti-toolbox-{id}"))
}

fn copy_project_sources(
    project_path: &Path,
    inspection: &ProjectInspection,
    output: &Path,
) -> Result<Vec<CopiedFile>, String> {
    let root = if project_path.is_dir() {
        project_path
    } else {
        project_path.parent().ok_or("无法确定源工程目录")?
    };
    let mut copied = Vec::new();
    for file in &inspection.files {
        let lower = file.path.to_ascii_lowercase();
        if lower.contains("startup_") {
            continue;
        }
        if inspection.target_kind == ProjectKind::Ccs
            && inspection
                .files
                .iter()
                .any(|item| item.file_type == "syscfg")
            && (lower.ends_with("ti_msp_dl_config.c") || lower.ends_with("ti_msp_dl_config.h"))
        {
            continue;
        }
        let source = resolve_source_file(project_path, root, &file.path, inspection.kind)?;
        if !source.is_file() {
            continue;
        }
        let relative = safe_source_path(&file.path);
        let target = output.join(&relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::copy(&source, &target)
            .map_err(|error| format!("复制 {} 失败：{error}", source.display()))?;
        copied.push(CopiedFile {
            relative: relative.to_string_lossy().replace('\\', "/"),
            group: file.group.clone(),
            file_type: file.file_type.clone(),
        });
    }
    if inspection.kind == ProjectKind::Ccs && inspection.target_kind == ProjectKind::Keil {
        include_ccs_sysconfig_outputs(root, output, &mut copied)?;
    }
    if copied.is_empty() {
        return Err("源工程没有可转换的 C/C++、头文件、汇编或 SysConfig 文件".into());
    }
    Ok(copied)
}

fn include_ccs_sysconfig_outputs(
    project_root: &Path,
    output: &Path,
    copied: &mut Vec<CopiedFile>,
) -> Result<(), String> {
    let requires_sysconfig = copied.iter().any(|file| file.file_type == "syscfg")
        || copied.iter().any(|file| {
            matches!(file.file_type.as_str(), "source" | "header")
                && fs::read_to_string(output.join(&file.relative))
                    .is_ok_and(|content| content.contains("ti_msp_dl_config.h"))
        });
    if !requires_sysconfig {
        return Ok(());
    }

    for (name, file_type) in [
        ("ti_msp_dl_config.h", "header"),
        ("ti_msp_dl_config.c", "source"),
    ] {
        if let Some(existing) = copied.iter().find(|file| {
            Path::new(&file.relative)
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case(name))
        }) {
            if name.eq_ignore_ascii_case("ti_msp_dl_config.h") {
                make_sysconfig_header_armclang_compatible(&output.join(&existing.relative))?;
            }
            continue;
        }
        let source = find_generated_sysconfig_file(project_root, name)?.ok_or_else(|| {
            format!("工程使用 SysConfig，但未找到 {name}；请先在 CCS 中构建一次工程，再重新转换")
        })?;
        let relative = PathBuf::from("src/generated").join(name);
        let target = output.join(&relative);
        fs::create_dir_all(target.parent().unwrap()).map_err(|error| error.to_string())?;
        fs::copy(&source, &target)
            .map_err(|error| format!("复制 {} 失败：{error}", source.display()))?;
        if name.eq_ignore_ascii_case("ti_msp_dl_config.h") {
            make_sysconfig_header_armclang_compatible(&target)?;
        }
        copied.push(CopiedFile {
            relative: relative.to_string_lossy().replace('\\', "/"),
            group: "SysConfig".into(),
            file_type: file_type.into(),
        });
    }
    Ok(())
}

fn make_sysconfig_header_armclang_compatible(header: &Path) -> Result<(), String> {
    let content = fs::read_to_string(header)
        .map_err(|error| format!("无法读取 {}：{error}", header.display()))?;
    if !content.contains("SYSCONFIG_WEAK")
        || content.contains("defined(__clang__)")
        || content.contains("defined(__ARMCC_VERSION)")
    {
        return Ok(());
    }
    let replacement = "defined(__GNUC__) || defined(__clang__) || defined(__ARMCC_VERSION)";
    let updated = if content.contains("defined(__GNUC__)") {
        content.replacen("defined(__GNUC__)", replacement, 1)
    } else {
        return Err(format!(
            "{} 中的 SYSCONFIG_WEAK 定义无法自动兼容 ArmClang",
            header.display()
        ));
    };
    fs::write(header, updated).map_err(|error| format!("无法更新 {}：{error}", header.display()))
}

fn find_generated_sysconfig_file(root: &Path, name: &str) -> Result<Option<PathBuf>, String> {
    for candidate in [
        root.join(name),
        root.join("Debug/syscfg").join(name),
        root.join("Release/syscfg").join(name),
        root.join("debug/syscfg").join(name),
        root.join("release/syscfg").join(name),
    ] {
        if candidate.is_file() {
            return Ok(Some(candidate));
        }
    }
    find_named_file(root, name, 4)
}

fn find_named_file(
    root: &Path,
    name: &str,
    remaining_depth: usize,
) -> Result<Option<PathBuf>, String> {
    for entry in fs::read_dir(root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_file()
            && entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(name)
        {
            return Ok(Some(entry.path()));
        }
        if remaining_depth > 0 && file_type.is_dir() {
            if let Some(path) = find_named_file(&entry.path(), name, remaining_depth - 1)? {
                return Ok(Some(path));
            }
        }
    }
    Ok(None)
}

fn resolve_source_file(
    selected: &Path,
    root: &Path,
    relative: &str,
    kind: ProjectKind,
) -> Result<PathBuf, String> {
    let direct = root.join(relative);
    if direct.is_file() {
        return Ok(direct);
    }
    if kind == ProjectKind::Keil {
        let uvprojx = if selected.is_file() {
            selected.to_path_buf()
        } else {
            find_project_file(selected, "uvprojx", 3)?.ok_or("未找到 Keil 工程文件")?
        };
        let from_project = uvprojx.parent().unwrap_or(root).join(relative);
        if from_project.is_file() {
            return Ok(from_project);
        }
    }
    Ok(direct)
}

fn safe_source_path(path: &str) -> PathBuf {
    let mut safe = PathBuf::from("src");
    let mut external = false;
    for component in Path::new(path).components() {
        match component {
            Component::Normal(value) => safe.push(value),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => external = true,
            Component::CurDir => {}
        }
    }
    if external {
        let tail = safe.strip_prefix("src").unwrap_or(&safe).to_path_buf();
        PathBuf::from("src/external").join(tail)
    } else {
        safe
    }
}

fn generate_keil(
    inspection: &ProjectInspection,
    resources: &ResourceInfo,
    sdk_path: &Path,
    template_root: &Path,
    output: &Path,
    copied: &[CopiedFile],
) -> Result<(Vec<String>, Vec<String>), String> {
    let template_dir = template_root.join("keil");
    let template = find_first_file(&template_dir, "uvprojx")?.ok_or("SDK 模板缺少 .uvprojx")?;
    let text = fs::read_to_string(&template).map_err(|error| error.to_string())?;
    let mut xml =
        Element::parse(text.as_bytes()).map_err(|error| format!("Keil 模板无法解析：{error}"))?;
    let project_name = normalized_project_name(&inspection.name, "keil");
    set_element_text(&mut xml, "TargetName", &project_name)?;
    set_element_text(&mut xml, "OutputName", &project_name)?;
    set_element_text(&mut xml, "LayName", &project_name).ok();
    set_element_text(&mut xml, "Device", &inspection.device)?;
    set_pack_id(
        &mut xml,
        &format!(
            "TexasInstruments.{}.{}",
            resources.pack_name, resources.pack_version
        ),
    )?;

    let mut defines = inspection.defines.clone();
    defines.push(format!("__{}__", inspection.device));
    dedup(&mut defines);
    set_element_text(&mut xml, "Define", &defines.join(";"))?;

    let mut include_paths = BTreeSet::from([".".to_string(), "src".to_string()]);
    for file in copied {
        if let Some(parent) = Path::new(&file.relative).parent() {
            include_paths.insert(parent.to_string_lossy().replace('\\', "/"));
        }
    }
    include_paths.insert(sdk_path.join("source").to_string_lossy().replace('/', "\\"));
    include_paths.insert(
        sdk_path
            .join("source/third_party/CMSIS/Core/Include")
            .to_string_lossy()
            .replace('/', "\\"),
    );
    set_element_text(
        &mut xml,
        "IncludePath",
        &include_paths.into_iter().collect::<Vec<_>>().join(";"),
    )?;

    let startup = find_named_source(&template_dir, "startup_", &["s", "asm"])?;
    let scatter = find_first_file(&template_dir, "sct")?.ok_or("SDK 模板缺少 .sct")?;
    let startup_name = startup
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("启动文件名无效")?;
    let scatter_name = scatter
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("链接脚本名无效")?;
    fs::copy(&startup, output.join(startup_name)).map_err(|error| error.to_string())?;
    fs::copy(&scatter, output.join(scatter_name)).map_err(|error| error.to_string())?;
    set_element_text(&mut xml, "ScatterFile", &format!("./{scatter_name}"))?;

    if let Some(library) = element_text(&xml, "Misc") {
        let resolved = template_dir.join(library.replace('\\', "/"));
        let library_path = resolved.canonicalize().unwrap_or(resolved);
        set_element_text(
            &mut xml,
            "Misc",
            &format!("\"{}\"", keil_windows_path(&library_path)),
        )?;
    }
    replace_keil_groups(&mut xml, copied, startup_name)?;

    let mut warnings = Vec::new();
    if copied.iter().any(|file| file.file_type == "syscfg") {
        if let Some(before_make) = find_element_mut(&mut xml, "RunUserProg1") {
            set_text(before_make, "0");
        }
        warnings.push(
            "已保留 SysConfig 及生成文件，但关闭了 SDK 模板中绑定本机路径的预编译脚本；修改 .syscfg 后需在本机重新生成"
                .into(),
        );
    }
    let project_file = format!("{project_name}.uvprojx");
    write_xml(&xml, &output.join(&project_file))?;
    Ok((
        vec![project_file, startup_name.into(), scatter_name.into()],
        warnings,
    ))
}

fn keil_windows_path(path: &Path) -> String {
    let value = path.to_string_lossy().replace('/', "\\");
    if let Some(unc) = value.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else if let Some(local) = value.strip_prefix(r"\\?\") {
        local.to_string()
    } else {
        value
    }
}

fn generate_ccs(
    inspection: &ProjectInspection,
    template_root: &Path,
    output: &Path,
    copied: &[CopiedFile],
) -> Result<(Vec<String>, Vec<String>), String> {
    let template_dir = template_root.join("ticlang");
    let template = find_first_file(&template_dir, "projectspec")?
        .ok_or("SDK 模板缺少 TI Clang .projectspec")?;
    let text = fs::read_to_string(&template).map_err(|error| error.to_string())?;
    let mut xml =
        Element::parse(text.as_bytes()).map_err(|error| format!("CCS 模板无法解析：{error}"))?;
    let project_name = normalized_project_name(&inspection.name, "ccs");
    let project = find_element_mut(&mut xml, "project").ok_or("CCS 模板缺少 project 节点")?;
    project
        .attributes
        .insert("title".into(), inspection.name.clone());
    project
        .attributes
        .insert("name".into(), project_name.clone());
    project
        .attributes
        .insert("device".into(), inspection.device.clone());

    let mut options = project
        .attributes
        .get("compilerBuildOptions")
        .cloned()
        .unwrap_or_default();
    for define in &inspection.defines {
        options.push_str(&format!("\n            -D{define}"));
    }
    let mut include_dirs = BTreeSet::new();
    for file in copied {
        if let Some(parent) = Path::new(&file.relative).parent() {
            include_dirs.insert(parent.to_string_lossy().replace('\\', "/"));
        }
    }
    for directory in include_dirs {
        options.push_str(&format!("\n            -I${{PROJECT_ROOT}}/{directory}"));
    }
    project
        .attributes
        .insert("compilerBuildOptions".into(), options);
    project
        .children
        .retain(|node| !matches!(node, XMLNode::Element(element) if element.name == "file"));
    let mut opened = false;
    for file in copied {
        let mut element = Element::new("file");
        element
            .attributes
            .insert("path".into(), file.relative.clone());
        element.attributes.insert("action".into(), "copy".into());
        element
            .attributes
            .insert("excludeFromBuild".into(), "false".into());
        let open = !opened && file.file_type == "source";
        element
            .attributes
            .insert("openOnCreation".into(), open.to_string());
        opened |= open;
        if file.file_type != "syscfg" {
            if let Some(parent) = Path::new(&file.relative).parent() {
                element.attributes.insert(
                    "targetDirectory".into(),
                    parent.to_string_lossy().replace('\\', "/"),
                );
            }
        }
        project.children.push(XMLNode::Element(element));
    }
    if let Some(context) = find_element_mut(&mut xml, "context") {
        context
            .attributes
            .insert("deviceId".into(), inspection.device.clone());
    }
    let project_file = format!("{project_name}.projectspec");
    write_xml(&xml, &output.join(&project_file))?;
    Ok((
        vec![project_file],
        vec!["Keil 的 Scatter 文件、下载器和 ArmClang 专属参数未写入 CCS；CCS 将使用所选 SDK 的 TI Clang 官方配置".into()],
    ))
}

fn replace_keil_groups(
    xml: &mut Element,
    copied: &[CopiedFile],
    startup_name: &str,
) -> Result<(), String> {
    let groups = find_element_mut(xml, "Groups").ok_or("Keil 模板缺少 Groups")?;
    groups.children.clear();
    let mut by_group: BTreeMap<String, Vec<&CopiedFile>> = BTreeMap::new();
    for file in copied {
        by_group.entry(file.group.clone()).or_default().push(file);
    }
    by_group.entry("Device".into()).or_default();
    for (name, files) in by_group {
        let mut group = Element::new("Group");
        group
            .children
            .push(XMLNode::Element(text_element("GroupName", &name)));
        let mut list = Element::new("Files");
        for file in files {
            list.children.push(XMLNode::Element(keil_file_element(
                Path::new(&file.relative)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or(&file.relative),
                &file.relative,
                keil_file_type(&file.relative, &file.file_type),
            )));
        }
        if name == "Device" {
            list.children.push(XMLNode::Element(keil_file_element(
                startup_name,
                startup_name,
                "2",
            )));
        }
        group.children.push(XMLNode::Element(list));
        groups.children.push(XMLNode::Element(group));
    }
    Ok(())
}

fn keil_file_element(name: &str, path: &str, file_type: &str) -> Element {
    let mut file = Element::new("File");
    file.children
        .push(XMLNode::Element(text_element("FileName", name)));
    file.children
        .push(XMLNode::Element(text_element("FileType", file_type)));
    file.children.push(XMLNode::Element(text_element(
        "FilePath",
        &path.replace('/', "\\"),
    )));
    file
}

fn keil_file_type(path: &str, kind: &str) -> &'static str {
    let extension = Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(extension.as_str(), "s" | "asm") {
        "2"
    } else if kind == "source" {
        "1"
    } else {
        "5"
    }
}

fn set_pack_id(xml: &mut Element, pack_id: &str) -> Result<(), String> {
    if let Some(element) = find_element_mut(xml, "PackID") {
        set_text(element, pack_id);
        return Ok(());
    }
    let common =
        find_element_mut(xml, "TargetCommonOption").ok_or("Keil 模板缺少 TargetCommonOption")?;
    let position = common
        .children
        .iter()
        .position(|node| matches!(node, XMLNode::Element(element) if element.name == "Vendor"))
        .map(|index| index + 1)
        .unwrap_or(0);
    common
        .children
        .insert(position, XMLNode::Element(text_element("PackID", pack_id)));
    Ok(())
}

fn set_element_text(element: &mut Element, name: &str, value: &str) -> Result<(), String> {
    let target = find_element_mut(element, name).ok_or_else(|| format!("模板缺少 {name}"))?;
    set_text(target, value);
    Ok(())
}

fn set_text(element: &mut Element, value: &str) {
    element
        .children
        .retain(|node| !matches!(node, XMLNode::Text(_)));
    element.children.insert(0, XMLNode::Text(value.into()));
}

fn text_element(name: &str, value: &str) -> Element {
    let mut element = Element::new(name);
    set_text(&mut element, value);
    element
}

fn find_element_mut<'a>(element: &'a mut Element, name: &str) -> Option<&'a mut Element> {
    if element.name == name {
        return Some(element);
    }
    element.children.iter_mut().find_map(|node| match node {
        XMLNode::Element(child) => find_element_mut(child, name),
        _ => None,
    })
}

fn find_named_source(root: &Path, prefix: &str, extensions: &[&str]) -> Result<PathBuf, String> {
    for entry in fs::read_dir(root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if name.starts_with(prefix)
            && extensions
                .iter()
                .any(|item| extension.eq_ignore_ascii_case(item))
        {
            return Ok(path);
        }
    }
    Err(format!("SDK 模板缺少 {prefix}* 启动文件"))
}

fn normalized_project_name(name: &str, suffix: &str) -> String {
    let base: String = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '_' | '-') {
                character
            } else {
                '_'
            }
        })
        .collect();
    if base.to_ascii_lowercase().ends_with(&format!("_{suffix}")) {
        base
    } else {
        format!("{base}_{suffix}")
    }
}

fn write_xml(xml: &Element, path: &Path) -> Result<(), String> {
    let mut buffer = Vec::new();
    xml.write_with_config(
        &mut buffer,
        xmltree::EmitterConfig::new()
            .perform_indent(true)
            .write_document_declaration(true),
    )
    .map_err(|error| error.to_string())?;
    fs::write(path, buffer).map_err(|error| error.to_string())
}

fn list_relative_files(root: &Path) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    fn visit(root: &Path, current: &Path, files: &mut Vec<String>) -> Result<(), String> {
        for entry in fs::read_dir(current).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            if entry
                .file_type()
                .map_err(|error| error.to_string())?
                .is_dir()
            {
                visit(root, &entry.path(), files)?;
            } else {
                files.push(
                    entry
                        .path()
                        .strip_prefix(root)
                        .unwrap_or(&entry.path())
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
        Ok(())
    }
    visit(root, root, &mut files)?;
    Ok(files)
}

pub fn inspect_project(path: &Path) -> Result<ProjectInspection, String> {
    match detect_project(path)? {
        ProjectKind::Ccs => inspect_ccs(path),
        ProjectKind::Keil => inspect_keil(path),
    }
}

fn inspect_ccs(path: &Path) -> Result<ProjectInspection, String> {
    let root = if path.is_dir() {
        path
    } else {
        path.parent().ok_or("无法确定 CCS 工程目录")?
    };
    let cproject_path = root.join(".cproject");
    if !cproject_path.is_file() {
        return inspect_projectspec(path);
    }
    let cproject_text = fs::read_to_string(&cproject_path)
        .map_err(|error| format!("无法读取 .cproject：{error}"))?;
    let cproject = Element::parse(cproject_text.as_bytes())
        .map_err(|error| format!(".cproject 无法解析：{error}"))?;
    let mut defines = Vec::new();
    let mut include_paths = Vec::new();
    collect_ccs_options(&cproject, &mut defines, &mut include_paths);
    dedup(&mut defines);
    dedup(&mut include_paths);

    let name = fs::read_to_string(root.join(".project"))
        .ok()
        .and_then(|text| Element::parse(text.as_bytes()).ok())
        .and_then(|project| {
            find_element(&project, "name")
                .and_then(Element::get_text)
                .map(|value| value.into_owned())
        })
        .unwrap_or_else(|| file_name(root));
    let device = find_device(&cproject_text).ok_or("无法从 CCS 工程识别 MSPM0 芯片")?;
    let excluded_sources = ccs_source_exclusions(&cproject);
    let mut files = Vec::new();
    collect_local_sources(root, root, &excluded_sources, &mut files)?;
    files.sort_by(|left, right| left.path.cmp(&right.path));
    let mut warnings = files
        .iter()
        .filter(|file| {
            file.path.to_ascii_lowercase().contains("startup_") && file.file_type == "source"
        })
        .map(|file| {
            format!(
                "{} 是 CCS 启动文件，转换时将替换为目标工具链版本",
                file.path
            )
        })
        .collect::<Vec<_>>();
    let requires_sysconfig = files.iter().any(|file| file.file_type == "syscfg")
        || files.iter().any(|file| {
            matches!(file.file_type.as_str(), "source" | "header")
                && fs::read_to_string(root.join(&file.path))
                    .is_ok_and(|content| content.contains("ti_msp_dl_config.h"))
        });
    if requires_sysconfig
        && (find_generated_sysconfig_file(root, "ti_msp_dl_config.c")?.is_none()
            || find_generated_sysconfig_file(root, "ti_msp_dl_config.h")?.is_none())
    {
        warnings.push(
            "工程使用 SysConfig，但缺少 ti_msp_dl_config.c/h；如果本机没有 CCS，请让发送方先构建一次并一并发送生成文件，或在本机配置 SysConfig/CCS 后再转换"
                .into(),
        );
    }
    Ok(ProjectInspection {
        kind: ProjectKind::Ccs,
        target_kind: ProjectKind::Keil,
        name,
        device,
        files,
        include_paths,
        defines,
        warnings,
    })
}

fn inspect_keil(_path: &Path) -> Result<ProjectInspection, String> {
    let uvprojx = if _path.is_file() {
        _path.to_path_buf()
    } else {
        find_project_file(_path, "uvprojx", 3)?.ok_or("Keil 工程中未找到 .uvprojx")?
    };
    let text =
        fs::read_to_string(&uvprojx).map_err(|error| format!("无法读取 Keil 工程：{error}"))?;
    let xml =
        Element::parse(text.as_bytes()).map_err(|error| format!(".uvprojx 无法解析：{error}"))?;
    let name = find_element(&xml, "TargetName")
        .and_then(Element::get_text)
        .map(|value| value.into_owned())
        .unwrap_or_else(|| file_name(&uvprojx));
    let device = find_element(&xml, "Device")
        .and_then(Element::get_text)
        .map(|value| value.into_owned())
        .or_else(|| find_device(&text))
        .ok_or("无法从 Keil 工程识别 MSPM0 芯片")?;
    let mut defines = split_list(element_text(&xml, "Define").as_deref());
    let mut include_paths = split_list(element_text(&xml, "IncludePath").as_deref());
    dedup(&mut defines);
    dedup(&mut include_paths);
    let selected_root = if _path.is_dir() {
        _path
    } else {
        uvprojx.parent().unwrap_or(Path::new("."))
    };
    let mut files = Vec::new();
    collect_keil_groups(&xml, &uvprojx, selected_root, &mut files);
    let mut warnings = Vec::new();
    if let Some(libraries) = element_text(&xml, "Misc") {
        if libraries.contains(".a") {
            warnings
                .push("Keil 预编译库不会直接用于 CCS，将尝试从 SDK 选择 TI Clang 对应库".into());
        }
    }
    Ok(ProjectInspection {
        kind: ProjectKind::Keil,
        target_kind: ProjectKind::Ccs,
        name,
        device,
        files,
        include_paths,
        defines,
        warnings,
    })
}

fn collect_keil_groups(
    element: &Element,
    uvprojx: &Path,
    selected_root: &Path,
    files: &mut Vec<ProjectFile>,
) {
    if element.name == "Group" {
        let group = child_text(element, "GroupName").unwrap_or_else(|| "Source".into());
        if let Some(file_list) = element.get_child("Files") {
            for child in &file_list.children {
                let XMLNode::Element(file) = child else {
                    continue;
                };
                if file.name != "File" {
                    continue;
                }
                let Some(raw_path) = child_text(file, "FilePath") else {
                    continue;
                };
                let resolved = uvprojx.parent().unwrap_or(Path::new(".")).join(&raw_path);
                let Some(kind) = source_type(&resolved) else {
                    continue;
                };
                let display = resolved
                    .canonicalize()
                    .ok()
                    .and_then(|path| {
                        path.strip_prefix(selected_root.canonicalize().ok()?)
                            .ok()
                            .map(Path::to_path_buf)
                    })
                    .unwrap_or_else(|| PathBuf::from(&raw_path))
                    .to_string_lossy()
                    .replace('\\', "/");
                files.push(ProjectFile {
                    path: display,
                    group: group.clone(),
                    file_type: kind.into(),
                });
            }
        }
        return;
    }
    for child in &element.children {
        if let XMLNode::Element(child) = child {
            collect_keil_groups(child, uvprojx, selected_root, files);
        }
    }
}

fn element_text(element: &Element, name: &str) -> Option<String> {
    find_element(element, name)
        .and_then(Element::get_text)
        .map(|value| value.trim().to_string())
}

fn split_list(value: Option<&str>) -> Vec<String> {
    value
        .unwrap_or("")
        .split([';', ','])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn inspect_projectspec(path: &Path) -> Result<ProjectInspection, String> {
    let projectspec = if path.is_file() {
        path.to_path_buf()
    } else {
        find_project_file(path, "projectspec", 3)?.ok_or("CCS 工程中未找到 .projectspec")?
    };
    let text = fs::read_to_string(&projectspec).map_err(|error| error.to_string())?;
    let xml = Element::parse(text.as_bytes()).map_err(|error| error.to_string())?;
    let project = find_element(&xml, "project").ok_or(".projectspec 缺少 project 节点")?;
    let device = project
        .attributes
        .get("device")
        .cloned()
        .ok_or(".projectspec 缺少器件")?;
    let name = project
        .attributes
        .get("name")
        .cloned()
        .unwrap_or_else(|| file_name(&projectspec));
    let mut files = Vec::new();
    collect_projectspec_files(project, &projectspec, &mut files);
    Ok(ProjectInspection {
        kind: ProjectKind::Ccs,
        target_kind: ProjectKind::Keil,
        name,
        device,
        files,
        include_paths: Vec::new(),
        defines: Vec::new(),
        warnings: Vec::new(),
    })
}

fn collect_ccs_options(
    element: &Element,
    defines: &mut Vec<String>,
    include_paths: &mut Vec<String>,
) {
    if element.name == "option" {
        let class = element
            .attributes
            .get("superClass")
            .map(String::as_str)
            .unwrap_or("");
        let target = if class.contains("DEFINE") {
            Some(&mut *defines)
        } else if class.contains("INCLUDE_PATH") {
            Some(&mut *include_paths)
        } else {
            None
        };
        if let Some(target) = target {
            collect_attribute(element, "listOptionValue", "value", target);
        }
    }
    for child in &element.children {
        if let XMLNode::Element(child) = child {
            collect_ccs_options(child, defines, include_paths);
        }
    }
}

fn collect_local_sources(
    root: &Path,
    current: &Path,
    excluded_sources: &[String],
    files: &mut Vec<ProjectFile>,
) -> Result<(), String> {
    const SKIPPED: &[&str] = &[
        ".git",
        ".settings",
        "Debug",
        "Release",
        "Objects",
        "Listings",
        "targetConfigs",
    ];
    for entry in fs::read_dir(current).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        let relative = entry
            .path()
            .strip_prefix(root)
            .unwrap_or(&entry.path())
            .to_string_lossy()
            .replace('\\', "/");
        if file_type.is_dir() {
            if !SKIPPED.contains(&entry.file_name().to_string_lossy().as_ref()) {
                collect_local_sources(root, &entry.path(), excluded_sources, files)?;
            }
            continue;
        }
        let Some(kind) = source_type(&entry.path()) else {
            continue;
        };
        if kind != "header" && is_ccs_source_excluded(&relative, excluded_sources) {
            continue;
        }
        files.push(ProjectFile {
            path: relative,
            group: source_group(kind).into(),
            file_type: kind.into(),
        });
    }
    Ok(())
}

fn ccs_source_exclusions(cproject: &Element) -> Vec<String> {
    let mut exclusions = Vec::new();
    let Some(source_entries) = find_element(cproject, "sourceEntries") else {
        return exclusions;
    };
    for child in &source_entries.children {
        let XMLNode::Element(entry) = child else {
            continue;
        };
        let base = normalize_ccs_source_path(
            entry
                .attributes
                .get("name")
                .map(String::as_str)
                .unwrap_or(""),
        );
        for excluded in entry
            .attributes
            .get("excluding")
            .map(String::as_str)
            .unwrap_or("")
            .split('|')
        {
            let excluded = normalize_ccs_source_path(excluded);
            if excluded.is_empty() {
                continue;
            }
            if base.is_empty() || excluded == base || excluded.starts_with(&format!("{base}/")) {
                exclusions.push(excluded);
            } else {
                exclusions.push(format!("{base}/{excluded}"));
            }
        }
    }
    dedup(&mut exclusions);
    exclusions
}

fn normalize_ccs_source_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim_matches('/')
        .to_ascii_lowercase()
}

fn is_ccs_source_excluded(path: &str, exclusions: &[String]) -> bool {
    let path = normalize_ccs_source_path(path);
    exclusions.iter().any(|excluded| {
        path == *excluded
            || path
                .strip_prefix(excluded)
                .is_some_and(|suffix| suffix.starts_with('/'))
    })
}

fn collect_projectspec_files(project: &Element, projectspec: &Path, files: &mut Vec<ProjectFile>) {
    for child in &project.children {
        let XMLNode::Element(element) = child else {
            continue;
        };
        if element.name != "file" {
            continue;
        }
        let Some(path) = element.attributes.get("path") else {
            continue;
        };
        let resolved = projectspec.parent().unwrap_or(Path::new(".")).join(path);
        let Some(kind) = source_type(&resolved) else {
            continue;
        };
        files.push(ProjectFile {
            path: path.replace('\\', "/"),
            group: source_group(kind).into(),
            file_type: kind.into(),
        });
    }
}

fn source_type(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "c" | "cc" | "cpp" | "cxx" | "s" | "asm" => Some("source"),
        "h" | "hpp" => Some("header"),
        "syscfg" => Some("syscfg"),
        _ => None,
    }
}

fn source_group(kind: &str) -> &'static str {
    match kind {
        "header" => "Headers",
        "syscfg" => "SysConfig",
        _ => "Source",
    }
}

fn find_device(text: &str) -> Option<String> {
    let start = text.find("MSPM0")?;
    let device: String = text[start..]
        .chars()
        .take_while(|character| character.is_ascii_alphanumeric())
        .collect();
    (device.len() > 5).then_some(device)
}

fn file_name(path: &Path) -> String {
    path.file_stem()
        .or_else(|| path.file_name())
        .and_then(|value| value.to_str())
        .unwrap_or("project")
        .to_string()
}

fn dedup(values: &mut Vec<String>) {
    values.sort();
    values.dedup();
}

pub fn validate_resources(sdk_path: &Path, pack_path: &Path) -> Result<ResourceInfo, String> {
    let sdk_version = validate_sdk(sdk_path)?;
    let (pack_name, pack_version, devices) = validate_pack(pack_path)?;
    Ok(ResourceInfo {
        sdk_version,
        pack_name,
        pack_version,
        devices,
    })
}

fn validate_pack(pack_path: &Path) -> Result<(String, String, Vec<String>), String> {
    let pdsc = Element::parse(read_pdsc(pack_path)?.as_bytes())
        .map_err(|error| format!("Pack PDSC 无法解析：{error}"))?;
    let pack_name = child_text(&pdsc, "name").ok_or("Pack PDSC 缺少名称")?;
    let pack_version = find_element(&pdsc, "release")
        .and_then(|element| element.attributes.get("version"))
        .cloned()
        .ok_or("Pack PDSC 缺少版本号")?;
    let mut devices = Vec::new();
    collect_attribute(&pdsc, "device", "Dname", &mut devices);
    devices.sort();
    devices.dedup();
    if devices.is_empty() {
        return Err("Pack PDSC 未声明任何器件".into());
    }
    Ok((pack_name, pack_version, devices))
}

fn validate_sdk(sdk_path: &Path) -> Result<String, String> {
    let product_path = sdk_path.join(".metadata/product.json");
    let product: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&product_path)
            .map_err(|_| format!("SDK 无效：未找到 {}", product_path.display()))?,
    )
    .map_err(|error| format!("SDK product.json 无法解析：{error}"))?;
    if product.get("name").and_then(|value| value.as_str()) != Some("mspm0_sdk") {
        return Err("所选目录不是 MSPM0 SDK".into());
    }
    Ok(product
        .get("version")
        .and_then(|value| value.as_str())
        .ok_or("SDK product.json 缺少版本号")?
        .to_string())
}

fn read_pdsc(pack_path: &Path) -> Result<String, String> {
    if pack_path.is_dir() {
        let path = find_first_file(pack_path, "pdsc")?.ok_or("Pack 目录中未找到 .pdsc")?;
        return fs::read_to_string(&path)
            .map_err(|error| format!("无法读取 {}：{error}", path.display()));
    }
    if !pack_path.is_file() {
        return Err("Pack 文件或目录不存在".into());
    }
    if pack_path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("pdsc"))
    {
        return fs::read_to_string(pack_path)
            .map_err(|error| format!("无法读取 {}：{error}", pack_path.display()));
    }
    let file = File::open(pack_path).map_err(|error| format!("无法打开 Pack：{error}"))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|error| format!("Pack 不是有效压缩包：{error}"))?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
        if entry.name().to_ascii_lowercase().ends_with(".pdsc") {
            let mut content = String::new();
            entry
                .read_to_string(&mut content)
                .map_err(|error| format!("无法读取 Pack PDSC：{error}"))?;
            return Ok(content);
        }
    }
    Err("Pack 中未找到 .pdsc".into())
}

fn find_first_file(root: &Path, extension: &str) -> Result<Option<PathBuf>, String> {
    for entry in fs::read_dir(root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_dir() {
            if let Some(path) = find_first_file(&entry.path(), extension)? {
                return Ok(Some(path));
            }
        } else if entry
            .path()
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case(extension))
        {
            return Ok(Some(entry.path()));
        }
    }
    Ok(None)
}

fn child_text(element: &Element, name: &str) -> Option<String> {
    element
        .get_child(name)
        .and_then(Element::get_text)
        .map(|value| value.trim().to_string())
}

fn find_element<'a>(element: &'a Element, name: &str) -> Option<&'a Element> {
    if element.name == name {
        return Some(element);
    }
    element.children.iter().find_map(|node| match node {
        XMLNode::Element(child) => find_element(child, name),
        _ => None,
    })
}

fn collect_attribute(
    element: &Element,
    element_name: &str,
    attribute: &str,
    values: &mut Vec<String>,
) {
    if element.name == element_name {
        if let Some(value) = element.attributes.get(attribute) {
            values.push(value.clone());
        }
    }
    for child in &element.children {
        if let XMLNode::Element(child) = child {
            collect_attribute(child, element_name, attribute, values);
        }
    }
}

pub fn detect_project(path: &Path) -> Result<ProjectKind, String> {
    if path.is_file() {
        return match path.extension().and_then(|value| value.to_str()) {
            Some("uvprojx") => Ok(ProjectKind::Keil),
            Some("projectspec") => Ok(ProjectKind::Ccs),
            _ => Err("请选择 CCS/Keil 工程目录或工程文件".into()),
        };
    }

    if !path.is_dir() {
        return Err("工程路径不存在".into());
    }
    if path.join(".cproject").is_file() {
        return Ok(ProjectKind::Ccs);
    }
    let ccs = find_project_file(path, "projectspec", 3)?.is_some();
    let keil = find_project_file(path, "uvprojx", 3)?.is_some();
    match (ccs, keil) {
        (true, false) => return Ok(ProjectKind::Ccs),
        (false, true) => return Ok(ProjectKind::Keil),
        (true, true) => {
            return Err("目录中同时存在 CCS 和 Keil 工程，请选择更具体的工程子目录".into())
        }
        (false, false) => {}
    }
    Err("未找到 CCS 的 .cproject/.projectspec 或 Keil 的 .uvprojx".into())
}

fn find_project_file(
    root: &Path,
    extension: &str,
    remaining_depth: usize,
) -> Result<Option<PathBuf>, String> {
    for entry in fs::read_dir(root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_file()
            && entry
                .path()
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case(extension))
        {
            return Ok(Some(entry.path()));
        }
        if remaining_depth > 0 && file_type.is_dir() {
            if let Some(path) = find_project_file(&entry.path(), extension, remaining_depth - 1)? {
                return Ok(Some(path));
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, time::SystemTime};

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let id = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("ti-toolbox-{name}-{id}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn detects_ccs_and_keil_projects_from_a_directory() {
        let ccs = temp_dir("ccs");
        fs::write(ccs.join(".cproject"), "<cproject/>").unwrap();
        assert_eq!(detect_project(&ccs).unwrap(), ProjectKind::Ccs);

        let keil = temp_dir("keil");
        fs::create_dir_all(keil.join("keil")).unwrap();
        fs::write(keil.join("keil/demo.uvprojx"), "<Project/>").unwrap();
        assert_eq!(detect_project(&keil).unwrap(), ProjectKind::Keil);

        fs::remove_dir_all(ccs).unwrap();
        fs::remove_dir_all(keil).unwrap();

        let mixed = temp_dir("mixed");
        fs::write(mixed.join("demo.projectspec"), "<projectSpec/>").unwrap();
        fs::write(mixed.join("demo.uvprojx"), "<Project/>").unwrap();
        assert!(detect_project(&mixed).is_err());
        fs::remove_dir_all(mixed).unwrap();
    }

    #[test]
    fn validates_user_selected_sdk_and_pack() {
        let root = temp_dir("resources");
        let sdk = root.join("sdk");
        fs::create_dir_all(sdk.join(".metadata")).unwrap();
        fs::write(
            sdk.join(".metadata/product.json"),
            r#"{"name":"mspm0_sdk","version":"2.10.00.04"}"#,
        )
        .unwrap();
        let pack = root.join("pack");
        fs::create_dir_all(&pack).unwrap();
        fs::write(
            pack.join("TexasInstruments.pdsc"),
            r#"<package><name>MSPM0_DFP</name><releases><release version="1.3.1"/></releases><devices><family><device Dname="MSPM0G3507"/></family></devices></package>"#,
        )
        .unwrap();

        let info = validate_resources(&sdk, &pack).unwrap();
        assert_eq!(info.sdk_version, "2.10.00.04");
        assert_eq!(info.pack_version, "1.3.1");
        assert_eq!(info.devices, ["MSPM0G3507"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discovers_an_installed_pack_for_the_project_device() {
        let root = temp_dir("discover-environment");
        let project = root.join("project");
        let sdk = root.join("mspm0_sdk_2_10_00_04");
        let keil = root.join("Keil_v5");
        let pack = keil.join("ARM/Packs/TexasInstruments/MSPM0G1X0X_G3X0X_DFP/1.3.1");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(sdk.join(".metadata")).unwrap();
        fs::create_dir_all(keil.join("UV4")).unwrap();
        fs::create_dir_all(&pack).unwrap();
        fs::write(
            project.join(".project"),
            "<projectDescription><name>demo</name></projectDescription>",
        )
        .unwrap();
        fs::write(project.join(".cproject"), r#"<cproject><option superClass="compiler.DEFINE"><listOptionValue value="__MSPM0G3507__"/></option></cproject>"#).unwrap();
        fs::write(project.join("main.c"), "int main(void) { return 0; }").unwrap();
        fs::write(
            sdk.join(".metadata/product.json"),
            r#"{"name":"mspm0_sdk","version":"2.10.00.04"}"#,
        )
        .unwrap();
        fs::write(keil.join("UV4/UV4.exe"), "fixture").unwrap();
        fs::write(
            pack.join("TexasInstruments.MSPM0G1X0X_G3X0X_DFP.pdsc"),
            r#"<package><vendor>TexasInstruments</vendor><name>MSPM0G1X0X_G3X0X_DFP</name><releases><release version="1.3.1"/></releases><devices><family><device Dname="MSPM0G3507"/></family></devices></package>"#,
        )
        .unwrap();
        fs::create_dir_all(keil.join("ARM/PACK/.Web")).unwrap();
        fs::write(
            keil.join("ARM/PACK/.Web/TexasInstruments.MSPM0G1X0X_G3X0X_DFP.pdsc"),
            r#"<package><vendor>TexasInstruments</vendor><name>MSPM0G1X0X_G3X0X_DFP</name><releases><release version="9.9.9"/></releases><devices><family><device Dname="MSPM0G3507"/></family></devices></package>"#,
        )
        .unwrap();

        let request = EnvironmentRequest {
            project_path: project.to_string_lossy().into_owned(),
            sdk_path: sdk.to_string_lossy().into_owned(),
            pack_path: String::new(),
            ccs_path: String::new(),
            keil_path: keil.to_string_lossy().into_owned(),
            sysconfig_path: String::new(),
            search_depth: 2,
        };
        let found = discover_environment(&request).unwrap();

        assert_eq!(
            Path::new(found.pack_path.as_deref().unwrap())
                .canonicalize()
                .unwrap(),
            pack.canonicalize().unwrap()
        );
        assert!(found.pack_installed);
        assert_eq!(found.pack_name.as_deref(), Some("MSPM0G1X0X_G3X0X_DFP"));
        assert_eq!(found.pack_version.as_deref(), Some("1.3.1"));
        assert_eq!(
            found.pack_download_url.as_deref(),
            Some("https://www.keil.arm.com/packs/mspm0g1x0x_g3x0x_dfp-texasinstruments/boards/")
        );

        fs::remove_dir_all(keil.join("ARM/Packs/TexasInstruments")).unwrap();
        let catalog = discover_environment(&request).unwrap();
        assert!(!catalog.pack_installed);
        assert_eq!(catalog.pack_version.as_deref(), Some("9.9.9"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discovers_keil_environment_without_a_project() {
        let root = temp_dir("discover-keil-environment");
        let sdk = root.join("mspm0_sdk_2_10_00_04");
        let keil = root.join("Keil_v5");
        let sysconfig = root.join("sysconfig_1.26.2");
        fs::create_dir_all(sdk.join(".metadata")).unwrap();
        fs::create_dir_all(keil.join("UV4")).unwrap();
        fs::create_dir_all(sysconfig.join("nw")).unwrap();
        fs::write(
            sdk.join(".metadata/product.json"),
            r#"{"name":"mspm0_sdk","version":"2.10.00.04"}"#,
        )
        .unwrap();
        fs::write(keil.join("UV4/UV4.exe"), "fixture").unwrap();
        fs::write(sysconfig.join("nw/nw.exe"), "fixture").unwrap();
        fs::write(sysconfig.join("sysconfig_cli.bat"), "fixture").unwrap();

        let found = discover_keil_environment(&KeilEnvironmentRequest {
            sdk_path: sdk.to_string_lossy().into_owned(),
            keil_path: keil.to_string_lossy().into_owned(),
            sysconfig_path: sysconfig.to_string_lossy().into_owned(),
            search_depth: 2,
        })
        .unwrap();

        assert_eq!(found.sdk_version.as_deref(), Some("2.10.00.04"));
        assert!(found
            .keil_executable
            .as_deref()
            .is_some_and(|path| path.ends_with("UV4.exe")));
        assert!(found
            .sysconfig_executable
            .as_deref()
            .is_some_and(|path| path.ends_with("nw.exe")));
        assert!(found.warnings.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn keeps_existing_keil_tools_and_reuses_an_equivalent_sysconfig_entry() {
        let registry = r#"
Mtx0    REG_SZ    User Tool
Mex0    REG_SZ    C:\tools\user.exe
Mtx1    REG_SZ    Sysconfig v1.26.2
Mex1    REG_SZ    "D:\ti\ccs2100\sysconfig_1.26.2\nw\nw.exe" "D:\ti\ccs2100\sysconfig_1.26.2"
"#;
        assert_eq!(
            choose_keil_tool_slot(
                registry,
                r#""D:\ti\ccs2100\sysconfig_1.26.2\nw\nw.exe" "D:\ti\ccs2100\sysconfig_1.26.2""#,
            ),
            Some((1, true))
        );

        let without_sysconfig =
            "Mtx0    REG_SZ    User Tool\nMex0    REG_SZ    C:\\tools\\user.exe";
        assert_eq!(
            choose_keil_tool_slot(without_sysconfig, "sysconfig-command"),
            Some((1, false))
        );
    }

    #[test]
    fn updates_the_sdk_keil_sysconfig_files_with_selected_paths() {
        let bat = "@echo off\r\nset SYSCFG_PATH=old\\sysconfig_cli.bat\r\necho ready\r\n";
        let cfg = "[Old]\r\nCommand=old\r\nInitial Folder=old\r\nArguments=keep\r\n";
        let (updated_bat, updated_cfg) = render_keil_sysconfig_files(
            bat,
            cfg,
            Path::new(r"D:\ti\mspm0_sdk_2_10_00_04"),
            "2.10.00.04",
            Path::new(r"D:\ti\sysconfig_1.26.2"),
        )
        .unwrap();

        assert!(
            updated_bat.contains(r#"set SYSCFG_PATH="D:\ti\sysconfig_1.26.2\sysconfig_cli.bat""#)
        );
        assert!(updated_cfg.contains("[Sysconfig v1.26.2 - MSPM0 SDK v2_10_00_04]"));
        assert!(updated_cfg
            .contains(r#"Command="D:\ti\sysconfig_1.26.2\nw\nw.exe" "D:\ti\sysconfig_1.26.2""#));
        assert!(updated_cfg.contains(r"Initial Folder=D:\ti\mspm0_sdk_2_10_00_04"));
        assert!(updated_cfg.contains("Arguments=keep"));
    }

    #[test]
    fn inspects_a_ccs_project_for_conversion() {
        let root = temp_dir("inspect-ccs");
        fs::write(
            root.join(".project"),
            "<projectDescription><name>Blinky</name></projectDescription>",
        )
        .unwrap();
        fs::write(
            root.join(".cproject"),
            r#"<cproject><option superClass="compiler.DEFINE"><listOptionValue value="APP_DEBUG"/><listOptionValue value="__MSPM0G3507__"/></option><option superClass="compiler.INCLUDE_PATH"><listOptionValue value="${PROJECT_ROOT}/include"/></option></cproject>"#,
        )
        .unwrap();
        fs::write(root.join("main.c"), "int main(void) { return 0; }").unwrap();

        let result = inspect_project(&root).unwrap();
        assert_eq!(result.name, "Blinky");
        assert_eq!(result.device, "MSPM0G3507");
        assert_eq!(result.defines, ["APP_DEBUG", "__MSPM0G3507__"]);
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.target_kind, ProjectKind::Keil);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ignores_ccs_source_directories_excluded_from_build() {
        let root = temp_dir("inspect-ccs-exclusions");
        fs::create_dir_all(root.join("Drivers/DISABLED")).unwrap();
        fs::create_dir_all(root.join("Drivers/ACTIVE")).unwrap();
        fs::write(
            root.join(".project"),
            "<projectDescription><name>Blinky</name></projectDescription>",
        )
        .unwrap();
        fs::write(
            root.join(".cproject"),
            r#"<cproject><storageModule><configuration><sourceEntries><entry excluding="Drivers/DISABLED" flags="VALUE_WORKSPACE_PATH|RESOLVED" kind="sourcePath" name=""/></sourceEntries><option superClass="compiler.DEFINE"><listOptionValue value="__MSPM0G3507__"/></option></configuration></storageModule></cproject>"#,
        )
        .unwrap();
        fs::write(root.join("main.c"), "int main(void) { return 0; }").unwrap();
        fs::write(
            root.join("Drivers/DISABLED/disabled.c"),
            "void disabled(void) {}",
        )
        .unwrap();
        fs::write(
            root.join("Drivers/DISABLED/disabled.h"),
            "void disabled(void);",
        )
        .unwrap();
        fs::write(root.join("Drivers/ACTIVE/active.c"), "void active(void) {}").unwrap();

        let result = inspect_project(&root).unwrap();
        assert!(result
            .files
            .iter()
            .any(|file| file.path.ends_with("active.c")));
        assert!(!result
            .files
            .iter()
            .any(|file| file.path.ends_with("disabled.c")));
        assert!(result
            .files
            .iter()
            .any(|file| file.path.ends_with("disabled.h")));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn warns_when_a_received_ccs_project_lacks_generated_sysconfig_files() {
        let root = temp_dir("inspect-missing-sysconfig");
        fs::write(
            root.join(".project"),
            "<projectDescription><name>Blinky</name></projectDescription>",
        )
        .unwrap();
        fs::write(
            root.join(".cproject"),
            r#"<cproject><option superClass="compiler.DEFINE"><listOptionValue value="__MSPM0G3507__"/></option></cproject>"#,
        )
        .unwrap();
        fs::write(root.join("main.c"), "#include \"ti_msp_dl_config.h\"").unwrap();
        fs::write(root.join("demo.syscfg"), "// fixture").unwrap();

        let result = inspect_project(&root).unwrap();
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("ti_msp_dl_config.c/h") && warning.contains("发送方")));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn inspects_a_keil_project_for_conversion() {
        let root = temp_dir("inspect-keil");
        fs::write(root.join("main.c"), "int main(void) { return 0; }").unwrap();
        fs::write(
            root.join("Blinky.uvprojx"),
            r#"<Project><Targets><Target><TargetName>Blinky</TargetName><TargetOption><TargetCommonOption><Device>MSPM0G3507</Device></TargetCommonOption><TargetArmAds><Cads><VariousControls><Define>APP_DEBUG;__MSPM0G3507__</Define><IncludePath>.;include</IncludePath></VariousControls></Cads></TargetArmAds></TargetOption><Groups><Group><GroupName>App</GroupName><Files><File><FileName>main.c</FileName><FileType>1</FileType><FilePath>main.c</FilePath></File></Files></Group></Groups></Target></Targets></Project>"#,
        )
        .unwrap();

        let result = inspect_project(&root).unwrap();
        assert_eq!(result.name, "Blinky");
        assert_eq!(result.device, "MSPM0G3507");
        assert_eq!(result.files[0].group, "App");
        assert_eq!(result.target_kind, ProjectKind::Ccs);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn converts_the_sample_ccs_project_to_keil() {
        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let output = temp_dir("ccs-to-keil").join("output");
        let report = convert_project(&ConversionRequest {
            project_path: workspace.join("ccs_project").to_string_lossy().into_owned(),
            sdk_path: workspace
                .join("data/mspm0_sdk_2_10_00_04")
                .to_string_lossy()
                .into_owned(),
            pack_path: workspace
                .join("data/TexasInstruments.MSPM0G1X0X_G3X0X_DFP.1.3.1.pack")
                .to_string_lossy()
                .into_owned(),
            output_path: output.to_string_lossy().into_owned(),
        })
        .unwrap();

        assert_eq!(report.target_kind, ProjectKind::Keil);
        assert!(report
            .generated_files
            .iter()
            .any(|path| path.ends_with(".uvprojx")));
        assert!(output.join("src/main.c").is_file());
        let uvprojx = report
            .generated_files
            .iter()
            .find(|path| path.ends_with(".uvprojx"))
            .unwrap();
        let project_xml = Element::parse(fs::File::open(output.join(uvprojx)).unwrap()).unwrap();
        let linker_input = element_text(&project_xml, "Misc").unwrap();
        assert!(linker_input.starts_with('"') && linker_input.ends_with('"'));
        let linker_path = linker_input.trim_matches('"');
        assert!(!linker_path.starts_with(r"\\?\") && !linker_path.starts_with(r"\?\"));
        assert!(linker_path.ends_with("driverlib.a"));
        fs::remove_dir_all(output.parent().unwrap()).unwrap();
    }

    #[test]
    fn ccs_to_keil_includes_sysconfig_files_generated_under_debug() {
        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let root = temp_dir("ccs-sysconfig");
        let input = root.join("input");
        let output = root.join("output");
        fs::create_dir_all(input.join("Debug/syscfg")).unwrap();
        fs::write(
            input.join(".project"),
            "<projectDescription><name>sysconfig-demo</name></projectDescription>",
        )
        .unwrap();
        fs::write(
            input.join(".cproject"),
            r#"<cproject><option superClass="compiler.DEFINE"><listOptionValue value="__MSPM0G3507__"/></option></cproject>"#,
        )
        .unwrap();
        fs::write(
            input.join("main.c"),
            "#include \"ti_msp_dl_config.h\"\nint main(void) { return 0; }",
        )
        .unwrap();
        fs::write(input.join("project.syscfg"), "// fixture").unwrap();
        fs::write(
            input.join("Debug/syscfg/ti_msp_dl_config.h"),
            r#"#if defined(__ti_version__) || defined(__TI_COMPILER_VERSION__)
#define SYSCONFIG_WEAK __attribute__((weak))
#elif defined(__IAR_SYSTEMS_ICC__)
#define SYSCONFIG_WEAK __weak
#elif defined(__GNUC__)
#define SYSCONFIG_WEAK __attribute__((weak))
#endif
void SYSCFG_DL_init(void);"#,
        )
        .unwrap();
        fs::write(
            input.join("Debug/syscfg/ti_msp_dl_config.c"),
            "void SYSCFG_DL_init(void) {}",
        )
        .unwrap();

        let report = convert_project(&ConversionRequest {
            project_path: input.to_string_lossy().into_owned(),
            sdk_path: workspace
                .join("data/mspm0_sdk_2_10_00_04")
                .to_string_lossy()
                .into_owned(),
            pack_path: workspace
                .join("data/TexasInstruments.MSPM0G1X0X_G3X0X_DFP.1.3.1.pack")
                .to_string_lossy()
                .into_owned(),
            output_path: output.to_string_lossy().into_owned(),
        })
        .unwrap();

        assert!(report
            .generated_files
            .iter()
            .any(|path| path.ends_with("ti_msp_dl_config.h")));
        assert!(report
            .generated_files
            .iter()
            .any(|path| path.ends_with("ti_msp_dl_config.c")));
        let uvprojx = report
            .generated_files
            .iter()
            .find(|path| path.ends_with(".uvprojx"))
            .unwrap();
        let project_xml = fs::read_to_string(output.join(uvprojx)).unwrap();
        assert!(project_xml.contains("src\\generated"));
        assert!(project_xml.contains("ti_msp_dl_config.h"));
        assert!(project_xml.contains("ti_msp_dl_config.c"));
        let generated_header =
            fs::read_to_string(output.join("src/generated/ti_msp_dl_config.h")).unwrap();
        assert!(generated_header.contains("defined(__ARMCC_VERSION)"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn converts_the_sample_keil_project_to_ccs() {
        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let output = temp_dir("keil-to-ccs").join("output");
        let report = convert_project(&ConversionRequest {
            project_path: workspace
                .join("keil_project/keil")
                .to_string_lossy()
                .into_owned(),
            sdk_path: workspace
                .join("data/mspm0_sdk_2_10_00_04")
                .to_string_lossy()
                .into_owned(),
            pack_path: String::new(),
            output_path: output.to_string_lossy().into_owned(),
        })
        .unwrap();

        assert_eq!(report.target_kind, ProjectKind::Ccs);
        assert!(report
            .generated_files
            .iter()
            .any(|path| path.ends_with(".projectspec")));
        assert!(report
            .generated_files
            .iter()
            .any(|path| path.ends_with("empty.c")));
        fs::remove_dir_all(output.parent().unwrap()).unwrap();
    }

    #[test]
    fn makes_a_separate_strict_ccs_link_target() {
        let makefile = r#"
demo.out: $(OBJS) $(GEN_CMDS)
	"tiarmclang.exe" -Wl,-m"demo.map" -Wl,--xml_link_info="demo_linkInfo.xml" -Wl,--rom_model -o "demo.out" $(ORDERED_OBJS)
"#;
        let (patched, target, artifacts) = strict_makefile(makefile).unwrap();

        assert_eq!(target, "ti-toolbox-strict-validation.out");
        assert!(patched.contains("-Wl,--unused_section_elimination=off"));
        assert!(patched.contains("ti-toolbox-strict-validation.map"));
        assert!(patched.contains("ti-toolbox-strict-validation_linkInfo.xml"));
        assert!(!patched.contains("-o \"demo.out\""));
        assert_eq!(artifacts.len(), 3);
    }

    #[test]
    fn judges_keil_build_from_its_log_not_process_exit_code() {
        assert!(keil_log_succeeded(
            r#"".\Objects\demo.axf" - 0 Error(s), 0 Warning(s).
Build Time Elapsed: 00:00:01"#
        ));
        assert!(!keil_log_succeeded(
            r#"Error: L6218E: Undefined symbol TrackN.
".\Objects\demo.axf" - 1 Error(s), 0 Warning(s).
Build Time Elapsed: 00:00:01"#
        ));
    }

    #[test]
    fn accepts_ccs_and_keil_install_roots() {
        let root = temp_dir("toolchains");
        let ccs = root.join("ccs");
        let theia = ccs.join("theia");
        let keil = root.join("Keil_v5");
        fs::create_dir_all(ccs.join("eclipse")).unwrap();
        fs::create_dir_all(&theia).unwrap();
        fs::create_dir_all(keil.join("UV4")).unwrap();
        fs::write(ccs.join("eclipse/ccs-serverc.exe"), "fixture").unwrap();
        fs::write(keil.join("UV4/UV4.exe"), "fixture").unwrap();

        validate_toolchains(&theia, &keil).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn searches_tool_directories_only_to_the_selected_depth() {
        let root = temp_dir("nested-toolchain");
        let nested = root.join("one/two/three");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("UV4.exe"), "fixture").unwrap();

        assert!(locate_uv4(&root, 0).is_err());
        assert!(locate_uv4(&root, 2).is_err());
        assert_eq!(locate_uv4(&root, 3).unwrap(), nested.join("UV4.exe"));
        assert!(locate_toolchains(&root, &root, 5).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reports_an_unreadable_tool_search_root() {
        let root = temp_dir("missing-tool-root");
        let missing = root.join("not-created");
        assert!(find_tool_bounded(&missing, "UV4.exe", 2).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn only_cleans_its_own_validation_directory() {
        let validation = unique_temp_dir("ccs-validation");
        fs::create_dir_all(&validation).unwrap();
        fs::write(validation.join("fixture"), "test").unwrap();
        cleanup_validation_copy(&validation).unwrap();
        assert!(!validation.exists());

        let unrelated = temp_dir("unrelated");
        assert!(cleanup_validation_copy(&unrelated).is_err());
        fs::remove_dir_all(unrelated).unwrap();
    }
}
