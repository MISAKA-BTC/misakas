//! **Per-position DA for segmented claims (K2-TIR-v4)** — `docs/design/palw/k2-real-scale.md` §4.
//!
//! A demand on a segmented claim names a POSITION and asks for **everything committed at it** — every value, derived windows included,
//! each authenticated through its node commitment and the position root against the claim's on-chain segment root. The material is
//! laid out deterministically from the program's shapes at `H(p)` ([`position_parts_v1`]) and served in parts of at most
//! [`SEG_PART_BYTES_V4`] ([`SegPartResponseV1`]): a value that fits is served whole, a larger one in contiguous row-leaf ranges, each with
//! its Merkle range proof ([`crate::merkle3::RowRangeV3`]). A part either authenticates completely or is classified as the existing
//! response classes (`malformed`, `wrong_bytes`, `wrong_root`, `fake_opening`); a position some part of which is still missing at the
//! demand's deadline is the producer's availability default. Nothing served is kept in the ledger: the bytes stay in the blocks.

use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::program::TirProgramV1;
use misaka_palw_tir::{DType, Tensor};

use crate::hash::Digest;
use crate::merkle::AXIS_ROW;
use crate::merkle3::{LayoutV3, RowRangeV3, TILE_V3, TreesV3, tensor_commitment_v3};
use crate::seg::{NodeOpeningV1, SEG_LEN_V4, position_in_segment, position_node_opening_v1};
use crate::trace::WiringV1;

/// The most bytes one served part may hold (its borsh encoding).
pub const SEG_PART_BYTES_V4: u64 = 1 << 20;
/// A part's fixed fields (its index, the position root and its path), bounded.
const PART_HEADER_BOUND_V4: u64 = 2048;
/// A demander's open position sessions per claim: no set of other bonds can refuse an honest prosecutor its two (G14 criterion 6).
pub const SEG_OPEN_PER_DEMANDER_V4: u32 = 4;
/// The most parts one position may have (a bitmap bound).
pub const SEG_MAX_PARTS_V4: u32 = 1 << 20;

/// One chunk of a part, as the layout names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum ChunkSpecV1 {
    Whole {
        occurrence: u16,
        node: u16,
    },
    Rows {
        occurrence: u16,
        node: u16,
        first_leaf: u64,
        leaves: u64,
    },
    /// A withheld value (`crate::seg_scope`, DA16b option (c)): its node opening alone, never an element.
    Withheld {
        occurrence: u16,
        node: u16,
    },
}

impl ChunkSpecV1 {
    pub fn at(&self) -> (u16, u16) {
        match *self {
            Self::Whole { occurrence, node } | Self::Rows { occurrence, node, .. } | Self::Withheld { occurrence, node } => {
                (occurrence, node)
            }
        }
    }
}

/// An upper bound on a chunk's encoding beyond its element bytes: the node opening's commitment and path, the header, the range proof
/// and the column root.
fn chunk_overhead(node_count: u64, row_leaves: u64, rank: usize) -> u64 {
    let depth = |n: u64| crate::merkle::depth(n) + 1;
    16 + 64 + 4 + 64 * depth(node_count) + 4 + 8 * rank as u64 + 1 + 4 + 16 + 4 + 64 * 2 * depth(row_leaves) + 64 + 64
}

/// The declared values of position `p`: `(occurrence, node, dtype, shape, withheld)` in canonical order.
fn declared_values(w: &WiringV1<'_>, p: u32) -> Vec<(u16, u16, DType, Vec<usize>, bool)> {
    let withheld = crate::seg_scope::seg_withheld_mask_v1(w.program);
    let mut out = Vec::new();
    for s in 0..w.occurrences.len() as u16 {
        let b = w.occurrences[s as usize].0 as usize;
        for n in 0..w.program.blocks[b].nodes.len() as u16 {
            let node = w.node(s, n);
            out.push((s, n, node.out.dtype, node.out.resolve(w.h(s, p)), withheld[s as usize][n as usize]));
        }
    }
    out
}

/// **The parts position `p`'s material is served in** — a pure function of the program and `p` (the shapes at `H(p)`; a withheld
/// value is its node opening alone).
pub fn position_parts_v1(program: &TirProgramV1, p: u32) -> Result<Vec<Vec<ChunkSpecV1>>, String> {
    let w = WiringV1::new(program).map_err(|e| e.to_string())?;
    layout_parts(&declared_values(&w, p))
}

