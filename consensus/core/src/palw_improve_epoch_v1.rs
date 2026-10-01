//! **RFC-0004 §4 / spec 17 §17.5–§17.8: the epoch's pure functions** — the times an epoch opens with,
//! the material tree and `dataset_root`, the epoch seed, the draw, the suite items, the judge draw and
//! the pairwise order. Everything here is a function of its arguments; the fold
//! (`palw_improve_fold_v1`) applies them.

use crate::Hash64;
use crate::palw_improve_state_v1::{PalwEpochTimesV1, PalwEpochWindowsV1, PalwMaterialFrontierV1};
use crate::palw_state_v2::PalwBondKeyV2;

pub const PALW_IMPROVE_MATERIAL_LEAF_DOMAIN_V1: &[u8] = b"misaka-palw/improve/material-leaf/v1";
pub const PALW_IMPROVE_MATERIAL_NODE_DOMAIN_V1: &[u8] = b"misaka-palw/improve/material-node/v1";
pub const PALW_IMPROVE_MATERIAL_EMPTY_DOMAIN_V1: &[u8] = b"misaka-palw/improve/material-empty/v1";
pub const PALW_IMPROVE_DATASET_ROOT_DOMAIN_V1: &[u8] = b"misaka-palw/improve/dataset-root/v1";
pub const PALW_IMPROVE_EPOCH_SEED_DOMAIN_V1: &[u8] = b"misaka-palw/improve/epoch-seed/v1";
pub const PALW_IMPROVE_DRAW_DOMAIN_V1: &[u8] = b"misaka-palw/improve/draw/v1";
pub const PALW_IMPROVE_SETTER_ITEM_DOMAIN_V1: &[u8] = b"misaka-palw/improve/setter-item/v1";
pub const PALW_IMPROVE_SUITE_ITEM_DOMAIN_V1: &[u8] = b"misaka-palw/improve/suite-item/v1";
pub const PALW_IMPROVE_SUITE_DRAW_DOMAIN_V1: &[u8] = b"misaka-palw/improve/suite-draw/v1";
pub const PALW_IMPROVE_JUDGE_DOMAIN_V1: &[u8] = b"misaka-palw/improve/judge/v1";

