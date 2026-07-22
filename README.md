# TI工具箱

TI工具箱是一款面向 TI MSPM0 的 Windows 桌面工具，支持 CCS 与 Keil 工程双向转换，也可以一键配置 Keil 的 TI SysConfig 环境。

## 下载与安装

请从 [GitHub Releases](https://github.com/zuoliangyu/ti-toolbox/releases) 下载最新版：

各版本变化请查看 [更新日志](CHANGELOG.md)。

- 安装版：文件名包含 `setup`，安装后可以自动检查并安装更新。
- 便携版：文件名包含 `portable`，解压后直接运行；更新时需要重新下载新版。

Windows 首次运行未签名程序时可能显示 SmartScreen 提示，请确认下载来源为本仓库。

## 使用前准备

根据需要安装以下官方工具和资源：

- [MSPM0G1X0X_G3X0X_DFP Pack](https://www.keil.arm.com/packs/mspm0g1x0x_g3x0x_dfp-texasinstruments/versions/)：CCS 转 Keil 时需要。
- [MSPM0 SDK](https://www.ti.com.cn/tool/cn/MSPM0-SDK)：工程双向转换都需要。
- [Code Composer Studio（CCS）](https://www.ti.com/tool/CCSTUDIO)：打开 CCS 工程以及执行 CCS 构建验证时需要。
- [SysConfig](https://www.ti.com/tool/SYSCONFIG)：工程包含 `.syscfg` 或需要配置 Keil SysConfig 菜单时需要。
- Keil MDK：打开 Keil 工程以及执行 Keil 构建验证时需要。

> [!IMPORTANT]
> 使用工程互转功能时，源工程和目标环境的 SysConfig 版本必须一致。例如源工程使用 SysConfig `1.26.2` 生成配置，转换和接收工程的电脑也应使用 `1.26.2`。版本不一致可能导致生成文件、外设配置或构建结果不兼容。

## 工程转换

1. 打开“工程转换”。
2. 选择 CCS 或 Keil 工程目录，点击“解析工程”。
3. 等待工具自动检测 MSPM0 SDK、Pack、CCS、Keil 和 SysConfig；未识别到的路径可以手动选择。
4. 建议先执行“一键构建验证”，确认源工程本身能够正常编译。
5. 选择一个空目录作为输出位置，然后开始转换。
6. 转换完成后，工具会在已安装目标 IDE 的情况下继续执行目标工程构建验证。

### 所需环境

| 转换方向 | 必需 | 建议安装 |
|---|---|---|
| CCS → Keil | MSPM0 SDK、支持目标芯片的 Pack | CCS、Keil、与源工程一致版本的 SysConfig |
| Keil → CCS | MSPM0 SDK | Keil、CCS、与源工程一致版本的 SysConfig |

CCS 转 Keil 时，如果工程使用了 SysConfig，请先在 CCS 中构建一次，确保工程内存在 `ti_msp_dl_config.c` 和 `ti_msp_dl_config.h`。缺少这些生成文件时，工具会停止转换并提示处理方法。

转换结果必须写入不存在或为空的目录，工具不会覆盖源工程。只有明确选择“原工程直接构建”时，CCS 才会更新源工程的构建产物。

## Keil TI 环境配置

这个功能不需要先选择工程：

1. 打开“Keil TI 环境配置”。
2. 自动检测或手动选择 MSPM0 SDK、Keil 和带图形界面的 SysConfig。
3. Pack 请先通过上方官方链接下载并安装到 Keil。
4. 点击“一键配置 Keil TI 环境”，确认后由工具更新 SDK 配置和当前用户的 Keil Tools 菜单。

工具修改 SDK 配置前会创建 `.ti-toolbox.bak` 备份，并且不会覆盖其他自定义 Keil Tools 项目。

## 构建验证说明

- CCS 验证默认复制工程到临时目录后执行，不修改源工程。
- “原工程直接构建”会执行 CCS Clean + Full Build，并更新 `Debug`、`Release` 和 SysConfig 生成文件。
- Keil 验证会调用 `UV4.exe` 构建工程，并根据构建日志判断是否成功。
- 未安装源 IDE 时仍可在确认风险后转换；如果实际构建已经失败，需要先修复工程错误。

## 当前支持范围

工具主要面向普通 NoRTOS DriverLib 工程，并已使用 MSPM0 SDK `2.10.00.04`、MSPM0G1X0X_G3X0X_DFP `1.3.1` 和 MSPM0G3507 工程进行验证。

RTOS、Bootloader、自定义安全启动、多 Target Keil 工程、复杂 CCS linked resources、专用预编译库或自定义构建步骤可能需要手动调整转换结果。
