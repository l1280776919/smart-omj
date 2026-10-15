//! Shared desktop/CLI closed loop. One outstanding capture and inference per device.
use crate::{device::{Adb,CancelSignal},tasks::soul_graph::{Controller,Decision,Page,classify},vision::detector::{Detector,Observation}};
use serde::Serialize;
use std::{io::Write,path::{Path,PathBuf},sync::{Arc,Mutex},time::{Duration,Instant}};

pub type DetectorCache=Arc<Mutex<Option<Detector>>>;
pub fn cache()->DetectorCache {Arc::new(Mutex::new(None))}
pub async fn infer(cache:DetectorCache,resources:PathBuf,bytes:Vec<u8>,cancel:CancelSignal)->Result<Observation,String> {
    tokio::task::spawn_blocking(move||{
        let mut slot=cache.lock().map_err(|e|e.to_string())?;
        if cancel.is_cancelled(){return Err("用户停止".into());}
        if slot.is_none(){*slot=Some(Detector::load(&resources)?);}
        slot.as_mut().unwrap().observe(&bytes,&cancel)
    }).await.map_err(|e|format!("识别线程失败：{e}"))?
}
#[derive(Serialize)]
pub struct Report {
    pub frame:u64,
    pub elapsed_ms:u64,
    pub page:Page,
    pub completed:u32,
    pub decision:Decision,
    pub observation:Observation,
    pub dispatched:bool,
    pub message:String,
    pub click_variation:Option<ClickVariation>,
}
#[derive(Debug, Serialize)]
pub struct ClickVariation {
    pub requested_delay_ms:u64,
    pub actual_delay_ms:f64,
    pub original_point:[u32;2],
    pub point:[u32;2],
}
fn click_variation(bounds:[f32;4],point:[u32;2],dimensions:[u32;2],random:[u32;3])->Result<ClickVariation,String>{
    if dimensions.contains(&0)||bounds.iter().any(|v|!v.is_finite()) {return Err("点击框无效".into());}
    let mut target=point;
    for axis in 0..2 {
        let lo=bounds[axis].max(0.);let hi=bounds[axis+2].min(dimensions[axis] as f32-1.);
        if hi<=lo || (point[axis] as f32)<lo || (point[axis] as f32)>hi {return Err("点击中心不在有效框内".into());}
        // Restrict jitter to the central half of the box, at most 10% per axis / 12 pixels.
        let radius=((hi-lo)*0.1).min(12.).floor() as i64;
        let lower=(lo+(hi-lo)*0.25).ceil() as i64;
        let upper=(hi-(hi-lo)*0.25).floor() as i64;
        if lower>upper {return Err("点击框过小".into());}
        let offset=(random[axis+1] as i64%(2*radius+1))-radius;
        target[axis]=(point[axis] as i64+offset).clamp(lower,upper) as u32;
    }
    Ok(ClickVariation{requested_delay_ms:120+(random[0]%161) as u64,actual_delay_ms:0.,original_point:point,point:target})
}
async fn click_wait(delay:u64,cancel:&CancelSignal)->Result<(),String>{
    tokio::select!{biased;_=cancel.cancelled()=>Err("用户停止".into()),_=tokio::time::sleep(Duration::from_millis(delay))=>Ok(())}
}
pub fn timing_log(report:&Report)->String {
    let t=&report.observation.timings;
    let action=match &report.decision { Decision::Tap(plan)=>format!("{} / {}",plan.target,if report.dispatched {"已点击"}else{"未派发"}),Decision::Complete=>"完成".into(),Decision::Wait=>"等待".into() };
    let capture_detail=if t.capture_mode=="mumu-ipc"{format!("（取帧含转换 {:.1} / PNG 编码 {:.1}）",t.capture_read_ms,t.capture_encode_ms)}else{String::new()};
    let action=if let Some(v)=&report.click_variation {format!("{action} · 随机等待 {}/{:.1} ms（计划/实际）· 点击坐标 {:?} → {:?}",v.requested_delay_ms,v.actual_delay_ms,v.original_point,v.point)}else{action};
    format!("帧 #{} · {} · {} · 截图 {} {:.1} ms · 识别合计 {:.1} ms（解码 {:.1} / 预处理 {:.1} / 按钮推理 {:.1} / 后处理 {:.1} / 场景 {:.1} / 模板 {:.1}）· 决策 {:.2} ms · 点击 {:.1} ms · 存图 {:.1} ms · 本帧合计 {:.1} ms · {}",
        report.frame,report.page.label(),report.observation.compute.provider,format!("{}{}",t.capture_mode,capture_detail),t.screenshot_ms,t.recognition_ms,t.decode_ms,t.preprocess_ms,t.inference_ms,t.postprocess_ms,t.scene_ms,t.template_ms,t.decision_ms,t.click_ms,t.screenshot_write_ms,t.total_ms,action)
}
pub struct Options {
    pub serial:String,
    pub layer:u32,
    pub button_model:Option<PathBuf>,
    pub rounds:u32,
    pub max_seconds:u64,
    pub stage_timeout_seconds:u64,
    pub observe_only:bool,
    pub max_frames:Option<u64>,
}
pub async fn run(
    adb:&Adb, resources:&Path, output:&Path, detector:DetectorCache,
    options:Options, cancel:&CancelSignal, mut progress:impl FnMut(&Report),
)->Result<u32,String> {
    crate::config::validate_serial(&options.serial)?;
    let lock_name=options.serial.replace([':', '.'],"_");
    let device_lock=std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false)
        .open(std::env::temp_dir().join(format!("smart-omj-{lock_name}.lock"))).map_err(|e|e.to_string())?;
    device_lock.try_lock().map_err(|_|"另一个进程正在控制此设备".to_string())?;
    if options.rounds==0||options.max_seconds==0{return Err("轮数和时间限制必须大于零".into());}
    std::fs::create_dir_all(output).map_err(|e|e.to_string())?;
    // Warm up/load before any observation is used for a click.
    let load_cache=detector.clone();let root=resources.to_path_buf();let token=cancel.clone();
    if options.layer==1 { tokio::task::spawn_blocking(move||{
        if token.is_cancelled(){return Err("用户停止".to_string());}
        let mut guard=load_cache.lock().map_err(|e|e.to_string())?;
        if guard.is_none(){*guard=Some(Detector::load(&root)?);}
        Ok::<_,String>(())
    }).await.map_err(|e|e.to_string())??; }
    if ![1,11].contains(&options.layer){return Err("当前闭环仅支持壹层与悲鸣".into());}
    let soul11 = if options.layer==11 {
        let root=resources.to_path_buf();let model=options.button_model.clone().ok_or("悲鸣流程需要启用按钮模型")?;
        Some(Arc::new(Mutex::new(tokio::task::spawn_blocking(move||crate::tasks::soul11::Observer::load(&root,&model)).await.map_err(|e|e.to_string())??)))
    } else {None};
    let mut trace=std::fs::OpenOptions::new().create(true).append(true).open(output.join("trace.jsonl")).map_err(|e|e.to_string())?;
    let mut native=if soul11.is_some(){Some(crate::device::mumu::Capture::discover(adb,&options.serial,cancel).await)}else{None};
    if let Some(capture)=&native {writeln!(trace,"{}",serde_json::json!({"event":"capture_backend","message":capture.status})).map_err(|e|e.to_string())?;}
    let start=Instant::now();let mut frame=0;let mut last_page=Page::Unknown;
    let mut controller=if options.layer==11 {Controller::soul11(options.rounds,options.stage_timeout_seconds*1000)}else{Controller::new(options.rounds,options.stage_timeout_seconds*1000)};
    let mut previous_dimensions=None;
    let result=async {
        loop {
            if cancel.is_cancelled(){return Err("用户停止".into());}
            if start.elapsed()>=Duration::from_secs(options.max_seconds){return Err(format!("达到运行时间限制；已完成 {} / {} 轮",controller.completed,options.rounds));}
            if options.max_frames.is_some_and(|n|frame>=n){return Ok(controller.completed);}
            let capture_start=Instant::now();
            let mut capture_read_ms=0.;let mut capture_encode_ms=0.;
            let (bytes,rgb,capture_mode)=if soul11.is_some(){
                let native_frame=if let Some(capture)=&mut native {
                    let before=capture.status.clone();let frame=capture.frame(cancel).await;
                    if capture.status!=before {writeln!(trace,"{}",serde_json::json!({"event":"capture_backend","message":capture.status})).map_err(|e|e.to_string())?;}
                    frame
                }else{None};
                let (rgb,png,mode)=if let Some((rgb,png,read,encode))=native_frame {
                    capture_read_ms=read;capture_encode_ms=encode;(rgb,png,"mumu-ipc".to_string())
                }else{adb.screenshot_fast(&options.serial,cancel).await?};
                (png,Some(rgb),mode)
            }else{(adb.screenshot(&options.serial,cancel).await?,None,"png".to_string())};
            let screenshot_ms=capture_start.elapsed().as_secs_f64()*1000.;
            let write_start=Instant::now();
            std::fs::write(output.join("latest.png"),&bytes).map_err(|e|e.to_string())?;
            let screenshot_write_ms=write_start.elapsed().as_secs_f64()*1000.;
            let recognition_start=Instant::now();
            let mut observation=if let Some(observer)=&soul11 {
                let observer=observer.clone();let rgb=rgb.ok_or("原始截图缺失")?;let cancel=cancel.clone();
                tokio::task::spawn_blocking(move||observer.lock().map_err(|e|e.to_string())?.observe_rgb(&rgb,&cancel)).await.map_err(|e|e.to_string())??
            }else{infer(detector.clone(),resources.to_path_buf(),bytes.clone(),cancel.clone()).await?};
            observation.timings.recognition_ms=recognition_start.elapsed().as_secs_f64()*1000.;
            observation.timings.screenshot_write_ms=screenshot_write_ms;
            observation.timings.screenshot_ms=screenshot_ms;
            observation.timings.capture_mode=capture_mode;
            observation.timings.capture_read_ms=capture_read_ms;
            observation.timings.capture_encode_ms=capture_encode_ms;
            if previous_dimensions.is_some_and(|size|size!=observation.dimensions){return Err("运行中分辨率或方向改变，已停止；请重新观察后启动".into());}
            previous_dimensions=Some(observation.dimensions);
            if cancel.is_cancelled(){return Err("用户停止".into());}
            if start.elapsed()>=Duration::from_secs(options.max_seconds){return Err("观察后达到时间限制，未派发动作".into());}
            let now=start.elapsed().as_millis() as u64;
            let decision_start=Instant::now();
            let choice=controller.tick(&observation,now);
            observation.timings.decision_ms=decision_start.elapsed().as_secs_f64()*1000.;
            observation.timings.total_ms=capture_start.elapsed().as_secs_f64()*1000.;
            let mut report=Report {frame,elapsed_ms:now,page:controller.page,completed:controller.completed,
                decision:choice.as_ref().cloned().unwrap_or(Decision::Wait),observation,dispatched:false,
                message:choice.as_ref().err().cloned().unwrap_or_default(),click_variation:None};
            if report.page!=last_page||matches!(report.decision,Decision::Tap(_))||choice.is_err() {
                let write_start=Instant::now();
                std::fs::write(output.join(format!("{frame:05}-{:?}.png",report.page)),&bytes).map_err(|e|e.to_string())?;
                report.observation.timings.screenshot_write_ms+=write_start.elapsed().as_secs_f64()*1000.;
            }
            last_page=report.page;
            if let Decision::Tap(plan)=&report.decision
                && !options.observe_only && choice.is_ok() {
                if capture_start.elapsed()>Duration::from_millis(1500) {
                    report.message="截图过期，本帧动作已丢弃".into();
                } else {
                    let random=uuid::Uuid::new_v4();let bytes=random.as_bytes();
                    let random=std::array::from_fn(|i|u32::from_le_bytes(bytes[i*4..i*4+4].try_into().unwrap()));
                    let mut variation=click_variation(plan.bounds,plan.point,report.observation.dimensions,random)?;
                    let wait_start=Instant::now();
                    click_wait(variation.requested_delay_ms,cancel).await?;
                    variation.actual_delay_ms=wait_start.elapsed().as_secs_f64()*1000.;
                    let point=variation.point;report.click_variation=Some(variation);
                    if start.elapsed()>=Duration::from_secs(options.max_seconds){return Err("随机等待后达到运行时间限制，未派发动作".into());}
                    if capture_start.elapsed()>Duration::from_millis(1500){
                        report.message="随机等待后截图过期，本帧动作已丢弃".into();
                    }else{
                    // Cancellation, freshness and bounds checked again at dispatch.
                    let click_start=Instant::now();
                    adb.tap_observed(&options.serial,point,report.observation.dimensions,capture_start,cancel).await?;
                    report.observation.timings.click_ms=click_start.elapsed().as_secs_f64()*1000.;
                    controller.dispatched(plan.clone(),start.elapsed().as_millis() as u64);
                    report.dispatched=true;
                    }
                }
            }
            report.observation.timings.total_ms=capture_start.elapsed().as_secs_f64()*1000.;
            serde_json::to_writer(&mut trace,&report).map_err(|e|e.to_string())?;
            writeln!(trace).map_err(|e|e.to_string())?;trace.flush().map_err(|e|e.to_string())?;
            progress(&report);
            choice?;
            if matches!(report.decision,Decision::Complete){return Ok(controller.completed);}
            frame+=1;
            // interval is capture-start to capture-start, not an extra delay after processing.
            let remaining=poll_delay(controller.interval_ms(),capture_start.elapsed());
            tokio::select! {biased;_=cancel.cancelled()=>return Err("用户停止".into()),_=tokio::time::sleep(remaining)=>{}}
        }
    }.await;
    if let Err(message)=&result {
        let _=std::fs::copy(output.join("latest.png"),output.join("failure.png"));
        let _=writeln!(trace,"{}",serde_json::json!({"event":"stopped","reason":message,"completed":controller.completed}));
    }
    std::fs::write(output.join("result.json"),serde_json::to_vec_pretty(&serde_json::json!({
        "success":result.is_ok(),"observe_only":options.observe_only,"layer":options.layer,"button_model":options.button_model,"completed":controller.completed,
        "requested":options.rounds,"elapsed_ms":start.elapsed().as_millis(),"error":result.as_ref().err()
    })).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    result
}

