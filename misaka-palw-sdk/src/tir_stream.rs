//! **The TIR inventory root, streamed by byte ranges** (RFC-0002 Part II, the streaming loader).
//!
//! `kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_inventory_root_v1` holds one tensor
//! instance resident at a time — up to 4 GiB, the most a row offset addresses. This computes the same
//! root holding one *leaf* (32 KiB) and a read-ahead window: it asks the consensus layer for the
//! leaves' coordinates in inventory order ([`palw_tir_visit_inventory_rows_v1`]), reads each leaf's
//! bytes from a [`PalwTirRangeSourceV1`] — a `PALWTIR1` container on disk, or a chunk store holding
//! the artifact's canonical chunks — hashes it with the consensus leaf function
//! ([`artifact_leaf_parts_v1`]) and pushes it into the consensus Merkle frontier. No consensus rule is
//! re-implemented; what is new is only the order of reads, and `tests` pin the result equal to
//! the consensus function's, root and leaf count, over containers of every kind this repository
//! produces.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_artifact::{
    PalwArtifactMerkleFrontierV1, PalwArtifactOpeningV1, PalwArtifactOperandV1, artifact_leaf_parts_v1,
};
use kaspa_consensus_core::palw_tir_artifact_v1::{
    PALW_TIR_ROW_PIECE_BYTES_V1, PalwTirInventoryRowV1, PalwTirModelInventoryV2, palw_tir_inventory_leaf_count_v1,
    palw_tir_visit_inventory_rows_v1,
};
use misaka_palw_tir::TirProgramV1;
use misaka_palw_tir_artifact::PalwTirContainerV1;
use misaka_palw_tir_artifact::chunks::{ChunkId, ChunkStore, ChunkedArtifactV1, PALW_TIR_CHUNK_BYTES_V1};
use std::ops::Range;
use std::path::Path;
use std::sync::Mutex;

/// Reads byte ranges of a tensor instance (`param`, `layer`) — little-endian, the declared dtype's
/// width, as the container stores it.
pub trait PalwTirRangeSourceV1 {
    fn read_range(&self, param: u16, layer: Option<u16>, range: Range<u64>, out: &mut [u8]) -> Result<(), String>;
}

/// Reproducible local evidence consumed by the actual-node identity test. This
/// contains no private trace or weights beyond the two bounded openings, and is
/// not a conformance, fidelity, activation or reward certificate.
#[derive(Clone, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct ArtifactTileCourtBundleV3 {
    pub version: u16,
    pub program_bytes: Vec<u8>,
    pub class: kaspa_consensus_core::palw_tir_class_v1::PalwTirClassV1,
    pub inventory_root: Hash64,
    pub params: misaka_palw_kernel::trace::ParamCommitmentsV1,
    pub honest: kaspa_consensus_core::palw_onboarding_v1::ArtifactMismatchProofV1,
    pub false_binding: kaspa_consensus_core::palw_onboarding_v1::ArtifactMismatchProofV1,
}

/// **The inventory root and leaf count**, streamed. Equal to `palw_tir_inventory_root_v1` over the
/// same tensors.
pub fn palw_tir_inventory_root_streamed_v1(program: &TirProgramV1, src: &dyn PalwTirRangeSourceV1) -> Result<(Hash64, u32), String> {
    inventory_root_streamed(Inventory::Legacy(program), src)
}

/// Stream only the model bytes selected by an exact descriptor-scoped inventory. Job-bound
/// tensors need not exist in the range source. The root is meaningful only together with this
/// descriptor/program scope; it does not replace a historical V2 registry artifact root.
pub fn palw_tir_model_inventory_root_streamed_v2(
    inventory: &PalwTirModelInventoryV2<'_>,
    src: &dyn PalwTirRangeSourceV1,
) -> Result<(Hash64, u32), String> {
    inventory_root_streamed(Inventory::Model(inventory), src)
}

