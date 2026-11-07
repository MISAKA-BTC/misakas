//! Opt-in, preconversion pins for NEW full-court25 artifacts. Old conversion bytes
//! remain unchanged. Source-byte integrity and full component inventory are not
//! source fidelity or publisher authentication; those remain independent gates.
use crate::{
    error::{LowerError, Result},
    hf_schema::TensorIndex,
    model::FeatureScope,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{collections::BTreeSet, io::Read, path::Path};
pub const SOURCE_PIN_SCHEMA_V25: &str = "misaka.palw.full-court-source-pin.v25";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceFilePinV25 {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourcePinV25 {
    pub schema: String,
    pub repository: String,
    pub revision: String,
    pub original_context_max: u32,
    pub advertised_task: String,
    pub required_components: Vec<String>,
    /// Every source tensor, sorted once before conversion; no excluded component.
    pub complete_tensor_inventory: Vec<String>,
    pub source_files: Vec<SourceFilePinV25>,
}
impl SourcePinV25 {
    pub fn from_file(path: &Path) -> Result<Self> {
        let f = std::fs::File::open(path).map_err(|e| LowerError::bad(format!("source25 pin: {e}")))?;
        if f.metadata().map_err(|e| LowerError::bad(e.to_string()))?.len() > 4 << 20 {
            return Err(LowerError::bad("source25 manifest cap"));
        }
        let mut bytes = Vec::new();
        f.take((4 << 20) + 1).read_to_end(&mut bytes).map_err(|e| LowerError::bad(e.to_string()))?;
        if bytes.len() > 4 << 20 {
            return Err(LowerError::bad("source25 manifest cap"));
        }
        serde_json::from_slice(&bytes).map_err(|e| LowerError::bad(format!("source25 pin grammar: {e}")))
    }
    pub fn verify_files(&self, directory: &Path) -> Result<()> {
        if self.schema != SOURCE_PIN_SCHEMA_V25
            || self.repository.is_empty()
            || self.revision.len() != 40
            || !self.revision.bytes().all(|b| b.is_ascii_hexdigit())
            || self.original_context_max == 0
            || self.source_files.is_empty()
            || self.source_files.len() > 1024
        {
            return Err(LowerError::bad("source25 original revision/context/file pins required"));
        }
        let root = directory.canonicalize().map_err(|e| LowerError::bad(format!("source25 directory: {e}")))?;
        let mut names = BTreeSet::new();
        let mut total = 0u64;
        for pin in &self.source_files {
            total = total.checked_add(pin.bytes).ok_or_else(|| LowerError::bad("source25 total extent overflow"))?;
            if total > 128 << 30 {
                return Err(LowerError::bad("source25 total original file extent cap"));
            }
            let relative = Path::new(&pin.path);
            if relative.is_absolute()
                || relative.components().any(|c| !matches!(c, std::path::Component::Normal(_)))
                || !names.insert(pin.path.clone())
                || pin.sha256.len() != 64
                || !pin.sha256.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err(LowerError::bad("source25 unique relative file/SHA pins required"));
            }
            let path = root.join(relative).canonicalize().map_err(|e| LowerError::bad(format!("source25 pinned file: {e}")))?;
            if !path.starts_with(&root) {
                return Err(LowerError::bad("source25 file escapes source directory"));
            }
            let mut f = std::fs::File::open(path).map_err(|e| LowerError::bad(format!("source25 file: {e}")))?;
            if f.metadata().map_err(|e| LowerError::bad(e.to_string()))?.len() != pin.bytes {
                return Err(LowerError::bad("source25 exact payload length differs"));
            }
            let mut h = Sha256::new();
            let mut b = [0u8; 65536];
            let mut extent = 0u64;
            loop {
                let n = f.read(&mut b).map_err(|e| LowerError::bad(e.to_string()))?;
                if n == 0 {
                    break;
                }
                extent = extent.checked_add(n as u64).ok_or_else(|| LowerError::bad("source25 length overflow"))?;
                if extent > pin.bytes {
                    return Err(LowerError::bad("source25 payload grew while hashing"));
                }
                h.update(&b[..n]);
            }
            let actual = h.finalize().iter().map(|b| format!("{b:02x}")).collect::<String>();
            if extent != pin.bytes || actual != pin.sha256.to_ascii_lowercase() {
                return Err(LowerError::bad("source25 exact SHA256 differs"));
            }
        }
        if !names.contains("tokenizer.json") || !names.contains("tokenizer_config.json") {
            return Err(LowerError::bad("source25 complete text preprocessing/tokenizer must be pinned"));
        }
        for metadata in ["generation_config.json", "model.safetensors.index.json"] {
            if root.join(metadata).is_file() && !names.contains(metadata) {
                return Err(LowerError::bad("source25 task/shard metadata exists but is unpinned"));
            }
        }
        if !names.contains("config.json") {
            return Err(LowerError::bad("source25 source config must be pinned"));
        }
        let mut weight_files = 0;
        for entry in std::fs::read_dir(&root).map_err(|e| LowerError::bad(e.to_string()))? {
            let entry = entry.map_err(|e| LowerError::bad(e.to_string()))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(".safetensors") {
                weight_files += 1;
                if !names.contains(&name) {
                    return Err(LowerError::bad("source25 has an unpinned source weight shard"));
                }
            }
        }
        if weight_files == 0 {
            return Err(LowerError::bad("source25 single-program conversion requires all original safetensor shards"));
        }
        Ok(())
    }
    pub fn check_frontend(
        &self,
        index: &TensorIndex,
        scope: &FeatureScope,
        source_context: Option<usize>,
        lowered_context: Option<usize>,
        window: Option<u32>,
    ) -> Result<()> {
        if source_context != Some(self.original_context_max as usize)
            || lowered_context != Some(self.original_context_max as usize)
            || window.is_some_and(|w| w < self.original_context_max)
            || self.advertised_task != "text-generation"
            || scope.task != self.advertised_task
            || scope.text_only
            || !scope.excluded.is_empty()
            || self.required_components != ["text".to_string()]
            || !index.has_all_shapes()
            || self.complete_tensor_inventory != index.tensors.keys().cloned().collect::<Vec<_>>()
        {
            return Err(LowerError::bad(
                "source25 original context/full task/component inventory is not implemented by this single-program frontend",
            ));
        }
        Ok(())
    }
}
