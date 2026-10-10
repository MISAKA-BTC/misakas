//! **Tensor commitment v3 — tiled dual-root Merkle commitments** (K2-TIR-v4, `docs/design/palw/k2-real-scale.md` §1.1).
//!
//! v2's commitment ([`crate::merkle`]) binds a row tree and a column tree over the same elements; a leaf is a whole row or column,
//! so one leaf of a `[1, 248,320]` logits vector is the whole vector. v3 keeps both trees and **tiles every line**: a leaf holds at
//! most [`TILE_V3`] elements of one row (or column), so an opening is bounded whatever the tensor's size.
//!
//! ```text
//! rows  = len / n   (n the last dim; rank ≤ 1: one row of len)       row leaf (r, t) = elements [r·n + t·T, r·n + min((t+1)·T, n))
//! cols  = batch · n (m the second-to-last dim; rank ≤ 1: one column)  col leaf (c, t) = x[b, t·T .. min((t+1)·T, m), j], c = b·n + j
//! leaf  = H(LEAF_V3; dtype, axis, line u64, tile u64, count u64, elements at width)
//! tree  = binary over the leaves in (line, tile) order; an unpaired last node is carried up
//! C(x)  = H(TENSOR_V3; dtype, rank, dims, row root, col root)
//! ```
//!
//! [`LeafOpeningV3`] opens one leaf with its path and the other root; [`RowRangeV3`] opens a contiguous range of row leaves with a
//! Merkle range proof (a served part of a value too large for one response, §4). Everything decoded from a wire is checked without
//! panicking: an overflowing shape, a wrong count or a path that fits no tree is malformed, never a crash.

use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::{DType, Tensor};

use crate::hash::{Digest, finish, keyed};
use crate::merkle::{AXIS_COL, AXIS_ROW};

pub const TENSOR_COMMITMENT_DOMAIN_V3: &[u8] = b"misaka-palw/kernel/tensor/v3";
pub const TENSOR_LEAF_DOMAIN_V3: &[u8] = b"misaka-palw/kernel/tensor-leaf/v3";
pub const TENSOR_NODE_DOMAIN_V3: &[u8] = b"misaka-palw/kernel/tensor-node/v3";

/// The most elements one leaf holds.
pub const TILE_V3: u64 = 4096;

/// The leaf layout of a shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutV3 {
    pub len: u64,
    /// Lines along the last axis and their length.
    pub rows: u64,
    pub row_len: u64,
    /// Lines along the second-to-last axis (`batch · n`) and their length (`m`).
    pub cols: u64,
    pub col_len: u64,
    pub m: u64,
    pub n: u64,
    pub row_tiles: u64,
    pub col_tiles: u64,
}

impl LayoutV3 {
    /// `None` when the element count overflows a `u64` (a malformed wire shape).
    pub fn try_of(shape: &[usize]) -> Option<Self> {
        let dims: Vec<u64> = shape.iter().map(|d| *d as u64).collect();
        let len: u64 = if dims.contains(&0) { 0 } else { dims.iter().try_fold(1u64, |acc, d| acc.checked_mul(*d))? };
        let (rows, row_len, cols, col_len, m, n) = match dims.len() {
            0 | 1 => (1, len, 1, len, 1, len),
            r => {
                let (m, n) = (dims[r - 2], dims[r - 1]);
                let batch = if len == 0 { 0 } else { len / (m * n) };
                (if n == 0 { 0 } else { len / n }, n, batch * n, m, m, n)
            }
        };
        let tiles = |l: u64| l.div_ceil(TILE_V3).max(if l == 0 { 0 } else { 1 });
        Some(Self { len, rows, row_len, cols, col_len, m, n, row_tiles: tiles(row_len), col_tiles: tiles(col_len) })
    }

    pub fn of(shape: &[usize]) -> Self {
        Self::try_of(shape).expect("a constructed tensor's element count fits a u64")
    }

    /// Leaves of one tree.
    pub fn leaves(&self, axis: u8) -> u64 {
        if axis == AXIS_ROW { self.rows * self.row_tiles } else { self.cols * self.col_tiles }
    }

    fn line_len(&self, axis: u8) -> u64 {
        if axis == AXIS_ROW { self.row_len } else { self.col_len }
    }

