//! **A stored Merkle index of a TIR artifact** (RFC-0013 §5 and §7.2; the "stored leaf-hash index" the beacon-conformance record lists as
//! not built).
//!
//! Authenticating a byte range of an artifact used to mean hashing the artifact: [`crate::tir_stream`] reads every 32 KiB leaf in inventory
//! order and folds them into the root (3–7 s for a 1.87 GB class, proportional to its size), and `authenticated_openings` pays that pass
//! again to open the leaves a beacon draw names. This module stores what that pass computes — **one 64-byte hash per leaf, in inventory
//! order** — beside the artifact, so that afterwards
//!
//! * the index is checked once against the root the registration or the pack pins: reading `n` hashes and folding them is
//!   `n − 1` node hashes (`n ≈ 57 000` for 1.87 GB), not a pass over the bytes;
//! * a byte range is authenticated by hashing **only the leaves that cover it** and comparing each with the stored leaf
//!   ([`PalwTirMerkleIndexV1::read_authenticated`]) — a row tile of a tensor larger than memory is checked without reading the rest;
//! * the leaves a beacon draw names are opened with the consensus multiproof built from the stored hashes
//!   ([`PalwTirMerkleIndexV1::multiproof`]), reading only those leaves' bytes.
//!
//! # What the index is — and is not
//!
//! * **Not an authority.** It is a cache of a computation anyone can redo. A loaded index is trusted only after
//!   [`PalwTirMerkleIndexV1::verify_root`] shows its leaves fold to the root the *caller* holds; a leaf the file got wrong, a leaf from
//!   another artifact, a truncated or reordered file all fail that check or the file's own checks. After it, a stored leaf is the true
//!   leaf of the committed artifact (collision resistance of the leaf and node hashes), so a range whose leaves hash to the stored ones is
//!   the committed artifact's bytes.
//! * **Not a shortcut past the first pass.** Building an index reads the whole artifact once ([`PalwTirMerkleIndexV1::build_streamed`]); the
//!   saving is every later authentication. An index built from a file is only as good as that file: build it from the bytes whose root you
//!   have already checked, or check the built index's root against the registered one (that is the point of
//!   [`PalwTirMerkleIndexV1::verify_root`]).
//! * **No consensus rule moves.** The leaves are the consensus leaves (`artifact_leaf_parts_v1`), the tree is the consensus tree
//!   (`artifact_root_v1`, an odd last node promoted), the coordinates are `PalwTirInventoryIndexV1`'s. The tests hold the root to
//!   `palw_tir_inventory_root_v1` and every multiproof to `verify_artifact_multiproof_v1`.
//!
//! # File format (`PALWTMX1`, version 1)
//!
//! ```text
//!   magic 8 = "PALWTMX1" | version u16 LE | leaf_count u32 LE | root 64 | program_binding 64 | leaf_count × 64 | trailer 64
//! ```
//!
//! `program_binding` is the keyed BLAKE2b-512 of the program's canonical bytes (an index of another program is refused by name even when
//! the leaf counts happen to agree); `trailer` is the keyed BLAKE2b-512 of everything before it (a truncated or bit-rotted file is refused as
//! a format error rather than as a wrong root). Neither is a signature.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_artifact::{
    PalwArtifactMerkleFrontierV1, PalwArtifactMultiproofV1, PalwArtifactOperandV1, artifact_leaf_parts_v1, artifact_root_v1,
    palw_artifact_multiproof_v1,
};
use kaspa_consensus_core::palw_tir_artifact_v1::{
    PALW_TIR_ROW_PIECE_BYTES_V1, PalwTirInventoryRowV1, palw_tir_inventory_leaf_count_v1, palw_tir_tensor_bytes_v1,
    palw_tir_visit_inventory_rows_v1,
};
use kaspa_consensus_core::palw_tir_court_v1::PalwTirInventoryIndexV1;
use misaka_palw_tir::TirProgramV1;
use std::ops::Range;
use std::path::Path;

use crate::tir_stream::PalwTirRangeSourceV1;

