//! **Parents before children, without asking blue work.**
//!
//! Upstream Kaspa hands blocks to the pipeline in `(blue_work, hash)` order and calls that order
//! topological, and there it is: a block's selected parent is its heaviest parent and adds its own
//! work, so every block is strictly heavier than each of its parents. ADR-0125 ends that on a
//! network that opens the round lane. A round block is never a selected parent and never blue, and
//! a round block whose parents are all round blocks takes their anchor as its own selected parent —
//! so every round block hanging from one anchor weighs exactly `blue_work(anchor) + work(anchor)`,
//! and so does a block whose only blue is that anchor. Blue work is still never LOWER than a
//! parent's, but it is often EQUAL, and among equals `(blue_work, hash)` falls back to the hash,
//! which says nothing about which block is whose parent.
//!
//! testnet-12, DAA 316 (2026-09-26): the first lane — twenty round blocks, each naming the previous
//! one, all anchored at heartbeat `ccfa9be7…` — carries ONE blue work, `0x2ba00221`, and every
//! "blue-work order" of it is hash order. A syncing node was sent round block `0c2d65dd…` before
//! its parent `656391c6…`, refused it with `MissingParents`, dropped the peer and started again —
//! against every peer, every time.
//!
//! This module is the node's answer and changes nothing consensus computes: a **stable** reorder
//! that puts every item after each of its parents that is in the same list, and otherwise keeps the
//! input order. Where the input already had that property the output IS the input, item for item,
//! so on a DAG whose blue work is strictly monotone — every upstream network, and every stretch of
//! this one without a round lane — a node hands consensus exactly what it handed it before.
//!
//! The consensus orders that rest on the same premise (the acceptance order of a mergeset among
//! them) are NOT touched here: changing them changes what a block accepts, which is a fence.

use crate::{BlockHash, BlockHashMap, BlockHashSet, HashMapCustomHasher, header::Header};
use std::{cmp::Reverse, collections::BinaryHeap, sync::Arc};

/// **`items`, reordered — only as far as it must be — so that no item precedes one of its parents.**
///
/// `hash_of` names an item; `parents_of` lists its parents, of which only those present in `items`
/// constrain the order. Each output position takes the earliest remaining input item whose in-list
/// parents have all been placed, so:
///
/// * an input that already lists every parent before its children is returned untouched (a single
///   pass decides that, and nothing is moved);
/// * otherwise the only items that move are ones that had to, and ties keep their input order.
///
/// A list that names one block twice is returned as it came: which copy a parent constraint would
/// refer to is not well defined, and consensus already settles duplicates by itself. A parent cycle
/// cannot exist among hash-committed headers; if one were passed anyway, whatever it leaves unplaced
/// follows the rest in input order rather than being dropped.
pub fn stable_topological_order<T, H, P, I>(items: Vec<T>, mut hash_of: H, mut parents_of: P) -> Vec<T>
where
    H: FnMut(&T) -> BlockHash,
    P: FnMut(&T) -> I,
    I: IntoIterator<Item = BlockHash>,
{
    if items.len() < 2 {
        return items;
    }
    let mut position: BlockHashMap<usize> = BlockHashMap::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        if position.insert(hash_of(item), i).is_some() {
            return items;
        }
    }
    let mut out_of_order = false;
    let parents: Vec<Vec<usize>> = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let mut in_list: Vec<usize> =
                parents_of(item).into_iter().filter_map(|parent| position.get(&parent).copied()).filter(|&j| j != i).collect();
            in_list.sort_unstable();
            in_list.dedup();
            out_of_order |= in_list.last().is_some_and(|&j| j > i);
            in_list
        })
        .collect();
    if !out_of_order {
        return items;
    }
    let order = earliest_ready_first(&parents);
    let mut slots: Vec<Option<T>> = items.into_iter().map(Some).collect();
    order.into_iter().map(|i| slots[i].take().expect("every position is emitted exactly once")).collect()
}

