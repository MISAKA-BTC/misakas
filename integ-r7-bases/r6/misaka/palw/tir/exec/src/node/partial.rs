//! **Partial-read adjudication: the artifact's byte ranges a court leaf list names, nothing else.**
//!
//! Adjudication used to expand model operands from the whole `.palwtir` (every instance, whole,
//! into the executor's dense tensors). This path reads only the inventory pieces a demand-closure
//! leaf list names: each piece is seek+read from the container file at the instance's own offset
//! ([`PalwTirContainerV1::locate`]), and it is checked against the class's inventory root through the
//! existing per-leaf opening ([`TirInventoryTreeV1::open_with`] + `verify_artifact_opening_v1`). No
//! new trust assumption: the openings are the ones the chain's root already commits to.
//!
//! What this module does NOT do: it does not feed the evaluator. The executor's `TirParams` holds a
//! dense instance per param; a partial operand has no evaluator path yet (a `Gather` over a param is
//! served by rows only when a residency serves the instance). So the step-result identity between this
//! path and the full evaluator is NOT established here; the tests say so.
//!
//! Accounting ([`TirPartialCountsV1`]) keeps three byte classes apart: **model operand bytes** (the
//! piece bytes), **auth-path bytes** (the sibling hashes of each opening) and nothing else. Step and
//! history bytes are the closure's, counted by the caller.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

use kaspa_consensus_core::palw_artifact::{PalwArtifactOpeningV1, verify_artifact_opening_v1};
use kaspa_consensus_core::Hash64;

use super::artifact::TirArtifactV1;
use super::inventory::{TirByteSourceV1, TirInventoryTreeV1};

/// Byte size of one sibling hash in an opening's path (`Hash64`).
pub const TIR_PARTIAL_HASH_BYTES_V1: u64 = 64;

/// The counts of one partial reader. `*_before_dedup` counts every request; the `read_*` fields count
/// what the file was actually asked for (a leaf read once per reader, then served from the cache).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TirPartialCountsV1 {
    /// Piece bytes requested, summed over every request (before dedup).
    pub model_operand_bytes_before_dedup: u64,
    /// Piece bytes read from the file (after dedup): each leaf once.
    pub model_operand_bytes_read: u64,
    /// Sibling-hash bytes of the openings verified, before dedup (one path per request).
    pub auth_path_bytes_before_dedup: u64,
    /// Distinct leaves verified and read.
    pub leaves_read: u64,
    /// `read_exact` calls issued to the file.
    pub file_reads: u64,
}

/// A byte source over the artifact's container file: `read_bytes` seeks to the instance's offset.
pub struct TirPartialFileV1<'a> {
    art: &'a TirArtifactV1,
    file: RefCell<File>,
    counts: RefCell<TirPartialCountsV1>,
}

impl TirByteSourceV1 for TirPartialFileV1<'_> {
    fn read_bytes(&self, param: u16, layer: Option<u16>, at: u64, buf: &mut [u8]) -> Result<(), String> {
        let (off, len) = self.art.container().locate(param, layer).ok_or_else(|| format!("no tensor for param {param} at {layer:?}"))?;
        let end = at.checked_add(buf.len() as u64).ok_or("a read past the end of the instance")?;
        if end > len {
            return Err(format!("param {param} at {layer:?}: bytes {at}..{end} leave an instance of {len}"));
        }
        let mut f = self.file.borrow_mut();
        f.seek(SeekFrom::Start(off + at)).map_err(|e| e.to_string())?;
        f.read_exact(buf).map_err(|e| e.to_string())?;
        self.counts.borrow_mut().file_reads += 1;
        Ok(())
    }
}

/// A partial reader for one artifact: holds the file, the verified-piece cache (one entry per leaf)
/// and the counts.
pub struct TirPartialReaderV1<'a> {
    art: &'a TirArtifactV1,
    tree: &'a TirInventoryTreeV1,
    root: Hash64,
    src: TirPartialFileV1<'a>,
    /// Verified pieces, by inventory leaf (the dedup cache).
    verified: RefCell<BTreeMap<u32, Vec<u8>>>,
}

impl<'a> TirPartialReaderV1<'a> {
    /// A reader over `art`'s own container file. The inventory tree and root are the artifact's
    /// (its commitment; built once by [`TirArtifactV1::inventory_tree`], not per read).
    pub fn new(art: &'a TirArtifactV1) -> Result<Self, String> {
        let file = File::open(&art.container().path).map_err(|e| e.to_string())?;
        let (root, _) = art.inventory_root()?;
        let tree = art.inventory_tree()?;
        Ok(Self {
            art,
            tree,
            root,
            src: TirPartialFileV1 { art, file: RefCell::new(file), counts: RefCell::new(TirPartialCountsV1::default()) },
            verified: RefCell::new(BTreeMap::new()),
        })
    }

    pub fn counts(&self) -> TirPartialCountsV1 {
        let mut c = *self.src.counts.borrow();
        c.leaves_read = self.verified.borrow().len() as u64;
        c.model_operand_bytes_read = self.verified.borrow().values().map(|v| v.len() as u64).sum();
        c
    }

    /// **The verified pieces of `leaves`**, in the order given. Each leaf is read from the file at its
    /// instance's offset, its opening (piece + sibling path) is built from the tree, and the opening is
    /// verified against the artifact's inventory root before the piece is used. A leaf already verified
    /// in this reader is served from the cache (counted before dedup, not read again).
    pub fn read_leaves(&self, leaves: &[u32]) -> Result<Vec<(u32, Vec<u8>)>, String> {
        let program = &self.art.container().program;
        let mut out = Vec::with_capacity(leaves.len());
        for &leaf in leaves {
            let (_, _, _, len) = self.tree.index().piece_of(leaf).ok_or_else(|| format!("leaf {leaf} is outside the inventory"))?;
            {
                let mut c = self.src.counts.borrow_mut();
                c.model_operand_bytes_before_dedup += len as u64;
            }
            if let Some(piece) = self.verified.borrow().get(&leaf) {
                out.push((leaf, piece.clone()));
                // The path of a cached leaf is still a request's bytes: count it, do not re-verify.
                let depth = self.path_len(leaf);
                self.src.counts.borrow_mut().auth_path_bytes_before_dedup += depth * TIR_PARTIAL_HASH_BYTES_V1;
                continue;
            }
            let opening: PalwArtifactOpeningV1 =
                self.tree.open_with(program, &self.src, leaf).ok_or_else(|| format!("leaf {leaf}: the opening is not available"))?;
            verify_artifact_opening_v1(&opening, self.root).map_err(|e| format!("leaf {leaf}: {e:?}"))?;
            self.src.counts.borrow_mut().auth_path_bytes_before_dedup += opening.path.len() as u64 * TIR_PARTIAL_HASH_BYTES_V1;
            let piece = opening.operand.bytes.clone();
            self.verified.borrow_mut().insert(leaf, piece.clone());
            out.push((leaf, piece));
        }
        Ok(out)
    }

    /// Sibling count of leaf `leaf`'s opening (the tree's levels, an odd last node promoted).
    fn path_len(&self, leaf: u32) -> u64 {
        self.tree.path_len_v1(leaf) as u64
    }
}