/// File replay uses the same RGB preprocessing, ONNX session and page classifier.
pub fn inspect_file(resources:&Path,path:&Path)->Result<serde_json::Value,String> {
    let bytes=std::fs::read(path).map_err(|e|e.to_string())?;
    let observation=Detector::load(resources)?.observe(&bytes,&CancelSignal::new())?;
    Ok(serde_json::json!({"page":classify(&observation),"observation":observation}))
}

pub async fn cli(resources:&Path,args:&[String])->Result<(),String> {
    if args.len()!=4 || !matches!(args[0].as_str(),"run"|"observe"|"run11"|"observe11") {
        return Err("Usage: soul_loop file <png> | observe[11] <serial> <frames> <output-dir> | run[11] <serial> <rounds> <output-dir>. Ctrl+C or create output-dir/STOP to cancel.".into());
    }
    let number=args[2].parse::<u32>().map_err(|_|"次数必须为正整数")?;
    if number==0||number>1000{return Err("次数必须在1–1000之间".into());}
    let output=PathBuf::from(&args[3]);
    std::fs::create_dir_all(&output).map_err(|e|e.to_string())?;
    if output.join("STOP").exists(){return Err("STOP 文件仍存在，请先移除再启动".into());}
    let cancel=CancelSignal::new();let token=cancel.clone();let stop=output.join("STOP");
    let monitor=tokio::spawn(async move {
        loop {
            tokio::select! {_=tokio::signal::ctrl_c()=>{token.cancel();break;},_=tokio::time::sleep(Duration::from_millis(100))=>{
                if stop.exists(){token.cancel();break;}
            }}
        }
    });
    let adb=Adb {executable:resources.join("adb/adb.exe")};
    let layer=if args[0].ends_with("11"){11}else{1};
    let active=std::env::var("APPDATA").ok().map(|p|PathBuf::from(p).join("com.smartomj.desktop/button-studio/active.json"));
    let button_model=active.and_then(|p|std::fs::read(p).ok()).and_then(|b|serde_json::from_slice::<serde_json::Value>(&b).ok()).and_then(|v|v["directory"].as_str().map(PathBuf::from));
    let options=Options {serial:args[1].clone(),layer,button_model,rounds:if args[0].starts_with("run") {number}else{1},
        max_seconds:600,stage_timeout_seconds:180,observe_only:args[0].starts_with("observe"),
        max_frames:(args[0].starts_with("observe")).then_some(number as u64)};
    let result=run(&adb,resources,&output,cache(),options,&cancel,|report|{
        println!("{}",serde_json::to_string(report).unwrap());
    }).await;
    monitor.abort();result.map(|_|())
}

