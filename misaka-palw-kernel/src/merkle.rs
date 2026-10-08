//! **Row/column Merkle commitments of node values** — the commitment suite that lets a court open part of a tensor.
//!
//! A tensor's commitment binds its dtype, its shape and two Merkle roots over the same elements:
//!
//! * the **row root** — one leaf per row along the last axis (`len / last` leaves of `last` elements);
//! * the **column root** — one leaf per column of every `[m, n]` matrix of the last two axes (`batch · n` leaves of `m`
//!   elements, leaf `b·n + j` holding `t[b, :, j]`).
//!
//! A rank-0 or rank-1 tensor has one leaf in each tree (all its elements). A full opening recomputes both roots from the whole
//! tensor; a **partial opening** ([`TensorOpeningV1`]) is one row or one column with its Merkle path, plus the other root, and
//! authenticates against the same commitment. A `MatMul` scalar court therefore opens one row of `X`, one column of `W` and
//! one row of `Y` — `O(k + n)` elements and `O(log)` hashes — instead of the three tensors (RFC-0011 §15.5's bounded
//! localization bytes).
//!
//! Trees are binary over the leaves in order; an unpaired last node is carried up unchanged. Leaf and node hashes are
//! domain-separated, and a leaf binds its axis and index, so a row cannot be presented as a column or as another row.

use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::{DType, Tensor};

use crate::hash::{Digest, finish, keyed};

pub const TENSOR_COMMITMENT_DOMAIN_V2: &[u8] = b"misaka-palw/kernel/tensor/v2";
pub const TENSOR_LEAF_DOMAIN_V2: &[u8] = b"misaka-palw/kernel/tensor-leaf/v2";
pub const TENSOR_NODE_DOMAIN_V2: &[u8] = b"misaka-palw/kernel/tensor-node/v2";

/// Row leaves (axis 0) or column leaves (axis 1).
pub const AXIS_ROW: u8 = 0;
pub const AXIS_COL: u8 = 1;

/// The leaf layout of a shape: `(rows, row_len, cols, col_len, m, n)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutV1 {
    pub rows: u64,
    pub row_len: u64,
    pub cols: u64,
    pub col_len: u64,
    /// The last two axes (`m = 1` for rank ≤ 1).
    pub m: u64,
    pub n: u64,
}

impl LayoutV1 {
    /// The layout of a constructed tensor (whose element count fits a `usize`, hence a `u64`).
    pub fn of(shape: &[usize]) -> Self {
        Self::try_of(shape).expect("a constructed tensor's element count fits a u64")
    }

    /// The layout of a shape that may come from a wire or a filing: `None` when its element count does not fit a `u64` (a malformed
    /// opening, never a panic). Identical to [`Self::of`] wherever that does not overflow.
    pub fn try_of(shape: &[usize]) -> Option<Self> {
        let dims: Vec<u64> = shape.iter().map(|d| *d as u64).collect();
        let len: u64 = if dims.contains(&0) { 0 } else { dims.iter().try_fold(1u64, |acc, d| acc.checked_mul(*d))? };
        Some(match dims.len() {
            0 | 1 => LayoutV1 { rows: 1, row_len: len, cols: 1, col_len: len, m: 1, n: len },
            r => {
                let (m, n) = (dims[r - 2], dims[r - 1]);
                // `len == 0` covers every zero dimension; otherwise `m · n` divides `len`, so it fits.
                let batch = if len == 0 { 0 } else { len / (m * n) };
                LayoutV1 { rows: if n == 0 { 0 } else { len / n }, row_len: n, cols: batch * n, col_len: m, m, n }
            }
        })
    }

    fn rank_le_1(shape: &[usize]) -> bool {
        shape.len() <= 1
    }
}

fn leaf_hash(dtype: DType, axis: u8, index: u64, values: &[i128]) -> Digest {
    let mut s = keyed(TENSOR_LEAF_DOMAIN_V2);
    s.update(&[dtype.tag(), axis]).update(&index.to_le_bytes()).update(&(values.len() as u64).to_le_bytes());
    let w = dtype.width();
    for v in values {
        s.update(&v.to_le_bytes()[..w]);
    }
    finish(s)
}

fn node_hash(l: &Digest, r: &Digest) -> Digest {
    let mut s = keyed(TENSOR_NODE_DOMAIN_V2);
    s.update(l).update(r);
    finish(s)
}

fn root_of(mut level: Vec<Digest>) -> Digest {
    if level.is_empty() {
        return finish(keyed(TENSOR_NODE_DOMAIN_V2));
    }
    while level.len() > 1 {
        level = level.chunks(2).map(|c| if c.len() == 2 { node_hash(&c[0], &c[1]) } else { c[0] }).collect();
    }
    level[0]
}

/// The siblings of leaf `index` among `leaves`, bottom up (an unpaired level contributes none).
fn path_of(mut level: Vec<Digest>, mut index: usize) -> Vec<Digest> {
    let mut out = Vec::new();
    while level.len() > 1 {
        let sib = index ^ 1;
        if sib < level.len() {
            out.push(level[sib]);
        }
        level = level.chunks(2).map(|c| if c.len() == 2 { node_hash(&c[0], &c[1]) } else { c[0] }).collect();
        index >>= 1;
    }
    out
}

