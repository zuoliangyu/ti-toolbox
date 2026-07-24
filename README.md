# TI工具箱

TI工具箱是一款面向 MSPM0 开发者的 Windows 桌面工具，可在 CCS 和 Keil 工程之间双向转换，并自动检测开发环境、验证转换前后的构建结果。它还可以将 TI SysConfig 一键配置到 Keil Tools 菜单。

[下载最新版](https://github.com/zuoliangyu/ti-toolbox/releases/latest) · [查看更新日志](CHANGELOG.md)

## 主要功能

- **CCS ↔ Keil 双向转换**：识别 CCS 和 Keil 工程，并根据所选 MSPM0 SDK 生成目标工程。
- **自动检测开发环境**：查找 MSPM0 SDK、CMSIS Pack、CCS、Keil 和 SysConfig，也支持手动指定路径。
- **真实工具链验证**：转换前后可调用 CCS 或 Keil 构建工程，并实时显示构建日志。
- **Keil SysConfig 一键配置**：自动更新 SDK 配置和当前用户的 Keil Tools 菜单，修改前创建备份。
- **安装版自动更新**：安装版可在应用内检查、下载并安装新版本。

## 下载与安装

前往 [GitHub Releases](https://github.com/zuoliangyu/ti-toolbox/releases) 下载最新版：

- **安装版**：文件名包含 `setup`，支持应用内自动更新。
- **便携版**：文件名包含 `portable`，解压后直接运行，升级时需重新下载。

程序暂未进行代码签名。Windows 首次运行时如果出现 SmartScreen 提示，请先确认安装包来自本仓库。

## 快速开始

1. 打开左侧的“工程转换”，选择 CCS 或 Keil 工程目录。
2. 点击“解析工程”，确认自动检测到的 SDK、Pack、CCS、Keil 和 SysConfig 路径。
3. 建议先执行“构建验证”，确认源工程本身可以正常编译。
4. 选择一个不存在或内容为空的输出目录，然后开始转换。
5. 如果已安装目标 IDE，工具会在转换完成后继续验证生成的工程。

工具不会覆盖源工程，也不会把转换结果写入非空目录。只有主动选择“原工程直接构建”时，CCS 才会更新源工程的 `Debug`、`Release` 和 SysConfig 生成文件。

## 环境要求

| 转换方向 | 转换必需 | 构建验证需要 |
| --- | --- | --- |
| CCS → Keil | [MSPM0 SDK](https://www.ti.com.cn/tool/cn/MSPM0-SDK)、支持目标芯片的 [MSPM0 CMSIS Pack](https://www.keil.arm.com/packs/mspm0g1x0x_g3x0x_dfp-texasinstruments/versions/) | 源工程验证需要 [CCS](https://www.ti.com/tool/CCSTUDIO)，目标工程验证需要 Keil MDK |
| Keil → CCS | [MSPM0 SDK](https://www.ti.com.cn/tool/cn/MSPM0-SDK) | 源工程验证需要 Keil MDK，目标工程验证需要 [CCS](https://www.ti.com/tool/CCSTUDIO) |

工程包含 `.syscfg` 时，还需要与原工程一致版本的 [SysConfig](https://www.ti.com/tool/SYSCONFIG)。例如原工程使用 SysConfig `1.26.2` 生成配置，转换和后续开发也应使用 `1.26.2`，否则生成文件、外设配置或构建结果可能不一致。

CCS → Keil 时，如果工程使用了 SysConfig，请先在 CCS 中构建一次，确保工程内存在 `ti_msp_dl_config.c` 和 `ti_msp_dl_config.h`。缺少这些生成文件时，工具会停止转换并给出处理提示。

## 配置 Keil TI SysConfig

此功能不需要先选择工程，但需要 MSPM0 SDK、Keil MDK 和带图形界面的 SysConfig：

1. 打开左侧的“Keil TI 环境配置”。
2. 确认工具自动检测到的 SDK、Keil 和 SysConfig 路径，未识别时可手动选择。
3. 点击“一键配置 Keil TI 环境”，确认后等待配置完成。

工具会更新 SDK 中的 Keil SysConfig 配置，并写入当前用户的 Keil Tools 菜单。修改 SDK 文件前会创建 `.ti-toolbox.bak` 备份，已有的其他 Keil Tools 项目不会被覆盖。

## 构建验证

- CCS 验证默认复制工程到临时目录，再执行 Clean、Full Build 和严格链接，不修改源工程。
- Keil 验证会调用 `UV4.exe` 构建工程，并根据构建日志判断结果。
- 未安装源 IDE 时仍可在确认风险后执行转换，但无法验证源工程是否本身存在编译或链接错误。

## 当前支持范围

目前主要支持普通 NoRTOS DriverLib 工程，已使用以下环境验证：

- MSPM0 SDK `2.10.00.04`
- MSPM0G1X0X_G3X0X_DFP `1.3.1`
- MSPM0G3507 工程

RTOS、Bootloader、自定义安全启动、多 Target Keil 工程、复杂 CCS linked resources、专用预编译库或自定义构建步骤可能需要手动调整转换结果。

## 从源码运行

请先准备 Node.js、Rust stable，以及 [Tauri 2 的 Windows 开发环境](https://v2.tauri.app/start/prerequisites/)。在 PowerShell 中执行：

```powershell
# 开发模式
.\dev.ps1

# 构建 Windows 安装包
.\build.ps1
```

脚本会在首次运行时自动安装 npm 依赖。

## 作者与反馈

- 作者：[左岚](https://space.bilibili.com/27619688)
- 仓库：[zuoliangyu/ti-toolbox](https://github.com/zuoliangyu/ti-toolbox)
- 问题反馈：[GitHub Issues](https://github.com/zuoliangyu/ti-toolbox/issues)