/// The file's magic.
pub const PALW_TIR_MERKLE_INDEX_MAGIC_V1: [u8; 8] = *b"PALWTMX1";
/// The file's version.
pub const PALW_TIR_MERKLE_INDEX_VERSION_V1: u16 = 1;

const PROGRAM_BINDING_KEY: &[u8] = b"misaka-palw/tir-merkle-index/program/v1";
const TRAILER_KEY: &[u8] = b"misaka-palw/tir-merkle-index/trailer/v1";
const HEADER_BYTES: usize = 8 + 2 + 4 + 64 + 64;

/// Why an index could not be built, read or used. Every variant is a refusal; none is a guess.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwTirIndexError {
    /// The bytes are not an index (magic, version, length, trailer, leaf count, or a stored root that is not the leaves' fold).
    Format(String),
    /// The index was made for another program than the one it is being used with.
    NotThisProgram,
    /// The leaves fold to `stored`, but the caller holds `expected`.
    RootMismatch { stored: Hash64, expected: Hash64 },
    /// The artifact's bytes at `leaf` do not hash to the stored leaf: the file is not the indexed artifact (or it changed).
    LeafMismatch { leaf: u32 },
    /// The requested instance, range or leaf is not in the inventory.
    OutOfRange(String),
    /// The byte source failed.
    Source(String),
}

impl std::fmt::Display for PalwTirIndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Format(e) => write!(f, "not a Merkle index of a TIR artifact: {e}"),
            Self::NotThisProgram => write!(f, "this Merkle index was made for another program"),
            Self::RootMismatch { stored, expected } => {
                write!(f, "the index's leaves fold to {stored}, not the root {expected} the caller holds")
            }
            Self::LeafMismatch { leaf } => {
                write!(f, "inventory leaf {leaf} does not hash to the stored leaf: the file is not the indexed artifact")
            }
            Self::OutOfRange(e) => write!(f, "outside the inventory: {e}"),
            Self::Source(e) => write!(f, "the artifact could not be read: {e}"),
        }
    }
}

impl std::error::Error for PalwTirIndexError {}

/// What an authenticated read cost: the leaves hashed and the bytes they held (the range asked for is a part of them).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwTirAuthReadV1 {
    pub leaves: u32,
    pub bytes_hashed: u64,
}

/// **Every leaf hash of a TIR artifact's inventory, in inventory order, with the root they fold to.**
#[derive(Clone, Debug)]
pub struct PalwTirMerkleIndexV1 {
    inventory: PalwTirInventoryIndexV1,
    program_binding: Hash64,
    root: Hash64,
    leaves: Vec<Hash64>,
}