    /// The elements of leaf `(line, tile)` of a tree, as flat (row-major) indices.
    pub fn leaf_elements(&self, axis: u8, line: u64, tile: u64) -> Option<Vec<u64>> {
        let lines = if axis == AXIS_ROW { self.rows } else { self.cols };
        let tiles = if axis == AXIS_ROW { self.row_tiles } else { self.col_tiles };
        if line >= lines || tile >= tiles {
            return None;
        }
        let ll = self.line_len(axis);
        let (a, b) = (tile * TILE_V3, ((tile + 1) * TILE_V3).min(ll));
        Some(if axis == AXIS_ROW || self.is_flat() {
            let base = line * ll;
            (a..b).map(|i| base + i).collect()
        } else {
            let (bi, j) = (line / self.n, line % self.n);
            (a..b).map(|i| bi * self.m * self.n + i * self.n + j).collect()
        })
    }

    /// Whether the tensor is rank ≤ 1 (both trees are the one flat line).
    fn is_flat(&self) -> bool {
        self.rows == 1 && self.cols == 1 && self.m == 1
    }

    /// The row leaf holding flat element `e`, and the offset inside it.
    pub fn row_leaf_of(&self, e: u64) -> Option<(u64, u64, u64)> {
        if e >= self.len || self.row_len == 0 {
            return None;
        }
        let (r, j) = (e / self.row_len, e % self.row_len);
        Some((r, j / TILE_V3, j % TILE_V3))
    }

    /// The column leaf holding flat element `e`, and the offset inside it.
    pub fn col_leaf_of(&self, e: u64) -> Option<(u64, u64, u64)> {
        if e >= self.len {
            return None;
        }
        if self.is_flat() {
            return Some((0, e / TILE_V3, e % TILE_V3));
        }
        let (b, i, j) = (e / (self.m * self.n), (e / self.n) % self.m, e % self.n);
        Some((b * self.n + j, i / TILE_V3, i % TILE_V3))
    }

    /// The leaf index of `(line, tile)` in its tree.
    pub fn leaf_index(&self, axis: u8, line: u64, tile: u64) -> u64 {
        line * if axis == AXIS_ROW { self.row_tiles } else { self.col_tiles } + tile
    }
}

pub(crate) fn leaf_hash_v3(dtype: DType, axis: u8, line: u64, tile: u64, values: &[i128]) -> Digest {
    leaf_hash_values_v3(dtype, axis, line, tile, values.len() as u64, values.iter().copied())
}

/// The unchanged v3 preimage, streamed through a fixed 4 KiB encoding buffer. BLAKE2 update boundaries have no wire meaning.
/// A producer can hash contiguous rows or strided columns directly, without allocating index/value vectors for every leaf.
fn leaf_hash_values_v3(dtype: DType, axis: u8, line: u64, tile: u64, count: u64, values: impl Iterator<Item = i128>) -> Digest {
    let mut s = keyed(TENSOR_LEAF_DOMAIN_V3);
    s.update(&[dtype.tag(), axis]).update(&line.to_le_bytes()).update(&tile.to_le_bytes()).update(&count.to_le_bytes());
    let width = dtype.width();
    let mut encoded = [0u8; 4096];
    let mut used = 0;
    for v in values {
        if used + width > encoded.len() {
            s.update(&encoded[..used]);
            used = 0;
        }
        encoded[used..used + width].copy_from_slice(&v.to_le_bytes()[..width]);
        used += width;
    }
    if used > 0 {
        s.update(&encoded[..used]);
    }
    finish(s)
}

pub(crate) fn node_hash_v3(l: &Digest, r: &Digest) -> Digest {
    let mut s = keyed(TENSOR_NODE_DOMAIN_V3);
    s.update(l).update(r);
    finish(s)
}

/// A binary root over `level` (unpaired last carried up); the empty tree's root is the domain's empty hash.
pub(crate) fn tree_root(mut level: Vec<Digest>, node: fn(&Digest, &Digest) -> Digest, empty: &[u8]) -> Digest {
    if level.is_empty() {
        return finish(keyed(empty));
    }
    while level.len() > 1 {
        level = level.chunks(2).map(|c| if c.len() == 2 { node(&c[0], &c[1]) } else { c[0] }).collect();
    }
    level[0]
}

/// The siblings of leaf `index`, bottom up.
pub(crate) fn tree_path(mut level: Vec<Digest>, mut index: usize, node: fn(&Digest, &Digest) -> Digest) -> Vec<Digest> {
    let mut out = Vec::new();
    while level.len() > 1 {
        let sib = index ^ 1;
        if sib < level.len() {
            out.push(level[sib]);
        }
        level = level.chunks(2).map(|c| if c.len() == 2 { node(&c[0], &c[1]) } else { c[0] }).collect();
        index >>= 1;
    }
    out
}

