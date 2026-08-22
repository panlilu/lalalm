//! Minimal GGUF header reader — extracts architecture / quantization /
//! context length without reading the whole file.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GgufMeta {
    pub architecture: Option<String>,
    pub name: Option<String>,
    pub size_label: Option<String>,
    pub quant: Option<String>,
    pub context_length: Option<u64>,
    pub block_count: Option<u64>,
    pub embedding_length: Option<u64>,
    pub head_count: Option<u64>,
    pub file_version: Option<u32>,
}

const MAGIC: u32 = 0x46554747; // "GGUF" LE

struct Reader<R: Read> {
    inner: R,
}

impl<R: Read> Reader<R> {
    fn bytes(&mut self, n: usize) -> std::io::Result<Vec<u8>> {
        let mut v = vec![0u8; n];
        self.inner.read_exact(&mut v)?;
        Ok(v)
    }
    fn u8v(&mut self) -> std::io::Result<u8> {
        Ok(self.bytes(1)?[0])
    }
    fn u16v(&mut self) -> std::io::Result<u16> {
        let b = self.bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32v(&mut self) -> std::io::Result<u32> {
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn u64v(&mut self) -> std::io::Result<u64> {
        let b = self.bytes(8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }
    fn f32v(&mut self) -> std::io::Result<f32> {
        Ok(f32::from_bits(self.u32v()?))
    }
    fn f64v(&mut self) -> std::io::Result<f64> {
        Ok(f64::from_bits(self.u64v()?))
    }
    fn string(&mut self) -> std::io::Result<String> {
        let len = self.u64v()? as usize;
        if len > 1 << 20 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "string too long",
            ));
        }
        let b = self.bytes(len)?;
        String::from_utf8(b).map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "utf8"))
    }
    /// Skip one value of the given GGUF metadata type (arrays included).
    fn skip_value(&mut self, ty: u32) -> std::io::Result<()> {
        match ty {
            0 | 7 => self.u8v().map(|_| ()),
            1 => self.u8v().map(|_| ()),
            2 | 3 => self.u16v().map(|_| ()),
            4 | 5 | 6 => self.u32v().map(|_| ()),
            8 => self.string().map(|_| ()),
            9 => {
                let elem = self.u32v()?;
                let count = self.u64v()?;
                if count > 50_000_000 {
                    return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "array too big"));
                }
                for _ in 0..count {
                    self.skip_value(elem)?;
                }
                Ok(())
            }
            10 | 11 | 12 => self.u64v().map(|_| ()),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "unknown gguf type",
            )),
        }
    }
}

/// llama.cpp `general.file_type` → human readable quant label.
pub fn quant_from_file_type(t: u64) -> Option<&'static str> {
    Some(match t {
        0 => "F32",
        1 => "F16",
        2 => "Q4_0",
        3 => "Q4_1",
        7 => "Q8_0",
        8 => "Q5_0",
        9 => "Q5_1",
        10 => "Q2_K",
        11 => "Q3_K_S",
        12 => "Q3_K_M",
        13 => "Q3_K_L",
        14 => "Q4_K_S",
        15 => "Q4_K_M",
        16 => "Q5_K_S",
        17 => "Q5_K_M",
        18 => "Q6_K",
        19 => "IQ2_XXS",
        20 => "IQ2_XS",
        21 => "Q2_K_S",
        22 => "IQ3_XS",
        23 => "IQ3_XXS",
        24 => "IQ1_S",
        25 => "IQ4_NL",
        26 => "IQ3_S",
        27 => "IQ3_M",
        28 => "IQ4_XS",
        29 => "IQ1_M",
        30 => "BF16",
        _ => return None,
    })
}

static QUANT_RE: once_cell::sync::Lazy<regex::Regex> = once_cell::sync::Lazy::new(|| {
    regex::Regex::new(r"(?i)(iq[0-9]+(?:_[a-z0-9]+)*|q[0-9]+(?:_[a-z0-9]+)*|bf16|fp16|f16|fp32|f32)")
        .unwrap()
});

/// Guess quantization from a file name like `Model-7B-Q4_K_M.gguf`.
pub fn quant_from_filename(name: &str) -> Option<String> {
    let stem = name.trim_end_matches(".gguf").trim_end_matches(".GGUF");
    let mut best: Option<&str> = None;
    for m in QUANT_RE.find_iter(stem) {
        let s = m.as_str();
        // Prefer the longest (most specific) match, e.g. Q4_K_M over Q4.
        if best.map(|b| s.len() >= b.len()).unwrap_or(true) {
            best = Some(s);
        }
    }
    best.map(|s| s.to_uppercase())
}