fn keyed64(key: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
    for part in parts {
        state.update(part);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

// ---- times ----

/// **The epoch's times**, opened at the grid boundary `t_open` (spec 17 §17.5.3 step 1). Saturating:
/// the policy check bounds every window by 2^32, so no real epoch saturates.
pub fn palw_improve_epoch_times_v1(t_open: u64, w: &PalwEpochWindowsV1) -> PalwEpochTimesV1 {
    let t_fix = t_open.saturating_add(w.w_collect);
    let t_close = t_fix.saturating_add(w.w_submit);
    let t_draw = t_close.saturating_add(w.w_holdout);
    let t_eval = t_draw.saturating_add(w.w_eval);
    PalwEpochTimesV1 { t_open, t_fix, t_close, t_draw, t_eval, t_score: t_eval.saturating_add(w.court_margin) }
}

/// The largest multiple of `grid` at or below `daa` (a grid boundary) — `grid ≥ 1` by the policy check.
pub fn palw_improve_grid_floor_v1(daa: u64, grid: u64) -> u64 {
    daa - daa % grid.max(1)
}

/// The first multiple of `grid` strictly above `daa`.
pub fn palw_improve_next_boundary_v1(daa: u64, grid: u64) -> u64 {
    palw_improve_grid_floor_v1(daa, grid).saturating_add(grid.max(1))
}

// ---- material (spec 17 §17.6.1) ----

/// A material leaf's kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwMaterialKindV1 {
    HardCase = 1,
    Dataset = 2,
    TeachingArtifact = 3,
}

/// `H("…/material-leaf/v1", u8 kind ‖ id)`.
pub fn palw_improve_material_leaf_v1(kind: PalwMaterialKindV1, id: &Hash64) -> Hash64 {
    keyed64(PALW_IMPROVE_MATERIAL_LEAF_DOMAIN_V1, &[&[kind as u8], id.as_byte_slice()])
}

fn node(left: &Hash64, right: &Hash64) -> Hash64 {
    keyed64(PALW_IMPROVE_MATERIAL_NODE_DOMAIN_V1, &[left.as_byte_slice(), right.as_byte_slice()])
}

/// **Append a leaf** to the frontier: the frontier holds the roots of the perfect subtrees the count's
/// set bits describe, largest first; a new leaf merges with every trailing subtree of its own size.
pub fn palw_improve_material_append_v1(frontier: &mut PalwMaterialFrontierV1, leaf: Hash64) {
    let mut carry = leaf;
    let mut count = frontier.count;
    while count & 1 == 1 {
        let left = frontier.frontier.pop().expect("a set bit has its subtree");
        carry = node(&left, &carry);
        count >>= 1;
    }
    frontier.frontier.push(carry);
    frontier.count += 1;
}

/// **The tree's root**: RFC 6962's Merkle tree hash over the leaves in admission order — the
/// frontier folded from its smallest subtree up (`root = node(f[i], root)`). The empty tree's root is
/// `H("…/material-empty/v1", "")`.
pub fn palw_improve_material_root_v1(frontier: &PalwMaterialFrontierV1) -> Hash64 {
    let mut parts = frontier.frontier.iter().rev();
    let Some(first) = parts.next() else { return keyed64(PALW_IMPROVE_MATERIAL_EMPTY_DOMAIN_V1, &[]) };
    parts.fold(*first, |root, left| node(left, &root))
}

/// **`dataset_root`** (spec 17 §17.6.1): `H("…/dataset-root/v1", LE u32 count ‖ root)`.
pub fn palw_improve_dataset_root_v1(frontier: &PalwMaterialFrontierV1) -> Hash64 {
    keyed64(
        PALW_IMPROVE_DATASET_ROOT_DOMAIN_V1,
        &[&frontier.count.to_le_bytes(), palw_improve_material_root_v1(frontier).as_byte_slice()],
    )
}

// ---- the draw (spec 17 §17.8.1) ----

/// **The epoch seed**: `H("…/epoch-seed/v1", block_hash ‖ line_id ‖ LE u64 epoch)`, the block being
/// the chain block whose fold performs the draw.
pub fn palw_improve_epoch_seed_v1(block: &Hash64, line_id: &Hash64, epoch: u64) -> Hash64 {
    keyed64(PALW_IMPROVE_EPOCH_SEED_DOMAIN_V1, &[block.as_byte_slice(), line_id.as_byte_slice(), &epoch.to_le_bytes()])
}

/// A setter item's id: `H("…/setter-item/v1", set_id ‖ LE u32 i)`.
pub fn palw_improve_setter_item_id_v1(set_id: &Hash64, index: u32) -> Hash64 {
    keyed64(PALW_IMPROVE_SETTER_ITEM_DOMAIN_V1, &[set_id.as_byte_slice(), &index.to_le_bytes()])
}

/// A suite item's id: `H("…/suite-item/v1", dataset_id ‖ LE u32 entry)` — the suite's registered dataset and the
/// entry drawn from it (spec 17 §17.8.1).
pub fn palw_improve_suite_item_id_v1(dataset_id: &Hash64, entry: u32) -> Hash64 {
    keyed64(PALW_IMPROVE_SUITE_ITEM_DOMAIN_V1, &[dataset_id.as_byte_slice(), &entry.to_le_bytes()])
}

/// **The entries a suite draws from its dataset** (spec 17 §17.8.1; RFC-0004 §7 as decided 2026-09-30): `min(count, n)`
/// distinct entry indices of the dataset's `n` entries, by a sparse Fisher–Yates shuffle driven by the epoch seed —
/// for `j = 0, 1, …`: `r_j = LE u64(H("…/suite-draw/v1", seed ‖ dataset_id ‖ role ‖ LE u32 j)[0..8]) mod (n − j)`, the
/// entry at virtual position `j + r_j` is drawn and the entry at `j` takes its place. `role` is 1 for the regression
/// suite and 2 for the safety suite, so two suites naming one dataset do not draw the same entries. Cost
/// `O(count · log count)`, whatever `n` is.
pub fn palw_improve_suite_draw_v1(seed: &Hash64, dataset_id: &Hash64, role: u8, n: u32, count: u32) -> Vec<u32> {
    let k = count.min(n);
    let mut moved: std::collections::BTreeMap<u32, u32> = std::collections::BTreeMap::new();
    let at = |moved: &std::collections::BTreeMap<u32, u32>, position: u32| moved.get(&position).copied().unwrap_or(position);
    let mut drawn = Vec::with_capacity(k as usize);
    for j in 0..k {
        let h =
            keyed64(PALW_IMPROVE_SUITE_DRAW_DOMAIN_V1, &[seed.as_byte_slice(), dataset_id.as_byte_slice(), &[role], &j.to_le_bytes()]);
        let mut word = [0u8; 8];
        word.copy_from_slice(&h.as_byte_slice()[0..8]);
        let position = j + (u64::from_le_bytes(word) % (n - j) as u64) as u32;
        let (here, there) = (at(&moved, j), at(&moved, position));
        drawn.push(there);
        moved.insert(position, here);
    }
    drawn
}

/// An entry's order key: `H("…/draw/v1", seed ‖ entry id)`.
pub fn palw_improve_draw_key_v1(seed: &Hash64, entry_id: &Hash64) -> Hash64 {
    keyed64(PALW_IMPROVE_DRAW_DOMAIN_V1, &[seed.as_byte_slice(), entry_id.as_byte_slice()])
}

/// One entry of the evaluation pool as the draw sees it: its id and its supplier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwDrawEntryV1 {
    pub id: Hash64,
    pub supplier: PalwBondKeyV2,
}

