//! **The TIR inventory, held as a tree** (RFC-0002 Phase F, F3's layout; design §2.10): every level
//! of the artifact tree over an IR class's params, so a node opens any inventory leaf — what an IR
//! court close carries for every param element its cone reads — with one path walk instead of a
//! pass over the artifact.
//!
//! The leaves are F3's, in F3's order (`PalwTirInventoryIndexV1` is the consensus closed form of
//! both directions), each the ordinary artifact leaf over `(ParamDecl.name, layer, byte offset,
//! bytes)`; the tree is the ordinary artifact tree (`artifact_node_v1`, an odd last node promoted).
//! Nothing here is a second spelling: the tests hold the root to `palw_tir_inventory_root_v1` and
//! every opening to `palw_tir_open_leaf_v1` and `verify_artifact_opening_v1`.
//!
//! Memory: one hash per node, about two per leaf — ≈ 120 MiB for a 1.5B-parameter class's ≈ 10^6
//! leaves; the bytes are never copied (an opening reads its piece from the source).

use std::borrow::Cow;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_artifact::{
    PalwArtifactMultiproofV1, PalwArtifactOpeningV1, PalwArtifactOperandV1, artifact_leaf_parts_v1, artifact_node_v1,
};
use kaspa_consensus_core::palw_tir_artifact_v1::{PalwTirTensorSourceV1, palw_tir_tensor_bytes_v1};
use kaspa_consensus_core::palw_tir_court_v1::PalwTirInventoryIndexV1;
use misaka_palw_tir::TirProgramV1;

use crate::params::TirParams;

/// Every level of an IR class's inventory tree, leaves first.
pub struct TirInventoryTreeV1 {
    index: PalwTirInventoryIndexV1,
    levels: Vec<Vec<Hash64>>,
}

impl TirInventoryTreeV1 {
    /// **The tree over leaf hashes already computed** (a residency's streamed pass), in inventory
    /// order: refused unless they are exactly the program's leaf count.
    pub fn from_leaves(program: &TirProgramV1, leaves: Vec<Hash64>) -> Result<Self, String> {
        let index = PalwTirInventoryIndexV1::new(program).ok_or("the TIR inventory refuses this program")?;
        if leaves.is_empty() || leaves.len() as u64 != index.leaf_count() as u64 {
            return Err(format!("{} leaf hashes for an inventory of {}", leaves.len(), index.leaf_count()));
        }
        Ok(Self { index, levels: Self::levels(leaves) })
    }

    fn levels(leaves: Vec<Hash64>) -> Vec<Vec<Hash64>> {
        let mut levels = vec![leaves];
        while levels.last().expect("non-empty").len() > 1 {
            let level = levels.last().expect("non-empty");
            let mut next = Vec::with_capacity(level.len().div_ceil(2));
            let mut pairs = level.chunks_exact(2);
            for pair in &mut pairs {
                next.push(artifact_node_v1(&pair[0], &pair[1]));
            }
            if let [odd] = pairs.remainder() {
                next.push(*odd);
            }
            levels.push(next);
        }
        levels
    }

