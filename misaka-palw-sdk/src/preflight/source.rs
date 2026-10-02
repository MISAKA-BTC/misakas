//! **What a model preflight reads, and nothing else** (RFC-0002 Part II §II.2.1).
//!
//! A Hugging Face directory: `config.json`, the presence and size of the files beside it,
//! `model.safetensors.index.json` and the HEADER of each safetensors shard (the 8-byte length and the JSON
//! header). A `.gguf` file: the magic, the metadata and the tensor table. **Never the data region of any
//! file**: a shard may be absent, or truncated after its header, and a GGUF may be a header alone; the
//! report says what it could and could not read.

use misaka_palw_tir_lower::LowerError;
use misaka_palw_tir_lower::gguf::{GgufFile, GgufModel};
use misaka_palw_tir_lower::hf_schema::{TensorEntry, TensorIndex};
use misaka_palw_tir_lower::quantfmt::QuantRegistry;
use misaka_palw_tir_lower::weights::{Tensor, TensorMeta, TensorSource};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Seek, SeekFrom};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// The most a safetensors header may weigh before it is refused (a hostile length is never allocated).
const MAX_HEADER_BYTES: u64 = 256 << 20;

/// What kind of thing was given.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InputKind {
    HfDirectory,
    ConfigFile,
    Gguf,
    Artifact,
    /// A model repository read by HTTP ranges ([`super::remote`]): `config.json`, the index and the shard headers.
    Remote,
}

impl InputKind {
    pub fn label(self) -> &'static str {
        match self {
            InputKind::HfDirectory => "Hugging Face directory",
            InputKind::ConfigFile => "config.json",
            InputKind::Gguf => "GGUF file",
            InputKind::Artifact => ".palwtir artifact",
            InputKind::Remote => "model repository (HTTP ranges)",
        }
    }
}

/// Which input a path is: a directory with a `config.json` (or a lone GGUF), a `.gguf`, a `config.json`, or a
/// PALWTIR1 container.
pub fn detect(path: &Path) -> Result<InputKind, String> {
    if path.is_dir() {
        if path.join("config.json").exists() {
            return Ok(InputKind::HfDirectory);
        }
        if misaka_palw_tir_lower::fidelity::gguf_path(path).is_some() {
            return Ok(InputKind::Gguf);
        }
        return Err(format!("{}: a directory with neither a config.json nor a model.gguf", path.display()));
    }
    if !path.exists() {
        return Err(format!("{}: no such file or directory", path.display()));
    }
    match path.extension().and_then(|e| e.to_str()) {
        Some("gguf") => Ok(InputKind::Gguf),
        Some("palwtir") => Ok(InputKind::Artifact),
        Some("json") => Ok(InputKind::ConfigFile),
        _ if crate::tir_manifest::PalwTirManifestV1::sniff(path) => Ok(InputKind::Artifact),
        _ => Err(format!("{}: not a model directory, a config.json, a .gguf or a .palwtir artifact", path.display())),
    }
}

/// A file beside the checkpoint, by name and size.
#[derive(Clone, Debug, Serialize)]
pub struct FileInfo {
    pub name: String,
    pub bytes: u64,
}

/// One tensor of a safetensors header.
#[derive(Clone, Debug)]
pub struct ShardEntry {
    pub dtype: String,
    pub shape: Vec<usize>,
    /// Bytes the header says the tensor's data takes.
    pub bytes: u64,
    /// Offset of the data inside the shard's data region.
    pub start: u64,
}

/// The header of one safetensors shard, read without its data.
#[derive(Clone, Debug)]
pub struct ShardHeader {
    pub file: String,
    pub path: PathBuf,
    /// 8 + the JSON header's length.
    pub header_bytes: u64,
    /// The data region's size, as the header declares it.
    pub declared_data_bytes: u64,
    /// The file's size on disk.
    pub file_bytes: u64,
    pub entries: BTreeMap<String, ShardEntry>,
    /// SHA-256 of the header bytes (the 8-byte length included): what a pack can pin.
    pub header_sha256: String,
}