/// **The draw** (spec 17 §17.8.1) [E7], [E10]: the pool sorted by order key (bytes, ascending), taken
/// in that order, skipping an entry whose supplier already supplies `cap = max(1, ⌊κ·n/1000⌋)` drawn
/// items, stopping at `n`. Without replacement; a short pool gives all it can. Returns indices into
/// `pool`, in draw order.
pub fn palw_improve_draw_v1(seed: &Hash64, pool: &[PalwDrawEntryV1], n: u32, setter_cap_permille: u16) -> Vec<usize> {
    let cap = ((setter_cap_permille as u64 * n as u64) / 1000).max(1);
    let mut order: Vec<(Hash64, usize)> =
        pool.iter().enumerate().map(|(i, entry)| (palw_improve_draw_key_v1(seed, &entry.id), i)).collect();
    order.sort_by(|a, b| a.0.as_byte_slice().cmp(b.0.as_byte_slice()).then(a.1.cmp(&b.1)));
    let mut supplied: Vec<(PalwBondKeyV2, u64)> = Vec::new();
    let mut drawn = Vec::with_capacity(n as usize);
    for (_, index) in order {
        if drawn.len() as u64 >= n as u64 {
            break;
        }
        let supplier = pool[index].supplier;
        let count = match supplied.iter_mut().find(|(s, _)| *s == supplier) {
            Some((_, count)) => count,
            None => {
                supplied.push((supplier, 0));
                &mut supplied.last_mut().expect("just pushed").1
            }
        };
        if *count >= cap {
            continue;
        }
        *count += 1;
        drawn.push(index);
    }
    drawn
}

/// **The judge of a drawn item**: `judge_set[LE u64(H("…/judge/v1", seed ‖ LE u32 i)[0..8]) mod |set|]`.
/// `None` for an empty set.
pub fn palw_improve_judge_index_v1(seed: &Hash64, item: u32, set_len: usize) -> Option<usize> {
    if set_len == 0 {
        return None;
    }
    let h = keyed64(PALW_IMPROVE_JUDGE_DOMAIN_V1, &[seed.as_byte_slice(), &item.to_le_bytes()]);
    let mut word = [0u8; 8];
    word.copy_from_slice(&h.as_byte_slice()[0..8]);
    Some((u64::from_le_bytes(word) % set_len as u64) as usize)
}

