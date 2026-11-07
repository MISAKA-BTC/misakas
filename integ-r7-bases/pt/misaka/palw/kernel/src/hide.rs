//! **Flat 64-element hiding tiles** — DA16b option (a) (`docs/design/palw/models/k2-real-scale.md` §4.2, `docs/design/palw/verification/da16-transport-and-provider-court.md`
//! §7.9; lane G14-S, 2026-10-11). The v3 leaf rule for a **masked** value (model-dependent, ADR-0177 D2 Level 2) whose whole-value
//! court (option (c), [`crate::seg_scope`]) does not fit its bound: every 9B / BGE activation.
//!
//! ```text
//! tiles(len)    = 0 if len = 0, else max(1, ⌊len / 64⌋)          (row-major order of the WHOLE value, never per line)
//! tile t        = elements [64·t, 64·t + 64), the last tile [64·(tiles − 1), len)   (the ragged tail merged: 64 ..= 127 elements)
//! leaf          = H(HIDE_LEAF_V1; dtype, tile u64, count u64, elements at the dtype's width)
//! tree          = binary over the leaves in tile order; an unpaired last node is carried up (the v3 tree's rule, its own domain)
//! C_hide(x)     = H(HIDE_TENSOR_V1; dtype, rank, dims, root)
//! ```
//!
//! **Why flat.** v3 tiles each LINE (a row or a column). A model-dependent value with lines shorter than 64 elements (wide128's
//! `d = 16`, dense-MoE's head rows of 4) would be owed one hash per short line, and a hash of a few low-entropy activations is a
//! brute-force away from the activations themselves. A flat tile spans lines, so every leaf of a value of at least 64 elements hashes
//! at least 64 of them (≥ 512 bits of an `i8` value). A value under 64 elements is ONE leaf of its whole length, and its leaf hash is no
//! weaker than its commitment, which every claim publishes anyway (residual, recorded in the scope report as `short`).
//!
//! **What is owed.** A demanded position owes a hiding value as its leaf hashes, with a range proof per served range
//! ([`HidingHashRangeV1`]); never an element. The court ([`crate::element::HidingLeafFaultV1`]) recomputes ONE tile from operands the
//! filer supplies from its own re-execution (they authenticate only if they equal the producer's committed values) and the filer's own
//! copy of the model, and compares the tile's hash with the producer's authenticated leaf hash ([`HidingLeafHashV1`]).

use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::{DType, Tensor};

use crate::hash::{Digest, finish, keyed};
use crate::merkle3::{range_root, range_siblings, tree_path, tree_root, tree_root_from_path};

/// The elements of a full hiding tile (DA16b's hiding width).
pub const HIDE_TILE_V1: u64 = crate::scope::COURT_HIDING_LEAF_ELEMENTS_V1;
/// The most elements one hiding tile holds (a full tile and a ragged tail of 63 merged into it).
pub const HIDE_TILE_MAX_V1: u64 = 2 * HIDE_TILE_V1 - 1;

pub const TENSOR_HIDE_COMMITMENT_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/tensor-hide/v1";
pub const TENSOR_HIDE_LEAF_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/tensor-hide-leaf/v1";
pub const TENSOR_HIDE_NODE_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/tensor-hide-node/v1";

/// The tiles of a value of `len` elements.
pub fn hide_tiles_v1(len: u64) -> u64 {
    if len == 0 { 0 } else { (len / HIDE_TILE_V1).max(1) }
}

/// The flat range `[a, b)` of tile `tile` of a value of `len` elements (`None`: no such tile).
pub fn hide_tile_range_v1(len: u64, tile: u64) -> Option<(u64, u64)> {
    let tiles = hide_tiles_v1(len);
    if tile >= tiles {
        return None;
    }
    let a = tile * HIDE_TILE_V1;
    Some((a, if tile + 1 == tiles { len } else { a + HIDE_TILE_V1 }))
}

/// The tile holding flat element `e`.
pub fn hide_tile_of_v1(len: u64, e: u64) -> Option<u64> {
    (e < len).then(|| (e / HIDE_TILE_V1).min(hide_tiles_v1(len) - 1))
}

fn len_of(shape: &[usize]) -> Option<u64> {
    if shape.contains(&0) {
        return Some(0);
    }
    shape.iter().try_fold(1u64, |n, d| n.checked_mul(*d as u64))
}

pub(crate) fn hide_leaf_hash_v1(dtype: DType, tile: u64, values: &[i128]) -> Digest {
    let mut s = keyed(TENSOR_HIDE_LEAF_DOMAIN_V1);
    s.update(&[dtype.tag()]).update(&tile.to_le_bytes()).update(&(values.len() as u64).to_le_bytes());
    let mut bytes = Vec::with_capacity(values.len() * dtype.width());
    for v in values {
        dtype.encode_le(*v, &mut bytes);
    }
    s.update(&bytes);
    finish(s)
}

