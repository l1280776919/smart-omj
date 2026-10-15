# Smart OMJ

基于 **Tauri 2 + Vue 3 + TypeScript + Rust** 的阴阳师自动化桌面应用，当前优先支持 **Windows x64 + MuMu 模拟器**。

项目包含场景识别、独立按钮识别工作台和御魂任务编排。截图、推理、点击及流程控制在 Rust 中执行；Python 用于素材处理、训练和模型导出，不作为游戏自动化的常驻服务。

> **当前状态 · 2026-10-16：** 已在 RTX 5070 / MuMu / 1920×1080 环境验证魂十一「悲鸣」单人循环、CUDA 推理和 MuMu 原生截图。项目仍处于开发验证阶段，模型权重和本机素材未随源码发布，尚未完成干净 Windows 环境的安装包验收。

## 已实现功能

| 模块 | 当前能力 |
| --- | --- |
| 设备管理 | ADB 设备发现、连接、截图预览与操作诊断 |
| 场景识别 | 庭院、探索、御魂入口、八岐大蛇、准备、战斗、结算、失败，共 8 类 |
| 场景训练 | 录像抽帧、素材筛选、训练/验证划分、训练与 ONNX 导出、模型启用和识别测试 |
| 按钮工作台 | 图片/录像素材采集、预标注、框选修正、类别管理、逐图或批量划分、训练、模型评估与坐标测试 |
| 御魂流程 | 壹层实验流程与魂十一「悲鸣」循环；轮数限制、运行超时、停止和失败留档 |
| 任务调度 | 持久化预约、周期队列、优先级及中断恢复处理 |
| 配置与日志 | 版本化配置、旧 OAS 配置预览导入、实时日志、逐帧耗时及运行记录 |

**按钮识别独立于场景识别。** 按钮模型输出类别与边界框，自动化编排再结合场景、标题和锁阵容等证据决定动作。不能把单个按钮被识别出来等同于允许点击。

## 魂十一 · 悲鸣

### 当前验证范围

先在游戏中选好 **八岐大蛇 → 悲鸣**，停留在挑战页，锁定预设阵容并开启自动战斗。在应用中选择设备和「魂十一·悲鸣」，核对启动提示后运行。

循环大致为：

```text
核验悲鸣挑战页 → 挑战 → 准备/战斗 → 奖励结算 → 返回挑战页
       ↑                                      │
       └──────── 未达到设定轮数 ───────────────┘
```

- 挑战前同时核验八岐大蛇场景、悲鸣标题、阵容锁定和挑战按钮，并排除冲突标记。
- 连续两帧确认状态；自己发起挑战且观察到战斗、结算和返回后，才计为完成一轮。
- 点击前随机等待 **120–280 ms**；每轴偏移最多为按钮宽高的 **10%**，且不超过 **12 个原图像素**，限定在框中央区域。
- 随机等待可以取消；等待后再次检查任务时限、截图有效期和坐标。超过 **1500 ms** 的截图不用于点击。
- 挑战不盲目重试；准备和结算重试有上限，未知页面、失败或超时会停止并保留诊断记录。

**尚未支持自动滚动选择悲鸣、自动配队或组队刷图。** 当前魂十一验证限定横屏 16:9；其他分辨率、模拟器版本及异常场景仍需补充测试。实验流程不代表已覆盖上游全部任务。

详细规则、实测和限制见 [魂十一流程](docs/soul11-loop.md)。

## 截图与识别性能

魂十一循环优先使用 MuMu 的 `external_renderer_ipc.dll` 直接取图。启动时通过 Android boot UUID 核对实例，再与 ADB 截图比较尺寸和内容；验证失败或接口异常时回退 ADB raw / PNG。点击仍通过所选设备的 ADB 连接发送。

图像缩放、RGBA 转换与垂直翻转使用 OpenCV。按钮和场景使用 ONNX Runtime，支持 CUDA；GPU 初始化失败会显示 CPU 回退原因。

本机挑战页稳定帧中位数，排除首次模型冷启动及点击派发帧：

