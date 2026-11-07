//! A raw GGUF checkpoint, including split containers. No weights are merged or decoded here.
use super::{HEADER_BYTES, MAX_ALIGNMENT};
use crate::frontend_pack::{MAX_CONFIG_BYTES, MAX_PACK_BYTES, MAX_SOURCE_TENSORS, bad, source_dir};
use crate::gguf::{GValue, GgufFile, GgufReadBudget};
use crate::quantfmt::QuantRegistry;
use crate::{LowerError, Result};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

pub(super) const MAX_PARTS: usize = 1024;
const TRANSPORT: &[&str] = &["split.no", "split.count", "split.tensors.count"];
pub(super) const INDEX_FORMAT: &str = "misaka.palw.gguf-checkpoint.v1";

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Index {
    format: String,
    parts: Vec<String>,
}
impl Index {
    fn read(path: &Path) -> Result<Self> {
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .map_err(|e| LowerError::Io(e.to_string()))?
            .take((MAX_PACK_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| LowerError::Io(e.to_string()))?;
        if bytes.len() > MAX_PACK_BYTES {
            return Err(bad("FRONTEND_GGUF_LIMIT: index bytes"));
        }
        let index: Self = serde_json::from_slice(&bytes).map_err(|e| bad(format!("FRONTEND_GGUF_INDEX: {e}")))?;
        if index.format != INDEX_FORMAT || index.parts.is_empty() || index.parts.len() > MAX_PARTS {
            return Err(bad("FRONTEND_GGUF_INDEX: format or part count"));
        }
        let mut unique = BTreeSet::new();
        for name in &index.parts {
            if name.len() > 255
                || name.is_empty()
                || name.contains(['/', '\\', ':'])
                || !matches!(Path::new(name).components().next(), Some(Component::Normal(_)))
                || Path::new(name).components().count() != 1
                || !unique.insert(name)
            {
                return Err(bad("FRONTEND_GGUF_INDEX: distinct local part basenames required"));
            }
        }
        Ok(index)
    }
}
pub(super) fn is_index(path: &Path) -> bool {
    path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with(".gguf.index.json"))
}

/// The public split naming convention: arbitrary prefix, one-based fixed-width part/count.
pub(super) fn split_name(path: &Path) -> Option<(&str, usize, usize)> {
    let base = path.file_name()?.to_str()?.strip_suffix(".gguf")?;
    let (left, count) = base.rsplit_once("-of-")?;
    let (prefix, no) = left.rsplit_once('-')?;
    if prefix.is_empty()
        || no.len() != 5
        || count.len() != 5
        || !no.bytes().all(|b| b.is_ascii_digit())
        || !count.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let no: usize = no.parse().ok()?;
    let count: usize = count.parse().ok()?;
    (no > 0 && no <= count && count <= MAX_PARTS).then_some((prefix, no - 1, count))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Split {
    no: usize,
    count: usize,
    tensors: usize,
}
fn split(file: &GgufFile) -> Result<Option<Split>> {
    let declared = TRANSPORT.iter().filter(|key| file.meta.contains_key(**key)).count();
    if declared == 0 {
        return Ok(None);
    }
    if declared != TRANSPORT.len() {
        return Err(bad("FRONTEND_GGUF_SPLIT: incomplete split metadata"));
    }
    let number = |key: &str| {
        file.meta
            .get(key)
            .and_then(GValue::as_u64)
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| bad(format!("FRONTEND_GGUF_SPLIT: invalid {key}")))
    };
    let s = Split { no: number("split.no")?, count: number("split.count")?, tensors: number("split.tensors.count")? };
    if s.count > MAX_PARTS || s.tensors > MAX_SOURCE_TENSORS || s.no >= s.count.max(1) {
        return Err(bad("FRONTEND_GGUF_SPLIT: part/total count or number exceeds its bound"));
    }
    Ok(Some(s))
}
fn budget() -> GgufReadBudget {
    GgufReadBudget::new(HEADER_BYTES, MAX_CONFIG_BYTES as u64, MAX_SOURCE_TENSORS as u64)
}
fn open_part(path: &Path, registry: &QuantRegistry, budget: &mut GgufReadBudget) -> Result<GgufFile> {
    let file = GgufFile::open_with_budget(path, registry, budget)?;
    if file.alignment > MAX_ALIGNMENT {
        return Err(bad("FRONTEND_GGUF_LIMIT: alignment exceeds 64KiB"));
    }
    Ok(file)
}