    /// Hash every leaf of `program`'s inventory from `src` (one instance's bytes resident at a
    /// time) and fold the levels.
    pub fn build(program: &TirProgramV1, src: &dyn PalwTirTensorSourceV1) -> Result<Self, String> {
        let index = PalwTirInventoryIndexV1::new(program).ok_or("the TIR inventory refuses this program")?;
        let n = index.leaf_count();
        if n == 0 {
            return Err("the program declares no param, so its inventory has no leaf and no root".into());
        }
        let mut leaves = Vec::with_capacity(n as usize);
        let mut current: Option<((u16, Option<u16>), Cow<'_, [u8]>)> = None;
        for leaf in 0..n {
            let (j, layer, start, len) = index.piece_of(leaf).ok_or_else(|| format!("inventory leaf {leaf} has no coordinates"))?;
            if current.as_ref().is_none_or(|(key, _)| *key != (j, layer)) {
                let bytes = src.tensor_bytes(j, layer).ok_or_else(|| format!("no tensor for param {j} at {layer:?}"))?;
                let want = palw_tir_tensor_bytes_v1(program, j);
                if bytes.len() as u64 != want {
                    return Err(format!("param {j} at {layer:?}: {} bytes, the declaration needs {want}", bytes.len()));
                }
                current = Some(((j, layer), bytes));
            }
            let (_, bytes) = current.as_ref().expect("set above");
            let piece = &bytes[start as usize..start as usize + len as usize];
            leaves.push(artifact_leaf_parts_v1(&program.params[j as usize].name, layer, start, piece));
        }
        Ok(Self { index, levels: Self::levels(leaves) })
    }

    /// The inventory root — the class's `artifact_root`.
    pub fn root(&self) -> Hash64 {
        self.levels.last().expect("non-empty")[0]
    }

    pub fn leaf_count(&self) -> u32 {
        self.index.leaf_count()
    }

    /// Every leaf hash, in inventory order.
    pub fn leaves(&self) -> &[Hash64] {
        &self.levels[0]
    }

    pub fn index(&self) -> &PalwTirInventoryIndexV1 {
        &self.index
    }

    /// **Open inventory leaf `leaf`**: its operand (the piece read from `src`) and its sibling path,
    /// in `verify_artifact_opening_v1`'s form. `None` for a leaf outside the inventory or a source
    /// that does not hold the piece.
    pub fn open(&self, program: &TirProgramV1, src: &dyn PalwTirTensorSourceV1, leaf: u32) -> Option<PalwArtifactOpeningV1> {
        let (j, layer, start, len) = self.index.piece_of(leaf)?;
        let bytes = src.tensor_bytes(j, layer)?;
        let piece = bytes.get(start as usize..start as usize + len as usize)?.to_vec();
        self.opening_of(program, leaf, j, layer, start, piece)
    }

    /// [`Self::open`], reading only the leaf's piece from `src` — what a residency serves, through
    /// the file descriptor, without the instance around it.
    pub fn open_with(&self, program: &TirProgramV1, src: &dyn TirByteSourceV1, leaf: u32) -> Option<PalwArtifactOpeningV1> {
        let (j, layer, start, len) = self.index.piece_of(leaf)?;
        let mut piece = vec![0u8; len as usize];
        src.read_bytes(j, layer, start as u64, &mut piece).ok()?;
        self.opening_of(program, leaf, j, layer, start, piece)
    }

    fn opening_of(
        &self,
        program: &TirProgramV1,
        leaf: u32,
        j: u16,
        layer: Option<u16>,
        start: u32,
        piece: Vec<u8>,
    ) -> Option<PalwArtifactOpeningV1> {
        let operand =
            PalwArtifactOperandV1 { tensor_name: program.params[j as usize].name.clone(), layer, row_start: start, bytes: piece };
        let mut path = Vec::new();
        let mut at = leaf as usize;
        for level in &self.levels[..self.levels.len() - 1] {
            let promoted = at == level.len() - 1 && level.len() % 2 == 1;
            if !promoted {
                path.push(level[at ^ 1]);
            }
            at /= 2;
        }
        Some(PalwArtifactOpeningV1 { operand, leaf_index: leaf, leaf_count: self.leaf_count(), path })
    }

    /// **The operands of `leaves`** — each leaf's piece read from `src`, in ascending leaf order (what
    /// a multiproof or a composite carriage opens). `None` for an empty set, a repeated leaf, a leaf
    /// outside the inventory, or a source that does not hold a piece.
    pub fn operands(
        &self,
        program: &TirProgramV1,
        src: &dyn PalwTirTensorSourceV1,
        leaves: &[u32],
    ) -> Option<Vec<(u32, PalwArtifactOperandV1)>> {
        let mut sorted = leaves.to_vec();
        sorted.sort_unstable();
        if sorted.is_empty() || sorted.windows(2).any(|w| w[0] == w[1]) || *sorted.last()? >= self.leaf_count() {
            return None;
        }
        let mut opened = Vec::with_capacity(sorted.len());
        let mut current: Option<((u16, Option<u16>), Cow<'_, [u8]>)> = None;
        for &leaf in &sorted {
            let (j, layer, start, len) = self.index.piece_of(leaf)?;
            if current.as_ref().is_none_or(|(key, _)| *key != (j, layer)) {
                current = Some(((j, layer), src.tensor_bytes(j, layer)?));
            }
            let (_, bytes) = current.as_ref()?;
            let piece = bytes.get(start as usize..start as usize + len as usize)?.to_vec();
            let operand =
                PalwArtifactOperandV1 { tensor_name: program.params[j as usize].name.clone(), layer, row_start: start, bytes: piece };
            opened.push((leaf, operand));
        }
        Some(opened)
    }

    /// [`Self::operands`], reading only each leaf's piece from `src` (a residency's reads).
    pub fn operands_with(
        &self,
        program: &TirProgramV1,
        src: &dyn TirByteSourceV1,
        leaves: &[u32],
    ) -> Option<Vec<(u32, PalwArtifactOperandV1)>> {
        let mut sorted = leaves.to_vec();
        sorted.sort_unstable();
        if sorted.is_empty() || sorted.windows(2).any(|w| w[0] == w[1]) || *sorted.last()? >= self.leaf_count() {
            return None;
        }
        let mut opened = Vec::with_capacity(sorted.len());
        for &leaf in &sorted {
            let (j, layer, start, len) = self.index.piece_of(leaf)?;
            let mut piece = vec![0u8; len as usize];
            src.read_bytes(j, layer, start as u64, &mut piece).ok()?;
            let operand =
                PalwArtifactOperandV1 { tensor_name: program.params[j as usize].name.clone(), layer, row_start: start, bytes: piece };
            opened.push((leaf, operand));
        }
        Some(opened)
    }

    /// [`Self::multiproof`], reading only each leaf's piece from `src` (a residency's reads).
    pub fn multiproof_with(
        &self,
        program: &TirProgramV1,
        src: &dyn TirByteSourceV1,
        leaves: &[u32],
    ) -> Option<PalwArtifactMultiproofV1> {
        let opened = self.operands_with(program, src, leaves)?;
        Some(self.multiproof_of(opened))
    }

    /// **One multiproof over `leaves`** (RFC-0002 Phase F §2.12.1: an IR close carries its parameter
    /// openings as one `PalwArtifactMultiproofV1`): the operands read from `src` in ascending leaf
    /// order, and exactly the siblings `palw_artifact_multiproof_v1` supplies — read off this tree's
    /// levels instead of folding the inventory again, so a run of leaves costs its two boundary paths.
    /// `None` for an empty set, a repeated leaf, a leaf outside the inventory, or a source that does not
    /// hold a piece.
    pub fn multiproof(
        &self,
        program: &TirProgramV1,
        src: &dyn PalwTirTensorSourceV1,
        leaves: &[u32],
    ) -> Option<PalwArtifactMultiproofV1> {
        let opened = self.operands(program, src, leaves)?;
        Some(self.multiproof_of(opened))
    }

    fn multiproof_of(&self, opened: Vec<(u32, PalwArtifactOperandV1)>) -> PalwArtifactMultiproofV1 {
        let sorted: Vec<u32> = opened.iter().map(|(leaf, _)| *leaf).collect();
        // The builder's walk: level by level, a known node whose partner is not known supplies it.
        let mut known: Vec<u64> = sorted.iter().map(|&i| i as u64).collect();
        let mut siblings = Vec::new();
        for level in &self.levels[..self.levels.len() - 1] {
            let width = level.len() as u64;
            let mut next = Vec::with_capacity(known.len());
            let mut i = 0;
            while i < known.len() {
                let index = known[i];
                if index == width - 1 && width % 2 == 1 {
                    next.push(index / 2); // an odd last node is promoted
                    i += 1;
                    continue;
                }
                let partner = index ^ 1;
                if known.get(i + 1).copied() == Some(partner) {
                    i += 2;
                } else {
                    siblings.push(level[partner as usize]);
                    i += 1;
                }
                next.push(index / 2);
            }
            next.dedup();
            known = next;
        }
        PalwArtifactMultiproofV1 { leaf_count: self.leaf_count(), opened, siblings }
    }
}

/// **Reads bytes of param instances** — a piece for an opening, a chunk for a pass — without asking
/// for a whole instance: a residency reads exactly the bytes asked for, through the file descriptor
/// (`super::residency`).
pub trait TirByteSourceV1 {
    /// `buf.len()` bytes of instance `(param, layer)` from byte `at` of it.
    fn read_bytes(&self, param: u16, layer: Option<u16>, at: u64, buf: &mut [u8]) -> Result<(), String>;
}

/// **Any tensor source, a piece at a time** — each read cut out of the whole instance it serves.
pub struct TirWholeInstancesV1<'s>(pub &'s dyn PalwTirTensorSourceV1);

impl TirByteSourceV1 for TirWholeInstancesV1<'_> {
    fn read_bytes(&self, param: u16, layer: Option<u16>, at: u64, buf: &mut [u8]) -> Result<(), String> {
        let bytes = self.0.tensor_bytes(param, layer).ok_or_else(|| format!("no tensor for param {param} at {layer:?}"))?;
        let piece = usize::try_from(at)
            .ok()
            .and_then(|at| bytes.get(at..at.checked_add(buf.len())?))
            .ok_or_else(|| format!("param {param} at {layer:?}: bytes {at}..+{} leave the instance", buf.len()))?;
        buf.copy_from_slice(piece);
        Ok(())
    }
}

