//! Remote model hub clients: Hugging Face / hf-mirror (same API) and ModelScope.

use crate::config::{encode_path, Config, ProxyMode, Source};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSummary {
    pub repo: String,
    pub author: String,
    pub name: String,
    pub source: Source,
    pub downloads: u64,
    pub likes: u64,
    pub last_modified: String,
    pub tags: Vec<String>,
    pub pipeline_tag: Option<String>,
    pub gguf: bool,
    pub params: Option<String>,
    pub avatar: Option<String>,
}

// Organization avatar cache (author → avatar URL).
static AVATAR_CACHE: once_cell::sync::Lazy<
    std::sync::Mutex<std::collections::HashMap<String, String>>,
> = once_cell::sync::Lazy::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

fn cached_avatar(author: &str) -> Option<String> {
    AVATAR_CACHE.lock().unwrap().get(author).cloned()
}

fn store_avatar(author: &str, url: &str) {
    AVATAR_CACHE
        .lock()
        .unwrap()
        .insert(author.to_string(), url.to_string());
}

/// Fetch an org avatar from the HF users API (`base` may be huggingface.co
/// or the hf-mirror — both serve the same accounts). Cached per author.
/// Example: Qwen →
/// https://cdn-avatars.huggingface.co/v1/production/uploads/6215ca5692c0ecfba9186921/….jpeg
pub(crate) async fn fetch_org_avatar(
    http: &reqwest::Client,
    base: &'static str,
    author: &str,
) -> Option<String> {
    // Empty-string cache entry = known-absent (negative cache).
    if let Some(u) = cached_avatar(author) {
        return if u.is_empty() { None } else { Some(u) };
    }
    if author.is_empty() || author.contains('/') {
        return None;
    }

    // HF serves ORGANIZATION avatars and USER avatars from two DIFFERENT
    // endpoints (/api/organizations/X/avatar vs /api/users/X/avatar — the
    // users one answers "This user does not exist" for every org like Qwen,
    // deepseek-ai, mlx-community). Try both paths on the primary host, then
    // fall back to huggingface.co directly (hf-mirror does not proxy the
    // organizations endpoint).
    let enc = crate::config::encode_path(author);
    let mut urls: Vec<String> = Vec::with_capacity(4);
    for b in [base, "https://huggingface.co"] {
        urls.push(format!("{b}/api/organizations/{enc}/avatar"));
        urls.push(format!("{b}/api/users/{enc}/avatar"));
    }
    urls.dedup();
    for url in &urls {
        let resp = match http.get(url).timeout(std::time::Duration::from_secs(4)).send().await {
            Ok(r) => r,
            Err(_) => continue,
        };
        if !resp.status().is_success() {
            continue;
        }
        let v = match resp.json::<Value>().await {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(u) = v["avatarUrl"].as_str() {
            store_avatar(author, u);
            return Some(u.to_string());
        }
    }
    store_avatar(author, ""); // negative cache so we don't retry every search
    None
}

/// Fill in org avatars for a result list (concurrently, capped, cached).
/// Works for HF-family results directly; ModelScope publishers are looked up
/// by the same account name on HF (most orgs mirror across both hubs).
async fn fill_org_avatars(list: &mut [ModelSummary], http: reqwest::Client, src: Source) {
    let base: &'static str = match src {
        Source::ModelScope => Source::HfMirror.api_base(),
        _ => src.api_base(),
    };
    let mut todo: Vec<String> = Vec::new();
    for item in list.iter() {
        if item.avatar.is_some()
            || item.author.is_empty()
            || item.author.contains('/')
            || cached_avatar(&item.author).is_some()
        {
            continue;
        }
        if !todo.contains(&item.author) {
            todo.push(item.author.clone());
            if todo.len() >= 48 {
                break;
            }
        }
    }
    let handles = todo.into_iter().map(|author| {
        let http = http.clone();
        tokio::spawn(async move {
            fetch_org_avatar(&http, base, &author).await;
            author
        })
    });
    for h in handles {
        let _ = h.await;
    }
    for item in list.iter_mut() {
        if item.avatar.is_none() {
            item.avatar = cached_avatar(&item.author);
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelFile {
    pub path: String,
    pub size: u64,
    pub quant: Option<String>,
    pub is_gguf: bool,
    pub role: FileRole,
}

impl ModelFile {
    fn new(path: String, size: u64) -> Self {
        let is_gguf = path.to_lowercase().ends_with(".gguf");
        let quant = if is_gguf {
            crate::gguf::quant_from_filename(&path)
        } else {
            None
        };
        Self {
            role: classify_role(&path),
            is_gguf,
            quant,
            path,
            size,
        }
    }
}

/// What a repository distributes: llama.cpp GGUF weights, Apple MLX
/// weights, or plain transformers tensors (safetensors/bin).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelFormat {
    Gguf,
    Mlx,
    Tensor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FileRole {
    #[allow(dead_code)]
    Weights,
    Mmproj,
    Config,
    Tokenizer,
    Other,
}

pub fn classify_role(path: &str) -> FileRole {
    let low = path.to_lowercase();
    let name = low.rsplit('/').next().unwrap_or(&low);
    if name.contains("mmproj") && name.ends_with(".gguf") {
        return FileRole::Mmproj;
    }
    match name {
        "config.json"
        | "generation_config.json"
        | "params.json"
        | "configuration.json" => FileRole::Config,
        "tokenizer.json"
        | "tokenizer.model"
        | "tokenizer_config.json"
        | "special_tokens_map.json"
        | "vocab.json"
        | "merges.txt"
        | "sentencepiece.bpe.model"
        | "chat_template.jinja"
        | "chat_template.json"
        | "preprocessor_config.json"
        | "processor_config.json" => FileRole::Tokenizer,
        _ => {
            if name.starts_with("configuration") && name.ends_with(".json") {
                FileRole::Config
            } else {
                FileRole::Other
            }
        }
    }
}

/// One user-facing downloadable unit: a quantization (GGUF) or weight set
/// (MLX / safetensors), plus optional companion files.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Variant {
    pub id: String,
    pub label: String,
    pub quant: Option<String>,
    pub files: Vec<ModelFile>,
    pub total_size: u64,
    pub companions: Vec<ModelFile>,
    pub companions_size: u64,
    pub recommended: bool,
}

static SHARD_RE: once_cell::sync::Lazy<regex::Regex> = once_cell::sync::Lazy::new(|| {
    regex::Regex::new(r"-\d{5}-of-\d{5}\.gguf$").unwrap()
});

fn detect_format(files: &[ModelFile], tags: &[String]) -> ModelFormat {
    if files.iter().any(|f| f.is_gguf) {
        return ModelFormat::Gguf;
    }
    if tags.iter().any(|t| t.eq_ignore_ascii_case("mlx")) {
        return ModelFormat::Mlx;
    }
    ModelFormat::Tensor
}

/// Group repository files into user-facing variants.
fn build_variants(format: ModelFormat, files: &[ModelFile]) -> Vec<Variant> {
    match format {
        ModelFormat::Gguf => build_gguf_variants(files),
        ModelFormat::Mlx | ModelFormat::Tensor => build_tensor_variants(files),
    }
}

/// Repos may ship several vision projectors (mmproj-f16 / bf16 / f32 …).
/// Only ONE is worth downloading: prefer the standard `f16`, then bf16 /
/// quantized variants, tie-breaking on the smaller file. Returns None when
/// the repo has no mmproj files.
fn pick_best_mmproj(candidates: &[&ModelFile]) -> Option<ModelFile> {
    let rank = |p: &str| -> u8 {
        let l = p.to_lowercase();
        let name = l.rsplit('/').next().unwrap_or(&l);
        if name.contains("f16") {
            5
        } else if name.contains("bf16") {
            4
        } else if name.contains("q8") {
            3
        } else if name.contains("q6") || name.contains("q5") {
            2
        } else if name.contains("f32") {
            1 // huge and unnecessary — only picked if it's all there is
        } else {
            0
        }
    };
    candidates
        .iter()
        .max_by_key(|f| (rank(&f.path), std::cmp::Reverse(f.size)))
        .map(|f| (*f).clone())
}

fn build_gguf_variants(files: &[ModelFile]) -> Vec<Variant> {
    use std::collections::BTreeMap;
    // group key = shard-normalized file name; preserves multi-shard quants
    let mut groups: BTreeMap<String, Vec<ModelFile>> = BTreeMap::new();
    for f in files {
        if !f.is_gguf || classify_role(&f.path) == FileRole::Mmproj {
            continue;
        }
        let base = SHARD_RE.replace(&f.path, ".gguf").to_string();
        groups.entry(base).or_default().push(f.clone());
    }

    let mut companions: Vec<ModelFile> = files
        .iter()
        .filter(|f| matches!(classify_role(&f.path), FileRole::Config | FileRole::Tokenizer))
        .cloned()
        .collect();
    // One vision projector max — the best fit for this machine.
    let mmproj_all: Vec<&ModelFile> = files
        .iter()
        .filter(|f| classify_role(&f.path) == FileRole::Mmproj)
        .collect();
    if let Some(best) = pick_best_mmproj(&mmproj_all) {
        companions.push(best);
    }
    let companions_size = companions.iter().map(|f| f.size).sum();

    let mut variants: Vec<Variant> = groups
        .into_iter()
        .map(|(base, mut gfiles)| {
            gfiles.sort_by(|a, b| a.path.cmp(&b.path));
            let name_only = base.rsplit('/').next().unwrap_or(&base);
            let quant = crate::gguf::quant_from_filename(name_only);
            let label = quant.clone().unwrap_or_else(|| name_only.to_string());
            let total = gfiles.iter().map(|f| f.size).sum();
            Variant {
                id: base.clone(),
                label,
                quant,
                files: gfiles,
                total_size: total,
                companions_size,
                recommended: false,
                companions: companions.clone(),
            }
        })
        .collect();

    variants.sort_by(|a, b| {
        let ra = a.quant.as_deref().map(crate::gguf::quant_rank).unwrap_or(500);
        let rb = b.quant.as_deref().map(crate::gguf::quant_rank).unwrap_or(500);
        ra.cmp(&rb).then_with(|| b.total_size.cmp(&a.total_size))
    });
    if let Some(v) = variants.iter_mut().find(|v| {
        v.quant.as_deref().is_some_and(|q| {
            crate::gguf::quant_rank(q) == crate::gguf::quant_rank("Q4_K_M")
        })
    }) {
        v.recommended = true;
    } else if let Some(v) = variants.first_mut() {
        v.recommended = true;
    }
    variants
}

fn build_tensor_variants(files: &[ModelFile]) -> Vec<Variant> {
    // Weight sets grouped by their parent directory (mlx repos ship multiple
    // quantizations as subfolders like `4bit/`, `8bit/`, `bf16/`).
    use std::collections::BTreeMap;
    let is_weight = |f: &ModelFile| -> bool {
        let low = f.path.to_lowercase();
        let name = low.rsplit('/').next().unwrap_or("");
        (name.ends_with(".safetensors") || name.ends_with(".bin") || name.ends_with(".npz"))
            && classify_role(&f.path) != FileRole::Mmproj
    };
    let mut groups: BTreeMap<String, Vec<ModelFile>> = BTreeMap::new();
    for f in files.iter().filter(|f| is_weight(f)) {
        let dir = match f.path.rfind('/') {
            Some(i) => f.path[..i].to_string(),
            None => String::new(),
        };
        groups.entry(dir).or_default().push(f.clone());
    }
    // Repos without recognizable weight files still deserve one variant.
    if groups.is_empty() && !files.is_empty() {
        groups.insert(String::new(), vec![]);
    }

    let mut companions: Vec<ModelFile> = files
        .iter()
        .filter(|f| matches!(classify_role(&f.path), FileRole::Config | FileRole::Tokenizer))
        .cloned()
        .collect();
    // One vision projector max — the best fit for this machine.
    let mmproj_all: Vec<&ModelFile> = files
        .iter()
        .filter(|f| classify_role(&f.path) == FileRole::Mmproj)
        .collect();
    if let Some(best) = pick_best_mmproj(&mmproj_all) {
        companions.push(best);
    }
    let companions_size = companions.iter().map(|f| f.size).sum();

    let mut variants: Vec<Variant> = groups
        .into_iter()
        .map(|(dir, mut gfiles)| {
            gfiles.sort_by(|a, b| a.path.cmp(&b.path));
            let label = if dir.is_empty() {
                "全量权重".to_string()
            } else {
                dir.rsplit('/').next().unwrap_or(&dir).to_string()
            };
            let total = gfiles.iter().map(|f| f.size).sum();
            Variant {
                id: if dir.is_empty() { "root".into() } else { dir },
                label,
                quant: None,
                files: gfiles,
                total_size: total,
                companions_size,
                recommended: false,
                companions: companions.clone(),
            }
        })
        .collect();

    variants.sort_by(|a, b| a.total_size.cmp(&b.total_size));
    if let Some(v) = variants.first_mut() {
        v.recommended = true;
    }
    variants
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDetail {
    pub summary: ModelSummary,
    pub format: ModelFormat,
    pub variants: Vec<Variant>,
    pub files: Vec<ModelFile>,
    pub readme_md: Option<String>,
    pub gguf_total_size: u64,
    pub all_total_size: u64,
}


/// ModelScope returns update times as unix seconds (number); normalize to
/// "YYYY-MM-DD" so the frontend's formatter shows a real date.
fn ms_time_to_string(v: &Value) -> String {
    if let Some(s) = v.as_str() {
        return s.to_string();
    }
    if let Some(secs) = v.as_i64() {
        let days = secs.div_euclid(86400);
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = if m <= 2 { y + 1 } else { y };
        return format!("{y:04}-{m:02}-{d:02}");
    }
    String::new()
}

pub struct HubClient {
    /// Proxy-aware client (system / manual proxy) — used for HF-family hubs.
    http: reqwest::Client,
    /// Always-direct client. ModelScope is a China-hosted service; forcing
    /// it through a system proxy (Clash etc.) breaks requests, so all
    /// modelscope.cn traffic bypasses the proxy entirely.
    http_direct: reqwest::Client,
}

impl Clone for HubClient {
    fn clone(&self) -> Self {
        Self {
            http: self.http.clone(),
            http_direct: self.http_direct.clone(),
        }
    }
}

/// Parse the output of `scutil --proxy` and return a proxy URL if enabled.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_scutil_proxy(output: &str) -> Option<String> {
    let mut map = std::collections::HashMap::new();
    for line in output.lines() {
        let line = line.trim().trim_end_matches(';').trim();
        if let Some((k, v)) = line.split_once(':') {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    let get = |k: &str| map.get(k).cloned();
    let enabled = |k: &str| get(k).map(|v| v == "1").unwrap_or(false);

    if enabled("HTTPSEnable") {
        if let Some(url) =
            assemble_http_url(&get("HTTPSProxy"), &get("HTTPSPort"))
        {
            return Some(url);
        }
    }
    if enabled("HTTPEnable") {
        if let Some(url) = assemble_http_url(&get("HTTPProxy"), &get("HTTPPort")) {
            return Some(url);
        }
    }
    if enabled("SOCKSEnable") {
        if let Some(url) =
            assemble_socks_url(&get("SOCKSProxy"), &get("SOCKSPort"))
        {
            return Some(url);
        }
    }
    None
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn assemble_http_url(host: &Option<String>, port: &Option<String>) -> Option<String> {
    let host = host.as_deref()?.trim().to_string();
    if host.is_empty() || host == "(null)" {
        return None;
    }
    let port: u16 = port.as_deref().unwrap_or("").parse().ok()?;
    if port == 0 {
        return None;
    }
    Some(format!("http://{host}:{port}"))
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn assemble_socks_url(host: &Option<String>, port: &Option<String>) -> Option<String> {
    let host = host.as_deref()?.trim().to_string();
    if host.is_empty() || host == "(null)" {
        return None;
    }
    let port: u16 = port.as_deref().unwrap_or("").parse().ok()?;
    if port == 0 {
        return None;
    }
    Some(format!("socks5://{host}:{port}"))
}

/// Parse `reg query "HKCU\...\Internet Settings"` output for the proxy.
/// ProxyServer is either "host:port" or a map "http=…;https=…;socks=…".
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub fn parse_windows_reg_proxy(text: &str) -> Option<String> {
    let mut enabled = false;
    let mut server: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("ProxyEnable") {
            enabled = rest.split_whitespace().last() == Some("0x1");
        } else if let Some(rest) = line.strip_prefix("ProxyServer") {
            // Format: "ProxyServer    REG_SZ    <value>"
            let mut it = rest.split_whitespace();
            let _type = it.next();
            if let Some(v) = it.next() {
                server = Some(v.to_string());
            }
        }
    }
    if !enabled {
        return None;
    }
    let raw = server?;
    if raw.contains('=') {
        // protocol map; prefer https, then http, then socks
        for key in ["https=", "http=", "socks="] {
            for part in raw.split(';') {
                let part = part.trim();
                if let Some(v) = part.strip_prefix(key) {
                    if !v.is_empty() {
                        let scheme = key.trim_end_matches('=');
                        return Some(format!("{scheme}://{v}"));
                    }
                }
            }
        }
        None
    } else if !raw.is_empty() {
        Some(format!("http://{raw}"))
    } else {
        None
    }
}

/// Detect the system-wide proxy (macOS Network settings / Windows Internet
/// settings registry), with an env-var fallback everywhere else.
pub fn detect_system_proxy() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        if let Ok(out) = std::process::Command::new("scutil").arg("--proxy").output() {
            if let Ok(text) = String::from_utf8(out.stdout) {
                if let Some(p) = parse_scutil_proxy(&text) {
                    return Some(p);
                }
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        let mut cmd = std::process::Command::new("reg");
        cmd.args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
        ]);
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        if let Ok(out) = cmd.output() {
            let text = String::from_utf8_lossy(&out.stdout).into_owned();
            if let Some(p) = parse_windows_reg_proxy(&text) {
                return Some(p);
            }
        }
    }
    // Fallback: conventional environment variables.
    for k in ["https_proxy", "HTTPS_PROXY", "all_proxy", "ALL_PROXY", "http_proxy", "HTTP_PROXY"] {
        if let Ok(v) = std::env::var(k) {
            let v = v.trim().to_string();
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    None
}

/// Resolve the effective proxy URL for a given config (None = direct).
pub fn effective_proxy(cfg: &Config) -> Option<String> {
    match cfg.proxy_mode {
        ProxyMode::Direct => None,
        ProxyMode::Manual => {
            let u = cfg.proxy_url.trim().to_string();
            if u.is_empty() { None } else { Some(u) }
        }
        ProxyMode::System => detect_system_proxy(),
    }
}

impl HubClient {
    /// Build a client honoring the configured proxy mode.
    /// - System: use the macOS system proxy when present (otherwise keep
    ///   reqwest's default env-var handling);
    /// - Direct: never proxy;
    /// - Manual: use the given proxy URL (http/https/socks5).
    pub fn build(mode: ProxyMode, manual_url: &str) -> Self {
        let mut b = reqwest::Client::builder()
            .user_agent("LalaLM/0.1 (+https://github.com/lalalm)")
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(25));
        match mode {
            ProxyMode::Direct => {
                b = b.no_proxy();
            }
            ProxyMode::Manual => {
                let url = manual_url.trim();
                if !url.is_empty() {
                    if let Ok(p) = reqwest::Proxy::all(url) {
                        b = b.no_proxy().proxy(p);
                    }
                }
            }
            ProxyMode::System => {
                if let Some(url) = detect_system_proxy() {
                    if let Ok(p) = reqwest::Proxy::all(&url) {
                        b = b.no_proxy().proxy(p);
                    }
                }
            }
        }
        let direct = reqwest::Client::builder()
            .user_agent("LalaLM/0.1 (+https://github.com/lalalm)")
            .no_proxy()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(25))
            .build()
            .expect("reqwest direct client");
        Self {
            http: b.build().expect("reqwest client"),
            http_direct: direct,
        }
    }

    fn auth(&self, src: Source, cfg: &Config) -> Option<String> {
        src.auth_header(cfg)
    }

    pub async fn get_json(&self, url: &str, auth: Option<&str>) -> Result<Value, String> {
        self.get_json_via(&self.http, url, auth).await
    }

    async fn get_json_via(
        &self,
        client: &reqwest::Client,
        url: &str,
        auth: Option<&str>,
    ) -> Result<Value, String> {
        let mut req = client.get(url);
        if let Some(a) = auth {
            req = req.header("Authorization", a);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| format!("网络请求失败: {e}"))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(format!("HTTP {status} — {url}"));
        }
        resp.json::<Value>()
            .await
            .map_err(|e| format!("响应解析失败: {e}"))
    }

    async fn get_text(&self, url: &str, auth: Option<&str>) -> Result<String, String> {
        self.get_text_via(&self.http, url, auth).await
    }

    async fn get_text_via(
        &self,
        client: &reqwest::Client,
        url: &str,
        auth: Option<&str>,
    ) -> Result<String, String> {
        let mut req = client.get(url);
        if let Some(a) = auth {
            req = req.header("Authorization", a);
        }
        let resp = req.send().await.map_err(|e| format!("网络请求失败: {e}"))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(format!("HTTP {status}"));
        }
        resp.text().await.map_err(|e| e.to_string())
    }

    // ---------------------------------------------------------------- search

    pub async fn search(
        &self,
        src: Source,
        query: &str,
        sort: &str,
        gguf_only: bool,
        limit: u32,
        cfg: &Config,
    ) -> Result<Vec<ModelSummary>, String> {
        match src {
            Source::HuggingFace | Source::HfMirror => {
                self.search_hf(src, query, sort, gguf_only, limit, cfg).await
            }
            Source::ModelScope => {
                self.search_ms(query, sort, limit, gguf_only).await
            }
        }
    }

    async fn search_hf(
        &self,
        src: Source,
        query: &str,
        sort: &str,
        gguf_only: bool,
        limit: u32,
        cfg: &Config,
    ) -> Result<Vec<ModelSummary>, String> {
        // trendingScore is not accepted by every endpoint; fall back gracefully.
        for sort_key in [sort, "downloads"] {
            let mut params = vec![
                ("limit", limit.to_string()),
                ("sort", sort_key.to_string()),
                ("direction", "-1".to_string()),
            ];
            if !query.trim().is_empty() {
                params.push(("search", query.trim().to_string()));
            }
            if gguf_only {
                params.push(("filter", "gguf".to_string()));
            }
            let url = format!("{}/api/models?{}", src.api_base(), serde_urlencoded(&params));
            let auth = self.auth(src, cfg);
            match self.get_json(&url, auth.as_deref()).await {
                Ok(v) => {
                    let arr = v.as_array().cloned().unwrap_or_default();
                    let mut out: Vec<ModelSummary> =
                        arr.iter().filter_map(|m| hf_summary(m, src)).collect();
                    if !out.is_empty() {
                        let http = self.http.clone();
                        fill_org_avatars(&mut out, http, src).await;
                        return Ok(out);
                    }
                    if arr.is_empty() {
                        return Ok(Vec::new());
                    }
                }
                Err(e) => {
                    if sort_key == "downloads" {
                        return Err(e);
                    }
                }
            }
        }
        Ok(Vec::new())
    }

    async fn search_ms(
        &self,
        query: &str,
        _sort: &str,
        limit: u32,
        gguf_only: bool,
    ) -> Result<Vec<ModelSummary>, String> {
        let q = query.trim();
        // ModelScope uses the PUT /api/v1/dolphin/models search API.
        // NOTE: the filter key is "Name" — "Query" is silently ignored and
        // returns the unfiltered full catalog. Space-separated terms are
        // AND-ed ("qwen2.5 gguf" → only matching GGUF models). SortBy must
        // stay "Default" (other values are rejected by the API).
        let name_q = if gguf_only {
            let lower = q.to_lowercase();
            if lower.contains("gguf") {
                q.to_string()
            } else if q.is_empty() {
                "gguf".to_string()
            } else {
                format!("{q} gguf")
            }
        } else {
            q.to_string()
        };
        let body = serde_json::json!({
            "Name": name_q,
            "PageSize": limit,
            "PageNumber": 1,
            "SortBy": "Default",
        });
        let resp = self
            .http_direct
            .put("https://modelscope.cn/api/v1/dolphin/models")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("ModelScope 请求失败: {e}"))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(format!("ModelScope HTTP {status}"));
        }
        let v: Value = resp.json().await.map_err(|e| e.to_string())?;
        let models = v["Data"]["Model"]["Models"]
            .as_array()
            .or_else(|| v["Data"]["Models"].as_array())
            .cloned()
            .unwrap_or_default();
        let mut out: Vec<ModelSummary> = models.iter().filter_map(ms_summary).collect();
        if !out.is_empty() {
            // ModelScope has no public org-icon API; publishers usually mirror
            // their HF account, so look the org up by the same name there.
            let http = self.http.clone();
            fill_org_avatars(&mut out, http, Source::ModelScope).await;
        }
        Ok(out)
    }

    // ---------------------------------------------------------------- detail
    /// Resolve the org/user avatar for a publisher on `src`.
    pub async fn org_avatar(
        &self,
        src: Source,
        author: &str,
        cfg: &Config,
    ) -> Option<String> {
        let _ = cfg;
        let base: &'static str = match src {
            Source::ModelScope => Source::HfMirror.api_base(),
            _ => src.api_base(),
        };
        let http = match src {
            Source::ModelScope => &self.http_direct,
            _ => &self.http,
        };
        // ModelScope has no public org-icon API; resolve via HF under the
        // same account name (the http_direct client reaches hf-mirror fine
        // and the chain falls back to huggingface.co directly).
        fetch_org_avatar(http, base, author).await
    }

    /// Light-weight check whether `repo` exists on `src`.
    pub async fn repo_exists(
        &self,
        src: Source,
        repo: &str,
        cfg: &Config,
    ) -> Result<bool, String> {
        let auth = self.auth(src, cfg);
        match src {
            Source::HuggingFace | Source::HfMirror => {
                let url = format!("{}/api/models/{}", src.api_base(), encode_path(repo));
                match self.get_json_status(&url, auth.as_deref()).await {
                    Ok(status) => Ok(status),
                    Err(_) => Ok(false),
                }
            }
            Source::ModelScope => {
                let (org, name) = match repo.split_once('/') {
                    Some((o, n)) => (o, n),
                    None => ("", repo),
                };
                if org.is_empty() || name.is_empty() {
                    return Ok(false);
                }
                let url = format!(
                    "https://modelscope.cn/api/v1/models/{}/{}",
                    encode_path(org),
                    encode_path(name)
                );
                match self
                    .get_json_via(&self.http_direct, &url, auth.as_deref())
                    .await
                {
                    Ok(v) => Ok(v["Code"].as_i64() == Some(200) && v["Data"].is_object()),
                    Err(_) => Ok(false),
                }
            }
        }
    }

    async fn get_json_status(&self, url: &str, auth: Option<&str>) -> Result<bool, String> {
        let mut req = self.http.get(url);
        if let Some(a) = auth {
            req = req.header("Authorization", a);
        }
        let resp = req.send().await.map_err(|e| format!("网络请求失败: {e}"))?;
        Ok(resp.status().is_success())
    }

    pub async fn detail(
        &self,
        src: Source,
        repo: &str,
        cfg: &Config,
    ) -> Result<ModelDetail, String> {
        match src {
            Source::HuggingFace | Source::HfMirror => self.detail_hf(src, repo, cfg).await,
            Source::ModelScope => self.detail_ms(repo, cfg).await,
        }
    }

    async fn detail_hf(
        &self,
        src: Source,
        repo: &str,
        cfg: &Config,
    ) -> Result<ModelDetail, String> {
        let auth = self.auth(src, cfg);
        let info_url = format!("{}/api/models/{}", src.api_base(), encode_path(repo));
        let info = self.get_json(&info_url, auth.as_deref()).await?;

        let tree_url = format!(
            "{}/api/models/{}/tree/main?recursive=true",
            src.api_base(),
            encode_path(repo)
        );
        let tree = self.get_json(&tree_url, auth.as_deref())
            .await
            .unwrap_or(Value::Array(vec![]));

        let mut files: Vec<ModelFile> = Vec::new();
        if let Some(arr) = tree.as_array() {
            for entry in arr {
                if entry["type"].as_str() != Some("file") {
                    continue;
                }
                let path = entry["path"].as_str().unwrap_or_default().to_string();
                if path.is_empty() {
                    continue;
                }
                let size = entry["lfs"]["size"]
                    .as_u64()
                    .or(entry["size"].as_u64())
                    .unwrap_or(0);
                files.push(ModelFile::new(path, size));
            }
        }
        files.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.path.cmp(&b.path)));

        let readme_md = self
            .get_text(
                &format!("{}/{}/raw/main/README.md", src.api_base(), encode_path(repo)),
                auth.as_deref(),
            )
            .await
            .ok();

        let mut summary = hf_summary(&info, src).ok_or("模型信息解析失败")?;
        // Org icon for the detail hero (same lookup as the search grid).
        if summary.avatar.is_none() {
            summary.avatar =
                fetch_org_avatar(&self.http, src.api_base(), &summary.author).await;
        }
        Ok(finalize_detail(summary, files, readme_md))
    }

    async fn detail_ms(&self, repo: &str, cfg: &Config) -> Result<ModelDetail, String> {
        let auth = self.auth(Source::ModelScope, cfg);
        let parts: Vec<&str> = repo.splitn(2, '/').collect();
        let (org, name) = match parts.as_slice() {
            [o, n] => (*o, *n),
            _ => ("", repo),
        };
        let enc_repo = format!("{}/{}", encode_path(org), encode_path(name));
        let files_url = format!(
            "https://modelscope.cn/api/v1/models/{}/repo/files?Recursive=true&Revision=master",
            enc_repo
        );
        let tree = self
            .get_json_via(&self.http_direct, &files_url, auth.as_deref())
            .await
            .map_err(|e| format!("获取文件列表失败: {e}"))?;

        let mut files: Vec<ModelFile> = Vec::new();
        let farr = tree["Data"]["Files"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for entry in farr {
            let ty = entry["Type"].as_str().unwrap_or("");
            if ty != "blob" && ty != "file" {
                continue;
            }
            let path = entry["Path"].as_str().unwrap_or("").to_string();
            if path.is_empty() {
                continue;
            }
            let size = entry["Size"].as_u64().unwrap_or(0);
            files.push(ModelFile::new(path, size));
        }
        files.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.path.cmp(&b.path)));

        let readme_md = self
            .get_text_via(
                &self.http_direct,
                &format!("https://modelscope.cn/models/{enc_repo}/resolve/master/README.md"),
                auth.as_deref(),
            )
            .await
            .ok();

        // Best-effort metadata (downloads / likes).
        let mut summary = ModelSummary {
            repo: repo.to_string(),
            author: org.to_string(),
            name: name.to_string(),
            source: Source::ModelScope,
            downloads: 0,
            likes: 0,
            last_modified: String::new(),
            tags: Vec::new(),
            pipeline_tag: None,
            gguf: files.iter().any(|f| f.is_gguf),
            params: crate::gguf::params_from_name(name),
            avatar: None,
        };
        let info_url = format!("https://modelscope.cn/api/v1/models/{}", enc_repo);
        if let Ok(v) = self
            .get_json_via(&self.http_direct, &info_url, auth.as_deref())
            .await
        {
            let d = &v["Data"];
            summary.downloads = d["Downloads"].as_u64().unwrap_or(0);
            summary.likes = d["Likes"].as_u64().or(d["Stars"].as_u64()).unwrap_or(0);
            summary.last_modified = {
                let t = ms_time_to_string(&d["LastUpdatedTime"]);
                if t.is_empty() {
                    ms_time_to_string(&d["CreateTime"])
                } else {
                    t
                }
            };
            summary.tags = d["Tags"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|t| t.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
        }
        if summary.avatar.is_none() {
            // ModelScope has no public org-icon API; try the same publisher
            // name on the HF mirror (most orgs mirror across hubs).
            let author = summary.author.clone();
            summary.avatar =
                fetch_org_avatar(&self.http, Source::HfMirror.api_base(), &author).await;
        }
        Ok(finalize_detail(summary, files, readme_md))
    }
}

