//! Local model discovery across HF / LM Studio / ModelScope caches,
//! the LalaLM library and user-defined search paths.

use crate::config::Config;
use crate::gguf::{self, GgufMeta};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub const ORIGIN_LIBRARY: &str = "lalalm-library";
pub const ORIGIN_HF: &str = "huggingface-cache";
pub const ORIGIN_LMS: &str = "lm-studio";
pub const ORIGIN_MS: &str = "modelscope-cache";
pub const ORIGIN_CUSTOM: &str = "custom";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalModel {
    pub id: String,
    pub name: String,
    pub repo: Option<String>,
    pub family: String,
    pub quant: Option<String>,
    pub file_path: PathBuf,
    pub file_name: String,
    pub size: u64,
    pub modified: u64,
    pub origin: String,
    pub kind: String,
    pub meta: Option<GgufMeta>,
}

const MAX_META_READS: usize = 400;

fn hash_id(s: &str) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    format!("{:016x}", h.finish())
}

fn kind_of(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    if lower.ends_with(".gguf") {
        Some("gguf")
    } else if lower.ends_with(".safetensors") {
        Some("safetensors")
    } else {
        None
    }
}

/// Recursively collect *.gguf / *.safetensors under `root`.
fn walk(root: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 8 || out.len() > 5000 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(root) else { return };
    for entry in rd.flatten() {
        let p = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            walk(&p, depth + 1, out);
        } else if ft.is_file() {
            if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                if kind_of(name).is_some() {
                    out.push(p);
                }
            }
        }
    }
}

struct Entry {
    path: PathBuf,
    origin: &'static str,
    repo: Option<String>,
    family: String,
}

fn push_entries(origin: &'static str, root: &Path, family_of: impl Fn(&Path) -> String, repo_of: impl Fn(&Path) -> Option<String>, out: &mut Vec<Entry>) {
    let mut files = Vec::new();
    walk(root, 0, &mut files);
    for f in files {
        out.push(Entry {
            family: family_of(&f),
            repo: repo_of(&f),
            path: f,
            origin,
        });
    }
}

