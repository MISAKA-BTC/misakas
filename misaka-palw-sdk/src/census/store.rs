//! **The header store of one repository** (written by `tools/hf_census/fetch.py`) and the preflight source built from it.
//!
//! A fetched repository is a directory: `listing.json` (its [`super::listing::ListingV1`]), `fetch.json` ([`FetchV1`]: the inventory at
//! the pinned revision, with every file's size and content id, and one item per read), `f/<path>` (the metadata files, as fetched) and
//! `h/<path>.hdr.gz` (a safetensors header — its 8-byte length and JSON — or a GGUF's metadata and tensor infos, gzip). A GGUF header
//! is stored at its length with the contents of its large arrays (the tokenizer's tokens, merges, scores) zeroed: nothing the preflight
//! reads is in them (it asks only whether `tokenizer.ggml.tokens` exists), and the SHA-256 of the bytes as fetched is in the item.
//!
//! The source is the local preflight's: the configuration and the index as files, each shard as its header alone, **the sizes the
//! Hub's inventory states** (so a file shorter than its header declares is an incomplete upload, and a read of tensor data finds
//! "its data is not in this file" and is reported as not checkable, never as wrong).

use super::codes;
use super::listing::{ArtifactKind, ListingV1, SelectedV1};
use crate::preflight::remote::Scratch;
use crate::preflight::source::{self, FileInfo, InputKind, Source};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

pub const FETCH_SCHEMA_V1: &str = "misaka.palw.hf-census-fetch.v1";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct InfoV1 {
    /// `ok`, or `error`.
    pub status: String,
    /// The fetch error's stable kind (`http_404`, `gated`, `timeout`, …).
    pub error: Option<String>,
    pub detail: Option<String>,
    pub sha: Option<String>,
    pub gated: serde_json::Value,
}

/// One file of the inventory at the pinned revision.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FileV1 {
    pub path: String,
    pub size: u64,
    /// The LFS object id (SHA-256 of the content), for a file stored in LFS.
    pub lfs_sha256: Option<String>,
    /// The git blob id.
    pub blob_id: Option<String>,
}