/// Recompute a root from a leaf, its index among `count`, and its siblings (`None`: a path that fits no tree).
pub(crate) fn tree_root_from_path(
    leaf: Digest,
    mut index: u64,
    mut count: u64,
    siblings: &[Digest],
    node: fn(&Digest, &Digest) -> Digest,
) -> Option<Digest> {
    if index >= count {
        return None;
    }
    let mut cur = leaf;
    let mut it = siblings.iter();
    while count > 1 {
        let sib = index ^ 1;
        if sib < count {
            let s = it.next()?;
            cur = if index & 1 == 0 { node(&cur, s) } else { node(s, &cur) };
        }
        index >>= 1;
        count = count.div_ceil(2);
    }
    it.next().is_none().then_some(cur)
}

/// The siblings a contiguous range `[a, b)` of `level` needs to recompute the root (per level: left, then right).
pub(crate) fn range_siblings(mut level: Vec<Digest>, mut a: usize, mut b: usize, node: fn(&Digest, &Digest) -> Digest) -> Vec<Digest> {
    let mut out = Vec::new();
    while level.len() > 1 {
        if a % 2 == 1 {
            out.push(level[a - 1]);
        }
        if (b - 1) % 2 == 0 && b < level.len() {
            out.push(level[b]);
        }
        a /= 2;
        b = (b - 1) / 2 + 1;
        level = level.chunks(2).map(|c| if c.len() == 2 { node(&c[0], &c[1]) } else { c[0] }).collect();
    }
    out
}

/// **Recompute a root from the contiguous leaves `[a, a + leaves.len())` of a tree of `count` leaves** and their range siblings.
pub(crate) fn range_root(
    leaves: &[Digest],
    a: u64,
    count: u64,
    siblings: &[Digest],
    node: fn(&Digest, &Digest) -> Digest,
) -> Option<Digest> {
    let b = a.checked_add(leaves.len() as u64)?;
    if leaves.is_empty() || b > count {
        return None;
    }
    let (mut a, mut b, mut len) = (a, b, count);
    let mut known: Vec<Digest> = leaves.to_vec();
    let mut it = siblings.iter();
    while len > 1 {
        let mut ext = Vec::with_capacity(known.len() + 2);
        let start = if a % 2 == 1 {
            ext.push(*it.next()?);
            a - 1
        } else {
            a
        };
        ext.extend_from_slice(&known);
        let mut end = b;
        if (b - 1) % 2 == 0 && b < len {
            ext.push(*it.next()?);
            end = b + 1;
        }
        // `ext` holds indices [start, end): every pair complete but an unpaired last node of the level.
        known = ext.chunks(2).map(|c| if c.len() == 2 { node(&c[0], &c[1]) } else { c[0] }).collect();
        a = start / 2;
        b = (end - 1) / 2 + 1;
        len = len.div_ceil(2);
    }
    (it.next().is_none() && known.len() == 1).then_some(known[0])
}

fn values_of(t: &Tensor, idx: &[u64]) -> Vec<i128> {
    idx.iter().map(|i| t.data[*i as usize]).collect()
}

fn tree_leaves(t: &Tensor, l: &LayoutV3, axis: u8) -> Vec<Digest> {
    let (lines, tiles) = if axis == AXIS_ROW { (l.rows, l.row_tiles) } else { (l.cols, l.col_tiles) };
    let mut out = Vec::with_capacity((lines * tiles) as usize);
    for line in 0..lines {
        for tile in 0..tiles {
            let len = l.line_len(axis);
            let (first, end) = (tile * TILE_V3, ((tile + 1) * TILE_V3).min(len));
            let count = end - first;
            let hash = if axis == AXIS_ROW || l.is_flat() {
                let base = line * len;
                leaf_hash_values_v3(
                    t.dtype,
                    axis,
                    line,
                    tile,
                    count,
                    t.data[(base + first) as usize..(base + end) as usize].iter().copied(),
                )
            } else {
                let base = (line / l.n) * l.m * l.n + line % l.n;
                leaf_hash_values_v3(t.dtype, axis, line, tile, count, (first..end).map(|i| t.data[(base + i * l.n) as usize]))
            };
            out.push(hash);
        }
    }
    out
}

pub(crate) fn commit_v3(dtype: DType, shape: &[usize], row_root: &Digest, col_root: &Digest) -> Digest {
    let mut s = keyed(TENSOR_COMMITMENT_DOMAIN_V3);
    s.update(&[dtype.tag(), shape.len() as u8]);
    for d in shape {
        s.update(&(*d as u64).to_le_bytes());
    }
    s.update(row_root).update(col_root);
    finish(s)
}

