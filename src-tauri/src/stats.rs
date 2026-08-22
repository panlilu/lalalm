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

/// Heuristic: does this adapter name belong to a virtual / software / helper
/// display device rather than a physical GPU?
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn gpu_name_is_virtual(name: &str) -> bool {
    const MARKERS: [&str; 12] = [
        "virtual",
        "dummy",
        "indirect",
        "basic display",
        "basicrender",
        "基本显示",
        "vmware",
        "virtualbox",
        "vbox",
        "hyper-v",
        "qxl",
        "parsec",
    ];
    let l = name.to_lowercase();
    MARKERS.iter().any(|m| l.contains(m))
}

/// Windows-only CIM fallback when the registry enumeration yields nothing.
#[cfg(target_os = "windows")]
fn fall_back_to_cim_gpu() -> GpuInfo {
    use std::os::windows::process::CommandExt;
    let mut ps = std::process::Command::new("powershell");
    ps.args([
        "-NoProfile",
        "-Command",
        "Get-CimInstance Win32_VideoController | ForEach-Object { \"$($_.Name)|$($_.AdapterRAM)\" }",
    ]);
    ps.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let out = ps
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let mut best: Option<(String, u64)> = None;
    for line in out.lines() {
        let line = line.trim();
        if let Some((n, r)) = line.split_once('|') {
            if gpu_name_is_virtual(n) {
                continue;
            }
            let vram = r.trim().parse::<u64>().unwrap_or(0);
            if best.as_ref().map(|(_, b)| vram > *b).unwrap_or(true) {
                best = Some((n.trim().to_string(), vram));
            }
        }
    }
    match best {
        Some((name, vram)) => GpuInfo {
            name: Some(name),
            vram_total: if vram > 0 { Some(vram) } else { None },
            unified: false,
        },
        None => GpuInfo {
            name: None,
            vram_total: None,
            unified: false,
        },
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
        use std::os::windows::process::CommandExt;
        const NO_WINDOW: u32 = 0x0800_0000; // CREATE_NO_WINDOW
        const DISPLAY_CLASS: &str =
            r"HKLM\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";

        fn reg_query(key: &str, value: &str) -> Option<String> {
            use std::os::windows::process::CommandExt;
            let mut cmd = std::process::Command::new("reg");
            cmd.args(["query", key, "/v", value]);
            cmd.creation_flags(NO_WINDOW);
            let out = cmd.output().ok()?;
            let text = String::from_utf8_lossy(&out.stdout).into_owned();
            // Line shape: "    DriverDesc    REG_SZ    NVIDIA GeForce RTX 4090"
            //              "    ...qpMemorySize    REG_QWORD    0x180000000"
            let line = text
                .lines()
                .find(|l| l.trim_start().starts_with(value))?;
            let mut it = line.split_whitespace();
            let _name = it.next()?;
            let _type = it.next()?; // REG_SZ / REG_QWORD
            let rest = it.collect::<Vec<_>>().join(" ");
            if rest.is_empty() {
                None
            } else {
                Some(rest)
            }
        }

        // 0) nvidia-smi (ships with the NVIDIA driver) reports the true VRAM
        //    even for modded cards (>4 GB) where CIM truncates and some
        //    drivers leave the registry QWORD empty. Preferred when present.
        let mut smi = std::process::Command::new("nvidia-smi");
        smi.args([
            "--query-gpu=name,memory.total",
            "--format=csv,noheader,nounits",
        ]);
        smi.creation_flags(NO_WINDOW);
        if let Ok(out) = smi.output() {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout).into_owned();
                let mut best: Option<(String, u64)> = None; // (name, MiB)
                for line in text.lines() {
                    let line = line.trim();
                    if let Some((n, mib)) = line.rsplit_once(',') {
                        let name = n.trim().to_string();
                        if gpu_name_is_virtual(&name) {
                            continue;
                        }
                        let mib: u64 = mib.trim().parse().unwrap_or(0);
                        if mib > 0 && best.as_ref().map(|(_, b)| mib > *b).unwrap_or(true) {
                            best = Some((name, mib));
                        }
                    }
                }
                if let Some((name, mib)) = best {
                    return GpuInfo {
                        name: Some(name),
                        vram_total: Some(mib * 1024 * 1024),
                        unified: false,
                    };
                }
            }
        }

        // Enumerate every adapter under the display class (0000, 0001, …).
        let mut cmd = std::process::Command::new("reg");
        cmd.args(["query", DISPLAY_CLASS]);
        cmd.creation_flags(NO_WINDOW);
        let listing = cmd
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default();
        let subkeys: Vec<String> = listing
            .lines()
            .filter_map(|l| l.split_whitespace().last().map(|s| s.to_string()))
            .filter(|t| t.len() == 4 && t.chars().all(|c| c.is_ascii_digit()))
            .collect();

        struct Adapter {
            name: Option<String>,
            vram: Option<u64>,
        }
        let mut adapters: Vec<Adapter> = Vec::new();
        for sk in &subkeys {
            let key = format!("{DISPLAY_CLASS}\\{sk}");
            let name = reg_query(&key, "DriverDesc").filter(|s| !s.is_empty());
            let vram = reg_query(&key, "HardwareInformation.qpMemorySize")
                .and_then(|v| u64::from_str_radix(v.trim_start_matches("0x"), 16).ok())
                .filter(|&b| b > 0);
            if name.is_some() || vram.is_some() {
                adapters.push(Adapter { name, vram });
            }
        }

        // Skip virtual / software / helper display adapters — they otherwise
        // shadow the real GPU (e.g. "SudoMaker Virtual Display Adapter").
        let physical: Vec<&Adapter> = adapters
            .iter()
            .filter(|a| {
                a.name
                    .as_deref()
                    .map(|n| !gpu_name_is_virtual(n))
                    .unwrap_or(false)
            })
            .collect();
        let pool: Vec<&Adapter> = if physical.is_empty() {
            adapters.iter().collect()
        } else {
            physical
        };
        // The real GPU is the one with the most memory (dGPU over iGPU).
        pool.iter()
            .max_by_key(|a| a.vram.unwrap_or(0))
            .map(|a| GpuInfo {
                name: a.name.clone(),
                vram_total: a.vram,
                unified: false,
            })
            .unwrap_or_else(|| fall_back_to_cim_gpu())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_virtual_adapters() {
        assert!(gpu_name_is_virtual("SudoMaker Virtual Display Adapter"));
        assert!(gpu_name_is_virtual("Microsoft Basic Display Adapter"));
        assert!(gpu_name_is_virtual("Microsoft 基本显示适配器"));
        assert!(gpu_name_is_virtual("VMware SVGA 3D"));
        assert!(gpu_name_is_virtual("Indirect Display Driver Sample"));
        // Real GPUs must NOT be flagged.
        assert!(!gpu_name_is_virtual("NVIDIA GeForce RTX 4090"));
        assert!(!gpu_name_is_virtual("AMD Radeon RX 7900 XTX"));
        assert!(!gpu_name_is_virtual("Intel(R) Arc(TM) A770 Graphics"));
    }
}