/// One read the fetch made.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ItemV1 {
    pub path: String,
    /// `file`, `st_header` or `gguf_header`.
    pub kind: String,
    /// `ok`, or `error`.
    pub status: String,
    /// `http_404`, `gated`, `too_large`, `header_too_large`, `header_invalid`, `timeout`, …
    pub error: Option<String>,
    pub detail: Option<String>,
    /// The stored copy, relative to the repository's directory.
    pub store: Option<String>,
    /// Bytes fetched.
    pub bytes: u64,
    /// SHA-256 of the bytes as fetched.
    pub sha256: Option<String>,
    /// The file's size on the Hub.
    pub file_size: Option<u64>,
    /// The header's length (8 + the JSON's for safetensors; the metadata and tensor infos for a GGUF).
    pub header_len: Option<u64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FetchV1 {
    pub schema: String,
    pub repo: String,
    pub revision: String,
    pub info: InfoV1,
    pub inventory: Vec<FileV1>,
    pub items: Vec<ItemV1>,
    /// An adapter's base, read at its pinned commit: its inventory and the items read of it are the ones above whose path starts
    /// with `base/`.
    pub base: Option<BaseFetchV1>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BaseFetchV1 {
    pub repo: String,
    pub revision: String,
    pub info: InfoV1,
    pub inventory: Vec<FileV1>,
}

impl FetchV1 {
    pub fn item(&self, path: &str) -> Option<&ItemV1> {
        self.items.iter().find(|i| i.path == path)
    }
    pub fn size_of(&self, path: &str) -> Option<u64> {
        self.inventory.iter().find(|f| f.path == path).map(|f| f.size)
    }
}

/// A fetched repository, read back.
pub struct Fetched {
    pub dir: PathBuf,
    pub fetch: FetchV1,
}

impl Fetched {
    pub fn load(dir: &Path) -> Result<Fetched, String> {
        let t = std::fs::read_to_string(dir.join("fetch.json")).map_err(|e| format!("{}/fetch.json: {e}", dir.display()))?;
        let fetch: FetchV1 = serde_json::from_str(&t).map_err(|e| format!("{}/fetch.json: {e}", dir.display()))?;
        if fetch.schema != FETCH_SCHEMA_V1 {
            return Err(format!("{}/fetch.json: schema {} (this build reads {FETCH_SCHEMA_V1})", dir.display(), fetch.schema));
        }
        Ok(Fetched { dir: dir.to_path_buf(), fetch })
    }

    /// The bytes of a stored item (gunzipped when the store is `.gz`).
    pub fn bytes_of(&self, it: &ItemV1) -> Result<Vec<u8>, String> {
        let rel = it.store.as_deref().ok_or_else(|| format!("{}: not stored", it.path))?;
        let p = self.dir.join(rel);
        let raw = std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        if rel.ends_with(".gz") {
            let mut out = Vec::new();
            flate2::read::GzDecoder::new(&raw[..]).read_to_end(&mut out).map_err(|e| format!("{}: {e}", p.display()))?;
            Ok(out)
        } else {
            Ok(raw)
        }
    }

    /// The content identity of the selected weights: BLAKE2b-256 over the sorted LFS ids (or blob ids) of the files, so repositories
    /// that copy the same weights count once among "unique weight sets".
    pub fn weights_identity(&self, files: &[String]) -> Option<String> {
        let mut ids: Vec<String> = Vec::new();
        for f in files {
            let inv = self.fetch.inventory.iter().find(|x| &x.path == f)?;
            ids.push(inv.lfs_sha256.clone().or_else(|| inv.blob_id.clone())?);
        }
        if ids.is_empty() {
            return None;
        }
        ids.sort();
        let mut st = blake2b_simd::Params::new().hash_length(32).key(b"misaka-palw/hf-census-weights/v1").to_state();
        for i in ids {
            st.update(i.as_bytes());
            st.update(b"\n");
        }
        Some(source::hex(st.finalize().as_bytes()))
    }
}

/// A problem the store shows, as a source-gate code (`FETCH_FAILED`, `WEIGHTS_INCOMPLETE`, …) on a path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Problem {
    pub code: &'static str,
    pub path: String,
    pub detail: String,
}

fn problem(code: &'static str, path: &str, detail: impl Into<String>) -> Problem {
    Problem { code, path: path.to_string(), detail: detail.into() }
}

/// The source-gate code of a failed item.
pub fn item_problem(it: &ItemV1) -> Problem {
    let e = it.error.as_deref().unwrap_or("error");
    let code = match e {
        "header_too_large" | "too_large" => codes::HEADER_TOO_LARGE,
        "header_invalid" => codes::HEADER_INVALID,
        "gated" | "http_401" | "http_403" => codes::GATED_ACCESS,
        _ => codes::FETCH_FAILED,
    };
    problem(code, &it.path, format!("{e}: {}", it.detail.clone().unwrap_or_default()))
}

/// The files of the inventory directly in `dir`, by name, with the Hub's sizes.
fn files_in(f: &FetchV1, dir: &str) -> Vec<FileInfo> {
    let mut v: Vec<FileInfo> = f
        .inventory
        .iter()
        .filter_map(|x| {
            let rel = if dir.is_empty() { x.path.as_str() } else { x.path.strip_prefix(dir)?.strip_prefix('/')? };
            (!rel.contains('/') && !rel.starts_with('.')).then(|| FileInfo { name: rel.to_string(), bytes: x.size })
        })
        .collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() { name.to_string() } else { format!("{dir}/{name}") }
}

