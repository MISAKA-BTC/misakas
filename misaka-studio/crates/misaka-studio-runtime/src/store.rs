//! The local model store: what is on this disk, and what is known about it.
//!
//! # Scanning is cheap, hashing is not
//!
//! A scan reads each model's header — kilobytes — and is safe to run whenever a directory might
//! have changed. A SHA-256 reads every byte, and on a 40 GB model that is a minute of disk at
//! full tilt. So they are separate operations: [`ModelStore::refresh`] happens freely,
//! [`ModelStore::ensure_hashed`] happens when someone asks for the provenance identity, and the
//! result is cached in a sidecar so it happens once per file.
//!
//! # The sidecar
//!
//! `<model>.gguf.misaka.json` holds what the file itself cannot: which repository and revision it
//! came from, and its digest. Both are needed for `h_M`, and neither is inside a GGUF. It sits
//! beside the model rather than in a central database so that moving a model directory to
//! another disk — the thing people do with 200 GB of models — moves its provenance with it.
//!
//! The sidecar records the **size at hash time**. A file whose size no longer matches has been
//! replaced, and the cached digest is discarded rather than trusted: a stale hash is a wrong
//! identity, which is worse than no identity.

use crate::{Error, Result};
use misaka_studio_core::model::{LocalModel, ModelSource};
use misaka_studio_core::quant::Quantization;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

/// How deep a model directory is walked. Deep enough for `models/org/repo/file.gguf`, shallow
/// enough that pointing the Studio at a home directory does not walk the whole disk.
const MAX_DEPTH: usize = 6;

/// Suffix of the metadata file written beside a model.
pub const SIDECAR_SUFFIX: &str = ".misaka.json";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Sidecar {
    pub source: ModelSource,
    pub sha256: Option<String>,
    /// File size when the digest was computed. The guard against a stale hash.
    pub hashed_size: Option<u64>,
}

impl Sidecar {
    pub fn path_for(model: &Path) -> PathBuf {
        let mut name = model.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        name.push_str(SIDECAR_SUFFIX);
        model.with_file_name(name)
    }

    pub fn load(model: &Path) -> Self {
        std::fs::read_to_string(Self::path_for(model)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self, model: &Path) -> Result<()> {
        let path = Self::path_for(model);
        let json = serde_json::to_string_pretty(self).map_err(|e| Error::io(path.display(), std::io::Error::other(e)))?;
        std::fs::write(&path, json).map_err(|e| Error::io(path.display(), e))
    }
}

/// Every model directory the Studio watches.
pub struct ModelStore {
    roots: RwLock<Vec<PathBuf>>,
    cache: RwLock<Arc<Vec<LocalModel>>>,
}

impl ModelStore {
    pub fn new(roots: Vec<PathBuf>) -> Self {
        ModelStore { roots: RwLock::new(roots), cache: RwLock::new(Arc::new(Vec::new())) }
    }

    pub async fn roots(&self) -> Vec<PathBuf> {
        self.roots.read().await.clone()
    }

    /// Replace the watched directories and rescan.
    pub async fn set_roots(&self, roots: Vec<PathBuf>) -> Result<Arc<Vec<LocalModel>>> {
        *self.roots.write().await = roots;
        self.refresh().await
    }

    /// Walk the directories and rebuild the list.
    pub async fn refresh(&self) -> Result<Arc<Vec<LocalModel>>> {
        let roots = self.roots.read().await.clone();
        // Blocking: a scan is filesystem work, and doing it on the async runtime stalls every
        // in-flight generation on a slow network drive.
        let models = tokio::task::spawn_blocking(move || scan_roots(&roots))
            .await
            .map_err(|e| Error::io("model scan", std::io::Error::other(e)))?;
        let models = Arc::new(models);
        *self.cache.write().await = models.clone();
        Ok(models)
    }

    /// The last scan's result. Cheap; call [`Self::refresh`] to make it current.
    pub async fn list(&self) -> Arc<Vec<LocalModel>> {
        self.cache.read().await.clone()
    }

    pub async fn get(&self, id: &str) -> Option<LocalModel> {
        self.cache.read().await.iter().find(|m| m.id == id).cloned()
    }

    pub async fn require(&self, id: &str) -> Result<LocalModel> {
        self.get(id).await.ok_or_else(|| Error::ModelNotFound { id: id.to_string() })
    }