fn hide_node_hash_v1(l: &Digest, r: &Digest) -> Digest {
    let mut s = keyed(TENSOR_HIDE_NODE_DOMAIN_V1);
    s.update(l).update(r);
    finish(s)
}

/// `C_hide = H(HIDE_TENSOR_V1; dtype, rank, dims, root)`.
pub fn hide_commit_v1(dtype: DType, shape: &[usize], root: &Digest) -> Digest {
    let mut s = keyed(TENSOR_HIDE_COMMITMENT_DOMAIN_V1);
    s.update(&[dtype.tag(), shape.len() as u8]);
    for d in shape {
        s.update(&(*d as u64).to_le_bytes());
    }
    s.update(root);
    finish(s)
}

/// The root of a hiding tree from its leaves (the empty tree's root is the node domain's empty hash).
pub fn hide_root_of_leaves_v1(leaves: &[Digest]) -> Digest {
    tree_root(leaves.to_vec(), hide_node_hash_v1, TENSOR_HIDE_NODE_DOMAIN_V1)
}

/// The commitment of a value of `(dtype, shape)` from its leaf hashes (`None`: not that many tiles).
pub fn hide_commitment_of_leaves_v1(dtype: DType, shape: &[usize], leaves: &[Digest]) -> Option<Digest> {
    let len = len_of(shape)?;
    (leaves.len() as u64 == hide_tiles_v1(len)).then(|| hide_commit_v1(dtype, shape, &hide_root_of_leaves_v1(leaves)))
}

/// The leaf hashes of a tensor's hiding tree (a prover keeps them to answer openings; a verifier compares its own with the served).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HidingTreeV1 {
    pub dtype: DType,
    pub shape: Vec<usize>,
    pub leaves: Vec<Digest>,
}

impl HidingTreeV1 {
    pub fn of(t: &Tensor) -> Self {
        let len = t.data.len() as u64;
        let leaves = (0..hide_tiles_v1(len))
            .map(|tile| {
                let (a, b) = hide_tile_range_v1(len, tile).expect("in range");
                hide_leaf_hash_v1(t.dtype, tile, &t.data[a as usize..b as usize])
            })
            .collect();
        Self { dtype: t.dtype, shape: t.shape.clone(), leaves }
    }

    pub fn root(&self) -> Digest {
        hide_root_of_leaves_v1(&self.leaves)
    }

    pub fn commitment(&self) -> Digest {
        hide_commit_v1(self.dtype, &self.shape, &self.root())
    }

    /// The siblings of tile `tile`, bottom up.
    pub fn path(&self, tile: u64) -> Vec<Digest> {
        tree_path(self.leaves.clone(), tile as usize, hide_node_hash_v1)
    }

    /// The first tile whose leaf hash is not `other`'s (`None`: every one agrees, or the counts differ — compare commitments first).
    pub fn first_differing_tile(&self, other: &[Digest]) -> Option<u64> {
        if other.len() != self.leaves.len() {
            return None;
        }
        self.leaves.iter().zip(other).position(|(a, b)| a != b).map(|i| i as u64)
    }
}

/// **The hiding commitment of a tensor** (`C_hide`).
pub fn tensor_commitment_hide_v1(t: &Tensor) -> Digest {
    HidingTreeV1::of(t).commitment()
}

/// **One hiding tile with its elements and path** — what a filer supplies of a hiding OPERAND (its own re-execution's value, which
/// authenticates only if it is the producer's). The dtype and shape are not carried: the court reads them from the operand's declared
/// type, so a tile authenticates only as a tile of a value of that type.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct HidingLeafOpeningV1 {
    pub tile: u64,
    /// The tile's elements at the dtype's width, little endian.
    pub bytes: Vec<u8>,
    pub siblings: Vec<Digest>,
}

impl HidingLeafOpeningV1 {
    /// Tile `tile` of `t` from its tree.
    pub fn of(t: &Tensor, tree: &HidingTreeV1, tile: u64) -> Option<Self> {
        let (a, b) = hide_tile_range_v1(t.data.len() as u64, tile)?;
        let mut bytes = Vec::with_capacity(((b - a) as usize) * t.dtype.width());
        for v in &t.data[a as usize..b as usize] {
            t.dtype.encode_le(*v, &mut bytes);
        }
        Some(Self { tile, bytes, siblings: tree.path(tile) })
    }