/// Rank quantization labels by practical quality preference (lower = better
/// starting point for a "recommended" pick). Mirrors the frontend list.
pub fn quant_rank(q: &str) -> u32 {
    const ORDER: [&str; 26] = [
        "F32", "BF16", "F16", "Q8_0", "Q6_K", "Q5_K_M", "Q5_K_S", "Q5_0", "Q5_1",
        "Q4_K_M", "Q4_K_S", "IQ4_XS", "IQ4_NL", "Q4_0", "Q4_1", "Q3_K_L", "Q3_K_M",
        "Q3_K_S", "IQ3_M", "IQ3_S", "IQ3_XS", "IQ3_XXS", "Q2_K", "Q2_K_S", "IQ2",
        "IQ1",
    ];
    let up = q.to_uppercase();
    for (i, k) in ORDER.iter().enumerate() {
        if up == *k || up.starts_with(k) {
            return i as u32;
        }
    }
    500
}

static PARAMS_RE: once_cell::sync::Lazy<regex::Regex> = once_cell::sync::Lazy::new(|| {
    regex::Regex::new(r"(?i)(\d+(?:\.\d+)?)\s*([bmk])\b").unwrap()
});

/// Extract a parameter count label ("7B", "1.5B", "30M"…) from a model name.
pub fn params_from_name(name: &str) -> Option<String> {
    let mut best: Option<(f64, String)> = None;
    for c in PARAMS_RE.captures_iter(name) {
        let num: f64 = c[1].parse().ok()?;
        let unit = c[2].to_uppercase();
        let score = match unit.as_str() {
            "B" => num,
            "M" => num / 1000.0,
            "K" => num / 1_000_000.0,
            _ => 0.0,
        };
        if best.as_ref().map(|b| score > b.0).unwrap_or(true) {
            best = Some((score, format!("{num}{unit}")));
        }
    }
    best.map(|(_, s)| s)
}

/// Parse the GGUF header of `path`. Returns `None` for non-GGUF/corrupt files.
pub fn read_meta(path: &Path) -> Option<GgufMeta> {
    let f = std::fs::File::open(path).ok()?;
    let mut r = Reader {
        inner: std::io::BufReader::with_capacity(1 << 16, f),
    };
    if r.u32v().ok()? != MAGIC {
        return None;
    }
    let version = r.u32v().ok()?;
    if !(1..=3).contains(&version) {
        return None;
    }
    let _tensor_count = r.u64v().ok()?;
    let kv_count = r.u64v().ok()?;
    if kv_count > 100_000 {
        return None;
    }

    let mut meta = GgufMeta {
        file_version: Some(version),
        ..Default::default()
    };

    for _ in 0..kv_count {
        let key = r.string().ok()?;
        let ty = r.u32v().ok()?;
        match ty {
            0 => {
                r.u8v().ok()?;
            }
            1 => {
                let v = r.u8v().ok()?; // i8
                if key == "general.file_type" {
                    meta.quant = quant_from_file_type(v as i8 as i64 as u64).map(String::from);
                }
            }
            2 => {
                r.u16v().ok()?;
            }
            3 => {
                r.u16v().ok()?;
            }
            4 | 6 => {
                let v = r.u32v().ok()? as u64;
                note_int(&mut meta, &key, v);
            }
            5 => {
                r.f32v().ok()?;
            }
            7 => {
                r.u8v().ok()?;
            }
            8 => {
                let v = r.string().ok()?;
                match key.as_str() {
                    "general.architecture" => meta.architecture = Some(v),
                    "general.name" => meta.name = Some(v),
                    "general.size_label" => meta.size_label = Some(v),
                    _ => {}
                }
            }
            9 => {
                r.skip_value(9).ok()?;
            }
            10 => {
                let v = r.u64v().ok()?;
                note_int(&mut meta, &key, v);
            }
            11 => {
                let v = r.u64v().ok()?; // i64 read as u64
                note_int(&mut meta, &key, v);
            }
            12 => {
                r.f64v().ok()?;
            }
            _ => return None,
        }
    }

    if meta.quant.is_none() {
        if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
            meta.quant = quant_from_filename(name);
        }
    }
    Some(meta)
}