/// The bytes of row leaf `j` of a value of layout `l` at `width` bytes an element (every leaf of a row is a full tile but the row's last).
fn row_leaf_bytes(l: &LayoutV3, width: u64, j: u64) -> u64 {
    let tile = j % l.row_tiles;
    (((tile + 1) * TILE_V3).min(l.row_len) - tile * TILE_V3) * width
}

/// **How many row leaves from leaf `from` fit `cap` bytes, and their bytes**: the greedy count, leaf by leaf to the end of the current
/// row, then whole rows at once, then leaf by leaf again. The same answer as summing every leaf, without visiting them (a 9B-8k position
/// has 10^5–10^6 leaves, and the layout runs at every demand and at every served part).
fn row_leaves_fitting(l: &LayoutV3, width: u64, from: u64, total: u64, cap: u64) -> (u64, u64) {
    let (mut i, mut left) = (from, cap);
    while i < total && i % l.row_tiles != 0 {
        let b = row_leaf_bytes(l, width, i);
        if b > left {
            return (i - from, cap - left);
        }
        left -= b;
        i += 1;
    }
    let row_bytes = l.row_len * width;
    if row_bytes > 0 {
        let whole = (left / row_bytes).min((total - i) / l.row_tiles);
        i += whole * l.row_tiles;
        left -= whole * row_bytes;
    }
    while i < total {
        let b = row_leaf_bytes(l, width, i);
        if b > left {
            break;
        }
        left -= b;
        i += 1;
    }
    (i - from, cap - left)
}

/// **The deterministic greedy layout of a position's values into parts**: a value that fits the current part joins it, one that fits an
/// empty part starts one, and a larger one is cut into contiguous row-leaf ranges, each as long as the part it lands in allows.
fn layout_parts(values: &[(u16, u16, DType, Vec<usize>, bool)]) -> Result<Vec<Vec<ChunkSpecV1>>, String> {
    let node_count = values.len() as u64;
    let budget = SEG_PART_BYTES_V4 - PART_HEADER_BOUND_V4;
    let mut parts: Vec<Vec<ChunkSpecV1>> = vec![Vec::new()];
    let mut room = budget;
    for (s, n, dtype, shape, withheld) in values {
        let (s, n) = (*s, *n);
        if *withheld {
            let oh = chunk_overhead(node_count, 1, 0);
            if oh > room {
                parts.push(Vec::new());
                room = budget;
            }
            parts.last_mut().expect("a part").push(ChunkSpecV1::Withheld { occurrence: s, node: n });
            room -= oh;
            continue;
        }
        let l = LayoutV3::of(shape);
        let width = dtype.width() as u64;
        let bytes = l.len * width;
        let oh = chunk_overhead(node_count, l.leaves(AXIS_ROW), shape.len());
        if bytes + oh <= room {
            parts.last_mut().expect("a part").push(ChunkSpecV1::Whole { occurrence: s, node: n });
            room -= bytes + oh;
            continue;
        }
        if bytes + oh <= budget {
            parts.push(vec![ChunkSpecV1::Whole { occurrence: s, node: n }]);
            room = budget - bytes - oh;
            continue;
        }
        // Larger than a part: contiguous row-leaf ranges.
        let total = l.leaves(AXIS_ROW);
        let mut i = 0u64;
        while i < total {
            let (k, leaf_bytes) = row_leaves_fitting(&l, width, i, total, room.saturating_sub(oh));
            if k == 0 {
                if room == budget {
                    return Err(format!("({s}, {n}): one row leaf and its overhead exceed a part"));
                }
                parts.push(Vec::new());
                room = budget;
                continue;
            }
            let used = oh + leaf_bytes;
            parts.last_mut().expect("a part").push(ChunkSpecV1::Rows { occurrence: s, node: n, first_leaf: i, leaves: k });
            room -= used;
            i += k;
        }
    }
    if parts.len() as u64 > SEG_MAX_PARTS_V4 as u64 {
        return Err(format!("{} parts a position: past the bound", parts.len()));
    }
    Ok(parts)
}

/// What a chunk carries.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum ChunkBodyV1 {
    /// The whole value, at its dtype's width (both roots recomputed from it).
    Whole(Vec<u8>) = 0,
    /// A contiguous range of its row leaves with the range proof and the column root.
    Rows(RowRangeV3) = 1,
    /// A withheld value: nothing but the chunk's node opening (`crate::seg_scope`).
    Withheld = 2,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ChunkV1 {
    pub occurrence: u16,
    pub node: u16,
    pub commitment: Digest,
    pub node_siblings: Vec<Digest>,
    pub body: ChunkBodyV1,
}

/// **One served part of one position.**
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SegPartResponseV1 {
    pub part: u32,
    pub position_root: Digest,
    pub position_siblings: Vec<Digest>,
    pub chunks: Vec<ChunkV1>,
}