/// Kahn's algorithm with the ready set kept as a min-heap of input positions: the earliest ready
/// item always goes next, which is what makes the reorder stable.
fn earliest_ready_first(parents: &[Vec<usize>]) -> Vec<usize> {
    let n = parents.len();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut waiting_on: Vec<usize> = Vec::with_capacity(n);
    for (i, in_list) in parents.iter().enumerate() {
        waiting_on.push(in_list.len());
        for &parent in in_list {
            children[parent].push(i);
        }
    }
    let mut ready: BinaryHeap<Reverse<usize>> = (0..n).filter(|&i| waiting_on[i] == 0).map(Reverse).collect();
    let mut order = Vec::with_capacity(n);
    let mut placed = vec![false; n];
    while let Some(Reverse(i)) = ready.pop() {
        order.push(i);
        placed[i] = true;
        for &child in &children[i] {
            waiting_on[child] -= 1;
            if waiting_on[child] == 0 {
                ready.push(Reverse(child));
            }
        }
    }
    if order.len() < n {
        order.extend((0..n).filter(|&i| !placed[i]));
    }
    order
}

/// Whether every item comes after each of its parents that is in the same list.
pub fn is_parent_first<T, H, P, I>(items: &[T], mut hash_of: H, mut parents_of: P) -> bool
where
    H: FnMut(&T) -> BlockHash,
    P: FnMut(&T) -> I,
    I: IntoIterator<Item = BlockHash>,
{
    let mut seen: BlockHashSet = BlockHashSet::with_capacity(items.len());
    let all: BlockHashSet = items.iter().map(&mut hash_of).collect();
    items.iter().all(|item| {
        let ok = parents_of(item).into_iter().all(|parent| !all.contains(&parent) || seen.contains(&parent));
        seen.insert(hash_of(item));
        ok
    })
}

/// [`stable_topological_order`] over headers, by their direct parents.
pub fn headers_parent_first(headers: Vec<Arc<Header>>) -> Vec<Arc<Header>> {
    stable_topological_order(headers, |header| header.hash, |header| header.direct_parents().to_vec())
}