impl ShardHeader {
    /// Whether the file holds its whole data region (else it is a header, or a partial download).
    pub fn complete(&self) -> bool {
        self.file_bytes >= self.header_bytes + self.declared_data_bytes
    }
}

/// Read the header of a safetensors file: the length, the JSON, and nothing after it. Lenient about the data
/// region (it may be absent).
pub fn read_safetensors_header(path: &Path) -> Result<ShardHeader, String> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string();
    let mut f = std::fs::File::open(path).map_err(|e| format!("{name}: {e}"))?;
    let file_bytes = f.metadata().map_err(|e| format!("{name}: {e}"))?.len();
    let mut len = [0u8; 8];
    f.read_exact(&mut len).map_err(|e| format!("{name}: {e} (not a safetensors file)"))?;
    let n = u64::from_le_bytes(len);
    if n == 0 || n > MAX_HEADER_BYTES {
        return Err(format!("{name}: a header of {n} bytes is not a safetensors header"));
    }
    let mut json = vec![0u8; n as usize];
    f.read_exact(&mut json).map_err(|e| format!("{name}: the header is cut short: {e}"))?;
    let v: serde_json::Value = serde_json::from_slice(&json).map_err(|e| format!("{name}: the header is not JSON: {e}"))?;
    let obj = v.as_object().ok_or_else(|| format!("{name}: the header is not an object"))?;
    let mut entries = BTreeMap::new();
    let mut declared = 0u64;
    for (k, e) in obj {
        if k == "__metadata__" {
            continue;
        }
        let dtype = e.get("dtype").and_then(|d| d.as_str()).ok_or_else(|| format!("{name}: `{k}` has no dtype"))?.to_string();
        let shape: Vec<usize> = e
            .get("shape")
            .and_then(|s| s.as_array())
            .ok_or_else(|| format!("{name}: `{k}` has no shape"))?
            .iter()
            .map(|d| d.as_u64().map(|d| d as usize).ok_or_else(|| format!("{name}: `{k}` has a shape that is not integers")))
            .collect::<Result<_, _>>()?;
        let off = e.get("data_offsets").and_then(|o| o.as_array()).filter(|o| o.len() == 2);
        let (a, b) = match off.map(|o| (o[0].as_u64(), o[1].as_u64())) {
            Some((Some(a), Some(b))) if a <= b => (a, b),
            _ => return Err(format!("{name}: `{k}` has no valid data_offsets")),
        };
        declared = declared.max(b);
        entries.insert(k.clone(), ShardEntry { dtype, shape, bytes: b - a, start: a });
    }
    let mut h = Sha256::new();
    h.update(len);
    h.update(&json);
    Ok(ShardHeader {
        file: name,
        path: path.to_path_buf(),
        header_bytes: 8 + n,
        declared_data_bytes: declared,
        file_bytes,
        entries,
        header_sha256: hex(&h.finalize()),
    })
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// A checkpoint known by its headers: tensor metadata from the shards, byte ranges served only where the bytes are
/// on disk. Enough for the frontend's binding check and for the few small role tensors a descriptor reads.
pub struct HeaderSource {
    entries: BTreeMap<String, (ShardEntry, PathBuf, u64)>,
}

impl HeaderSource {
    pub fn new(shards: &[ShardHeader]) -> HeaderSource {
        let mut entries = BTreeMap::new();
        for s in shards {
            for (n, e) in &s.entries {
                entries.insert(n.clone(), (e.clone(), s.path.clone(), s.header_bytes));
            }
        }
        HeaderSource { entries }
    }
}

