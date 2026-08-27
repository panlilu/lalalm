//! Download task management: start / pause / resume / cancel / retry,
//! aria2 reconciliation loop and history persistence.

use crate::aria2::Aria2;
use crate::config::{Config, effective_download_root, Source};
use crate::state::{new_task_id, AppState};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DlStatus {
    Queued,
    Active,
    Paused,
    Completed,
    Error,
    Cancelled,
    Interrupted,
}

impl DlStatus {
    pub fn terminal(&self) -> bool {
        matches!(
            self,
            DlStatus::Completed | DlStatus::Error | DlStatus::Cancelled
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadTask {
    pub id: String,
    pub repo: String,
    pub path: String,
    pub source: Source,
    pub url: String,
    pub dir: PathBuf,
    pub out: String,
    pub total: u64,
    pub downloaded: u64,
    pub speed: u64,
    pub status: DlStatus,
    pub error: Option<String>,
    pub gid: Option<String>,
    pub added_at: u64,
    pub updated_at: u64,
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn history_path(app_data: &std::path::Path) -> PathBuf {
    app_data.join("downloads.json")
}

pub fn load_history(app_data: &std::path::Path) -> Vec<DownloadTask> {
    let Ok(s) = std::fs::read_to_string(history_path(app_data)) else {
        return Vec::new();
    };
    let mut v: Vec<DownloadTask> = serde_json::from_str(&s).unwrap_or_default();
    for t in &mut v {
        if !t.status.terminal() {
            t.status = DlStatus::Interrupted;
            t.error = Some("应用重启导致下载中断，可点击重试续传".into());
            t.gid = None;
        }
    }
    v
}

pub fn persist(state: &AppState) {
    let tasks = state.tasks.read().unwrap().clone();
    let s = serde_json::to_string_pretty(&tasks).unwrap_or_else(|_| "[]".into());
    let _ = std::fs::write(history_path(&state.app_data), s);
}

/// Make sure an aria2c RPC server is running (restarts dead processes).
pub async fn ensure_aria2(state: &AppState) -> Result<(), String> {
    let cfg = state.config_clone();
    let fresh_start;
    {
        let mut guard = state.aria2.lock().await;
        if let Some(a2) = guard.as_mut() {
            if a2.is_alive() {
                return Ok(());
            }
        }
        let log = state.app_data.join("logs").join("aria2.log");
        let a2 = Aria2::start(
            &cfg.download_dir,
            cfg.aria2.max_concurrent_downloads.max(1),
            cfg.aria2.max_connection_per_server.max(1),
            cfg.aria2.split.max(1),
            &cfg.aria2.min_split_size,
            Some(log),
        )
        .await?;
        *guard = Some(a2);
        fresh_start = true;
    }
    // A brand-new engine knows none of the old gids: any non-terminal task
    // would otherwise poll forever at 0% while its partial file sits on
    // disk. Re-queue those tasks on the new instance (--continue resumes).
    if fresh_start {
        resume_orphans(state).await;
    }
    Ok(())
}

/// Re-add every non-terminal task to the freshly started engine.
async fn resume_orphans(state: &AppState) {
    let snapshot: Vec<DownloadTask> = {
        let tasks = state.tasks.read().unwrap();
        tasks
            .iter()
            .filter(|t| !t.status.terminal() && t.gid.is_some())
            .cloned()
            .collect()
    };
    if snapshot.is_empty() {
        return;
    }
    let cfg = state.config_clone();
    for t in snapshot {
        let source = t.source;
        let task_proxy = match source {
            Source::ModelScope => None,
            _ => crate::hub::effective_proxy(&cfg),
        };
        let gid = {
            let guard = state.aria2.lock().await;
            match guard.as_ref() {
                Some(a2) => {
                    let auth = source.auth_header(&cfg);
                    a2.add_uri(
                        &t.url.clone(),
                        &t.dir.to_string_lossy(),
                        &t.out,
                        auth.as_deref(),
                        cfg.aria2.max_connection_per_server.max(1),
                        cfg.aria2.split.max(1),
                        task_proxy.as_deref(),
                    )
                    .await
                }
                None => Err("aria2 未启动".into()),
            }
        };
        {
            let mut tasks = state.tasks.write().unwrap();
            if let Some(t) = find_mut(&mut tasks, &t.id) {
                match gid {
                    Ok(new_gid) => {
                        t.gid = Some(new_gid);
                        t.status = DlStatus::Active;
                        t.error = None;
                        t.updated_at = now();
                    }
                    Err(e) => {
                        t.status = DlStatus::Interrupted;
                        t.error = Some(format!("引擎重启后恢复失败：{e}"));
                        t.updated_at = now();
                    }
                }
            }
        }
        persist(state);
    }
}

/// Public entry used by the `start_download` command.
pub async fn start_download(
    state: &AppState,
    source: Source,
    repo: &str,
    path: &str,
) -> Result<DownloadTask, String> {
    let cfg = state.config_clone();
    let url = source.file_url(repo, path);
    let out = path
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(path)
        .to_string();

    // Destination: LalaLM library or the LM Studio models dir — both use the
    // same publisher/model/file layout so LM Studio recognizes files instantly.
    let dir = quick_target_dir(&cfg, repo);

    // Reject duplicate in-flight tasks for the same target file.
    {
        let tasks = state.tasks.read().unwrap();
        if tasks.iter().any(|t| {
            !t.status.terminal()
                && t.dir == dir
                && t.out == out
        }) {
            return Err("该文件已有进行中的下载任务".into());
        }
    }

    // Make sure the destination directory exists before handing off to aria2.
    let _ = std::fs::create_dir_all(&dir);

    ensure_aria2(state).await?;

    // Proxy is per-task and per-source: HF benefits from it, ModelScope
    // (China-hosted) must stay direct or downloads stall through proxies.
    let task_proxy = match source {
        Source::ModelScope => None,
        _ => crate::hub::effective_proxy(&cfg),
    };
    let gid = {
        let guard = state.aria2.lock().await;
        let a2 = guard.as_ref().ok_or("aria2 未启动")?;
        let auth = source.auth_header(&cfg);
        a2.add_uri(
            &url,
            &dir.to_string_lossy(),
            &out,
            auth.as_deref(),
            cfg.aria2.max_connection_per_server.max(1),
            cfg.aria2.split.max(1),
            task_proxy.as_deref(),
        )
        .await?
    };

    let task = DownloadTask {
        id: new_task_id(),
        repo: repo.to_string(),
        path: path.to_string(),
        source,
        url,
        dir,
        out,
        total: 0,
        downloaded: 0,
        speed: 0,
        status: DlStatus::Active,
        error: None,
        gid: Some(gid),
        added_at: now(),
        updated_at: now(),
    };
    state.tasks.write().unwrap().insert(0, task.clone());
    persist(state);
    Ok(task)
}

/// Where a file of `repo` lands given current settings (publisher/model dir
/// under the configured root — LalaLM library or LM Studio's directory).
pub fn quick_target_dir(cfg: &Config, repo: &str) -> PathBuf {
    let base = effective_download_root(cfg);
    let (author, name) = match repo.split_once('/') {
        Some((a, n)) => (a, n),
        None => ("", repo),
    };
    if author.is_empty() {
        base.join(sanitize(name))
    } else {
        base.join(sanitize(author)).join(sanitize(name))
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn find_mut<'a>(
    tasks: &'a mut Vec<DownloadTask>,
    id: &str,
) -> Option<&'a mut DownloadTask> {
    tasks.iter_mut().find(|t| t.id == id)
}

pub async fn pause(state: &AppState, id: &str) -> Result<(), String> {
    let gid = {
        let mut tasks = state.tasks.write().unwrap();
        let t = find_mut(&mut tasks, id).ok_or("任务不存在")?;
        t.status = DlStatus::Paused;
        t.updated_at = now();
        t.gid.clone()
    };
    persist(state);
    if let Some(gid) = gid {
        let guard = state.aria2.lock().await;
        if let Some(a2) = guard.as_ref() {
            a2.pause(&gid).await?;
        }
    }
    Ok(())
}

pub async fn resume(state: &AppState, id: &str) -> Result<(), String> {
    enum Action {
        Unpause(Option<String>),
        Retry,
    }
    let action: Action = {
        let mut tasks = state.tasks.write().unwrap();
        let t = find_mut(&mut tasks, id).ok_or("任务不存在")?;
        match t.status {
            DlStatus::Paused => {
                t.status = DlStatus::Active;
                t.updated_at = now();
                Action::Unpause(t.gid.clone())
            }
            DlStatus::Interrupted | DlStatus::Error | DlStatus::Cancelled => Action::Retry,
            _ => return Ok(()),
        }
    };
    persist(state);
    match action {
        Action::Retry => retry(state, id).await.map(|_| ()),
        Action::Unpause(gid) => {
            if let Some(gid) = gid {
                let guard = state.aria2.lock().await;
                if let Some(a2) = guard.as_ref() {
                    a2.unpause(&gid).await?;
                }
            }
            Ok(())
        }
    }
}

pub async fn cancel(state: &AppState, id: &str) -> Result<(), String> {
    let gid = {
        let mut tasks = state.tasks.write().unwrap();
        let t = find_mut(&mut tasks, id).ok_or("任务不存在")?;
        t.status = DlStatus::Cancelled;
        t.updated_at = now();
        t.gid.clone()
    };
    persist(state);
    if let Some(gid) = gid {
        let guard = state.aria2.lock().await;
        if let Some(a2) = guard.as_ref() {
            let _ = a2.remove(&gid).await;
        }
    }
    Ok(())
}

/// Re-queue a failed / cancelled / interrupted download (resumes partial data).
pub async fn retry(state: &AppState, id: &str) -> Result<DownloadTask, String> {
    ensure_aria2(state).await?;
    let cfg = state.config_clone();
    let (url, dir, out, old_gid, source) = {
        let mut tasks = state.tasks.write().unwrap();
        let t = find_mut(&mut tasks, id).ok_or("任务不存在")?;
        t.status = DlStatus::Active;
        t.error = None;
        t.downloaded = 0;
        t.total = 0;
        t.speed = 0;
        t.updated_at = now();
        (
            t.url.clone(),
            t.dir.clone(),
            t.out.clone(),
            t.gid.clone(),
            t.source,
        )
    };
    let gid = {
        let guard = state.aria2.lock().await;
        let a2 = guard.as_ref().ok_or("aria2 未启动")?;
        if let Some(old) = &old_gid {
            let _ = a2.remove_result(old).await;
        }
        let auth = source.auth_header(&cfg);
        let task_proxy = match source {
            Source::ModelScope => None,
            _ => crate::hub::effective_proxy(&cfg),
        };
        a2.add_uri(
            &url,
            &dir.to_string_lossy(),
            &out,
            auth.as_deref(),
            cfg.aria2.max_connection_per_server.max(1),
            cfg.aria2.split.max(1),
            task_proxy.as_deref(),
        )
        .await?
    };
    let task = {
        let mut tasks = state.tasks.write().unwrap();
        let t = find_mut(&mut tasks, id).ok_or("任务不存在")?;
        t.gid = Some(gid);
        t.clone()
    };
    persist(state);
    Ok(task)
}

pub fn remove_task(state: &AppState, id: &str) -> Result<(), String> {
    state.tasks.write().unwrap().retain(|t| t.id != id);
    persist(state);
    Ok(())
}

pub fn clear_finished(state: &AppState) -> usize {
    let mut tasks = state.tasks.write().unwrap();
    let before = tasks.len();
    tasks.retain(|t| !t.status.terminal());
    persist(state);
    before - tasks.len()
}

/// Background reconciliation between aria2 state and our task list.
pub async fn reconcile(state: &AppState, a2: &mut Aria2) -> Result<(), String> {
    let active = a2.tell_active().await.unwrap_or_default();
    let waiting = a2
        .tell("aria2.tellWaiting", 0, 200)
        .await
        .unwrap_or_default();
    let stopped = a2
        .tell("aria2.tellStopped", 0, 100)
        .await
        .unwrap_or_default();

    #[allow(clippy::type_complexity)]
    let mut updates: HashMap<
        String,
        (DlStatus, u64, u64, u64, Option<String>, Option<String>),
    > = HashMap::new();
    let mut clear_gids: Vec<String> = Vec::new();

    for e in active.iter().chain(waiting.iter()).chain(stopped.iter()) {
        let Some(gid) = e["gid"].as_str().map(|s| s.to_string()) else {
            continue;
        };
        let st = match e["status"].as_str().unwrap_or("") {
            "active" => DlStatus::Active,
            "waiting" => DlStatus::Queued,
            "paused" => DlStatus::Paused,
            "complete" => DlStatus::Completed,
            "error" => DlStatus::Error,
            "removed" => DlStatus::Cancelled,
            _ => continue,
        };
        let total = parse_u64(&e["totalLength"]);
        let completed = parse_u64(&e["completedLength"]);
        let speed = parse_u64(&e["downloadSpeed"]);
        let err = e["errorMessage"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        // Where aria2 actually put the file (may differ from the requested
        // name after exotic redirects) — used for a completion rename-back.
        let actual_path = e["files"][0]["path"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        updates.insert(gid.clone(), (st, total, completed, speed, err, actual_path));
        if st.terminal() {
            clear_gids.push(gid);
        }
    }

    if !updates.is_empty() {
        let mut changed = false;
        let mut rename_back: Vec<(std::path::PathBuf, std::path::PathBuf)> = Vec::new();
        {
            let mut tasks = state.tasks.write().unwrap();
            for t in tasks.iter_mut() {
                if let Some(gid) = t.gid.clone() {
                    if let Some((st, total, completed, speed, err, actual)) =
                        updates.get(&gid)
                    {
                        // Completion insurance: if the engine saved the file
                        // under a different name than requested, move it to
                        // the expected one so LM Studio / llama.cpp find it.
                        if *st == DlStatus::Completed {
                            if let Some(ap) = actual {
                                let final_path = t.dir.join(&t.out);
                                let ap_path = std::path::PathBuf::from(ap);
                                if ap_path != final_path
                                    && ap_path.exists()
                                    && !final_path.exists()
                                    && ap_path.parent() == Some(final_path.parent().unwrap_or(&ap_path))
                                {
                                    rename_back.push((ap_path, final_path));
                                }
                            }
                        }
                        if t.status != *st
                            || t.downloaded != *completed
                            || t.total != *total
                        {
                            t.status = *st;
                            t.total = *total;
                            t.downloaded = *completed;
                            t.speed = *speed;
                            if err.is_some() {
                                t.error = err.clone();
                            }
                            t.updated_at = now();
                            changed = true;
                        }
                    }
                }
            }
        }
        for (from, to) in rename_back {
            let _ = std::fs::rename(&from, &to);
        }
        if changed {
            persist(state);
        }
    }

    for gid in clear_gids {
        let _ = a2.remove_result(&gid).await;
    }
    Ok(())
}

fn parse_u64(v: &Value) -> u64 {
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .unwrap_or(0)
}

/// Monitor loop: reconciles aria2, pushes task + system-stat events to the UI.
pub async fn monitor_loop(app: AppHandle) {
    let mut tick: u64 = 0;
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(900)).await;
        tick = tick.wrapping_add(1);
        let state = app.state::<AppState>();

        {
            let mut guard = state.aria2.lock().await;
            let alive = guard
                .as_mut()
                .map(|a2| a2.is_alive())
                .unwrap_or(false);
            if alive {
                if let Some(a2) = guard.as_mut() {
                    let _ = reconcile(&state, a2).await;
                }
            } else if guard.is_some() {
                *guard = None;
            }
        }

        let tasks = state.tasks.read().unwrap().clone();
        let _ = app.emit("downloads-changed", &tasks);
        crate::update_tray_status(&app, &tasks);

        if tick % 2 == 0 {
            let stats = crate::stats::collect(&state);
            let _ = app.emit("sys-stats", &stats);
        }
    }
}
