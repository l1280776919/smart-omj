use crate::device::CancelSignal;
use crate::{
    app::{AppState, Snapshot},
    config::{self, Config, LIMITS, Limits},
    device::Device,
    engine,
    vision::{self, Diagnosis, Templates},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Serialize;
use tauri::{AppHandle, State};

#[derive(Serialize)]
pub struct Bootstrap {
    pub snapshot: Snapshot,
    pub limits: Limits,
}
#[derive(Serialize)]
pub struct Screenshot {
    pub data_url: String,
    pub width: u32,
    pub height: u32,
}
#[derive(Serialize)]
pub struct VerifiedClick {
    pub screenshot: Screenshot,
    pub diagnosis: Diagnosis,
    pub target_disappeared: bool,
    pub verification: String,
}

fn token() -> CancelSignal {
    CancelSignal::new()
}
fn busy() -> String {
    "设备正在执行任务或其他操作，请先停止并等待结束".into()
}

#[derive(Serialize)]
pub struct VisualPreview {
    screenshot: Screenshot,
    observation: crate::vision::detector::Observation,
    page: crate::tasks::soul_graph::Page,
    page_label: String,
    suggested: crate::tasks::soul_graph::Decision,
    reason: String,
}
#[tauri::command]
pub async fn observe_soul(app: AppHandle, state: State<'_, AppState>, serial: String) -> Result<VisualPreview,String> {
    let _guard=state.operation.try_lock().map_err(|_|busy())?;
    let start=std::time::Instant::now();
    let (bytes,screenshot)=capture(&state,&serial).await?;
    let screenshot_ms=start.elapsed().as_secs_f64()*1000.;
    let recognition_start=std::time::Instant::now();
    let soul11=state.snapshot().config.soul.layer==11;
    let mut observation=if soul11 {
        let resources=state.resources.clone();
        let value:serde_json::Value=serde_json::from_slice(&std::fs::read(state.data.join("button-studio/active.json")).map_err(|_|"请先启用按钮模型")?).map_err(|e|e.to_string())?;
        let model=std::path::PathBuf::from(value["directory"].as_str().ok_or("启用模型路径缺失")?);
        tokio::task::spawn_blocking(move||crate::tasks::soul11::Observer::load(&resources,&model)?.observe(&bytes,&token())).await.map_err(|e|e.to_string())??
    }else{crate::closed_loop::infer(state.detector.clone(),state.resources.clone(),bytes,token()).await?};
    observation.timings.recognition_ms=recognition_start.elapsed().as_secs_f64()*1000.;
    observation.timings.screenshot_ms=screenshot_ms;
    observation.timings.total_ms=start.elapsed().as_secs_f64()*1000.;
    let page=if soul11 {crate::tasks::soul_graph::classify_soul11(&observation)}else{crate::tasks::soul_graph::classify(&observation)};
    let mut controller=if soul11 {crate::tasks::soul_graph::Controller::soul11(1,180000)}else{crate::tasks::soul_graph::Controller::new(1,180000)};
    // A preview proposes an action; execution still requires two fresh frames.
    let decision_start=std::time::Instant::now();
    let first=controller.tick(&observation,0);
    let next=if first.is_ok(){controller.tick(&observation,0)}else{first};
    observation.timings.decision_ms=decision_start.elapsed().as_secs_f64()*1000.;
    observation.timings.total_ms=start.elapsed().as_secs_f64()*1000.;
    let t=&observation.timings;
    state.publish(&app,"debug",&format!("观察截图 · {} · 截图 {:.1} ms · 识别合计 {:.1} ms（含模型加载）· 解码 {:.1} / 预处理 {:.1} / 按钮推理 {:.1} / 后处理 {:.1} / 场景 {:.1} / 模板 {:.1} ms · 决策 {:.2} ms · 合计 {:.1} ms；未点击",page.label(),t.screenshot_ms,t.recognition_ms,t.decode_ms,t.preprocess_ms,t.inference_ms,t.postprocess_ms,t.scene_ms,t.template_ms,t.decision_ms,t.total_ms));
    let reason=next.as_ref().err().cloned().unwrap_or_else(||format!("{}；{} 个检测元素。建议动作仅供观察，执行时需要连续新截图确认。",page.label(),observation.detections.len()));
    Ok(VisualPreview {screenshot,observation,page,page_label:page.label().into(),suggested:next.unwrap_or(crate::tasks::soul_graph::Decision::Wait),reason})
}

#[tauri::command]
pub fn get_snapshot(state: State<'_, AppState>) -> Bootstrap {
    Bootstrap {
        snapshot: state.snapshot(),
        limits: LIMITS,
    }
}
#[tauri::command]
pub async fn save_config(
    app: AppHandle,
    state: State<'_, AppState>,
    config: Config,
) -> Result<(), String> {
    let _guard = state.operation.try_lock().map_err(|_| busy())?;
    config.validate()?;
    config::atomic_json(&state.data.join("config/config.json"), &config)?;
    state
        .runtime
        .lock()
        .map_err(|e| e.to_string())?
        .snapshot
        .config = config;
    state.publish(&app, "info", "配置已保存");
    Ok(())
}
#[tauri::command]
pub async fn list_devices(state: State<'_, AppState>) -> Result<Vec<Device>, String> {
    let _guard = state.operation.try_lock().map_err(|_| busy())?;
    state.adb.devices(&token()).await
}
#[tauri::command]
pub async fn connect_device(
    app: AppHandle,
    state: State<'_, AppState>,
    endpoint: String,
) -> Result<String, String> {
    let _guard = state.operation.try_lock().map_err(|_| busy())?;
    let result = state.adb.connect(&endpoint, &token()).await?;
    state.publish(&app, "info", &result);
    Ok(result)
}
async fn capture(state: &AppState, serial: &str) -> Result<(Vec<u8>, Screenshot), String> {
    let bytes = state.adb.screenshot(serial, &token()).await?;
    let image = vision::decode(&bytes)?;
    let screen = Screenshot {
        data_url: format!("data:image/png;base64,{}", STANDARD.encode(&bytes)),
        width: image.width(),
        height: image.height(),
    };
    Ok((bytes, screen))
}
#[tauri::command]
pub async fn capture_screen(
    state: State<'_, AppState>,
    serial: String,
) -> Result<Screenshot, String> {
    let _guard = state.operation.try_lock().map_err(|_| busy())?;
    capture(&state, &serial).await.map(|(_, screen)| screen)
}
#[tauri::command]
pub async fn tap_device(
    app: AppHandle,
    state: State<'_, AppState>,
    serial: String,
    x: u32,
    y: u32,
    expected_size: [u32; 2],
) -> Result<Screenshot, String> {
    let _guard = state.operation.try_lock().map_err(|_| busy())?;
    let (_, screen) = capture(&state, &serial).await?;
    let dimensions = [screen.width, screen.height];
    crate::device::ensure_dimensions(expected_size, dimensions)?;
    state.adb.tap(&serial, x, y, dimensions, &token()).await?;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    state.publish(&app, "info", &format!("手动点击 ({x}, {y})，重新截图"));
    capture(&state, &serial).await.map(|(_, screen)| screen)
}
#[tauri::command]
pub async fn swipe_device(
    state: State<'_, AppState>,
    serial: String,
    points: [u32; 4],
    duration_ms: u32,
    expected_size: [u32; 2],
) -> Result<Screenshot, String> {
    let _guard = state.operation.try_lock().map_err(|_| busy())?;
    let (_, screen) = capture(&state, &serial).await?;
    let dimensions = [screen.width, screen.height];
    crate::device::ensure_dimensions(expected_size, dimensions)?;
    state
        .adb
        .swipe(&serial, points, duration_ms, dimensions, &token())
        .await?;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    capture(&state, &serial).await.map(|(_, screen)| screen)
}
#[tauri::command]
pub async fn diagnose_screen(
    state: State<'_, AppState>,
    serial: String,
) -> Result<Diagnosis, String> {
    let _guard = state.operation.try_lock().map_err(|_| busy())?;
    let (bytes, _) = capture(&state, &serial).await?;
    let resources = state.resources.clone();
    tokio::task::spawn_blocking(move || Templates::load(&resources)?.diagnose(&bytes, &token()))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn click_element(
    app: AppHandle,
    state: State<'_, AppState>,
    serial: String,
    element: String,
) -> Result<VerifiedClick, String> {
    let _guard = state.operation.try_lock().map_err(|_| busy())?;
    if !["challenge", "ready", "locked"].contains(&element.as_str()) {
        return Err("此元素不允许诊断点击".into());
    }
    let (bytes, _) = capture(&state, &serial).await?;
    let resources = state.resources.clone();
    let before = tokio::task::spawn_blocking(move || {
        Templates::load(&resources)?.diagnose(&bytes, &token())
    })
    .await
    .map_err(|e| e.to_string())??;
    let target = before
        .matches
        .iter()
        .find(|m| m.id == element && m.found)
        .ok_or("新截图中目标未匹配，未发送点击")?;
    let [x, y, w, h] = target.bounds;
    let [x, y] = before.viewport.point(x + w / 2, y + h / 2)?;
    state
        .adb
        .tap(&serial, x, y, before.viewport.source_size, &token())
        .await?;
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    let (bytes, screenshot) = capture(&state, &serial).await?;
    let resources = state.resources.clone();
    let diagnosis = tokio::task::spawn_blocking(move || {
        Templates::load(&resources)?.diagnose(&bytes, &token())
    })
    .await
    .map_err(|e| e.to_string())??;
    let target_disappeared = diagnosis.alignment_confident
        && !diagnosis.matches.iter().any(|m| m.id == element && m.found);
    let verification = if !diagnosis.alignment_confident {
        "unconfirmed"
    } else if target_disappeared {
        "disappeared"
    } else {
        "still_present"
    };
    state.publish(
        &app,
        "info",
        &format!(
            "识别 {element} 后点击并重新识别，目标{}；不自动重试",
            if !diagnosis.alignment_confident {
                "状态未确认"
            } else if target_disappeared {
                "已消失"
            } else {
                "仍存在"
            }
        ),
    );
    Ok(VerifiedClick {
        screenshot,
        diagnosis,
        target_disappeared,
        verification: verification.into(),
    })
}
#[tauri::command]
pub async fn start_task(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let guard = state
        .operation
        .clone()
        .try_lock_owned()
        .map_err(|_| busy())?;
    let config = state.snapshot().config;
    config.validate()?;
    config::validate_serial(&config.serial)?;
    if ![1,11].contains(&config.soul.layer) {
        return Err("当前视觉闭环支持壹层与悲鸣".into());
    }
    if !state.resources.join("detector/soul1.onnx").is_file() {
        return Err("缺少御魂检测模型，请先训练并导出模型".into());
    }
    let cancel = token();
    let run = engine::new_run(&config);
    let id = run.run_id.clone();
    // Persist before making the run active; on failure no worker is started.
    config::atomic_json(&state.data.join("config/last-run.json"), &run)?;
    let latest = state.data.join("screenshots/latest.png");
    if latest.exists() {
        std::fs::remove_file(&latest).map_err(|e| e.to_string())?;
    }
    {
        let mut runtime = state.runtime.lock().map_err(|e| e.to_string())?;
        runtime.snapshot.run = Some(run);
        runtime.cancel = Some(cancel.clone());
    }
    state.publish(
        &app,
        "info",
        "启动御魂循环：观察当前页面，验证目标层数后执行",
    );
    tauri::async_runtime::spawn(async move {
        let result = engine::execute(&app, config, &cancel).await;
        engine::finish(&app, result, &cancel);
        drop(guard);
    });
    Ok(id)
}
#[tauri::command]
pub fn stop_task(app: AppHandle, state: State<'_, AppState>) {
    state.cancel();
    state.publish(&app, "info", "已请求停止，等待当前操作结束");
}

#[tauri::command]
pub fn preview_oas_config(
    state: State<'_, AppState>,
    content: String,
) -> Result<config::import::Preview, String> {
    config::import::preview(&content, &state.snapshot().config)
}
#[tauri::command]
pub async fn import_oas_config(
    app: AppHandle,
    state: State<'_, AppState>,
    content: String,
    expected_base: Config,
) -> Result<String, String> {
    let _guard = state.operation.try_lock().map_err(|_| busy())?;
    let backup = {
        let mut runtime = state.runtime.lock().map_err(|e| e.to_string())?;
        if runtime.snapshot.queue.enabled {
            return Err("请先暂停队列，再导入配置".into());
        }
        let (report, backup) = config::import::apply(
            &state.data.join("config"),
            &content,
            &runtime.snapshot.config,
            &expected_base,
        )?;
        runtime.snapshot.config = report.config;
        backup
    };
    state.publish(
        &app,
        "info",
        "旧OAS配置已导入，原始文件与导入前配置已备份；未启动任务",
    );
    Ok(backup)
}
