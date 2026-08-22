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

/// The base directory downloads should go to given current settings.
pub fn effective_download_root(cfg: &Config) -> PathBuf {
    match cfg.download_destination {
        DownloadDestination::Library => cfg.download_dir.clone(),
        DownloadDestination::LmStudio => lm_studio_dir(),
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
        ("LM Studio Models".into(), home.join(".lmstudio/models")),
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