/// The leaf digests of both trees of a tensor (a prover keeps them to answer openings).
pub struct TreesV3 {
    pub layout: LayoutV3,
    pub row_leaves: Vec<Digest>,
    pub col_leaves: Vec<Digest>,
}

impl TreesV3 {
    pub fn of(t: &Tensor) -> Self {
        let layout = LayoutV3::of(&t.shape);
        Self { row_leaves: tree_leaves(t, &layout, AXIS_ROW), col_leaves: tree_leaves(t, &layout, AXIS_COL), layout }
    }

    pub fn row_root(&self) -> Digest {
        tree_root(self.row_leaves.clone(), node_hash_v3, TENSOR_NODE_DOMAIN_V3)
    }

    pub fn col_root(&self) -> Digest {
        tree_root(self.col_leaves.clone(), node_hash_v3, TENSOR_NODE_DOMAIN_V3)
    }
}

/// **The v3 commitment of a tensor.**
pub fn tensor_commitment_v3(t: &Tensor) -> Digest {
    let trees = TreesV3::of(t);
    commit_v3(t.dtype, &t.shape, &trees.row_root(), &trees.col_root())
}

/// **One leaf of a committed tensor**, with its path and the other tree's root.
///
/// Its wire form (`BorshSerialize` below) carries the values **at the dtype's width**, exactly as the leaf hash reads them: a leaf of
/// 4,096 `i8` elements files 4,096 bytes of values, not 65,536. Every price of a filing (`byte_len`, the element court's
/// `element_court_cost_v1`) is a price of this form; the derived `i128`-per-element encoding filed up to 16× the priced bytes, so a court
/// the gate admitted against the carrier could produce a filing the carrier refuses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeafOpeningV3 {
    pub dtype: u8,
    pub shape: Vec<u64>,
    pub axis: u8,
    pub line: u64,
    pub tile: u64,
    pub values: Vec<i128>,
    pub siblings: Vec<Digest>,
    pub other_root: Digest,
}

fn leaf_wire_error(why: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, format!("a leaf opening: {why}"))
}

/// The wire form: `dtype u8 ‖ shape Vec<u64> ‖ axis u8 ‖ line u64 ‖ tile u64 ‖ count u32 ‖ count × value at the dtype's width (little
/// endian, two's complement; `idx` unsigned) ‖ siblings Vec<Digest> ‖ other_root`. A value outside its dtype, or an unknown dtype, has
/// no wire form (serializing it is an error, never a silent truncation).
impl BorshSerialize for LeafOpeningV3 {
    fn serialize<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<()> {
        let dtype = self.dtype().ok_or_else(|| leaf_wire_error("an unknown dtype"))?;
        self.dtype.serialize(w)?;
        self.shape.serialize(w)?;
        self.axis.serialize(w)?;
        self.line.serialize(w)?;
        self.tile.serialize(w)?;
        u32::try_from(self.values.len()).map_err(|_| leaf_wire_error("too many values"))?.serialize(w)?;
        let mut bytes = Vec::with_capacity(self.values.len() * dtype.width());
        for v in &self.values {
            if !dtype.contains(*v) {
                return Err(leaf_wire_error("a value outside its dtype"));
            }
            dtype.encode_le(*v, &mut bytes);
        }
        w.write_all(&bytes)?;
        self.siblings.serialize(w)?;
        self.other_root.serialize(w)
    }
}

impl BorshDeserialize for LeafOpeningV3 {
    fn deserialize_reader<R: std::io::Read>(r: &mut R) -> std::io::Result<Self> {
        let tag = u8::deserialize_reader(r)?;
        let dtype = DType::ALL.into_iter().find(|d| d.tag() == tag).ok_or_else(|| leaf_wire_error("an unknown dtype"))?;
        let shape = Vec::<u64>::deserialize_reader(r)?;
        let axis = u8::deserialize_reader(r)?;
        let line = u64::deserialize_reader(r)?;
        let tile = u64::deserialize_reader(r)?;
        let count = u32::deserialize_reader(r)? as usize;
        let width = dtype.width();
        // Never allocate what the bytes do not carry: a count the reader cannot back fails at its first missing element.
        let mut values = Vec::with_capacity(count.min(TILE_V3 as usize));
        let mut buf = [0u8; 16];
        for _ in 0..count {
            r.read_exact(&mut buf[..width])?;
            values.push(dtype.decode_le(&buf[..width]));
        }
        let siblings = Vec::<Digest>::deserialize_reader(r)?;
        let other_root = Digest::deserialize_reader(r)?;
        Ok(Self { dtype: tag, shape, axis, line, tile, values, siblings, other_root })
    }
}

