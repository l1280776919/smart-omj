mod scene_studio;
mod button_studio;
mod app;
mod commands;
pub mod closed_loop;
mod config;
mod device;
pub mod diagnostics;
mod engine;
mod scheduler;
mod tasks;
mod telemetry;
mod vision;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(scene_studio::Studio::default())
        .manage(button_studio::ButtonStudio::default())
        .setup(|app| {
            app.manage(app::AppState::new(app.handle())?);
            scheduler::spawn(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            button_studio::button_status,
            button_studio::button_history,
            button_studio::button_prelabel_drafts,
            button_studio::button_import,
            button_studio::button_import_scene,
            button_studio::button_sample,
            button_studio::button_save,
            button_studio::button_partition,
            button_studio::button_prune_classes,
            button_studio::button_edit_class,
            button_studio::button_delete,
            button_studio::button_start,
            button_studio::button_cancel,
            button_studio::button_activate,
            button_studio::button_recognize,
            scene_studio::scene_status,
            scene_studio::scene_samples,
            scene_studio::scene_set_excluded,
            scene_studio::scene_start,
            scene_studio::scene_cancel,
            scene_studio::scene_activate,
            scene_studio::scene_open_folder,
            scene_studio::scene_recognize,
            scheduler::enqueue_task,
            scheduler::set_queue_enabled,
            scheduler::remove_queue_entry,
            commands::preview_oas_config,
            commands::import_oas_config,
            commands::get_snapshot,
            commands::save_config,
            commands::list_devices,
            commands::connect_device,
            commands::capture_screen,
            commands::tap_device,
            commands::swipe_device,
            commands::diagnose_screen,
            commands::observe_soul,
            commands::click_element,
            commands::start_task,
            commands::stop_task
        ])
        .build(tauri::generate_context!())
        .expect("failed to initialize Smart OMJ")
        .run(|handle, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                handle.state::<scene_studio::Studio>().shutdown();
                handle.state::<button_studio::ButtonStudio>().shutdown();
                let state = handle.state::<app::AppState>();
                state
                    .runtime
                    .lock()
                    .expect("runtime")
                    .snapshot
                    .queue
                    .enabled = false;
                if state.is_running() {
                    api.prevent_exit();
                    state.cancel();
                    let handle = handle.clone();
                    tauri::async_runtime::spawn(async move {
                        let state = handle.state::<app::AppState>();
                        let _guard = state.operation.lock().await;
                        handle.exit(0);
                    });
                }
            }
        });
}
