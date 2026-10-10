//! Checkpoint acquisition only: raw names/layouts, never architecture/name dispatch or decoding.
use super::{FrontendPack, MAX_CONFIG_BYTES, MAX_SOURCE_TENSORS, bad};
use crate::gguf::{GValue, GgufFile};
use crate::quantfmt::QuantRegistry;
use crate::weights::{Checkpoint, Tensor, TensorMeta, TensorSource};
use crate::{LowerError, Result};
use serde_json::Value;
use std::ops::Range;
use std::path::{Path, PathBuf};

const HEADER_BYTES: u64 = 64 << 20;
const MAX_ALIGNMENT: u64 = 65_536;

/// Select one checkpoint. Ambiguous containers require an explicit file path.
/// Unlike the family importer, a GGUF can have a config.json sidecar.
pub fn checkpoint_path(path: &Path) -> Result<PathBuf> {
    if !path.is_dir() {
        return Ok(path.to_path_buf());
    }
    let mut gguf = Vec::new();
    let mut entries = 0usize;
    for entry in std::fs::read_dir(path).map_err(|e| LowerError::Io(e.to_string()))? {
        entries += 1;
        if entries > MAX_SOURCE_TENSORS {
            return Err(bad("FRONTEND_SOURCE_LIMIT: directory inventory"));
        }
        let entry = entry.map_err(|e| LowerError::Io(e.to_string()))?;
        let p = entry.path();
        if p.extension().is_some_and(|x| x == "gguf") && p.is_file() {
            gguf.push(p);
        }
    }
    let ordinary = ["model.safetensors.index.json", "model.safetensors", "pytorch_model.bin.index.json", "pytorch_model.bin"]
        .iter()
        .any(|n| path.join(n).exists());
    if gguf.len() > 1 || (ordinary && !gguf.is_empty()) {
        return Err(bad("FRONTEND_SOURCE_AMBIGUOUS: select an explicit checkpoint file"));
    }
    Ok(gguf.pop().unwrap_or_else(|| path.to_path_buf()))
}

pub fn source_dir(path: &Path) -> &Path {
    if path.is_dir() { path } else { path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new(".")) }
}

enum RawSource {
    Ordinary(Checkpoint),
    Gguf { file: GgufFile, registry: QuantRegistry },
}

/// A producer-side source for the public frontend. GGUF dimensions reverse into row-major shapes;
/// bytes/names stay untouched. Any rotary permutation, gain or other semantic transform is explicit TIR.
pub struct FrontendSource {
    raw: RawSource,
}
impl FrontendSource {
    pub fn is_gguf(&self) -> bool {
        matches!(self.raw, RawSource::Gguf { .. })
    }
    pub fn open(path: &Path, pack: &FrontendPack) -> Result<Self> {
        let selected = checkpoint_path(path)?;
        let raw = if selected.extension().is_some_and(|x| x == "gguf") {
            // Registry lookup computes storage extent only. The binding's pinned local key selects
            // the decoder. Unknown types can supply a self-tested block descriptor and public ID.
            let mut registry = QuantRegistry::builtin().clone();
            for format in pack.formats.values().filter(|f| f.ggml_id().is_some()) {
                if format.as_blocks().is_none() || format.ids().iter().filter(|i| i.scheme == "ggml").count() != 1 {
                    return Err(bad("FRONTEND_GGUF_FORMAT: exactly one GGML ID on a block layout required"));
                }
                if format.ggml_id().is_some_and(|id| (24..=27).contains(&id)) {
                    return Err(bad("FRONTEND_GGUF_FORMAT: scalar integer storage cannot be redefined"));
                }
                registry.add((**format).clone())?;
            }
            let file = open_gguf(&selected, &registry)?;
            if let Some(t) = file.tensors.values().find(|t| t.ty.block().is_none()) {
                return Err(bad(format!(
                    "FRONTEND_GGUF_FORMAT: tensor {} needs a block descriptor for GGML type {}",
                    t.name, t.ty.id
                )));
            }
            RawSource::Gguf { file, registry }
        } else {
            RawSource::Ordinary(Checkpoint::open(&selected)?)
        };
        Ok(Self { raw })
    }

    /// Files holding raw bytes, for output protection and public source-file SHA pins.
    pub fn files(&self) -> Vec<PathBuf> {
        match &self.raw {
            RawSource::Ordinary(c) => c.files.iter().map(|f| f.path.clone()).collect(),
            RawSource::Gguf { file, .. } => vec![file.path.clone()],
        }
    }