    /// `(flat index, value)` of every element, as a tile of a value of `(dtype, shape)` (`None`: malformed for that type).
    pub fn elements(&self, dtype: DType, shape: &[usize]) -> Option<Vec<(u64, i128)>> {
        let (a, b) = hide_tile_range_v1(len_of(shape)?, self.tile)?;
        let w = dtype.width();
        if self.bytes.len() as u64 != (b - a) * w as u64 {
            return None;
        }
        Some(self.bytes.chunks_exact(w).enumerate().map(|(i, c)| (a + i as u64, dtype.decode_le(c))).collect())
    }

    /// The commitment this tile is of, as a tile of a value of `(dtype, shape)`.
    pub fn recomputed_commitment(&self, dtype: DType, shape: &[usize]) -> Option<Digest> {
        let values: Vec<i128> = self.elements(dtype, shape)?.into_iter().map(|(_, v)| v).collect();
        let leaf = hide_leaf_hash_v1(dtype, self.tile, &values);
        let root = tree_root_from_path(leaf, self.tile, hide_tiles_v1(len_of(shape)?), &self.siblings, hide_node_hash_v1)?;
        Some(hide_commit_v1(dtype, shape, &root))
    }

    pub fn authenticates(&self, dtype: DType, shape: &[usize], commitment: &Digest) -> bool {
        self.recomputed_commitment(dtype, shape).is_some_and(|c| c == *commitment)
    }

    /// Bytes this opening puts in a filing (its borsh form).
    pub fn byte_len(&self) -> u64 {
        8 + 4 + self.bytes.len() as u64 + 4 + 64 * self.siblings.len() as u64
    }
}

/// The filed bytes of one hiding tile of a value of `len` elements at `width` bytes an element: a tile of the most elements and the
/// tree's full depth (an upper bound of every [`HidingLeafOpeningV1::byte_len`] of that value).
pub fn hiding_leaf_opening_bytes_v1(len: u64, width: u64) -> u64 {
    8 + 4 + HIDE_TILE_MAX_V1.min(len.max(1)) * width + 4 + 64 * crate::merkle::depth(hide_tiles_v1(len).max(1))
}

/// **The producer's leaf hash of one tile, with its path** — the output side of a hiding-leaf court: the court authenticates it
/// against the committed value (as a value of the node's declared type) and compares it with the hash of the tile it recomputes.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct HidingLeafHashV1 {
    pub tile: u64,
    pub leaf: Digest,
    pub siblings: Vec<Digest>,
}

impl HidingLeafHashV1 {
    /// Tile `tile`'s hash and path in a tree of leaf hashes (the producer's, as served).
    pub fn of_leaves(leaves: &[Digest], tile: u64) -> Option<Self> {
        let leaf = *leaves.get(tile as usize)?;
        Some(Self { tile, leaf, siblings: tree_path(leaves.to_vec(), tile as usize, hide_node_hash_v1) })
    }

    pub fn authenticates(&self, dtype: DType, shape: &[usize], commitment: &Digest) -> bool {
        let Some(len) = len_of(shape) else { return false };
        tree_root_from_path(self.leaf, self.tile, hide_tiles_v1(len), &self.siblings, hide_node_hash_v1)
            .is_some_and(|root| hide_commit_v1(dtype, shape, &root) == *commitment)
    }

    pub fn byte_len(&self) -> u64 {
        8 + 64 + 4 + 64 * self.siblings.len() as u64
    }
}

/// **A contiguous range of a hiding value's leaf hashes with its Merkle range proof** — one served slice of a hiding value (leaf-hash
/// DA): the producer owes the hashes, never the elements.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct HidingHashRangeV1 {
    pub first: u64,
    pub leaves: Vec<Digest>,
    pub siblings: Vec<Digest>,
}

impl HidingHashRangeV1 {
    /// Leaves `[first, first + count)` of `tree`.
    pub fn of(tree: &HidingTreeV1, first: u64, count: u64) -> Option<Self> {
        let end = first.checked_add(count)?;
        if count == 0 || end > tree.leaves.len() as u64 {
            return None;
        }
        Some(Self {
            first,
            leaves: tree.leaves[first as usize..end as usize].to_vec(),
            siblings: range_siblings(tree.leaves.clone(), first as usize, end as usize, hide_node_hash_v1),
        })
    }