pub(super) struct GgufSet {
    pub(super) parts: Vec<GgufFile>,
    pub(super) tensors: BTreeMap<String, usize>,
    registry: QuantRegistry,
    index: Option<(PathBuf, Index)>,
}
impl GgufSet {
    pub(super) fn open(selected: &Path, registry: QuantRegistry, require_geometry: bool) -> Result<Self> {
        let index = if is_index(selected) { Some((selected.to_path_buf(), Index::read(selected)?)) } else { None };
        let first_path = index.as_ref().map_or_else(|| selected.to_path_buf(), |(p, i)| source_dir(p).join(&i.parts[0]));
        let mut remaining = budget();
        let seed = open_part(&first_path, &registry, &mut remaining)?;
        let seed_split = split(&seed)?;
        let paths = if let Some((p, i)) = &index {
            i.parts.iter().map(|n| source_dir(p).join(n)).collect::<Vec<_>>()
        } else if let Some(s) = seed_split.filter(|s| s.count > 1) {
            let (prefix, no, count) = split_name(&first_path)
                .ok_or_else(|| bad("FRONTEND_GGUF_SPLIT: standard filename or explicit .gguf.index.json required"))?;
            if no != s.no || count != s.count {
                return Err(bad("FRONTEND_GGUF_SPLIT: filename disagrees with metadata"));
            }
            (0..count).map(|n| source_dir(&first_path).join(format!("{prefix}-{:05}-of-{count:05}.gguf", n + 1))).collect()
        } else {
            vec![first_path.clone()]
        };
        if seed_split.map_or(paths.len() != 1, |s| s.count.max(1) != paths.len()) {
            return Err(bad("FRONTEND_GGUF_SPLIT: index/part count disagrees with metadata"));
        }
        let mut seed = Some(seed);
        let mut parts = BTreeMap::new();
        for path in paths {
            let file =
                if path == first_path { seed.take().expect("distinct paths") } else { open_part(&path, &registry, &mut remaining)? };
            let current = split(&file)?;
            if current.map(|s| (s.count, s.tensors)) != seed_split.map(|s| (s.count, s.tensors)) {
                return Err(bad("FRONTEND_GGUF_SPLIT: inconsistent part declarations"));
            }
            let no = current.map_or(0, |s| s.no);
            if index.is_none()
                && seed_split.is_some_and(|s| s.count > 1)
                && split_name(&file.path).is_none_or(|(_, named, _)| named != no)
            {
                return Err(bad("FRONTEND_GGUF_SPLIT: filename disagrees with metadata"));
            }
            if parts.insert(no, file).is_some() {
                return Err(bad("FRONTEND_GGUF_SPLIT: duplicate part number"));
            }
        }
        if parts.keys().copied().ne(0..parts.len()) {
            return Err(bad("FRONTEND_GGUF_SPLIT: missing part number"));
        }
        let parts: Vec<_> = parts.into_values().collect();
        let primary = &parts[0];
        let mut tensors = BTreeMap::new();
        for (j, file) in parts.iter().enumerate() {
            if file.version != primary.version {
                return Err(bad("FRONTEND_GGUF_SPLIT: inconsistent container versions"));
            }
            if j != 0 {
                for (key, value) in
                    file.meta.iter().filter(|(key, _)| !TRANSPORT.contains(&key.as_str()) && key.as_str() != "general.alignment")
                {
                    if primary.meta.get(key) != Some(value) {
                        return Err(bad(format!("FRONTEND_GGUF_SPLIT: conflicting or additional metadata {key}")));
                    }
                }
            }
            for (name, t) in &file.tensors {
                if require_geometry && t.ty.block().is_none() {
                    return Err(bad(format!(
                        "FRONTEND_GGUF_FORMAT: tensor {name} needs a block descriptor for GGML type {}",
                        t.ty.id
                    )));
                }
                if tensors.insert(name.clone(), j).is_some() {
                    return Err(bad(format!("FRONTEND_GGUF_SPLIT: duplicate tensor {name}")));
                }
            }
        }
        if seed_split.is_some_and(|s| s.tensors != tensors.len()) {
            return Err(bad("FRONTEND_GGUF_SPLIT: total tensor count mismatch"));
        }
        Ok(Self { parts, tensors, registry, index })
    }
    pub(super) fn inputs(&self) -> Vec<PathBuf> {
        self.index.iter().map(|(p, _)| p.clone()).chain(self.parts.iter().map(|p| p.path.clone())).collect()
    }
    pub(super) fn metadata(&self) -> impl Iterator<Item = (&String, &GValue)> {
        self.parts[0].meta.iter().filter(|(key, _)| !TRANSPORT.contains(&key.as_str()))
    }
    pub(super) fn validate_snapshot(&self) -> Result<()> {
        if let Some((p, expected)) = &self.index
            && &Index::read(p)? != expected
        {
            return Err(bad("FRONTEND_SOURCE_CHANGED: GGUF index changed since acquisition"));
        }
        let mut remaining = budget();
        for file in &self.parts {
            let current = open_part(&file.path, &self.registry, &mut remaining)?;
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
