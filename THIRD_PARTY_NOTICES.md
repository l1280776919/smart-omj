# 第三方来源与修改说明

Smart OMJ 自 2026-10-14 起基于 OnmyojiAutoScript 的任务流程与模板改写，采用 GPL-3.0-only；许可证全文见根目录 LICENSE。分发衍生应用时应按 GPL 提供对应源码及此声明。

## OnmyojiAutoScript

- 作者：runhey 与上游贡献者。
- 来源：https://github.com/runhey/OnmyojiAutoScript
- 本次参考版本：04f3b2344d0bfdccda6e6f3e655d30b11ce478f8。
- 许可：GPL-3.0；保留上游 LICENSE 全文。
- 使用内容：Orochi 单人挑战行为、GeneralBattle 的准备/胜利/奖励/失败识别规则，11 张 PNG 模板。
- 模板复制自 `tasks/Orochi/o/` 和 `tasks/Component/GeneralBattle/gb/`，文件保持原名与原像素。搜索区域来自对应 `image.json`。
- 修改：用 Rust 状态机重新实现受限单人流程；不保留 Python 执行环境；增加阶段超时、取消、参数校验和有界重试。当前灰度匹配阈值统一为 0.90，与上游 OpenCV 彩色匹配不等价，尚待真实样本校准。

## OASX

- 来源：https://github.com/runhey/OASX ，GPL-3.0。
- 仅参考设备、任务、日志等功能划分，本次未复制 Dart 代码或界面资源。

## Android ADB

- 来源：开发者安装的 Android SDK platform-tools；官方来源 https://developer.android.com/tools/releases/platform-tools ，ADB 开源源码 https://android.googlesource.com/platform/packages/modules/adb/ 。
- `pnpm prepare:adb` 复制 adb.exe、AdbWinApi.dll、AdbWinUsbApi.dll，并原样保留该 SDK 的 NOTICE.txt 和 source.properties，生成各文件 SHA-256 清单。
- 本次本机构建使用 platform-tools 37.0.0，ADB 1.0.41（14910828）。二进制不提交 Git；构建者必须准备自己的 SDK 工具。
- 不复制其余 SDK 工具，不调用旧版 OAS 的安装文件。SDK 内开源组件受各自开源许可证约束，完整第三方许可随 NOTICE.txt 分发；公开发布前复核所用版本与源码提供要求。

## 框架与依赖

Tauri、Tokio、Vue、Vue Router、Pinia、Naive UI、image、serde、base64、uuid 等来源与版本固定在 Cargo.lock / pnpm-lock.yaml。公开发布前应生成锁定依赖的完整许可证清单与对应源码信息。

已引入OpenCV，具体来源和许可见下节。ONNX Runtime与OCR模型尚未引入。

## OAS配置兼容测试样本

`src-tauri/tests/fixtures/oas-template-subset.json` 摘自 OnmyojiAutoScript 提交 `04f3b2344d0bfdccda6e6f3e655d30b11ce478f8` 的 `config/template.json`，仅保留 script 与 orochi 节点并重新排版，遵循上游 GPL-3.0 许可。

## OpenCV原生运行库与Rust绑定

- OpenCV 4.12.0: https://github.com/opencv/opencv/releases/tag/4.12.0 ，Apache-2.0。官方Windows SDK SHA256：`b753b14d880b9bc8d89d6acd3b665c040baec0211078435432fcae117db707af`。
- opencv Rust绑定0.101.1: https://github.com/twistedfall/opencv-rust ，MIT；锁定版本见Cargo.lock。
- world二进制内含第三方组件；准备脚本复制OpenCV LICENSE/COPYRIGHT、源码3rdparty许可证，以及对应IPP包的EULA和third-party-programs清单到随包resources/opencv。IPP源版本由OpenCV 4.12.0的ippicv.cmake固定，档案SHA256为`f87c59573cece75260de2bfe1ecfa305c931a420feb57e03310ae06bb0e8f3c7`。
- Microsoft VC++运行库来自Visual Studio Build Tools的x64可再分发CRT目录，不从System32收集DLL。适用Microsoft再分发条款，Redist.txt和ThirdPartyNotices随资源保存；具体版本、来源与DLL哈希见resources/opencv/manifest.json。

本项目仅使用OpenCV core/imgproc进行灰度模板匹配；未分发官方包内的Python组件、FFmpeg插件、调试DLL或SDK头文件。用户运行不依赖Python、LLVM或Visual Studio。开发构建仍需要LLVM/libclang与MSVC工具链。
