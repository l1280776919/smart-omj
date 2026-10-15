use crate::device::CancelSignal;
use crate::{
    app::{AppState, RunState},
    config::{self, Config},
    telemetry,
};
use tauri::{AppHandle, Manager};

pub async fn execute(app: &AppHandle, config: Config, cancel: &CancelSignal) -> Result<(), String> {
    if ![1,11].contains(&config.soul.layer) { return Err("当前闭环支持壹层与悲鸣".into()); }
    let state = app.state::<AppState>();
    let button_model=std::fs::read(state.data.join("button-studio/active.json")).ok()
        .and_then(|b|serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|v|v["directory"].as_str().map(std::path::PathBuf::from));
    let run_id = state.snapshot().run.as_ref().ok_or("运行记录缺失")?.run_id.clone();
    let output = state.data.join("runs").join(run_id);
    let mut last_page = crate::tasks::soul_graph::Page::Unknown;
    let result = crate::closed_loop::run(&state.adb, &state.resources, &output, state.detector.clone(),
        crate::closed_loop::Options { serial: config.serial, layer:config.soul.layer, button_model, rounds: config.soul.rounds,
            max_seconds: u64::from(config.soul.max_minutes)*60,
            stage_timeout_seconds: u64::from(config.soul.stage_timeout_seconds),
            observe_only: false, max_frames: None }, cancel, |report| {
            update_stage(&state, report.page.label(), report.completed);
            state.publish(app, "debug", &crate::closed_loop::timing_log(report));
            if report.page != last_page || report.dispatched || !report.message.is_empty() {
                state.publish(app, "info", &format!("{} · 完成 {} 轮 · {}{}",
                    report.page.label(), report.completed,
                    if report.dispatched { "动作已派发；等待新截图验证。" } else { "" }, report.message));
            }
            last_page = report.page;
            let _ = persist_run(&state);
        }).await;
    if output.join("latest.png").exists() {
        std::fs::copy(output.join("latest.png"), state.data.join("screenshots/latest.png")).map_err(|e|e.to_string())?;
    }
    result.map(|_|())
}

fn update_stage(state: &AppState, stage: &str, completed: u32) {
    if let Some(run) = state
        .runtime
        .lock()
        .expect("runtime lock poisoned")
        .snapshot
        .run
        .as_mut()
    {
        run.stage = stage.into();
        run.completed_rounds = completed;
    }
}
pub fn persist_run(state: &AppState) -> Result<(), String> {
    if let Some(run) = &state.snapshot().run {
        config::atomic_json(&state.data.join("config/last-run.json"), run)?;
    }
    Ok(())
}
pub fn new_run(config: &Config) -> RunState {
    RunState {
        run_id: uuid::Uuid::new_v4().to_string(),
        device_id: config.serial.clone(),
        status: "running".into(),
        stage: "准备".into(),
        completed_rounds: 0,
        started_at_ms: telemetry::now_ms(),
        ended_at_ms: None,
        message: if config.soul.layer==11 {"御魂悲鸣循环：逐轮核对层数与阵容".into()}else{"御魂壹层视觉闭环（本机试验模型）".into()},
        failure_screenshot: None,
    }
}
pub fn finish(app: &AppHandle, result: Result<(), String>, cancel: &CancelSignal) {
    let state = app.state::<AppState>();
    let (status, message) = if cancel.is_cancelled() {
        ("stopped", "用户停止；不再发送新操作".into())
    } else {
        match result {
            Ok(()) => ("completed", "已达到轮数或运行时间限制".into()),
            Err(error) => ("failed", error),
        }
    };
    {
        let mut runtime = state.runtime.lock().expect("runtime lock poisoned");
        if let Some(run) = runtime.snapshot.run.as_mut() {
            run.status = status.into();
            run.message = message.clone();
            run.ended_at_ms = Some(telemetry::now_ms());
            if status == "failed" {
                let path = state
                    .data
                    .join("screenshots")
                    .join(format!("failure-{}.png", run.run_id));
                if std::fs::copy(state.data.join("screenshots/latest.png"), &path).is_ok() {
                    run.failure_screenshot = Some(path.display().to_string());
                }
            }
        }
        runtime.cancel = None;
    }
    if let Err(error) = persist_run(&state) {
        state.publish(app, "error", &format!("运行记录保存失败：{error}"));
    }
    state.publish(
        app,
        if status == "failed" { "error" } else { "info" },
        &message,
    );
}