/// What [`source_of`] built: the source, the scratch directory it reads (removed when dropped), and the store's problems.
pub struct CensusSource {
    pub source: Source,
    pub scratch: Scratch,
    pub label: String,
    pub bytes_read: u64,
    /// The frontend needs the data of a tensor the store does not hold (a GGUF whose configuration is rebuilt from a small tensor,
    /// `rope_freqs.weight`): its name and size.
    pub needs_tensor_data: Option<String>,
}

/// **The preflight source of a fetched repository**, or the source-gate problems that stop it (a needed file not fetched, a shard the
/// index names that is not in the inventory, a file shorter than its header declares).
pub fn source_of(l: &ListingV1, sel: &SelectedV1, fx: &Fetched) -> Result<CensusSource, Vec<Problem>> {
    let f = &fx.fetch;
    let label = format!("hf://{}@{}", l.id, f.revision);
    let scratch = Scratch::new().map_err(|e| vec![problem(codes::FETCH_FAILED, "(scratch)", e)])?;
    let reg = misaka_palw_tir_lower::quantfmt::QuantRegistry::builtin();
    let mut problems = Vec::new();
    let mut bytes_read = 0u64;
    let write = |name: &str, b: &[u8]| -> Result<(), Problem> {
        std::fs::write(scratch.0.join(name), b).map_err(|e| problem(codes::FETCH_FAILED, name, format!("scratch: {e}")))
    };
    match sel.kind {
        ArtifactKind::Safetensors | ArtifactKind::SafetensorsOther | ArtifactKind::DiffusersComponent => {
            let cfg =
                sel.config.clone().ok_or_else(|| vec![problem(codes::FETCH_FAILED, "config.json", "no configuration selected")])?;
            match f.item(&cfg) {
                Some(it) if it.status == "ok" => match fx.bytes_of(it) {
                    Ok(b) => {
                        bytes_read += b.len() as u64;
                        if let Err(p) = write("config.json", &b) {
                            problems.push(p);
                        }
                    }
                    Err(e) => problems.push(problem(codes::FETCH_FAILED, &cfg, e)),
                },
                Some(it) => problems.push(item_problem(it)),
                None => problems.push(problem(codes::FETCH_FAILED, &cfg, "not fetched")),
            }
            let mut shards: BTreeSet<String> = BTreeSet::new();
            if let Some(ix) = &sel.index {
                match f.item(ix) {
                    Some(it) if it.status == "ok" => match fx.bytes_of(it) {
                        Ok(b) => {
                            bytes_read += b.len() as u64;
                            match serde_json::from_slice::<serde_json::Value>(&b) {
                                Ok(v) => match v.get("weight_map").and_then(|w| w.as_object()) {
                                    Some(wm) => {
                                        shards.extend(wm.values().filter_map(|x| x.as_str().map(str::to_string)));
                                        if let Err(p) = write("model.safetensors.index.json", &b) {
                                            problems.push(p);
                                        }
                                    }
                                    None => problems.push(problem(codes::HEADER_INVALID, ix, "the index has no weight_map")),
                                },
                                Err(e) => problems.push(problem(codes::HEADER_INVALID, ix, format!("not JSON: {e}"))),
                            }
                        }
                        Err(e) => problems.push(problem(codes::FETCH_FAILED, ix, e)),
                    },
                    Some(it) => problems.push(item_problem(it)),
                    None => problems.push(problem(codes::FETCH_FAILED, ix, "not fetched")),
                }
                for s in &shards {
                    if s.contains('/') || s.contains("..") {
                        problems.push(problem(codes::HEADER_INVALID, ix, format!("the index names `{s}`, not a file beside it")));
                    } else if f.size_of(&join(&sel.dir, s)).is_none() {
                        problems.push(problem(
                            codes::WEIGHTS_INCOMPLETE,
                            &join(&sel.dir, s),
                            "named by the index and not in the repository",
                        ));
                    }
                }
            } else {
                shards.extend(sel.weights.iter().map(|w| w.rsplit('/').next().unwrap_or(w).to_string()));
            }
            for s in &shards {
                let path = join(&sel.dir, s);
                if f.size_of(&path).is_none() {
                    continue;
                }
                match f.item(&path) {
                    Some(it) if it.status == "ok" => match fx.bytes_of(it) {
                        Ok(b) => {
                            bytes_read += b.len() as u64;
                            if let Err(p) = write(s, &b) {
                                problems.push(p);
                            }
                        }
                        Err(e) => problems.push(problem(codes::FETCH_FAILED, &path, e)),
                    },
                    Some(it) => problems.push(item_problem(it)),
                    None => problems.push(problem(codes::FETCH_FAILED, &path, "header not fetched")),
                }
            }
            if !problems.is_empty() {
                return Err(problems);
            }
            let mut src =
                source::open(&scratch.0, InputKind::Remote, None, reg).map_err(|e| vec![problem(codes::HEADER_INVALID, &label, e)])?;
            // The Hub's sizes; a shard shorter than its header declares is an incomplete upload.
            src.files = files_in(f, &sel.dir);
            for sh in src.shards.iter_mut() {
                let size = f.size_of(&join(&sel.dir, &sh.file)).unwrap_or(0);
                sh.file_bytes = size;
                if !sh.complete() {
                    problems.push(problem(
                        codes::WEIGHTS_INCOMPLETE,
                        &join(&sel.dir, &sh.file),
                        format!("{size} bytes on the Hub, the header declares {}", sh.header_bytes + sh.declared_data_bytes),
                    ));
                }
            }
            if !problems.is_empty() {
                return Err(problems);
            }
            // The tokenizer check of a directory reads the file names; the census's are the inventory's.
            src.kind = InputKind::HfDirectory;
            src.label = label.clone();
            Ok(CensusSource { source: src, scratch, label, bytes_read, needs_tensor_data: None })
        }
        ArtifactKind::Gguf => {
            if sel.weights.len() != 1 {
                return Err(vec![problem(codes::FETCH_FAILED, "(gguf)", "a split GGUF set is not read by this build")]);
            }
            let path = &sel.weights[0];
            let it = match f.item(path) {
                Some(it) if it.status == "ok" => it,
                Some(it) => return Err(vec![item_problem(it)]),
                None => return Err(vec![problem(codes::FETCH_FAILED, path, "header not fetched")]),
            };
            let b = fx.bytes_of(it).map_err(|e| vec![problem(codes::FETCH_FAILED, path, e)])?;
            bytes_read += it.header_len.unwrap_or(b.len() as u64);
            let size = f.size_of(path).ok_or_else(|| vec![problem(codes::FETCH_FAILED, path, "not in the inventory")])?;
            let parsed = misaka_palw_tir_lower::gguf::GgufFile::parse(&b[..], Some(size), Path::new(path), reg).map_err(|e| {
                let msg = e.to_string();
                if msg.contains("runs past the end") {
                    vec![problem(codes::WEIGHTS_INCOMPLETE, path, msg)]
                } else {
                    vec![problem(codes::HEADER_INVALID, path, msg)]
                }
            })?;
            let mut needs_tensor_data = None;
            let (model, config, config_error) = match misaka_palw_tir_lower::gguf::GgufModel::from_file(parsed.clone()) {
                Ok(m) => {
                    let c = m.config.clone();
                    (Some(m), Some(c), None)
                }
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("header-only view holds no tensor data") {
                        // The mapping reads a small tensor's data (a Llama-3 GGUF's rope frequency factors).
                        let t = ["rope_freqs.weight"]
                            .iter()
                            .find_map(|n| parsed.tensors.get(*n).map(|t| format!("{n} ({} bytes)", t.bytes)))
                            .unwrap_or_else(|| "a tensor of the file".to_string());
                        needs_tensor_data = Some(t);
                    }
                    (None, None, Some(msg))
                }
            };
            let dir = path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
            let src = Source {
                kind: InputKind::Gguf,
                label: label.clone(),
                files: files_in(f, dir),
                config,
                config_error,
                config_sha256: None,
                shards: Vec::new(),
                index_names: None,
                index_total_size: None,
                missing_shards: Vec::new(),
                gguf_file: Some(parsed),
                gguf_model: model,
                gguf_truncated: false,
                gguf_file_bytes: size,
                bytes_read,
            };
            Ok(CensusSource { source: src, scratch, label, bytes_read, needs_tensor_data })
        }
        ArtifactKind::Diffusers | ArtifactKind::Adapter | ArtifactKind::Other | ArtifactKind::None => {
            Err(vec![problem(codes::FETCH_FAILED, "(selection)", format!("{:?} has no preflight source", sel.kind))])
        }
    }
}

