# Smart OMJ

基于 Tauri 2、Vue 3、TypeScript 和 Rust 的阴阳师一体化桌面应用工程。

当前状态：已初始化项目骨架，尚未接入 OAS 引擎、任务配置或游戏控制。

## 工作区

- `../OnmyojiAutoScript`：上游 Python 自动化引擎，保留独立 Git 历史。
- `../OASX`：上游 Flutter 控制界面，供功能和接口迁移参考。
- 本仓库：新的 Tauri 桌面应用，独立版本管理。

## 开发

需要 Node.js、Rust MSVC 工具链、Microsoft C++ Build Tools 和 WebView2。

```powershell
pnpm install
pnpm tauri dev
```

仅检查前端：`pnpm build`。
检查 Rust：`cargo check --manifest-path src-tauri/Cargo.toml`。
生成桌面安装包：`pnpm tauri build`。

## 整合方向

交付一个安装包和一个启动入口，由 Tauri 管理内置 Python 引擎的启动、健康检查和退出。用户不需要填写 IP、端口或单独启动服务。

第一阶段保留现有 Python 业务逻辑及本地 API，前端通过桌面层接入。后续按需要迁移子进程通信，不在第一阶段重写游戏任务。

待实现：引擎打包与生命周期管理、配置和日志数据目录、任务配置界面、模拟器发现、实时日志与运行状态、托盘和更新机制。

开发时不要依赖某台电脑上的绝对 Python 安装路径。安装到电脑的 `D:\Applications\OnmyojiAutoScript` 是现有可用版本，不属于本工程。

## 上游来源

- https://github.com/runhey/OnmyojiAutoScript
- https://github.com/runhey/OASX

本仓库目前为新建骨架。后续引入上游代码时，需要保留其版权声明并遵守对应许可证（OAS 为 GPL-3.0）；发布前明确整合后的许可证及源码分发方式。


包管理器固定为 pnpm 11.22.0，项目 .npmrc 使用 https://registry.npmmirror.com 国内镜像。提交 pnpm-lock.yaml，不使用 package-lock.json。
