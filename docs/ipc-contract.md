# IPC 契约 v1

所有 IPC 只在 Tauri 主窗口使用。前端不得执行 Shell，命令不接受可执行文件路径或输出路径。Rust 为权威状态，前端只投影状态。

| Command | 参数 | 结果 / 行为 |
| --- | --- | --- |
| get_snapshot | 无 | snapshot + Rust 定义的表单 limits |
| save_config | config | 校验 schema、设备标识、端口、数值范围后原子保存 |
| list_devices | 无 | serial、status、description；保留 offline/unauthorized |
| connect_device | endpoint | 只允许 IPv4 回环地址和非零端口 |
| capture_screen | serial | PNG data URL + width/height；预览可显示非目标分辨率 |
| tap_device | serial,x,y,expectedSize | 原始像素；核对预览尺寸、真实边界，派发前再核对尺寸，返回新截图 |
| swipe_device | serial,points,durationMs,expectedSize | 原始像素；同样核对预览尺寸和真实边界；时长限制 100–3000ms |
| diagnose_screen | serial | ROI 模板命中、分数、位置和耗时；OCR 明确返回未验证说明 |
| click_element | serial,element | element 只允许 challenge/ready/locked；重新截图识别后点击，再截图识别；返回 target_disappeared 和 verification（disappeared/still_present/unconfirmed），不自动重试 |
| start_task | 无 | 使用已保存配置，返回 run_id；后台执行御魂实验任务 |
| stop_task | 无 | 取消并进入 stopping；工作线程释放设备后进入 stopped |

参数采用 Tauri 的 camelCase 命名，Rust 返回 JSON 字段采用 snake_case。命令错误返回可显示的字符串；不会将失败转换为模拟成功。

## 状态与事件

`engine:snapshot` 携带完整 snapshot：sequence、config、run、最近 400 条 logs、adb_source、data_directory。

run 含 run_id、device_id、status、stage、completed_rounds、started_at_ms、ended_at_ms、message、failure_screenshot。status 为 running/stopping/completed/stopped/failed/interrupted。

每个日志含 sequence、timestamp_ms、level、device_id、run_id、message。sequence 在单个应用进程内递增。重新连接时重置前端投影、先订阅事件再查询完整状态，忽略 sequence 较旧或重复的快照。该顺序处理查询期间发生的事件，防止恢复旧状态。

首版每阶段或操作发一次完整快照，流量有限；尚未实现 tracing 批量增量推送。日志磁盘文件每 5 MiB 轮换一次，保留当前和 previous 两份；界面采用虚拟列表。

## 操作与取消

单个全局异步锁覆盖连接、诊断、手动操作和完整任务。任务运行时其他设备操作及配置保存直接拒绝，不进入隐式等待队列。停止命令绕过操作锁。

CancelSignal 用互斥门保护子进程派发与取消：取消生效后禁止派发新进程。正在执行的 ADB 客户端在取消/12 秒超时后终止并回收。只清理当前子进程，不结束共享 ADB server 或用户其他程序。阻塞图像识别按行检查取消并限制总识别预算；工作线程结束前不释放操作锁。

## 本地存储

通过 Tauri app_data_dir 定位，config/config.json 为业务配置，config/last-run.json 为运行检查点，logs/engine.jsonl 为日志，screenshots/latest.png 为当前运行截图。失败时另存 failure-<run_id>.png。崩溃后 running/stopping 检查点标记为 interrupted，不恢复游戏操作。

配置解析错误保留原文件并拒绝启动；不会静默覆盖。JSON 保存使用同目录唯一临时文件、sync_all 和 rename 替换。旧 OAS JSON 导入与字段迁移尚未实现。

诊断增加 viewport 与 alignment_confident。匹配框属于 working_size 识别画布，自动点击通过 viewport 映射回设备坐标，详见 [自适应策略](adaptive-resolution.md)。

## 一次性任务队列

Snapshot 新增 `queue: { enabled, entries }`。每条记录包含 `id/config/priority/due_at_ms/status/run_id/message`，时间为Unix毫秒，状态为pending/running/completed/failed/stopped/interrupted。

- `enqueue_task({ config, priority, dueAtMs })`：校验配置和设备，优先级0–100，预约不超过未来366天；过去时间表示已到期。保存独立配置，不修改当前配置。
- `set_queue_enabled({ enabled })`：显式启用或暂停派发，暂停不停止当前任务。
- `remove_queue_entry({ id })`：删除非运行中记录；运行中须先停止。
- `stop_task()`：取消当前任务并暂停后续派发。

上述命令成功后通过既有 `engine:snapshot` 发布完整状态；入队不会自动启用。应用重启一律暂停，运行中记录标记interrupted。队列全局串行，不抢占，暂不支持周期重复或系统唤醒。

## 旧 OAS 配置导入

- `preview_oas_config({ content })`：只读分析，返回 `base/config/mappings/warnings/errors/ignored`。mappings含source/target/before/after；ignored为未迁移JSON Pointer。输入限1 MiB、4096字段，只接受已知OAS结构。
- `import_oas_config({ content, expectedBase })`：持有设备互斥锁且要求队列暂停，比对当前配置与预览base，重新解析并校验。存在任何错误即拒绝。先备份原始文本、导入前配置与报告，再替换当前配置，返回备份目录并发布snapshot。

前端不传文件路径；后端不写入旧安装目录。选中文件仅预览，确认后才导入。既有队列配置快照不改变。

## 固定间隔周期扩展

`enqueue_task` 可额外传 `repeat: { interval_minutes, total_runs } | null`，省略或null表示单次。间隔1–10080分钟，总次数2–100（含首次）。QueueEntry增加 `repeat`、`completed_runs` 和 `last_run`；旧持久化文件缺省为null/0/null。

成功结束后累计次数加一，尚未达到总次数时设置pending与下一次due_at_ms（ended_at_ms + interval_minutes × 60000），保留last_run与上次run_id。开始下一次时生成新run_id。失败或停止终止周期并暂停队列，手动重新启用不会重试终止项。只保留最近一次结果，完整日志仍写JSONL。

暂停不会取消当前任务，也不会因任务完成而恢复队列。应用重启总是暂停；不自动补跑错过的间隔或唤醒系统。输入与载入记录均校验周期参数。
