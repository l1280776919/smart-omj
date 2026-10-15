use crate::{
    app::{AppState, RunState},
    config::{self, Config},
    device::CancelSignal,
    engine, telemetry,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::{AppHandle, Manager, State};

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Queue {
    pub enabled: bool,
    pub entries: Vec<Entry>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Repeat {
    pub interval_minutes: u32,
    pub total_runs: u32,
}
impl Repeat {
    fn validate(&self) -> Result<(), String> {
        if !(1..=10080).contains(&self.interval_minutes) || !(2..=100).contains(&self.total_runs) {
            return Err("重复间隔须为1–10080分钟，总执行次数须为2–100次".into());
        }
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub config: Config,
    pub priority: u8,
    pub due_at_ms: u64,
    pub status: String,
    pub run_id: Option<String>,
    pub message: String,
    #[serde(default)]
    pub repeat: Option<Repeat>,
    #[serde(default)]
    pub completed_runs: u32,
    #[serde(default)]
    pub last_run: Option<RunState>,
}
impl Queue {
    fn recover(&mut self) {
        self.enabled = false;
        for entry in &mut self.entries {
            if entry.status == "running" {
                entry.status = "interrupted".into();
                entry.message = "上次运行中断，不自动重试".into();
            }
        }
    }
    fn finish(&mut self, id: &str, run: &RunState) -> Result<(), String> {
        let entry = self
            .entries
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or("队列记录不存在")?;
        if entry.status != "running" || entry.run_id.as_deref() != Some(&run.run_id) {
            return Err("队列运行标识不匹配，已暂停派发".into());
        }
        entry.status = run.status.clone();
        entry.message = run.message.clone();
        entry.last_run = Some(run.clone());
        if run.status != "completed" {
            self.enabled = false;
            return Ok(());
        }
        entry.completed_runs = entry
            .completed_runs
            .checked_add(1)
            .ok_or("累计执行次数溢出")?;
        if let Some(repeat) = &entry.repeat {
            repeat.validate()?;
            if entry.completed_runs < repeat.total_runs {
                // Fixed delay from completion: missed intervals are never replayed.
                entry.due_at_ms = run
                    .ended_at_ms
                    .ok_or("运行缺少结束时间")?
                    .checked_add(u64::from(repeat.interval_minutes) * 60_000)
                    .ok_or("下次执行时间溢出")?;
                entry.status = "pending".into();
                entry.message = format!(
                    "已成功执行 {}/{} 次，等待下次预约",
                    entry.completed_runs, repeat.total_runs
                );
            }
        }
        Ok(())
    }
    fn next(&self, now: u64) -> Option<usize> {
        if !self.enabled {
            return None;
        }
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.status == "pending" && e.due_at_ms <= now)
            .min_by_key(|(i, e)| (std::cmp::Reverse(e.priority), e.due_at_ms, *i))
            .map(|(i, _)| i)
    }
}
pub fn load(path: &Path) -> Result<Queue, String> {
    let mut queue: Queue = if path.exists() {
        serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| format!("队列损坏，原文件保留：{e}"))?
    } else {
        Queue::default()
    };
    for entry in &queue.entries {
        entry.config.validate()?;
        if entry.priority > 100 || entry.completed_runs > 100 {
            return Err("队列优先级或累计次数超出范围，原文件保留".into());
        }
        if let Some(repeat) = &entry.repeat {
            repeat.validate()?;
            if entry.completed_runs > repeat.total_runs
                || (entry.status == "pending" && entry.completed_runs == repeat.total_runs)
            {
                return Err("周期任务累计次数与状态不一致，原文件保留".into());
            }
        }
        config::validate_serial(&entry.config.serial)?;
    }
    queue.recover();
    config::atomic_json(path, &queue)?;
    Ok(queue)
}
fn change(
    state: &AppState,
    edit: impl FnOnce(&mut Queue) -> Result<(), String>,
) -> Result<(), String> {
    let mut runtime = state.runtime.lock().map_err(|e| e.to_string())?;
    let mut queue = runtime.snapshot.queue.clone();
    edit(&mut queue)?;
    config::atomic_json(&state.data.join("config/queue.json"), &queue)?;
    runtime.snapshot.queue = queue;
    Ok(())
}
#[tauri::command]
pub fn enqueue_task(
    app: AppHandle,
    state: State<'_, AppState>,
    config: Config,
    priority: u8,
    due_at_ms: u64,
    repeat: Option<Repeat>,
) -> Result<(), String> {
    config.validate()?;
    if let Some(repeat) = &repeat {
        repeat.validate()?;
    }
    config::validate_serial(&config.serial)?;
    if priority > 100 || due_at_ms > telemetry::now_ms() + 366 * 86_400_000 {
        return Err("优先级须为0–100，预约时间须在一年以内".into());
    }
    change(&state, |queue| {
        if queue.entries.len() >= 200 {
            return Err("队列最多保留200条记录，请清理已结束记录".into());
        }
        queue.entries.push(Entry {
            id: uuid::Uuid::new_v4().to_string(),
            config,
            priority,
            due_at_ms,
            status: "pending".into(),
            run_id: None,
            message: String::new(),
            repeat,
            completed_runs: 0,
            last_run: None,
        });
        Ok(())
    })?;
    state.publish(&app, "info", "任务配置快照已加入队列");
    Ok(())
}
#[tauri::command]
pub fn set_queue_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    change(&state, |queue| {
        queue.enabled = enabled;
        Ok(())
    })?;
    state.publish(
        &app,
        "info",
        if enabled {
            "队列已启用，到期后串行运行"
        } else {
            "队列已暂停；当前任务可单独停止"
        },
    );
    Ok(())
}
#[tauri::command]
pub fn remove_queue_entry(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    change(&state, |queue| {
        let entry = queue
            .entries
            .iter()
            .find(|e| e.id == id)
            .ok_or("记录不存在")?;
        if entry.status == "running" {
            return Err("请先停止正在运行的任务".into());
        }
        queue.entries.retain(|e| e.id != id);
        Ok(())
    })?;
    state.publish(&app, "info", "已移除队列记录");
    Ok(())
}
// Claim and install cancellation atomically with respect to pause/stop/exit.
fn claim(state: &AppState) -> Result<Option<(Entry, CancelSignal)>, String> {
    let mut runtime = state.runtime.lock().map_err(|e| e.to_string())?;
    let mut queue = runtime.snapshot.queue.clone();
    let Some(index) = queue.next(telemetry::now_ms()) else {
        return Ok(None);
    };
    let entry = &mut queue.entries[index];
    let run = engine::new_run(&entry.config);
    entry.status = "running".into();
    entry.run_id = Some(run.run_id.clone());
    let entry = entry.clone();
    config::atomic_json(&state.data.join("config/queue.json"), &queue)?;
    let cancel = CancelSignal::new();
    runtime.snapshot.queue = queue;
    runtime.snapshot.run = Some(run);
    runtime.cancel = Some(cancel.clone());
    Ok(Some((entry, cancel)))
}
pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let state = app.state::<AppState>();
            let Ok(_guard) = state.operation.try_lock() else {
                continue;
            };
            if state.snapshot().queue.next(telemetry::now_ms()).is_none() {
                continue;
            }
            let (entry, cancel) = match claim(&state) {
                Ok(Some(selected)) => selected,
                Ok(None) => continue,
                Err(error) => {
                    state
                        .runtime
                        .lock()
                        .expect("runtime")
                        .snapshot
                        .queue
                        .enabled = false;
                    state.publish(&app, "error", &error);
                    continue;
                }
            };
            state.publish(&app, "info", "队列任务开始，使用入队时的配置");
            let result = match engine::persist_run(&state) {
                Ok(()) => {
                    let latest = state.data.join("screenshots/latest.png");
                    if latest.exists() {
                        match std::fs::remove_file(latest) {
                            Ok(()) => engine::execute(&app, entry.config, &cancel).await,
                            Err(e) => Err(e.to_string()),
                        }
                    } else {
                        engine::execute(&app, entry.config, &cancel).await
                    }
                }
                Err(error) => Err(error),
            };
            engine::finish(&app, result, &cancel);
            let run = state.snapshot().run.expect("finished run");
            if let Err(error) = change(&state, |queue| queue.finish(&entry.id, &run)) {
                state
                    .runtime
                    .lock()
                    .expect("runtime")
                    .snapshot
                    .queue
                    .enabled = false;
                state.publish(&app, "error", &error);
            }
            state.publish(&app, "info", "队列状态已更新；失败或停止后暂停队列");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entry(priority: u8, due: u64) -> Entry {
        Entry {
            id: uuid::Uuid::new_v4().to_string(),
            config: Config::default(),
            priority,
            due_at_ms: due,
            status: "pending".into(),
            run_id: None,
            message: String::new(),
            repeat: None,
            completed_runs: 0,
            last_run: None,
        }
    }
    fn periodic_queue() -> (Queue, RunState) {
        let mut task = entry(50, 1000);
        task.status = "running".into();
        task.repeat = Some(Repeat {
            interval_minutes: 2,
            total_runs: 2,
        });
        let mut run = engine::new_run(&task.config);
        run.status = "completed".into();
        run.ended_at_ms = Some(900_000);
        task.run_id = Some(run.run_id.clone());
        (
            Queue {
                enabled: true,
                entries: vec![task],
            },
            run,
        )
    }
    #[test]
    fn periodic_completion_uses_finish_time_and_stops_at_total() {
        let (mut queue, run) = periodic_queue();
        let id = queue.entries[0].id.clone();
        queue.finish(&id, &run).unwrap();
        assert_eq!(queue.entries[0].due_at_ms, 1_020_000);
        assert_eq!(queue.entries[0].completed_runs, 1);
        assert_eq!(queue.entries[0].status, "pending");
        assert_eq!(queue.next(1_019_999), None);
        assert_eq!(queue.next(1_020_000), Some(0));
        assert_eq!(
            queue.entries[0].last_run.as_ref().unwrap().run_id,
            run.run_id
        );
        queue.entries[0].status = "running".into();
        queue.finish(&id, &run).unwrap();
        assert_eq!(queue.entries[0].status, "completed");
        assert_eq!(queue.entries[0].completed_runs, 2);
        assert!(queue.next(u64::MAX).is_none());
    }
    #[test]
    fn failed_or_stopped_periodic_tasks_never_reschedule() {
        for status in ["failed", "stopped", "interrupted"] {
            let (mut queue, mut run) = periodic_queue();
            run.status = status.into();
            let id = queue.entries[0].id.clone();
            queue.finish(&id, &run).unwrap();
            assert!(!queue.enabled);
            assert_eq!(queue.entries[0].completed_runs, 0);
            assert_eq!(queue.entries[0].status, status);
            queue.enabled = true;
            assert!(queue.next(u64::MAX).is_none());
        }
    }
    #[test]
    fn completion_does_not_unpause_queue_or_accept_duplicate_results() {
        let (mut queue, run) = periodic_queue();
        let id = queue.entries[0].id.clone();
        queue.enabled = false;
        let mut wrong_run = run.clone();
        wrong_run.run_id = "other-run".into();
        assert!(queue.finish(&id, &wrong_run).is_err());
        assert_eq!(queue.entries[0].completed_runs, 0);
        queue.finish(&id, &run).unwrap();
        assert!(!queue.enabled);
        assert!(queue.next(u64::MAX).is_none());
        assert!(queue.finish(&id, &run).is_err());
        assert_eq!(queue.entries[0].completed_runs, 1);
    }
    #[test]
    fn old_queue_entries_remain_one_shot_and_new_fields_round_trip() {
        let mut value = serde_json::to_value(entry(50, 0)).unwrap();
        for field in ["repeat", "completed_runs", "last_run"] {
            value.as_object_mut().unwrap().remove(field);
        }
        let old: Entry = serde_json::from_value(value).unwrap();
        assert!(old.repeat.is_none());
        assert_eq!(old.completed_runs, 0);
        let (queue, _) = periodic_queue();
        let mut restored: Queue =
            serde_json::from_slice(&serde_json::to_vec(&queue).unwrap()).unwrap();
        restored.recover();
        assert!(!restored.enabled);
        assert_eq!(restored.entries[0].status, "interrupted");
        assert_eq!(restored.entries[0].repeat.as_ref().unwrap().total_runs, 2);
    }
    #[test]
    fn recurrence_bounds_and_date_overflow_are_rejected() {
        for (interval_minutes, total_runs) in [(0, 2), (10081, 2), (1, 1), (1, 101)] {
            assert!(
                Repeat {
                    interval_minutes,
                    total_runs
                }
                .validate()
                .is_err()
            );
        }
        assert!(
            Repeat {
                interval_minutes: 10080,
                total_runs: 100
            }
            .validate()
            .is_ok()
        );
        let (mut queue, mut run) = periodic_queue();
        run.ended_at_ms = Some(u64::MAX);
        let id = queue.entries[0].id.clone();
        assert!(queue.finish(&id, &run).is_err());
    }
    fn state(data: std::path::PathBuf) -> AppState {
        let mut task = entry(50, 0);
        task.config.serial = "emulator-5554".into();
        AppState {
            runtime: std::sync::Mutex::new(crate::app::Runtime {
                snapshot: crate::app::Snapshot {
                    sequence: 0,
                    queue: Queue {
                        enabled: true,
                        entries: vec![task],
                    },
                    config: Config::default(),
                    run: None,
                    logs: vec![],
                    adb_source: String::new(),
                    data_directory: String::new(),
                },
                cancel: None,
            }),
            operation: std::sync::Arc::new(tokio::sync::Mutex::new(())),
            adb: crate::device::Adb {
                executable: "unused".into(),
            },
            data,
            resources: "unused".into(),
            detector: crate::closed_loop::cache(),
        }
    }
    #[test]
    fn failed_persistence_does_not_claim_or_activate_task() {
        let state = state(std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()));
        assert!(claim(&state).is_err());
        assert!(!state.is_running());
        assert!(state.snapshot().run.is_none());
        assert_eq!(state.snapshot().queue.entries[0].status, "pending");
    }
    #[test]
    fn claim_is_persisted_cancelable_and_recovered_without_retry() {
        let dir = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(dir.join("config")).unwrap();
        let state = state(dir.clone());
        let (entry, token) = claim(&state).unwrap().unwrap();
        assert_eq!(entry.config.serial, "emulator-5554");
        assert!(state.is_running());
        assert!(claim(&state).unwrap().is_none());
        state.cancel();
        assert!(token.is_cancelled());
        assert!(claim(&state).unwrap().is_none());
        let recovered = load(&dir.join("config/queue.json")).unwrap();
        assert!(!recovered.enabled);
        assert_eq!(recovered.entries[0].status, "interrupted");
        assert_eq!(recovered.entries[0].run_id, entry.run_id);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn stop_before_claim_prevents_activation() {
        let state = state(std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()));
        state.cancel();
        assert!(claim(&state).unwrap().is_none());
        assert!(!state.is_running());
    }
    #[test]
    fn orders_due_tasks_by_priority_then_time_then_insertion() {
        let mut queue = Queue {
            enabled: true,
            entries: vec![
                entry(1, 1),
                entry(9, 100),
                entry(2, 2),
                entry(2, 1),
                entry(2, 1),
            ],
        };
        assert_eq!(queue.next(10), Some(3));
        queue.entries[3].status = "completed".into();
        assert_eq!(queue.next(10), Some(4));
        assert_eq!(queue.next(100), Some(1));
        queue.enabled = false;
        assert_eq!(queue.next(100), None);
    }
    #[test]
    fn recovery_pauses_pending_and_never_retries_interrupted() {
        let mut queue = Queue {
            enabled: true,
            entries: vec![entry(1, 0), entry(2, 0)],
        };
        queue.entries[1].status = "running".into();
        queue.recover();
        assert!(!queue.enabled);
        assert_eq!(queue.entries[1].status, "interrupted");
        queue.enabled = true;
        assert_eq!(queue.next(1), Some(0));
    }
}
