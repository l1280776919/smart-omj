use crate::device::CancelSignal;
use crate::{
    config::{self, Config},
    device::Adb,
    telemetry::{self, Log},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunState {
    pub run_id: String,
    pub device_id: String,
    pub status: String,
    pub stage: String,
    pub completed_rounds: u32,
    pub started_at_ms: u64,
    pub ended_at_ms: Option<u64>,
    pub message: String,
    pub failure_screenshot: Option<String>,
}
#[derive(Clone, Serialize)]
pub struct Snapshot {
    pub sequence: u64,
    pub queue: crate::scheduler::Queue,
    pub config: Config,
    pub run: Option<RunState>,
    pub logs: Vec<Log>,
    pub adb_source: String,
    pub data_directory: String,
}
pub struct Runtime {
    pub snapshot: Snapshot,
    pub cancel: Option<CancelSignal>,
}
pub struct AppState {
    pub detector: crate::closed_loop::DetectorCache,
    pub runtime: Mutex<Runtime>,
    pub operation: Arc<tokio::sync::Mutex<()>>,
    pub adb: Adb,
    pub data: PathBuf,
    pub resources: PathBuf,
}
impl AppState {
    pub fn new(app: &AppHandle) -> Result<Self, Box<dyn std::error::Error>> {
        let data = app.path().app_data_dir()?;
        for folder in ["config", "logs", "screenshots"] {
            fs::create_dir_all(data.join(folder))?;
        }
        let resources = if cfg!(debug_assertions) {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources")
        } else {
            app.path().resource_dir()?.join("resources")
        };
        let bundled = resources.join("adb/adb.exe");
        let executable = if bundled.is_file() {
            bundled
        } else if cfg!(debug_assertions) {
            PathBuf::from("adb")
        } else {
            return Err("发布包缺少 resources/adb/adb.exe，禁止借用开发机 ADB".into());
        };
        let config = config::load(&data.join("config/config.json"))?;
        let checkpoint = data.join("config/last-run.json");
        let mut run: Option<RunState> = if checkpoint.exists() {
            Some(serde_json::from_slice(&fs::read(&checkpoint)?)?)
        } else {
            None
        };
        if let Some(run) = run.as_mut()
            && matches!(run.status.as_str(), "running" | "stopping")
        {
            run.status = "interrupted".into();
            run.message = "上次运行未正常结束，已标记中断；不会自动恢复操作".into();
            run.ended_at_ms = Some(telemetry::now_ms());
            config::atomic_json(&checkpoint, run)?;
        }
        Ok(Self {
            detector: crate::closed_loop::cache(),
            runtime: Mutex::new(Runtime {
                snapshot: Snapshot {
                    sequence: 0,
                    queue: crate::scheduler::load(&data.join("config/queue.json"))?,
                    config,
                    run,
                    logs: vec![],
                    adb_source: executable.display().to_string(),
                    data_directory: data.display().to_string(),
                },
                cancel: None,
            }),
            operation: Arc::new(tokio::sync::Mutex::new(())),
            adb: Adb { executable },
            data,
            resources,
        })
    }
    pub fn snapshot(&self) -> Snapshot {
        self.runtime
            .lock()
            .expect("runtime lock poisoned")
            .snapshot
            .clone()
    }
    pub fn is_running(&self) -> bool {
        self.runtime
            .lock()
            .expect("runtime lock poisoned")
            .cancel
            .is_some()
    }
    pub fn cancel(&self) {
        let mut runtime = self.runtime.lock().expect("runtime lock poisoned");
        runtime.snapshot.queue.enabled = false;
        if let Some(token) = &runtime.cancel {
            token.cancel();
        }
        if let Some(run) = runtime.snapshot.run.as_mut()
            && run.status == "running"
        {
            run.status = "stopping".into();
        }
        runtime.snapshot.sequence += 1;
    }
    pub fn publish(&self, app: &AppHandle, level: &str, message: &str) {
        let mut runtime = self.runtime.lock().expect("runtime lock poisoned");
        runtime.snapshot.sequence += 1;
        let log = Log {
            sequence: runtime.snapshot.sequence,
            timestamp_ms: telemetry::now_ms(),
            level: level.into(),
            device_id: runtime
                .snapshot
                .run
                .as_ref()
                .map(|r| r.device_id.clone())
                .unwrap_or_else(|| runtime.snapshot.config.serial.clone()),
            run_id: runtime.snapshot.run.as_ref().map(|r| r.run_id.clone()),
            message: message.into(),
        };
        if let Err(error) = telemetry::append(&self.data.join("logs/engine.jsonl"), &log) {
            eprintln!("日志保存失败：{error}");
        }
        runtime.snapshot.logs.push(log);
        if runtime.snapshot.logs.len() > 400 {
            runtime.snapshot.logs.remove(0);
        }
        let snapshot = runtime.snapshot.clone();
        drop(runtime);
        let _ = app.emit("engine:snapshot", snapshot);
    }
}