fn inventory_root_streamed(inventory: Inventory<'_>, src: &dyn PalwTirRangeSourceV1) -> Result<(Hash64, u32), String> {
    let program = inventory.program();
    let count = inventory.leaf_count()?;
    let mut frontier = PalwArtifactMerkleFrontierV1::new();
    let mut buf = vec![0u8; PALW_TIR_ROW_PIECE_BYTES_V1 as usize];
    let mut failure: Option<String> = None;
    inventory
        .visit_rows(&mut |row: PalwTirInventoryRowV1| {
            if failure.is_some() {
                return;
            }
            let bytes = &mut buf[..row.len as usize];
            let at = row.row_start as u64;
            match src.read_range(row.param, row.layer, at..at + row.len as u64, bytes) {
                Ok(()) => {
                    frontier.push(artifact_leaf_parts_v1(&program.params[row.param as usize].name, row.layer, row.row_start, bytes))
                }
                Err(e) => failure = Some(e),
            }
        })
        .map_err(|e| e.to_string())?;
    if let Some(e) = failure {
        return Err(e);
    }
    debug_assert_eq!(frontier.leaf_count(), count as u64);
    Ok((frontier.root().ok_or("the program declares no param")?, count))
}

/// Open one inventory leaf while retaining a single piece and Merkle frontiers,
/// rather than all tensor bytes or all leaf digests. Sibling intervals partition
/// the other leaves; each interval's frontier computes exactly its promoted subtree.
/// The returned root must still be compared with the registered class's root.
pub fn palw_tir_open_leaf_streamed_v1(
    program: &TirProgramV1,
    src: &dyn PalwTirRangeSourceV1,
    index: u32,
) -> Result<(Hash64, PalwArtifactOpeningV1), String> {
    open_leaf_streamed(Inventory::Legacy(program), src, index)
}

/// Bounded public opening in the model-only inventory, using the same leaf hash and Merkle
/// grammar as v1. The descriptor and program must accompany any authenticated root statement.
pub fn palw_tir_model_open_leaf_streamed_v2(
    inventory: &PalwTirModelInventoryV2<'_>,
    src: &dyn PalwTirRangeSourceV1,
    index: u32,
) -> Result<(Hash64, PalwArtifactOpeningV1), String> {
    open_leaf_streamed(Inventory::Model(inventory), src, index)
}

enum Inventory<'a> {
    Legacy(&'a TirProgramV1),
    Model(&'a PalwTirModelInventoryV2<'a>),
}
impl Inventory<'_> {
    fn program(&self) -> &TirProgramV1 {
        match self {
            Self::Legacy(p) => p,
            Self::Model(i) => i.program(),
        }
    }
    fn leaf_count(&self) -> Result<u32, String> {
        match self {
            Self::Legacy(p) => palw_tir_inventory_leaf_count_v1(p).map_err(|e| e.to_string()),
            Self::Model(i) => Ok(i.leaf_count()),
        }
    }
    fn visit_rows(&self, visit: &mut dyn FnMut(PalwTirInventoryRowV1)) -> Result<(), String> {
        match self {
            Self::Legacy(p) => palw_tir_visit_inventory_rows_v1(p, visit),
            Self::Model(i) => i.visit_rows(visit),
        }
        .map_err(|e| e.to_string())
    }
}

