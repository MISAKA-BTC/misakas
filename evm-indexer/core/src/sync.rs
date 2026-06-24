//! §10.5 — the reorg-reconcile planner: the service's brain, kept pure (no IO).
//!
//! After a WebSocket gap (or on startup) the indexer compares its locally
//! indexed chain against the node's canonical chain and decides what to undo
//! and what to (re)apply. This module answers that with NO IO: given the local
//! head and two "block at height" lookups (local + node), it returns the set of
//! local blocks to detach (newest-first, so balances unwind correctly) and the
//! first height to re-apply from. The service executes the plan against its
//! store + the node RPC; this logic is unit-tested in isolation.

/// One block's identity at a height (its eth-rpc hash). Two blocks at the same
/// height with different hashes are a reorg.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockId {
    pub number: u64,
    pub rpc_hash: [u8; 32],
}

/// The reconcile actions: detach `revert` (newest-first) then (re)apply node
/// blocks `apply_from..=node_head` (the caller bounds the upper end at the
/// node's head).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncPlan {
    pub revert: Vec<[u8; 32]>,
    pub apply_from: u64,
}

/// Compute the reconcile plan. `local_at(n)` / `node_at(n)` return the canonical
/// block each chain holds at height `n` (or `None`). Walks down from the local
/// head to the first height where the two agree (the common ancestor): every
/// local block above it is reverted, and re-application starts just above it. An
/// empty local index re-applies from height 0; a total divergence reverts
/// everything and re-applies from 0.
pub fn plan_reconcile<L, N>(local_head: Option<BlockId>, local_at: L, node_at: N) -> SyncPlan
where
    L: Fn(u64) -> Option<BlockId>,
    N: Fn(u64) -> Option<BlockId>,
{
    let Some(head) = local_head else {
        return SyncPlan { revert: Vec::new(), apply_from: 0 };
    };
    let mut revert = Vec::new();
    let mut n = head.number;
    loop {
        match (local_at(n), node_at(n)) {
            // Common ancestor: the chains agree at this height. Re-apply above it.
            (Some(l), Some(node)) if l.rpc_hash == node.rpc_hash => {
                return SyncPlan { revert, apply_from: n + 1 };
            }
            // Local block at this height diverges (or the node has nothing here) —
            // detach it.
            (Some(l), _) => revert.push(l.rpc_hash),
            // No local block at this height (below the indexed range) — nothing to
            // detach; keep walking in case a lower height still agrees.
            (None, _) => {}
        }
        if n == 0 {
            break;
        }
        n -= 1;
    }
    // No agreement down to genesis: re-apply everything.
    SyncPlan { revert, apply_from: 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(number: u64, tag: u8) -> BlockId {
        BlockId { number, rpc_hash: [tag; 32] }
    }

    /// No reorg: the node agrees at the local head → nothing reverts, apply resumes
    /// just above the head.
    #[test]
    fn no_reorg_resumes_above_head() {
        let local = |n: u64| (n <= 5).then(|| id(n, n as u8));
        let node = |n: u64| (n <= 8).then(|| id(n, n as u8));
        let plan = plan_reconcile(Some(id(5, 5)), local, node);
        assert_eq!(plan, SyncPlan { revert: vec![], apply_from: 6 });
    }

    /// A 2-block reorg: local 4,5 differ from the node; common ancestor at 3.
    /// Reverts 5 then 4 (newest-first), re-applies from 4.
    #[test]
    fn reorg_reverts_newest_first_to_common_ancestor() {
        // local: heights 0..=5 with tag = height (so 4→[4;32], 5→[5;32]).
        let local = |n: u64| (n <= 5).then(|| id(n, n as u8));
        // node: agrees up to 3, diverges at 4,5 (tag = height + 100).
        let node = |n: u64| match n {
            0..=3 => Some(id(n, n as u8)),
            4..=9 => Some(id(n, (n + 100) as u8)),
            _ => None,
        };
        let plan = plan_reconcile(Some(id(5, 5)), local, node);
        assert_eq!(plan.revert, vec![[5u8; 32], [4u8; 32]], "newest-first");
        assert_eq!(plan.apply_from, 4, "re-apply from just above the common ancestor (3)");
    }

    /// Local is ahead of a node that rolled back below the local head: still finds
    /// the ancestor and reverts the now-orphaned local blocks.
    #[test]
    fn local_ahead_of_rolled_back_node() {
        let local = |n: u64| (n <= 7).then(|| id(n, n as u8));
        // node only has up to height 4 (agrees), nothing at 5..7.
        let node = |n: u64| (n <= 4).then(|| id(n, n as u8));
        let plan = plan_reconcile(Some(id(7, 7)), local, node);
        assert_eq!(plan.revert, vec![[7u8; 32], [6u8; 32], [5u8; 32]]);
        assert_eq!(plan.apply_from, 5);
    }

    #[test]
    fn empty_local_applies_from_genesis() {
        let none = |_n: u64| None;
        assert_eq!(plan_reconcile(None, none, none), SyncPlan { revert: vec![], apply_from: 0 });
    }

    /// Total divergence (even genesis differs): revert all, re-apply from 0.
    #[test]
    fn total_divergence_reapplies_from_zero() {
        let local = |n: u64| (n <= 2).then(|| id(n, n as u8));
        let node = |n: u64| (n <= 2).then(|| id(n, (n + 50) as u8));
        let plan = plan_reconcile(Some(id(2, 2)), local, node);
        assert_eq!(plan.revert, vec![[2u8; 32], [1u8; 32], [0u8; 32]]);
        assert_eq!(plan.apply_from, 0);
    }
}