| 耗时 | 原 ADB raw 链路 | 优化后 |
| --- | ---: | ---: |
| 截图合计，含 PNG 编码 | 367 ms | 57 ms |
| 按钮预处理 | 89 ms | 8 ms |
| 场景识别 | 64 ms | 3 ms |
| 整帧处理 | 534 ms | 84 ms |

优化后原生取帧及像素转换约 **8 ms**，PNG 留档编码约 **49 ms**。这些数据来自加入随机等待前的性能测试；随机等待另外计入点击帧总耗时。两轮实测约 **52.2 秒**，战斗和动画仍决定大部分刷图时间，不能把单帧提速倍数当作刷图速度提升。

日志记录截图后端、取帧/编码、预处理、推理、场景、模板、决策、点击、存图和总耗时，以及随机等待和实际点击坐标。**手动单次观察暂未切换到 IPC 截图路径。**

## 按钮工作台

入口：**场景训练 → 按钮识别与坐标**。包含「标注图片」「数据集与类别」「训练模型」「识别测试」四个页签。

1. 导入图片、采集截图或从录像筛选具有差异的帧。
2. 使用已有模型预标注，检查并修正错框、漏框和类别；预标注结果不会直接作为已审核真值。
3. 为图片设置训练集或验证集，并保存完整标注与审核状态。
4. 训练后查看整体及各类指标，在实际截图上检查识别框和点击点，再决定是否启用。

同一真实来源批次可以跨训练集和验证集，只提示相似素材可能抬高分数；完全相同图片跨集合会阻止训练。修改单张图片用途不会改动同批其他图片。类别修改有备份，删除类别保留原图，并让受影响图片重新审核。

“可训练”只表示满足启动条件，不代表数据充足或模型可靠。训练结果不会自动替换当前启用模型。

`soulbtn` 表示探索页**底部功能栏的御魂图标**，不要与画面中的竖牌混标。类别定义和命名边界见 [按钮类别表](docs/button-classes.md)。

## 开发运行

### 环境要求

- Windows x64、WebView2、MuMu，以及 Android SDK platform-tools。
- Node.js 24、pnpm **11.22.0**、Rust MSVC 工具链、Microsoft C++ Build Tools。
- LLVM / libclang；默认配置路径为 `C:\Program Files\LLVM\bin`，非默认安装需调整 `.cargo/config.toml`。
- 训练需 Python 虚拟环境 `training/.venv`；本机验证使用 Python 3.12。
- GPU 验证组合：RTX 5070、PyTorch 2.14.1 + CUDA 13.0、ONNX Runtime GPU 1.31.0、cuDNN 9。其他环境需要单独验证。

### 安装源码依赖

在 PowerShell 中执行：

```powershell
git clone https://github.com/l1280776919/smart-omj.git
cd smart-omj
pnpm install --frozen-lockfile
pnpm prepare:adb
pnpm prepare:opencv
```

