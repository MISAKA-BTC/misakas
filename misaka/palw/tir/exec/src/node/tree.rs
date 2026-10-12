//! **An IR execution's step tree, held level by level** — the openings a court close carries for
//! step leaves (the disputed leaf, and one range per contiguous run of operand leaves), answered in
//! `O(log n)` each from the levels instead of a fold over every leaf per opening.
//!
//! The tree is the consensus step tree (`palw_step_leg`: each leaf `step_merkle_leaf_v1(index,
//! hash)`, each node `step_merkle_node_v1`, an odd last node promoted), and both walks — a single
//! leaf's path and a range's siblings — read the nodes `step_opening_capped_v1` and
//! `step_merkle_range_siblings_capped_v1` read, in their order; the tests hold them byte-identical.
//!
//! **A tree need not be whole.** A challenger that re-executed a claim honestly holds the accused's
//! leaves only BEFORE the disputed one (the bisection narrowed to the first leaf they differ at), and
//! the accused's disputed leaf with its opening. [`TirStepTreeV1::prefix_with_opening`] holds exactly
//! the nodes those determine — every node over leaves before the disputed one, the disputed leaf's
//! path, and the path's siblings — and every opening of a range that ends at or before the disputed
//! leaf reads only such nodes; one that reads any other is refused, never guessed.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_step_leg::{PalwStepOpeningV1, step_merkle_leaf_v1, step_merkle_node_v1};

/// Every level of a step tree; `None` for a node this holder cannot derive.
#[derive(Clone, Debug)]
pub struct TirStepTreeV1 {
    /// The leaf hashes the holder knows (`step_tile_leaf_hash_v1`), by index.
    leaves: Vec<Option<Hash64>>,
    levels: Vec<Vec<Option<Hash64>>>,
}

fn fold(level: &[Option<Hash64>]) -> Vec<Option<Hash64>> {
    let mut next = Vec::with_capacity(level.len().div_ceil(2));
    let mut pairs = level.chunks_exact(2);
    for pair in &mut pairs {
        next.push(match (pair[0], pair[1]) {
            (Some(l), Some(r)) => Some(step_merkle_node_v1(&l, &r)),
            _ => None,
        });
    }
    if let [odd] = pairs.remainder() {
        next.push(*odd);
    }
    next
}

impl TirStepTreeV1 {
    /// The whole tree over `leaf_hashes`.
    pub fn full(leaf_hashes: &[Hash64]) -> Self {
        let leaves: Vec<Option<Hash64>> = leaf_hashes.iter().copied().map(Some).collect();
        let level0 = leaf_hashes.iter().enumerate().map(|(i, h)| Some(step_merkle_leaf_v1(i as u64, h))).collect();
        Self::from_level0(leaves, level0)
    }

    fn from_level0(leaves: Vec<Option<Hash64>>, level0: Vec<Option<Hash64>>) -> Self {
        let mut levels = vec![level0];
        while levels.last().expect("non-empty").len() > 1 {
            let next = fold(levels.last().expect("non-empty"));
            levels.push(next);
        }
        Self { leaves, levels }
    }

    /// **The tree a challenger holds**: the accused's tree of `leaf_count` leaves, known over
    /// `prefix` (its leaves before `disputed.leaf_index`, equal to the challenger's own) and along
    /// `disputed`'s path. `Err` when the opening does not fit the prefix (a left sibling on the path
    /// that the prefix computes differently: the executions differ before the disputed leaf).
    pub fn prefix_with_opening(prefix: &[Hash64], leaf_count: u64, disputed: &PalwStepOpeningV1) -> Result<Self, String> {
        let l = disputed.leaf_index;
        if l >= leaf_count || (prefix.len() as u64) < l {
            return Err(format!("leaf {l} of {leaf_count} with a prefix of {}", prefix.len()));
        }
        let n = leaf_count as usize;
        let mut leaves = vec![None; n];
        let mut level0 = vec![None; n];
        for (i, h) in prefix.iter().take(l as usize).enumerate() {
            leaves[i] = Some(*h);
            level0[i] = Some(step_merkle_leaf_v1(i as u64, h));
        }
        leaves[l as usize] = Some(disputed.leaf_hash);
        level0[l as usize] = Some(step_merkle_leaf_v1(l, &disputed.leaf_hash));
        let mut levels = vec![level0];
        let mut at = l as usize;
        let mut siblings = disputed.siblings.iter();
        while levels.last().expect("non-empty").len() > 1 {
            let level = levels.last_mut().expect("non-empty");
            let width = level.len();
            let promoted = width % 2 == 1 && at == width - 1;
            if !promoted {
                let s = *siblings.next().ok_or("the disputed leaf's opening is too short")?;
                match level[at ^ 1] {
                    // A left sibling the prefix derives must be the one the accused opened with.
                    Some(known) if known != s => {
                        return Err(format!(
                            "the accused's path disagrees with the prefix at level {} (node {})",
                            levels.len() - 1,
                            at ^ 1
                        ));
                    }
                    _ => level[at ^ 1] = Some(s),
                }
            }
            let next = fold(levels.last().expect("non-empty"));
            levels.push(next);
            at /= 2;
        }
        if siblings.next().is_some() {
            return Err("the disputed leaf's opening is too long".into());
        }
        Ok(Self { leaves, levels })
    }

    pub fn leaf_count(&self) -> u64 {
        self.leaves.len() as u64
    }

