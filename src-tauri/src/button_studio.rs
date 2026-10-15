//! Independent button annotations, training jobs and raw-coordinate inference.
use crate::{app::AppState, config::atomic_json, device::CancelSignal, vision::detector::Detector};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
};
use tauri::State;

#[derive(Clone, Serialize, Deserialize)]
pub struct ButtonClass {
    id: String,
    name: String,
    anchor: [f32; 2],
}
#[derive(Clone, Serialize, Deserialize)]
pub struct BoxLabel {
    label: String,
    bounds: [f32; 4],
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Sample {
    id: String,
    name: String,
    batch: String,
    split: String,
    reviewed: bool,
    width: u32,
    height: u32,
    boxes: Vec<BoxLabel>,
}
#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Dataset {
    classes: Vec<ButtonClass>,
    samples: Vec<Sample>,
}
struct Job {
    child: Child,
    directory: PathBuf,
    status: String,
}
type Cache = Arc<Mutex<Option<(PathBuf, Detector)>>>;
#[derive(Default)]
pub struct ButtonStudio {
    data: Mutex<()>,
    job: Mutex<Option<Job>>,
    cache: Cache,
}
impl ButtonStudio {
    pub fn shutdown(&self) {
        if let Ok(mut guard) = self.job.lock() {
            if let Some(job) = guard.as_mut() {
                let _ = refresh(job);
                if job.status == "running" {
                    let _ = job.child.kill();
                    let _ = job.child.wait();
                    job.status = "cancelled".into();
                    let _ = atomic_json(&job.directory.join("state.json"), &json!("cancelled"));
                }
            }
        }
    }
}
impl Drop for ButtonStudio {
    fn drop(&mut self) {
        self.shutdown();
    }
}
fn root(state: &AppState) -> PathBuf {
    state.data.join("button-studio")
}
fn training() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("training")
}
fn read(path: impl AsRef<Path>) -> Value {
    fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(Value::Null)
}
fn dataset(root: &Path) -> Result<Dataset, String> {
    if !root.join("annotations.json").exists() {
        return Ok(Dataset::default());
    }
    serde_json::from_slice(&fs::read(root.join("annotations.json")).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
fn save(root: &Path, data: &Dataset) -> Result<(), String> {
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    atomic_json(&root.join("annotations.json"), data)
}
fn validate(classes: &[ButtonClass], sample: &Sample) -> Result<(), String> {
    let mut ids = std::collections::HashSet::new();
    for c in classes {
        if c.id.is_empty()
            || c.id.len() > 64
            || !c.id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            || !ids.insert(&c.id)
            || c.name.trim().is_empty()
            || c.name.len() > 120
            || c.anchor
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err("按钮标识、名称或点击点无效".into());
        }
    }
    if !["train", "val"].contains(&sample.split.as_str())
        || sample.batch.trim().is_empty()
        || sample.batch.len() > 200
        || sample.boxes.len() > 200
    {
        return Err("采集批次或标注无效".into());
    }
    for b in &sample.boxes {
        let [x1, y1, x2, y2] = b.bounds;
        if !ids.contains(&b.label)
            || b.bounds.iter().any(|v| !v.is_finite())
            || x1 < 0.
            || y1 < 0.
            || x2 > sample.width as f32
            || y2 > sample.height as f32
            || x2 - x1 < 2.
            || y2 - y1 < 2.
        {
            return Err("标注框超出图片或类别不存在".into());
        }
    }
    Ok(())
}
fn image_bytes(url: &str) -> Result<Vec<u8>, String> {
    if url.len() > 32 * 1024 * 1024 {
        return Err("图片不得超过 24 MB".into());
    }
    STANDARD
        .decode(
            url.strip_prefix("data:image/png;base64,")
                .ok_or("请使用 PNG 图片")?,
        )
        .map_err(|e| e.to_string())
}
fn import(root: &Path, bytes: &[u8], name: String, batch: String) -> Result<String, String> {
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().map_err(|e| e.to_string())?;
    let mut data = dataset(root)?;
    let id = uuid::Uuid::new_v4().to_string();
    let sample = Sample {
        id: id.clone(),
        name,
        batch,
        split: "train".into(),
        reviewed: false,
        width: image.width(),
        height: image.height(),
        boxes: vec![],
    };
    validate(&data.classes, &sample)?;
    fs::create_dir_all(root.join("images")).map_err(|e| e.to_string())?;
    image
        .save(root.join("images").join(format!("{id}.png")))
        .map_err(|e| e.to_string())?;
    data.samples.push(sample);
    save(root, &data)?;
    Ok(id)
}
fn refresh(job: &mut Job) -> Result<(), String> {
    if job.status == "running" {
        if let Some(exit) = job.child.try_wait().map_err(|e| e.to_string())? {
            job.status = if exit.success() && job.directory.join("result.json").is_file() {
                "completed"
            } else {
                "failed"
            }
            .into();
            atomic_json(&job.directory.join("state.json"), &job.status)?;
        }
    }
    Ok(())
}
fn job_info(directory: &Path, status: &str) -> Value {
    let log = fs::read_to_string(directory.join("output.log")).unwrap_or_default();
    let tail = log
        .lines()
        .rev()
        .take(20)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    json!({"id":directory.file_name().and_then(|v|v.to_str()),"config":read(directory.join("config.json")),"lineage":read(directory.join("lineage.json")),"status":status,"progress":read(directory.join("progress.json")),"result":read(directory.join("result.json")),"log":tail})
}
fn active(state: &AppState) -> PathBuf {
    read(root(state).join("active.json"))["directory"]
        .as_str()
        .map(PathBuf::from)
        .unwrap_or_else(|| state.resources.join("detector"))
}
fn classes(path: &Path) -> Vec<ButtonClass> {
    let mut values: Vec<ButtonClass> = serde_json::from_value(read(path.join("buttons.json")))
        .unwrap_or_else(|_| {
            read(path.join("model.json"))["classes"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str())
                        .map(|id| ButtonClass {
                            id: id.into(),
                            name: id.into(),
                            anchor: [0.5, 0.5],
                        })
                        .collect()
                })
                .unwrap_or_default()
        });
    let defaults: Vec<ButtonClass> =
        serde_json::from_str(include_str!("../resources/detector/buttons.json"))
            .unwrap_or_default();
    for item in &mut values {
        if item.name == item.id {
            if let Some(default) = defaults.iter().find(|c| c.id == item.id) {
                item.name = default.name.clone();
            }
        }
    }
    values
}
fn candidate(root: &Path, id: &str) -> Result<PathBuf, String> {
    uuid::Uuid::parse_str(id).map_err(|_| "模型任务 ID 无效".to_string())?;
    let dir = root.join("jobs").join(id);
    if !dir.join("result.json").is_file() || !dir.join("export/soul1.onnx").is_file() {
        return Err("候选模型尚未成功导出".into());
    }
    Ok(dir)
}
#[tauri::command]
pub fn button_history(state: State<'_, AppState>) -> Result<Value, String> {
    let root = root(&state);
    let mut entries = Vec::new();
    if root.join("jobs").exists() {
        for entry in fs::read_dir(root.join("jobs")).map_err(|e|e.to_string())? {
            let dir = entry.map_err(|e|e.to_string())?.path();
            if !dir.is_dir() { continue; }
            let saved = read(dir.join("state.json"));
            let status = if dir.join("result.json").is_file() { "completed" } else { saved.as_str().unwrap_or("interrupted") };
            let mut info = job_info(&dir, status);
            info["classes"] = serde_json::to_value(classes(&dir.join("export"))).map_err(|e|e.to_string())?;
            entries.push(info);
        }
    }
    entries.sort_by(|a,b| b["result"]["created_at"].as_u64().cmp(&a["result"]["created_at"].as_u64()));
    Ok(json!({"jobs":entries,"active":read(root.join("active.json")),"active_directory":active(&state),"active_classes":classes(&active(&state))}))
}
#[tauri::command]
pub fn button_status(
    state: State<'_, AppState>,
    studio: State<'_, ButtonStudio>,
) -> Result<Value, String> {
    let _guard = studio.data.lock().map_err(|e| e.to_string())?;
    let root = root(&state);
    let data = dataset(&root)?;
    let mut jobs = studio.job.lock().map_err(|e| e.to_string())?;
    let job = if let Some(job) = jobs.as_mut() {
        refresh(job)?;
        job_info(&job.directory, &job.status)
    } else {
        read(root.join("last-job.json"))["directory"]
            .as_str()
            .map(|p| {
                let dir = PathBuf::from(p);
                let saved = read(dir.join("state.json"));
                let status = saved.as_str().unwrap_or("interrupted");
                let recovered = if status == "running" {
                    if dir.join("result.json").is_file() {
                        "completed"
                    } else {
                        "interrupted"
                    }
                } else {
                    status
                };
                if recovered != status {
                    let _ = atomic_json(&dir.join("state.json"), &recovered);
                }
                job_info(&dir, recovered)
            })
            .unwrap_or(Value::Null)
    };
    Ok(
        json!({"dataset":data,"job":job,"available":training().join(".venv/Scripts/python.exe").is_file(),"model_classes":classes(&active(&state)),"root":root}),
    )
}
#[tauri::command]
pub fn button_import(
    state: State<'_, AppState>,
    studio: State<'_, ButtonStudio>,
    data_url: String,
    name: String,
    batch: String,
) -> Result<String, String> {
    let bytes = image_bytes(&data_url)?;
    let _guard = studio.data.lock().map_err(|e| e.to_string())?;
    import(&root(&state), &bytes, name, batch)
}
#[tauri::command]
pub fn button_import_scene(
    state: State<'_, AppState>,
    studio: State<'_, ButtonStudio>,
    index: usize,
) -> Result<String, String> {
    let prepared = read(training().join("scene-prepared.json"));
    let dir = PathBuf::from(prepared["job"].as_str().ok_or("请先抽取场景素材")?);
    let rows = read(dir.join("samples.json"));
    let row = rows
        .as_array()
        .and_then(|a| a.get(index))
        .ok_or("素材不存在")?;
    let source = PathBuf::from(row["image"].as_str().ok_or("素材路径无效")?);
    let im = image::open(&source).map_err(|e| e.to_string())?;
    let mut bytes = Cursor::new(Vec::new());
    im.write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    let _guard = studio.data.lock().map_err(|e| e.to_string())?;
    import(
        &root(&state),
        bytes.get_ref(),
        source
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into(),
        row["batch"].as_str().unwrap_or("scene-import").into(),
    )
}
#[tauri::command]
pub fn button_sample(
    state: State<'_, AppState>,
    studio: State<'_, ButtonStudio>,
    id: String,
) -> Result<Value, String> {
    let _guard = studio.data.lock().map_err(|e| e.to_string())?;
    let root = root(&state);
    let data = dataset(&root)?;
    let sample = data
        .samples
        .iter()
        .find(|r| r.id == id)
        .ok_or("图片不存在")?;
    let bytes = fs::read(root.join("images").join(format!("{}.png", sample.id)))
        .map_err(|e| e.to_string())?;
    Ok(
        json!({"sample":sample,"data_url":format!("data:image/png;base64,{}",STANDARD.encode(bytes))}),
    )
}
#[tauri::command]
pub fn button_save(
    state: State<'_, AppState>,
    studio: State<'_, ButtonStudio>,
    sample: Sample,
    classes: Vec<ButtonClass>,
) -> Result<(), String> {
    let _guard = studio.data.lock().map_err(|e| e.to_string())?;
    let root = root(&state);
    let mut data = dataset(&root)?;
    update_sample(&mut data, sample, classes)?;
    save(&root, &data)
}
fn update_sample(
    data: &mut Dataset,
    sample: Sample,
    classes: Vec<ButtonClass>,
) -> Result<(), String> {
    let index = data
        .samples
        .iter()
        .position(|r| r.id == sample.id)
        .ok_or("图片不存在")?;
    if sample.width != data.samples[index].width || sample.height != data.samples[index].height {
        return Err("图片尺寸不可修改".into());
    }
    // Class IDs are stable across every annotated image.
    for row in &data.samples {
        validate(&classes, row)?;
    }
    validate(&classes, &sample)?;
    data.classes = classes;
    data.samples[index] = sample;
    Ok(())
}
#[derive(Deserialize)]
pub struct Partition {
    id: String,
    split: String,
    batch: String,
}
fn update_partitions(data: &mut Dataset, updates: Vec<Partition>) -> Result<(), String> {
    let mut next = data.clone();
    for update in updates {
        let row = next
            .samples
            .iter_mut()
            .find(|s| s.id == update.id)
            .ok_or("图片不存在，请刷新素材列表")?;
        row.split = update.split;
        row.batch = update.batch;
        validate(&next.classes, row)?;
    }
    *data = next;
    Ok(())
}
#[tauri::command]
pub fn button_partition(
    state: State<'_, AppState>,
    studio: State<'_, ButtonStudio>,
    updates: Vec<Partition>,
) -> Result<(), String> {
    let _guard = studio.data.lock().map_err(|e| e.to_string())?;
    let root = root(&state);
    let mut data = dataset(&root)?;
    update_partitions(&mut data, updates)?;
    save(&root, &data)
}
#[tauri::command]
pub fn button_prune_classes(
    state: State<'_, AppState>,
    studio: State<'_, ButtonStudio>,
) -> Result<usize, String> {
    let _guard = studio.data.lock().map_err(|e| e.to_string())?;
    let root = root(&state);
    let mut data = dataset(&root)?;
    let before = data.classes.len();
    let used: std::collections::HashSet<_> = data
        .samples
        .iter()
        .flat_map(|s| s.boxes.iter().map(|b| b.label.clone()))
        .collect();
    data.classes.retain(|c| used.contains(&c.id));
    save(&root, &data)?;
    Ok(before - data.classes.len())
}
fn edit_class(
    data: &mut Dataset,
    id: &str,
    replacement: Option<ButtonClass>,
) -> Result<(), String> {
    let index = data
        .classes
        .iter()
        .position(|c| c.id == id)
        .ok_or("按钮类别不存在")?;
    let mut next = data.clone();
    if let Some(class) = replacement {
        next.classes[index] = class.clone();
        for row in &mut next.samples {
            for b in &mut row.boxes {
                if b.label == id {
                    b.label = class.id.clone();
                }
            }
        }
        // Validate even when this dataset has no images yet.
        let probe = Sample {
            id: "probe".into(),
            name: "".into(),
            batch: "probe".into(),
            split: "train".into(),
            reviewed: false,
            width: 1,
            height: 1,
            boxes: vec![],
        };
        validate(&next.classes, &probe)?;
    } else {
        next.classes.remove(index);
        for row in &mut next.samples {
            let before = row.boxes.len();
            row.boxes.retain(|b| b.label != id);
            if row.boxes.len() != before {
                row.reviewed = false;
            }
        }
    }
    for row in &next.samples {
        validate(&next.classes, row)?;
    }
    *data = next;
    Ok(())
}
#[tauri::command]
pub fn button_edit_class(
    state: State<'_, AppState>,
    studio: State<'_, ButtonStudio>,
    id: String,
    replacement: Option<ButtonClass>,
) -> Result<(), String> {
    let _guard = studio.data.lock().map_err(|e| e.to_string())?;
    let root = root(&state);
    let mut data = dataset(&root)?;
    // Preserve a recoverable snapshot before category-wide changes.
    let backup = root.join("history");
    fs::create_dir_all(&backup).map_err(|e| e.to_string())?;
    atomic_json(
        &backup.join(format!("classes-{}.json", uuid::Uuid::new_v4())),
        &data,
    )?;
    edit_class(&mut data, &id, replacement)?;
    save(&root, &data)
}
#[tauri::command]
pub fn button_prelabel_drafts(
    state: State<'_, AppState>, studio: State<'_, ButtonStudio>, samples: Vec<Sample>,
) -> Result<usize, String> {
    let _guard = studio.data.lock().map_err(|e|e.to_string())?;
    let root = root(&state);
    let mut data = dataset(&root)?;
    let original = data.clone();
    apply_drafts(&mut data, &samples)?;
    let backup = root.join("history");
    fs::create_dir_all(&backup).map_err(|e|e.to_string())?;
    atomic_json(&backup.join(format!("prelabels-{}.json",uuid::Uuid::new_v4())), &original)?;
    save(&root, &data)?;
    Ok(samples.len())
}
fn apply_drafts(data: &mut Dataset, samples: &[Sample]) -> Result<(), String> {
    let mut next = data.clone();
    let mut ids = std::collections::HashSet::new();
    for draft in samples {
        if !ids.insert(&draft.id) { return Err("重复图片".into()); }
        validate(&next.classes, draft)?;
        let current = next.samples.iter_mut().find(|s|s.id==draft.id).ok_or("图片已删除")?;
        if current.reviewed || !current.boxes.is_empty() || current.width != draft.width || current.height != draft.height {
            return Err("图片已有人工标注或已审核，请刷新后重试；不会覆盖已有标注".into());
        }
        current.boxes = draft.boxes.clone();
        current.reviewed = false;
    }
    *data = next;
    Ok(())
}
#[tauri::command]
pub fn button_delete(
    state: State<'_, AppState>,
    studio: State<'_, ButtonStudio>,
    id: String,
) -> Result<(), String> {
    let _guard = studio.data.lock().map_err(|e| e.to_string())?;
    let root = root(&state);
    let mut data = dataset(&root)?;
    data.samples.retain(|s| s.id != id);
    save(&root, &data)
}
fn select_training_classes(data: &mut Dataset, selection: Option<Vec<String>>) -> Result<(), String> {
    if let Some(ids) = selection {
        if ids.is_empty() || ids.iter().any(|id| !data.classes.iter().any(|c| &c.id == id)) || ids.iter().collect::<std::collections::HashSet<_>>().len() != ids.len() { return Err("请选择有效且不重复的训练类别".into()); }
        data.classes.retain(|c| ids.contains(&c.id));
        for sample in &mut data.samples { sample.boxes.retain(|b| ids.contains(&b.label)); }
    }
    Ok(())
}
#[tauri::command]
pub fn button_start(
    state: State<'_, AppState>,
    studio: State<'_, ButtonStudio>,
    epochs: u32,
    device: String,
    class_ids: Option<Vec<String>>,
    imgsz: Option<u32>,
    patience: Option<u32>,
) -> Result<(), String> {
    if !(1..=300).contains(&epochs) || !["auto", "cuda", "cpu"].contains(&device.as_str()) {
        return Err("训练参数无效".into());
    }
    let imgsz = imgsz.unwrap_or(640);
    let patience = patience.unwrap_or(15);
    if ![640,960,1280].contains(&imgsz) || !(1..=100).contains(&patience) { return Err("分辨率或早停参数无效".into()); }
    let _guard = studio.data.lock().map_err(|e| e.to_string())?;
    let mut jobs = studio.job.lock().map_err(|e| e.to_string())?;
    if let Some(job) = jobs.as_mut() {
        refresh(job)?;
        if job.status == "running" {
            return Err("已有按钮训练正在运行".into());
        }
    }
    let root = root(&state);
    let mut data = dataset(&root)?;
    select_training_classes(&mut data, class_ids)?;
    for c in &data.classes {
        for split in ["train", "val"] {
            if !data.samples.iter().any(|s| s.reviewed && s.split == split && s.boxes.iter().any(|b| b.label == c.id)) { return Err(format!("{} 缺少 {} 已审核标注", c.name, split)); }
        }
    }
    if data.classes.is_empty() || !data.samples.iter().any(|s| s.reviewed && s.split == "val") {
        return Err("请添加按钮并准备已审核的训练集和独立验证集".into());
    }
    let directory = root.join("jobs").join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    atomic_json(&directory.join("annotations.json"), &data)?;
    atomic_json(&directory.join("config.json"), &json!({"epochs":epochs,"device":device,"imgsz":imgsz,"patience":patience,"class_ids":data.classes.iter().map(|c| &c.id).collect::<Vec<_>>()}))?;
    let log = fs::File::create(directory.join("output.log")).map_err(|e| e.to_string())?;
    let mut command = Command::new(training().join(".venv/Scripts/python.exe"));
    command
        .arg("-u")
        .arg(training().join("button_workbench.py"))
        .arg("--root")
        .arg(&root)
        .arg("--job")
        .arg(&directory)
        .arg("--epochs")
        .arg(epochs.to_string())
        .arg("--imgsz").arg(imgsz.to_string())
        .arg("--patience").arg(patience.to_string())
        .arg("--device")
        .arg(device)
        .current_dir(training())
        .env("PYTHONIOENCODING", "utf-8")
        .stdin(Stdio::null())
        .stdout(log.try_clone().map_err(|e| e.to_string())?)
        .stderr(log);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    atomic_json(&root.join("last-job.json"), &json!({"directory":directory}))?;
    atomic_json(&directory.join("state.json"), &json!("running"))?;
    let child = command.spawn().map_err(|e| {
        let _ = atomic_json(&directory.join("state.json"), &json!("failed"));
        let _ = atomic_json(&directory.join("progress.json"), &json!({"stage":"启动失败","progress":0,"error":e.to_string()}));
        e.to_string()
    })?;
    *jobs = Some(Job {
        child,
        directory,
        status: "running".into(),
    });
    Ok(())
}
#[tauri::command]
pub fn button_cancel(studio: State<'_, ButtonStudio>) {
    studio.shutdown();
}
#[tauri::command]
pub fn button_activate(
    state: State<'_, AppState>,
    studio: State<'_, ButtonStudio>,
    job_id: Option<String>,
) -> Result<(), String> {
    let mut guard = studio.job.lock().map_err(|e| e.to_string())?;
    let directory = if let Some(id) = job_id { candidate(&root(&state), &id)? } else if let Some(job) = guard.as_mut() {
        refresh(job)?;
        if job.status != "completed" {
            return Err("请等待训练成功完成".into());
        }
        job.directory.clone()
    } else {
        let saved = read(root(&state).join("last-job.json"));
        let dir = PathBuf::from(saved["directory"].as_str().ok_or("尚无训练结果")?);
        if read(dir.join("state.json")) != "completed" {
            return Err("没有成功的训练结果".into());
        }
        dir
    };
    let source = directory.join("export");
    let _model = Detector::load_from(&source, &state.resources)?;
    let dest = root(&state)
        .join("models")
        .join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    for name in ["soul1.onnx", "model.json", "buttons.json"] {
        fs::copy(source.join(name), dest.join(name)).map_err(|e| e.to_string())?;
    }
    let backup = root(&state).join("history");
    fs::create_dir_all(&backup).map_err(|e|e.to_string())?;
    atomic_json(&backup.join(format!("active-{}.json",uuid::Uuid::new_v4())), &read(root(&state).join("active.json")))?;
    atomic_json(
        &root(&state).join("active.json"),
        &json!({"directory":dest,"source_job":directory.file_name().and_then(|v|v.to_str())}),
    )?;
    *studio.cache.lock().map_err(|e| e.to_string())? = None;
    Ok(())
}
#[tauri::command]
pub async fn button_recognize(
    state: State<'_, AppState>,
    studio: State<'_, ButtonStudio>,
    data_url: String,
    threshold: f32,
    job_id: Option<String>,
) -> Result<Value, String> {
    if !threshold.is_finite() || !(0.0..=1.0).contains(&threshold) { return Err("阈值无效".into()); }
    let bytes = image_bytes(&data_url)?;
    let directory = match job_id { Some(id) => candidate(&root(&state), &id)?.join("export"), None => active(&state) };
    let resources = state.resources.clone();
    let cache = studio.cache.clone();
    tokio::task::spawn_blocking(move||{
        let mut guard=cache.lock().map_err(|e|e.to_string())?;
        if guard.as_ref().is_none_or(|(p,_)|*p!=directory){*guard=Some((directory.clone(),Detector::load_from(&directory,&resources)?));}
        let observation=guard.as_mut().unwrap().1.observe_threshold(&bytes,&CancelSignal::new(),threshold)?;
        let classes=classes(&directory);
        let items:Vec<_>=observation.detections.iter().map(|d|{
            let class=classes.iter().find(|c|c.id==d.class);let anchor=class.map(|c|c.anchor).unwrap_or([0.5,0.5]);let [x1,y1,x2,y2]=d.bounds;
            json!({"label":d.class,"name":class.map(|c|c.name.as_str()).unwrap_or(&d.class),"confidence":d.confidence,"bounds":d.bounds,
                "point":[((x1+(x2-x1)*anchor[0]).round() as u32).min(observation.dimensions[0]-1),((y1+(y2-y1)*anchor[1]).round() as u32).min(observation.dimensions[1]-1)]})
        }).collect();
        Ok(json!({"model_directory":directory,"items":items,"classes":classes,"dimensions":observation.dimensions,"compute":observation.compute,"timings":observation.timings}))
    }).await.map_err(|e|e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Dataset {
        let class = ButtonClass {
            id: "challenge".into(),
            name: "挑战".into(),
            anchor: [0.5, 0.5],
        };
        let sample = Sample {
            id: "one".into(),
            name: "one.png".into(),
            batch: "shared-visit".into(),
            split: "train".into(),
            reviewed: true,
            width: 1920,
            height: 1080,
            boxes: vec![BoxLabel {
                label: "challenge".into(),
                bounds: [1600., 800., 1800., 1000.],
            }],
        };
        let mut second = sample.clone();
        second.id = "two".into();
        Dataset {
            classes: vec![class],
            samples: vec![sample, second],
        }
    }
    #[test]
    fn drafts_never_overwrite_reviewed_or_existing_boxes_and_are_transactional() {
        let mut data = fixture();
        let original = serde_json::to_value(&data).unwrap();
        let mut draft = data.samples[0].clone();
        draft.reviewed = false;
        assert!(apply_drafts(&mut data, &[draft.clone()]).is_err());
        assert_eq!(serde_json::to_value(&data).unwrap(), original);
        data.samples[0].reviewed = false;
        data.samples[0].boxes.clear();
        let before = serde_json::to_value(&data).unwrap();
        let rejected = data.samples[1].clone();
        assert!(apply_drafts(&mut data, &[draft.clone(), rejected]).is_err());
        assert_eq!(serde_json::to_value(&data).unwrap(), before);
        apply_drafts(&mut data, &[draft]).unwrap();
        assert!(!data.samples[0].reviewed);
        assert_eq!(data.samples[0].boxes.len(), 1);
    }
    #[test]
    fn selected_classes_only_change_the_training_snapshot() {
        let mut original = fixture();
        original.classes.push(ButtonClass {id:"excluded".into(),name:"排除".into(),anchor:[0.5,0.5]});
        original.samples[0].boxes.push(BoxLabel {label:"excluded".into(),bounds:[1.,1.,10.,10.]});
        let mut snapshot = original.clone();
        select_training_classes(&mut snapshot, Some(vec!["challenge".into()])).unwrap();
        assert_eq!(snapshot.classes.len(),1);
        assert_eq!(snapshot.samples[0].boxes.len(),1);
        assert_eq!(original.samples[0].boxes.len(),2);
        assert_eq!(snapshot.samples[0].batch,original.samples[0].batch);
        assert!(select_training_classes(&mut snapshot, Some(vec![])).is_err());
        assert!(select_training_classes(&mut snapshot, Some(vec!["unknown".into()])).is_err());
        assert!(select_training_classes(&mut snapshot, Some(vec!["challenge".into(),"challenge".into()])).is_err());
    }
    #[test]
    fn candidate_rejects_paths_and_missing_exports() {
        assert!(candidate(Path::new("."), "../export").is_err());
        assert!(candidate(Path::new("."), "C:\\models").is_err());
        assert!(candidate(Path::new("."), &uuid::Uuid::new_v4().to_string()).is_err());
    }
    #[test]
    fn changing_one_image_never_repartitions_other_images_in_the_batch() {
        let mut data = fixture();
        let mut sample = data.samples[0].clone();
        sample.split = "val".into();
        let classes = data.classes.clone();
        update_sample(&mut data, sample, classes).unwrap();
        assert_eq!(data.samples[0].split, "val");
        assert_eq!(data.samples[1].split, "train");
        update_partitions(
            &mut data,
            vec![Partition {
                id: "two".into(),
                split: "val".into(),
                batch: "independent-visit".into(),
            }],
        )
        .unwrap();
        assert_eq!(data.samples[0].batch, "shared-visit");
        assert_eq!(data.samples[1].batch, "independent-visit");
    }
    #[test]
    fn class_edit_updates_references_and_delete_requires_review() {
        let mut data = fixture();
        edit_class(
            &mut data,
            "challenge",
            Some(ButtonClass {
                id: "challenge_button".into(),
                name: "开始挑战".into(),
                anchor: [0.4, 0.6],
            }),
        )
        .unwrap();
        assert!(
            data.samples
                .iter()
                .all(|s| s.boxes[0].label == "challenge_button")
        );
        assert_eq!(data.classes[0].name, "开始挑战");
        edit_class(&mut data, "challenge_button", None).unwrap();
        assert!(data.classes.is_empty());
        assert_eq!(data.samples.len(), 2);
        assert!(
            data.samples
                .iter()
                .all(|s| s.boxes.is_empty() && !s.reviewed)
        );
    }
    #[test]
    fn invalid_bulk_update_does_not_partially_mutate_dataset() {
        let mut data = fixture();
        assert!(
            update_partitions(
                &mut data,
                vec![
                    Partition {
                        id: "one".into(),
                        split: "val".into(),
                        batch: "new".into()
                    },
                    Partition {
                        id: "missing".into(),
                        split: "val".into(),
                        batch: "new".into()
                    }
                ]
            )
            .is_err()
        );
        assert_eq!(data.samples[0].split, "train");
        assert_eq!(data.samples[0].batch, "shared-visit");
    }
    #[test]
    fn builtin_names_match_model_codes_without_changing_their_order() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/detector");
        let names = classes(&root);
        let spec = read(root.join("model.json"));
        let ids: Vec<_> = spec["classes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(names.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ids);
        assert_eq!(
            names.iter().find(|c| c.id == "challenge").unwrap().name,
            "挑战"
        );
        assert!(
            names
                .iter()
                .find(|c| c.id == "soul_title")
                .unwrap()
                .name
                .contains("标记")
        );
    }
    #[test]
    fn rejects_invalid_boxes_and_accepts_reviewed_negative() {
        let classes = vec![ButtonClass {
            id: "challenge".into(),
            name: "挑战".into(),
            anchor: [0.5, 0.5],
        }];
        let mut sample = Sample {
            id: "test".into(),
            name: "test".into(),
            batch: "visit".into(),
            split: "val".into(),
            reviewed: true,
            width: 1920,
            height: 1080,
            boxes: vec![],
        };
        assert!(validate(&classes, &sample).is_ok());
        sample.boxes.push(BoxLabel {
            label: "challenge".into(),
            bounds: [1800., 900., 2000., 1000.],
        });
        assert!(validate(&classes, &sample).is_err());
        sample.boxes[0].bounds = [1800., 900., 1900., 1000.];
        assert!(validate(&classes, &sample).is_ok());
        sample.boxes[0].label = "missing".into();
        assert!(validate(&classes, &sample).is_err());
    }
    #[test]
    fn imports_preserve_original_dimensions_and_start_unreviewed() {
        let root =
            std::env::temp_dir().join(format!("button-studio-test-{}", uuid::Uuid::new_v4()));
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(320, 180)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let id = import(
            &root,
            bytes.get_ref(),
            "example.png".into(),
            "visit-1".into(),
        )
        .unwrap();
        let data = dataset(&root).unwrap();
        let row = &data.samples[0];
        assert_eq!((row.width, row.height), (320, 180));
        assert!(!row.reviewed);
        assert!(row.boxes.is_empty());
        assert!(root.join("images").join(format!("{id}.png")).is_file());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[ignore = "requires the button worker smoke export and NVIDIA hardware"]
    fn exported_button_model_loads_and_runs_on_gpu() {
        let resources = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
        let smoke = training().join("button-smoke");
        let mut model = Detector::load_from(&smoke.join("job/export"), &resources).unwrap();
        let first = fs::read_dir(smoke.join("images"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let bytes = fs::read(first).unwrap();
        let result = model
            .observe_threshold(&bytes, &CancelSignal::new(), 0.05)
            .unwrap();
        assert_eq!(result.compute.provider, "CUDA");
        assert_eq!(result.dimensions, [1920, 1080]);
        for item in result.detections {
            assert_eq!(item.class, "challenge");
            assert!(item.bounds[2] <= 1920. && item.bounds[3] <= 1080.);
        }
    }
}
