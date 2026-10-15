//! Resident YOLO/ONNX detector. Boxes are returned in source-device pixels.
use crate::device::CancelSignal;
use image::{ImageReader, RgbImage};
use ort::{session::Session, value::Tensor};
use serde::{Deserialize, Serialize};
use std::{io::Cursor, path::Path, time::Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Detection {
    pub class: String,
    pub confidence: f32,
    pub bounds: [f32; 4], // xyxy, device pixels
}
impl Detection {
    pub fn center(&self) -> [u32; 2] {
        [((self.bounds[0] + self.bounds[2]) / 2.) as u32,
         ((self.bounds[1] + self.bounds[3]) / 2.) as u32]
    }
}
#[derive(Debug, Clone, Serialize, Default)]
pub struct Timings {
    pub screenshot_ms: f64,
    pub capture_mode: String,
    pub capture_read_ms: f64,
    pub capture_encode_ms: f64,
    pub decode_ms: f64,
    pub recognition_ms: f64,
    pub scene_ms: f64,
    pub template_ms: f64,
    pub click_ms: f64,
    pub screenshot_write_ms: f64,
    pub preprocess_ms: f64,
    pub inference_ms: f64,
    pub postprocess_ms: f64,
    pub decision_ms: f64,
    pub total_ms: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct Observation {
    pub compute: super::runtime::Compute,
    pub dimensions: [u32; 2],
    pub detections: Vec<Detection>,
    pub timings: Timings,
}
#[derive(Deserialize)]
struct ModelSpec {
    input_size: u32,
    classes: Vec<String>,
    confidence: f32,
    nms_iou: f32,
}
pub struct Detector {
    compute: super::runtime::Compute,
    session: Session,
    spec: ModelSpec,
}

#[derive(Debug, Clone, Copy)]
pub struct Letterbox {
    pub source: [u32; 2],
    pub resized: [u32; 2],
    pub padding: [u32; 2],
}
impl Letterbox {
    pub fn new(source: [u32; 2], size: u32) -> Result<Self, String> {
        if source.contains(&0) || size == 0 { return Err("图像尺寸为空".into()); }
        let scale = (size as f32 / source[0] as f32).min(size as f32 / source[1] as f32);
        let resized = source.map(|v| ((v as f32 * scale).round() as u32).clamp(1, size));
        Ok(Self { source, resized, padding: [(size-resized[0])/2, (size-resized[1])/2] })
    }
    pub fn restore(&self, xywh: [f32; 4]) -> Option<[f32; 4]> {
        if !xywh.iter().all(|v| v.is_finite()) || xywh[2] <= 0. || xywh[3] <= 0. { return None; }
        let [x,y,w,h] = xywh;
        let sx = self.source[0] as f32 / self.resized[0] as f32;
        let sy = self.source[1] as f32 / self.resized[1] as f32;
        let b = [
            ((x-w/2.-self.padding[0] as f32)*sx).clamp(0.,self.source[0] as f32),
            ((y-h/2.-self.padding[1] as f32)*sy).clamp(0.,self.source[1] as f32),
            ((x+w/2.-self.padding[0] as f32)*sx).clamp(0.,self.source[0] as f32),
            ((y+h/2.-self.padding[1] as f32)*sy).clamp(0.,self.source[1] as f32),
        ];
        (b[2]-b[0] >= 2. && b[3]-b[1] >= 2.).then_some(b)
    }
}
pub fn iou(a: [f32; 4], b: [f32; 4]) -> f32 {
    let intersection = (a[2].min(b[2])-a[0].max(b[0])).max(0.) * (a[3].min(b[3])-a[1].max(b[1])).max(0.);
    let union = (a[2]-a[0])*(a[3]-a[1])+(b[2]-b[0])*(b[3]-b[1])-intersection;
    if union <= 0. { 0. } else { intersection / union }
}
pub(crate) fn decode_rgb(bytes: &[u8]) -> Result<RgbImage, String> {
    let mut reader = ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(256*1024*1024);
    reader.limits(limits);
    reader.decode().map(|v| v.to_rgb8()).map_err(|e| format!("截图解码失败：{e}"))
}
impl Detector {
    pub fn load(resources: &Path) -> Result<Self, String> {
        let root = resources.join("detector");
        Self::load_from(&root, resources)
    }
    pub fn load_from(root: &Path, resources: &Path) -> Result<Self, String> {
        let spec: ModelSpec = serde_json::from_slice(&std::fs::read(root.join("model.json"))
            .map_err(|e| format!("检测模型清单缺失：{e}"))?).map_err(|e| e.to_string())?;
        if !(320..=1280).contains(&spec.input_size) || spec.classes.is_empty()
            || !spec.confidence.is_finite() || !(0.5..=1.).contains(&spec.confidence)
            || !(0.1..=0.9).contains(&spec.nms_iou) { return Err("检测模型配置无效".into()); }
        let (session, compute) = super::runtime::session(resources, &root.join("soul1.onnx"))?;
        Ok(Self { session, spec, compute })
    }
    pub fn observe(&mut self, bytes: &[u8], cancel: &CancelSignal) -> Result<Observation,String> {
        self.observe_threshold(bytes, cancel, self.spec.confidence)
    }
    pub fn observe_threshold(&mut self, bytes: &[u8], cancel: &CancelSignal, confidence: f32) -> Result<Observation,String> {
        let start=Instant::now();
        if cancel.is_cancelled(){return Err("识别已取消".into());}
        let rgb=decode_rgb(bytes)?;
        let decode_ms=start.elapsed().as_secs_f64()*1000.;
        let mut result=self.observe_rgb_threshold(&rgb,cancel,confidence)?;
        result.timings.decode_ms=decode_ms;
        result.timings.total_ms=start.elapsed().as_secs_f64()*1000.;
        Ok(result)
    }
    pub(crate) fn observe_rgb(&mut self,rgb:&RgbImage,cancel:&CancelSignal)->Result<Observation,String>{
        self.observe_rgb_threshold(rgb,cancel,self.spec.confidence)
    }
    fn observe_rgb_threshold(&mut self,rgb:&RgbImage,cancel:&CancelSignal,confidence:f32)->Result<Observation,String>{
        if !confidence.is_finite() || !(0.05..=1.).contains(&confidence) {return Err("置信度阈值无效".into());}
        let threshold=confidence;
        let start=Instant::now();
        if cancel.is_cancelled(){return Err("识别已取消".into());}
        let dimensions = [rgb.width(), rgb.height()];
        let size = self.spec.input_size;
        let map = Letterbox::new(dimensions,size)?;
        let resized = super::resize::linear(rgb,map.resized[0],map.resized[1])?;
        let plane = (size*size) as usize;
        let mut values = vec![114./255.; plane*3];
        for (x,y,pixel) in resized.enumerate_pixels() {
            let index = ((y+map.padding[1])*size+x+map.padding[0]) as usize;
            for c in 0..3 { values[c*plane+index] = f32::from(pixel[c])/255.; }
        }
        if cancel.is_cancelled() { return Err("识别已取消".into()); }
        let tensor = Tensor::from_array(([1usize,3,size as usize,size as usize],values)).map_err(|e| e.to_string())?;
        let preprocess_ms = start.elapsed().as_secs_f64()*1000.;
        let infer_start = Instant::now();
        let outputs = self.session.run(ort::inputs![tensor]).map_err(|e| format!("ONNX 推理失败：{e}"))?;
        let inference_ms = infer_start.elapsed().as_secs_f64()*1000.;
        if cancel.is_cancelled() { return Err("识别已取消".into()); }
        let post_start = Instant::now();
        let (shape, raw) = outputs[0].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
        if shape.len()!=3 || shape[0]!=1 || shape[1] as usize != 4+self.spec.classes.len() || shape[2]<=0 {
            return Err(format!("不支持的 YOLO 输出形状：{shape:?}，要求 [1,4+classes,anchors]"));
        }
        let n = shape[2] as usize;
        let mut detections = Vec::new();
        for anchor in 0..n {
            let best = (0..self.spec.classes.len()).map(|c|(c,raw[(c+4)*n+anchor]))
                .filter(|(_,score)|score.is_finite()).max_by(|a,b|a.1.total_cmp(&b.1));
            if let Some((class,confidence)) = best
                && confidence >= threshold && confidence<=1.
                && let Some(bounds) = map.restore([raw[anchor],raw[n+anchor],raw[2*n+anchor],raw[3*n+anchor]]) {
                    detections.push(Detection { class:self.spec.classes[class].clone(),confidence,bounds });
            }
        }
        detections.sort_by(|a,b|b.confidence.total_cmp(&a.confidence));
        let mut kept: Vec<Detection> = Vec::new();
        for detection in detections {
            if !kept.iter().any(|k| k.class==detection.class && iou(k.bounds,detection.bounds)>self.spec.nms_iou) {
                kept.push(detection);
            }
            if kept.len()>=100 { break; }
        }
        Ok(Observation { compute:self.compute.clone(), dimensions,detections:kept,timings:Timings {preprocess_ms,inference_ms,
            postprocess_ms:post_start.elapsed().as_secs_f64()*1000.,total_ms:start.elapsed().as_secs_f64()*1000.,..Default::default()} })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn letterbox_roundtrip_and_padding_do_not_create_clicks() {
        for source in [[1920,1080],[1024,768],[2340,1080],[720,1280]] {
            let map=Letterbox::new(source,640).unwrap();
            let b=map.restore([map.padding[0] as f32+map.resized[0] as f32/2.,map.padding[1] as f32+map.resized[1] as f32/2.,map.resized[0] as f32,map.resized[1] as f32]).unwrap();
            assert!((b[2]-source[0] as f32).abs()<1.);
            assert!((b[3]-source[1] as f32).abs()<1.);
        }
        assert!(Letterbox::new([1920,1080],640).unwrap().restore([50.,20.,20.,20.]).is_none());
        assert!(Letterbox::new([0,1],640).is_err());
    }
}
