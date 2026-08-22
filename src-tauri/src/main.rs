// Prevents an additional console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod aria2;
mod commands;
mod config;
mod downloads;
mod gguf;
mod hub;
mod registry;
mod state;
mod stats;

use state::AppState;
use std::sync::Mutex;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data).ok();

            let cfg = config::load(&app_data);
            let tasks = downloads::load_history(&app_data);

            let sys = sysinfo::System::new();
            let disks = sysinfo::Disks::new_with_refreshed_list();

            app.manage(AppState {
                app_data: app_data.clone(),
                config: std::sync::RwLock::new(cfg.clone()),
                hub: std::sync::RwLock::new(hub::HubClient::build(
                    cfg.proxy_mode,
                    &cfg.proxy_url,
                )),
                aria2: tokio::sync::Mutex::new(None),
                tasks: std::sync::RwLock::new(tasks),
                sys: Mutex::new(sys),
                disks: Mutex::new(disks),
                gpu: Mutex::new(None),
            });

            // Download + stats monitor loop.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(downloads::monitor_loop(handle));

            // One-shot GPU/VRAM probe (system_profiler is slow).
            let handle2 = app.handle().clone();
            tauri::async_runtime::spawn_blocking(move || {
                let info = stats::probe_gpu();
                if let Some(st) = handle2.try_state::<AppState>() {
                    *st.gpu.lock().unwrap() = Some(info);
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::get_cache_paths,
            commands::search_models,
            commands::get_model_detail,
            commands::start_download,
            commands::start_download_batch,
            commands::list_downloads,
            commands::pause_download,
            commands::resume_download,
            commands::cancel_download,
            commands::retry_download,
            commands::remove_download,
            commands::clear_finished_downloads,
            commands::list_local_models,
            commands::delete_local_models,
            commands::move_local_models,
            commands::dir_sizes,
            commands::pick_folder,
            commands::reveal_path,
            commands::open_path,
            commands::get_sys_stats,
            commands::open_url,
            commands::check_repo_exists,
            commands::get_app_version,
        ])
        .build(tauri::generate_context!())
        .expect("error while building LalaLM")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                // Make sure the bundled aria2c does not outlive us
                // (it also runs with --stop-with-process as a safety net).
                let state = app.state::<AppState>();
                tauri::async_runtime::block_on(async {
                    let mut guard = state.aria2.lock().await;
                    if let Some(a2) = guard.as_mut() {
                        a2.shutdown().await;
                    }
                    *guard = None;
                });
            }
        });
}