/// **The syncing node's half: which headers consensus can take now, and which must wait.**
///
/// Orders `headers` parents-first ([`headers_parent_first`]) and walks them: a header is released
/// when each of its parents is either `available` — consensus holds it, or it was handed over
/// earlier — or released ahead of it in this same walk; otherwise it is held, and so is anything
/// under it. Returns `(released, held)`, both parents-first, released in the order to hand them over.
///
/// Holding a header back instead of handing it over is what turns a syncer's out-of-order batch
/// from a failed IBD round (`MissingParents`, peer dropped) into a short wait for the parent's
/// batch. When every parent is available the result is `(headers, [])` in the input order.
pub fn release_parent_first(
    headers: Vec<Arc<Header>>,
    mut available: impl FnMut(BlockHash) -> bool,
) -> (Vec<Arc<Header>>, Vec<Arc<Header>>) {
    let ordered = headers_parent_first(headers);
    let mut released_hashes: BlockHashSet = BlockHashSet::with_capacity(ordered.len());
    let mut released = Vec::with_capacity(ordered.len());
    let mut held = Vec::new();
    for header in ordered {
        if header.direct_parents().iter().all(|parent| released_hashes.contains(parent) || available(*parent)) {
            released_hashes.insert(header.hash);
            released.push(header);
        } else {
            held.push(header);
        }
    }
    (released, held)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(word: u64) -> BlockHash {
        BlockHash::from_u64_word(word)
    }

    fn header(hash: u64, parents: &[u64]) -> Arc<Header> {
        Arc::new(Header::from_precomputed_hash(h(hash), parents.iter().copied().map(h).collect()))
    }

    fn hashes(headers: &[Arc<Header>]) -> Vec<BlockHash> {
        headers.iter().map(|header| header.hash).collect()
    }

    /// The testnet-12 shape: a chain block `A` (1), a lane hanging from it whose blocks tie on blue
    /// work — `r1 ← r2 ← r3` — and hash order putting the lane backwards.
    fn lane_in_hash_order() -> Vec<Arc<Header>> {
        vec![header(1, &[0]), header(4, &[3]), header(3, &[2]), header(2, &[1])]
    }

    #[test]
    fn an_order_that_is_already_parent_first_comes_back_untouched() {
        let input = vec![header(1, &[0]), header(7, &[1]), header(2, &[1]), header(9, &[7, 2]), header(3, &[99])];
        assert!(is_parent_first(&input, |x| x.hash, |x| x.direct_parents().to_vec()));
        let output = headers_parent_first(input.clone());
        assert_eq!(hashes(&output), hashes(&input));
        // Item for item, not merely hash for hash: the very same allocations come back.
        assert!(output.iter().zip(input.iter()).all(|(a, b)| Arc::ptr_eq(a, b)));
    }

    #[test]
    fn a_lane_sorted_by_hash_comes_out_parents_first() {
        let input = lane_in_hash_order();
        assert!(!is_parent_first(&input, |x| x.hash, |x| x.direct_parents().to_vec()));
        let output = headers_parent_first(input);
        assert_eq!(hashes(&output), vec![h(1), h(2), h(3), h(4)]);
        assert!(is_parent_first(&output, |x| x.hash, |x| x.direct_parents().to_vec()));
    }

    #[test]
    fn only_what_must_move_moves_and_ties_keep_their_input_order() {
        // 10 and 11 are unrelated to the lane and to each other; they keep their places relative to
        // each other and to the lane's first block, and the lane is put right.
        let input = vec![header(10, &[0]), header(4, &[3]), header(11, &[0]), header(3, &[2]), header(2, &[0])];
        let output = headers_parent_first(input);
        assert_eq!(hashes(&output), vec![h(10), h(11), h(2), h(3), h(4)]);
    }

    #[test]
    fn a_child_named_twice_or_a_parent_named_twice_is_handled() {
        // A header naming the same parent twice is one constraint, not two.
        let input = vec![header(3, &[2, 2]), header(2, &[0])];
        assert_eq!(hashes(&headers_parent_first(input)), vec![h(2), h(3)]);
        // A list naming one block twice is left alone.
        let input = vec![header(3, &[2]), header(2, &[0]), header(3, &[2])];
        assert_eq!(hashes(&headers_parent_first(input.clone())), hashes(&input));
    }

    #[test]
    fn a_cycle_cannot_lose_an_item() {
        // Impossible with hash-committed parents; the reorder still returns every item.
        let input = vec![header(5, &[0]), header(1, &[2]), header(2, &[1])];
        let output = headers_parent_first(input);
        assert_eq!(hashes(&output), vec![h(5), h(1), h(2)]);
    }

    #[test]
    fn the_general_form_orders_bare_hashes_by_a_parent_lookup() {
        let parents = |x: &BlockHash| -> Vec<BlockHash> {
            match x {
                x if *x == h(4) => vec![h(3)],
                x if *x == h(3) => vec![h(2)],
                x if *x == h(2) => vec![h(1)],
                _ => vec![h(0)],
            }
        };
        let output = stable_topological_order(vec![h(1), h(4), h(3), h(2)], |x| *x, parents);
        assert_eq!(output, vec![h(1), h(2), h(3), h(4)]);
        let already = vec![h(1), h(2), h(3), h(4)];
        assert_eq!(stable_topological_order(already.clone(), |x| *x, parents), already);
    }

    #[test]
    fn release_holds_back_what_waits_on_a_parent_still_to_come() {
        // Chunk one carries the lane's tail before its head, and the head's parent (2) is in the
        // NEXT chunk: 3 and 4 are held, 10 goes now.
        let known = |p: BlockHash| p == h(0);
        let (released, held) = release_parent_first(vec![header(4, &[3]), header(10, &[0]), header(3, &[2])], known);
        assert_eq!(hashes(&released), vec![h(10)]);
        assert_eq!(hashes(&held), vec![h(3), h(4)], "held parents-first, ready to follow their parent");
        // Chunk two brings 2; the held headers ride along behind it.
        let mut batch = held;
        batch.push(header(2, &[0]));
        let (released, held) = release_parent_first(batch, known);
        assert_eq!(hashes(&released), vec![h(2), h(3), h(4)]);
        assert!(held.is_empty());
    }

    #[test]
    fn release_of_a_well_ordered_batch_is_the_batch() {
        let input = vec![header(1, &[0]), header(2, &[1]), header(3, &[2, 1])];
        let (released, held) = release_parent_first(input.clone(), |p| p == h(0));
        assert!(held.is_empty());
        assert!(released.iter().zip(input.iter()).all(|(a, b)| Arc::ptr_eq(a, b)));
    }
}