/// Recompute a root from a leaf, its index among `count` leaves, and its siblings. `None` on a malformed path.
fn root_from_path(leaf: Digest, mut index: u64, mut count: u64, siblings: &[Digest]) -> Option<Digest> {
    if index >= count {
        return None;
    }
    let mut cur = leaf;
    let mut it = siblings.iter();
    while count > 1 {
        let sib = index ^ 1;
        if sib < count {
            let s = it.next()?;
            cur = if index & 1 == 0 { node_hash(&cur, s) } else { node_hash(s, &cur) };
        }
        index >>= 1;
        count = count.div_ceil(2);
    }
    it.next().is_none().then_some(cur)
}

/// Row `r`'s elements.
fn row_values(t: &Tensor, l: &LayoutV1, r: u64) -> Vec<i128> {
    let (a, n) = ((r * l.row_len) as usize, l.row_len as usize);
    t.data[a..a + n].to_vec()
}

/// Column leaf `c`'s elements (`t[b, :, j]`, or the whole tensor at rank ≤ 1).
fn col_values(t: &Tensor, l: &LayoutV1, c: u64) -> Vec<i128> {
    if LayoutV1::rank_le_1(&t.shape) {
        return t.data.clone();
    }
    let (b, j) = (c / l.n, c % l.n);
    (0..l.m).map(|i| t.data[(b * l.m * l.n + i * l.n + j) as usize]).collect()
}

fn leaves(t: &Tensor, l: &LayoutV1, axis: u8) -> Vec<Digest> {
    if axis == AXIS_ROW {
        (0..l.rows).map(|r| leaf_hash(t.dtype, AXIS_ROW, r, &row_values(t, l, r))).collect()
    } else {
        (0..l.cols).map(|c| leaf_hash(t.dtype, AXIS_COL, c, &col_values(t, l, c))).collect()
    }
}

fn commit(dtype: DType, shape: &[usize], row_root: &Digest, col_root: &Digest) -> Digest {
    let mut s = keyed(TENSOR_COMMITMENT_DOMAIN_V2);
    s.update(&[dtype.tag(), shape.len() as u8]);
    for d in shape {
        s.update(&(*d as u64).to_le_bytes());
    }
    s.update(row_root).update(col_root);
    finish(s)
}

/// The two roots of a tensor.
pub fn roots(t: &Tensor) -> (Digest, Digest) {
    let l = LayoutV1::of(&t.shape);
    (root_of(leaves(t, &l, AXIS_ROW)), root_of(leaves(t, &l, AXIS_COL)))
}

/// **The commitment of a node value**: `H(dtype ‖ rank ‖ dims ‖ row root ‖ column root)`.
pub fn tensor_commitment_v2(t: &Tensor) -> Digest {
    let (r, c) = roots(t);
    commit(t.dtype, &t.shape, &r, &c)
}

/// **One row or one column of a committed tensor**, with its Merkle path and the other tree's root.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TensorOpeningV1 {
    pub dtype: u8,
    pub shape: Vec<u64>,
    /// [`AXIS_ROW`] or [`AXIS_COL`].
    pub axis: u8,
    pub index: u64,
    pub values: Vec<i128>,
    pub siblings: Vec<Digest>,
    /// The root of the tree this opening is NOT in.
    pub other_root: Digest,
}

impl TensorOpeningV1 {
    fn open(t: &Tensor, axis: u8, index: u64) -> Option<Self> {
        let l = LayoutV1::of(&t.shape);
        let count = if axis == AXIS_ROW { l.rows } else { l.cols };
        if index >= count {
            return None;
        }
        let (rl, cl) = (leaves(t, &l, AXIS_ROW), leaves(t, &l, AXIS_COL));
        let (mine, other) = if axis == AXIS_ROW { (rl, cl) } else { (cl, rl) };
        Some(TensorOpeningV1 {
            dtype: t.dtype.tag(),
            shape: t.shape.iter().map(|d| *d as u64).collect(),
            axis,
            index,
            values: if axis == AXIS_ROW { row_values(t, &l, index) } else { col_values(t, &l, index) },
            siblings: path_of(mine, index as usize),
            other_root: root_of(other),
        })
    }

    /// Row `index` (along the last axis).
    pub fn row(t: &Tensor, index: u64) -> Option<Self> {
        Self::open(t, AXIS_ROW, index)
    }

    /// Column leaf `index` (`b·n + j`).
    pub fn col(t: &Tensor, index: u64) -> Option<Self> {
        Self::open(t, AXIS_COL, index)
    }

    pub fn dtype(&self) -> Option<DType> {
        DType::ALL.into_iter().find(|d| d.tag() == self.dtype)
    }

    pub fn shape_usize(&self) -> Option<Vec<usize>> {
        self.shape.iter().map(|d| usize::try_from(*d).ok()).collect()
    }

