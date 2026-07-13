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
7. 例外读取 `Debug/syscfg` 或 `Release/syscfg` 中的 `ti_msp_dl_config.c/h`；生成文件缺失时拒绝转换。
8. 在输出副本中为 `SYSCONFIG_WEAK` 补充 ArmClang 条件，源工程生成文件保持不变。

## Keil → CCS

1. 从 SDK 的 `LP_<DEVICE>/driverlib/empty/ticlang` 读取官方 `.projectspec`。
2. 写入工程名、器件、宏、文件清单和生成目录的 Include Path。
3. 源文件随转换结果保存，ProjectSpec 使用 `action="copy"` 导入。
4. 不迁移 Keil `.sct`、下载器配置和 ArmClang 专属参数。
5. 由 CCS 和 MSPM0 SDK 生成 TI Clang 的默认设备、链接及 SysConfig 配置。

## 构建验证

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
