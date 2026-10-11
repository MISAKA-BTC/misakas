//! Flat commitment v4 for the staged 64-element commitment-only court contract.
//!
//! This is a separate hash/wire grammar. No v3 commitment, execution descriptor,
//! class, candidate statement or reward path selects it yet. Every served leaf
//! contains 64..=127 elements: a short final tail joins the preceding full tile.
//! Rank/line boundaries cannot create a small exposed leaf. This is a structural
//! width rule, not a claim of entropy or cryptographic hiding of predictable data.
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::{DType, Tensor};
use std::io::{Cursor, Error, ErrorKind, Read};

use crate::hash::{Digest, finish, keyed};
use crate::merkle3::tree_root_from_path;

pub const FLAT_LEAF_ELEMENTS_V4: u64 = 64;
pub const FLAT_LEAF_MAX_ELEMENTS_V4: usize = 127;
pub const FLAT_HASH_MAX_BYTES_V4: usize = 1 + 4 + 8 * 4 + 8 + 64 + 4 + 64 * 64;
pub const FLAT_VALUE_MAX_BYTES_V4: usize = FLAT_HASH_MAX_BYTES_V4 + 4 + FLAT_LEAF_MAX_ELEMENTS_V4 * 4;
pub const FLAT_COMMITMENT_DOMAIN_V4: &[u8] = b"misaka-palw/kernel/tensor/flat/v4";
pub const FLAT_LEAF_DOMAIN_V4: &[u8] = b"misaka-palw/kernel/tensor-leaf/flat/v4";
pub const FLAT_NODE_DOMAIN_V4: &[u8] = b"misaka-palw/kernel/tensor-node/flat/v4";

