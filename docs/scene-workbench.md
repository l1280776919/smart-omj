# 场景识别与训练工作台

在项目根目录运行 `pnpm tauri dev`，进入左侧「场景训练」。普通浏览器仅预览 UI，不能调用本地训练和 ADB。

## 验证已内置的模型

1. 在「设备管理」连接 MuMu 并选择设备。
2. 进入「场景训练 → 场景识别」，点击「截图并识别」。
3. 核对截图、预测类别和各类别置信度。

内置模型支持 `yard`（庭院）、`explore`（探索）、`mitama`（御魂入口）、`orochi`（八岐大蛇）、`ready`（战斗准备）、`fight`（战斗中）、`fail`（挑战失败）、`results`（战斗结算）。Rust 使用 ONNX Runtime 推理，无需 Python。置信度低于 80% 时显示「暂不确定」；高于阈值也不能证明画面一定属于已有类别。此功能用于观察验证，不会自动点击或替换现有任务的判断逻辑。

## 训练流程

1. 「打开录像目录」，在 `training/videos/<类别>/` 中放入视频。同一类别尽量录制多个独立批次。
2. 设置抽帧间隔，默认 0.5 秒，点击「抽取图片素材」。输出保留原始分辨率。
3. 点击「检查图片」。分页预览中可排除错误场景或过渡帧，支持恢复，原图不删除。
4. 设置训练轮数，点击「开始训练」。自动生成数据集、训练、评估、导出 ONNX，并检查与 PyTorch 输出的一致性。
5. 检查验证指标和日志，点击「启用此模型」，然后回到「场景识别」实机核对。

多个视频按完整视频留出验证；仅有一个视频时，按时间划分并留出约 4 秒间隔。后一种仅为同源初步验证，不能作为泛化准确率。过短或有效样本不足会明确报错，不偷偷使用训练集代替验证集。

## 存储与运行

- `training/scene-jobs/<任务编号>/`：每次任务的独立图片、划分清单、进度、日志、训练结果。
- `training/scene-prepared.json`：最新成功抽帧的批次。
- `training/scene-last-job.json`：最近任务的位置，用于重启后找回成功结果或提示中断。
- 应用数据目录中的 `scene-models/` 与 `scene-active.json`：复制并验证后的启用模型。切换使用原子配置写入，不覆盖内置模型。
- `src-tauri/resources/scene/`：内置 8 类场景模型及 Rust 测试样例。

训练需要本项目 `training/.venv/Scripts/python.exe` 和 `scene_workbench.py`，复用现有环境；默认自动优先 CUDA，也可选择强制 GPU 或 CPU；强制 GPU 不可用会报错。训练进度显示实际设备；场景识别和视觉观察显示 CUDA/CPU 及回退原因。此版本不分发 Python 环境，移动到没有源码/虚拟环境的电脑后训练按钮会不可用，内置识别仍可运行。

训练可取消，切换页面不会中断；正常关闭应用会终止任务。异常进程终止不保证子进程清理，重启后不会自动恢复或启用模型。重新抽帧会建立新批次，不继承上批的排除标记。首次缺少预训练权重时 Ultralytics 会下载权重。

预处理约定：RGB，双线性等比例缩放到 224×224 内，居中补 RGB(114,114,114)，float32 NCHW，除以 255，无额外标准化。实际类别顺序以 `scene_model.json` 为准。

## GPU 环境（Windows / NVIDIA）

当前验证组合：RTX 5070、PyTorch 2.14.1 + CUDA 13.0、ONNX Runtime GPU 1.31.0、cuDNN 9。

```powershell
training/.venv/Scripts/python.exe -m pip uninstall -y onnxruntime
training/.venv/Scripts/python.exe -m pip install -r training/requirements-gpu.txt --extra-index-url https://download.pytorch.org/whl/cu130
pnpm prepare:gpu
cargo test --manifest-path src-tauri/Cargo.toml --lib staged_gpu_runs_scene_and_detector -- --ignored
```

`prepare:gpu` 将运行库复制到 `src-tauri/resources/gpu/`，包含于 Tauri 资源包，约 1.9 GB；识别无需启动 Python。这些 DLL 不进 Git，在构建机器上重新准备。CUDA 加载失败会回退 CPU 并报告原因。导出和 PyTorch/ONNX 一致性检查使用 CPU，以便保持导出结果可移植；训练与验证使用所选设备。

2026-10-15 的 8 类试训抽取 342 张图片，GPU 训练后留出集 top-1 为 100%，ONNX 最大误差约 1.19e-7。失败类仅有 2 张训练、2 张验证图片，结算类为 5/2；部分类别采用同录像时间留出，不能当作独立泛化准确率。请补充独立录像并审核过渡帧。