    /// Delete a model and its sidecar.
    ///
    /// The path comes from the scan cache and is checked against the roots before anything is
    /// unlinked. Both halves matter: an id is user input, and "delete the model named
    /// `../../../etc/passwd`" must be impossible by construction rather than by the id happening
    /// not to contain a slash.
    pub async fn delete(&self, id: &str) -> Result<()> {
        let model = self.require(id).await?;
        let roots = self.roots.read().await.clone();
        let path = model.path.clone();
        let canonical = std::fs::canonicalize(&path).map_err(|e| Error::io(path.display(), e))?;
        let inside = roots.iter().any(|root| std::fs::canonicalize(root).map(|r| canonical.starts_with(r)).unwrap_or(false));
        if !inside {
            return Err(Error::bad_request(format!(
                "{} is outside every configured model directory; delete it with your file manager",
                path.display()
            )));
        }

        let sidecar = Sidecar::path_for(&canonical);
        tokio::task::spawn_blocking(move || -> Result<()> {
            if canonical.is_dir() {
                std::fs::remove_dir_all(&canonical).map_err(|e| Error::io(canonical.display(), e))?;
            } else {
                std::fs::remove_file(&canonical).map_err(|e| Error::io(canonical.display(), e))?;
            }
            let _ = std::fs::remove_file(&sidecar);
            Ok(())
        })
        .await
        .map_err(|e| Error::io("delete", std::io::Error::other(e)))??;

        self.refresh().await?;
        Ok(())
    }

    /// Compute the file's SHA-256 if it is not already known, and cache it.
    ///
    /// Returns the model with `sha256` filled in — which is what makes
    /// [`LocalModel::identity`](misaka_studio_core::model::LocalModel::identity) available, and
    /// with it the chain-compatible `h_M`.
    pub async fn ensure_hashed(&self, id: &str) -> Result<LocalModel> {
        let model = self.require(id).await?;
        let path = model.path.clone();
        let size = model.size_bytes;

        let sidecar = Sidecar::load(&path);
        if let (Some(sha), Some(hashed_size)) = (sidecar.sha256.clone(), sidecar.hashed_size)
            && hashed_size == size
        {
            return Ok(LocalModel { sha256: Some(sha), ..model });
        }

        let hash_path = path.clone();
        let digest = tokio::task::spawn_blocking(move || sha256_file(&hash_path))
            .await
            .map_err(|e| Error::io("hash", std::io::Error::other(e)))??;

        let mut sidecar = Sidecar::load(&path);
        sidecar.sha256 = Some(digest.clone());
        sidecar.hashed_size = Some(size);
        sidecar.save(&path)?;

        let updated = LocalModel { sha256: Some(digest), ..model };
        // Keep the cache in step so the next request does not re-read the sidecar.
        {
            let mut cache = self.cache.write().await;
            let mut models = (**cache).clone();
            if let Some(slot) = models.iter_mut().find(|m| m.id == updated.id) {
                *slot = updated.clone();
            }
            *cache = Arc::new(models);
        }
        Ok(updated)
    }

    /// Record where a model came from — called by the downloader when a file lands.
    pub async fn write_source(&self, path: &Path, source: ModelSource) -> Result<()> {
        let mut sidecar = Sidecar::load(path);
        sidecar.source = source;
        sidecar.save(path)
    }
}

/// SHA-256 of a file, read in 8 MiB blocks.
pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path).map_err(|e| Error::io(path.display(), e))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 8 << 20];
    loop {
        let read = file.read(&mut buffer).map_err(|e| Error::io(path.display(), e))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn scan_roots(roots: &[PathBuf]) -> Vec<LocalModel> {
    let mut found: HashMap<String, LocalModel> = HashMap::new();
    for root in roots {
        scan_dir(root, 0, &mut found);
    }
    let mut models: Vec<LocalModel> = found.into_values().collect();
    models.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    models
}

fn scan_dir(dir: &Path, depth: usize, found: &mut HashMap<String, LocalModel>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Ok(file_type) = entry.file_type() else { continue };

        if file_type.is_dir() {
            // An MLX model is a directory, so a directory is checked as a model before being
            // descended into.
            if let Some(model) = inspect_mlx_dir(&path) {
                found.entry(model.id.clone()).or_insert(model);
                continue;
            }
            scan_dir(&path, depth + 1, found);
            continue;
        }

        if !name.to_ascii_lowercase().ends_with(".gguf") {
            continue;
        }
        // A multi-part GGUF is one model: llama.cpp is pointed at part 1 and finds the rest, so
        // listing every shard would offer the user four broken entries and one that works.
        if is_non_first_shard(&name) {
            continue;
        }
        let sidecar = Sidecar::load(&path);
        match LocalModel::inspect(&path, sidecar.source.clone()) {
            Ok(mut model) => {
                if let (Some(sha), Some(size)) = (sidecar.sha256, sidecar.hashed_size)
                    && size == model.size_bytes
                {
                    model.sha256 = Some(sha);
                }
                found.entry(model.id.clone()).or_insert(model);
            }
            // A file that is not a readable GGUF is skipped with a log line, not an error: one
            // truncated download must not make the model list fail to load.
            Err(e) => tracing::warn!(path = %path.display(), "skipping: {e}"),
        }
    }
}