/// The configurations of a diffusers pipeline's components, by component name: `(library, class, config)` from `model_index.json` and
/// `<component>/config.json`.
pub fn diffusers_components(fx: &Fetched) -> Result<BTreeMap<String, (String, String, Option<serde_json::Value>)>, Problem> {
    let f = &fx.fetch;
    let it = f.item("model_index.json").ok_or_else(|| problem(codes::FETCH_FAILED, "model_index.json", "not fetched"))?;
    if it.status != "ok" {
        return Err(item_problem(it));
    }
    let b = fx.bytes_of(it).map_err(|e| problem(codes::FETCH_FAILED, "model_index.json", e))?;
    let v: serde_json::Value =
        serde_json::from_slice(&b).map_err(|e| problem(codes::HEADER_INVALID, "model_index.json", format!("not JSON: {e}")))?;
    let mut out = BTreeMap::new();
    for (k, x) in v.as_object().into_iter().flatten() {
        if k.starts_with('_') {
            continue;
        }
        let Some(arr) = x.as_array() else { continue };
        if arr.len() != 2 {
            continue;
        }
        let lib = arr[0].as_str().unwrap_or("null").to_string();
        let class = arr[1].as_str().unwrap_or("null").to_string();
        if lib == "null" && class == "null" {
            continue;
        }
        let cfg = ["config.json", "scheduler_config.json"].iter().find_map(|n| {
            let p = format!("{k}/{n}");
            let it = f.item(&p).filter(|i| i.status == "ok")?;
            let b = fx.bytes_of(it).ok()?;
            serde_json::from_slice::<serde_json::Value>(&b).ok()
        });
        out.insert(k.clone(), (lib, class, cfg));
    }
    Ok(out)
}