fn open_leaf_streamed(
    inventory: Inventory<'_>,
    src: &dyn PalwTirRangeSourceV1,
    index: u32,
) -> Result<(Hash64, PalwArtifactOpeningV1), String> {
    let program = inventory.program();
    let count = inventory.leaf_count()?;
    if index >= count {
        return Err("the selected leaf is outside the inventory".into());
    }
    let (mut at, mut width, mut level) = (index as u64, count as u64, 0u32);
    let mut siblings = Vec::new();
    while width > 1 {
        if !(at == width - 1 && width % 2 == 1) {
            let sibling = at ^ 1;
            siblings.push((sibling << level, ((sibling + 1) << level).min(count as u64), PalwArtifactMerkleFrontierV1::new()));
        }
        at /= 2;
        width = width.div_ceil(2);
        level += 1;
    }
    // Keep path order in `siblings`; walk the disjoint intervals in stream order.
    let mut order: Vec<usize> = (0..siblings.len()).collect();
    order.sort_unstable_by_key(|k| siblings[*k].0);
    let mut interval = 0usize;
    let mut frontier = PalwArtifactMerkleFrontierV1::new();
    let mut buf = vec![0u8; PALW_TIR_ROW_PIECE_BYTES_V1 as usize];
    let mut selected = None;
    let mut failure = None;
    inventory
        .visit_rows(&mut |row| {
            if failure.is_some() {
                return;
            }
            let bytes = &mut buf[..row.len as usize];
            let start = row.row_start as u64;
            if let Err(e) = src.read_range(row.param, row.layer, start..start + row.len as u64, bytes) {
                failure = Some(e);
                return;
            }
            let name = &program.params[row.param as usize].name;
            let i = frontier.leaf_count();
            let hash = artifact_leaf_parts_v1(name, row.layer, row.row_start, bytes);
            frontier.push(hash);
            if i == index as u64 {
                selected = Some(PalwArtifactOperandV1 {
                    tensor_name: name.clone(),
                    layer: row.layer,
                    row_start: row.row_start,
                    bytes: bytes.to_vec(),
                });
            } else {
                while interval < order.len() && i >= siblings[order[interval]].1 {
                    interval += 1;
                }
                let Some(&k) = order.get(interval) else {
                    failure = Some("the proof intervals do not cover the inventory".into());
                    return;
                };
                if i < siblings[k].0 {
                    failure = Some("the proof intervals leave an inventory gap".into());
                    return;
                }
                siblings[k].2.push(hash);
            }
        })
        .map_err(|e| e.to_string())?;
    if let Some(e) = failure {
        return Err(e);
    }
    let path = siblings.iter().map(|(_, _, f)| f.root().ok_or("an empty sibling interval".to_string())).collect::<Result<_, _>>()?;
    let root = frontier.root().ok_or("an empty inventory")?;
    let opening = PalwArtifactOpeningV1 {
        operand: selected.ok_or("the inventory omitted the selected leaf")?,
        leaf_index: index,
        leaf_count: count,
        path,
    };
    kaspa_consensus_core::palw_artifact::verify_artifact_opening_v1(&opening, root).map_err(|e| e.to_string())?;
    Ok((root, opening))
}

/// How much a [`ContainerRanges`] reads ahead.
const WINDOW: u64 = 4 << 20;

/// A `PALWTIR1` container's tensors by range: each read is served from a 4 MiB window of the file
/// (the walk is sequential, so one `pread` serves about a hundred leaves).
pub struct ContainerRanges<'c> {
    c: &'c PalwTirContainerV1,
    file: std::fs::File,
    window: Mutex<(u64, Vec<u8>)>,
    window_bytes: u64,
}

impl<'c> ContainerRanges<'c> {
    pub fn open(c: &'c PalwTirContainerV1) -> Result<Self, String> {
        Self::open_with_window(c, WINDOW)
    }

    /// [`Self::open`] with a read-ahead of `window_bytes` (at least one request): the sequential walk of the inventory wants 4 MiB, a reader of
    /// scattered row tiles (RFC-0013 §5, `tir_rows`) wants a few leaves.
    pub fn open_with_window(c: &'c PalwTirContainerV1, window_bytes: u64) -> Result<Self, String> {
        Ok(Self {
            c,
            file: std::fs::File::open(&c.path).map_err(|e| format!("{}: {e}", c.path.display()))?,
            window: Mutex::new((0, Vec::new())),
            window_bytes: window_bytes.max(1),
        })
    }
}

#[cfg(unix)]
pub(crate) fn pread(file: &std::fs::File, buf: &mut [u8], at: u64) -> std::io::Result<()> {
    use std::os::unix::fs::FileExt;
    file.read_exact_at(buf, at)
}

#[cfg(not(unix))]
pub(crate) fn pread(file: &std::fs::File, buf: &mut [u8], at: u64) -> std::io::Result<()> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = file.try_clone()?;
    f.seek(SeekFrom::Start(at))?;
    f.read_exact(buf)
}

impl PalwTirRangeSourceV1 for ContainerRanges<'_> {
    fn read_range(&self, param: u16, layer: Option<u16>, range: Range<u64>, out: &mut [u8]) -> Result<(), String> {
        let (off, bytes) = self
            .c
            .locate(param, layer)
            .ok_or_else(|| format!("param {param} (layer {layer:?}) is not in {}", self.c.path.display()))?;
        if range.end > bytes || out.len() as u64 != range.end - range.start {
            return Err(format!("bytes {range:?} of a tensor of {bytes}"));
        }
        let abs = off + range.start;
        let mut w = self.window.lock().expect("window");
        if !(abs >= w.0 && abs + out.len() as u64 <= w.0 + w.1.len() as u64) {
            // A new window from here, as far as the file (not past this tensor's end: the next
            // tensor is a different instance, read when the walk reaches it).
            let len = self.window_bytes.max(out.len() as u64).min(self.c.file_len - abs);
            w.1.resize(len as usize, 0);
            pread(&self.file, &mut w.1, abs).map_err(|e| format!("{}: {e}", self.c.path.display()))?;
            w.0 = abs;
        }
        let s = (abs - w.0) as usize;
        out.copy_from_slice(&w.1[s..s + out.len()]);
        Ok(())
    }
}