/// **The producer's part `part` of position `p`**, from the position's values and the position root's path to its segment root.
pub fn position_part_v1(
    program: &TirProgramV1,
    p: u32,
    values: &[Vec<Tensor>],
    position_siblings: Vec<Digest>,
    part: u32,
) -> Result<SegPartResponseV1, String> {
    let parts = position_parts_v1(program, p)?;
    let specs = parts.get(part as usize).ok_or("no such part")?;
    let commitments: Vec<Vec<Digest>> = values.iter().map(|occ| occ.iter().map(tensor_commitment_v3).collect()).collect();
    let mut chunks = Vec::with_capacity(specs.len());
    let mut position_root = [0u8; 64];
    for spec in specs {
        let (s, n) = spec.at();
        let opening: NodeOpeningV1 =
            position_node_opening_v1(p, &commitments, position_siblings.clone(), s, n).ok_or("a value the position does not have")?;
        position_root = opening.position_root;
        let t = &values[s as usize][n as usize];
        let body = match *spec {
            ChunkSpecV1::Whole { .. } => ChunkBodyV1::Whole(t.to_le_bytes()),
            ChunkSpecV1::Rows { first_leaf, leaves, .. } => {
                ChunkBodyV1::Rows(RowRangeV3::of(t, &TreesV3::of(t), first_leaf, leaves).ok_or("a row range outside the value")?)
            }
            ChunkSpecV1::Withheld { .. } => ChunkBodyV1::Withheld,
        };
        chunks.push(ChunkV1 { occurrence: s, node: n, commitment: opening.commitment, node_siblings: opening.node_siblings, body });
    }
    if specs.is_empty() {
        position_root = crate::seg::position_root_of_v1(p, &commitments);
    }
    Ok(SegPartResponseV1 { part, position_root, position_siblings, chunks })
}

/// The flat index of `(s, n)` among a position's values and the count, from the wiring.
fn node_index_of(w: &WiringV1<'_>, s: u16, n: u16) -> Option<(u64, u64)> {
    let mut at = 0u64;
    let mut index = None;
    for occ in 0..w.occurrences.len() as u16 {
        let b = w.occurrences[occ as usize].0 as usize;
        let count = w.program.blocks[b].nodes.len() as u64;
        if occ == s && (n as u64) < count {
            index = Some(at + n as u64);
        }
        at += count;
    }
    index.map(|i| (i, at))
}

/// **Classify one served part** of position `p` against the claim's segment roots: its part index, or the class of its failure.
pub fn classify_part_v1(
    program: &TirProgramV1,
    segment_roots: &[Digest],
    positions: u32,
    p: u32,
    bytes: &[u8],
) -> Result<u32, &'static str> {
    if bytes.len() as u64 > SEG_PART_BYTES_V4 {
        return Err("oversized");
    }
    let r: SegPartResponseV1 = borsh::from_slice(bytes).map_err(|_| "malformed")?;
    let w = WiringV1::new(program).map_err(|_| "malformed")?;
    let parts = position_parts_v1(program, p).map_err(|_| "malformed")?;
    let specs = parts.get(r.part as usize).ok_or("malformed")?;
    if specs.len() != r.chunks.len() {
        return Err("malformed");
    }
    if !position_in_segment(p, &r.position_root, &r.position_siblings, segment_roots, positions) {
        return Err("wrong_root");
    }
    for (spec, chunk) in specs.iter().zip(&r.chunks) {
        let (s, n) = spec.at();
        if (chunk.occurrence, chunk.node) != (s, n) {
            return Err("malformed");
        }
        let (index, count) = node_index_of(&w, s, n).ok_or("malformed")?;
        let opening = NodeOpeningV1 {
            position: p,
            occurrence: s,
            node: n,
            commitment: chunk.commitment,
            node_siblings: chunk.node_siblings.clone(),
            position_root: r.position_root,
            position_siblings: r.position_siblings.clone(),
        };
        if !opening.authenticates(segment_roots, positions, index, count) {
            return Err("fake_opening");
        }
        let node = w.node(s, n);
        let (dtype, shape) = (node.out.dtype, node.out.resolve(w.h(s, p)));
        match (spec, &chunk.body) {
            (ChunkSpecV1::Whole { .. }, ChunkBodyV1::Whole(bytes)) => match Tensor::from_le_bytes(dtype, &shape, bytes) {
                Ok(t) if tensor_commitment_v3(&t) == chunk.commitment => {}
                Ok(_) => return Err("wrong_bytes"),
                Err(_) => return Err("malformed"),
            },
            (ChunkSpecV1::Rows { first_leaf, leaves, .. }, ChunkBodyV1::Rows(range)) => {
                if (range.first_leaf, range.leaves) != (*first_leaf, *leaves) {
                    return Err("malformed");
                }
                match range.recomputed_commitment(dtype, &shape) {
                    Some(c) if c == chunk.commitment => {}
                    Some(_) => return Err("wrong_root"),
                    None => return Err("fake_opening"),
                }
            }
            // A withheld value: its authenticated opening is all it owes.
            (ChunkSpecV1::Withheld { .. }, ChunkBodyV1::Withheld) => {}
            _ => return Err("malformed"),
        }
    }
    let _ = SEG_LEN_V4;
    Ok(r.part)
}

