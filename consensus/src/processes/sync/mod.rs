use std::{cmp::min, ops::Deref, sync::Arc};

use itertools::Itertools;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::errors::sync::{SyncManagerError, SyncManagerResult};
use kaspa_database::prelude::StoreResultExt;
use kaspa_math::uint::malachite_base::num::arithmetic::traits::CeilingLogBase2;
use parking_lot::RwLock;

use crate::model::{
    services::reachability::{MTReachabilityService, ReachabilityService},
    stores::{
        ghostdag::GhostdagStoreReader, headers_selected_tip::HeadersSelectedTipStoreReader, pruning::PruningStoreReader,
        reachability::ReachabilityStoreReader, relations::RelationsStoreReader, selected_chain::SelectedChainStoreReader,
        statuses::StatusesStoreReader,
    },
};

use super::traversal_manager::DagTraversalManager;

#[derive(Clone)]
pub struct SyncManager<
    S: RelationsStoreReader,
    T: ReachabilityStoreReader,
    U: GhostdagStoreReader,
    V: SelectedChainStoreReader,
    W: HeadersSelectedTipStoreReader,
    X: PruningStoreReader,
    Y: StatusesStoreReader,
> {
    mergeset_size_limit: u64,
    reachability_service: MTReachabilityService<T>,
    traversal_manager: DagTraversalManager<U, T, S>,
    ghostdag_store: Arc<U>,
    selected_chain_store: Arc<RwLock<V>>,
    _header_selected_tip_store: Arc<RwLock<W>>,
    pruning_point_store: Arc<RwLock<X>>,
    statuses_store: Arc<RwLock<Y>>,
    /// **RFC-0008 v2 (sync): the EXEC blocks a chain block's anchor covered** — the syncer lists them beside the chain block's
    /// mergeset, because no block names them as a parent. `None` where the EXEC payload is not armed.
    exec_members: Option<ExecHook>,
    /// **RFC-0008 v2 (sync): the header-only lane blocks hanging off a block** — what a syncing node requests the bodies of beside
    /// the chain's own (it learns which of them a chain block anchors only from that block's body). `None` where the EXEC payload is
    /// not armed.
    exec_children: Option<ExecHook>,
}

/// A hook the consensus services install: a block's lane blocks (RFC-0008 v2).
pub type ExecHook = Arc<dyn Fn(BlockHash) -> Vec<BlockHash> + Send + Sync>;

impl<
    S: RelationsStoreReader,
    T: ReachabilityStoreReader,
    U: GhostdagStoreReader,
    V: SelectedChainStoreReader,
    W: HeadersSelectedTipStoreReader,
    X: PruningStoreReader,
    Y: StatusesStoreReader,
