use crate::{app::AppState, device::CancelSignal, vision::scene::{Classifier, Prediction}};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::{Child, Command, Stdio}, sync::Mutex};
use tauri::State;

struct Job { child: Child, directory: PathBuf, action: String, status: String }
#[derive(Default)]
pub struct Studio { job: Mutex<Option<Job>>, cache: Mutex<Option<(PathBuf,Classifier)>> }
impl Studio {
    pub fn shutdown(&self) {
        if let Ok(mut slot)=self.job.lock() {
            if let Some(job)=slot.as_mut() {
                if job.status=="running" { let _=job.child.kill();let _=job.child.wait();job.status="cancelled".into(); }
            }
        }
    }
}
fn root() -> PathBuf { PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("training") }
fn read(path: impl AsRef<std::path::Path>) -> Value {
    fs::read(path).ok().and_then(|b|serde_json::from_slice(&b).ok()).unwrap_or(Value::Null)
}
fn active(state: &AppState) -> PathBuf {
    let pointer=read(state.data.join("scene-active.json"));
    pointer["directory"].as_str().map(PathBuf::from).unwrap_or_else(||state.resources.join("scene"))
}
fn refresh(job: &mut Job) -> Result<(),String> {
    if job.status=="running" {
        if let Some(exit)=job.child.try_wait().map_err(|e|e.to_string())? {
            job.status=if exit.success(){"completed"}else{"failed"}.into();
        }
    }
    Ok(())
}
#[tauri::command]
pub fn scene_status(state:State<'_,AppState>, studio:State<'_,Studio>) -> Result<Value,String> {
    let root=root();
    let mut classes=Vec::new();
    if let Ok(folders)=fs::read_dir(root.join("videos")) {
        for folder in folders.flatten().filter(|v|v.path().is_dir()) {
            let videos:Vec<_>=fs::read_dir(folder.path()).map_err(|e|e.to_string())?.flatten()
                .filter(|v|v.path().extension().and_then(|e|e.to_str()).is_some_and(|e|["mp4","mkv","mov","avi"].contains(&e.to_lowercase().as_str())))
                .map(|v|v.file_name().to_string_lossy().to_string()).collect();
            classes.push(json!({"name":folder.file_name().to_string_lossy(),"videos":videos}));
        }
    }
    classes.sort_by_key(|v|v["name"].as_str().unwrap_or("").to_owned());
    let mut guard=studio.job.lock().map_err(|e|e.to_string())?;
    let job=if let Some(job)=guard.as_mut() {
        refresh(job)?;
        let log=fs::read_to_string(job.directory.join("output.log")).unwrap_or_default();
        let tail=log.lines().rev().take(18).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
        json!({"action":job.action,"status":job.status,"directory":job.directory,"progress":read(job.directory.join("progress.json")),"result":read(job.directory.join("result.json")),"log":tail})
    } else {
        let saved=read(root.join("scene-last-job.json"));
        if let Some(path)=saved["directory"].as_str() {
            let dir=PathBuf::from(path);
            let result=read(dir.join("result.json"));
            let progress=read(dir.join("progress.json"));
            let completed=!result.is_null() || progress["stage"]=="素材已就绪";
            json!({"action":saved["action"],"status":if completed{"completed"}else{"interrupted"},"directory":dir,"progress":progress,"result":result,"log":""})
        } else {Value::Null}
    };
    Ok(json!({"root":root,"available":root.join(".venv/Scripts/python.exe").is_file() && root.join("scene_workbench.py").is_file(),"classes":classes,
        "prepared":read(root.join("scene-prepared.json")),"job":job,"model":read(active(&state).join("scene_model.json"))}))
}
#[tauri::command]
pub fn scene_start(studio:State<'_,Studio>, action:String, interval:f64, epochs:u32, device:Option<String>) -> Result<(),String> {
    let device=device.as_deref().unwrap_or("auto");
    if !["auto","cuda","cpu"].contains(&device) {return Err("计算设备无效".into());}
    if !["prepare","train"].contains(&action.as_str()) || !interval.is_finite() || !(0.1..=30.).contains(&interval) || !(1..=300).contains(&epochs) {return Err("训练参数无效".into());}
    let mut guard=studio.job.lock().map_err(|e|e.to_string())?;
    if let Some(job)=guard.as_mut() {refresh(job)?;if job.status=="running" {return Err("已有任务运行中".into());}}
    let root=root();
    let directory=root.join("scene-jobs").join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&directory).map_err(|e|e.to_string())?;
    let log=fs::File::create(directory.join("output.log")).map_err(|e|e.to_string())?;
    let mut command=Command::new(root.join(".venv/Scripts/python.exe"));
    command.arg("-u").arg(root.join("scene_workbench.py")).arg(&action).arg("--root").arg(&root)
        .arg("--job").arg(&directory).arg("--interval").arg(interval.to_string()).arg("--epochs").arg(epochs.to_string())
        .arg("--device").arg(device)
        .current_dir(&root).env("PYTHONIOENCODING","utf-8").stdin(Stdio::null()).stdout(log.try_clone().map_err(|e|e.to_string())?).stderr(log);
    #[cfg(windows)] { use std::os::windows::process::CommandExt; command.creation_flags(0x08000000); }
    let child=command.spawn().map_err(|e|format!("训练环境启动失败：{e}"))?;
    let _=crate::config::atomic_json(&root.join("scene-last-job.json"),&json!({"directory":directory,"action":action}));
    *guard=Some(Job{child,directory,action,status:"running".into()});
    Ok(())
}
#[tauri::command]
pub fn scene_cancel(studio:State<'_,Studio>) -> Result<(),String> {
    let mut guard=studio.job.lock().map_err(|e|e.to_string())?;
    if let Some(job)=guard.as_mut() {
        refresh(job)?;
        if job.status=="running" {job.child.kill().map_err(|e|e.to_string())?;job.child.wait().map_err(|e|e.to_string())?;job.status="cancelled".into();}
    }
    Ok(())
}
impl Drop for Studio {
    fn drop(&mut self) { if let Ok(slot)=self.job.get_mut() {if let Some(job)=slot.as_mut() {let _=job.child.kill();let _=job.child.wait();}} }
}
#[tauri::command]
pub fn scene_samples(page:usize) -> Result<Value,String> {
    let prepared=read(root().join("scene-prepared.json"));
    let dir=PathBuf::from(prepared["job"].as_str().ok_or("请先抽取图片")?);
    let manifest=read(dir.join("samples.json"));
    let samples=manifest.as_array().ok_or("素材清单不可读")?;
    let excluded=read(dir.join("excluded.json"));
    let items:Vec<_>=samples.iter().enumerate().skip(page.saturating_mul(12)).take(12).map(|(index,r)| {
        let name=PathBuf::from(r["image"].as_str().unwrap_or(""));
        let thumb=dir.join("thumbs").join(r["label"].as_str().unwrap_or("")).join(format!("{}.png",name.file_name().unwrap_or_default().to_string_lossy()));
        let data=fs::read(thumb).ok().map(|b|format!("data:image/png;base64,{}",STANDARD.encode(b)));
        json!({"index":index,"label":r["label"],"seconds":r["seconds"],"thumbnail":data,"excluded":excluded.as_array().is_some_and(|a|a.contains(&r["image"]))})
    }).collect();
    Ok(json!({"total":samples.len(),"items":items}))
}
#[tauri::command]
pub fn scene_set_excluded(studio:State<'_,Studio>,index:usize,excluded:bool) -> Result<(),String> {
    let mut guard=studio.job.lock().map_err(|e|e.to_string())?;
    if let Some(job)=guard.as_mut(){refresh(job)?;if job.status=="running"{return Err("请等待当前任务完成后再审核素材".into());}}
    let prepared=read(root().join("scene-prepared.json"));
    let dir=PathBuf::from(prepared["job"].as_str().ok_or("请先抽取图片")?);
    let manifest=read(dir.join("samples.json"));
    let image=manifest.as_array().and_then(|a|a.get(index)).and_then(|v|v["image"].as_str()).ok_or("图片编号不存在")?;
    let previous=read(dir.join("excluded.json"));
    let mut values=previous.as_array().cloned().unwrap_or_default();
    values.retain(|v|v.as_str()!=Some(image));
    if excluded{values.push(json!(image));}
    crate::config::atomic_json(&dir.join("excluded.json"),&values)
}
#[tauri::command]
pub fn scene_activate(state:State<'_,AppState>, studio:State<'_,Studio>) -> Result<(),String> {
    let guard=studio.job.lock().map_err(|e|e.to_string())?;
    let directory=if let Some(job)=guard.as_ref() {
        if job.status!="completed" || job.action!="train" {return Err("请等待训练成功完成".into());}
        job.directory.clone()
    } else {
        let saved=read(root().join("scene-last-job.json"));
        if saved["action"]!="train" {return Err("尚无训练结果".into());}
        PathBuf::from(saved["directory"].as_str().ok_or("尚无训练结果")?)
    };
    let result=read(directory.join("result.json"));
    let onnx=PathBuf::from(result["onnx"].as_str().ok_or("训练结果缺少导出模型")?);
    let source=onnx.parent().ok_or("模型路径无效")?.canonicalize().map_err(|e|e.to_string())?;
    if !source.starts_with(directory.canonicalize().map_err(|e|e.to_string())?) {return Err("模型不在当前任务目录内".into());}
    let model=Classifier::load(&source,&state.resources)?;
    drop(model);
    let dest=state.data.join("scene-models").join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&dest).map_err(|e|e.to_string())?;
    for file in ["best.onnx","scene_model.json"] {fs::copy(source.join(file),dest.join(file)).map_err(|e|e.to_string())?;}
    crate::config::atomic_json(&state.data.join("scene-active.json"),&json!({"directory":dest}))?;
    *studio.cache.lock().map_err(|e|e.to_string())?=None;
    Ok(())
}
#[tauri::command]
pub fn scene_open_folder(kind:String,studio:State<'_,Studio>) -> Result<(),String> {
    let path=match kind.as_str() {
        "videos"=>root().join("videos"),
        "prepared"=>PathBuf::from(read(root().join("scene-prepared.json"))["job"].as_str().ok_or("请先抽取素材")?).join("images"),
        "result"=> {
            let guard=studio.job.lock().map_err(|e|e.to_string())?;
            if let Some(job)=guard.as_ref(){job.directory.clone()}else{PathBuf::from(read(root().join("scene-last-job.json"))["directory"].as_str().ok_or("尚无任务")?)}
        },
        _=>return Err("目录类型无效".into())
    };
    if !path.is_dir(){return Err("目录不存在".into());}
    Command::new("explorer.exe").arg(path).spawn().map_err(|e|e.to_string())?;
    Ok(())
}
#[tauri::command]
pub async fn scene_recognize(state:State<'_,AppState>,studio:State<'_,Studio>,serial:String) -> Result<Value,String> {
    let _guard=state.operation.try_lock().map_err(|_|"设备正在使用，请稍后重试")?;
    let bytes=state.adb.screenshot(&serial,&CancelSignal::new()).await?;
    let directory=active(&state);
    // A dedicated blocking task keeps image decoding and ONNX off the async executor.
    let resources=state.resources.clone();
    let data_url=format!("data:image/png;base64,{}",STANDARD.encode(&bytes));
    let mut classifier={
        let mut cache=studio.cache.lock().map_err(|e|e.to_string())?;
        cache.take().filter(|(path,_)|*path==directory).map(|(_,model)|model)
    };
    let path=directory.clone();
    let (model,result)=tokio::task::spawn_blocking(move|| -> Result<(Classifier,Prediction),String> {
        let mut model=match classifier.take(){Some(model)=>model,None=>Classifier::load(&path,&resources)?};
        let result=model.predict(&bytes)?;
        Ok((model,result))
    }).await.map_err(|e|e.to_string())??;
    *studio.cache.lock().map_err(|e|e.to_string())?=Some((directory,model));
    Ok(json!({"prediction":result,"data_url":data_url}))
}