    /// The root, when the holder can derive it.
    pub fn root(&self) -> Option<Hash64> {
        *self.levels.last()?.first()?
    }

    /// The leaf hash at `index`, when held.
    pub fn leaf_hash(&self, index: u64) -> Option<Hash64> {
        *self.leaves.get(index as usize)?
    }

    /// The node at `pos` of `level` (0 = the leaves' own nodes, `step_merkle_leaf_v1`), when held —
    /// what an opening's sibling at that place is compared with.
    pub fn node(&self, level: usize, pos: u64) -> Option<Hash64> {
        *self.levels.get(level)?.get(pos as usize)?
    }

    /// **Node `(level, index)`'s frontier and opening** — `palw_tir_step_node_parts_v1`'s answer (the
    /// second IR fence's `TirStepNode`), read off the held levels instead of re-folding every leaf: the
    /// nodes [`kaspa_consensus_core::palw_tir_court_v1::PALW_TIR_STEP_NODE_DEPTH_V1`] levels below it (the
    /// leaf nodes, when nearer) and its siblings up to the root, promotion as the tree folds. `None`
    /// for a leaf (`level` 0), a node past the tree, or one this holder cannot derive.
    pub fn node_parts(&self, level: u8, index: u64) -> Option<(Vec<Hash64>, Vec<Hash64>)> {
        let (below, lo, hi) =
            kaspa_consensus_core::palw_tir_court_v1::palw_tir_step_node_frontier_v1(self.leaf_count(), level, index)?;
        let frontier = self.levels.get(below as usize)?.get(lo as usize..hi as usize)?.iter().copied().collect::<Option<Vec<_>>>()?;
        let mut siblings = Vec::new();
        let mut position = index;
        for row in self.levels.iter().skip(level as usize) {
            let width = row.len() as u64;
            if width == 1 {
                break;
            }
            if !(width % 2 == 1 && position == width - 1) {
                siblings.push((*row.get((position ^ 1) as usize)?)?);
            }
            position /= 2;
        }
        Some((frontier, siblings))
    }

    /// **The opening of leaf `index`** — `step_opening_capped_v1`'s path.
    pub fn opening(&self, index: u64) -> Option<PalwStepOpeningV1> {
        let leaf_hash = self.leaf_hash(index)?;
        let mut at = index as usize;
        let mut siblings = Vec::new();
        for level in &self.levels[..self.levels.len() - 1] {
            let width = level.len();
            let promoted = width % 2 == 1 && at == width - 1;
            if !promoted {
                siblings.push((*level.get(at ^ 1)?)?);
            }
            at /= 2;
        }
        Some(PalwStepOpeningV1 { leaf_index: index, leaf_hash, siblings })
    }

    /// **The siblings of the range `[first, first + count)`** — `step_merkle_range_siblings_capped_v1`'s
    /// set, in its order.
    pub fn range_siblings(&self, first: u64, count: u64) -> Option<Vec<Hash64>> {
        if count == 0 || first.checked_add(count)? > self.leaf_count() {
            return None;
        }
        let (mut a, mut b) = (first as usize, (first + count) as usize);
        let mut out = Vec::new();
        for level in &self.levels[..self.levels.len() - 1] {
            if a % 2 == 1 {
                out.push((*level.get(a - 1)?)?);
            }
            if b % 2 == 1 {
                let promoted = level.len() % 2 == 1 && b == level.len();
                if !promoted {
                    out.push((*level.get(b)?)?);
                }
            }
            a /= 2;
            b = b.div_ceil(2);
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_tir_court_v1::{
        palw_tir_step_node_parts_v1, palw_tir_step_node_reaches_v1, palw_tir_step_tree_height_v1,
    };

    fn h(i: u64) -> Hash64 {
        Hash64::from_u64_word(i.wrapping_mul(0x9E37_79B9).wrapping_add(7))
    }

    /// **A held tree answers every step node exactly as consensus builds it** — frontier and siblings
    /// equal to `palw_tir_step_node_parts_v1` over the same leaves, for every node of trees of every
    /// width up to 300 (odd levels promoted, heights past the eight-level frontier), each reaching the
    /// root by the fold's own check; a leaf, and a node past the tree, answer nothing.
    #[test]
    fn a_held_tree_answers_every_step_node_as_consensus_builds_it() {
        for n in (1u64..=40).chain([63, 64, 65, 255, 256, 257, 300]) {
            let leaves: Vec<Hash64> = (0..n).map(h).collect();
            let tree = TirStepTreeV1::full(&leaves);
            let root = tree.root().expect("a root");
            let height = palw_tir_step_tree_height_v1(n);
            for level in 1..=height {
                let width = tree.levels[level as usize].len() as u64;
                for index in 0..width {
                    let held = tree.node_parts(level, index).unwrap_or_else(|| panic!("n={n}: ({level}, {index})"));
                    assert_eq!(Some(held.clone()), palw_tir_step_node_parts_v1(&leaves, level, index), "n={n}: ({level}, {index})");
                    palw_tir_step_node_reaches_v1(n, &root, level, index, &held.0, &held.1)
                        .unwrap_or_else(|e| panic!("n={n}: ({level}, {index}): {e}"));
                }
                assert!(tree.node_parts(level, width).is_none(), "n={n}: past the level");
            }
            assert!(tree.node_parts(0, 0).is_none(), "a leaf is a TirStepLeaf");
            assert!(tree.node_parts(height + 1, 0).is_none(), "past the root");
        }
    }
}
