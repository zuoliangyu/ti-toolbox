# 双向转换规则

## 公共模型

转换前先提取双方共有的信息：

| 字段 | CCS 来源 | Keil 来源 |
|---|---|---|
| 工程名 | `.project` / `.projectspec` | `TargetName` |
| 芯片 | `.cproject` / `.projectspec` | `Device` |
| 源文件 | 工程目录 / ProjectSpec file | `Groups/Files` |
| 宏定义 | `DEFINE` option | `VariousControls/Define` |
| Include Path | `INCLUDE_PATH` option | `VariousControls/IncludePath` |
| SysConfig | `.syscfg` 文件 | `.syscfg` 文件 |

## CCS → Keil

1. 从 SDK 的 `LP_<DEVICE>/driverlib/empty/keil` 读取官方 `.uvprojx`。
2. 用源工程的名称、文件、宏和本地头文件目录替换模板内容。
3. 从模板目录复制 Keil 启动文件和 `.sct`。
4. 将 SDK Include Path 和 Keil DriverLib 路径改为用户选择的 SDK 绝对路径。
   DriverLib 链接参数会去掉 Rust `canonicalize` 产生的 `\\?\` 前缀并使用双引号，支持 `D:\ccs sdk\...` 等含空格路径及旧版 Arm Linker。
5. 从 Pack PDSC 写入 `PackID`。
6. 不复制 CCS 启动文件、`.cmd`、`.ccxml` 和构建输出目录。
7. 遵循 `.cproject` 的 `sourceEntries/excluding`：不迁移已排除构建的 C/C++、汇编和 SysConfig 文件，但保留头文件及其 Keil Include Path，因为 CCS 的预处理器仍可从这些目录引用头文件。
8. 例外读取 `Debug/syscfg` 或 `Release/syscfg` 中的 `ti_msp_dl_config.c/h`；生成文件缺失时拒绝转换。
9. 在输出副本中为 `SYSCONFIG_WEAK` 补充 ArmClang 条件，源工程生成文件保持不变。

## Keil → CCS

1. 从 SDK 的 `LP_<DEVICE>/driverlib/empty/ticlang` 读取官方 `.projectspec`。
2. 写入工程名、器件、宏、文件清单和生成目录的 Include Path。
3. 源文件随转换结果保存，ProjectSpec 使用 `action="copy"` 导入。
4. 不迁移 Keil `.sct`、下载器配置和 ArmClang 专属参数。
5. 由 CCS 和 MSPM0 SDK 生成 TI Clang 的默认设备、链接及 SysConfig 配置。

## 构建验证

- CCS 与 Keil 安装目录会按用户设置的层级向下搜索；层级范围为 `0–4`，默认 `2`，并优先使用层级最浅的匹配项。
- 构建期间在界面底部实时追加日志；失败时直接展示日志末尾 18 行，结束后在验证结果中保留完整日志。
- IDE 仅在对应构建验证时需要；未执行验证时允许用户确认风险后转换，已有失败结果时拒绝继续转换。
- 目标 IDE 缺失时保留转换结果，并标记为“未执行目标构建验证”。

### CCS

1. 用户选择临时目录验证或原工程直接构建；默认使用临时目录。
2. 通过用户指定 CCS 中的 `ccs-serverc.exe` 导入工程，依次执行 Clean Build 和 Full Build。
3. 普通构建成功后，复用生成的对象文件与 `makefile` 创建临时链接目标，并加入 `--unused_section_elimination=off`。
4. 严格链接不会覆盖 CCS 正常生成的 `.out`、`.map` 或链接信息文件。
5. 临时目录验证成功后，转换使用临时副本中新生成的 SysConfig 文件，转换结束后清理副本。
6. 原工程直接构建会更新 `Debug`、`Release`、SysConfig 等构建产物，执行前必须向用户确认。

### Keil

1. 使用用户指定的 `UV4.exe` 和 `-b` 参数构建 `.uvprojx`。
2. 构建是否成功以日志中的 `Error(s)` 为准，不依赖 `UV4.exe` 进程退出码。
3. 转换成功但目标构建失败时，保留转换结果和完整构建日志，并明确区分两种状态。

## 安全规则

- 转换本身始终只读；只有用户明确选择“原工程直接构建”时，CCS 才会更新源工程构建产物。
- 输出目录必须不存在或为空。
- 先写入同级临时目录，全部成功后再替换为空的目标目录。
- Pack 不支持目标器件或 SDK 缺少官方模板时停止转换。
- 启动文件、链接脚本和目标工具链库只从用户指定的 SDK/Pack 派生，不做文本级猜测转换。

## 环境发现与 Keil SysConfig

- CCS → Keil 需要 SDK 和 Pack 元数据；CCS 只负责源构建验证，Keil 只负责目标构建验证。
- Keil → CCS 只强制要求 SDK，Pack 不参与 ProjectSpec 生成。
- 已安装 Pack 从 `ARM/PACK/TexasInstruments` 或 `ARM/Packs/TexasInstruments` 识别；`.Web` PDSC 可提供 PackID 和官方下载链接，但不视为已安装。
- 不自动下载 Pack，也不调用 Pack Installer。
- Keil TI 环境配置无需先选择工程；自动检测 MSPM0 SDK、Keil 与带 `nw/nw.exe` 的图形化 SysConfig，Pack 继续由用户手动安装。CLI-only SysConfig 不视为可配置 Keil Tools 图形化入口。
- 配置 Keil SysConfig 前必须由用户确认；工具更新 SDK 的 `syscfg.bat` 和 `MSPM0_SDK_syscfg_menu_import.cfg`，并为首次修改保留 `.ti-toolbox.bak`。
- Keil Tools 配置优先复用命令相同的现有项；否则更新 TI工具箱自有项或使用首个空闲槽位，不覆盖其他工具。
