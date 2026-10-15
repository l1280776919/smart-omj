//! Soul eleven combines independent scene evidence, button detections and explicit layer evidence.
use crate::{device::CancelSignal, vision::{decode, find, TemplateSpec, detector::{Detector,Detection,Observation,decode_rgb},scene::Classifier}};
use std::path::Path;
pub struct Observer { buttons:Detector, scene:Classifier, templates:Vec<(TemplateSpec,image::GrayImage)> }
impl Observer {
 fn templates(resources:&Path)->Result<Vec<(TemplateSpec,image::GrayImage)>,String>{
  let mut templates=Vec::new();
  for (id,file,roi) in [
   ("layer11_selected","o_orochi_check_11.png",[896,100,269,84]),
   ("layer10_selected","o_orochi_check_10.png",[896,100,269,84]),
   ("layer12_selected","o_orochi_check_12.png",[896,100,269,84]),
   ("layer13_selected","o_orochi_check_13.png",[896,100,269,84]),
   ("formation_locked","soul11-lock.png",[670,635,90,65]),
   ("formation_unlocked","soul11-unlock.png",[670,635,90,65]),
  ] {
   let bytes=std::fs::read(resources.join("templates").join(file)).map_err(|e|e.to_string())?;
   templates.push((TemplateSpec{id:id.into(),file:file.into(),roi,threshold:0.93},decode(&bytes)?));
  }
  Ok(templates)
 }
 pub fn load(resources:&Path,buttons:&Path)->Result<Self,String>{
  Ok(Self{buttons:Detector::load_from(buttons,resources)?,scene:Classifier::load(&resources.join("scene"),resources)?,templates:Self::templates(resources)?})
 }
 pub fn observe(&mut self,bytes:&[u8],cancel:&CancelSignal)->Result<Observation,String>{
  let decode_start=std::time::Instant::now();
  let rgb=decode_rgb(bytes)?;
  let decode_ms=decode_start.elapsed().as_secs_f64()*1000.;
  let mut o=self.observe_rgb(&rgb,cancel)?;
  o.timings.decode_ms=decode_ms;
  Ok(o)
 }
 pub fn observe_rgb(&mut self,rgb:&image::RgbImage,cancel:&CancelSignal)->Result<Observation,String>{
  let mut o=self.buttons.observe_rgb(rgb,cancel)?;
  // This profile has only been verified for landscape 16:9. Never stretch another viewport into a match.
  let [w,h]=o.dimensions;
  if (w as f32/h as f32-16.0/9.0).abs()>0.02{return Err("悲鸣流程目前需要横屏 16:9 原始游戏画面".into());}
  let scene=self.scene.predict_rgb(rgb)?;
  o.timings.scene_ms=scene.elapsed_ms;
  if cancel.is_cancelled(){return Err("用户停止".into());}
  if !scene.uncertain && scene.confidence>=0.90 {
   o.detections.push(Detection{class:format!("scene_{}",scene.class),confidence:scene.confidence,bounds:[0.,0.,2.,2.]});
  }
  if scene.class=="orochi" && scene.confidence>=0.90 {
   let template_start=std::time::Instant::now();
   o.detections.extend(match_regions(rgb,&self.templates,cancel)?);
   o.timings.template_ms=template_start.elapsed().as_secs_f64()*1000.;
  }
  Ok(o)
 }
}

fn match_regions(rgb:&image::RgbImage,templates:&[(TemplateSpec,image::GrayImage)],cancel:&CancelSignal)->Result<Vec<Detection>,String>{
 let (w,h)=rgb.dimensions();
 let mut regions=std::collections::HashMap::new();
 let mut found=Vec::new();
 for (spec,template) in templates {
  let [x,y,rw,rh]=spec.roi;
  let screen=regions.entry(spec.roi).or_insert_with(||{
   let sx=(x as f64*w as f64/1280.).round() as u32;
   let sy=(y as f64*h as f64/720.).round() as u32;
   let sw=((x+rw) as f64*w as f64/1280.).round() as u32-sx;
   let sh=((y+rh) as f64*h as f64/720.).round() as u32-sy;
   let crop=image::imageops::crop_imm(rgb,sx,sy,sw,sh).to_image();
   let gray=image::DynamicImage::ImageRgb8(crop).to_luma8();
   image::imageops::resize(&gray,rw,rh,image::imageops::FilterType::Triangle)
  });
  let mut local=spec.clone();local.roi=[0,0,rw,rh];
  let m=find(screen,template,&local,cancel)?;
  if m.found {let [mx,my,tw,th]=m.bounds;found.push(Detection{class:spec.id.clone(),confidence:m.score,bounds:[(x+mx) as f32*w as f32/1280.,(y+my) as f32*h as f32/720.,(x+mx+tw) as f32*w as f32/1280.,(y+my+th) as f32*h as f32/720.]});}
 }
 Ok(found)
}

#[cfg(test)]
mod tests {
 use super::*;
 #[test]
 fn real_open_and_closed_locks_are_not_interchanged() {
  let root=Path::new(env!("CARGO_MANIFEST_DIR"));
  let templates=Observer::templates(&root.join("resources")).unwrap();
  for (file,expected) in [("locked-challenge.png","formation_locked"),("live.png","formation_unlocked")] {
   let bytes=std::fs::read(root.join("../training/soul11-review").join(file)).unwrap();
   let screen=image::imageops::resize(&decode(&bytes).unwrap(),1280,720,image::imageops::FilterType::Triangle);
   let matches:Vec<_>=templates.iter().filter(|(spec,_)|spec.id.starts_with("formation_")).filter_map(|(spec,template)| {
    let m=find(&screen,template,spec,&CancelSignal::new()).unwrap();
    m.found.then_some(spec.id.as_str())
   }).collect();
   assert_eq!(matches,vec![expected],"{file}");
   let rgb=decode_rgb(&bytes).unwrap();
   let optimized=match_regions(&rgb,&templates,&CancelSignal::new()).unwrap();
   assert_eq!(optimized.iter().filter(|d|d.class.starts_with("formation_")).map(|d|d.class.as_str()).collect::<Vec<_>>(),vec![expected]);
   assert!(optimized.iter().any(|d|d.class=="layer11_selected"));
  }
 }
}