impl LeafOpeningV3 {
    /// Leaf `(line, tile)` of tree `axis` of `t`.
    pub fn of(t: &Tensor, axis: u8, line: u64, tile: u64) -> Option<Self> {
        let trees = TreesV3::of(t);
        Self::of_trees(t, &trees, axis, line, tile)
    }

    /// The same, with the trees already built (a prover opening many leaves of one value).
    pub fn of_trees(t: &Tensor, trees: &TreesV3, axis: u8, line: u64, tile: u64) -> Option<Self> {
        let l = &trees.layout;
        let idx = l.leaf_elements(axis, line, tile)?;
        let index = l.leaf_index(axis, line, tile) as usize;
        let (mine, other) =
            if axis == AXIS_ROW { (&trees.row_leaves, &trees.col_leaves) } else { (&trees.col_leaves, &trees.row_leaves) };
        Some(Self {
            dtype: t.dtype.tag(),
            shape: t.shape.iter().map(|d| *d as u64).collect(),
            axis,
            line,
            tile,
            values: values_of(t, &idx),
            siblings: tree_path(mine.clone(), index, node_hash_v3),
            other_root: tree_root(other.clone(), node_hash_v3, TENSOR_NODE_DOMAIN_V3),
        })
    }

    /// The leaf of the cheapest tree that holds flat element `e` (`axis` names which).
    pub fn holding(t: &Tensor, trees: &TreesV3, axis: u8, e: u64) -> Option<Self> {
        let (line, tile, _) = if axis == AXIS_ROW { trees.layout.row_leaf_of(e)? } else { trees.layout.col_leaf_of(e)? };
        Self::of_trees(t, trees, axis, line, tile)
    }

    pub fn dtype(&self) -> Option<DType> {
        DType::ALL.into_iter().find(|d| d.tag() == self.dtype)
    }

    pub fn shape_usize(&self) -> Option<Vec<usize>> {
        self.shape.iter().map(|d| usize::try_from(*d).ok()).collect()
    }

    pub fn layout(&self) -> Option<LayoutV3> {
        LayoutV3::try_of(&self.shape_usize()?)
    }

    /// The commitment this leaf is of, if it is a well-formed leaf of SOME tensor.
    pub fn recomputed_commitment(&self) -> Option<Digest> {
        let (dtype, shape) = (self.dtype()?, self.shape_usize()?);
        if shape.len() > misaka_palw_tir::types::MAX_RANK {
            return None;
        }
        let l = LayoutV3::try_of(&shape)?;
        if self.axis != AXIS_ROW && self.axis != AXIS_COL {
            return None;
        }
        let idx = l.leaf_elements(self.axis, self.line, self.tile)?;
        if self.values.len() != idx.len() || self.values.iter().any(|v| !dtype.contains(*v)) {
            return None;
        }
        let leaf = leaf_hash_v3(dtype, self.axis, self.line, self.tile, &self.values);
        let root = tree_root_from_path(
            leaf,
            l.leaf_index(self.axis, self.line, self.tile),
            l.leaves(self.axis),
            &self.siblings,
            node_hash_v3,
        )?;
        let (r, c) = if self.axis == AXIS_ROW { (root, self.other_root) } else { (self.other_root, root) };
        Some(commit_v3(dtype, &shape, &r, &c))
    }

    pub fn authenticates(&self, commitment: &Digest) -> bool {
        self.recomputed_commitment().is_some_and(|c| c == *commitment)
    }

    /// `(flat index, value)` of every element the leaf holds (`None` for a malformed leaf).
    pub fn elements(&self) -> Option<Vec<(u64, i128)>> {
        let idx = self.layout()?.leaf_elements(self.axis, self.line, self.tile)?;
        (idx.len() == self.values.len()).then(|| idx.into_iter().zip(self.values.iter().copied()).collect())
    }

    /// Bytes this leaf puts in a filing: its wire form's length is `94 + 8·rank + count·width + 64·siblings`; this is that plus two.
    pub fn byte_len(&self) -> u64 {
        let w = self.dtype().map(|d| d.width()).unwrap_or(16) as u64;
        self.values.len() as u64 * w + (self.siblings.len() as u64 + 1) * 64 + 8 * self.shape.len() as u64 + 32
    }
}

/// **A contiguous range of row leaves of a committed tensor** with its Merkle range proof and the column root — one served slice of
/// a value too large for one response part.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RowRangeV3 {
    pub first_leaf: u64,
    pub leaves: u64,
    /// The elements of the leaves, in order, at the dtype's width.
    pub bytes: Vec<u8>,
    pub siblings: Vec<Digest>,
    pub col_root: Digest,
}

