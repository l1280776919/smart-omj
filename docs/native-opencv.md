# OpenCV开发与打包

Windows x64需要MSVC C++ Build Tools、LLVM（包含libclang.dll和clang.exe）。安装在默认目录时执行：

```powershell
pnpm prepare:adb
pnpm prepare:opencv
cargo test --manifest-path src-tauri/Cargo.toml --lib
pnpm tauri dev
pnpm tauri build --bundles nsis
```

`.cargo/config.toml` 为OpenCV头文件和链接库指定项目内路径，固定使用opencv_world4120发布库，调试构建也不依赖调试CRT。LLVM默认位置为 `C:/Program Files/LLVM/bin`；非默认安装需在调用Cargo前设置 `LIBCLANG_PATH` 和 `CLANG_PATH` 覆盖默认值，准备脚本可传 `-LlvmDirectory` 校验相应目录。

准备脚本下载约187MB官方SDK及约29MB的IPP包（仅提取许可证），校验固定SHA256。SDK只放在忽略的 `src-tauri/target/native-deps`，不会进安装包。DLL放在 `src-tauri/native-runtime`，许可证及版本/哈希放在 `src-tauri/resources/opencv`。若下载哈希不符，脚本报错并保留文件，检查网络或手动移除坏下载后重试。`cargo clean` 后须重新执行准备脚本。

安装包把OpenCV和四个VC++运行库DLL放在主程序同目录，许可证随resources分发。build.rs同样为debug/release及其deps/examples目录准备运行库。运行不需要系统PATH中的OpenCV/LLVM，不包含Python、FFmpeg插件或SDK。

主识别流程使用灰度TM_CCOEFF_NORMED，常量模板无效；识别前后检查取消，总诊断预算8秒。OpenCV单次调用内无法取消；ROI约束限制输入规模。灰度阈值和原OAS彩色行为不等价，仍需真实截图校准。Windows N或裁剪媒体组件的系统可能缺少MF依赖，需系统Media Feature Pack；这部分未干净安装验证。

## 验证记录

2026-10-15：37项Rust测试通过，包含多分辨率、旋转/黑边/锚定、歧义禁点与原Rust匹配器对照。1920×1080设备只读诊断约1620ms，当前界面无可靠模板命中，未点击。清空开发工具PATH后示例仍成功。安装后的原生UI与真实游戏循环另行验收，OCR不在本次后端实现内。