fn invalid() -> Error {
    Error::new(ErrorKind::InvalidData, "invalid bounded flat-v4 opening")
}
fn allowed<const V: u8>(d: DType) -> bool {
    V == 5 || (V == 4 && d.committable())
}
fn dtype<const V: u8>(tag: u8) -> Option<DType> {
    DType::ALL.into_iter().find(|d| d.tag() == tag && allowed::<V>(*d))
}
fn domain<const V: u8>(v4: &'static [u8], v5: &'static [u8]) -> &'static [u8] {
    if V == 5 { v5 } else { v4 }
}
pub type FlatTreeV4 = FlatTree<4>;
pub type FlatLeafHashV4 = FlatLeafHash<4>;
pub type FlatLeafValueV4 = FlatLeafValue<4>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlatLayoutV4 {
    elements: u64,
    leaves: u64,
    rank: usize,
}
impl FlatLayoutV4 {
    pub fn elements(&self) -> u64 {
        self.elements
    }
    pub fn leaves(&self) -> u64 {
        self.leaves
    }
    /// Small tensors use a whole-value contract instead; no sub-width leaf exists.
    pub fn of(shape: &[u64]) -> Option<Self> {
        if shape.len() > misaka_palw_tir::types::MAX_RANK || shape.contains(&0) {
            return None;
        }
        let elements = shape.iter().try_fold(1u64, |n, d| n.checked_mul(*d))?;
        (elements >= FLAT_LEAF_ELEMENTS_V4).then_some(Self { elements, leaves: elements / FLAT_LEAF_ELEMENTS_V4, rank: shape.len() })
    }
    pub fn span(&self, index: u64) -> Option<(u64, usize)> {
        if index >= self.leaves {
            return None;
        }
        let first = index.checked_mul(FLAT_LEAF_ELEMENTS_V4)?;
        let count = if index + 1 == self.leaves { self.elements.checked_sub(first)? } else { FLAT_LEAF_ELEMENTS_V4 };
        let count = usize::try_from(count).ok()?;
        (count >= FLAT_LEAF_ELEMENTS_V4 as usize && count <= FLAT_LEAF_MAX_ELEMENTS_V4).then_some((first, count))
    }
    pub fn leaf_of(&self, element: u64) -> Option<u64> {
        (element < self.elements).then(|| (element / FLAT_LEAF_ELEMENTS_V4).min(self.leaves - 1))
    }
    pub fn bounds(&self, dtype: DType) -> Option<FlatMaterialBoundsV4> {
        self.bounds_for::<4>(dtype)
    }
    pub(crate) fn bounds_for<const V: u8>(&self, dtype: DType) -> Option<FlatMaterialBoundsV4> {
        let rank = self.rank;
        if !allowed::<V>(dtype) {
            return None;
        }
        let mut nodes = 0u128;
        let mut width = self.leaves;
        let mut levels = 0u64;
        loop {
            nodes += width as u128;
            levels += 1;
            if width == 1 {
                break;
            }
            width = width.div_ceil(2);
        }
        let max_elements = self.span(self.leaves - 1)?.1 as u64;
        let hash_bytes = 1 + 4 + rank as u64 * 8 + 8 + 64 + 4 + crate::merkle::depth(self.leaves) * 64;
        Some(FlatMaterialBoundsV4 {
            source_bytes: self.elements as u128 * dtype.width() as u128,
            leaf_elements: max_elements,
            leaf_hash_bytes: hash_bytes,
            leaf_value_bytes: hash_bytes + 4 + max_elements * dtype.width() as u64,
            retained_tree_bytes: nodes * 64 + levels as u128 * 24 + 128,
            // The streaming root keeps a 64-level stack and one <=127-element
            // raw-width buffer. It never retains a full tensor or Merkle tree.
            streaming_scratch_bytes: 8192,
            hash_input_bytes: self.elements as u128 * dtype.width() as u128
                + self.leaves as u128 * (1 + 8 + 8)
                + self.leaves.saturating_sub(1) as u128 * 128
                + 2
                + rank as u128 * 8
                + 64,
            localization_replies: crate::merkle::depth(self.leaves) + 1,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlatMaterialBoundsV4 {
    pub source_bytes: u128,
    pub leaf_elements: u64,
    pub leaf_hash_bytes: u64,
    pub leaf_value_bytes: u64,
    pub retained_tree_bytes: u128,
    pub streaming_scratch_bytes: u64,
    pub hash_input_bytes: u128,
    /// Includes the initial probe and the terminal divergent-leaf reply.
    pub localization_replies: u64,
}

fn leaf_state<const V: u8>(dtype: DType, index: u64, count: usize) -> blake2b_simd::State {
    let mut s = keyed(domain::<V>(FLAT_LEAF_DOMAIN_V4, b"misaka-palw/kernel/tensor-leaf/flat/v5"));
    s.update(&[dtype.tag()]).update(&index.to_le_bytes()).update(&(count as u64).to_le_bytes());
    s
}
fn node_hash<const V: u8>(left: &Digest, right: &Digest) -> Digest {
    let mut s = keyed(domain::<V>(FLAT_NODE_DOMAIN_V4, b"misaka-palw/kernel/tensor-node/flat/v5"));
    s.update(left).update(right);
    finish(s)
}
fn commitment<const V: u8>(dtype: DType, shape: &[u64], root: &Digest) -> Digest {
    let mut s = keyed(domain::<V>(FLAT_COMMITMENT_DOMAIN_V4, b"misaka-palw/kernel/tensor/flat/v5"));
    s.update(&[dtype.tag(), shape.len() as u8]);
    for d in shape {
        s.update(&d.to_le_bytes());
    }
    s.update(root);
    finish(s)
}

/// Typed header authentication without allocating a tensor. Malformed-profile
/// courts also need to authenticate zero/small metadata before judging its type.
pub(crate) fn header_commitment<const V: u8>(tag: u8, shape: &[u64], root: &Digest) -> Option<Digest> {
    let d = dtype::<V>(tag)?;
    if shape.len() > 4 {
        return None;
    }
    Some(commitment::<V>(d, shape, root))
}

/// Root-only hashing of a borrowed raw-width image. Exact shape/byte length is
/// checked before any leaf work; hashing uses constant scratch, including ragged tails.
pub fn flat_commitment_from_bytes_v4(dtype: DType, shape: &[u64], bytes: &[u8]) -> Result<Digest, Error> {
    flat_commitment_from_bytes::<4>(dtype, shape, bytes)
}
pub(crate) fn flat_commitment_from_bytes<const V: u8>(dtype: DType, shape: &[u64], bytes: &[u8]) -> Result<Digest, Error> {
    let l = FlatLayoutV4::of(shape).ok_or_else(invalid)?;
    if !allowed::<V>(dtype) || l.elements.checked_mul(dtype.width() as u64) != Some(bytes.len() as u64) {
        return Err(invalid());
    }
    let mut stack = [None; 64];
    for index in 0..l.leaves {
        let (first, count) = l.span(index).ok_or_else(invalid)?;
        let start = first as usize * dtype.width();
        let mut s = leaf_state::<V>(dtype, index, count);
        s.update(&bytes[start..start + count * dtype.width()]);
        let mut hash = finish(s);
        let mut level = 0;
        while let Some(left) = stack[level].take() {
            hash = node_hash::<V>(&left, &hash);
            level += 1;
        }
        stack[level] = Some(hash);
    }
    let mut root = None;
    for left in stack.into_iter().flatten() {
        root = Some(match root {
            None => left,
            Some(right) => node_hash::<V>(&left, &right),
        });
    }
    Ok(commitment::<V>(dtype, shape, &root.ok_or_else(invalid)?))
}

/// Root-only hashing of an already resident reference tensor, using the same
/// fixed stack and raw-width leaf buffer; it does not retain an additional tree
/// or allocate a full raw-width copy of the tensor.
pub fn flat_commitment_from_tensor_v4(t: &Tensor) -> Result<Digest, Error> {
    flat_commitment_from_tensor::<4>(t)
}
pub(crate) fn flat_commitment_from_tensor<const V: u8>(t: &Tensor) -> Result<Digest, Error> {
    if t.shape.len() > misaka_palw_tir::types::MAX_RANK || !allowed::<V>(t.dtype) {
        return Err(invalid());
    }
    let shape: Vec<_> = t.shape.iter().map(|d| *d as u64).collect();
    let l = FlatLayoutV4::of(&shape).ok_or_else(invalid)?;
    if l.elements != t.data.len() as u64 {
        return Err(invalid());
    }
    let mut stack = [None; 64];
    let mut bytes = Vec::with_capacity(FLAT_LEAF_MAX_ELEMENTS_V4 * t.dtype.width());
    for index in 0..l.leaves {
        let (first, count) = l.span(index).ok_or_else(invalid)?;
        bytes.clear();
        for v in &t.data[first as usize..first as usize + count] {
            if !t.dtype.contains(*v) {
                return Err(invalid());
            }
            t.dtype.encode_le(*v, &mut bytes);
        }
        let mut s = leaf_state::<V>(t.dtype, index, count);
        s.update(&bytes);
        let mut hash = finish(s);
        let mut level = 0;
        while let Some(left) = stack[level].take() {
            hash = node_hash::<V>(&left, &hash);
            level += 1;
        }
        stack[level] = Some(hash);
    }
    let mut root = None;
    for left in stack.into_iter().flatten() {
        root = Some(match root {
            None => left,
            Some(right) => node_hash::<V>(&left, &right),
        });
    }
    Ok(commitment::<V>(t.dtype, &shape, &root.ok_or_else(invalid)?))
}

/// A model holder/producer may retain the priced tree to serve/localize paths.
/// Path reads visit one digest per level; no full tree is copied per probe.
pub struct FlatTree<const V: u8> {
    dtype: DType,
    shape: Vec<u64>,
    layout: FlatLayoutV4,
    levels: Vec<Vec<Digest>>,
}
impl<const V: u8> FlatTree<V> {
    pub fn from_bytes(dtype: DType, shape: &[u64], bytes: &[u8]) -> Result<Self, Error> {
        Self::from_bytes_bounded(dtype, shape, bytes, u128::MAX)
    }
    /// Check the caller's tree-retention budget before hashing or allocating a
    /// digest array. A root-only caller can use the fixed-scratch function above.
    pub fn from_bytes_bounded(dtype: DType, shape: &[u64], bytes: &[u8], retained_limit: u128) -> Result<Self, Error> {
        let layout = FlatLayoutV4::of(shape).ok_or_else(invalid)?;
        if !allowed::<V>(dtype) || layout.elements.checked_mul(dtype.width() as u64) != Some(bytes.len() as u64) {
            return Err(invalid());
        }
        if layout.bounds_for::<V>(dtype).ok_or_else(invalid)?.retained_tree_bytes > retained_limit {
            return Err(invalid());
        }
        let mut leaves = Vec::new();
        leaves.try_reserve_exact(usize::try_from(layout.leaves).map_err(|_| invalid())?).map_err(|_| invalid())?;
        for index in 0..layout.leaves {
            let (first, count) = layout.span(index).ok_or_else(invalid)?;
            let start = first as usize * dtype.width();
            let mut s = leaf_state::<V>(dtype, index, count);
            s.update(&bytes[start..start + count * dtype.width()]);
            leaves.push(finish(s));
        }
        Self::from_leaves(dtype, shape, layout, leaves)
    }
    pub fn of(t: &Tensor) -> Result<Self, Error> {
        Self::of_bounded(t, u128::MAX)
    }
    pub fn of_bounded(t: &Tensor, retained_limit: u128) -> Result<Self, Error> {
        if t.shape.len() > misaka_palw_tir::types::MAX_RANK || !allowed::<V>(t.dtype) {
            return Err(invalid());
        }
        let shape: Vec<_> = t.shape.iter().map(|d| *d as u64).collect();
        let layout = FlatLayoutV4::of(&shape).ok_or_else(invalid)?;
        if !allowed::<V>(t.dtype) || layout.elements != t.data.len() as u64 {
            return Err(invalid());
        }
        if layout.bounds_for::<V>(t.dtype).ok_or_else(invalid)?.retained_tree_bytes > retained_limit {
            return Err(invalid());
        }
        let mut leaves = Vec::new();
        leaves.try_reserve_exact(usize::try_from(layout.leaves).map_err(|_| invalid())?).map_err(|_| invalid())?;
        let mut bytes = Vec::with_capacity(FLAT_LEAF_MAX_ELEMENTS_V4 * t.dtype.width());
        for index in 0..layout.leaves {
            let (first, count) = layout.span(index).ok_or_else(invalid)?;
            bytes.clear();
            for v in &t.data[first as usize..first as usize + count] {
                if !t.dtype.contains(*v) {
                    return Err(invalid());
                }
                t.dtype.encode_le(*v, &mut bytes);
            }
            let mut s = leaf_state::<V>(t.dtype, index, count);
            s.update(&bytes);
            leaves.push(finish(s));
        }
        Self::from_leaves(t.dtype, &shape, layout, leaves)
    }
    fn from_leaves(dtype: DType, shape: &[u64], layout: FlatLayoutV4, leaves: Vec<Digest>) -> Result<Self, Error> {
        let mut levels = Vec::new();
        levels.try_reserve_exact(crate::merkle::depth(layout.leaves) as usize + 1).map_err(|_| invalid())?;
        levels.push(leaves);
        while levels.last().ok_or_else(invalid)?.len() > 1 {
            let mut next = Vec::new();
            next.try_reserve_exact(levels.last().unwrap().len().div_ceil(2)).map_err(|_| invalid())?;
            for p in levels.last().unwrap().chunks(2) {
                next.push(if p.len() == 2 { node_hash::<V>(&p[0], &p[1]) } else { p[0] });
            }
            levels.push(next);
        }
        Ok(Self { dtype, shape: shape.to_vec(), layout, levels })
    }
    pub fn root(&self) -> Digest {
        self.levels.last().unwrap()[0]
    }
    pub fn commitment(&self) -> Digest {
        commitment::<V>(self.dtype, &self.shape, &self.root())
    }
    pub fn layout(&self) -> FlatLayoutV4 {
        self.layout
    }
    pub fn opening(&self, index: u64) -> Option<FlatLeafHash<V>> {
        self.layout.span(index)?;
        let hash = self.levels[0][usize::try_from(index).ok()?];
        let mut at = index as usize;
        let mut siblings = Vec::new();
        for level in &self.levels {
            if let Some(s) = level.get(at ^ 1) {
                siblings.push(*s);
            }
            at >>= 1;
        }
        Some(FlatLeafHash { dtype: self.dtype.tag(), shape: self.shape.clone(), index, hash, siblings })
    }
}

/// Public structure only, no elements. Metadata and position are bound through
/// the v4 leaf domain and the typed tensor commitment; the path is exact.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatLeafHash<const V: u8> {
    pub dtype: u8,
    pub shape: Vec<u64>,
    pub index: u64,
    pub hash: Digest,
    pub siblings: Vec<Digest>,
}
impl<const V: u8> FlatLeafHash<V> {
    pub fn layout(&self) -> Option<FlatLayoutV4> {
        FlatLayoutV4::of(&self.shape)
    }
    pub fn recomputed_commitment(&self) -> Option<Digest> {
        let dtype = dtype::<V>(self.dtype)?;
        let l = self.layout()?;
        l.span(self.index)?;
        if self.siblings.len() > 64 {
            return None;
        }
        let root = tree_root_from_path(self.hash, self.index, l.leaves, &self.siblings, node_hash::<V>)?;
        Some(commitment::<V>(dtype, &self.shape, &root))
    }
    pub fn authenticates(&self, root: &Digest) -> bool {
        self.recomputed_commitment().is_some_and(|r| r == *root)
    }
    /// Evaluate a future leaf relation with the same leaf preimage. The caller
    /// supplies exactly the declared span; a truncated/surplus value is refused.
    pub fn matches_bytes(&self, bytes: &[u8]) -> bool {
        let Some(dtype) = dtype::<V>(self.dtype) else {
            return false;
        };
        let Some((_, count)) = self.layout().and_then(|l| l.span(self.index)) else {
            return false;
        };
        if bytes.len() != count * dtype.width() {
            return false;
        }
        let mut s = leaf_state::<V>(dtype, self.index, count);
        s.update(bytes);
        finish(s) == self.hash
    }
    /// A public reply is authenticated before comparing it with the model
    /// holder's expected tree. Bottom-up sibling comparisons select a strictly
    /// smaller divergent branch, including carried odd leaves.
    pub fn descend(&self, actual: &Digest, expected: &FlatTree<V>) -> Result<FlatDescentV4, Error> {
        if !self.authenticates(actual) || self.dtype != expected.dtype.tag() || self.shape != expected.shape {
            return Err(invalid());
        }
        if *actual == expected.commitment() {
            return Ok(FlatDescentV4::Matched);
        }
        let own = expected.opening(self.index).ok_or_else(invalid)?;
        if self.hash != own.hash {
            return Ok(FlatDescentV4::DivergentLeaf(self.index));
        }
        let mut index = self.index;
        let mut count = expected.layout.leaves;
        let mut sibling = 0;
        let mut level = 0;
        while count > 1 {
            let other = index ^ 1;
            if other < count {
                if self.siblings[sibling] != own.siblings[sibling] {
                    return Ok(FlatDescentV4::Probe(other.checked_shl(level).ok_or_else(invalid)?));
                }
                sibling += 1;
            }
            index >>= 1;
            count = count.div_ceil(2);
            level += 1;
        }
        Err(invalid())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlatDescentV4 {
    Matched,
    Probe(u64),
    DivergentLeaf(u64),
}

impl<const V: u8> BorshDeserialize for FlatLeafHash<V> {
    fn deserialize_reader<R: Read>(r: &mut R) -> std::io::Result<Self> {
        let tag = u8::deserialize_reader(r)?;
        dtype::<V>(tag).ok_or_else(invalid)?;
        let rank = u32::deserialize_reader(r)? as usize;
        if rank > misaka_palw_tir::types::MAX_RANK {
            return Err(invalid());
        }
        let shape = (0..rank).map(|_| u64::deserialize_reader(r)).collect::<std::io::Result<Vec<_>>>()?;
        let layout = FlatLayoutV4::of(&shape).ok_or_else(invalid)?;
        let index = u64::deserialize_reader(r)?;
        layout.span(index).ok_or_else(invalid)?;
        let hash = Digest::deserialize_reader(r)?;
        let count = u32::deserialize_reader(r)? as usize;
        if count > crate::merkle::depth(layout.leaves) as usize || count > 64 {
            return Err(invalid());
        }
        let siblings = (0..count).map(|_| Digest::deserialize_reader(r)).collect::<std::io::Result<Vec<_>>>()?;
        let opening = Self { dtype: tag, shape, index, hash, siblings };
        opening.recomputed_commitment().ok_or_else(invalid)?;
        Ok(opening)
    }
}

/// A filer-owned operand leaf, distinct from public hash-only material.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatLeafValue<const V: u8> {
    pub opening: FlatLeafHash<V>,
    pub bytes: Vec<u8>,
}
impl<const V: u8> FlatLeafValue<V> {
    pub fn of(t: &Tensor, tree: &FlatTree<V>, index: u64) -> Option<Self> {
        if t.dtype != tree.dtype
            || t.shape.len() != tree.shape.len()
            || t.shape.iter().zip(&tree.shape).any(|(a, b)| *a as u64 != *b)
            || t.data.len() as u64 != tree.layout.elements
        {
            return None;
        }
        let opening = tree.opening(index)?;
        let (first, count) = tree.layout.span(index)?;
        let mut bytes = Vec::with_capacity(count * t.dtype.width());
        for v in &t.data[first as usize..first as usize + count] {
            if !t.dtype.contains(*v) {
                return None;
            }
            t.dtype.encode_le(*v, &mut bytes);
        }
        opening.matches_bytes(&bytes).then_some(Self { opening, bytes })
    }
    pub fn authenticates(&self, commitment: &Digest) -> bool {
        self.opening.authenticates(commitment) && self.opening.matches_bytes(&self.bytes)
    }
    pub fn elements(&self) -> Option<Vec<(u64, i128)>> {
        let dtype = dtype::<V>(self.opening.dtype)?;
        let (first, count) = self.opening.layout()?.span(self.opening.index)?;
        if !self.opening.matches_bytes(&self.bytes) {
            return None;
        }
        Some(
            self.bytes
                .chunks_exact(dtype.width())
                .enumerate()
                .map(|(i, b)| (first + i as u64, dtype.decode_le(b)))
                .take(count)
                .collect(),
        )
    }
}
impl<const V: u8> BorshDeserialize for FlatLeafValue<V> {
    fn deserialize_reader<R: Read>(r: &mut R) -> std::io::Result<Self> {
        let opening = FlatLeafHash::<V>::deserialize_reader(r)?;
        let dtype = dtype::<V>(opening.dtype).ok_or_else(invalid)?;
        let (_, count) = opening.layout().and_then(|l| l.span(opening.index)).ok_or_else(invalid)?;
        let len = u32::deserialize_reader(r)? as usize;
        if len != count * dtype.width() || len > FLAT_LEAF_MAX_ELEMENTS_V4 * if V == 5 { 16 } else { 4 } {
            return Err(invalid());
        }
        let mut bytes = vec![0; len];
        r.read_exact(&mut bytes)?;
        if !opening.matches_bytes(&bytes) {
            return Err(invalid());
        }
        Ok(Self { opening, bytes })
    }
}

pub fn decode_flat_hash_v4(bytes: &[u8]) -> Result<FlatLeafHashV4, Error> {
    if bytes.len() > FLAT_HASH_MAX_BYTES_V4 {
        return Err(invalid());
    }
    let mut r = Cursor::new(bytes);
    let out = FlatLeafHashV4::deserialize_reader(&mut r)?;
    if r.position() as usize != bytes.len() {
        return Err(invalid());
    }
    Ok(out)
}
pub fn decode_flat_value_v4(bytes: &[u8]) -> Result<FlatLeafValueV4, Error> {
    if bytes.len() > FLAT_VALUE_MAX_BYTES_V4 {
        return Err(invalid());
    }
    let mut r = Cursor::new(bytes);
    let out = FlatLeafValueV4::deserialize_reader(&mut r)?;
    if r.position() as usize != bytes.len() {
        return Err(invalid());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn tensor(dtype: DType, shape: &[usize]) -> Tensor {
        let n: usize = shape.iter().product();
        let values = (0..n).map(|i| if dtype == DType::Idx { i as i128 } else { (i % 127) as i128 - 63 }).collect();
        Tensor::new(dtype, shape.to_vec(), values).unwrap()
    }
    // Independent one-buffer hash preimages and recursive tree construction;
    // does not call the streamed leaf or stack-root implementation.
    fn oracle(t: &Tensor) -> Digest {
        fn tree(leaves: &[Digest]) -> Digest {
            if leaves.len() == 1 {
                return leaves[0];
            }
            let next: Vec<_> = leaves
                .chunks(2)
                .map(|p| {
                    if p.len() == 1 {
                        p[0]
                    } else {
                        let mut s = keyed(FLAT_NODE_DOMAIN_V4);
                        s.update(&p[0]).update(&p[1]);
                        finish(s)
                    }
                })
                .collect();
            tree(&next)
        }
        let count = t.data.len() / 64;
        let leaves: Vec<_> = (0..count)
            .map(|index| {
                let from = index * 64;
                let end = if index + 1 == count { t.data.len() } else { from + 64 };
                let mut wire = vec![t.dtype.tag()];
                wire.extend((index as u64).to_le_bytes());
                wire.extend(((end - from) as u64).to_le_bytes());
                for v in &t.data[from..end] {
                    t.dtype.encode_le(*v, &mut wire);
                }
                let mut s = keyed(FLAT_LEAF_DOMAIN_V4);
                s.update(&wire);
                finish(s)
            })
            .collect();
        let mut s = keyed(FLAT_COMMITMENT_DOMAIN_V4);
        s.update(&[t.dtype.tag(), t.shape.len() as u8]);
        for d in &t.shape {
            s.update(&(*d as u64).to_le_bytes());
        }
        s.update(&tree(&leaves));
        finish(s)
    }

    #[test]
    fn flat_layout_merges_every_short_tail_and_covers_ranks_without_small_leaves() {
        assert_eq!(FLAT_LEAF_ELEMENTS_V4, crate::scope::COURT_HIDING_LEAF_ELEMENTS_V1);
        for n in 0..=8193u64 {
            let Some(l) = FlatLayoutV4::of(&[n]) else {
                assert!(n < 64);
                continue;
            };
            let mut first = 0;
            for index in 0..l.leaves {
                let (start, count) = l.span(index).unwrap();
                assert_eq!(start, first);
                assert!((64..=127).contains(&count));
                for e in start..start + count as u64 {
                    assert_eq!(l.leaf_of(e), Some(index));
                }
                first += count as u64;
            }
            assert_eq!(first, n);
            assert_eq!(l.leaf_of(n), None);
            assert!(l.span(l.leaves).is_none());
        }
        for shape in [vec![65], vec![5, 13], vec![1, 5, 13], vec![1, 1, 5, 13]] {
            let l = FlatLayoutV4::of(&shape).unwrap();
            assert_eq!(l.elements(), 65);
            assert_eq!(l.leaves(), 1);
        }
        for shape in [vec![], vec![0, 64], vec![u64::MAX, 2], vec![1, 1, 1, 1, 64]] {
            assert!(FlatLayoutV4::of(&shape).is_none());
        }
    }

    #[test]
    fn flat_streaming_tree_and_independent_oracle_match_all_dtypes_tails_and_odd_trees() {
        for dtype in [DType::I8, DType::I16, DType::I32, DType::Idx] {
            for n in [64, 65, 127, 128, 129, 191, 192, 193, 257, 513, 1025, 4097, 65_537] {
                let t = tensor(dtype, &[n]);
                let bytes = t.to_le_bytes();
                let tree = FlatTreeV4::of(&t).unwrap();
                let raw = FlatTreeV4::from_bytes(dtype, &[n as u64], &bytes).unwrap();
                let root = tree.commitment();
                assert_eq!(root, raw.commitment());
                assert_eq!(root, oracle(&t));
                assert_eq!(root, flat_commitment_from_tensor_v4(&t).unwrap());
                assert_eq!(root, flat_commitment_from_bytes_v4(dtype, &[n as u64], &bytes).unwrap());
                assert_ne!(root, crate::merkle3::tensor_commitment_v3(&t));
                for index in [0, tree.layout.leaves / 2, tree.layout.leaves - 1] {
                    let opening = tree.opening(index).unwrap();
                    assert!(opening.authenticates(&root));
                    let value = FlatLeafValueV4::of(&t, &tree, index).unwrap();
                    assert!(value.authenticates(&root));
                    let (start, count) = tree.layout.span(index).unwrap();
                    assert_eq!(
                        value.elements().unwrap(),
                        t.data[start as usize..start as usize + count]
                            .iter()
                            .enumerate()
                            .map(|(i, v)| (start + i as u64, *v))
                            .collect::<Vec<_>>()
                    );
                }
            }
        }
        let t = tensor(DType::I8, &[5, 13]);
        let root = FlatTreeV4::of(&t).unwrap().commitment();
        assert_ne!(root, FlatTreeV4::of(&Tensor::new(DType::I8, vec![65], t.data.clone()).unwrap()).unwrap().commitment());
    }

    #[test]
    fn flat_openings_refuse_borrowed_metadata_values_coordinates_and_extra_paths() {
        let t = tensor(DType::I8, &[513]);
        let tree = FlatTreeV4::of(&t).unwrap();
        let root = tree.commitment();
        let value = FlatLeafValueV4::of(&t, &tree, 7).unwrap();
        let mut wrong = value.clone();
        wrong.opening.shape = vec![1, 513];
        assert!(!wrong.authenticates(&root));
        let mut wrong = value.clone();
        wrong.opening.index = 6;
        assert!(!wrong.authenticates(&root));
        let mut wrong = value.clone();
        wrong.opening.dtype = DType::I16.tag();
        assert!(!wrong.authenticates(&root));
        let mut wrong = value.clone();
        wrong.opening.hash[0] ^= 1;
        assert!(!wrong.authenticates(&root));
        let mut wrong = value.clone();
        wrong.bytes[0] ^= 1;
        assert!(!wrong.authenticates(&root));
        let mut wrong = value.clone();
        wrong.opening.siblings.push([0; 64]);
        assert!(!wrong.authenticates(&root));
        let mut wrong = value.clone();
        wrong.opening.siblings.pop();
        assert!(!wrong.authenticates(&root));
        let mut changed = t.clone();
        changed.data[448] += 1;
        assert!(FlatLeafValueV4::of(&changed, &tree, 7).is_none());
        let other = FlatTreeV4::of(&changed).unwrap().commitment();
        assert!(!value.authenticates(&other));
        for i in 0..tree.layout.leaves {
            let leaf = FlatLeafValueV4::of(&t, &tree, i).unwrap();
            assert_ne!(leaf.opening.hash, crate::hash::object_id(FLAT_LEAF_DOMAIN_V4, &leaf.bytes));
        }
    }

    #[test]
    fn flat_public_descent_finds_every_divergent_leaf_within_the_derived_reply_bound() {
        for n in [65, 127, 128, 129, 193, 257, 385, 513, 1025, 4097] {
            let t = tensor(DType::I8, &[n]);
            let own = FlatTreeV4::of(&t).unwrap();
            for bad in 0..own.layout.leaves {
                let mut changed = t.clone();
                let (start, count) = own.layout.span(bad).unwrap();
                changed.data[start as usize + count - 1] += 1;
                let actual = FlatTreeV4::of(&changed).unwrap();
                let root = actual.commitment();
                let limit = own.layout.bounds(DType::I8).unwrap().localization_replies;
                let mut next = 0;
                let mut visited = BTreeSet::new();
                let mut found = false;
                for _ in 0..limit {
                    assert!(visited.insert(next), "descent cycles at {next}");
                    let reply = actual.opening(next).unwrap();
                    match reply.descend(&root, &own).unwrap() {
                        FlatDescentV4::Probe(index) => next = index,
                        FlatDescentV4::DivergentLeaf(index) => {
                            assert_eq!(index, bad);
                            found = true;
                            break;
                        }
                        FlatDescentV4::Matched => panic!("wrong root passed"),
                    }
                }
                assert!(found, "{n} {bad} requires more than {limit} public replies");
            }
            let reply = own.opening(0).unwrap();
            assert_eq!(reply.descend(&own.commitment(), &own).unwrap(), FlatDescentV4::Matched);
            let mut forged = reply.clone();
            forged.hash[0] ^= 1;
            assert!(forged.descend(&own.commitment(), &own).is_err());
        }
    }

    #[test]
    fn flat_wire_bounds_counts_before_allocation_and_preserves_exact_borsh() {
        let t = tensor(DType::I32, &[65_537]);
        let tree = FlatTreeV4::of(&t).unwrap();
        let value = FlatLeafValueV4::of(&t, &tree, tree.layout.leaves - 1).unwrap();
        let hash = borsh::to_vec(&value.opening).unwrap();
        let wire = borsh::to_vec(&value).unwrap();
        assert_eq!(decode_flat_hash_v4(&hash).unwrap(), value.opening);
        assert_eq!(decode_flat_value_v4(&wire).unwrap(), value);
        let mut bad = hash.clone();
        bad[0] = 255;
        assert!(decode_flat_hash_v4(&bad).is_err());
        let mut bad = hash[..1].to_vec();
        bad.extend(u32::MAX.to_le_bytes());
        assert!(decode_flat_hash_v4(&bad).is_err());
        let mut bad = hash.clone();
        bad[5..13].copy_from_slice(&63u64.to_le_bytes());
        assert!(decode_flat_hash_v4(&bad).is_err());
        let path_at = 1 + 4 + 8 + 8 + 64;
        let mut bad = hash[..path_at].to_vec();
        bad.extend(u32::MAX.to_le_bytes());
        assert!(decode_flat_hash_v4(&bad).is_err());
        let mut bad = hash.clone();
        bad.push(0);
        assert!(decode_flat_hash_v4(&bad).is_err());
        let mut bad = wire[..hash.len()].to_vec();
        bad.extend(u32::MAX.to_le_bytes());
        assert!(decode_flat_value_v4(&bad).is_err());
        let mut bad = wire.clone();
        bad[hash.len()..hash.len() + 4].copy_from_slice(&4u32.to_le_bytes());
        assert!(decode_flat_value_v4(&bad).is_err());
        let mut bad = wire.clone();
        *bad.last_mut().unwrap() ^= 1;
        assert!(decode_flat_value_v4(&bad).is_err());
        let mut bad = wire.clone();
        bad.push(0);
        assert!(decode_flat_value_v4(&bad).is_err());
        for n in 0..wire.len() {
            assert!(decode_flat_value_v4(&wire[..n]).is_err());
        }
        assert!(decode_flat_hash_v4(&vec![0; FLAT_HASH_MAX_BYTES_V4 + 1]).is_err());
        assert!(decode_flat_value_v4(&vec![0; FLAT_VALUE_MAX_BYTES_V4 + 1]).is_err());
    }

    #[test]
    fn flat_material_prices_exact_retention_leaf_carriers_and_streaming_scratch() {
        for n in [64, 65, 127, 128, 129, 257, 513, 65_537, 1_048_577] {
            let t = tensor(DType::I8, &[n]);
            let tree = FlatTreeV4::of(&t).unwrap();
            let bounds = tree.layout.bounds(t.dtype).unwrap();
            let retained = tree.levels.iter().map(|l| l.capacity() as u128 * 64).sum::<u128>()
                + tree.levels.capacity() as u128 * 24
                + tree.shape.capacity() as u128 * 8
                + std::mem::size_of::<FlatTreeV4>() as u128;
            assert!(retained <= bounds.retained_tree_bytes);
            assert_eq!(bounds.source_bytes, n as u128);
            for index in [0, tree.layout.leaves - 1] {
                let value = FlatLeafValueV4::of(&t, &tree, index).unwrap();
                assert!(borsh::to_vec(&value.opening).unwrap().len() as u64 <= bounds.leaf_hash_bytes);
                assert!(borsh::to_vec(&value).unwrap().len() as u64 <= bounds.leaf_value_bytes);
                assert!(value.bytes.len() >= 64 && value.bytes.len() <= 127);
            }
            assert_eq!(flat_commitment_from_bytes_v4(t.dtype, &[n as u64], &t.to_le_bytes()).unwrap(), tree.commitment());
            assert!(bounds.streaming_scratch_bytes <= 8192);
            assert!(FlatTreeV4::from_bytes_bounded(t.dtype, &[n as u64], &t.to_le_bytes(), bounds.retained_tree_bytes - 1).is_err());
            assert_eq!(FlatTreeV4::of_bounded(&t, bounds.retained_tree_bytes).unwrap().commitment(), tree.commitment());
        }
        let scratch = std::mem::size_of::<[Option<Digest>; 64]>()
            + 2 * std::mem::size_of::<blake2b_simd::State>()
            + 4 * std::mem::size_of::<Digest>()
            + FLAT_LEAF_MAX_ELEMENTS_V4 * 4
            + 256;
        assert!(scratch <= 8192);
        let l = FlatLayoutV4::of(&[u64::MAX]).unwrap();
        let b = l.bounds(DType::I32).unwrap();
        assert!(b.source_bytes > u64::MAX as u128);
        assert!(b.leaf_hash_bytes <= FLAT_HASH_MAX_BYTES_V4 as u64);
        assert!(b.leaf_value_bytes <= FLAT_VALUE_MAX_BYTES_V4 as u64);
        assert!(flat_commitment_from_bytes_v4(DType::I32, &[u64::MAX], &[]).is_err());
        assert!(FlatTreeV4::from_bytes(DType::I8, &[u64::MAX], &[]).is_err());
        assert!(FlatTreeV4::from_bytes(DType::I128, &[64], &[0; 1024]).is_err());
        assert!(FlatTreeV4::of(&Tensor { dtype: DType::I8, shape: vec![64], data: vec![256; 64] }).is_err());
    }
}