fn keyed(key: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
    for part in parts {
        state.update(part);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

fn program_binding(program: &TirProgramV1) -> Hash64 {
    keyed(PROGRAM_BINDING_KEY, &[&program.encode()])
}

impl PalwTirMerkleIndexV1 {
    /// **The index over leaf hashes already computed**, in inventory order: refused unless they are exactly the program's leaf count.
    pub fn from_leaves(program: &TirProgramV1, leaves: Vec<Hash64>) -> Result<Self, PalwTirIndexError> {
        let inventory = PalwTirInventoryIndexV1::new(program)
            .ok_or_else(|| PalwTirIndexError::Format("the TIR inventory refuses this program".into()))?;
        if leaves.is_empty() || leaves.len() as u64 != inventory.leaf_count() as u64 {
            return Err(PalwTirIndexError::Format(format!(
                "{} leaf hashes for an inventory of {}",
                leaves.len(),
                inventory.leaf_count()
            )));
        }
        let root =
            artifact_root_v1(&leaves).ok_or_else(|| PalwTirIndexError::Format("an inventory with no leaf has no root".into()))?;
        Ok(Self { inventory, program_binding: program_binding(program), root, leaves })
    }

    /// **Build the index: one streamed pass** over the artifact's bytes (32 KiB resident), hashing every leaf with the consensus leaf function.
    /// The root is the one `palw_tir_inventory_root_streamed_v1` returns; check it against the registered root with [`Self::verify_root`].
    pub fn build_streamed(program: &TirProgramV1, src: &dyn PalwTirRangeSourceV1) -> Result<Self, PalwTirIndexError> {
        let count = palw_tir_inventory_leaf_count_v1(program).map_err(|e| PalwTirIndexError::Format(e.to_string()))?;
        let mut leaves = Vec::with_capacity(count as usize);
        let mut buf = vec![0u8; PALW_TIR_ROW_PIECE_BYTES_V1 as usize];
        let mut failure: Option<String> = None;
        palw_tir_visit_inventory_rows_v1(program, &mut |row: PalwTirInventoryRowV1| {
            if failure.is_some() {
                return;
            }
            let bytes = &mut buf[..row.len as usize];
            let at = row.row_start as u64;
            match src.read_range(row.param, row.layer, at..at + row.len as u64, bytes) {
                Ok(()) => {
                    leaves.push(artifact_leaf_parts_v1(&program.params[row.param as usize].name, row.layer, row.row_start, bytes))
                }
                Err(e) => failure = Some(e),
            }
        })
        .map_err(|e| PalwTirIndexError::Format(e.to_string()))?;
        if let Some(e) = failure {
            return Err(PalwTirIndexError::Source(e));
        }
        Self::from_leaves(program, leaves)
    }

    /// The root the leaves fold to.
    pub fn root(&self) -> Hash64 {
        self.root
    }

    pub fn leaf_count(&self) -> u32 {
        self.inventory.leaf_count()
    }

    /// Every leaf hash, in inventory order.
    pub fn leaves(&self) -> &[Hash64] {
        &self.leaves
    }

    /// The consensus coordinates of the inventory this index is over.
    pub fn inventory(&self) -> &PalwTirInventoryIndexV1 {
        &self.inventory
    }

    /// **Trust the index only as far as this holds**: its leaves fold to the root the caller has from the registration, the class record or
    /// the pack. (The fold was computed when the index was built or loaded; this compares it, and so costs nothing.)
    pub fn verify_root(&self, expected: Hash64) -> Result<(), PalwTirIndexError> {
        if self.root == expected { Ok(()) } else { Err(PalwTirIndexError::RootMismatch { stored: self.root, expected }) }
    }

    // ------------------------------------------------------------------------------------------------------------------
    // The file
    // ------------------------------------------------------------------------------------------------------------------

    /// The index as the bytes of a `PALWTMX1` file.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_BYTES + self.leaves.len() * 64 + 64);
        out.extend_from_slice(&PALW_TIR_MERKLE_INDEX_MAGIC_V1);
        out.extend_from_slice(&PALW_TIR_MERKLE_INDEX_VERSION_V1.to_le_bytes());
        out.extend_from_slice(&self.inventory.leaf_count().to_le_bytes());
        out.extend_from_slice(self.root.as_byte_slice());
        out.extend_from_slice(self.program_binding.as_byte_slice());
        for leaf in &self.leaves {
            out.extend_from_slice(leaf.as_byte_slice());
        }
        let trailer = keyed(TRAILER_KEY, &[&out]);
        out.extend_from_slice(trailer.as_byte_slice());
        out
    }

    /// **Read a `PALWTMX1` file's bytes for `program`.** Refused unless the file is exactly the format, is for this program, has the program's
    /// leaf count, an intact trailer, and a stored root equal to the fold of its leaves. It is *not* yet trusted: call [`Self::verify_root`]
    /// with the root you hold.
    pub fn decode(program: &TirProgramV1, bytes: &[u8]) -> Result<Self, PalwTirIndexError> {
        let format = |why: &str| PalwTirIndexError::Format(why.to_string());
        if bytes.len() < HEADER_BYTES + 64 || bytes[0..8] != PALW_TIR_MERKLE_INDEX_MAGIC_V1 {
            return Err(format("the magic or the length is wrong"));
        }
        if u16::from_le_bytes([bytes[8], bytes[9]]) != PALW_TIR_MERKLE_INDEX_VERSION_V1 {
            return Err(format("an unknown version"));
        }
        let inventory = PalwTirInventoryIndexV1::new(program).ok_or_else(|| format("the TIR inventory refuses this program"))?;
        let count = u32::from_le_bytes(bytes[10..14].try_into().expect("4 bytes"));
        if count != inventory.leaf_count() {
            return Err(PalwTirIndexError::Format(format!("{count} leaves for an inventory of {}", inventory.leaf_count())));
        }
        let want_len = HEADER_BYTES as u128 + count as u128 * 64 + 64;
        if bytes.len() as u128 != want_len {
            return Err(format("the length is not the header, the leaves and the trailer"));
        }
        let (body, trailer) = bytes.split_at(bytes.len() - 64);
        if keyed(TRAILER_KEY, &[body]).as_byte_slice() != trailer {
            return Err(format("the trailer does not match: the file is truncated or damaged"));
        }
        let hash_at = |at: usize| Hash64::from_bytes(bytes[at..at + 64].try_into().expect("64 bytes"));
        let (stored_root, binding) = (hash_at(14), hash_at(78));
        if binding != program_binding(program) {
            return Err(PalwTirIndexError::NotThisProgram);
        }
        let leaves: Vec<Hash64> = (0..count as usize).map(|i| hash_at(HEADER_BYTES + i * 64)).collect();
        let root = artifact_root_v1(&leaves).ok_or_else(|| format("an index with no leaf"))?;
        if root != stored_root {
            return Err(format("the stored root is not the fold of the stored leaves"));
        }
        Ok(Self { inventory, program_binding: binding, root, leaves })
    }

    /// Write the file atomically (a sibling temp file, then a rename): a reader never sees half an index.
    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
        std::fs::write(&tmp, self.encode())?;
        std::fs::rename(&tmp, path).inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })
    }

    /// Read and decode the file at `path` for `program`.
    pub fn read(program: &TirProgramV1, path: &Path) -> Result<Self, PalwTirIndexError> {
        let bytes = std::fs::read(path).map_err(|e| PalwTirIndexError::Source(format!("{}: {e}", path.display())))?;
        Self::decode(program, &bytes)
    }

    // ------------------------------------------------------------------------------------------------------------------
    // Authenticated reads
    // ------------------------------------------------------------------------------------------------------------------

    /// **Read `range` of tensor instance `(param, layer)` into `out`, authenticated.** Every leaf that covers any byte of the range is read
    /// whole from `src`, hashed with the consensus leaf function and compared with the stored leaf; a mismatch is
    /// [`PalwTirIndexError::LeafMismatch`] and `out` is not to be used. Nothing outside the covering leaves is read or hashed. (Authentic
    /// only if [`Self::verify_root`] has passed for the root you hold.)
    pub fn read_authenticated(
        &self,
        program: &TirProgramV1,
        src: &dyn PalwTirRangeSourceV1,
        param: u16,
        layer: Option<u16>,
        range: Range<u64>,
        out: &mut [u8],
    ) -> Result<PalwTirAuthReadV1, PalwTirIndexError> {
        if out.len() as u64 != range.end.saturating_sub(range.start) || range.end < range.start {
            return Err(PalwTirIndexError::OutOfRange(format!("a buffer of {} bytes for the range {range:?}", out.len())));
        }
        if param as usize >= program.params.len() {
            return Err(PalwTirIndexError::OutOfRange(format!("param {param} of {}", program.params.len())));
        }
        let tensor_bytes = palw_tir_tensor_bytes_v1(program, param);
        if range.end > tensor_bytes {
            return Err(PalwTirIndexError::OutOfRange(format!("bytes {range:?} of a tensor of {tensor_bytes}")));
        }
        if range.is_empty() {
            return Ok(PalwTirAuthReadV1::default());
        }
        let missing = || PalwTirIndexError::OutOfRange(format!("param {param} at {layer:?} is not an instance of this inventory"));
        let first = self.inventory.leaf_of(param, layer, range.start).ok_or_else(missing)?;
        let last = self.inventory.leaf_of(param, layer, range.end - 1).ok_or_else(missing)?;
        let name = &program.params[param as usize].name;
        let mut scratch = vec![0u8; PALW_TIR_ROW_PIECE_BYTES_V1 as usize];
        let mut report = PalwTirAuthReadV1::default();
        for leaf in first..=last {
            let (j, l, start, len) =
                self.inventory.piece_of(leaf).ok_or_else(|| PalwTirIndexError::OutOfRange(format!("leaf {leaf}")))?;
            debug_assert_eq!((j, l), (param, layer), "the covering leaves of one instance are contiguous");
            let piece = &mut scratch[..len as usize];
            let (lo, hi) = (start as u64, start as u64 + len as u64);
            src.read_range(j, l, lo..hi, piece).map_err(PalwTirIndexError::Source)?;
            if artifact_leaf_parts_v1(name, l, start, piece) != self.leaves[leaf as usize] {
                return Err(PalwTirIndexError::LeafMismatch { leaf });
            }
            report.leaves += 1;
            report.bytes_hashed += len as u64;
            // The part of this leaf that is inside the requested range.
            let (from, to) = (lo.max(range.start), hi.min(range.end));
            if from < to {
                out[(from - range.start) as usize..(to - range.start) as usize]
                    .copy_from_slice(&piece[(from - lo) as usize..(to - lo) as usize]);
            }
        }
        Ok(report)
    }

    /// **The consensus multiproof of the leaves in `draw`**, built from the stored hashes; only those leaves' bytes are read from `src`
    /// (each is hashed and held against its stored leaf before it enters the proof). The result verifies with
    /// `verify_artifact_multiproof_v1` against the root. Replaces a pass over the whole artifact (`authenticated_openings`).
    pub fn multiproof(
        &self,
        program: &TirProgramV1,
        src: &dyn PalwTirRangeSourceV1,
        draw: &[u32],
    ) -> Result<PalwArtifactMultiproofV1, PalwTirIndexError> {
        let mut indices: Vec<u32> = draw.to_vec();
        indices.sort_unstable();
        indices.dedup();
        if indices.is_empty() {
            return Err(PalwTirIndexError::OutOfRange("an empty draw opens nothing".into()));
        }
        let mut opened: Vec<(u32, PalwArtifactOperandV1)> = Vec::with_capacity(indices.len());
        for leaf in indices {
            let (j, l, start, len) = self
                .inventory
                .piece_of(leaf)
                .ok_or_else(|| PalwTirIndexError::OutOfRange(format!("leaf {leaf} of an inventory of {}", self.leaf_count())))?;
            let mut bytes = vec![0u8; len as usize];
            src.read_range(j, l, start as u64..start as u64 + len as u64, &mut bytes).map_err(PalwTirIndexError::Source)?;
            let operand =
                PalwArtifactOperandV1 { tensor_name: program.params[j as usize].name.clone(), layer: l, row_start: start, bytes };
            if artifact_leaf_parts_v1(&operand.tensor_name, l, start, &operand.bytes) != self.leaves[leaf as usize] {
                return Err(PalwTirIndexError::LeafMismatch { leaf });
            }
            opened.push((leaf, operand));
        }
        palw_artifact_multiproof_v1(&self.leaves, &opened)
            .ok_or_else(|| PalwTirIndexError::Format("the multiproof could not be assembled from the stored leaves".into()))
    }

    /// The root recomputed by a fresh fold of the stored leaves (a streaming fold, no copy) — what [`Self::decode`] checked.
    pub fn refold(&self) -> Option<Hash64> {
        let mut frontier = PalwArtifactMerkleFrontierV1::new();
        for leaf in &self.leaves {
            frontier.push(*leaf);
        }
        frontier.root()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tir_manifest::PalwTirContainerSourceV1;
    use crate::tir_stream::{ContainerRanges, palw_tir_inventory_root_streamed_v1};
    use kaspa_consensus_core::palw_artifact::{PalwArtifactMultiproofStreamV1, verify_artifact_multiproof_v1};
    use kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_inventory_root_v1;
    use misaka_palw_base0::artifact::{Base0ArtifactV1, Base0ShapeV1, LN_THETA_10000_GEN_Q};
    use misaka_palw_base0::engine_a16::derived_a16_store;
    use misaka_palw_base0::tir_a16::convert_a16_to_tir;
    use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
    use misaka_palw_tir_artifact::PalwTirContainerV1;

    /// A converted container (the same fixture `tir_stream`'s tests use): several params, per-layer instances.
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
        let path = std::env::temp_dir().join(format!("tir-index-{tag}-{}.palwtir", std::process::id()));
        convert_a16_to_tir(&a, HISTORY_BOUND_V1_SMALL, &path, "{}".into()).expect("converted");
        path
    }

    fn open(path: &Path) -> PalwTirContainerV1 {
        PalwTirContainerV1::open(path).expect("opens")
    }

    #[test]
    fn the_built_index_folds_to_the_consensus_root_and_survives_a_round_trip_through_a_file() {
        let path = converted("roundtrip", 3, 96);
        let c = open(&path);
        let ranges = ContainerRanges::open(&c).expect("ranges");
        let index = PalwTirMerkleIndexV1::build_streamed(&c.program, &ranges).expect("built");
        let (streamed_root, count) = palw_tir_inventory_root_streamed_v1(&c.program, &ranges).expect("streamed root");
        assert_eq!((index.root(), index.leaf_count()), (streamed_root, count), "the index is the streamed pass, kept");
        assert_eq!(index.refold(), Some(index.root()));
        assert!(index.leaf_count() > 40, "the fixture has many leaves ({})", index.leaf_count());
        index.verify_root(streamed_root).expect("the root the caller holds");
        assert!(matches!(index.verify_root(Hash64::from_u64_word(1)), Err(PalwTirIndexError::RootMismatch { .. })));
        // The consensus function over whole tensors agrees.
        let (consensus_root, _) = palw_tir_inventory_root_v1(&c.program, &PalwTirContainerSourceV1(&c)).expect("consensus root");
        assert_eq!(index.root(), consensus_root);

        let file = std::env::temp_dir().join(format!("tir-index-roundtrip-{}.merkleidx", std::process::id()));
        index.write(&file).expect("written");
        let back = PalwTirMerkleIndexV1::read(&c.program, &file).expect("read");
        assert_eq!((back.root(), back.leaves()), (index.root(), index.leaves()));
        assert_eq!(back.encode(), index.encode(), "one encoding");
        // 64 bytes a leaf, plus the fixed header and trailer: the index is a small fraction of the artifact.
        assert_eq!(std::fs::metadata(&file).unwrap().len(), (HEADER_BYTES + index.leaf_count() as usize * 64 + 64) as u64);
        let _ = std::fs::remove_file(&file);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_file_that_is_not_this_artifacts_index_is_refused_by_name() {
        let path = converted("refuse-a", 3, 96);
        let other = converted("refuse-b", 2, 64);
        let (c, o) = (open(&path), open(&other));
        let index = PalwTirMerkleIndexV1::build_streamed(&c.program, &ContainerRanges::open(&c).unwrap()).unwrap();
        let bytes = index.encode();
        // Another program: its binding (or its leaf count) differs.
        assert!(PalwTirMerkleIndexV1::decode(&o.program, &bytes).is_err());
        // Truncation, an extra byte, a flipped bit anywhere (header, leaves, trailer), a wrong magic, a wrong version.
        assert!(PalwTirMerkleIndexV1::decode(&c.program, &bytes[..bytes.len() - 1]).is_err());
        let mut longer = bytes.clone();
        longer.push(0);
        assert!(PalwTirMerkleIndexV1::decode(&c.program, &longer).is_err());
        for at in [0, 9, 11, 20, 90, HEADER_BYTES + 5, bytes.len() / 2, bytes.len() - 70, bytes.len() - 1] {
            let mut bad = bytes.clone();
            bad[at] ^= 1;
            assert!(PalwTirMerkleIndexV1::decode(&c.program, &bad).is_err(), "a flipped bit at {at}");
        }
        assert_eq!(PalwTirMerkleIndexV1::decode(&c.program, &bytes).unwrap().root(), index.root());
        // A forged index: leaves of another artifact under this artifact's header still fold to ANOTHER root, which the caller's check refuses.
        let leaves: Vec<Hash64> = index.leaves().iter().map(|l| keyed(b"forged", &[l.as_byte_slice()])).collect();
        let forged = PalwTirMerkleIndexV1::from_leaves(&c.program, leaves).expect("well-formed");
        assert!(matches!(forged.verify_root(index.root()), Err(PalwTirIndexError::RootMismatch { .. })));
        assert!(PalwTirMerkleIndexV1::from_leaves(&c.program, index.leaves()[1..].to_vec()).is_err(), "a missing leaf");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&other);
    }

    #[test]
    fn an_authenticated_read_hashes_only_the_covering_leaves_and_refuses_a_changed_byte() {
        let path = converted("auth", 3, 96);
        let c = open(&path);
        let ranges = ContainerRanges::open(&c).unwrap();
        let index = PalwTirMerkleIndexV1::build_streamed(&c.program, &ranges).unwrap();
        let mut checked_multi_leaf = false;
        for e in c.header.tensors.iter() {
            let total = palw_tir_tensor_bytes_v1(&c.program, e.param);
            let whole = c.read_tensor_bytes(e.param, e.layer).unwrap();
            // Edge ranges, a middle range, a single byte, the whole tensor: bytes equal the file's, leaves counted are exactly the covering ones.
            let spans = [
                (0, total.min(7)),
                (total.saturating_sub(5), total),
                (total / 3, (total / 3 + 40_000).min(total)),
                (total / 2, total / 2 + 1),
                (0, total),
            ];
            for (a, b) in spans {
                if a >= b {
                    continue;
                }
                let mut out = vec![0u8; (b - a) as usize];
                let r = index.read_authenticated(&c.program, &ranges, e.param, e.layer, a..b, &mut out).expect("authentic");
                assert_eq!(out, &whole[a as usize..b as usize], "param {} layer {:?} {a}..{b}", e.param, e.layer);
                let first = index.inventory().leaf_of(e.param, e.layer, a).unwrap();
                let last = index.inventory().leaf_of(e.param, e.layer, b - 1).unwrap();
                assert_eq!(r.leaves, last - first + 1, "exactly the covering leaves");
                assert!(r.bytes_hashed >= b - a && r.bytes_hashed <= total, "the leaves hold the range and stay inside the tensor");
                if (b - a) > 1 && r.leaves > 1 {
                    checked_multi_leaf = true;
                }
            }
        }
        assert!(checked_multi_leaf, "some range spanned several leaves");
        // A one-byte range hashes ONE leaf, however large the artifact.
        let e = &c.header.tensors[0];
        let mut one = [0u8; 1];
        let r = index.read_authenticated(&c.program, &ranges, e.param, e.layer, 3..4, &mut one).unwrap();
        assert_eq!(r.leaves, 1);
        assert!(r.bytes_hashed <= PALW_TIR_ROW_PIECE_BYTES_V1);
        // Bad requests.
        let mut buf = [0u8; 4];
        assert!(matches!(
            index.read_authenticated(&c.program, &ranges, e.param, e.layer, 0..5, &mut buf),
            Err(PalwTirIndexError::OutOfRange(_))
        ));
        assert!(matches!(
            index.read_authenticated(&c.program, &ranges, 999, None, 0..4, &mut buf),
            Err(PalwTirIndexError::OutOfRange(_))
        ));
        let total = palw_tir_tensor_bytes_v1(&c.program, e.param);
        assert!(matches!(
            index.read_authenticated(&c.program, &ranges, e.param, e.layer, total - 2..total + 2, &mut buf),
            Err(PalwTirIndexError::OutOfRange(_))
        ));

        // A changed byte in the file: the read of its leaf (and only its leaf's range) is refused; a range elsewhere still verifies.
        let corrupt = std::env::temp_dir().join(format!("tir-index-auth-corrupt-{}.palwtir", std::process::id()));
        let mut bytes = std::fs::read(&path).unwrap();
        let (off, len) = c.locate(e.param, e.layer).unwrap();
        let victim = (off + len / 2) as usize;
        bytes[victim] ^= 0x40;
        std::fs::write(&corrupt, &bytes).unwrap();
        let cc = open(&corrupt);
        let cr = ContainerRanges::open(&cc).unwrap();
        let mid = len / 2;
        let mut out = vec![0u8; 16];
        let victim_leaf = index.inventory().leaf_of(e.param, e.layer, mid).unwrap();
        assert_eq!(
            index.read_authenticated(&cc.program, &cr, e.param, e.layer, mid..mid + 16, &mut out),
            Err(PalwTirIndexError::LeafMismatch { leaf: victim_leaf })
        );
        let mut head = vec![0u8; 8];
        let far_leaf = index.inventory().leaf_of(e.param, e.layer, 0).unwrap();
        if far_leaf != victim_leaf {
            index
                .read_authenticated(&cc.program, &cr, e.param, e.layer, 0..8, &mut head)
                .expect("a range outside the damaged leaf is unaffected");
        }
        let _ = std::fs::remove_file(&corrupt);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_multiproof_from_the_index_is_the_consensus_proof_and_reads_only_the_drawn_leaves() {
        let path = converted("proof", 3, 96);
        let c = open(&path);
        let ranges = ContainerRanges::open(&c).unwrap();
        let index = PalwTirMerkleIndexV1::build_streamed(&c.program, &ranges).unwrap();
        let n = index.leaf_count();
        let draw = [0, 1, n / 3, n / 2, n - 2, n - 1, n / 2];
        let proof = index.multiproof(&c.program, &ranges, &draw).expect("assembled");
        verify_artifact_multiproof_v1(&proof, index.root()).expect("verifies against the root");
        let mut want: Vec<u32> = draw.to_vec();
        want.sort_unstable();
        want.dedup();
        assert_eq!(proof.opened_indices(), want);
        // The same proof the consensus builder makes from the same leaves.
        let by_consensus = palw_artifact_multiproof_v1(index.leaves(), &proof.opened).expect("consensus builder");
        assert_eq!(proof, by_consensus);
        // …and the one `authenticated_openings` makes in a streamed pass over the whole artifact (O(k log n) memory).
        let mut stream = PalwArtifactMultiproofStreamV1::new(n, &want).expect("stream");
        for leaf in index.leaves() {
            stream.push(*leaf);
        }
        assert_eq!(stream.finish(&proof.opened).expect("assembled"), proof, "the indexed proof is the streamed pass's proof");
        // A source that fails anywhere but the drawn leaves is never asked: count the reads.
        struct Counting<'a> {
            inner: &'a ContainerRanges<'a>,
            reads: std::cell::Cell<u32>,
        }
        impl PalwTirRangeSourceV1 for Counting<'_> {
            fn read_range(&self, param: u16, layer: Option<u16>, range: Range<u64>, out: &mut [u8]) -> Result<(), String> {
                self.reads.set(self.reads.get() + 1);
                self.inner.read_range(param, layer, range, out)
            }
        }
        let counting = Counting { inner: &ranges, reads: std::cell::Cell::new(0) };
        index.multiproof(&c.program, &counting, &draw).unwrap();
        assert_eq!(counting.reads.get() as usize, want.len(), "one read per drawn leaf, not a pass over {n} leaves");
        // An empty draw and an out-of-range leaf are refused.
        assert!(index.multiproof(&c.program, &ranges, &[]).is_err());
        assert!(matches!(index.multiproof(&c.program, &ranges, &[n]), Err(PalwTirIndexError::OutOfRange(_))));
        // A file that is not the indexed artifact cannot be opened against the index.
        let corrupt = std::env::temp_dir().join(format!("tir-index-proof-corrupt-{}.palwtir", std::process::id()));
        let mut bytes = std::fs::read(&path).unwrap();
        let (off, _) = c.locate(c.header.tensors[0].param, c.header.tensors[0].layer).unwrap();
        bytes[off as usize] ^= 1;
        std::fs::write(&corrupt, &bytes).unwrap();
        let cc = open(&corrupt);
        let first = index.inventory().leaf_of(c.header.tensors[0].param, c.header.tensors[0].layer, 0).unwrap();
        assert_eq!(
            index.multiproof(&cc.program, &ContainerRanges::open(&cc).unwrap(), &[first]),
            Err(PalwTirIndexError::LeafMismatch { leaf: first })
        );
        let _ = std::fs::remove_file(&corrupt);
        let _ = std::fs::remove_file(&path);
    }
}