/// A position assembled from its served parts: every value (a withheld one as zeros of its declared shape — the verifier supplies its
/// own), every node commitment, and the position root's path.
pub type AssembledPositionV1 = (Vec<Vec<Tensor>>, Vec<Digest>, Vec<Vec<Digest>>);

/// **Every committed value of position `p` from its served parts** (a fresh verifier reading the responses from the blocks): `None`
/// unless every part is present and authentic. Returns `(values, siblings, commitments)` ([`AssembledPositionV1`]).
pub fn assemble_position_v1(
    program: &TirProgramV1,
    segment_roots: &[Digest],
    positions: u32,
    p: u32,
    parts: &[Vec<u8>],
) -> Option<AssembledPositionV1> {
    let w = WiringV1::new(program).ok()?;
    let layout = position_parts_v1(program, p).ok()?;
    let mut by_part: Vec<Option<SegPartResponseV1>> = vec![None; layout.len()];
    for b in parts {
        let i = classify_part_v1(program, segment_roots, positions, p, b).ok()?;
        by_part[i as usize] = borsh::from_slice(b).ok();
    }
    let mut bytes: Vec<Vec<Option<Vec<u8>>>> =
        w.occurrences.iter().map(|(b, _)| vec![Some(Vec::new()); program.blocks[*b as usize].nodes.len()]).collect();
    let mut commitments: Vec<Vec<Digest>> =
        w.occurrences.iter().map(|(b, _)| vec![[0u8; 64]; program.blocks[*b as usize].nodes.len()]).collect();
    let mut siblings = None;
    for r in by_part {
        let r = r?;
        siblings = Some(r.position_siblings.clone());
        for c in r.chunks {
            commitments[c.occurrence as usize][c.node as usize] = c.commitment;
            let slot = &mut bytes[c.occurrence as usize][c.node as usize];
            match c.body {
                ChunkBodyV1::Whole(b) => *slot = Some(b),
                ChunkBodyV1::Rows(range) => {
                    if let Some(v) = slot.as_mut() {
                        v.extend_from_slice(&range.bytes)
                    }
                }
                ChunkBodyV1::Withheld => *slot = None,
            }
        }
    }
    let mut values = Vec::with_capacity(bytes.len());
    for (s, occ) in bytes.into_iter().enumerate() {
        let mut row = Vec::with_capacity(occ.len());
        for (n, b) in occ.into_iter().enumerate() {
            let node = w.node(s as u16, n as u16);
            let shape = node.out.resolve(w.h(s as u16, p));
            row.push(match b {
                Some(b) => Tensor::from_le_bytes(node.out.dtype, &shape, &b).ok()?,
                None => Tensor::zeros(node.out.dtype, &shape),
            });
        }
        values.push(row);
    }
    Some((values, siblings.unwrap_or_default(), commitments))
}

/// What one position of a segmented claim costs to serve: `(material bytes, parts)`.
pub fn position_material_v1(program: &TirProgramV1, p: u32) -> Result<(u128, u32), String> {
    let w = WiringV1::new(program).map_err(|e| e.to_string())?;
    let bytes: u128 = declared_values(&w, p)
        .iter()
        .filter(|(.., withheld)| !withheld)
        .map(|(_, _, d, sh, _)| LayoutV3::of(sh).len as u128 * d.width() as u128)
        .sum();
    Ok((bytes, position_parts_v1(program, p)?.len() as u32))
}

/// A demand's progress on a segmented claim (ledger table 21): the parts served, and — once complete — the demanders whose bonds stay
/// reserved until the proof grace ends (settled there: refunded today; G14-R4 decides the burn).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SegProgressV1 {
    pub parts: u32,
    pub served: Vec<u8>,
    /// The DAA at which the last part was served (`None` while incomplete).
    pub complete_daa: Option<u64>,
    /// The demanders of a completed demand and their bonds, held until `grace_until`.
    pub held: Vec<(Digest, u64)>,
    pub grace_until: u64,
}