    /// **Does this opening belong to `commitment`?** Recomputes the opened tree's root from the leaf and the path, then the
    /// commitment from both roots. Every element must lie in the dtype.
    pub fn authenticates(&self, commitment: &Digest) -> bool {
        self.recomputed_commitment().is_some_and(|c| c == *commitment)
    }

    /// The commitment this opening is of, if it is a well-formed opening of SOME tensor (`None`: a malformed or fake opening —
    /// unknown dtype, wrong length, an element outside the dtype, an index or path that fits no tree).
    pub fn recomputed_commitment(&self) -> Option<Digest> {
        let (dtype, shape) = (self.dtype()?, self.shape_usize()?);
        let l = LayoutV1::try_of(&shape)?;
        let (count, len) = match self.axis {
            AXIS_ROW => (l.rows, l.row_len),
            AXIS_COL => (l.cols, l.col_len),
            _ => return None,
        };
        if self.values.len() as u64 != len || self.values.iter().any(|v| !dtype.contains(*v)) {
            return None;
        }
        let leaf = leaf_hash(dtype, self.axis, self.index, &self.values);
        let root = root_from_path(leaf, self.index, count, &self.siblings)?;
        let (r, c) = if self.axis == AXIS_ROW { (root, self.other_root) } else { (self.other_root, root) };
        Some(commit(dtype, &shape, &r, &c))
    }

    /// Bytes this opening puts in a court (elements at their width, path and the other root).
    pub fn byte_len(&self) -> u64 {
        let w = self.dtype().map(|d| d.width()).unwrap_or(16) as u64;
        self.values.len() as u64 * w + (self.siblings.len() as u64 + 1) * 64 + 8 * self.shape.len() as u64 + 10
    }
}

/// The Merkle depth of `count` leaves (siblings at most).
pub fn depth(count: u64) -> u64 {
    if count <= 1 { 0 } else { 64 - (count - 1).leading_zeros() as u64 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(shape: &[usize]) -> Tensor {
        let n: usize = shape.iter().product();
        Tensor::new(DType::I32, shape.to_vec(), (0..n as i128).map(|v| v * 7 - 50).collect()).unwrap()
    }

    #[test]
    fn an_opening_whose_shape_overflows_is_malformed_never_a_panic() {
        for shape in [vec![u64::MAX, 2], vec![0, 1 << 40, 1 << 40], vec![1 << 32, 1 << 32, 2], vec![u64::MAX, u64::MAX, 0]] {
            let o = TensorOpeningV1 {
                dtype: DType::I32.tag(),
                shape: shape.clone(),
                axis: AXIS_ROW,
                index: 0,
                values: vec![],
                siblings: vec![],
                other_root: [0; 64],
            };
            assert!(o.recomputed_commitment().is_none() || shape.contains(&0), "{shape:?}");
        }
        assert_eq!(LayoutV1::try_of(&[0, 1 << 40, 1 << 40]).map(|l| (l.rows, l.cols)), Some((0, 0)));
        assert!(LayoutV1::try_of(&[usize::MAX, 2]).is_none());
        assert_eq!(LayoutV1::try_of(&[3, 4]), Some(LayoutV1::of(&[3, 4])));
    }

    #[test]
    fn every_row_and_column_opens_against_the_commitment_and_nothing_else_does() {
        for shape in [vec![], vec![5], vec![3, 4], vec![2, 3, 5], vec![1, 7], vec![6, 1], vec![2, 2, 2, 3]] {
            let x = t(&shape);
            let c = tensor_commitment_v2(&x);
            let l = LayoutV1::of(&x.shape);
            for r in 0..l.rows {
                let o = TensorOpeningV1::row(&x, r).unwrap();
                assert!(o.authenticates(&c), "{shape:?} row {r}");
                let mut bad = o.clone();
                bad.values[0] += 1;
                assert!(!bad.authenticates(&c));
                let mut moved = o.clone();
                moved.index = (r + 1) % l.rows.max(1);
                if l.rows > 1 {
                    assert!(!moved.authenticates(&c), "a row cannot pose as another");
                }
            }
            for col in 0..l.cols {
                let o = TensorOpeningV1::col(&x, col).unwrap();
                assert!(o.authenticates(&c), "{shape:?} col {col}");
                let mut as_row = o.clone();
                as_row.axis = AXIS_ROW;
                if l.row_len != l.col_len || l.rows > 1 {
                    assert!(!as_row.authenticates(&c), "a column cannot pose as a row");
                }
            }
            assert!(TensorOpeningV1::row(&x, l.rows).is_none());
        }
    }

    #[test]
    fn the_commitment_binds_dtype_shape_and_every_element() {
        let a = t(&[3, 4]);
        let mut b = a.clone();
        b.data[5] += 1;
        assert_ne!(tensor_commitment_v2(&a), tensor_commitment_v2(&b));
        let mut c = a.clone();
        c.shape = vec![4, 3];
        assert_ne!(tensor_commitment_v2(&a), tensor_commitment_v2(&c));
        assert_eq!(depth(1), 0);
        assert_eq!(depth(5), 3);
    }
}
