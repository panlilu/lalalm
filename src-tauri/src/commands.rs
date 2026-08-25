//! Tauri command surface (invoked from the frontend).

use crate::config::{self, Config, Source};
use crate::state::AppState;
use serde::Serialize;
use std::path::PathBuf;
use tauri::State;

// ------------------------------------------------------------------- config

#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<Config, String> {
    Ok(state.config_clone())
}

#[tauri::command]
pub async fn save_config(
    state: State<'_, AppState>,
    mut config: Config,
) -> Result<Config, String> {
    // LM Studio destination only makes sense with its directory scanned.
    if config.download_destination == crate::config::DownloadDestination::LmStudio {
        config.scan_lm_studio = true;
    }
    // If any engine-relevant settings changed, drop the running instance so
    // the next download restarts with the new options; rebuild the HTTP
    // client when the proxy settings changed.
    let old = state.config_clone();
    let aria2_changed = old.aria2 != config.aria2
        || old.download_dir != config.download_dir
        || old.proxy_mode != config.proxy_mode
        || old.proxy_url != config.proxy_url
        || old.download_destination != config.download_destination;
    let proxy_changed =
        old.proxy_mode != config.proxy_mode || old.proxy_url != config.proxy_url;
    state.save_config(&config)?;
    if proxy_changed {
        *state.hub.write().unwrap() =
            crate::hub::HubClient::build(config.proxy_mode, &config.proxy_url);
    }
    if aria2_changed {
        let mut guard = state.aria2.lock().await;
        if let Some(a2) = guard.as_mut() {
            a2.shutdown().await;
        }
        *guard = None;
    }
    Ok(config)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CachePathInfo {
    pub label: String,
    pub path: String,
    pub exists: bool,
    pub scanned: bool,
}

#[tauri::command]
pub async fn get_cache_paths(state: State<'_, AppState>) -> Result<Vec<CachePathInfo>, String> {
    let cfg = state.config_clone();
    Ok(config::known_cache_paths(&cfg)
        .into_iter()
        .map(|(label, path)| CachePathInfo {
            scanned: match label.as_str() {
                "LalaLM Library" => true,
                "Hugging Face Cache" => cfg.scan_hf_cache,
                "LM Studio Models" | "LM Studio (legacy)" => cfg.scan_lm_studio,
                "ModelScope Cache" => cfg.scan_modelscope_cache,
                "Custom Path" => true,
                _ => false,
            },
            exists: path.is_dir(),
            label,
            path: path.to_string_lossy().to_string(),
        })
        .collect())
}

// -------------------------------------------------------------------- hub

#[tauri::command]
pub async fn search_models(
    state: State<'_, AppState>,
    source: Source,
    query: String,
    sort: Option<String>,
    gguf_only: Option<bool>,
    limit: Option<u32>,
) -> Result<Vec<crate::hub::ModelSummary>, String> {
    let cfg = state.config_clone();
    let q = query.trim().to_string();

    let res = state
        .hub()
        .search(
            source,
            &q,
            sort.as_deref().unwrap_or("downloads"),
            gguf_only.unwrap_or(true),
            limit.unwrap_or(30).min(50),
            &cfg,
        )
        .await;

    // Record successful non-empty searches as recent searches.
    if let Ok(list) = &res {
        if !list.is_empty() && !q.is_empty() {
            let mut c = cfg.clone();
            c.recent_searches.retain(|s| s != &q);
            c.recent_searches.insert(0, q);
            c.recent_searches.truncate(8);
            let _ = state.save_config(&c);
        }
    }
    res
}

#[tauri::command]
pub async fn get_model_detail(
    state: State<'_, AppState>,
    source: Source,
    repo: String,
) -> Result<crate::hub::ModelDetail, String> {
    let cfg = state.config_clone();
    state.hub().detail(source, &repo, &cfg).await
}

// ---------------------------------------------------------------- downloads

#[tauri::command]
pub async fn start_download(
    state: State<'_, AppState>,
    source: Source,
    repo: String,
    path: String,
) -> Result<crate::downloads::DownloadTask, String> {
    crate::downloads::start_download(&state, source, &repo, &path).await
}

/// Queue a set of files (e.g. one quantization + its companions).
/// Files already downloading are skipped; fails only if nothing was queued.
#[tauri::command]
pub async fn start_download_batch(
    state: State<'_, AppState>,
    source: Source,
    repo: String,
    paths: Vec<String>,
) -> Result<usize, String> {
    if paths.is_empty() {
        return Err("没有可下载的文件".into());
    }
    let mut started = 0usize;
    let mut first_err: Option<String> = None;
    for p in &paths {
        match crate::downloads::start_download(&state, source, &repo, p).await {
            Ok(_) => started += 1,
            Err(e) => {
                let dup = e.contains("已有进行中的下载任务");
                if !dup && first_err.is_none() {
                    first_err = Some(format!("{p}: {e}"));
                }
            }
        }
    }
    if started == 0 {
        return Err(first_err.unwrap_or_else(|| "下载失败".into()));
    }
    Ok(started)
}

#[tauri::command]
pub async fn list_downloads(
    state: State<'_, AppState>,
) -> Result<Vec<crate::downloads::DownloadTask>, String> {
    Ok(state.tasks.read().unwrap().clone())
}

#[tauri::command]
pub async fn pause_download(state: State<'_, AppState>, id: String) -> Result<(), String> {
    crate::downloads::pause(&state, &id).await
}

#[tauri::command]
pub async fn resume_download(state: State<'_, AppState>, id: String) -> Result<(), String> {
    crate::downloads::resume(&state, &id).await
}

#[tauri::command]
pub async fn cancel_download(state: State<'_, AppState>, id: String) -> Result<(), String> {
    crate::downloads::cancel(&state, &id).await
}

#[tauri::command]
pub async fn retry_download(
    state: State<'_, AppState>,
    id: String,
) -> Result<crate::downloads::DownloadTask, String> {
    crate::downloads::retry(&state, &id).await
}

#[tauri::command]
pub async fn remove_download(state: State<'_, AppState>, id: String) -> Result<(), String> {
    crate::downloads::remove_task(&state, &id)
}

#[tauri::command]
pub async fn clear_finished_downloads(state: State<'_, AppState>) -> Result<usize, String> {
    Ok(crate::downloads::clear_finished(&state))
}

// ----------------------------------------------------------------- registry

#[tauri::command]
pub async fn list_local_models(
    state: State<'_, AppState>,
) -> Result<Vec<crate::registry::LocalModel>, String> {
    let cfg = state.config_clone();
    let models = tokio::task::spawn_blocking(move || crate::registry::list_local_models(&cfg))
        .await
        .map_err(|e| e.to_string())?;
    Ok(models)
}

#[tauri::command]
pub async fn delete_local_models(
    paths: Vec<String>,
) -> Result<Vec<crate::registry::OpResult>, String> {
    Ok(crate::registry::delete_models(&paths))
}

#[tauri::command]
pub async fn move_local_models(
    paths: Vec<String>,
    dest_dir: String,
) -> Result<Vec<crate::registry::OpResult>, String> {
    Ok(crate::registry::move_models(&paths, &dest_dir))
}

#[tauri::command]
pub async fn dir_sizes(paths: Vec<String>) -> Result<std::collections::HashMap<String, u64>, String> {
    Ok(crate::registry::dir_sizes(paths).await)
}

// -------------------------------------------------------------------- misc

#[tauri::command]
pub async fn pick_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .pick_folder(move |p| {
            let _ = tx.send(p.map(|f| f.to_string()));
        });
    rx.await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn reveal_path(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .args(["-R", &path])
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer")
            .arg(format!("/select,{path}"))
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn open_path(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err("路径不存在".into());
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&p)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer")
            .arg(&p)
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open").arg(&p).spawn().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_sys_stats(state: State<'_, AppState>) -> Result<crate::stats::SysStats, String> {
    Ok(crate::stats::collect(&state))
}

/// Open an external URL (the model's original page) in the default browser.
#[tauri::command]
pub async fn open_url(url: String) -> Result<(), String> {
    let safe = url.starts_with("http://") || url.starts_with("https://");
    if !safe {
        return Err("仅允许打开 http(s) 链接".into());
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &url])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open").arg(&url).spawn().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Does the same repo exist on another hub? Used for the cross-source tab.
#[tauri::command]
pub async fn check_repo_exists(
    state: State<'_, AppState>,
    source: Source,
    repo: String,
) -> Result<bool, String> {
    let cfg = state.config_clone();
    state.hub().repo_exists(source, &repo, &cfg).await
}

/// The resolved LM Studio models directory (reads LM Studio's own config).
#[tauri::command]
pub fn lm_studio_dir() -> String {
    crate::config::resolve_lm_studio_dir()
        .to_string_lossy()
        .to_string()
}

/// Tail of the aria2c download log (the engine's formatted log file).
#[tauri::command]
pub fn read_aria2_log(state: State<'_, AppState>, lines: Option<u32>) -> Result<String, String> {
    let path = state.app_data.join("logs").join("aria2.log");
    if !path.exists() {
        return Ok("(日志文件尚未创建 —— 启动一次下载后这里会出现 aria2 的运行日志)".into());
    }
    let n = lines.unwrap_or(200).max(1) as usize;
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&bytes);
    let total = text.lines().count();
    let start = total.saturating_sub(n);
    let tail: String = text
        .lines()
        .skip(start)
        .collect::<Vec<_>>()
        .join("\n");
    Ok(format!("-- 共 {total} 行，显示最后 {} 行 --\n{tail}", total - start))
}

#[tauri::command]
pub async fn get_app_version() -> Result<String, String> {
    Ok(env!("CARGO_PKG_VERSION").to_string())
}

// ------------------------------------------------------------------ recommended

/// Curated "recommended models" list — compiled into the binary from
/// `src-tauri/assets/recommended.json` (edit that file + rebuild to change
/// the list; it ships with every build).
#[derive(serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RecommendedItem {
    pub source: Source,
    pub repo: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Restrict visibility to a platform, e.g. "macos" (Apple MLX entries).
    #[serde(default)]
    pub platform: Option<String>,
}

#[tauri::command]
pub fn get_recommended() -> Result<Vec<RecommendedItem>, String> {
    const JSON: &str = include_str!("../assets/recommended.json");
    let parsed: serde_json::Value =
        serde_json::from_str(JSON).map_err(|e| format!("推荐列表配置损坏: {e}"))?;
    let items: Vec<RecommendedItem> =
        serde_json::from_value(parsed["items"].clone()).map_err(|e| e.to_string())?;
    Ok(items)
}
