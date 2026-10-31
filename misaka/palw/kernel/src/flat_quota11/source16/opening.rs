//! Exact original v3 parameter header/subtree/leaf openings. No subtiles,
//! replacement commitment domains or externally correct R values authenticate
//! a producer's bound PC. Public bisection follows its actual odd-carried tree.
use crate::{
    hash::Digest,
    merkle::{AXIS_COL, AXIS_ROW},
    merkle3::{self, LayoutV3, LeafOpeningV3, TreesV3},
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::{DType, Tensor};
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct HeaderV16 {
    pub dtype: u8,
    pub rank: u8,
    pub shape: [u64; 4],
    pub row_root: Digest,
    pub col_root: Digest,
}
impl HeaderV16 {
    pub fn of(t: &Tensor, trees: &TreesV3) -> Option<Self> {
        if t.shape.len() > 4 {
            return None;
        }
        let mut shape = [0; 4];
        for (i, n) in t.shape.iter().enumerate() {
            shape[i] = *n as u64;
        }
        Some(Self { dtype: t.dtype.tag(), rank: t.shape.len() as u8, shape, row_root: trees.row_root(), col_root: trees.col_root() })
    }
    pub fn dims(&self) -> Option<Vec<usize>> {
        if self.rank > 4 || self.shape[self.rank as usize..].iter().any(|n| *n != 0) {
            return None;
        }
        self.shape[..self.rank as usize].iter().map(|n| usize::try_from(*n).ok()).collect()
    }
    pub fn commitment(&self) -> Option<Digest> {
        let dtype = DType::ALL.into_iter().find(|d| d.tag() == self.dtype)?;
        let dims = self.dims()?;
        LayoutV3::try_of(&dims)?;
        Some(merkle3::commit_v3(dtype, &dims, &self.row_root, &self.col_root))
    }
    pub fn authenticates(&self, pc: &Digest) -> bool {
        self.commitment().as_ref() == Some(pc)
    }
    pub fn matches_decl(&self, dtype: DType, shape: &[u32]) -> bool {
        self.dtype == dtype.tag()
            && self.rank as usize == shape.len()
            && self.shape[..shape.len()].iter().zip(shape).all(|(a, b)| *a == *b as u64)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SubtreeV16 {
    pub header: HeaderV16,
    pub axis: u8,
    pub level: u8,
    pub index: u64,
    pub hash: Digest,
    pub siblings: Vec<Digest>,
}
impl SubtreeV16 {
    pub fn of(t: &Tensor, trees: &TreesV3, axis: u8, level: u8, index: u64) -> Option<Self> {
        if axis != AXIS_ROW && axis != AXIS_COL {
            return None;
        }
        let mut hashes = if axis == AXIS_ROW { trees.row_leaves.clone() } else { trees.col_leaves.clone() };
        for _ in 0..level {
            if hashes.len() <= 1 {
                return None;
            }
            hashes = hashes.chunks(2).map(|v| if v.len() == 2 { merkle3::node_hash_v3(&v[0], &v[1]) } else { v[0] }).collect();
        }
        let hash = *hashes.get(usize::try_from(index).ok()?)?;
        let siblings = merkle3::tree_path(hashes, index as usize, merkle3::node_hash_v3);
        Some(Self { header: HeaderV16::of(t, trees)?, axis, level, index, hash, siblings })
    }
    pub fn authenticates(&self, pc: &Digest) -> bool {
        if !self.header.authenticates(pc) || (self.axis != AXIS_ROW && self.axis != AXIS_COL) || self.siblings.len() > 64 {
            return false;
        }
        let Some(dims) = self.header.dims() else {
            return false;
        };
        let Some(layout) = LayoutV3::try_of(&dims) else {
            return false;
        };
        let mut count = layout.leaves(self.axis);
        for _ in 0..self.level {
            if count <= 1 {
                return false;
            }
            count = count.div_ceil(2);
        }
        let root = if self.axis == AXIS_ROW { self.header.row_root } else { self.header.col_root };
        merkle3::tree_root_from_path(self.hash, self.index, count, &self.siblings, merkle3::node_hash_v3) == Some(root)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub enum AnswerV16 {
    Header(HeaderV16),
    Subtree(SubtreeV16),
    Leaf(LeafOpeningV3),
}
pub fn decode_answer_v16(bytes: &[u8]) -> Result<AnswerV16, String> {
    if bytes.len() > super::SOURCE_ANSWER_MAX_BYTES_V16 as usize {
        return Err("source answer exceeds original paid envelope".into());
    }
    let kind = *bytes.first().ok_or("source answer tag absent")?;
    let body = &bytes[1..];
    match kind {
        0 if body.len() == 162 => borsh::from_slice(body).map(AnswerV16::Header).map_err(|_| "source header framing".into()),
        1 => {
            const PREFIX: usize = 162 + 1 + 1 + 8 + 64;
            let raw = body.get(PREFIX..PREFIX + 4).ok_or("source subtree fixed framing")?;
            let n = u32::from_le_bytes(raw.try_into().unwrap()) as usize;
            if n > 64 || body.len() != PREFIX + 4 + n * 64 {
                return Err("source subtree bounded path".into());
            }
            borsh::from_slice(body).map(AnswerV16::Subtree).map_err(|_| "source subtree framing".into())
        }
        2 => {
            let dtype = DType::ALL.into_iter().find(|d| Some(&d.tag()) == body.first()).ok_or("source leaf dtype")?;
            let n = u32::from_le_bytes(body.get(1..5).ok_or("source leaf shape frame")?.try_into().unwrap()) as usize;
            if n > 4 {
                return Err("source leaf shape rank".into());
            }
            let count_at = 5 + n * 8 + 1 + 8 + 8;
            let count =
                u32::from_le_bytes(body.get(count_at..count_at + 4).ok_or("source leaf count frame")?.try_into().unwrap()) as usize;
            if count > merkle3::TILE_V3 as usize {
                return Err("source leaf exceeds ORIGINAL 4096 elements".into());
            }
            let paths_at = count_at + 4 + count * dtype.width();
            let paths =
                u32::from_le_bytes(body.get(paths_at..paths_at + 4).ok_or("source leaf path frame")?.try_into().unwrap()) as usize;
            if paths > 64 || body.len() != paths_at + 4 + paths * 64 + 64 {
                return Err("source leaf exact bounded path/payload".into());
            }
            borsh::from_slice(body).map(AnswerV16::Leaf).map_err(|_| "source original leaf framing".into())
        }
        _ => Err("source original answer kind/framing".into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_odd_carried_subtrees_and_full_4096_leaf_authenticate_both_axes() {
        let t = Tensor::new(DType::I128, vec![3, 8193], (0..3 * 8193).map(|n| n as i128).collect()).unwrap();
        let trees = TreesV3::of(&t);
        let pc = merkle3::tensor_commitment_v3(&t);
        for axis in [AXIS_ROW, AXIS_COL] {
            let total = trees.layout.leaves(axis);
            let mut count = total;
            let mut level = 0;
            while count > 0 {
                for index in [0, count - 1] {
                    let p = SubtreeV16::of(&t, &trees, axis, level, index).unwrap();
                    assert!(p.authenticates(&pc));
                    let b = borsh::to_vec(&AnswerV16::Subtree(p.clone())).unwrap();
                    assert_eq!(decode_answer_v16(&b).unwrap(), AnswerV16::Subtree(p.clone()));
                    let mut bad = p;
                    bad.hash[0] ^= 1;
                    assert!(!bad.authenticates(&pc));
                }
                if count == 1 {
                    break;
                }
                count = count.div_ceil(2);
                level += 1;
            }
        }
        let leaf = LeafOpeningV3::of_trees(&t, &trees, AXIS_ROW, 0, 0).unwrap();
        assert_eq!(leaf.values.len(), 4096);
        let b = borsh::to_vec(&AnswerV16::Leaf(leaf.clone())).unwrap();
        assert!(b.len() > 64 << 10);
        assert_eq!(decode_answer_v16(&b).unwrap(), AnswerV16::Leaf(leaf.clone()));
        assert!(leaf.authenticates(&pc));
        let mut short = leaf;
        short.values.truncate(64);
        assert!(!short.authenticates(&pc));
    }
    #[test]
    fn hostile_leaf_count_and_shape_are_refused_before_allocation() {
        let mut b = vec![2, 0];
        b.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_answer_v16(&b).is_err());
        let mut b = vec![2, DType::I128.tag()];
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&[0; 17]);
        b.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_answer_v16(&b).is_err());
    }
}