/// An artifact's tensors by range, from its chunk store. Chunks are canonical (every chunk of an
/// instance but the last is exactly [`PALW_TIR_CHUNK_BYTES_V1`]), so a range names its chunk by
/// division; the chunk just read is kept, and every chunk read is re-hashed against its address.
pub struct ChunkedRanges<'s> {
    store: &'s ChunkStore,
    artifact: &'s ChunkedArtifactV1,
    last: Mutex<Option<(ChunkId, Vec<u8>)>>,
}

impl<'s> ChunkedRanges<'s> {
    pub fn new(store: &'s ChunkStore, artifact: &'s ChunkedArtifactV1) -> Self {
        Self { store, artifact, last: Mutex::new(None) }
    }
}

impl PalwTirRangeSourceV1 for ChunkedRanges<'_> {
    fn read_range(&self, param: u16, layer: Option<u16>, range: Range<u64>, out: &mut [u8]) -> Result<(), String> {
        let t =
            self.artifact.tensors.get(&(param, layer)).ok_or_else(|| format!("param {param} (layer {layer:?}) was never produced"))?;
        if range.end > t.bytes || out.len() as u64 != range.end - range.start {
            return Err(format!("bytes {range:?} of a tensor of {}", t.bytes));
        }
        let mut done = 0usize;
        let mut at = range.start;
        while done < out.len() {
            let k = (at / PALW_TIR_CHUNK_BYTES_V1 as u64) as usize;
            let c = t.chunks.get(k).ok_or_else(|| format!("no chunk {k} in param {param}'s {} chunks", t.chunks.len()))?;
            let want = if k + 1 < t.chunks.len() {
                PALW_TIR_CHUNK_BYTES_V1 as u32
            } else {
                (t.bytes - k as u64 * PALW_TIR_CHUNK_BYTES_V1 as u64) as u32
            };
            if c.len != want {
                return Err(format!("chunk {k} of param {param} is {} bytes, canonical chunking needs {want}", c.len));
            }
            let mut last = self.last.lock().expect("chunk");
            if last.as_ref().map(|(id, _)| *id) != Some(c.id) {
                *last = Some((c.id, self.store.get(&c.id).map_err(|e| format!("chunk {}: {e}", c.id))?));
            }
            let bytes = &last.as_ref().expect("just set").1;
            let from = (at - k as u64 * PALW_TIR_CHUNK_BYTES_V1 as u64) as usize;
            let n = (bytes.len() - from).min(out.len() - done);
            out[done..done + n].copy_from_slice(&bytes[from..from + n]);
            done += n;
            at += n as u64;
        }
        Ok(())
    }
}