> SyncManager<S, T, U, V, W, X, Y>
{
    pub fn new(
        mergeset_size_limit: u64,
        reachability_service: MTReachabilityService<T>,
        traversal_manager: DagTraversalManager<U, T, S>,
        ghostdag_store: Arc<U>,
        selected_chain_store: Arc<RwLock<V>>,
        header_selected_tip_store: Arc<RwLock<W>>,
        pruning_point_store: Arc<RwLock<X>>,
        statuses_store: Arc<RwLock<Y>>,
    ) -> Self {
        Self {
            mergeset_size_limit,
            reachability_service,
            traversal_manager,
            ghostdag_store,
            selected_chain_store,
            _header_selected_tip_store: header_selected_tip_store,
            pruning_point_store,
            statuses_store,
            exec_members: None,
            exec_children: None,
        }
    }

    /// Install RFC-0008 v2's two hooks (only on a network that armed the EXEC payload).
    pub fn with_exec_hooks(mut self, members: ExecHook, children: ExecHook) -> Self {
        self.exec_members = Some(members);
        self.exec_children = Some(children);
        self
    }

    /// Returns the hashes of the blocks between low's antipast and high's antipast, or up to `max_blocks`, if provided.
    /// The result excludes low and includes high. If low == high, returns nothing. If max_blocks is some then it MUST be >= MergeSetSizeLimit
    /// because it returns blocks with MergeSet granularity, so if MergeSet > max_blocks, the function will return nothing which is undesired behavior.
    pub fn antipast_hashes_between(&self, low: BlockHash, high: BlockHash, max_blocks: Option<usize>) -> (Vec<BlockHash>, BlockHash) {
        let max_blocks = max_blocks.unwrap_or(usize::MAX);
        assert!(max_blocks >= self.mergeset_size_limit as usize);

        // If low is not in the chain of high - forward_chain_iterator will fail.
        // Therefore, we traverse down low's chain until we reach a block that is in
        // high's chain.
        // We keep original_low to filter out blocks in its past later down the road
        let original_low = low;
        let low = self.find_highest_common_chain_block(low, high);

        let low_bs = self.ghostdag_store.get_blue_score(low).unwrap();
        let high_bs = self.ghostdag_store.get_blue_score(high).unwrap();
        assert!(low_bs <= high_bs);

        let mut highest_reached = low; // The highest chain block we reached before completing/reaching a limit
        let mut blocks = Vec::with_capacity(min(max_blocks, (high_bs - low_bs) as usize));
        for current in self.reachability_service.forward_chain_iterator(low, high, true).skip(1) {
            let gd = self.ghostdag_store.get_data(current).unwrap();
            // RFC-0008 v2: the EXEC blocks this chain block's anchor covered ride with it — before it, since its body cannot be judged
            // without them. A first segment is always taken whole, however many it holds.
            let exec: Vec<BlockHash> = self.exec_members.as_ref().map(|hook| hook(current)).unwrap_or_default();
            if blocks.len() + gd.mergeset_size() + if blocks.is_empty() { 0 } else { exec.len() } > max_blocks {
                break;
            }
            let mut segment: Vec<BlockHash> = gd
                .consensus_ordered_mergeset(self.ghostdag_store.deref())
                .filter(|hash| !self.reachability_service.is_dag_ancestor_of(*hash, original_low))
                .collect();
            segment.extend(exec.into_iter().filter(|hash| !self.reachability_service.is_dag_ancestor_of(*hash, original_low)));
            blocks.extend(self.parents_first(segment));
            highest_reached = current;
        }

        // The process above doesn't return `highest_reached`, so include it explicitly unless it is `low`
        if low != highest_reached {
            blocks.push(highest_reached);
        }

        (blocks, highest_reached)
    }

    /// **One chain block's share of [`Self::antipast_hashes_between`], parents first.**
    ///
    /// The segment is the selected parent followed by the mergeset in consensus order — ascending
    /// `(blue_work, hash)` — which upstream relies on being topological. ADR-0125 breaks that: every
    /// round block hanging from one anchor carries the same blue work, so a lane comes out in HASH
    /// order, a child ahead of its parent. The list feeds a syncer's header batches
    /// (`get_hashes_between`) and a syncing node's own body requests
    /// (`get_missing_block_body_hashes`), and either one handed to the pipeline child-first fails with
    /// `MissingParents` (testnet-12 from DAA 316: no node could finish IBD).
    ///
    /// Segments need no reordering against each other: a block's parents lie in the selected
    /// parent's past, or are the selected parent, or are in the same mergeset. Within a segment the
    /// reorder is stable, and a segment that is already parents-first — every segment of a DAG whose
    /// blue work is strictly monotone — comes back untouched. Only the ORDER of this list changes:
    /// its contents, and the consensus order a mergeset is accepted in, do not.
    fn parents_first(&self, segment: Vec<BlockHash>) -> Vec<BlockHash> {
        kaspa_consensus_core::topological_order::stable_topological_order(
            segment,
            |hash| *hash,
            |hash| self.traversal_manager.direct_parents(*hash),
        )
    }

    pub fn find_highest_common_chain_block(&self, low: BlockHash, high: BlockHash) -> BlockHash {
        self.reachability_service
            .default_backward_chain_iterator(low)
            .find(|candidate| self.reachability_service.is_chain_ancestor_of(*candidate, high))
            .expect("because of the pruning rules such block has to exist")
    }

    /// Returns a logarithmic amount of blocks sampled from the virtual selected chain between `low` and `high`.
    /// Expects both blocks to be on the virtual selected chain, otherwise an error is returned
    pub fn create_virtual_selected_chain_block_locator(
        &self,
        low: Option<BlockHash>,
        high: Option<BlockHash>,
    ) -> SyncManagerResult<Vec<BlockHash>> {
        let low = low.unwrap_or_else(|| self.pruning_point_store.read().pruning_point().unwrap());
        let sc_read = self.selected_chain_store.read();
        let high = high.unwrap_or_else(|| sc_read.get_tip().unwrap().1);
        if low == high {
            return Ok(vec![low]);
        }

        let low_index = match sc_read.get_by_hash(low).optional().unwrap() {
            Some(index) => index,
            None => return Err(SyncManagerError::BlockNotInSelectedParentChain(low)),
        };

        let high_index = match sc_read.get_by_hash(high).optional().unwrap() {
            Some(index) => index,
            None => return Err(SyncManagerError::BlockNotInSelectedParentChain(high)),
        };

        if low_index > high_index {
            return Err(SyncManagerError::LowHashHigherThanHighHash(low, high));
        }

        let mut locator = Vec::with_capacity((high_index - low_index).ceiling_log_base_2() as usize);
        let mut step = 1;
        let mut current_index = high_index;
        while current_index > low_index {
            locator.push(sc_read.get_by_index(current_index).unwrap());
            if current_index < step {
                break;
            }

            current_index -= step;
            step *= 2;
        }

        locator.push(low);
        Ok(locator)
    }

    pub fn get_missing_block_body_hashes(&self, high: BlockHash) -> SyncManagerResult<Vec<BlockHash>> {
        let pp = self.pruning_point_store.read().pruning_point().unwrap();
        if !self.reachability_service.is_chain_ancestor_of(pp, high) {
            return Err(SyncManagerError::PruningPointNotInChain(pp, high));
        }

        let mut highest_with_body = None;
        let mut forward_iterator = self.reachability_service.forward_chain_iterator(pp, high, true).tuple_windows();
        let mut backward_iterator = self.reachability_service.backward_chain_iterator(high, pp, true);
        loop {
            // We loop from both directions in parallel in order to use the shorter path
            let Some((parent, current)) = forward_iterator.next() else {
                break;
            };
            let status = self.statuses_store.read().get(current).unwrap();
            if status.is_header_only() {
                // Going up, the first parent which has a header-only child is our target
                highest_with_body = Some(parent);
                break;
            }

            let Some(backward_current) = backward_iterator.next() else {
                break;
            };
            let status = self.statuses_store.read().get(backward_current).unwrap();
            if status.has_block_body() {
                // Since this iterator is going down, current must be the highest with body
                highest_with_body = Some(backward_current);
                break;
            }
        }

        if highest_with_body.is_none_or(|h| h == high) {
            return Ok(vec![]);
        };

        let (hashes_between, _) = self.antipast_hashes_between(highest_with_body.unwrap(), high, None);
        let statuses = self.statuses_store.read();
        let Some(children) = self.exec_children.as_ref() else {
            let mut hashes_between = hashes_between;
            hashes_between.retain(|&h| statuses.get(h).unwrap().is_header_only());
            return Ok(hashes_between);
        };
        // RFC-0008 v2: each header-only lane block hanging off a listed block is requested right after it — before the chain block that
        // anchors it, which follows in the list.
        let mut out = Vec::with_capacity(hashes_between.len());
        let mut seen: std::collections::HashSet<BlockHash> = hashes_between.iter().copied().collect();
        for hash in hashes_between {
            if statuses.get(hash).unwrap().is_header_only() {
                out.push(hash);
            }
            let lane: Vec<BlockHash> = children(hash)
                .into_iter()
                .filter(|lane_block| seen.insert(*lane_block))
                .filter(|lane_block| statuses.get(*lane_block).optional().unwrap().is_some_and(|status| status.is_header_only()))
                .collect();
            out.extend(self.parents_first(lane));
        }
        Ok(out)
    }

    pub fn create_block_locator_from_pruning_point(
        &self,
        high: BlockHash,
        low: BlockHash,
        limit: Option<usize>,
    ) -> SyncManagerResult<Vec<BlockHash>> {
        if !self.reachability_service.is_chain_ancestor_of(low, high) {
            return Err(SyncManagerError::LocatorLowHashNotInHighHashChain(low, high));
        }

        let low_bs = self.ghostdag_store.get_blue_score(low).unwrap();
        let mut current = high;
        let mut step = 1;
        let mut locator = Vec::new();
        loop {
            locator.push(current);
            if limit == Some(locator.len()) {
                break;
            }

            let current_gd = self.ghostdag_store.get_compact_data(current).unwrap();

            // Nothing more to add once the low node has been added.
            if current_gd.blue_score <= low_bs {
                break;
            }

            // Calculate blue score of previous block to include ensuring the
            // final block is `low`.
            let next_bs = if current_gd.blue_score < step || current_gd.blue_score - step < low_bs {
                low_bs
            } else {
                current_gd.blue_score - step
            };

            // Walk down current's selected parent chain to the appropriate ancestor
            current = self.traversal_manager.lowest_chain_block_above_or_equal_to_blue_score(current, next_bs);

            // Double the distance between included hashes
            step *= 2;
        }

        Ok(locator)
    }
}
