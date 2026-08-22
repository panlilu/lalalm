//! Application configuration: sources, tokens, directories, aria2 options.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    HuggingFace,
    HfMirror,
    ModelScope,
}

/// UI appearance: follow the OS, or force dark / light.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ThemeMode {
    #[default]
    System,
    Dark,
    Light,
}

/// Proxy behavior: follow macOS system proxy (default), direct connection,
/// or a manually specified proxy URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ProxyMode {
    #[default]
    System,
    Direct,
    Manual,
}

/// Where new downloads land: the LalaLM library, or straight into the
/// LM Studio models directory (same publisher/model/file layout, so LM
/// Studio picks the files up immediately).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum DownloadDestination {
    #[default]
    Library,
    LmStudio,
}

/// LM Studio's default models directory.
pub fn lm_studio_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".lmstudio")
        .join("models")
}

/// Extract `downloadsFolder` from LM Studio config JSON text.
/// Tolerates nested layouts and escaped Windows separators.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn extract_downloads_folder(text: &str) -> Option<String> {
    static RE: once_cell::sync::Lazy<regex::Regex> = once_cell::sync::Lazy::new(|| {
        regex::Regex::new(r#""downloadsFolder"\s*:\s*"((?:[^"\\]|\\.)*)""#).unwrap()
    });
    let caps = RE.captures(text)?;
    let raw = caps.get(1)?.as_str();
    let unescaped = raw.replace("\\\\", "\\");
    let trimmed = unescaped.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Decode possibly-UTF-16 (BOM) or UTF-8 config bytes into a lossy string.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn decode_config_bytes(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = bytes[2..]
            .chunks(2)
            .filter(|c| c.len() == 2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..]
            .chunks(2)
            .filter(|c| c.len() == 2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// Search the well-known LM Studio config locations for `downloadsFolder`.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn discover_downloads_folder() -> Option<String> {
    let home = dirs::home_dir()?;
    let mut candidates: Vec<PathBuf> = vec![
        // Plain config FILE at ~/.cache/lm-studio (observed on Windows).
        home.join(".cache/lm-studio"),
    ];
    // Directory-style installs: scan *.json up to depth 3.
    let appdata = std::env::var("APPDATA").map(PathBuf::from).ok();
    let mut dirs: Vec<PathBuf> = vec![
        home.join(".lmstudio/.internal"),
        home.join(".config/LM Studio"),
        home.join("Library/Application Support/LM Studio"),
    ];
    if let Some(ad) = appdata {
        dirs.push(ad.join("LM Studio"));
    }
    for d in dirs {
        if d.is_dir() {
            let ok = std::fs::read_dir(&d).is_ok();
            if ok {
                collect_json_files(&d, 0, &mut candidates);
            }
        }
    }
    for c in candidates {
        let Ok(bytes) = std::fs::read(&c) else {
            continue;
        };
        if !bytes.starts_with(b"{") && !bytes.starts_with(&[0xFF, 0xFE]) && !bytes.starts_with(&[0xFE, 0xFF]) {
            continue;
        }
        let text = decode_config_bytes(&bytes);
        if let Some(f) = extract_downloads_folder(&text) {
            return Some(f);
        }
    }
    None
}

fn collect_json_files(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 2 || out.len() > 64 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_json_files(&p, depth + 1, out);
        } else if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("json")) {
            out.push(p);
        }
    }
}

/// Resolve the actual LM Studio models directory on this machine.
///
/// Order:
/// 1. `LM_STUDIO_MODELS_DIR` environment override,
/// 2. LM Studio's own config — `downloadsFolder` key (config file
///    `~/.cache/lm-studio` or the app's settings JSON), e.g. `F:\lm-studio-models`,
/// 3. the default `~/.lmstudio/models`,
/// 4. the legacy `~/.cache/lm-studio/models`.
pub fn resolve_lm_studio_dir() -> PathBuf {
    if let Ok(v) = std::env::var("LM_STUDIO_MODELS_DIR") {
        let p = PathBuf::from(v.trim());
        if p.is_dir() {
            return p;
        }
    }
    if let Some(dir) = discover_downloads_folder() {
        let p = PathBuf::from(dir);
        if p.is_dir() {
            return p;
        }
    }
    let default = lm_studio_dir();
    if default.is_dir() {
        return default;
    }
    home_dir_join(".cache/lm-studio/models")
}

fn home_dir_join(rel: &str) -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(rel)
}

/// The base directory downloads should go to given current settings.
pub fn effective_download_root(cfg: &Config) -> PathBuf {
    match cfg.download_destination {
        DownloadDestination::Library => cfg.download_dir.clone(),
        DownloadDestination::LmStudio => resolve_lm_studio_dir(),
    }
}

impl Default for Source {
    fn default() -> Self {
        Source::HuggingFace
    }
}