    /// The commitment of the value of `(dtype, shape)` these leaf hashes are a range of (`None`: malformed).
    pub fn recomputed_commitment(&self, dtype: DType, shape: &[usize]) -> Option<Digest> {
        let total = hide_tiles_v1(len_of(shape)?);
        let root = range_root(&self.leaves, self.first, total, &self.siblings, hide_node_hash_v1)?;
        Some(hide_commit_v1(dtype, shape, &root))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(dtype: DType, shape: &[usize]) -> Tensor {
        let n: usize = shape.iter().product();
        Tensor::new(dtype, shape.to_vec(), (0..n as i128).map(|v| (v * 7 + 3) % 101 - 50).collect()).unwrap()
    }

    #[test]
    fn every_tile_holds_64_to_127_elements_and_short_lines_never_make_a_short_leaf() {
        // wide128's d = 16 and dense-MoE's rows of 4: a [H, 16] or [H, 2, 4] value is tiled in row-major order across its lines.
        for (len, tiles) in [(0u64, 0u64), (1, 1), (16, 1), (63, 1), (64, 1), (127, 1), (128, 2), (191, 2), (192, 3), (4096, 64)] {
            assert_eq!(hide_tiles_v1(len), tiles, "{len}");
            let mut covered = 0;
            for tile in 0..tiles {
                let (a, b) = hide_tile_range_v1(len, tile).unwrap();
                assert_eq!(a, covered, "contiguous");
                covered = b;
                if len >= HIDE_TILE_V1 {
                    assert!((HIDE_TILE_V1..=HIDE_TILE_MAX_V1).contains(&(b - a)), "{len} tile {tile}: {}", b - a);
                }
            }
            assert_eq!(covered, len);
            for e in 0..len {
                let (a, b) = hide_tile_range_v1(len, hide_tile_of_v1(len, e).unwrap()).unwrap();
                assert!(a <= e && e < b);
            }
        }
        // A window of 16-element rows (wide128's d) at H = 4..=40: every leaf spans at least four rows.
        for h in 4..=40usize {
            let x = t(DType::I8, &[h, 16]);
            let tree = HidingTreeV1::of(&x);
            for tile in 0..tree.leaves.len() as u64 {
                let o = HidingLeafOpeningV1::of(&x, &tree, tile).unwrap();
                assert!(o.bytes.len() >= 64, "H = {h}: a leaf of {} elements", o.bytes.len());
            }
        }
    }

    #[test]
    fn every_tile_opens_against_the_hiding_commitment_and_nothing_else_does() {
        for (dtype, shape) in [
            (DType::I32, vec![5]),
            (DType::I8, vec![64]),
            (DType::I16, vec![3, 5, 13]),
            (DType::I8, vec![7, 16]),
            (DType::I64, vec![2, 2, 4]),
            (DType::I32, vec![1, 4096]),
            (DType::I8, vec![9001]),
        ] {
            let x = t(dtype, &shape);
            let tree = HidingTreeV1::of(&x);
            let c = tree.commitment();
            assert_eq!(c, tensor_commitment_hide_v1(&x));
            assert_ne!(c, crate::merkle3::tensor_commitment_v3(&x), "a hiding commitment is never a v3 one");
            assert_eq!(hide_commitment_of_leaves_v1(dtype, &shape, &tree.leaves), Some(c));
            for tile in 0..tree.leaves.len() as u64 {
                let o = HidingLeafOpeningV1::of(&x, &tree, tile).unwrap();
                assert!(o.authenticates(dtype, &shape, &c), "{shape:?} {tile}");
                for (e, v) in o.elements(dtype, &shape).unwrap() {
                    assert_eq!(x.data[e as usize], v);
                }
                let mut bad = o.clone();
                bad.bytes[0] ^= 1;
                assert!(!bad.authenticates(dtype, &shape, &c));
                let h = HidingLeafHashV1::of_leaves(&tree.leaves, tile).unwrap();
                assert!(h.authenticates(dtype, &shape, &c));
                let mut wrong = h.clone();
                wrong.leaf[0] ^= 1;
                assert!(!wrong.authenticates(dtype, &shape, &c));
                let mut other = shape.clone();
                other[0] += 1;
                assert!(!h.authenticates(dtype, &other, &c), "the type is bound");
                assert!(o.byte_len() <= hiding_leaf_opening_bytes_v1(x.data.len() as u64, dtype.width() as u64));
                assert_eq!(borsh::to_vec(&o).unwrap().len() as u64, o.byte_len());
            }
            let total = tree.leaves.len() as u64;
            for a in 0..total {
                for b in a + 1..=total.min(a + 9) {
                    let r = HidingHashRangeV1::of(&tree, a, b - a).unwrap();
                    assert_eq!(r.recomputed_commitment(dtype, &shape), Some(c), "{shape:?} [{a}, {b})");
                    let mut bad = r.clone();
                    bad.leaves[0][0] ^= 1;
                    assert_ne!(bad.recomputed_commitment(dtype, &shape), Some(c));
                }
            }
        }
    }
}