/// **The source of a PEFT adapter over its pinned base** (RFC-0004: a candidate is a parent plus an adapter): the base's checkpoint as
/// [`source_of`] reads it, plus the adapter's own header as one more shard (its `lora_A`/`lora_B` tensors), the files of both
/// repositories (an adapter may carry the tokenizer), and the adapter's configuration and tensor names for the preflight to attach.
/// `Ok(None)` when no base was read.
pub fn adapter_source_of(
    l: &ListingV1,
    sel: &SelectedV1,
    fx: &Fetched,
) -> Result<Option<(CensusSource, crate::preflight::LoraInput)>, Vec<Problem>> {
    let Some(base) = &fx.fetch.base else { return Ok(None) };
    if base.info.status != "ok" {
        let e = base.info.error.clone().unwrap_or_else(|| "error".into());
        let code = if e == "gated" { codes::BASE_UNPINNED } else { codes::FETCH_FAILED };
        return Err(vec![problem(
            code,
            &format!("base:{}", base.repo),
            format!("{e}: {}", base.info.detail.clone().unwrap_or_default()),
        )]);
    }
    let f = &fx.fetch;
    // The adapter's configuration and tensor names.
    let cfg_path = sel.config.clone().unwrap_or_else(|| "adapter_config.json".into());
    let config = match f.item(&cfg_path) {
        Some(it) if it.status == "ok" => {
            String::from_utf8(fx.bytes_of(it).map_err(|e| vec![problem(codes::FETCH_FAILED, &cfg_path, e)])?)
                .map_err(|e| vec![problem(codes::HEADER_INVALID, &cfg_path, e.to_string())])?
        }
        Some(it) => return Err(vec![item_problem(it)]),
        None => return Err(vec![problem(codes::FETCH_FAILED, &cfg_path, "not fetched")]),
    };
    let ad_path = sel
        .weights
        .iter()
        .find(|w| w.ends_with(".safetensors"))
        .cloned()
        .ok_or_else(|| vec![problem(codes::FETCH_FAILED, "adapter_model.safetensors", "no safetensors adapter file selected")])?;
    let ad_item = match f.item(&ad_path) {
        Some(it) if it.status == "ok" => it,
        Some(it) => return Err(vec![item_problem(it)]),
        None => return Err(vec![problem(codes::FETCH_FAILED, &ad_path, "header not fetched")]),
    };
    let ad_bytes = fx.bytes_of(ad_item).map_err(|e| vec![problem(codes::FETCH_FAILED, &ad_path, e)])?;
    // The base, as a fetch of its own: the items under `base/`, its inventory.
    let base_fetch = FetchV1 {
        schema: f.schema.clone(),
        repo: base.repo.clone(),
        revision: base.revision.clone(),
        info: base.info.clone(),
        inventory: base.inventory.clone(),
        items: f
            .items
            .iter()
            .filter_map(|it| it.path.strip_prefix("base/").map(|p| ItemV1 { path: p.to_string(), ..it.clone() }))
            .collect(),
        base: None,
    };
    let base_listing =
        ListingV1 { id: base.repo.clone(), siblings: base.inventory.iter().map(|x| x.path.clone()).collect(), ..Default::default() };
    let base_sel = super::listing::select(&base_listing);
    if !matches!(base_sel.kind, ArtifactKind::Safetensors) {
        return Err(vec![problem(
            codes::BASE_UNPINNED,
            &format!("base:{}", base.repo),
            format!("the base holds no transformers safetensors checkpoint ({:?})", base_sel.kind),
        )]);
    }
    let base_fx = Fetched { dir: fx.dir.clone(), fetch: base_fetch };
    let mut cs = source_of(&base_listing, &base_sel, &base_fx)?;
    // The adapter's header as one more shard.
    let name = "adapter_model.safetensors";
    std::fs::write(cs.scratch.0.join(name), &ad_bytes).map_err(|e| vec![problem(codes::FETCH_FAILED, name, e.to_string())])?;
    let mut sh =
        source::read_safetensors_header(&cs.scratch.0.join(name)).map_err(|e| vec![problem(codes::HEADER_INVALID, &ad_path, e)])?;
    sh.file_bytes = f.size_of(&ad_path).unwrap_or(0);
    if !sh.complete() {
        return Err(vec![problem(codes::WEIGHTS_INCOMPLETE, &ad_path, "shorter than its header declares")]);
    }
    let tensors: Vec<String> = sh.entries.keys().cloned().collect();
    cs.bytes_read += sh.header_bytes + config.len() as u64;
    cs.source.shards.push(sh);
    // The files of both repositories: the adapter's own (its tokenizer, when it carries one) over the base's.
    let mut files = files_in(f, "");
    for b in files_in(&base_fx.fetch, &base_sel.dir) {
        if !files.iter().any(|x| x.name == b.name) {
            files.push(b);
        }
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    cs.source.files = files;
    cs.label = format!("hf://{}@{} over hf://{}@{}", l.id, f.revision, base.repo, base.revision);
    cs.source.label = cs.label.clone();
    Ok(Some((cs, crate::preflight::LoraInput { config, tensors })))
}