/// `model-00002-of-00003.gguf` is a continuation shard; part 1 represents the model.
fn is_non_first_shard(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let Some(idx) = lower.find("-of-") else { return false };
    let before = &lower[..idx];
    let Some(dash) = before.rfind('-') else { return false };
    let part = &before[dash + 1..];
    part.len() >= 3 && part.chars().all(|c| c.is_ascii_digit()) && part.parse::<u32>().map(|n| n != 1).unwrap_or(false)
}

/// An MLX model directory: `config.json` plus safetensors.
fn inspect_mlx_dir(path: &Path) -> Option<LocalModel> {
    let config_path = path.join("config.json");
    if !config_path.is_file() {
        return None;
    }
    let has_weights = std::fs::read_dir(path).ok()?.flatten().any(|e| e.file_name().to_string_lossy().ends_with(".safetensors"));
    if !has_weights {
        return None;
    }

    let config: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&config_path).ok()?).ok()?;
    let id = path.file_name()?.to_string_lossy().into_owned();
    let size_bytes = dir_size(path);
    let bits = config.get("quantization").and_then(|q| q.get("bits")).and_then(|b| b.as_u64());

    Some(LocalModel {
        name: id.clone(),
        id,
        path: path.to_path_buf(),
        size_bytes,
        quantization: bits.map(|b| Quantization::unknown(format!("MLX-{b}bit"))).or_else(|| Some(Quantization::unknown("MLX-fp16"))),
        architecture: config
            .get("model_type")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| config.get("architectures").and_then(|a| a.get(0)).and_then(|v| v.as_str()).map(str::to_string)),
        parameter_count: None,
        context_length: config.get("max_position_embeddings").and_then(|v| v.as_u64()),
        block_count: config.get("num_hidden_layers").and_then(|v| v.as_u64()),
        expert_count: config.get("num_experts").and_then(|v| v.as_u64()),
        kv_cache_bytes_per_token: None,
        has_chat_template: path.join("tokenizer_config.json").is_file(),
        source: Sidecar::load(path).source,
        sha256: None,
        modified_at: None,
    })
}

fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else { return 0 };
    entries
        .flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_size(&e.path()),
            _ => e.metadata().map(|m| m.len()).unwrap_or(0),
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal but real GGUF so the store is tested on files, not mocks.
    fn write_gguf(path: &Path, arch: &str, ftype: u32) {
        let mut out = Vec::new();
        out.extend_from_slice(b"GGUF");
        out.extend_from_slice(&3u32.to_le_bytes());
        out.extend_from_slice(&0u64.to_le_bytes()); // tensors
        out.extend_from_slice(&2u64.to_le_bytes()); // kv pairs
        let kv = |key: &str, value: &[u8], ty: u32, out: &mut Vec<u8>| {
            out.extend_from_slice(&(key.len() as u64).to_le_bytes());
            out.extend_from_slice(key.as_bytes());
            out.extend_from_slice(&ty.to_le_bytes());
            out.extend_from_slice(value);
        };
        let mut arch_value = Vec::new();
        arch_value.extend_from_slice(&(arch.len() as u64).to_le_bytes());
        arch_value.extend_from_slice(arch.as_bytes());
        kv("general.architecture", &arch_value, 8, &mut out);
        kv("general.file_type", &ftype.to_le_bytes(), 4, &mut out);
        std::fs::write(path, out).expect("write gguf");
    }

    #[tokio::test]
    async fn scanning_finds_models_and_reads_their_quantization() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_gguf(&dir.path().join("Qwen3-4B-Q4_K_M.gguf"), "qwen3", 15);
        std::fs::create_dir(dir.path().join("nested")).expect("mkdir");
        write_gguf(&dir.path().join("nested/Llama-3-8B-Q8_0.gguf"), "llama", 7);
        std::fs::write(dir.path().join("notes.txt"), "not a model").expect("write");

        let store = ModelStore::new(vec![dir.path().to_path_buf()]);
        let models = store.refresh().await.expect("scans");
        assert_eq!(models.len(), 2, "found {:?}", models.iter().map(|m| &m.id).collect::<Vec<_>>());
        let qwen = store.get("Qwen3-4B-Q4_K_M").await.expect("qwen is listed");
        assert_eq!(qwen.quantization.expect("quant").label, "Q4_K_M");
        assert_eq!(qwen.architecture.as_deref(), Some("qwen3"));
    }

    /// A four-shard model must appear once, as its first part — not four times.
    #[test]
    fn continuation_shards_are_not_separate_models() {
        assert!(is_non_first_shard("model-00002-of-00004.gguf"));
        assert!(is_non_first_shard("Deepseek-Q4_K_M-00003-of-00009.gguf"));
        assert!(!is_non_first_shard("model-00001-of-00004.gguf"));
        assert!(!is_non_first_shard("Qwen3-4B-Q4_K_M.gguf"));
    }

    #[tokio::test]
    async fn hashing_is_cached_in_the_sidecar_and_invalidated_by_a_size_change() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("m-Q4_K_M.gguf");
        write_gguf(&path, "llama", 15);
        let store = ModelStore::new(vec![dir.path().to_path_buf()]);
        store.refresh().await.expect("scans");

        let hashed = store.ensure_hashed("m-Q4_K_M").await.expect("hashes");
        let digest = hashed.sha256.clone().expect("a digest");
        assert!(Sidecar::path_for(&path).is_file(), "the sidecar is written");

        // The identity is now available, and it re-derives.
        let identity = hashed.identity().expect("identity");
        assert!(identity.verify());

        // Replace the file with different content and a different length: the cached digest must
        // not survive.
        let mut bytes = std::fs::read(&path).expect("read");
        bytes.extend_from_slice(b"different now");
        std::fs::write(&path, bytes).expect("write");
        store.refresh().await.expect("rescans");
        let rehashed = store.ensure_hashed("m-Q4_K_M").await.expect("rehashes");
        assert_ne!(rehashed.sha256.expect("digest"), digest, "a changed file gets a new digest");
    }

    #[tokio::test]
    async fn deleting_removes_the_model_and_its_sidecar() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("gone-Q4_K_M.gguf");
        write_gguf(&path, "llama", 15);
        let store = ModelStore::new(vec![dir.path().to_path_buf()]);
        store.refresh().await.expect("scans");
        store.ensure_hashed("gone-Q4_K_M").await.expect("hashes");

        store.delete("gone-Q4_K_M").await.expect("deletes");
        assert!(!path.exists());
        assert!(!Sidecar::path_for(&path).exists());
        assert!(store.get("gone-Q4_K_M").await.is_none());
    }

    #[tokio::test]
    async fn deleting_an_unknown_model_is_a_not_found_not_a_panic() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ModelStore::new(vec![dir.path().to_path_buf()]);
        store.refresh().await.expect("scans");
        assert!(matches!(store.delete("nothing").await, Err(Error::ModelNotFound { .. })));
    }

    #[tokio::test]
    async fn an_mlx_directory_is_one_model() {
        let dir = tempfile::tempdir().expect("tempdir");
        let model_dir = dir.path().join("Qwen3-4B-mlx");
        std::fs::create_dir(&model_dir).expect("mkdir");
        std::fs::write(
            model_dir.join("config.json"),
            r#"{"model_type":"qwen3","num_hidden_layers":36,"max_position_embeddings":32768,"quantization":{"bits":4}}"#,
        )
        .expect("write config");
        std::fs::write(model_dir.join("model.safetensors"), vec![0u8; 128]).expect("write weights");

        let store = ModelStore::new(vec![dir.path().to_path_buf()]);
        let models = store.refresh().await.expect("scans");
        assert_eq!(models.len(), 1);
        let model = &models[0];
        assert_eq!(model.id, "Qwen3-4B-mlx");
        assert_eq!(model.block_count, Some(36));
        assert_eq!(model.quantization.as_ref().expect("quant").label, "MLX-4bit");
        assert!(model.size_bytes >= 128);
    }
}
