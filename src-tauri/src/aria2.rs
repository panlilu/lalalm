//! aria2c sidecar process management + JSON-RPC client.

use rand::Rng;
use serde_json::{json, Value};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::{Child, Command};

pub struct Aria2 {
    pub port: u16,
    secret: String,
    child: Child,
    http: reqwest::Client,
    #[allow(dead_code)]
    bin_path: PathBuf,
}

/// Locate the aria2c binary that should be used.
/// Order: bundled sidecar (next to our executable) → repo dev path → PATH.
pub fn resolve_binary() -> Option<PathBuf> {
    let exe_suffix = std::env::consts::EXE_SUFFIX;
    let triple = target_triple();

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(format!("aria2c{exe_suffix}")));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join(format!(
            "src-tauri/binaries/aria2c-{triple}{exe_suffix}"
        )));
        candidates.push(cwd.join(format!("src-tauri/binaries/aria2c-{triple}")));
        candidates.push(cwd.join("src-tauri/binaries/aria2c"));
    }
    for c in candidates {
        if c.is_file() {
            return Some(c);
        }
    }
    // Fall back to a system-wide aria2c (dev convenience).
    which_aria2c()
}

fn which_aria2c() -> Option<PathBuf> {
    let path = std::env::var("PATH").ok()?;
    let name = format!("aria2c{}", std::env::consts::EXE_SUFFIX);
    let sep = if std::env::consts::OS == "windows" { ';' } else { ':' };
    for dir in path.split(sep) {
        let p = Path::new(dir).join(&name);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

fn target_triple() -> String {
    let arch = match std::env::consts::ARCH {
        "aarch64" => "aarch64",
        "x86_64" => "x86_64",
        other => other,
    };
    match std::env::consts::OS {
        "macos" => format!("{arch}-apple-darwin"),
        "windows" => format!("{arch}-pc-windows-msvc"),
        "linux" => format!("{arch}-unknown-linux-gnu"),
        other => format!("{arch}-{other}"),
    }
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .unwrap_or(16800)
}

impl Aria2 {
    /// Spawn a new aria2c RPC server.
    #[allow(clippy::too_many_arguments)]
    pub async fn start(
        download_dir: &Path,
        max_concurrent: u32,
        max_conn_per_server: u32,
        split: u32,
        min_split_size: &str,
        proxy: Option<&str>,
        log_file: Option<PathBuf>,
    ) -> Result<Self, String> {
        let bin = resolve_binary().ok_or_else(|| {
            "未找到 aria2c。请先运行 `pnpm fetch:aria2` 构建内置下载器，或安装系统 aria2。".to_string()
        })?;

        std::fs::create_dir_all(download_dir).map_err(|e| e.to_string())?;

        let port = free_port();
        let secret: String = rand::thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(24)
            .map(char::from)
            .collect();

        let mut cmd = Command::new(&bin);
        let mut args: Vec<String> = vec![
            "--enable-rpc=true".into(),
            "--rpc-listen-all=false".into(),
            format!("--rpc-listen-port={port}"),
            format!("--rpc-secret={secret}"),
            format!("--dir={}", download_dir.display()),
            "--continue=true".into(),
            "--file-allocation=none".into(),
            "--auto-file-renaming=false".into(),
            "--allow-overwrite=true".into(),
            "--always-resume=true".into(),
            format!("--max-concurrent-downloads={max_concurrent}"),
            format!("--max-connection-per-server={max_conn_per_server}"),
            format!("--split={split}"),
            format!("--min-split-size={min_split_size}"),
            "--summary-interval=0".into(),
            "--enable-color=false".into(),
            "--console-log-level=warn".into(),
            "--user-agent=LalaLM/0.1".into(),
            format!("--stop-with-process={}", std::process::id()),
        ];
        if let Some(p) = proxy {
            let p = p.trim();
            if !p.is_empty() {
                args.push(format!("--all-proxy={p}"));
            }
        }
        cmd.args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null());
        // aria2c is a console app — without this flag a black cmd window
        // flashes every time the GUI process spawns it on Windows.
        #[cfg(target_os = "windows")]
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW

        match log_file {
            Some(f) => {
                if let Some(dir) = f.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                if let Ok(file) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&f)
                {
                    cmd.stderr(std::process::Stdio::from(file));
                } else {
                    cmd.stderr(std::process::Stdio::null());
                }
            }
            None => {
                cmd.stderr(std::process::Stdio::null());
            }
        }

        let child = cmd
            .spawn()
            .map_err(|e| format!("启动 aria2c 失败 ({}): {e}", bin.display()))?;

        // Wait for the RPC port to accept connections.
        let mut ok = false;
        for _ in 0..80 {
            if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                ok = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        if !ok {
            return Err("aria2c RPC 端口未就绪".into());
        }

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| e.to_string())?;

        Ok(Self {
            port,
            secret,
            child,
            http,
            bin_path: bin,
        })
    }

    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    pub async fn shutdown(&mut self) {
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
    }

    /// Raw JSON-RPC call with the secret token already attached.
    pub async fn rpc(&self, method: &str, params: Value) -> Result<Value, String> {
        let url = format!("http://127.0.0.1:{}/jsonrpc", self.port);
        let mut rpc_params: Vec<Value> = vec![json!(format!("token:{}", self.secret))];
        if let Some(arr) = params.as_array() {
            rpc_params.extend(arr.iter().cloned());
        }
        let body = json!({
            "jsonrpc": "2.0",
            "id": "lalalm",
            "method": method,
            "params": rpc_params,
        });
        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("aria2 RPC 请求失败: {e}"))?;
        let v: Value = resp.json().await.map_err(|e| format!("RPC 响应异常: {e}"))?;
        if let Some(err) = v.get("error") {
            return Err(
                err["message"].as_str().unwrap_or("aria2 error").to_string()
            );
        }
        Ok(v.get("result").cloned().unwrap_or(Value::Null))
    }

    pub async fn add_uri(&self, url: &str, dir: &str, out: &str, auth: Option<&str>, conn: u32, split: u32) -> Result<String, String> {
        let mut opts = json!({
            "dir": dir,
            "out": out,
            "continue": "true",
            "max-connection-per-server": conn.to_string(),
            "split": split.to_string(),
        });
        if let Some(a) = auth {
            opts["header"] = json!([format!("Authorization: {a}")]);
        }
        let r = self.rpc("aria2.addUri", json!([[url], opts])).await?;
        r.as_str().map(|s| s.to_string()).ok_or_else(|| "addUri 无返回 gid".into())
    }

    pub async fn tell(&self, method: &str, offset: u64, num: u64) -> Result<Vec<Value>, String> {
        let r = self.rpc(method, json!([offset, num, {
            "gid": true, "status": true, "totalLength": true, "completedLength": true,
            "downloadSpeed": true, "errorMessage": true, "files": true
        }]))
        .await?;
        Ok(r.as_array().cloned().unwrap_or_default())
    }

    #[allow(dead_code)]
    pub async fn tell_status(&self, gid: &str) -> Result<Value, String> {
        self.rpc("aria2.tellStatus", json!([gid, {
            "gid": true, "status": true, "totalLength": true, "completedLength": true,
            "downloadSpeed": true, "errorMessage": true
        }]))
        .await
    }

    pub async fn pause(&self, gid: &str) -> Result<(), String> {
        self.rpc("aria2.pause", json!([gid])).await.map(|_| ())
    }

    pub async fn unpause(&self, gid: &str) -> Result<(), String> {
        self.rpc("aria2.unpause", json!([gid])).await.map(|_| ())
    }

    pub async fn remove(&self, gid: &str) -> Result<(), String> {
        self.rpc("aria2.remove", json!([gid])).await.map(|_| ())
    }

    #[allow(dead_code)]
    pub async fn force_remove(&self, gid: &str) -> Result<(), String> {
        self.rpc("aria2.forceRemove", json!([gid])).await.map(|_| ())
    }

    pub async fn remove_result(&self, gid: &str) -> Result<(), String> {
        self.rpc("aria2.removeDownloadResult", json!([gid]))
            .await
            .map(|_| ())
    }
}