fn finalize_detail(
    summary: ModelSummary,
    files: Vec<ModelFile>,
    readme_md: Option<String>,
) -> ModelDetail {
    let gguf_total_size = files.iter().filter(|f| f.is_gguf).map(|f| f.size).sum();
    let all_total_size = files.iter().map(|f| f.size).sum();
    let format = detect_format(&files, &summary.tags);
    let variants = build_variants(format, &files);
    ModelDetail {
        summary,
        format,
        variants,
        files,
        readme_md: readme_md.filter(|r| !r.is_empty()).map(|r| {
            if r.len() > 600_000 {
                format!("{}\n\n<!-- truncated -->", &r[..600_000])
            } else {
                r
            }
        }),
        gguf_total_size,
        all_total_size,
    }
}

fn hf_summary(m: &Value, src: Source) -> Option<ModelSummary> {
    let id = m["id"].as_str()?.to_string();
    let (author, name) = match id.split_once('/') {
        Some((a, n)) => (a.to_string(), n.to_string()),
        None => (String::new(), id.clone()),
    };
    let tags: Vec<String> = m["tags"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|t| t.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let gguf = tags.iter().any(|t| t.eq_ignore_ascii_case("gguf"));
    Some(ModelSummary {
        avatar: None,
        params: crate::gguf::params_from_name(&name),
        gguf,
        pipeline_tag: m["pipeline_tag"].as_str().map(|s| s.to_string()),
        downloads: m["downloads"].as_u64().unwrap_or(0),
        likes: m["likes"].as_u64().unwrap_or(0),
        last_modified: m["lastModified"]
            .as_str()
            .or(m["createdAt"].as_str())
            .unwrap_or("")
            .to_string(),
        repo: id,
        author,
        name,
        source: src,
        tags,
    })
}

fn ms_summary(m: &Value) -> Option<ModelSummary> {
    let path = m["Path"].as_str().unwrap_or("");
    let name = m["Name"].as_str()?;
    let repo = if path.is_empty() {
        name.to_string()
    } else {
        format!("{path}/{name}")
    };
    let tags: Vec<String> = m["Tags"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|t| t.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    Some(ModelSummary {
        repo,
        author: path.to_string(),
        name: name.to_string(),
        source: Source::ModelScope,
        // NOTE: the list API's "Avatar" is the model's own cover image, not
        // the publisher org icon — leave it unset (letter fallback instead).
        avatar: None,
        downloads: m["Downloads"].as_u64().unwrap_or(0),
        likes: m["Likes"].as_u64().or(m["Stars"].as_u64()).unwrap_or(0),
        last_modified: {
            let t = ms_time_to_string(&m["LastUpdatedTime"]);
            if t.is_empty() {
                let t2 = ms_time_to_string(&m["UpdateTime"]);
                if t2.is_empty() {
                    ms_time_to_string(&m["CreatedAt"])
                } else {
                    t2
                }
            } else {
                t
            }
        },
        pipeline_tag: m["Tasks"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|t| t.as_str())
            .map(|s| s.to_string()),
        gguf: tags.iter().any(|t| t.to_lowercase().contains("gguf"))
            || name.to_lowercase().contains("gguf"),
        params: crate::gguf::params_from_name(name),
        tags,
    })
}

/// Tiny `application/x-www-form-urlencoded` serializer to avoid an extra dep.
fn serde_urlencoded(params: &[(&str, String)]) -> String {
    params
        .iter()
        .map(|(k, v)| {
            format!(
                "{}={}",
                urlencode(k),
                urlencode(v)
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<dictionary> {
  HTTPEnable : 1
  HTTPPort : 7890
  HTTPProxy : 127.0.0.1
  HTTPSEnable : 1
  HTTPSPort : 7890
  HTTPSProxy : 127.0.0.1
  SOCKSEnable : 0
  SOCKSPort : 0
  SOCKSProxy : (null)
  ExcludeList : (null)
}"#;

    #[test]
    fn parses_scutil_output() {
        let p = parse_scutil_proxy(SAMPLE).expect("should detect");
        assert_eq!(p, "http://127.0.0.1:7890");
    }

    #[test]
    fn no_proxy_when_disabled() {
        let text = "HTTPEnable : 0\nHTTPSEnable : 0\nSOCKSEnable : 0";
        assert!(parse_scutil_proxy(text).is_none());
    }

    #[test]
    fn socks_fallback() {
        let text = "HTTPEnable : 0\nHTTPSEnable : 0\nSOCKSEnable : 1\nSOCKSProxy : 10.0.0.2\nSOCKSPort : 1080";
        assert_eq!(
            parse_scutil_proxy(text),
            Some("socks5://10.0.0.2:1080".to_string())
        );
    }

    #[test]
    fn windows_reg_proxy() {
        let enabled = "\r\n    ProxyEnable    REG_DWORD    0x1\r\n    ProxyServer    REG_SZ    127.0.0.1:7890\r\n";
        assert_eq!(
            parse_windows_reg_proxy(enabled),
            Some("http://127.0.0.1:7890".to_string())
        );
        let mapped = "ProxyEnable    REG_DWORD    0x1\nProxyServer    REG_SZ    http=1.2.3.4:8080;https=1.2.3.4:8443;socks=1.2.3.4:1080";
        assert_eq!(
            parse_windows_reg_proxy(mapped),
            Some("https://1.2.3.4:8443".to_string())
        );
        let off = "ProxyEnable    REG_DWORD    0x0\nProxyServer    REG_SZ    127.0.0.1:7890";
        assert_eq!(parse_windows_reg_proxy(off), None);
    }

    pub(super) fn mf(path: &str, size: u64) -> ModelFile {
        ModelFile::new(path.to_string(), size)
    }

    #[test]
    fn builds_gguf_variants() {
        let files = vec![
            mf("README.md", 1000),
            mf("config.json", 700),
            mf("tokenizer.json", 7_000_000),
            mf("mmproj-model-f16.gguf", 400_000_000),
            mf("Model-7B-Q4_K_M.gguf", 4_400_000_000),
            mf("Model-7B-Q8_0.gguf", 8_000_000_000),
            mf("Model-7B-Q6_K-00001-of-00002.gguf", 3_000_000_000),
            mf("Model-7B-Q6_K-00002-of-00002.gguf", 3_000_000_000),
        ];
        let format = detect_format(&files, &["gguf".to_string()]);
        assert_eq!(format, ModelFormat::Gguf);
        let v = build_variants(format, &files);

        // Sorted by quant quality: Q8_0 < Q6_K < Q4_K_M.
        let labels: Vec<&str> = v.iter().map(|x| x.label.as_str()).collect();
        assert_eq!(labels, vec!["Q8_0", "Q6_K", "Q4_K_M"]);
        // Recommended is Q4_K_M even though it sorts last.
        let rec = v.iter().find(|x| x.recommended).unwrap();
        assert_eq!(rec.label, "Q4_K_M");
        // Shards grouped together in one variant, in order.
        let q6 = v.iter().find(|x| x.label == "Q6_K").unwrap();
        assert_eq!(q6.files.len(), 2);
        assert_eq!(q6.total_size, 6_000_000_000);
        // Companions: mmproj + config + tokenizer, never weights/readme.
        assert!(rec.companions.iter().any(|f| f.path == "mmproj-model-f16.gguf"));
        assert!(rec.companions.iter().any(|f| f.path == "config.json"));
        assert!(!rec.files.iter().any(|f| f.path.contains("mmproj")));
    }

    #[test]
    fn builds_tensor_variants_with_mlx_dirs() {
        let files = vec![
            mf("config.json", 700),
            mf("tokenizer.json", 7_000_000),
            mf("model.safetensors", 1_000_000_000),
            mf("4bit/model.safetensors", 250_000_000),
            mf("8bit/model.safetensors", 500_000_000),
            mf("4bit/tokenizer_config.json", 100),
        ];
        let format = detect_format(&files, &["mlx".to_string()]);
        assert_eq!(format, ModelFormat::Mlx);
        let v = build_variants(format, &files);

        // Smallest first; root weights form their own "全量权重" variant.
        let labels: Vec<&str> = v.iter().map(|x| x.label.as_str()).collect();
        assert_eq!(labels, vec!["4bit", "8bit", "全量权重"]);
        assert_eq!(v[0].total_size, 250_000_000);
        assert!(v[0].recommended);
        // Config/tokenizer attached as companions on every variant.
        assert!(v[0].companions.iter().any(|f| f.path == "config.json"));
    }
}

#[cfg(test)]
mod variant_tests {
    use super::tests::mf;
    use super::*;

    #[test]
    fn picks_single_f16_mmproj() {
        let files = vec![
            mf("config.json", 700),
            mf("mmproj-model-f32.gguf", 1_600_000_000),
            mf("mmproj-model-f16.gguf", 800_000_000),
            mf("mmproj-project-bf16.gguf", 800_000_000),
            mf("Model-Q4_K_M.gguf", 4_000_000_000),
        ];
        let v = build_variants(ModelFormat::Gguf, &files);
        let rec = v.iter().find(|x| x.recommended).unwrap();
        let mm: Vec<&ModelFile> =
            rec.companions.iter().filter(|c| c.role == FileRole::Mmproj).collect();
        assert_eq!(mm.len(), 1, "exactly one mmproj should be offered");
        assert!(mm[0].path.contains("f16"));
        assert_eq!(rec.total_size, 4_000_000_000);
    }

    #[test]
    fn no_mmproj_means_no_companion() {
        let files = vec![
            mf("Model-f16.gguf", 4_000_000_000),
            mf("tokenizer.json", 7_000_000),
        ];
        let v = build_variants(ModelFormat::Gguf, &files);
        assert!(v[0].companions.iter().all(|c| c.role != FileRole::Mmproj));
    }
}

#[cfg(test)]
mod live_probe {
    use super::*;

    /// Manual live probe: `cargo test live_avatar_probe -- --ignored --nocapture`
    /// Prints what the app's own avatar chain resolves for a real search.
    #[tokio::test]
    #[ignore]
    async fn live_avatar_probe() {
        let client = HubClient::build(ProxyMode::System, "");
        let cfg = Config::default();
        for src in [Source::HfMirror, Source::ModelScope] {
            let out = client
                .search(src, "qwen", "downloads", true, 8, &cfg)
                .await
                .expect("search failed");
            println!("--- {src:?} ({} 结果)", out.len());
            for m in out.iter().take(8) {
                println!(
                    "  {:<18} avatar={}",
                    m.author,
                    match &m.avatar {
                        Some(u) => &u[..],
                        None => "(无)",
                    }
                );
            }
        }
    }
}
