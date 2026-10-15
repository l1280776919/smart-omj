use super::detector::Letterbox;
use ort::{session::Session, value::Tensor};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path, time::Instant};

#[derive(Clone, Deserialize, Serialize)]
pub struct Spec {
    pub classes: BTreeMap<String, String>,
    pub input_size: u32,
    pub top1: f64,
    pub limitation: String,
}
#[derive(Serialize)]
pub struct Score { pub class: String, pub confidence: f32 }
#[derive(Serialize)]
pub struct Prediction {
    pub compute: super::runtime::Compute,
    pub class: String,
    pub confidence: f32,
    pub uncertain: bool,
    pub scores: Vec<Score>,
    pub elapsed_ms: f64,
}
pub struct Classifier { session: Session, spec: Spec, compute: super::runtime::Compute }
impl Classifier {
    pub fn load(root: &Path, resources: &Path) -> Result<Self, String> {
        let spec: Spec = serde_json::from_slice(&std::fs::read(root.join("scene_model.json")).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        if spec.input_size != 224 || spec.classes.len() < 2 || (0..spec.classes.len()).any(|i|!spec.classes.contains_key(&i.to_string())) {
            return Err("场景模型配置无效，需要连续类别编号与 224 输入尺寸".into());
        }
        let (session, compute) = super::runtime::session(resources, &root.join("best.onnx"))?;
        Ok(Self { session, spec, compute })
    }
    pub fn predict(&mut self, bytes: &[u8]) -> Result<Prediction, String> {
        let start = Instant::now();
        let rgb = image::load_from_memory(bytes).map_err(|e|e.to_string())?.to_rgb8();
        let mut prediction=self.predict_rgb(&rgb)?;
        prediction.elapsed_ms=start.elapsed().as_secs_f64()*1000.;
        Ok(prediction)
    }
    pub(crate) fn predict_rgb(&mut self,rgb:&image::RgbImage)->Result<Prediction,String>{
        let start=Instant::now();
        let size = self.spec.input_size;
        let map = Letterbox::new([rgb.width(),rgb.height()],size)?;
        let resized = super::resize::linear(rgb,map.resized[0],map.resized[1])?;
        let plane = (size*size) as usize;
        let mut values = vec![114./255.;plane*3];
        for (x,y,p) in resized.enumerate_pixels() {
            let i=((y+map.padding[1])*size+x+map.padding[0]) as usize;
            for c in 0..3 { values[c*plane+i]=f32::from(p[c])/255.; }
        }
        let tensor=Tensor::from_array(([1usize,3,size as usize,size as usize],values)).map_err(|e|e.to_string())?;
        let outputs=self.session.run(ort::inputs![tensor]).map_err(|e|e.to_string())?;
        let (shape,raw)=outputs[0].try_extract_tensor::<f32>().map_err(|e|e.to_string())?;
        if shape.len()!=2 || shape[0]!=1 || raw.len()!=self.spec.classes.len() || raw.iter().any(|v|!v.is_finite() || !(0.0..=1.0).contains(v)) {
            return Err("场景模型输出不合法".into());
        }
        let mut scores:Vec<_>=raw.iter().enumerate().map(|(i,v)|Score{class:self.spec.classes[&i.to_string()].clone(),confidence:*v}).collect();
        scores.sort_by(|a,b|b.confidence.total_cmp(&a.confidence));
        Ok(Prediction { compute:self.compute.clone(), class:scores[0].class.clone(),confidence:scores[0].confidence,uncertain:scores[0].confidence<0.8,scores,elapsed_ms:start.elapsed().as_secs_f64()*1000. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_classifier_recognizes_all_eight_scenes() {
        let resources=std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
        let mut model=Classifier::load(&resources.join("scene"), &resources).unwrap();
        for label in ["yard","explore","fail","fight","mitama","orochi","ready","results"] {
            let bytes=std::fs::read(resources.join(format!("scene/{label}.png"))).unwrap();
            let result=model.predict(&bytes).unwrap();
            assert_eq!(result.class,label);
            assert!(result.confidence>0.8);
        }
        assert!(model.predict(b"invalid image").is_err());
    }
}