impl SegProgressV1 {
    pub fn new(parts: u32) -> Self {
        Self { parts, served: vec![0; (parts as usize).div_ceil(8)], complete_daa: None, held: Vec::new(), grace_until: 0 }
    }

    pub fn is_served(&self, part: u32) -> bool {
        self.served.get(part as usize / 8).is_some_and(|b| b & (1 << (part % 8)) != 0)
    }

    pub fn mark(&mut self, part: u32) {
        if let Some(b) = self.served.get_mut(part as usize / 8) {
            *b |= 1 << (part % 8);
        }
    }

    pub fn all_served(&self) -> bool {
        (0..self.parts).all(|i| self.is_served(i))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The layout as first written: every row leaf's bytes counted from its element list (the reference the fast one must equal).
    fn reference_layout(values: &[(u16, u16, DType, Vec<usize>)]) -> Vec<Vec<ChunkSpecV1>> {
        let node_count = values.len() as u64;
        let budget = SEG_PART_BYTES_V4 - PART_HEADER_BOUND_V4;
        let mut parts: Vec<Vec<ChunkSpecV1>> = vec![Vec::new()];
        let mut room = budget;
        for (s, n, dtype, shape) in values {
            let (s, n) = (*s, *n);
            let l = LayoutV3::of(shape);
            let width = dtype.width() as u64;
            let bytes = l.len * width;
            let oh = chunk_overhead(node_count, l.leaves(AXIS_ROW), shape.len());
            if bytes + oh <= room {
                parts.last_mut().unwrap().push(ChunkSpecV1::Whole { occurrence: s, node: n });
                room -= bytes + oh;
                continue;
            }
            if bytes + oh <= budget {
                parts.push(vec![ChunkSpecV1::Whole { occurrence: s, node: n }]);
                room = budget - bytes - oh;
                continue;
            }
            let total = l.leaves(AXIS_ROW);
            let leaf_bytes = |i: u64| {
                let (line, tile) = (i / l.row_tiles, i % l.row_tiles);
                l.leaf_elements(AXIS_ROW, line, tile).map_or(0, |v| v.len() as u64) * width
            };
            let mut i = 0u64;
            while i < total {
                let mut used = oh;
                let mut k = 0u64;
                while i + k < total && used + leaf_bytes(i + k) <= room {
                    used += leaf_bytes(i + k);
                    k += 1;
                }
                if k == 0 {
                    parts.push(Vec::new());
                    room = budget;
                    continue;
                }
                parts.last_mut().unwrap().push(ChunkSpecV1::Rows { occurrence: s, node: n, first_leaf: i, leaves: k });
                room -= used;
                i += k;
            }
        }
        parts
    }

    /// The arithmetic layout is the element-counting one, part for part, over values that fit, values that start a part, rows of one
    /// tile and of many (a ragged last tile), flat values, and long runs of whole rows.
    #[test]
    fn the_fast_layout_is_the_reference_layout() {
        let cases: Vec<Vec<(u16, u16, DType, Vec<usize>)>> = vec![
            vec![(0, 0, DType::I32, vec![8]), (0, 1, DType::I8, vec![16, 8])],
            vec![(0, 0, DType::I32, vec![40_000, 8]), (0, 1, DType::I64, vec![8])],
            vec![(0, 0, DType::I8, vec![3, 5_000]), (0, 1, DType::I16, vec![4_096 * 300]), (1, 0, DType::I64, vec![2, 3, 70_001])],
            vec![(0, 0, DType::I32, vec![8_192, 1_024]), (0, 1, DType::I32, vec![8_192, 1_024]), (0, 2, DType::I8, vec![248_320])],
            vec![(0, 0, DType::I64, vec![17, 9_000]), (0, 1, DType::I8, vec![1]), (0, 2, DType::I32, vec![1_000, 4_097])],
            (0..40u16).map(|n| (0, n, DType::I32, vec![1 + (n as usize * 7_919) % 9_000, 3 + n as usize * 13])).collect(),
        ];
        for (c, values) in cases.iter().enumerate() {
            let clear: Vec<(u16, u16, DType, Vec<usize>, bool)> =
                values.iter().map(|(s, n, d, sh)| (*s, *n, *d, sh.clone(), false)).collect();
            let fast = layout_parts(&clear).unwrap();
            assert_eq!(fast, reference_layout(values), "case {c}");
            assert!(fast.len() >= 1);
        }
    }
}