pub fn list_local_models(cfg: &Config) -> Vec<LocalModel> {
    let cfg = cfg.clone();
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    let mut entries: Vec<Entry> = Vec::new();

    // 1. LalaLM library (default download dir).
    push_entries(
        ORIGIN_LIBRARY,
        &cfg.download_dir,
        |p| {
            p.parent()
                .and_then(|d| d.strip_prefix(&cfg.download_dir).ok())
                .map(|r| r.to_string_lossy().to_string())
                .unwrap_or_else(|| "Library".into())
        },
        |p| {
            p.parent()
                .and_then(|d| d.strip_prefix(&cfg.download_dir).ok())
                .map(|r| {
                    let parts: Vec<&str> = r
                        .components()
                        .map(|c| c.as_os_str().to_str().unwrap_or(""))
                        .collect();
                    parts.join("/")
                })
                .filter(|s| s.matches('/').count() >= 1)
        },
        &mut entries,
    );

    // 2. Hugging Face cache: models--{org}--{name}/snapshots/{sha}/files.
    if cfg.scan_hf_cache {
        let hub = home.join(".cache/huggingface/hub");
        if hub.is_dir() {
            if let Ok(rd) = std::fs::read_dir(&hub) {
                for mdir in rd.flatten() {
                    let dir_name = mdir.file_name().to_string_lossy().to_string();
                    let Some(repo) = decode_hf_repo(&dir_name) else { continue };
                    let snapshots = mdir.path().join("snapshots");
                    let mut files = Vec::new();
                    walk(&snapshots, 0, &mut files);
                    for f in files {
                        entries.push(Entry {
                            family: repo.split('/').next_back().unwrap_or(&repo).to_string(),
                            repo: Some(repo.clone()),
                            path: f,
                            origin: ORIGIN_HF,
                        });
                    }
                }
            }
        }
    }

    // 3. LM Studio models — resolved from LM Studio's own config
    //    (downloadsFolder) when present, else the well-known defaults.
    if cfg.scan_lm_studio {
        let mut lms_dirs = vec![crate::config::resolve_lm_studio_dir()];
        let home2 = home.join(".cache/lm-studio/models");
        if !lms_dirs.contains(&home2) {
            lms_dirs.push(home2); // legacy location, kept as a courtesy
        }
        for lms in lms_dirs {
            if lms.is_dir() {
                push_entries(
                    ORIGIN_LMS,
                    &lms,
                    |p| {
                        p.parent()
                            .and_then(|d| d.strip_prefix(&lms).ok())
                            .map(|r| r.to_string_lossy().to_string())
                            .unwrap_or_else(|| "LM Studio".into())
                    },
                    |p| {
                        p.parent()
                            .and_then(|d| d.strip_prefix(&lms).ok())
                            .map(|r| {
                                let parts: Vec<&str> = r
                                    .components()
                                    .map(|c| c.as_os_str().to_str().unwrap_or(""))
                                    .collect();
                                parts.join("/")
                            })
                            .filter(|s| s.matches('/').count() >= 1)
                    },
                    &mut entries,
                );
            }
        }
    }

    // 4. ModelScope cache.
    if cfg.scan_modelscope_cache {
        let ms = home.join(".cache/modelscope/hub");
        push_entries(
            ORIGIN_MS,
            &ms,
            |p| {
                p.parent()
                    .and_then(|d| d.strip_prefix(&ms).ok())
                    .map(|r| r.to_string_lossy().to_string())
                    .unwrap_or_else(|| "ModelScope".into())
            },
            |p| {
                // hub/models/{org}/{name}/... or {org}/{name}/...
                let rel = p.parent()?.strip_prefix(&ms).ok()?;
                let parts: Vec<&str> = rel
                    .components()
                    .map(|c| c.as_os_str().to_str().unwrap_or(""))
                    .collect();
                let parts: Vec<&str> = if parts.first() == Some(&"models") {
                    parts[1..].to_vec()
                } else {
                    parts
                };
                if parts.len() >= 2 {
                    Some(format!("{}/{}", parts[0], parts[1]))
                } else {
                    None
                }
            },
            &mut entries,
        );
    }

    // 5. User-defined search paths.
    for sp in &cfg.search_paths {
        if sp.is_dir() {
            push_entries(
                ORIGIN_CUSTOM,
                sp,
                |p| {
                    p.parent()
                        .and_then(|d| d.strip_prefix(sp).ok())
                        .map(|r| r.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Custom".into())
                },
                |_| None,
                &mut entries,
            );
        }
    }

    // Dedup by canonical path.
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut meta_reads = 0usize;
    let mut models: Vec<LocalModel> = Vec::new();
    for e in entries {
        let canon = e
            .path
            .canonicalize()
            .unwrap_or_else(|_| e.path.clone());
        if !seen.insert(canon) {
            continue;
        }
        let Some(kind) = e.path.file_name().and_then(|s| s.to_str()).and_then(kind_of)
        else {
            continue;
        };
        let md = match std::fs::metadata(&e.path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let file_name = e
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let meta = if kind == "gguf" && meta_reads < MAX_META_READS {
            meta_reads += 1;
            gguf::read_meta(&e.path)
        } else {
            None
        };

        let quant = meta
            .as_ref()
            .and_then(|m| m.quant.clone())
            .or_else(|| gguf::quant_from_filename(&file_name));
        let display = e
            .repo
            .as_ref()
            .map(|r| r.split('/').next_back().unwrap_or(r).to_string())
            .unwrap_or_else(|| {
                e.family
                    .split('/')
                    .next_back()
                    .unwrap_or(&e.family)
                    .to_string()
            });

        models.push(LocalModel {
            id: hash_id(&e.path.to_string_lossy()),
            name: display,
            repo: e.repo,
            family: e.family,
            quant,
            size: md.len(),
            modified: md
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0),
            file_path: e.path.clone(),
            file_name,
            origin: e.origin.into(),
            kind: kind.into(),
            meta,
        });
    }

    models.sort_by(|a, b| {
        a.family
            .cmp(&b.family)
            .then_with(|| b.size.cmp(&a.size))
            .then_with(|| a.file_name.cmp(&b.file_name))
    });
    models
}

/// `models--meta-llama--Llama-2-7b-hf` → `meta-llama/Llama-2-7b-hf`.
fn decode_hf_repo(dir_name: &str) -> Option<String> {
    let rest = dir_name.strip_prefix("models--")?;
    let mut it = rest.splitn(2, "--");
    let org = it.next()?;
    let name = it.next()?;
    if org.is_empty() || name.is_empty() {
        return None;
    }
    Some(format!("{org}/{name}"))
}

// ------------------------------------------------------------------ actions

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpResult {
    pub path: String,
    pub to: Option<String>,
    pub ok: bool,
    pub error: Option<String>,
}

/// Move model files/directories into `dest_dir` (cross-device safe).
pub fn move_models(paths: &[String], dest_dir: &str) -> Vec<OpResult> {
    let dest = PathBuf::from(dest_dir);
    let mut results = Vec::new();
    if std::fs::create_dir_all(&dest).is_err() {
        return paths
            .iter()
            .map(|p| OpResult {
                path: p.clone(),
                to: None,
                ok: false,
                error: Some("目标目录不可创建".into()),
            })
            .collect();
    }
    for p in paths {
        let src = PathBuf::from(p);
        let Some(file_name) = src.file_name().map(|s| s.to_owned()) else {
            continue;
        };
        let mut target = dest.join(&file_name);
        let mut n = 1;
        while target.exists() {
            n += 1;
            let stem = Path::new(&file_name)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let ext = Path::new(&file_name)
                .extension()
                .map(|s| format!(".{}", s.to_string_lossy()))
                .unwrap_or_default();
            target = dest.join(format!("{stem} ({n}){ext}"));
        }
        let ok = match std::fs::rename(&src, &target) {
            Ok(_) => true,
            Err(_) => match copy_recursive(&src, &target) {
                Ok(_) => {
                    let _ = trash_or_remove(&src);
                    true
                }
                Err(e) => {
                    results.push(OpResult {
                        path: p.clone(),
                        to: None,
                        ok: false,
                        error: Some(e),
                    });
                    continue;
                }
            },
        };
        results.push(OpResult {
            path: p.clone(),
            to: Some(target.to_string_lossy().to_string()),
            ok,
            error: None,
        });
    }
    results
}

fn copy_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    if src.is_dir() {
        std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
        let rd = std::fs::read_dir(src).map_err(|e| e.to_string())?;
        for e in rd.flatten() {
            copy_recursive(&e.path(), &dst.join(e.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(src, dst)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// Delete to OS Trash when possible; hard-delete as fallback.
pub fn delete_models(paths: &[String]) -> Vec<OpResult> {
    paths
        .iter()
        .map(|p| match trash_or_remove(Path::new(p)) {
            Ok(_) => OpResult {
                path: p.clone(),
                to: None,
                ok: true,
                error: None,
            },
            Err(e) => OpResult {
                path: p.clone(),
                to: None,
                ok: false,
                error: Some(e),
            },
        })
        .collect()
}

fn trash_or_remove(p: &Path) -> Result<(), String> {
    if !p.exists() {
        return Ok(());
    }
    match trash::delete(p) {
        Ok(_) => Ok(()),
        Err(_) => {
            let r = if p.is_dir() {
                std::fs::remove_dir_all(p)
            } else {
                std::fs::remove_file(p)
            };
            r.map_err(|e| e.to_string())
        }
    }
}

/// Directory sizes for cache overview (bytes per path).
pub async fn dir_sizes(paths: Vec<String>) -> HashMap<String, u64> {
    let mut handles = Vec::new();
    for p in paths {
        handles.push(tokio::task::spawn_blocking(move || {
            let size = dir_size(Path::new(&p));
            (p, size)
        }));
    }
    let mut out = HashMap::new();
    for h in handles {
        if let Ok((p, s)) = h.await {
            out.insert(p, s);
        }
    }
    out
}

fn dir_size(p: &Path) -> u64 {
    let mut total = 0;
    let Ok(rd) = std::fs::read_dir(p) else { return 0 };
    for e in rd.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            total += dir_size(&e.path());
        } else if let Ok(md) = e.metadata() {
            total += md.len();
        }
    }
    total
}