impl RowRangeV3 {
    /// Row leaves `[first, first + count)` of `t`.
    pub fn of(t: &Tensor, trees: &TreesV3, first: u64, count: u64) -> Option<Self> {
        let l = &trees.layout;
        if count == 0 || first.checked_add(count)? > l.leaves(AXIS_ROW) {
            return None;
        }
        let mut bytes = Vec::new();
        for i in first..first + count {
            let (line, tile) = (i / l.row_tiles, i % l.row_tiles);
            for e in l.leaf_elements(AXIS_ROW, line, tile)? {
                t.dtype.encode_le(t.data[e as usize], &mut bytes);
            }
        }
        Some(Self {
            first_leaf: first,
            leaves: count,
            bytes,
            siblings: range_siblings(trees.row_leaves.clone(), first as usize, (first + count) as usize, node_hash_v3),
            col_root: trees.col_root(),
        })
    }

    /// The commitment of the tensor of `(dtype, shape)` these leaves are a range of (`None`: malformed).
    pub fn recomputed_commitment(&self, dtype: DType, shape: &[usize]) -> Option<Digest> {
        let l = LayoutV3::try_of(shape)?;
        let total = l.leaves(AXIS_ROW);
        if self.leaves == 0 || self.first_leaf.checked_add(self.leaves)? > total {
            return None;
        }
        let w = dtype.width();
        let mut at = 0usize;
        let mut hashes = Vec::with_capacity(self.leaves as usize);
        for i in self.first_leaf..self.first_leaf + self.leaves {
            let (line, tile) = (i / l.row_tiles, i % l.row_tiles);
            let n = l.leaf_elements(AXIS_ROW, line, tile)?.len();
            let end = at.checked_add(n.checked_mul(w)?)?;
            let chunk = self.bytes.get(at..end)?;
            let values: Vec<i128> = chunk.chunks_exact(w).map(|c| dtype.decode_le(c)).collect();
            hashes.push(leaf_hash_v3(dtype, AXIS_ROW, line, tile, &values));
            at = end;
        }
        if at != self.bytes.len() {
            return None;
        }
        let root = range_root(&hashes, self.first_leaf, total, &self.siblings, node_hash_v3)?;
        Some(commit_v3(dtype, shape, &root, &self.col_root))
    }
}

