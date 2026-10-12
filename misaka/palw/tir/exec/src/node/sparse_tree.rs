//! Authenticated openings from requested ranges without allocating a whole leaf-count tree.
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_step_leg::{
    PalwStepOpeningV1, PalwStepRangeOpeningV1, step_merkle_leaf_v1, step_merkle_node_v1, step_range_opening_root_capped_v1,
};
use std::collections::BTreeMap;

pub trait TirProofTreeV1 {
    fn opening(&self, index: u64) -> Option<PalwStepOpeningV1>;
    fn range_siblings(&self, first: u64, count: u64) -> Option<Vec<Hash64>>;
    fn node_parts(&self, level: u8, index: u64) -> Option<(Vec<Hash64>, Vec<Hash64>)>;
}
impl TirProofTreeV1 for super::tree::TirStepTreeV1 {
    fn opening(&self, index: u64) -> Option<PalwStepOpeningV1> {
        self.opening(index)
    }
    fn range_siblings(&self, first: u64, count: u64) -> Option<Vec<Hash64>> {
        self.range_siblings(first, count)
    }
    fn node_parts(&self, level: u8, index: u64) -> Option<(Vec<Hash64>, Vec<Hash64>)> {
        self.node_parts(level, index)
    }
}

#[derive(Clone, Debug)]
pub struct TirSparseStepTreeV1 {
    count: u64,
    root: Hash64,
    hashes: BTreeMap<u64, Hash64>,
    nodes: BTreeMap<(u8, u64), Hash64>,
}
impl TirSparseStepTreeV1 {
    pub fn new(count: u64, root: Hash64) -> Result<Self, String> {
        if count == 0 {
            return Err("a sparse step tree has no leaves".into());
        }
        Ok(Self { count, root, hashes: BTreeMap::new(), nodes: BTreeMap::new() })
    }
    pub fn known_nodes(&self) -> usize {
        self.nodes.len()
    }
    pub fn held_leaves(&self) -> usize {
        self.hashes.len()
    }
    pub fn add_range(&mut self, opening: &PalwStepRangeOpeningV1, cap: u64) -> Result<(), String> {
        if step_range_opening_root_capped_v1(self.count, opening, cap).map_err(|e| e.to_string())? != self.root {
            return Err("the sparse range does not reach the committed root".into());
        }
        let mut changes = BTreeMap::new();
        let mut current: Vec<Hash64> = opening
            .leaf_hashes
            .iter()
            .enumerate()
            .map(|(i, hash)| step_merkle_leaf_v1(opening.first_leaf_index + i as u64, hash))
            .collect();
        let (mut first, mut end, mut width, mut level) =
            (opening.first_leaf_index, opening.first_leaf_index + current.len() as u64, self.count, 0u8);
        let mut siblings = opening.siblings.iter();
        loop {
            for (i, node) in current.iter().enumerate() {
                changes.insert((level, first + i as u64), *node);
            }
            if width == 1 {
                break;
            }
            if first % 2 == 1 {
                let node = *siblings.next().ok_or("a sparse range misses its left sibling")?;
                changes.insert((level, first - 1), node);
                current.insert(0, node);
            }
            if end % 2 == 1 && end < width {
                let node = *siblings.next().ok_or("a sparse range misses its right sibling")?;
                changes.insert((level, end), node);
                current.push(node);
            }
            let mut next = Vec::with_capacity(current.len().div_ceil(2));
            let mut pairs = current.chunks_exact(2);
            for pair in &mut pairs {
                next.push(step_merkle_node_v1(&pair[0], &pair[1]));
            }
            if let [odd] = pairs.remainder() {
                next.push(*odd);
            }
            current = next;
            first /= 2;
            end = end.div_ceil(2);
            width = width.div_ceil(2);
            level = level.checked_add(1).ok_or("the sparse tree height overflows")?;
        }
        if siblings.next().is_some() || current != [self.root] {
            return Err("the sparse range has a noncanonical root/path".into());
        }
        if changes.iter().any(|(key, value)| self.nodes.get(key).is_some_and(|known| known != value)) {
            return Err("the sparse range conflicts with a previously authenticated node".into());
        }
        // Refusal is atomic: no leaf/node from a failed range is retained.
        for (i, hash) in opening.leaf_hashes.iter().enumerate() {
            self.hashes.insert(opening.first_leaf_index + i as u64, *hash);
        }
        self.nodes.extend(changes);
        Ok(())
    }
}
impl TirProofTreeV1 for TirSparseStepTreeV1 {
    fn opening(&self, index: u64) -> Option<PalwStepOpeningV1> {
        let leaf_hash = *self.hashes.get(&index)?;
        let (mut at, mut width, mut level) = (index, self.count, 0u8);
        let mut siblings = Vec::new();
        while width > 1 {
            if !(width % 2 == 1 && at == width - 1) {
                siblings.push(*self.nodes.get(&(level, at ^ 1))?);
            }
            at /= 2;
            width = width.div_ceil(2);
            level += 1;
        }
        Some(PalwStepOpeningV1 { leaf_index: index, leaf_hash, siblings })
    }
    fn range_siblings(&self, first: u64, count: u64) -> Option<Vec<Hash64>> {
        let mut end = first.checked_add(count)?;
        if count == 0 || end > self.count {
            return None;
        }
        let (mut first, mut width, mut level) = (first, self.count, 0u8);
        let mut out = Vec::new();
        while width > 1 {
            if first % 2 == 1 {
                out.push(*self.nodes.get(&(level, first - 1))?);
            }
            if end % 2 == 1 && end < width {
                out.push(*self.nodes.get(&(level, end))?);
            }
            first /= 2;
            end = end.div_ceil(2);
            width = width.div_ceil(2);
            level += 1;
        }
        Some(out)
    }
    fn node_parts(&self, level: u8, index: u64) -> Option<(Vec<Hash64>, Vec<Hash64>)> {
        let (below, first, end) = kaspa_consensus_core::palw_tir_court_v1::palw_tir_step_node_frontier_v1(self.count, level, index)?;
        let frontier = (first..end).map(|i| self.nodes.get(&(below, i)).copied()).collect::<Option<Vec<_>>>()?;
        let (mut at, mut width, mut row) = (index, self.count, 0u8);
        for _ in 0..level {
            width = width.div_ceil(2);
            row += 1;
        }
        let mut siblings = Vec::new();
        while width > 1 {
            if !(width % 2 == 1 && at == width - 1) {
                siblings.push(*self.nodes.get(&(row, at ^ 1))?);
            }
            at /= 2;
            width = width.div_ceil(2);
            row += 1;
        }
        Some((frontier, siblings))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_step_leg::{step_merkle_range_siblings_v1, step_merkle_root_v1};
    #[test]
    fn one_opening_in_a_trillion_leaf_tree_allocates_only_its_path() {
        let count = 1u64 << 40;
        let hash = Hash64::from_u64_word(73);
        let siblings: Vec<_> = (0..40).map(|i| Hash64::from_u64_word(100 + i)).collect();
        let mut root = step_merkle_leaf_v1(0, &hash);
        for sibling in &siblings {
            root = step_merkle_node_v1(&root, sibling);
        }
        let opening = PalwStepRangeOpeningV1 { first_leaf_index: 0, leaf_hashes: vec![hash], siblings };
        let mut tree = TirSparseStepTreeV1::new(count, root).unwrap();
        tree.add_range(&opening, count).unwrap();
        assert_eq!(tree.held_leaves(), 1);
        assert_eq!(tree.known_nodes(), 81);
        assert_eq!(tree.opening(0).unwrap().siblings, opening.siblings);
        assert!(tree.opening(1).is_none());
        assert!(tree.add_range(&opening, count - 1).is_err());
        assert_eq!(tree.known_nodes(), 81);
    }
    #[test]
    fn sparse_ranges_make_exact_full_tree_openings_across_odd_widths_and_refuse_atomically() {
        for n in [1, 2, 3, 7, 8, 9, 63, 64, 65, 255, 256, 257] {
            let hashes: Vec<_> = (0..n).map(Hash64::from_u64_word).collect();
            let root = step_merkle_root_v1(&hashes).unwrap();
            let full = super::super::tree::TirStepTreeV1::full(&hashes);
            let mut sparse = TirSparseStepTreeV1::new(n, root).unwrap();
            // Reverse order and overlapping runs exercise merged sibling nodes and promotion.
            for first in (0..n).step_by(7).collect::<Vec<_>>().into_iter().rev() {
                let count = 9.min(n - first);
                let opening = PalwStepRangeOpeningV1 {
                    first_leaf_index: first,
                    leaf_hashes: hashes[first as usize..(first + count) as usize].to_vec(),
                    siblings: step_merkle_range_siblings_v1(&hashes, first as usize, count as usize).unwrap(),
                };
                sparse.add_range(&opening, n).unwrap();
                for index in first..first + count {
                    assert_eq!(sparse.opening(index), full.opening(index), "n={n} i={index}");
                }
                for a in first..first + count {
                    for b in a + 1..=first + count {
                        assert_eq!(sparse.range_siblings(a, b - a), full.range_siblings(a, b - a));
                    }
                }
                let before = (sparse.known_nodes(), sparse.held_leaves());
                let mut bad = opening.clone();
                bad.leaf_hashes[0] = Hash64::from_u64_word(0xBAD);
                assert!(sparse.add_range(&bad, n).is_err());
                assert_eq!((sparse.known_nodes(), sparse.held_leaves()), before);
            }
            assert!(sparse.known_nodes() < 3 * n as usize + 64);
            assert_eq!(sparse.held_leaves(), n as usize);
            assert_eq!(sparse.opening(n), None);
        }
    }
}