    /// The embedded tokenizer's public representation, without inferring an algorithm from a
    /// model name. An explicit external tokenizer file can be chosen by the producer instead.
    pub fn embedded_tokenizer_id(&self) -> Result<Option<[u8; 64]>> {
        let RawSource::Gguf { file, .. } = &self.raw else { return Ok(None) };
        let mut values = serde_json::Map::new();
        for (key, value) in file.meta.iter().filter(|(key, _)| key.starts_with("tokenizer.")) {
            values.insert(key.clone(), metadata(value)?);
        }
        if values.is_empty() {
            return Ok(None);
        }
        let representation = serde_json::json!({"format":"misaka.palw.gguf-tokenizer.v1","metadata":values});
        Ok(Some(crate::artifact::tokenizer_id_of(crate::adapter::canonical_json(&representation).as_bytes())))
    }

    /// Literal GGUF metadata keys join the optional sidecar. All keys must be read or explicitly
    /// inert in the frontend, and all values enter the effective configuration digest.
    /// A sidecar cannot override or hide a native metadata key.
    pub fn configuration(&self, sidecar: Value) -> Result<Value> {
        let mut config = sidecar.as_object().cloned().ok_or_else(|| bad("FRONTEND_ENCODING: config is an object"))?;
        if let RawSource::Gguf { file, .. } = &self.raw {
            for (key, value) in &file.meta {
                if config.insert(key.clone(), metadata(value)?).is_some() {
                    return Err(bad(format!("FRONTEND_GGUF_CONFIG: sidecar duplicates native key {key}")));
                }
            }
        }
        Ok(Value::Object(config))
    }
}
fn open_gguf(path: &Path, registry: &QuantRegistry) -> Result<GgufFile> {
    let file = GgufFile::open_bounded(path, registry, HEADER_BYTES, MAX_CONFIG_BYTES as u64, MAX_SOURCE_TENSORS as u64)?;
    if file.alignment > MAX_ALIGNMENT {
        return Err(bad("FRONTEND_GGUF_LIMIT: alignment exceeds 64KiB"));
    }
    Ok(file)
}
fn metadata(value: &GValue) -> Result<Value> {
    Ok(match value {
        GValue::U8(x) => Value::from(*x),
        GValue::I8(x) => Value::from(*x),
        GValue::U16(x) => Value::from(*x),
        GValue::I16(x) => Value::from(*x),
        GValue::U32(x) => Value::from(*x),
        GValue::I32(x) => Value::from(*x),
        GValue::U64(x) => Value::from(*x),
        GValue::I64(x) => Value::from(*x),
        GValue::F32(x) => serde_json::Number::from_f64(*x as f64)
            .map(Value::Number)
            .ok_or_else(|| bad("FRONTEND_GGUF_CONFIG: non-finite metadata"))?,
        GValue::F64(x) => {
            serde_json::Number::from_f64(*x).map(Value::Number).ok_or_else(|| bad("FRONTEND_GGUF_CONFIG: non-finite metadata"))?
        }
        GValue::Bool(x) => Value::Bool(*x),
        GValue::Str(x) => Value::String(x.clone()),
        GValue::Arr(xs) => Value::Array(xs.iter().map(metadata).collect::<Result<Vec<_>>>()?),
    })
}
impl TensorSource for FrontendSource {
    fn names(&self) -> Vec<String> {
        match &self.raw {
            RawSource::Ordinary(c) => c.names(),
            RawSource::Gguf { file, .. } => file.tensors.keys().cloned().collect(),
        }
    }
    fn shape(&self, name: &str) -> Option<Vec<usize>> {
        self.metadata(name).map(|m| m.shape)
    }
    fn metadata(&self, name: &str) -> Option<TensorMeta> {
        match &self.raw {
            RawSource::Ordinary(c) => c.metadata(name),
            RawSource::Gguf { file, .. } => {
                let t = file.tensors.get(name)?;
                let (elems, bytes) = t.ty.block()?;
                Some(TensorMeta {
                    dtype: t
                        .ty
                        .stored_dtype()
                        .map(|(dtype, _)| dtype.to_string())
                        .unwrap_or_else(|| format!("GGML:{}:{elems}:{bytes}", t.ty.id)),
                    shape: t.shape(),
                    bytes: t.bytes,
                })
            }
        }
    }
    fn load(&self, name: &str) -> Result<Tensor> {
        Err(bad(format!("FRONTEND_SOURCE_RANGE_REQUIRED: {name}; no implicit whole-tensor decoding")))
    }
    fn read_slice(&self, name: &str, range: Range<u64>) -> Result<Vec<u8>> {
        match &self.raw {
            RawSource::Ordinary(c) => c.read_slice(name, range),
            RawSource::Gguf { file, .. } => file.read_range(name, range),
        }
    }
    fn validate_snapshot(&self) -> Result<()> {
        if let RawSource::Gguf { file, registry } = &self.raw {
            let current = open_gguf(&file.path, registry)?;
            if file.version != current.version
                || file.meta != current.meta
                || file.tensors != current.tensors
                || file.alignment != current.alignment
                || file.data_start != current.data_start
            {
                return Err(bad("FRONTEND_SOURCE_CHANGED: GGUF header changed since acquisition"));
            }
        }
        Ok(())
    }
}