// The pairwise order was retired by the 2026-09-30 decision: a Pairwise score shows the judge both orders
// (spec 17 §17.8.5), so no order is drawn.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::TransactionOutpoint;

    fn h(byte: u8) -> Hash64 {
        Hash64::from_bytes([byte; 64])
    }

    fn bond(byte: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: h(byte), index: 0 })
    }

    /// RFC 6962's MTH, recursively, as the definition the frontier must agree with.
    fn mth(leaves: &[Hash64]) -> Hash64 {
        match leaves.len() {
            0 => keyed64(PALW_IMPROVE_MATERIAL_EMPTY_DOMAIN_V1, &[]),
            1 => leaves[0],
            n => {
                let k = n.next_power_of_two() / 2;
                node(&mth(&leaves[..k]), &mth(&leaves[k..]))
            }
        }
    }

    #[test]
    fn the_frontier_root_is_rfc6962s_tree_hash() {
        let mut frontier = PalwMaterialFrontierV1::default();
        let mut leaves = Vec::new();
        assert_eq!(palw_improve_material_root_v1(&frontier), mth(&leaves));
        for i in 0..70u8 {
            let leaf = palw_improve_material_leaf_v1(PalwMaterialKindV1::HardCase, &h(i));
            palw_improve_material_append_v1(&mut frontier, leaf);
            leaves.push(leaf);
            assert_eq!(palw_improve_material_root_v1(&frontier), mth(&leaves), "{} leaves", leaves.len());
            assert_eq!(frontier.frontier.len() as u32, frontier.count.count_ones(), "one subtree per set bit");
        }
        assert_ne!(palw_improve_dataset_root_v1(&frontier), palw_improve_material_root_v1(&frontier), "the count is bound");
        let dataset = palw_improve_material_leaf_v1(PalwMaterialKindV1::Dataset, &h(1));
        assert_ne!(dataset, palw_improve_material_leaf_v1(PalwMaterialKindV1::HardCase, &h(1)), "the kind is bound");
    }

    #[test]
    fn times_and_boundaries() {
        let w = PalwEpochWindowsV1 {
            grid: 1_000,
            w_collect: 200,
            w_submit: 200,
            w_holdout: 100,
            w_eval: 300,
            beacon_delay: 10,
            court_margin: 150,
        };
        let t = palw_improve_epoch_times_v1(5_000, &w);
        assert_eq!((t.t_fix, t.t_close, t.t_draw, t.t_eval, t.t_score), (5_200, 5_400, 5_500, 5_800, 5_950));
        assert_eq!(palw_improve_grid_floor_v1(5_999, 1_000), 5_000);
        assert_eq!(palw_improve_next_boundary_v1(5_000, 1_000), 6_000, "strictly after");
        assert_eq!(palw_improve_next_boundary_v1(4_999, 1_000), 5_000);
    }

    #[test]
    fn the_draw_is_ordered_capped_and_without_replacement() {
        let seed = h(7);
        let pool: Vec<PalwDrawEntryV1> = (0..40u8).map(|i| PalwDrawEntryV1 { id: h(i), supplier: bond(i % 4) }).collect();
        let drawn = palw_improve_draw_v1(&seed, &pool, 12, 250);
        assert_eq!(drawn.len(), 12);
        let mut unique = drawn.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 12, "no replacement");
        for supplier in 0..4u8 {
            assert!(drawn.iter().filter(|&&i| pool[i].supplier == bond(supplier)).count() <= 3, "κ·n/1000 = 3 per supplier");
        }
        let keys: Vec<Hash64> = drawn.iter().map(|&i| palw_improve_draw_key_v1(&seed, &pool[i].id)).collect();
        assert!(keys.windows(2).all(|w| w[0].as_byte_slice() < w[1].as_byte_slice()), "in order-key order");
        assert_eq!(palw_improve_draw_v1(&seed, &pool[..5], 12, 1000).len(), 5, "a short pool gives all it can");
        assert_eq!(palw_improve_draw_v1(&h(8), &pool, 12, 250) == drawn, false, "another seed, another draw");
        // One supplier and a cap of max(1, ⌊κ·n⌋): 1 item.
        let lone: Vec<PalwDrawEntryV1> = (0..10u8).map(|i| PalwDrawEntryV1 { id: h(i), supplier: bond(1) }).collect();
        assert_eq!(palw_improve_draw_v1(&seed, &lone, 3, 100).len(), 1);
    }

    #[test]
    fn a_suite_draws_distinct_entries_of_its_dataset_whatever_its_size() {
        let (seed, dataset) = (h(5), h(6));
        let drawn = palw_improve_suite_draw_v1(&seed, &dataset, 1, 1_000, 32);
        assert_eq!(drawn.len(), 32);
        let unique: std::collections::BTreeSet<u32> = drawn.iter().copied().collect();
        assert_eq!(unique.len(), 32, "no replacement");
        assert!(drawn.iter().all(|&e| e < 1_000));
        assert_eq!(drawn, palw_improve_suite_draw_v1(&seed, &dataset, 1, 1_000, 32), "deterministic");
        assert_ne!(drawn, palw_improve_suite_draw_v1(&h(7), &dataset, 1, 1_000, 32), "another seed");
        assert_ne!(drawn, palw_improve_suite_draw_v1(&seed, &dataset, 2, 1_000, 32), "another role: another suite's entries");
        // A prefix of a longer draw: the same first entries (the shuffle is sequential).
        assert_eq!(drawn[..8], palw_improve_suite_draw_v1(&seed, &dataset, 1, 1_000, 8)[..]);
        // A dataset smaller than the suite gives all of it; a huge one costs the same.
        let all = palw_improve_suite_draw_v1(&seed, &dataset, 1, 5, 32);
        assert_eq!(all.len(), 5);
        assert_eq!(all.iter().copied().collect::<std::collections::BTreeSet<u32>>(), (0..5).collect());
        assert_eq!(palw_improve_suite_draw_v1(&seed, &dataset, 1, u32::MAX, 32).len(), 32);
        assert!(palw_improve_suite_draw_v1(&seed, &dataset, 1, 0, 32).is_empty());
        // Over a small dataset every entry comes up (a permutation), in a seed-dependent order.
        let order = palw_improve_suite_draw_v1(&seed, &dataset, 1, 20, 20);
        assert_eq!(order.iter().copied().collect::<std::collections::BTreeSet<u32>>(), (0..20).collect());
    }

    #[test]
    fn judges_are_seeded() {
        let seed = h(9);
        assert_eq!(palw_improve_judge_index_v1(&seed, 0, 0), None);
        let spread: std::collections::BTreeSet<usize> = (0..64).filter_map(|i| palw_improve_judge_index_v1(&seed, i, 4)).collect();
        assert_eq!(spread.len(), 4, "every judge is drawn somewhere in 64 items");
    }
}
