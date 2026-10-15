//! Read-only CLI diagnostics, sharing the actual device and recognition implementation.
//! Not an IPC endpoint: executable/output paths never cross the frontend boundary.
use crate::{
    device::{Adb, CancelSignal},
    vision::{self, Templates},
};
use std::path::Path;

pub async fn run(
    resources: &Path,
    output: &Path,
    serial: Option<&str>,
) -> Result<serde_json::Value, String> {
    let adb = Adb {
        executable: resources.join("adb/adb.exe"),
    };
    let cancel = CancelSignal::new();
    let devices = adb.devices(&cancel).await?;
    let mut report = serde_json::json!({ "devices": devices, "selected_serial": serial });
    if let Some(serial) = serial {
        let bytes = adb.screenshot(serial, &cancel).await?;
        let image = vision::decode(&bytes)?;
        std::fs::create_dir_all(output).map_err(|e| e.to_string())?;
        let screenshot = output.join("capture.png");
        std::fs::write(&screenshot, &bytes).map_err(|e| e.to_string())?;
        report["screenshot"] = serde_json::json!({ "path": screenshot, "width": image.width(), "height": image.height() });
        let resources = resources.to_path_buf();
        let result = tokio::task::spawn_blocking(move || {
            Templates::load(&resources)?.diagnose(&bytes, &cancel)
        })
        .await
        .map_err(|e| e.to_string())?;
        match result {
            Ok(diagnosis) => {
                report["recognition"] =
                    serde_json::to_value(diagnosis).map_err(|e| e.to_string())?
            }
            Err(error) => report["recognition_error"] = error.into(),
        }
        std::fs::write(
            output.join("report.json"),
            serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(report)
}
