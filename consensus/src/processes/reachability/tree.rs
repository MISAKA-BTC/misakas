//!
//! Tree-related functions internal to the module
//!
use super::{extensions::ReachabilityStoreIntervalExtensions, inquirer::*, interval::Interval, reindex::ReindexOperationContext, *};
use crate::model::stores::reachability::ReachabilityStore;
use kaspa_consensus_core::BlockHash;

/// ADR-0043 (A) — beyond this many direct children under one parent, new-child interval allocation
/// switches from `remaining/2` (halving: only ~log₂(capacity) insertions before exhaustion) to
/// `remaining/(2·n)` (harmonic: the reserve absorbs further insertions effectively without limit).
/// Honest concurrent width under a single parent is a few tens at most; only a flood crosses this.
const SIBLING_FLOOD_ALLOC_THRESHOLD: usize = 64;

/// Adds `new_block` as a child of `parent` in the tree structure. If this block
/// has no remaining interval to allocate, a reindexing is triggered. When a reindexing
/// is triggered, the reindex root point is used within the reindex algorithm's logic
pub fn add_tree_block(
    store: &mut (impl ReachabilityStore + ?Sized),
    new_block: BlockHash,
    parent: BlockHash,
    reindex_depth: u64,
    reindex_slack: u64,
) -> Result<()> {
    // Get the remaining interval capacity
    let remaining = store.interval_remaining_after(parent)?;
    // Append the new child to `parent.children`
    store.append_child(parent, new_block)?;
    let parent_height = store.get_height(parent)?;
    if remaining.is_empty() {
        // Init with the empty interval.
        // Note: internal logic relies on interval being this specific interval
        //       which comes exactly at the end of current capacity
        store.insert(new_block, parent, remaining, parent_height + 1)?;

        // Start a reindex operation (TODO: add timing)
        let reindex_root = store.get_reindex_root()?;
        let mut ctx = ReindexOperationContext::new(store, reindex_depth, reindex_slack);
        ctx.reindex_intervals(new_block, reindex_root)?;
    } else {
        // ADR-0043 (A): under a flooded parent, hand out a harmonic share instead of half of the
        // remaining capacity. Halving grants subtree headroom flood children (leaves, by attacker
        // economics) never use, and exhausts the parent after ~64 insertions — after which EVERY
        // further sibling triggers an O(subtree) reindex. The harmonic share keeps per-insertion
        // cost O(1) essentially forever; honest-width parents keep the original behavior.
        let n = store.get_children(parent)?.len();
        let allocated = if n > SIBLING_FLOOD_ALLOC_THRESHOLD {
            let share = (remaining.size() / (2 * n as u64)).max(1);
            Interval::new(remaining.start, remaining.start + share - 1)
        } else {
            remaining.split_half().0
        };
        store.insert(new_block, parent, allocated, parent_height + 1)?;
    };
    Ok(())
}

/// Finds the most recent tree ancestor common to both `block` and the given `reindex root`.
/// Note that we assume that almost always the chain between the reindex root and the common
/// ancestor is longer than the chain between block and the common ancestor, hence we iterate
/// from `block`.
pub fn find_common_tree_ancestor(
    store: &(impl ReachabilityStore + ?Sized),
    block: BlockHash,
    reindex_root: BlockHash,
) -> Result<BlockHash> {
    let mut current = block;
    loop {
        if is_chain_ancestor_of(store, current, reindex_root)? {
            return Ok(current);
        }
        current = store.get_parent(current)?;
    }
}

/// Finds a possible new reindex root, based on the `current` reindex root and the selected tip `hint`
pub fn find_next_reindex_root(
    store: &(impl ReachabilityStore + ?Sized),
    current: BlockHash,
    hint: BlockHash,
    reindex_depth: u64,
    reindex_slack: u64,
) -> Result<(BlockHash, BlockHash)> {
    let mut ancestor = current;
    let mut next = current;

    let hint_height = store.get_height(hint)?;

    // Test if current root is ancestor of selected tip (`hint`) - if not, this is a reorg case
    if !is_chain_ancestor_of(store, current, hint)? {
        let current_height = store.get_height(current)?;

        // We have reindex root out of (hint) selected tip chain, however we switch chains only after a sufficient
        // threshold of `reindex_slack` diff in order to address possible alternating reorg attacks.
        // The `reindex_slack` constant is used as an heuristic large enough on the one hand, but
        // one which will not harm performance on the other hand - given the available slack at the chain split point.
        //
        // Note: In some cases the height of the (hint) selected tip can be lower than the current reindex root height.
        // If that's the case we keep the reindex root unchanged.
        if hint_height < current_height || hint_height - current_height < reindex_slack {
            return Ok((current, current));
        }

        let common = find_common_tree_ancestor(store, hint, current)?;
        ancestor = common;
        next = common;
    }

    // Iterate from ancestor towards the selected tip (`hint`) until passing the
    // `reindex_window` threshold, for finding the new reindex root
    loop {
        let child = get_next_chain_ancestor_unchecked(store, hint, next)?;
        let child_height = store.get_height(child)?;

        if hint_height < child_height {
            return Err(ReachabilityError::DataInconsistency);
        }
        if hint_height - child_height < reindex_depth {
            break;
        }
        next = child;
    }

    Ok((ancestor, next))
}