impl TensorSource for HeaderSource {
    fn shape(&self, name: &str) -> Option<Vec<usize>> {
        self.entries.get(name).map(|e| e.0.shape.clone())
    }
    fn load(&self, name: &str) -> Result<Tensor, LowerError> {
        Err(LowerError::weights(format!("`{name}`: a preflight reads headers, not tensor data")))
    }
    fn names(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }
    fn metadata(&self, name: &str) -> Option<TensorMeta> {
        self.entries.get(name).map(|e| TensorMeta { dtype: e.0.dtype.clone(), shape: e.0.shape.clone(), bytes: e.0.bytes })
    }
    fn read_slice(&self, name: &str, range: Range<u64>) -> Result<Vec<u8>, LowerError> {
        let (e, path, header) = self.entries.get(name).ok_or_else(|| LowerError::weights(format!("no tensor `{name}`")))?;
        if range.end > e.bytes || range.start > range.end {
            return Err(LowerError::weights(format!("`{name}`: bytes {range:?} of {}", e.bytes)));
        }
        let mut f = std::fs::File::open(path).map_err(|x| LowerError::Io(format!("{}: {x}", path.display())))?;
        let at = header + e.start + range.start;
        let len = f.metadata().map_err(|x| LowerError::Io(x.to_string()))?.len();
        if at + (range.end - range.start) > len {
            return Err(LowerError::weights(format!("`{name}`: its data is not in this file (a header-only download)")));
        }
        f.seek(SeekFrom::Start(at)).map_err(|x| LowerError::Io(x.to_string()))?;
        let mut out = vec![0u8; (range.end - range.start) as usize];
        f.read_exact(&mut out).map_err(|x| LowerError::Io(x.to_string()))?;
        Ok(out)
    }
}

/// Everything read of one model input.
pub struct Source {
    pub kind: InputKind,
    pub label: String,
    /// The files beside the checkpoint (the directory the input lives in).
    pub files: Vec<FileInfo>,
    /// The configuration the frontend reads (a GGUF's: synthesised from its metadata), when there is one.
    pub config: Option<serde_json::Value>,
    /// Why there is none (a GGUF whose architecture has no mapping, a `config.json` that is not JSON).
    pub config_error: Option<String>,
    pub config_sha256: Option<String>,
    pub shards: Vec<ShardHeader>,
    /// The index's tensor names, when the index was read and some shard is absent (the names those shards would hold).
    pub index_names: Option<BTreeSet<String>>,
    /// The checkpoint's total size as `model.safetensors.index.json` records it.
    pub index_total_size: Option<u64>,
    /// Shards the index names that are absent from the directory.
    pub missing_shards: Vec<String>,
    pub gguf_file: Option<GgufFile>,
    pub gguf_model: Option<GgufModel>,
    /// The GGUF's data region is not all in the file (a header alone, or a partial download).
    pub gguf_truncated: bool,
    pub gguf_file_bytes: u64,
    /// Bytes read from disk: the configuration, the indexes and every header.
    pub bytes_read: u64,
}

impl Source {
    /// The tensors the checkpoint holds, in one index: dtypes and shapes where a header was read, names alone where
    /// only an index was.
    pub fn tensor_index(&self) -> Option<TensorIndex> {
        if let Some(g) = &self.gguf_model {
            let mut t = BTreeMap::new();
            for n in g.names() {
                if let Some(m) = g.metadata(&n) {
                    t.insert(n, TensorEntry { dtype: m.dtype, shape: Some(m.shape) });
                }
            }
            return Some(TensorIndex { tensors: t });
        }
        if self.shards.is_empty() && self.index_names.is_none() {
            return None;
        }
        let mut t = BTreeMap::new();
        for s in &self.shards {
            for (n, e) in &s.entries {
                t.insert(n.clone(), TensorEntry { dtype: e.dtype.clone(), shape: Some(e.shape.clone()) });
            }
        }
        if let Some(names) = &self.index_names {
            for n in names {
                t.entry(n.clone()).or_insert(TensorEntry { dtype: String::new(), shape: None });
            }
        }
        Some(TensorIndex { tensors: t })
    }