fn note_int(meta: &mut GgufMeta, key: &str, v: u64) {
    if key == "general.file_type" {
        meta.quant = quant_from_file_type(v).map(String::from);
        return;
    }
    let suffix_of = |prefix: &str, key: &str| key.strip_prefix(prefix).map(|s| s.to_string());
    // architecture-specific keys, e.g. llama.context_length
    if let Some(rest) = key.split_once('.').map(|(_, r)| r) {
        match rest {
            "context_length" => meta.context_length = Some(v),
            "block_count" => meta.block_count = Some(v),
            "embedding_length" => meta.embedding_length = Some(v),
            "attention.head_count" => meta.head_count = Some(v),
            _ => {}
        }
    }
    let _ = suffix_of("", key);
}

/// Convenience: map of raw metadata for display purposes.
#[allow(dead_code)]
pub fn summarize(meta: &GgufMeta) -> HashMap<String, String> {
    let mut m = HashMap::new();
    if let Some(v) = &meta.architecture {
        m.insert("architecture".into(), v.clone());
    }
    if let Some(v) = &meta.name {
        m.insert("name".into(), v.clone());
    }
    if let Some(v) = &meta.size_label {
        m.insert("sizeLabel".into(), v.clone());
    }
    if let Some(v) = meta.context_length {
        m.insert("contextLength".into(), v.to_string());
    }
    if let Some(v) = meta.block_count {
        m.insert("blocks".into(), v.to_string());
    }
    if let Some(v) = meta.embedding_length {
        m.insert("embedding".into(), v.to_string());
    }
    if let Some(v) = &meta.quant {
        m.insert("quant".into(), v.clone());
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid GGUF byte stream with a few metadata KVs.
    fn synth_gguf() -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&MAGIC.to_le_bytes()); // magic
        v.extend_from_slice(&3u32.to_le_bytes()); // version
        v.extend_from_slice(&2u64.to_le_bytes()); // tensor count
        v.extend_from_slice(&4u64.to_le_bytes()); // kv count

        let put_str = |v: &mut Vec<u8>, s: &str| {
            v.extend_from_slice(&(s.len() as u64).to_le_bytes());
            v.extend_from_slice(s.as_bytes());
        };
        let kv_str = |v: &mut Vec<u8>, k: &str, s: &str| {
            put_str(v, k);
            v.extend_from_slice(&8u32.to_le_bytes());
            put_str(v, s);
        };
        let kv_u32 = |v: &mut Vec<u8>, k: &str, n: u32| {
            put_str(v, k);
            v.extend_from_slice(&4u32.to_le_bytes());
            v.extend_from_slice(&n.to_le_bytes());
        };

        kv_str(&mut v, "general.architecture", "llama");
        kv_str(&mut v, "general.name", "TestModel-7B");
        kv_u32(&mut v, "llama.context_length", 32768);
        kv_u32(&mut v, "general.file_type", 15); // Q4_K_M
        v
    }

    #[test]
    fn parse_synth_header() {
        let dir = std::env::temp_dir().join("lalalm-gguf-test");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("TestModel-7B-Q4_K_M.gguf");
        std::fs::write(&p, synth_gguf()).unwrap();

        let meta = read_meta(&p).expect("should parse");
        assert_eq!(meta.architecture.as_deref(), Some("llama"));
        assert_eq!(meta.name.as_deref(), Some("TestModel-7B"));
        assert_eq!(meta.context_length, Some(32768));
        assert_eq!(meta.quant.as_deref(), Some("Q4_K_M"));
    }

    #[test]
    fn quant_from_filenames() {
        assert_eq!(
            quant_from_filename("Meta-Llama-3-8B-Instruct-Q4_K_M.gguf"),
            Some("Q4_K_M".into())
        );
        assert_eq!(quant_from_filename("model-IQ4_XS.gguf"), Some("IQ4_XS".into()));
        assert_eq!(quant_from_filename("qwen2.5-q8_0.gguf"), Some("Q8_0".into()));
    }

    #[test]
    fn params_from_names() {
        assert_eq!(params_from_name("Qwen2.5-7B-Instruct").as_deref(), Some("7B"));
        assert_eq!(params_from_name("SmolLM2-1.7B-GGUF").as_deref(), Some("1.7B"));
        assert_eq!(params_from_name("nothing-here"), None);
    }

    #[test]
    fn rejects_non_gguf() {
        let dir = std::env::temp_dir().join("lalalm-gguf-test");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("not-gguf.bin");
        std::fs::write(&p, b"garbage data not gguf at all......").unwrap();
        assert!(read_meta(&p).is_none());
    }
}