fn poll_delay(interval_ms:u64,elapsed:Duration)->Duration {Duration::from_millis(interval_ms).saturating_sub(elapsed)}
#[cfg(test)]
mod timing_tests {
 use super::*;
 #[test]
 fn cadence_subtracts_processing_and_never_queues_overdue_frames(){
  assert_eq!(poll_delay(1500,Duration::from_millis(930)),Duration::from_millis(570));
  assert_eq!(poll_delay(350,Duration::from_millis(1000)),Duration::ZERO);
 }
 #[test]
 fn random_click_stays_inside_large_thin_and_edge_buttons(){
  for (bounds,center) in [([1675.,830.,1899.,1054.],[1787,942]),([830.,1016.,1092.,1070.],[961,1043]),([0.,0.,4.,4.],[2,2])] {
   for seed in 0..1000 {
    let v=click_variation(bounds,center,[1920,1080],[seed,seed*7919,seed*104729]).unwrap();
    assert!((120..=280).contains(&v.requested_delay_ms));
    for axis in 0..2 {assert!(v.point[axis].abs_diff(center[axis])<=12);assert!(v.point[axis] as f32>bounds[axis]);assert!((v.point[axis] as f32)<bounds[axis+2]);}
   }
  }
  assert!(click_variation([0.,0.,0.,5.],[0,2],[1920,1080],[0;3]).is_err());
  assert!(click_variation([f32::NAN,0.,5.,5.],[2,2],[1920,1080],[0;3]).is_err());
  assert!(click_variation([10.,10.,20.,20.],[1,1],[1920,1080],[0;3]).is_err());
 }
 #[tokio::test]
 async fn stop_interrupts_click_wait(){
  let cancel=CancelSignal::new();let token=cancel.clone();
  tokio::spawn(async move{tokio::time::sleep(Duration::from_millis(10)).await;token.cancel();});
  let result=tokio::time::timeout(Duration::from_millis(200),click_wait(1000,&cancel)).await;
  assert!(result.unwrap().is_err());
 }
}