    /// Whether every tensor of the checkpoint has a shape (every header was read).
    pub fn shapes_known(&self) -> bool {
        self.gguf_model.is_some() || (!self.shards.is_empty() && self.missing_shards.is_empty())
    }

    /// The bytes the checkpoint's weight files declare: a GGUF's tensors, the data regions of the shards read, else
    /// the index's total.
    pub fn declared_weight_bytes(&self) -> Option<u64> {
        if let Some(g) = &self.gguf_file {
            return Some(g.tensors.keys().filter_map(|n| g.stored_bytes(n)).sum());
        }
        if self.missing_shards.is_empty() && !self.shards.is_empty() {
            return Some(self.shards.iter().map(|s| s.declared_data_bytes).sum());
        }
        self.index_total_size
    }

    pub fn file_names(&self) -> Vec<String> {
        self.files.iter().map(|f| f.name.clone()).collect()
    }
}

fn list_files(dir: &Path) -> Vec<FileInfo> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if let Ok(m) = e.metadata()
                && m.is_file()
            {
                out.push(FileInfo { name, bytes: m.len() });
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Open a model input and read its headers. `headers` (for a lone `config.json`) names a directory whose
/// safetensors files, or header prefixes of them, stand in for the checkpoint.
pub fn open(path: &Path, kind: InputKind, headers: Option<&Path>, reg: &QuantRegistry) -> Result<Source, String> {
    match kind {
        InputKind::Gguf => open_gguf(path, reg),
        InputKind::HfDirectory => open_hf(&path.join("config.json"), path, headers.unwrap_or(path), kind),
        InputKind::ConfigFile => {
            let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
            open_hf(path, dir, headers.unwrap_or(dir), kind)
        }
        InputKind::Artifact => Err("an artifact is not a model source".into()),
        // A repository was materialised as a local snapshot of headers first ([`super::remote::materialize_headers`]).
        InputKind::Remote => open_hf(&path.join("config.json"), path, headers.unwrap_or(path), kind),
    }
}

fn open_hf(config_path: &Path, dir: &Path, weights_dir: &Path, kind: InputKind) -> Result<Source, String> {
    let text = std::fs::read_to_string(config_path).map_err(|e| format!("{}: {e}", config_path.display()))?;
    let mut bytes_read = text.len() as u64;
    let (config, config_error) =
        match serde_json::from_str::<serde_json::Value>(&misaka_palw_tir_lower::hf_config::sanitize_json(&text)) {
            Ok(v) => (Some(v), None),
            Err(e) => (None, Some(format!("config.json is not JSON: {e}"))),
        };
    let config_sha256 = Some(hex(&Sha256::digest(text.as_bytes())));
    let mut files = list_files(dir);
    if weights_dir != dir {
        for f in list_files(weights_dir) {
            if !files.iter().any(|x| x.name == f.name) {
                files.push(f);
            }
        }
        files.sort_by(|a, b| a.name.cmp(&b.name));
    }
    // The index names the shards; without one, a lone model.safetensors, else any safetensors file here (a header dump).
    let index_path = weights_dir.join("model.safetensors.index.json");
    let mut shard_names: Vec<String> = Vec::new();
    let mut index_names: Option<BTreeSet<String>> = None;
    let mut index_total_size = None;
    if index_path.exists() {
        let t = std::fs::read_to_string(&index_path).map_err(|e| format!("model.safetensors.index.json: {e}"))?;
        bytes_read += t.len() as u64;
        let v: serde_json::Value = serde_json::from_str(&t).map_err(|e| format!("model.safetensors.index.json: {e}"))?;
        let wm = v.get("weight_map").and_then(|w| w.as_object()).ok_or("model.safetensors.index.json has no weight_map")?;
        index_names = Some(wm.keys().cloned().collect());
        index_total_size = v.get("metadata").and_then(|m| m.get("total_size")).and_then(|s| s.as_u64());
        let shards: BTreeSet<String> = wm.values().filter_map(|x| x.as_str().map(str::to_string)).collect();
        shard_names = shards.into_iter().collect();
    } else if weights_dir.join("model.safetensors").exists() {
        shard_names.push("model.safetensors".into());
    } else {
        for f in &files {
            if f.name.ends_with(".safetensors") {
                shard_names.push(f.name.clone());
            }
        }
    }
    let mut shards = Vec::new();
    let mut missing = Vec::new();
    for s in &shard_names {
        let p = weights_dir.join(s);
        if !p.exists() {
            missing.push(s.clone());
            continue;
        }
        let h = read_safetensors_header(&p)?;
        bytes_read += h.header_bytes;
        shards.push(h);
    }
    // When every shard's header was read the index's names are redundant; when some are missing they stand for them.
    if missing.is_empty() && !shards.is_empty() {
        index_names = None;
    }
    let label =
        dir.canonicalize().ok().and_then(|d| d.file_name().map(|n| n.to_string_lossy().to_string())).unwrap_or_else(|| "model".into());
    Ok(Source {
        kind,
        label,
        files,
        config,
        config_error,
        config_sha256,
        shards,
        index_names,
        index_total_size,
        missing_shards: missing,
        gguf_file: None,
        gguf_model: None,
        gguf_truncated: false,
        gguf_file_bytes: 0,
        bytes_read,
    })
}

/// Counts the bytes the GGUF parser consumes, so "N bytes read" is the header's size and not a buffer's.
struct CountingReader<R> {
    inner: R,
    read: Arc<AtomicU64>,
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.read.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
}

fn parse_gguf(file: &Path, len: Option<u64>, reg: &QuantRegistry) -> Result<(GgufFile, u64), String> {
    let f = std::fs::File::open(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let read = Arc::new(AtomicU64::new(0));
    let reader = CountingReader { inner: std::io::BufReader::with_capacity(1 << 16, f), read: read.clone() };
    let parsed = GgufFile::parse(reader, len, file, reg).map_err(|e| e.to_string())?;
    Ok((parsed, read.load(Ordering::Relaxed)))
}

fn open_gguf(path: &Path, reg: &QuantRegistry) -> Result<Source, String> {
    let file = misaka_palw_tir_lower::fidelity::gguf_path(path).unwrap_or_else(|| path.to_path_buf());
    let dir = file.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let files = list_files(dir);
    let len = std::fs::metadata(&file).map_err(|e| format!("{}: {e}", file.display()))?.len();
    // The header alone: `parse` reads the magic, the metadata and the tensor table and stops. With the file's size it also
    // bounds each tensor against it; a file cut after its header fails that, and is read again without the bound.
    let (parsed, bytes_read, truncated) = match parse_gguf(&file, Some(len), reg) {
        Ok((p, n)) => (p, n, false),
        Err(e) if e.contains("runs past the end") => {
            let (p, n) = parse_gguf(&file, None, reg)?;
            (p, n, true)
        }
        Err(e) => return Err(e),
    };
    let label = file.file_name().and_then(|n| n.to_str()).unwrap_or("model.gguf").to_string();
    let (model, config, config_error) = match GgufModel::from_file(parsed.clone()) {
        Ok(m) => {
            let c = m.config.clone();
            (Some(m), Some(c), None)
        }
        Err(e) => (None, None, Some(e.to_string())),
    };
    Ok(Source {
        kind: InputKind::Gguf,
        label,
        files,
        config,
        config_error,
        config_sha256: None,
        shards: Vec::new(),
        index_names: None,
        index_total_size: None,
        missing_shards: Vec::new(),
        gguf_file: Some(parsed),
        gguf_model: model,
        gguf_truncated: truncated,
        gguf_file_bytes: len,
        bytes_read,
    })
}
