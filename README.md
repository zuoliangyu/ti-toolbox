# CCS2KEIL

面向 TI MSPM0 的 CCS 与 Keil 工程双向转换工具。桌面端使用 Tauri 2，转换核心使用 Rust，界面不依赖前端框架。

## 当前能力

- 自动识别 CCS `.cproject` / `.projectspec` 与 Keil `.uvprojx`
- 用户自行选择 MSPM0 SDK 根目录和 CMSIS Pack 文件
- 校验 SDK、Pack 版本及 Pack 器件清单
- 转换前展示芯片、源文件、宏、Include Path 和风险提示
- CCS → Keil：基于 SDK 官方 Keil empty 工程生成 `.uvprojx`
- Keil → CCS：基于 SDK 官方 TI Clang empty 工程生成 `.projectspec`
- 输出到空目录，原工程只读且不覆盖已有文件

目前使用以下资源完成了实际样例验证：

- MSPM0 SDK `2.10.00.04`
- `TexasInstruments.MSPM0G1X0X_G3X0X_DFP` `1.3.1`
- `MSPM0G3507` NoRTOS DriverLib empty 工程

工具会按器件查找：

```text
<SDK>/examples/nortos/LP_<DEVICE>/driverlib/empty/
```

因此 Pack 支持某个器件并不等于当前 SDK 一定具有对应的官方 empty 模板；缺少模板时工具会停止转换并明确提示。

## 使用方式

1. 在“工具链资源”中选择 MSPM0 SDK 根目录和 `.pack` 文件。
2. 选择 CCS 或 Keil 工程目录，点击“检查工程”。
3. 选择一个空输出目录。
4. 确认转换方向和警告后开始转换。

SDK 和 Pack 路径保存在本机 WebView 的 `localStorage`，不会写进源工程。

### CCS 输出

Keil → CCS 生成 `.projectspec`。在 CCS 中使用“Import CCS Projects”或适用的 ProjectSpec 导入入口导入，不直接伪造与 CCS 版本绑定的 Eclipse 元数据。

### SysConfig

CCS → Keil 会保留 `.syscfg`，并从工程根目录或 `Debug/syscfg`、`Release/syscfg` 补入 `ti_msp_dl_config.c/h`。如果工程使用 SysConfig 但尚未生成这两个文件，转换会停止并提示先在 CCS 中构建一次，避免产生无法编译的 Keil 工程。

对 SDK 生成头中仅覆盖 TI/IAR/GNU 的 `SYSCONFIG_WEAK` 条件，转换器会在输出副本中补充 ArmClang 的 `__clang__` / `__ARMCC_VERSION` 判断，以兼容 Keil Arm Compiler 6.7 等旧版本。

SDK 自带的 Keil `syscfg.bat` 绑定本机 SysConfig 安装位置，而且要求工程位于 SDK 目录树内，因此外部输出工程会关闭该预编译步骤并给出警告；修改 `.syscfg` 后需在本机重新生成配置文件。

## 开发

环境要求：

- Rust 及 Cargo
- Node.js 20+
- Windows WebView2
- Tauri 2 的 Windows 构建依赖

安装依赖：

```powershell
npm install
```

前端类型检查：

```powershell
npx tsc --noEmit
```

核心测试：

```powershell
cargo test -p ccs2keil-core --manifest-path src-tauri/Cargo.toml
```

启动开发界面：

```powershell
.\dev.ps1
```

构建安装包：

```powershell
.\build.ps1
```

首次 Tauri 检查或构建会下载并编译较多 Rust 依赖。

## 已知边界

- 当前目标是普通 NoRTOS DriverLib 工程，不承诺 RTOS、Bootloader 或自定义安全启动工程无损转换。
- TI Clang 与 ArmClang 的专属参数、内联汇编、链接段和预编译库无法一一对应，工具会采用目标 SDK 官方配置并生成警告。
- 多 Target Keil 工程、复杂 CCS linked resources 和自定义构建步骤需要后续样例驱动扩展。
- 工程中同时存在 `.projectspec` 和 `.uvprojx` 时，需要选择更具体的子目录。

详细字段规则见 [docs/conversion-rules.md](docs/conversion-rules.md)。
