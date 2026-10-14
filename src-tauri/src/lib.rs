#[tauri::command]
fn desktop_status() -> &'static str {
    "Tauri 桌面通信正常。OAS 引擎尚未接入。"
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![desktop_status])
        .run(tauri::generate_context!())
        .expect("error while running Smart OMJ");
}