/// The streamed inventory root of the container at `path`.
pub fn palw_tir_inventory_root_of_file_v1(path: &Path) -> Result<(Hash64, u32), String> {
    let c = PalwTirContainerV1::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    palw_tir_inventory_root_streamed_v1(&c.program, &ContainerRanges::open(&c)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tir_manifest::PalwTirContainerSourceV1;
    use kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_inventory_root_v1;
    use misaka_palw_base0::artifact::{Base0ArtifactV1, Base0ShapeV1, LN_THETA_10000_GEN_Q};
    use misaka_palw_base0::engine_a16::derived_a16_store;
    use misaka_palw_base0::tir_a16::convert_a16_to_tir;
    use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
    use misaka_palw_tir_artifact::chunks::put_instance;

    fn converted(tag: &str, layers: usize, vocab: usize) -> std::path::PathBuf {
        let shape = Base0ShapeV1 {
            n_layers: layers,
            n_heads: 4,
            n_kv_heads: 2,
            d_head: 8,
            d_ff: 48,
            vocab,
            max_position: 32,
            ln_theta_gen_q: LN_THETA_10000_GEN_Q,
            eps_q: 1,
        };
        let a = Base0ArtifactV1::derive_deterministic(shape, 0x3F3)
            .expect("shape")
            .with_a16_params(derived_a16_store(&shape))
            .expect("store");
        let path = std::env::temp_dir().join(format!("tir-stream-{tag}-{}.palwtir", std::process::id()));
        convert_a16_to_tir(&a, HISTORY_BOUND_V1_SMALL, &path, "{}".into()).expect("converted");
        path
    }

    #[test]
    fn the_streamed_root_is_the_consensus_root_over_a_container() {
        for (tag, layers, vocab) in [("a", 2usize, 64usize), ("b", 3, 100), ("c", 1, 17)] {
            let path = converted(tag, layers, vocab);
            let c = PalwTirContainerV1::open(&path).expect("opens");
            let want = palw_tir_inventory_root_v1(&c.program, &PalwTirContainerSourceV1(&c)).expect("consensus root");
            let got =
                palw_tir_inventory_root_streamed_v1(&c.program, &ContainerRanges::open(&c).expect("ranges")).expect("streamed root");
            assert_eq!(got, want, "{tag}");
            assert_eq!(palw_tir_inventory_root_of_file_v1(&path).expect("of file"), want);
            let _ = std::fs::remove_file(&path);
        }
    }

    #[test]
    fn streamed_openings_match_the_consensus_paths_including_promoted_tails() {
        use kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_open_leaf_v1;
        let path = converted("openings", 1, 17);
        let c = PalwTirContainerV1::open(&path).unwrap();
        let ranges = ContainerRanges::open_with_window(&c, 32768).unwrap();
        let source = PalwTirContainerSourceV1(&c);
        let (root, count) = palw_tir_inventory_root_v1(&c.program, &source).unwrap();
        assert!(!count.is_power_of_two());
        for index in [0, 1, 2, count / 2, count - 3, count - 2, count - 1] {
            let (got, opening) = palw_tir_open_leaf_streamed_v1(&c.program, &ranges, index).unwrap();
            assert_eq!(got, root);
            assert_eq!(opening, palw_tir_open_leaf_v1(&c.program, &source, index).unwrap());
        }
        assert!(palw_tir_open_leaf_streamed_v1(&c.program, &ranges, count).is_err());
        struct Missing;
        impl PalwTirRangeSourceV1 for Missing {
            fn read_range(&self, _: u16, _: Option<u16>, _: Range<u64>, _: &mut [u8]) -> Result<(), String> {
                Err("missing".into())
            }
        }
        assert_eq!(palw_tir_open_leaf_streamed_v1(&c.program, &Missing, 0).unwrap_err(), "missing");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn the_streamed_root_over_chunks_is_the_same_root() {
        let path = converted("chunks", 2, 64);
        let c = PalwTirContainerV1::open(&path).expect("opens");
        let want = palw_tir_inventory_root_v1(&c.program, &PalwTirContainerSourceV1(&c)).expect("consensus root");
        // The same tensors as chunks of the canonical size — and of a size that cuts mid-row.
        let dir = std::env::temp_dir().join(format!("tir-stream-chunks-{}", std::process::id()));
        let store = ChunkStore::open(&dir).expect("store");
        let mut art = ChunkedArtifactV1::default();
        for e in &c.header.tensors {
            art.insert(put_instance(&store, e.param, e.layer, &c.read_tensor_bytes(e.param, e.layer).expect("bytes")).expect("put"));
        }
        let got = palw_tir_inventory_root_streamed_v1(&c.program, &ChunkedRanges::new(&store, &art)).expect("streamed");
        assert_eq!(got, want);
        // A corrupted chunk is refused.
        let victim = art.tensors.values().next().expect("a tensor").chunks[0].id;
        std::fs::write(store.path_of(&victim), b"corrupt").expect("corrupt");
        assert!(palw_tir_inventory_root_streamed_v1(&c.program, &ChunkedRanges::new(&store, &art)).is_err());
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_file(&path);
    }

    /// A container the lowerer's streaming converter makes from one of its Hugging Face fixtures.
    fn lowered(name: &str) -> std::path::PathBuf {
        lowered_as(name, "")
    }

    fn lowered_as(name: &str, tag: &str) -> std::path::PathBuf {
        use misaka_palw_tir_lower::convert::{ConvertOpts, convert_to_container};
        use misaka_palw_tir_lower::float_ref::stream::Streamed;
        use misaka_palw_tir_lower::lower::{LowerOpts, StreamOpts};
        use misaka_palw_tir_lower::quant::QuantPolicy;
        use misaka_palw_tir_lower::{fidelity, weights::Checkpoint};
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../misaka-palw-tir-lower/tests/fixtures/hf").join(name);
        let cfg = std::fs::read_to_string(dir.join("config.json")).expect("config");
        let prep = fidelity::prepare(&cfg, &LowerOpts::default()).expect("prepare");
        let ck = Checkpoint::open(&dir).expect("checkpoint");
        let loader = Streamed::new(&prep.hl, &prep.binding, &ck);
        let calib = fidelity::random_sequences(prep.hl.vocab, 2, 12, 7);
        let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &|_, _| {}).expect("calibrate");
        let out = std::env::temp_dir().join(format!("tir-stream-lowered-{name}{tag}-{}.palwtir", std::process::id()));
        let store = std::env::temp_dir().join(format!("tir-stream-lowered-{name}{tag}-{}.chunks", std::process::id()));
        let meta = |_: &misaka_palw_tir_lower::lower::StreamMaterialised| serde_json::json!({"fixture": name});
        let opts = ConvertOpts {
            stream: StreamOpts { defer_min_elems: 0, block_elems: 64 },
            layout: Vec::new(),
            tokenizer_id: [4u8; 64],
            meta: &meta,
            keep_chunks: false,
            math: misaka_palw_tir_lower::detmath::MathMode::LibmV1,
        };
        convert_to_container(&prep, &loader, &stats, &QuantPolicy::default(), &store, &out, &opts, &|_, _| {}).expect("converted");
        out
    }

    #[test]
    fn a_container_the_lowerer_streamed_has_the_same_manifest_derived_either_way() {
        use crate::tir_manifest::PalwTirManifestV1;
        for name in ["llama", "mixtral", "qwen3_5", "gemma2"] {
            let path = lowered(name);
            let (a, b) =
                (PalwTirManifestV1::derive(&path).expect("derive"), PalwTirManifestV1::derive_streamed(&path).expect("streamed"));
            assert_eq!(a, b, "{name}");
            assert!(a.leaf_count > 100, "{name}");
            // The checked manifest round-trips and agrees with the file.
            b.check(&path).expect("agrees");
            let _ = std::fs::remove_file(&path);
        }
    }

    /// `math: "libm-v1"` is what lets a seat rebuild an artifact and get the root: the same conversion
    /// on a pool of one thread and a pool of seven gives the same inventory root. Families whose tables are
    /// transcendental (RoPE variants, SSM, activations) are the ones that would show a difference.
    #[test]
    fn a_libm_v1_conversion_has_one_root_on_any_thread_count() {
        use crate::tir_manifest::PalwTirManifestV1;
        let pool = |n: usize| rayon::ThreadPoolBuilder::new().num_threads(n).build().expect("pool");
        let (one, many) = (pool(1), pool(7));
        for name in ["llama", "gemma2", "qwen3_5", "mamba2", "phi3_longrope"] {
            let (a, b) = (one.install(|| lowered_as(name, "-t1")), many.install(|| lowered_as(name, "-t7")));
            let (ra, rb) =
                (PalwTirManifestV1::derive_streamed(&a).expect("root"), PalwTirManifestV1::derive_streamed(&b).expect("root"));
            assert_eq!(ra.inventory_root, rb.inventory_root, "{name}: the root depends on the thread count");
            assert_eq!(ra, rb, "{name}");
            let _ = (std::fs::remove_file(&a), std::fs::remove_file(&b));
        }
    }

    #[test]
    fn a_range_past_a_tensor_is_refused() {
        let path = converted("range", 1, 17);
        let c = PalwTirContainerV1::open(&path).expect("opens");
        let r = ContainerRanges::open(&c).expect("ranges");
        let mut b = [0u8; 8];
        assert!(r.read_range(0, None, 0..8, &mut b).is_ok());
        let bytes = c.locate(0, None).expect("tensor").1;
        assert!(r.read_range(0, None, bytes - 4..bytes + 4, &mut b).is_err());
        assert!(r.read_range(9999, None, 0..8, &mut b).is_err());
        let _ = std::fs::remove_file(&path);
    }
}
