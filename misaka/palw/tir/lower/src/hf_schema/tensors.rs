//! **What the reader may know about a checkpoint without its weights**: tensor names, dtypes and
//! shapes — a safetensors header is enough (`palw-class` preflight reads nothing more). A
//! `model.safetensors.index.json` alone gives names without shapes.

use crate::error::{LowerError, Result};
use crate::weights::{Checkpoint, TensorSource};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TensorEntry {
    /// The safetensors dtype name (`BF16`, `F32`, `I64`, …); empty when only an index was read.
    pub dtype: String,
    /// `None` when only an index (names) was read.
    pub shape: Option<Vec<usize>>,
}

/// Tensor names (with dtypes and shapes when a header was read) of one checkpoint.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct TensorIndex {
    pub tensors: BTreeMap<String, TensorEntry>,
}

/// A tensor source that knows **names and shapes only** — a safetensors header — which is all the shape
/// check of the weight binding needs (`weights::check_weights`); it holds no data.
pub struct HeaderSource<'a>(pub &'a TensorIndex);

impl TensorSource for HeaderSource<'_> {
    fn shape(&self, name: &str) -> Option<Vec<usize>> {
        self.0.shape(name).map(<[usize]>::to_vec)
    }
    fn load(&self, name: &str) -> Result<crate::weights::Tensor> {
        Err(LowerError::weights(format!("`{name}`: a header-only source holds no tensor data")))
    }
    fn names(&self) -> Vec<String> {
        self.0.names().map(str::to_string).collect()
    }
}

impl TensorIndex {
    /// Whether every tensor has a shape (a header was read, not just an index).
    pub fn has_all_shapes(&self) -> bool {
        self.tensors.values().all(|e| e.shape.is_some())
    }

    /// From names alone (a hub file listing, an index file).
    pub fn from_names<I: IntoIterator<Item = S>, S: Into<String>>(names: I) -> Self {
        TensorIndex { tensors: names.into_iter().map(|n| (n.into(), TensorEntry { dtype: String::new(), shape: None })).collect() }
    }

    /// From names with shapes (tests, GGUF-served tensors).
    pub fn from_shapes<I: IntoIterator<Item = (S, Vec<usize>)>, S: Into<String>>(it: I) -> Self {
        TensorIndex {
            tensors: it.into_iter().map(|(n, s)| (n.into(), TensorEntry { dtype: String::new(), shape: Some(s) })).collect(),
        }
    }

    /// Read the headers of a checkpoint directory, a `.safetensors` file or an index whose shards
    /// are present. Reads headers only: no tensor data.
    pub fn from_checkpoint_path(path: &Path) -> Result<Self> {
        if path.extension().and_then(|e| e.to_str()) == Some("json") && !path.parent().map(|d| d.join("model.safetensors").exists()).unwrap_or(false) {
            // An index: names only unless the shards sit beside it.
            let dir = path.parent().unwrap_or(Path::new("."));
            let v: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(path).map_err(|e| LowerError::Io(format!("{}: {e}", path.display())))?,
            )
            .map_err(|e| LowerError::weights(format!("index: {e}")))?;
            let wm = v.get("weight_map").and_then(|w| w.as_object()).ok_or_else(|| LowerError::weights("index has no weight_map"))?;
            let shards_present = wm.values().filter_map(|x| x.as_str()).all(|s| dir.join(s).exists());
            if !shards_present {
                return Ok(Self::from_names(wm.keys().cloned()));
            }
        }
        let ck = Checkpoint::open(path)?;
        let mut tensors = BTreeMap::new();
        for f in &ck.files {
            for (n, e) in &f.entries {
                tensors.insert(n.clone(), TensorEntry { dtype: e.dtype.clone(), shape: Some(e.shape.clone()) });
            }
        }
        Ok(TensorIndex { tensors })
    }

    /// From any tensor source (names and shapes; no data).
    pub fn from_source(src: &dyn TensorSource) -> Self {
        TensorIndex {
            tensors: src
                .names()
                .into_iter()
                .map(|n| {
                    let shape = src.shape(&n);
                    (n, TensorEntry { dtype: String::new(), shape })
                })
                .collect(),
        }
    }

    pub fn has(&self, name: &str) -> bool {
        self.tensors.contains_key(name)
    }

    pub fn shape(&self, name: &str) -> Option<&[usize]> {
        self.tensors.get(name).and_then(|e| e.shape.as_deref())
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.tensors.keys().map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.tensors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tensors.is_empty()
    }
}