impl Source {
    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            Source::HuggingFace => "Hugging Face",
            Source::HfMirror => "hf-mirror",
            Source::ModelScope => "ModelScope",
        }
    }

    /// Base URL used for REST API + raw file access (HF-compatible endpoints).
    pub fn api_base(&self) -> &'static str {
        match self {
            Source::HuggingFace => "https://huggingface.co",
            Source::HfMirror => "https://hf-mirror.com",
            Source::ModelScope => "https://modelscope.cn",
        }
    }

    /// Direct download URL for a single file inside a repository.
    pub fn file_url(&self, repo: &str, path: &str) -> String {
        match self {
            Source::HuggingFace | Source::HfMirror => format!(
                "{}/{}/resolve/main/{}",
                self.api_base(),
                repo,
                encode_path(path)
            ),
            Source::ModelScope => format!(
                "https://modelscope.cn/models/{}/resolve/master/{}",
                repo,
                encode_path(path)
            ),
        }
    }

    pub fn auth_header(&self, cfg: &Config) -> Option<String> {
        match self {
            Source::HuggingFace | Source::HfMirror => {
                let t = cfg.hf_token.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(format!("Bearer {t}"))
                }
            }
            Source::ModelScope => {
                let t = cfg.modelscope_token.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(format!("Bearer {t}"))
                }
            }
        }
    }
}

/// Percent-encode a URL path segment list, keeping `/` separators.
pub fn encode_path(p: &str) -> String {
    let mut out = String::with_capacity(p.len());
    for b in p.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Aria2Config {
    /// When false the app falls back to its internal single-stream downloader.
    pub enabled: bool,
    pub max_connection_per_server: u32,
    pub split: u32,
    pub min_split_size: String,
    pub max_concurrent_downloads: u32,
}

impl Default for Aria2Config {
    fn default() -> Self {
        Self {
            enabled: true,
            max_connection_per_server: 8,
            split: 8,
            min_split_size: "4M".into(),
            max_concurrent_downloads: 4,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub source: Source,
    pub hf_token: String,
    pub modelscope_token: String,
    pub download_dir: PathBuf,
    pub search_paths: Vec<PathBuf>,
    pub scan_hf_cache: bool,
    pub scan_lm_studio: bool,
    pub scan_modelscope_cache: bool,
    pub recent_searches: Vec<String>,
    pub suggest_queries: Vec<String>,
    pub aria2: Aria2Config,
    pub proxy_mode: ProxyMode,
    pub proxy_url: String,
    pub download_destination: DownloadDestination,
    pub theme: ThemeMode,
}

impl Default for Config {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        Self {
            source: Source::default(),
            hf_token: String::new(),
            modelscope_token: String::new(),
            download_dir: home.join(".lalalm").join("models"),
            search_paths: Vec::new(),
            scan_hf_cache: true,
            scan_lm_studio: true,
            scan_modelscope_cache: true,
            recent_searches: Vec::new(),
            suggest_queries: default_suggestions(),
            aria2: Aria2Config::default(),
            proxy_mode: ProxyMode::default(),
            proxy_url: String::new(),
            download_destination: DownloadDestination::default(),
            theme: ThemeMode::default(),
        }
    }
}

pub fn default_suggestions() -> Vec<String> {
    vec![
        "llama 3 gguf".into(),
        "qwen2.5 gguf".into(),
        "deepseek-r1 gguf".into(),
        "phi-4 gguf".into(),
        "gemma 2 gguf".into(),
        "mistral gguf".into(),
        "embedding bge gguf".into(),
    ]
}

/// Well-known model cache locations on this machine (shown in Settings /
/// On Device and scanned according to toggles).
pub fn known_cache_paths(cfg: &Config) -> Vec<(String, PathBuf)> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    let mut v: Vec<(String, PathBuf)> = vec![
        ("LalaLM Library".into(), cfg.download_dir.clone()),
        (
            "Hugging Face Cache".into(),
            home.join(".cache/huggingface/hub"),
        ),
        ("LM Studio Models".into(), resolve_lm_studio_dir()),
        (
            "LM Studio (legacy)".into(),
            home.join(".cache/lm-studio/models"),
        ),
        (
            "ModelScope Cache".into(),
            home.join(".cache/modelscope/hub"),
        ),
        ("llama.cpp (custom)".into(), home.join(".llama.cpp/models")),
    ];
    for p in &cfg.search_paths {
        v.push(("Custom Path".into(), p.clone()));
    }
    v
}

pub fn config_path(app_data: &Path) -> PathBuf {
    app_data.join("config.json")
}

pub fn load(app_data: &Path) -> Config {
    let p = config_path(app_data);
    match std::fs::read_to_string(&p) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Config::default(),
    }
}

pub fn save(app_data: &Path, cfg: &Config) -> Result<(), String> {
    let p = config_path(app_data);
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let s = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(&p, s).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_downloads_folder() {
        let sample = r#"{
  "language": "zh_CN",
  "downloadsFolder": "F:\\lm-studio-models",
  "sidebar": { "showButtonNames": false },
  "developerMode": true
}"#;
        assert_eq!(
            extract_downloads_folder(sample),
            Some("F:\\lm-studio-models".to_string())
        );
        // Plain single-backslash form also accepted.
        assert_eq!(
            extract_downloads_folder(r#"{"downloadsFolder":"D:/models"}"#),
            Some("D:/models".to_string())
        );
        assert_eq!(extract_downloads_folder("{}"), None);
        assert_eq!(extract_downloads_folder(r#"{"downloadsFolder":""}"#), None);
    }
}