`prepare:adb` 默认从 PATH 查找 `adb.exe`，并要求 platform-tools 目录包含运行库、`NOTICE.txt` 和 `source.properties`。也可显式指定目录：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/stage-adb.ps1 -PlatformToolsDirectory "C:\Android\platform-tools"
```

### 训练与 GPU 运行库

以下命令用于新建 GPU 训练环境，不要同时安装 CPU 版 `onnxruntime`：

```powershell
py -3.12 -m venv training/.venv
training/.venv/Scripts/python.exe -m pip install -r training/requirements-gpu.txt --extra-index-url https://download.pytorch.org/whl/cu130
pnpm prepare:gpu
```

GPU 准备脚本从虚拟环境复制 ONNX Runtime、CUDA 和 cuDNN 依赖。训练工作台当前依赖源码和该虚拟环境，尚未作为独立 Python 训练环境随安装包分发。CPU 训练依赖另见 `training/requirements.txt`。

### 模型资源与启动

**源码仓库不包含模型权重或完整运行库。** 克隆后需要自行训练或准备与配置一致的模型，不能仅修改文件名就替换任意模型：

| 位置 | 用途 |
| --- | --- |
| `src-tauri/resources/detector/soul1.onnx` | 内置检测模型，类别顺序对应 `model.json` |
| `src-tauri/resources/scene/best.onnx` | 内置场景模型，类别及预处理对应 `scene_model.json` |
| `src-tauri/resources/gpu/` | GPU ONNX Runtime 与 CUDA/cuDNN 运行库 |
| `src-tauri/resources/detector/onnxruntime.dll` | 未使用 GPU 运行库时的 ONNX Runtime 加载位置 |
| 应用数据目录的 `button-studio/models/` | 工作台训练、验证并启用的自定义按钮模型 |

目前没有一键下载完整模型资源包。训练与导出入口见 `training/`、[场景工作台](docs/scene-workbench.md) 和 [按钮工作台](docs/button-workbench.md)。魂十一读取已启用按钮模型，场景判断使用内置场景模型。

资源准备完成后启动桌面应用：

```powershell
pnpm tauri dev
```

仅调试前端可以运行 `pnpm dev`，设备、训练和自动化等功能需要 Tauri 后端。

## 检查与构建

```powershell
pnpm build
pnpm test
pnpm test:rust
cargo build --manifest-path src-tauri/Cargo.toml
```

完整 Rust 测试中有部分依赖本机模型、原生运行库或未上传的截图样本，纯源码克隆不能直接复现全部测试。已提交的 GitHub Actions 工作流仍需补充测试资源和运行库准备；本机通过不代表云端 CI 已通过。

开发配置已设置 `[profile.dev] incremental = false`，规避本机反复出现的 MSVC `LNK2001/LNK2019` 匿名 LLVM 符号错误，无需每次临时设置环境变量。

准备全部运行资源后可尝试构建安装包：

```powershell
pnpm tauri build
```

干净环境安装、资源完整性及分发验收仍待完成。

## 数据与目录

Windows 应用数据位于 `%APPDATA%\com.smartomj.desktop\`，其中包含配置、运行记录，以及 `button-studio/annotations.json`、原图、标注历史、模型和启用记录。不要用 Git 工作区替代这些数据的备份。

```text
src/                       Vue 页面、组件、服务和状态
src-tauri/src/device/       ADB 与 MuMu 原生截图
src-tauri/src/vision/       检测、场景分类、模板匹配与坐标还原
src-tauri/src/tasks/        页面判断与任务状态机
src-tauri/src/closed_loop.rs 截图、识别、决策、点击及留档
src-tauri/src/*_studio.rs   工作台后端
src-tauri/resources/        模型配置与运行资源
training/                  素材处理、训练、评估和导出脚本
scripts/                   ADB、OpenCV、GPU 运行库准备
docs/                     功能规格和验证记录
```

截图、录像、训练集、模型权重、实验运行记录和大型二进制已加入 Git 忽略规则，保留在本机。源码中的小型流程模板与类别配置随仓库提交。

## 文档与后续工作

- [魂十一流程与性能记录](docs/soul11-loop.md)
- [独立按钮识别工作台](docs/button-workbench.md)
- [按钮类别表](docs/button-classes.md)
- [场景训练工作台](docs/scene-workbench.md)
- [OpenCV 开发与打包](docs/native-opencv.md)
- [自适应分辨率诊断](docs/adaptive-resolution.md)
- [IPC 接口](docs/ipc-contract.md)
- [历史迁移记录](docs/migration-status.md)

后续重点是补充独立采集的验证素材、覆盖更多异常页面与设备环境、完善可复现的模型/测试资源准备，并完成安装包验收。OCR、自动配队、组队及更多游戏任务不在当前已验证范围内。

## 来源与许可

任务行为和部分模板参考 [OnmyojiAutoScript](https://github.com/runhey/OnmyojiAutoScript)，界面功能划分参考 [OASX](https://github.com/runhey/OASX)。本项目采用 **GPL-3.0-only**；模板、原生库等第三方来源及说明见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) 和 [LICENSE](LICENSE)。