/// Attempts to advance or move the current reindex root according to the
/// provided `virtual selected parent` (`VSP`) hint.
/// It is important for the reindex root point to follow the consensus-agreed chain
/// since this way it can benefit from chain-robustness which is implied by the security
/// of the ordering protocol. That is, it enjoys from the fact that all future blocks are
/// expected to elect the root subtree (by converging to the agreement to have it on the
/// selected chain). See also the reachability algorithms overview (TODO)
pub fn try_advancing_reindex_root(
    store: &mut (impl ReachabilityStore + ?Sized),
    hint: BlockHash,
    reindex_depth: u64,
    reindex_slack: u64,
) -> Result<()> {
    // Get current root from the store
    let current = store.get_reindex_root()?;

    // Find the possible new root
    let (mut ancestor, next) = find_next_reindex_root(store, current, hint, reindex_depth, reindex_slack)?;

    // No update to root, return
    if current == next {
        return Ok(());
    }

    // if ancestor == next {
    //     trace!("next reindex root is an ancestor of current one, skipping concentration.")
    // }
    while ancestor != next {
        let child = get_next_chain_ancestor_unchecked(store, next, ancestor)?;
        let mut ctx = ReindexOperationContext::new(store, reindex_depth, reindex_slack);
        ctx.concentrate_interval(ancestor, child, child == next)?;
        ancestor = child;
    }

    // Update reindex root in the data store
    store.set_reindex_root(next)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::stores::reachability::{MemoryReachabilityStore, ReachabilityStoreReader};
    use crate::processes::reachability::tests::{StoreValidationExtensions, TreeBuilder};

    /// ADR-0043 (A) — the G6 shape at unit level: a sibling flood under ONE parent must not
    /// degenerate into per-insertion reindexing. With the pre-amendment allocator (halving +
    /// full-capacity re-tiles) this loop exhausts the parent after ~62 insertions and then
    /// reindexes on EVERY further insertion (~940 events for 1,000 siblings). With the harmonic
    /// allocator + trailing reserve the whole flood triggers at most a couple of reindex events,
    /// which is the amortized-O(1) bound the amendment claims.
    #[test]
    fn sibling_flood_reindexes_are_amortized_constant() {
        let mut store = MemoryReachabilityStore::new();
        let root: BlockHash = 1.into();
        let parent: BlockHash = 2.into();
        TreeBuilder::new(&mut store).init_with_params(root, Interval::maximal()).add_block(parent, root);

        let mut reindex_events = 0u64;
        for i in 0..1_000u64 {
            if store.interval_remaining_after(parent).unwrap().is_empty() {
                reindex_events += 1;
            }
            add_tree_block(&mut store, (10 + i).into(), parent, 100, 1 << 14).unwrap();
        }
        assert!(
            reindex_events <= 2,
            "a 1,000-sibling flood must amortize to O(1) reindex events, got {reindex_events}"
        );
        // The reachability invariants survive the flood allocation policy.
        store.validate_intervals(root).unwrap();
    }

    /// The honest-width regime (n ≤ threshold) keeps the original halving allocator: the first
    /// child of a fresh parent still receives half of the remaining capacity.
    #[test]
    fn honest_width_keeps_halving_allocation() {
        let mut store = MemoryReachabilityStore::new();
        let root: BlockHash = 1.into();
        let parent: BlockHash = 2.into();
        TreeBuilder::new(&mut store).init_with_params(root, Interval::new(1, 1_000_000)).add_block(parent, root);
        let before = store.interval_remaining_after(parent).unwrap();
        add_tree_block(&mut store, 10.into(), parent, 100, 1 << 14).unwrap();
        let child_size = store.get_interval(10.into()).unwrap().size();
        assert_eq!(child_size, before.split_half().0.size(), "below the flood threshold the halving policy is unchanged");
        store.validate_intervals(root).unwrap();
    }
}
