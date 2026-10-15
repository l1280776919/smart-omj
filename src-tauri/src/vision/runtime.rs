//! One process-wide ONNX environment, shared by every model session.
use ort::environment::Environment;
use std::{path::Path, sync::Mutex};

// Keep a strong reference even when all model sessions are unloaded. ort rc.14
// does not allow creating a second environment after dropping the first one.
static ENVIRONMENT: Mutex<Option<Environment>> = Mutex::new(None);

#[derive(Clone, Debug, serde::Serialize)]
pub struct Compute {
    pub provider: String,
    pub fallback_reason: Option<String>,
}

/// Explicit registration errors must not be swallowed and reported as GPU use.
pub fn session(resources: &Path, model: &Path) -> Result<(ort::session::Session, Compute), String> {
    let env = environment(resources)?;
    let gpu = (|| -> Result<ort::session::Session, String> {
        let root = resources.join("gpu");
        ort::ep::cuda::preload_dylibs(Some(&root), Some(&root)).map_err(|e| e.to_string())?;
        // cuDNN 9.24 adds sublibraries not yet listed by ort rc.14.
        for name in ["nvrtc64_130_0.dll", "cudnn_engines_tensor_ir64_9.dll", "cudnn_ext64_9.dll"] {
            let path = root.join(name);
            if path.is_file() { ort::util::preload_dylib(path).map_err(|e| e.to_string())?; }
        }
        ort::session::Session::builder(&env).map_err(|e| e.to_string())?
            .with_execution_providers([ort::ep::CUDA::default().build().error_on_failure()]).map_err(|e| e.to_string())?
            .commit_from_file(model).map_err(|e| e.to_string())
    })();
    match gpu {
        Ok(session) => Ok((session, Compute { provider: "CUDA".into(), fallback_reason: None })),
        Err(reason) => {
            eprintln!("CUDA unavailable, using CPU: {reason}");
            let session = ort::session::Session::builder(&env).map_err(|e| e.to_string())?
                .with_intra_threads(4).map_err(|e| e.to_string())?
                .commit_from_file(model).map_err(|e| e.to_string())?;
            Ok((session, Compute { provider: "CPU".into(), fallback_reason: Some(reason) }))
        }
    }
}

pub fn environment(resources: &Path) -> Result<Environment, String> {
    let mut slot = ENVIRONMENT.lock().map_err(|e| e.to_string())?;
    if let Some(environment) = slot.as_ref() {
        return Ok(environment.clone());
    }
    let gpu_runtime = resources.join("gpu/onnxruntime.dll");
    let runtime = if gpu_runtime.is_file() { gpu_runtime } else { resources.join("detector/onnxruntime.dll") };
    let environment = ort::init_from(runtime)
        .map_err(|e| format!("ONNX Runtime 加载失败：{e}"))?
        .build()
        .map_err(|e| format!("ONNX Runtime 初始化失败：{e}"))?;
    *slot = Some(environment.clone());
    Ok(environment)
}

#[cfg(test)]
mod tests {
    use crate::vision::{detector::Detector, scene::Classifier};

    #[test]
    #[ignore = "requires staged NVIDIA CUDA runtime and hardware"]
    fn staged_gpu_runs_scene_and_detector() {
        let resources = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
        let bytes = std::fs::read(resources.join("scene/yard.png")).unwrap();
        let mut scene = Classifier::load(&resources.join("scene"), &resources).unwrap();
        let result = scene.predict(&bytes).unwrap();
        assert_eq!(result.compute.provider, "CUDA", "{:?}", result.compute);
        let mut detector = Detector::load(&resources).unwrap();
        let result = detector.observe(&bytes, &crate::device::CancelSignal::new()).unwrap();
        assert_eq!(result.compute.provider, "CUDA", "{:?}", result.compute);
    }

    #[test]
    fn multiple_models_and_reloads_share_the_environment() {
        let resources = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
        // Model validation during activation may overlap with another model load.
        std::thread::scope(|scope| {
            let scene = scope.spawn(|| Classifier::load(&resources.join("scene"), &resources).unwrap());
            let detector = scope.spawn(|| Detector::load(&resources).unwrap());
            drop(scene.join().unwrap());
            drop(detector.join().unwrap());
        });
        // All sessions have been dropped; a subsequent load must still work.
        for _ in 0..2 {
            let mut model = Classifier::load(&resources.join("scene"), &resources).unwrap();
            let image = std::fs::read(resources.join("scene/yard.png")).unwrap();
            assert_eq!(model.predict(&image).unwrap().class, "yard");
        }
    }
}
