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
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;

/// The tray's dynamic "download status" line, updated by the monitor loop.
pub struct TrayStatusItem(pub Mutex<Option<MenuItem<tauri::Wry>>>);

fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn toggle_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        } else {
            show_main(app);
        }
    }
}

/// Update the tray status line from the download task list.
pub fn update_tray_status(app: &tauri::AppHandle, tasks: &[downloads::DownloadTask]) {
    let Some(st) = app.try_state::<TrayStatusItem>() else {
        return;
    };
    let guard = st.0.lock().unwrap();
    let Some(item) = guard.as_ref() else { return };
    let active = tasks
        .iter()
        .filter(|t| matches!(t.status, downloads::DlStatus::Queued | downloads::DlStatus::Active))
        .count();
    let text = match active {
        0 => "暂无进行中的下载".to_string(),
        1 => "⬇ 1 个下载任务进行中".to_string(),
        n => format!("⬇ {n} 个下载任务进行中"),
    };
    let _ = item.set_text(text);
}

fn main() {
    tauri::Builder::default()
        // Must be the FIRST plugin: second launches forward to this process
        // and the callback re-shows the main window.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        // The window starts hidden (`visible: false`) and is shown here once
        // the frontend has actually rendered — avoids the black period while
        // WebView2 initializes on Windows.
        .on_page_load(|w, payload| {
            if payload.event() == tauri::webview::PageLoadEvent::Finished {
                let _ = w.show();
                let _ = w.set_focus();
            }
        })
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

            // Safety net: if the page-load event never fires (broken asset,
            // dev server down), reveal the window after a grace period.
            let handle_fb = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                if let Some(w) = handle_fb.get_webview_window("main") {
                    if !w.is_visible().unwrap_or(true) {
                        let _ = w.show();
                    }
                }
            });

            // One-shot GPU/VRAM probe (system_profiler is slow).
            let handle2 = app.handle().clone();
            tauri::async_runtime::spawn_blocking(move || {
                let info = stats::probe_gpu();
                if let Some(st) = handle2.try_state::<AppState>() {
                    *st.gpu.lock().unwrap() = Some(info);
                }
            });

            // ---- system tray (macOS menu bar / Windows notification area) ----
            let open_i =
                MenuItem::with_id(app, "open", "打开 LalaLM", true, None::<&str>)?;
            let status_i =
                MenuItem::with_id(app, "status", "暂无进行中的下载", false, None::<&str>)?;
            let dl_i = MenuItem::with_id(app, "downloads", "下载任务", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "退出 LalaLM", true, None::<&str>)?;
            let sep1 = PredefinedMenuItem::separator(app)?;
            let sep2 = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(
                app,
                &[&open_i, &sep1, &status_i, &dl_i, &sep2, &quit_i],
            )?;
            app.manage(TrayStatusItem(Mutex::new(Some(status_i.clone()))));

            let mut tray = TrayIconBuilder::with_id("main-tray")
                .menu(&menu)
                .tooltip("LalaLM — 关闭窗口后下载继续")
                .show_menu_on_left_click(false) // left click toggles the window
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "open" => show_main(app),
                    "downloads" => {
                        show_main(app);
                        use tauri::Emitter;
                        let _ = app.emit("navigate", "downloads");
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        toggle_main(tray.app_handle());
                    }
                });
            #[cfg(target_os = "macos")]
            {
                let img = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-mac.png"))
                    .expect("tray-mac.png");
                tray = tray.icon(img).icon_as_template(true);
            }
            #[cfg(not(target_os = "macos"))]
            {
                let img = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-win.png"))
                    .expect("tray-win.png");
                tray = tray.icon(img);
            }
            tray.build(app)?;

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
            commands::lm_studio_dir,
            commands::read_aria2_log,
            commands::get_recommended,
            commands::get_org_avatar,
            commands::get_app_version,
        ])
        .build(tauri::generate_context!())
        .expect("error while building LalaLM")
        .run(|app, event| match event {
            tauri::RunEvent::ExitRequested { .. } => {
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
            // Closing the window hides it to the tray — downloads keep
            // running in the background. Quit via the tray menu.
            tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::CloseRequested { api, .. },
                ..
            } => {
                if label == "main" {
                    api.prevent_close();
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.hide();
                    }
                }
            }
            // Clicking the dock icon re-opens a hidden window (macOS).
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => show_main(app),
            _ => {}
        });
}