/// The Merkle depth of `count` leaves.
pub fn depth_v3(count: u64) -> u64 {
    crate::merkle::depth(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(shape: &[usize]) -> Tensor {
        let n: usize = shape.iter().product();
        Tensor::new(DType::I32, shape.to_vec(), (0..n as i128).map(|v| v * 7 - 50).collect()).unwrap()
    }

    /// Original scalar-update oracle, intentionally independent of the new iterator/buffering implementation.
    fn original_leaf(dtype: DType, axis: u8, line: u64, tile: u64, values: &[i128]) -> Digest {
        let mut s = keyed(TENSOR_LEAF_DOMAIN_V3);
        s.update(&[dtype.tag(), axis])
            .update(&line.to_le_bytes())
            .update(&tile.to_le_bytes())
            .update(&(values.len() as u64).to_le_bytes());
        for v in values {
            s.update(&v.to_le_bytes()[..dtype.width()]);
        }
        finish(s)
    }

    #[test]
    fn buffered_leaf_hashes_preserve_every_dtype_and_update_boundary() {
        for dtype in DType::ALL {
            for count in [0, 1, 127, 128, 255, 256, 257, 511, 512, 513, 4095, 4096, 4097] {
                // Includes sign bits and bytes past each narrow width: hashing preserves the historical preimage exactly.
                let values: Vec<i128> = (0..count).map(|i| (i128::MAX / 17).wrapping_mul(i as i128).wrapping_sub(511)).collect();
                for axis in [AXIS_ROW, AXIS_COL] {
                    assert_eq!(
                        leaf_hash_v3(dtype, axis, 7, 3, &values),
                        original_leaf(dtype, axis, 7, 3, &values),
                        "{} axis {axis} count {count}",
                        dtype.name()
                    );
                }
            }
        }
    }

    #[test]
    fn direct_row_and_column_hashing_preserves_original_roots_and_openings() {
        for shape in [
            vec![],
            vec![0],
            vec![4097],
            vec![0, 3],
            vec![3, 0],
            vec![3, 4097],
            vec![4097, 3],
            vec![2, 3, 7],
            vec![2, 1, 4097],
            vec![2, 2, 3, 5],
        ] {
            for dtype in DType::ALL {
                let len: usize = shape.iter().product();
                let values = (0..len).map(|i| if dtype == DType::Idx { (i % 117) as i128 } else { (i % 117) as i128 - 58 }).collect();
                let tensor = Tensor::new(dtype, shape.clone(), values).unwrap();
                let layout = LayoutV3::of(&shape);
                let oracle = |axis| {
                    let (lines, tiles) =
                        if axis == AXIS_ROW { (layout.rows, layout.row_tiles) } else { (layout.cols, layout.col_tiles) };
                    let mut leaves = Vec::new();
                    for line in 0..lines {
                        for tile in 0..tiles {
                            let indices = layout.leaf_elements(axis, line, tile).unwrap();
                            leaves.push(original_leaf(dtype, axis, line, tile, &values_of(&tensor, &indices)));
                        }
                    }
                    leaves
                };
                let trees = TreesV3::of(&tensor);
                let (rows, cols) = (oracle(AXIS_ROW), oracle(AXIS_COL));
                assert_eq!(trees.row_leaves, rows, "rows {} {shape:?}", dtype.name());
                assert_eq!(trees.col_leaves, cols, "cols {} {shape:?}", dtype.name());
                let old_root = commit_v3(
                    dtype,
                    &shape,
                    &tree_root(rows, node_hash_v3, TENSOR_NODE_DOMAIN_V3),
                    &tree_root(cols, node_hash_v3, TENSOR_NODE_DOMAIN_V3),
                );
                assert_eq!(tensor_commitment_v3(&tensor), old_root);
                if len > 0 {
                    for axis in [AXIS_ROW, AXIS_COL] {
                        let opened = LeafOpeningV3::holding(&tensor, &trees, axis, (len / 2) as u64).unwrap();
                        assert!(opened.authenticates(&old_root));
                        let bytes = borsh::to_vec(&opened).unwrap();
                        let decoded: LeafOpeningV3 = borsh::from_slice(&bytes).unwrap();
                        assert!(decoded.authenticates(&old_root));
                    }
                }
            }
        }
    }

    /// Local producer microbenchmark, not checkpoint throughput or consensus assurance. Criteria and geometries precede the run:
    /// exact roots; <=10% median regression in every case; >=10% median speedup for history and wide-logit tensors.
    #[test]
    #[ignore = "manual producer hash benchmark; hardware timing, not a consensus gate"]
    fn producer_hash_cost_against_original_scalar_updates() {
        fn original_root(t: &Tensor) -> Digest {
            let l = LayoutV3::of(&t.shape);
            let original_tree = |axis| {
                let (lines, tiles) = if axis == AXIS_ROW { (l.rows, l.row_tiles) } else { (l.cols, l.col_tiles) };
                let mut leaves = Vec::with_capacity((lines * tiles) as usize);
                for line in 0..lines {
                    for tile in 0..tiles {
                        let idx = l.leaf_elements(axis, line, tile).unwrap();
                        leaves.push(original_leaf(t.dtype, axis, line, tile, &values_of(t, &idx)));
                    }
                }
                leaves
            };
            // Match the original TreesV3 allocation/lifetime and root clones, not a reallocating or reduced-memory oracle.
            let rows = original_tree(AXIS_ROW);
            let cols = original_tree(AXIS_COL);
            let r = tree_root(rows.clone(), node_hash_v3, TENSOR_NODE_DOMAIN_V3);
            let c = tree_root(cols.clone(), node_hash_v3, TENSOR_NODE_DOMAIN_V3);
            commit_v3(t.dtype, &t.shape, &r, &c)
        }
        let cases = [
            ("history", DType::I16, vec![1024, 16], true),
            ("wide-logits", DType::I32, vec![131072], true),
            ("batched-strided", DType::I8, vec![2, 3, 257, 16], false),
            ("wide-accumulator", DType::I128, vec![3, 4097], false),
            ("short-row", DType::I16, vec![1, 8], false),
        ];
        for (name, dtype, shape, needs_gain) in cases {
            let count: usize = shape.iter().product();
            let tensor = Tensor::new(dtype, shape, (0..count).map(|i| (i % 117) as i128 - 58).collect()).unwrap();
            let expected = original_root(&tensor);
            assert_eq!(tensor_commitment_v3(&tensor), expected);
            let repetitions = if count < 100 { 1024 } else { 8 };
            let mut old = Vec::new();
            let mut new = Vec::new();
            for trial in 0..9 {
                let mut measure = |fast| {
                    let started = std::time::Instant::now();
                    for _ in 0..repetitions {
                        let root = if fast {
                            tensor_commitment_v3(std::hint::black_box(&tensor))
                        } else {
                            original_root(std::hint::black_box(&tensor))
                        };
                        assert_eq!(std::hint::black_box(root), expected);
                    }
                    let ns = started.elapsed().as_nanos();
                    if fast {
                        new.push(ns);
                    } else {
                        old.push(ns);
                    }
                };
                if trial % 2 == 0 {
                    measure(false);
                    measure(true);
                } else {
                    measure(true);
                    measure(false);
                }
            }
            old.sort_unstable();
            new.sort_unstable();
            let ratio = new[4] as f64 / old[4] as f64;
            eprintln!(
                "[producer-hash] {name} {:?} {} elements: original median {} ns, streamed median {} ns, ratio {ratio:.4}",
                dtype, count, old[4], new[4]
            );
            assert!(ratio <= 1.10, "{name}: >10% median regression, {ratio}");
            if needs_gain {
                assert!(ratio <= 0.90, "{name}: <10% median speedup, {ratio}");
            }
        }
    }

    #[test]
    fn every_leaf_of_both_trees_opens_and_holds_its_elements() {
        for shape in
            [vec![], vec![5], vec![9000], vec![3, 4], vec![2, 3, 5], vec![2, 5000], vec![5000, 2], vec![2, 2, 2, 3], vec![1, 9000]]
        {
            let x = t(&shape);
            let c = tensor_commitment_v3(&x);
            let trees = TreesV3::of(&x);
            let l = trees.layout;
            for axis in [AXIS_ROW, AXIS_COL] {
                let (lines, tiles) = if axis == AXIS_ROW { (l.rows, l.row_tiles) } else { (l.cols, l.col_tiles) };
                for line in 0..lines {
                    for tile in 0..tiles {
                        let o = LeafOpeningV3::of_trees(&x, &trees, axis, line, tile).unwrap();
                        assert!(o.authenticates(&c), "{shape:?} {axis} {line} {tile}");
                        assert!(o.values.len() as u64 <= TILE_V3);
                        for (e, v) in o.elements().unwrap() {
                            assert_eq!(x.data[e as usize], v);
                        }
                        let mut bad = o.clone();
                        bad.values[0] += 1;
                        assert!(!bad.authenticates(&c));
                    }
                }
            }
            for e in 0..l.len {
                for axis in [AXIS_ROW, AXIS_COL] {
                    let o = LeafOpeningV3::holding(&x, &trees, axis, e).unwrap();
                    assert!(o.elements().unwrap().iter().any(|(i, v)| *i == e && *v == x.data[e as usize]));
                }
            }
        }
    }

    #[test]
    fn every_row_range_proves_against_the_commitment() {
        for shape in [vec![9000], vec![7, 3], vec![3, 9000], vec![13, 2, 5]] {
            let x = t(&shape);
            let c = tensor_commitment_v3(&x);
            let trees = TreesV3::of(&x);
            let total = trees.layout.leaves(AXIS_ROW);
            for a in 0..total {
                for b in a + 1..=total {
                    let r = RowRangeV3::of(&x, &trees, a, b - a).unwrap();
                    assert_eq!(r.recomputed_commitment(x.dtype, &x.shape), Some(c), "{shape:?} [{a}, {b})");
                    let mut bad = r.clone();
                    bad.bytes[0] ^= 1;
                    assert_ne!(bad.recomputed_commitment(x.dtype, &x.shape), Some(c));
                    let mut moved = r.clone();
                    moved.first_leaf = (a + 1) % total;
                    if total > 1 && moved.first_leaf + moved.leaves <= total {
                        assert_ne!(moved.recomputed_commitment(x.dtype, &x.shape), Some(c));
                    }
                }
            }
        }
    }

    #[test]
    fn a_wire_shape_that_overflows_is_malformed_never_a_panic() {
        let o = LeafOpeningV3 {
            dtype: DType::I32.tag(),
            shape: vec![u64::MAX, 2],
            axis: AXIS_ROW,
            line: 0,
            tile: 0,
            values: vec![],
            siblings: vec![],
            other_root: [0; 64],
        };
        assert!(o.recomputed_commitment().is_none());
        assert!(LayoutV3::try_of(&[usize::MAX, 2]).is_none());
        let r = RowRangeV3 { first_leaf: u64::MAX, leaves: 2, bytes: vec![], siblings: vec![], col_root: [0; 64] };
        assert!(r.recomputed_commitment(DType::I32, &[3, 4]).is_none());
    }
}