/// **In-memory params as an inventory source**: each instance's elements as the little-endian
/// bytes of its dtype (converted per call — for tests and small classes; a mapped artifact serves
/// its bytes in place).
pub struct TirParamsSourceV1<'p, 'a>(pub &'p TirParams<'a>);

impl PalwTirTensorSourceV1 for TirParamsSourceV1<'_, '_> {
    fn tensor_bytes(&self, param: u16, layer: Option<u16>) -> Option<Cow<'_, [u8]>> {
        self.0.get(param, layer).map(|s| Cow::Owned(s.to_le_bytes()))
    }
}

/// **Whatever serves inventory openings for a class** — a mapped artifact, or a tree over params
/// in memory.
pub trait TirParamOpenerV1 {
    fn param_opening(&self, leaf: u32) -> Option<PalwArtifactOpeningV1>;
    /// One multiproof over `leaves` ([`TirInventoryTreeV1::multiproof`]).
    fn param_multiproof(&self, leaves: &[u32]) -> Option<PalwArtifactMultiproofV1>;
    /// **The carriage of `leaves` as a close carries it** (the court store's `param_carriage`): one
    /// multiproof against the class's root, unless the class is a composite candidate (RFC-0004 §6.3),
    /// which opens under its two sub-roots.
    fn param_carriage(&self, leaves: &[u32]) -> Option<kaspa_consensus_core::palw_tir_court_v1::PalwTirParamOpeningV1> {
        self.param_multiproof(leaves).map(kaspa_consensus_core::palw_tir_court_v1::PalwTirParamOpeningV1::Single)
    }
}

/// A tree and the source it was built from.
pub struct TirHeldInventoryV1<'s> {
    pub program: &'s TirProgramV1,
    pub src: &'s dyn PalwTirTensorSourceV1,
    pub tree: TirInventoryTreeV1,
}

impl<'s> TirHeldInventoryV1<'s> {
    pub fn build(program: &'s TirProgramV1, src: &'s dyn PalwTirTensorSourceV1) -> Result<Self, String> {
        Ok(Self { program, src, tree: TirInventoryTreeV1::build(program, src)? })
    }
}

impl TirParamOpenerV1 for TirHeldInventoryV1<'_> {
    fn param_opening(&self, leaf: u32) -> Option<PalwArtifactOpeningV1> {
        self.tree.open(self.program, self.src, leaf)
    }

    fn param_multiproof(&self, leaves: &[u32]) -> Option<PalwArtifactMultiproofV1> {
        self.tree.multiproof(self.program, self.src, leaves)
    }
}
