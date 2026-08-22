//! System statistics: CPU / RAM / VRAM(GPU) / disk for the sidebar monitor.

use crate::state::AppState;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuInfo {
    pub name: Option<String>,
    pub vram_total: Option<u64>,
    pub unified: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SysStats {
    pub cpu_usage: f32,
    pub cpu_count: usize,
    pub mem_total: u64,
    pub mem_used: u64,
    pub mem_percent: f32,
    pub swap_used: u64,
    pub vram_total: Option<u64>,
    pub vram_unified: bool,
    pub gpu_name: Option<String>,
    pub disk_free: u64,
    pub disk_total: u64,
    pub platform: String,
    pub arch: String,
}

pub fn collect(state: &AppState) -> SysStats {
    let mut sys = state.sys.lock().unwrap();
    sys.refresh_cpu_usage();
    sys.refresh_memory();
    let cpu_usage = sys.global_cpu_usage();
    let mem_total = sys.total_memory();
    let mem_used = sys.used_memory();
    let swap_used = sys.used_swap();
    drop(sys);

    {
        let mut disks = state.disks.lock().unwrap();
        disks.refresh(true);
    }
    let cfg = state.config_clone();
    let (disk_free, disk_total) = disk_for(&state.disks.lock().unwrap(), &cfg.download_dir);

    let gpu = state.gpu.lock().unwrap().clone();
    let mem_percent = if mem_total > 0 {
        (mem_used as f32 / mem_total as f32) * 100.0
    } else {
        0.0
    };

    SysStats {
        cpu_usage,
        cpu_count: std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(0),
        mem_total,
        mem_used,
        mem_percent,
        swap_used,
        vram_total: gpu.as_ref().and_then(|g| g.vram_total),
        vram_unified: gpu.as_ref().map(|g| g.unified).unwrap_or(false),
        gpu_name: gpu.as_ref().and_then(|g| g.name.clone()),
        disk_free,
        disk_total,
        platform: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
    }
}

fn disk_for(disks: &sysinfo::Disks, dir: &std::path::Path) -> (u64, u64) {
    // Prefer the disk that contains the models directory; fall back to "/".
    let mut best: Option<(usize, &sysinfo::Disk)> = None;
    for d in disks.list() {
        let mp = d.mount_point();
        if dir.starts_with(mp) {
            let len = mp.as_os_str().len();
            if best.map(|(l, _)| len > l).unwrap_or(true) {
                best = Some((len, d));
            }
        }
    }
    match best {
        Some((_, d)) => (d.available_space(), d.total_space()),
        None => disks
            .list()
            .iter()
            .find(|d| d.mount_point() == std::path::Path::new("/"))
            .map(|d| (d.available_space(), d.total_space()))
            .unwrap_or((0, 0)),
    }
}

/// Probe GPU/VRAM info once at startup (runs on a blocking thread).
pub fn probe_gpu() -> GpuInfo {
    #[cfg(target_os = "macos")]
    {
        let unified_arch = std::env::consts::ARCH == "aarch64";
        if let Ok(out) = std::process::Command::new("system_profiler")
            .args(["SPDisplaysDataType", "-json"])
            .output()
        {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&out.stdout) {
                if let Some(items) = v["SPDisplaysDataType"].as_array() {
                    let first = items.first().cloned().unwrap_or_default();
                    let name = first["sppci_model"]
                        .as_str()
                        .or(first["_name"].as_str())
                        .map(|s| s.to_string());
                    let vram = first["spdisplays_vram"]
                        .as_str()
                        .or_else(|| first["spdisplays_vram_shared_size"].as_str())
                        .and_then(parse_size_str);
                    return GpuInfo {
                        name,
                        vram_total: vram,
                        unified: unified_arch || vram.is_none(),
                    };
                }
            }
        }
        return GpuInfo {
            name: None,
            vram_total: None,
            unified: unified_arch,
        };
    }
    #[cfg(target_os = "windows")]
    {
        let name = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "(Get-CimInstance Win32_VideoController | Select-Object -First 1).Name",
            ])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("null"));
        return GpuInfo {
            name,
            vram_total: None,
            unified: false,
        };
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        GpuInfo {
            name: None,
            vram_total: None,
            unified: false,
        }
    }
}

/// Parse strings like "8 GB", "512 MB", "16384 KB".
#[allow(dead_code)]
fn parse_size_str(s: &str) -> Option<u64> {
    let s = s.trim();
    let mut split = s.splitn(2, ' ');
    let num: f64 = split.next()?.parse().ok()?;
    let unit = split.next().unwrap_or("").trim().to_uppercase();
    let mult = match unit.as_str() {
        "KB" | "K" => 1024.0,
        "MB" | "M" => 1024.0 * 1024.0,
        "GB" | "G" => 1024.0 * 1024.0 * 1024.0,
        "TB" | "T" => 1024.0f64.powi(4),
        _ => 1.0,
    };
    Some((num * mult) as u64)
}
