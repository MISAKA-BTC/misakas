//! The EVM segment pruner — retention that does not wait for L1 pruning.
//!
//! `recover_pruning_workflows_if_needed` refuses to prune while consensus is
//! transitional or virtual has not caught up. That is correct, and it is also
//! the entire IBD window, during which the node writes the most. Every EVM store
//! hung off that gate, so the node's highest-write period ran with reclamation
//! switched off — the shape of the 144 GB incident.
//!
//! RPC-only EVM data does not need the gate: deleting a receipt or a log posting
//! cannot change how a block validates. This pruner reclaims those continuously,
//! in small batches, resumably, while leaving anything execution or reorg
//! recovery reads to the L1 pruning point.
//!
//! Three properties the implementation is built around:
//!
//! * **Resumable.** Every pass commits its deletions and its cursor advance in
//!   ONE batch, so a crash mid-pass leaves the cursor exactly where the data
//!   actually is. A cursor written separately could claim progress that the
//!   deletions never made — the failure mode where a store looks pruned and is
//!   not.
//! * **Ordered.** Log postings are derived from receipts, so postings for a
//!   block must go before that block's receipts. The pruner enforces this as a
//!   cursor invariant rather than as an ordering convention someone can break.
//! * **Bounded.** A pass deletes at most `batch_rows` rows, so retention is a
//!   background trickle. A pruner that stalls block processing to reclaim space
//!   has traded a disk problem for a liveness problem.

use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::evm::{EvmNodeRole, EvmPruneCursor, EvmPruneSegment, EvmRetentionPolicy, SegmentRetention};

/// What one pass over one segment should do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SegmentPlan {
    pub segment: EvmPruneSegment,
    /// Reclaim EVM numbers in `[from, to)`.
    pub from: u64,
    pub to: u64,
}

impl SegmentPlan {
    pub fn is_empty(&self) -> bool {
        self.to <= self.from
    }

    pub fn len(&self) -> u64 {
        self.to.saturating_sub(self.from)
    }
}

/// Everything the planner needs to know about the node right now.
#[derive(Clone, Copy, Debug)]
pub struct PruneContext {
    /// The canonical EVM head.
    pub head_evm_number: u64,
    /// Observed EVM blocks per second, for converting a duration retention into
    /// a block distance.
    pub blocks_per_second: f64,
    /// Whether consensus is transitional / catching up. Only gates the
    /// state-history segments.
    pub consensus_transitional: bool,
    /// The EVM number of the L1 pruning point, if it has one. State-history
    /// segments are never reclaimed above it, whatever the retention says.
    pub l1_pruning_evm_number: Option<u64>,
    /// Cap on the numbers one pass may cover.
    pub batch_rows: usize,
}

/// Decide one segment's work. Pure, so the interesting cases are testable without
/// a database.
///
/// Returns `None` when there is nothing to do, which is the common case and must
/// stay cheap.
pub fn plan_segment(
    segment: EvmPruneSegment,
    policy: &EvmRetentionPolicy,
    cursor: &EvmPruneCursor,
    ctx: &PruneContext,
    // The log-posting cursor, which bounds how far receipts may be reclaimed.
    // Passed in rather than read here so the planner stays pure.
    log_postings_pruned_through: u64,
) -> Option<SegmentPlan> {
    // Code is reachability-based, never a block range.
    if segment == EvmPruneSegment::CodeGc {
        return None;
    }
    let retention = policy.retention(segment);
    if retention.is_archive() {
        return None;
    }
    // State history follows the L1 pruning point and the transitional gate. This
    // is the one rule the pruner must not relax under disk pressure: unlike an
    // index, a deleted diff is not rebuildable, and reorg recovery reads it.
    if !segment.prunable_during_ibd() {
        if ctx.consensus_transitional {
            return None;
        }
        // No pruning point yet means no bound to prune against — not "prune
        // freely".
        ctx.l1_pruning_evm_number?;
    }

    let mut keep_from = retention.keep_from(ctx.head_evm_number, ctx.blocks_per_second)?;

    // Never reclaim above the L1 pruning point for load-bearing segments, even
    // when the retention window would allow it.
    if !segment.prunable_during_ibd()
        && let Some(l1) = ctx.l1_pruning_evm_number
    {
        keep_from = keep_from.min(l1);
    }

    // Ordering invariant: a block's log postings are re-derived from its receipts,
    // so the receipts must outlive the postings. Capping the receipt plan by the
    // posting cursor makes that a property of the planner rather than a rule
    // someone has to remember when adding a segment.
    if segment == EvmPruneSegment::Receipts {
        keep_from = keep_from.min(log_postings_pruned_through);
    }

    let from = cursor.pruned_through;
    let to = keep_from.min(from.saturating_add(ctx.batch_rows as u64));
    let plan = SegmentPlan { segment, from, to };
    (!plan.is_empty()).then_some(plan)
}

/// Plan a whole pass, cheapest-and-largest first.
///
/// RPC segments lead: they are the ones that may run during IBD, they are the
/// ones that grow fastest, and they are rebuildable, so making progress there is
/// both safer and more valuable than making progress on state history.
pub fn plan_pass(
    policy: &EvmRetentionPolicy,
    ctx: &PruneContext,
    cursor_of: impl Fn(EvmPruneSegment) -> EvmPruneCursor,
) -> Vec<SegmentPlan> {
    let log_cursor = cursor_of(EvmPruneSegment::LogPostings);
    EvmPruneSegment::ALL
        .iter()
        .filter(|s| s.prunable_during_ibd() || !ctx.consensus_transitional)
        .filter_map(|&segment| plan_segment(segment, policy, &cursor_of(segment), ctx, log_cursor.pruned_through))
        .collect()
}

/// Whether this node writes a segment at all.
///
/// Not writing is strictly better than writing and later deleting: it saves the
/// write, the WAL record, the compaction and the tombstone. A compact validator
/// that never serves `eth_getLogs` should not pay to build the posting index and
/// then pay again to remove it.
pub fn writes_segment(policy: &EvmRetentionPolicy, segment: EvmPruneSegment) -> bool {
    policy.writes(segment)
}

/// The RPC-facing answer to "can you still serve this?".
///
/// Returning an empty result for pruned history is the failure this exists to
/// prevent: the caller cannot tell "no logs in that range" from "that range is
/// gone", and will happily conclude the chain is empty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SegmentAvailability {
    pub segment: EvmPruneSegment,
    pub available_from: u64,
    pub retention: SegmentRetention,
}

impl SegmentAvailability {
    pub fn covers(&self, evm_number: u64) -> bool {
        !self.retention.is_off() && evm_number >= self.available_from
    }
}

/// A default retention policy for a role, with the trace segment forced off when
/// the node exposes no debug RPC.
///
/// Writing a per-block replay plan for an endpoint that is not reachable is pure
/// cost, and it is one of the larger per-block values in the EVM lane.
pub fn policy_for(role: EvmNodeRole, debug_rpc_enabled: bool) -> EvmRetentionPolicy {
    let mut policy = EvmRetentionPolicy::for_role(role);
    if !debug_rpc_enabled {
        policy.trace_replay = SegmentRetention::Off;
    }
    policy
}

/// One block's worth of reclaimable identity, resolved from the canonical number
/// map. `None` for a number whose canonical block is already gone.
#[derive(Clone, Copy, Debug)]
pub struct PrunableBlock {
    pub evm_number: u64,
    pub block: BlockHash,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(head: u64) -> PruneContext {
        PruneContext {
            head_evm_number: head,
            blocks_per_second: 10.0,
            consensus_transitional: false,
            l1_pruning_evm_number: Some(head.saturating_sub(500_000)),
            batch_rows: 1000,
        }
    }

    fn cursor(pruned_through: u64) -> EvmPruneCursor {
        EvmPruneCursor { pruned_through, available_from: pruned_through, ..EvmPruneCursor::new() }
    }

    #[test]
    fn rpc_segments_are_planned_while_consensus_is_transitional() {
        // The whole point: during IBD the node writes the most and, before this,
        // reclaimed nothing.
        let policy = EvmRetentionPolicy::for_role(EvmNodeRole::RpcRecent);
        let mut c = ctx(10_000_000);
        c.consensus_transitional = true;

        let plans = plan_pass(&policy, &c, |_| cursor(0));
        assert!(!plans.is_empty(), "RPC retention must run during IBD");
        for p in &plans {
            assert!(p.segment.prunable_during_ibd(), "{} must not be planned while transitional", p.segment.as_str());
        }
    }

    #[test]
    fn state_history_never_moves_while_transitional_or_above_the_l1_pruning_point() {
        let policy = EvmRetentionPolicy::for_role(EvmNodeRole::RpcRecent);

        // Transitional: no state plan at all.
        let mut c = ctx(10_000_000);
        c.consensus_transitional = true;
        assert!(plan_segment(EvmPruneSegment::StateDiffs, &policy, &cursor(0), &c, u64::MAX).is_none());

        // Settled, but the retention window would reach ABOVE the L1 pruning
        // point — the plan must stop at the pruning point, not at the window.
        let c = PruneContext { consensus_transitional: false, l1_pruning_evm_number: Some(1_000), ..ctx(10_000_000) };
        let plan = plan_segment(EvmPruneSegment::StateDiffs, &policy, &cursor(0), &c, u64::MAX).unwrap();
        assert!(plan.to <= 1_000, "state history must never be reclaimed past the L1 pruning point: {plan:?}");

        // No pruning point yet: nothing to do rather than a guess.
        let c = PruneContext { l1_pruning_evm_number: None, ..ctx(10_000_000) };
        assert!(plan_segment(EvmPruneSegment::StateDiffs, &policy, &cursor(0), &c, u64::MAX).is_none());
    }

    #[test]
    fn receipts_can_never_outrun_the_log_postings_that_are_derived_from_them() {
        // Postings are re-derived from receipts at prune time, so reclaiming a
        // block's receipts first would strand its postings forever.
        let policy = EvmRetentionPolicy::for_role(EvmNodeRole::RpcRecent);
        let c = ctx(10_000_000);
        let plan = plan_segment(EvmPruneSegment::Receipts, &policy, &cursor(0), &c, 400).unwrap();
        assert_eq!(plan.to, 400, "receipts must stop where the posting cursor stopped: {plan:?}");

        // With postings fully caught up, receipts are bounded only by the batch.
        let plan = plan_segment(EvmPruneSegment::Receipts, &policy, &cursor(0), &c, u64::MAX).unwrap();
        assert_eq!(plan.len(), c.batch_rows as u64);
    }

    #[test]
    fn a_pass_is_bounded_by_the_batch_size() {
        // Retention must be a background trickle: a pruner that stalls block
        // processing has swapped a disk problem for a liveness problem.
        let policy = EvmRetentionPolicy::for_role(EvmNodeRole::RpcRecent);
        let c = PruneContext { batch_rows: 64, ..ctx(10_000_000) };
        for plan in plan_pass(&policy, &c, |_| cursor(0)) {
            assert!(plan.len() <= 64, "{} planned {} numbers", plan.segment.as_str(), plan.len());
        }
    }

    #[test]
    fn archive_plans_nothing_and_off_reclaims_everything() {
        let archive = EvmRetentionPolicy::for_role(EvmNodeRole::ArchiveIndexer);
        assert!(plan_pass(&archive, &ctx(10_000_000), |_| cursor(0)).is_empty());

        // A compact validator turns the posting index off; the planner must then
        // reclaim right up to the head rather than leaving a window behind.
        let compact = EvmRetentionPolicy::for_role(EvmNodeRole::CompactValidator);
        let c = PruneContext { batch_rows: usize::MAX, ..ctx(1_000) };
        let plan = plan_segment(EvmPruneSegment::LogPostings, &compact, &cursor(0), &c, u64::MAX).unwrap();
        assert_eq!(plan.to, 1_001, "an off segment reclaims through the head");
    }

    #[test]
    fn nothing_is_planned_once_a_cursor_has_caught_up() {
        // The common case, and it must be cheap.
        let policy = EvmRetentionPolicy::for_role(EvmNodeRole::RpcRecent);
        let c = ctx(10_000_000);
        let caught_up = policy.retention(EvmPruneSegment::Receipts).keep_from(c.head_evm_number, c.blocks_per_second).unwrap();
        assert!(plan_segment(EvmPruneSegment::Receipts, &policy, &cursor(caught_up), &c, u64::MAX).is_none());
    }

    #[test]
    fn code_gc_is_never_a_block_range() {
        // 222 entries are shared; only reachability decides. A range plan here
        // would delete bytecode that retained state still references.
        let policy = EvmRetentionPolicy::for_role(EvmNodeRole::RpcRecent);
        assert!(plan_segment(EvmPruneSegment::CodeGc, &policy, &cursor(0), &ctx(10_000_000), u64::MAX).is_none());
    }

    #[test]
    fn disabling_debug_rpc_stops_trace_plans_from_being_written() {
        let with_debug = policy_for(EvmNodeRole::RpcRecent, true);
        assert!(writes_segment(&with_debug, EvmPruneSegment::TraceReplay));
        let without = policy_for(EvmNodeRole::RpcRecent, false);
        assert!(!writes_segment(&without, EvmPruneSegment::TraceReplay), "no debug RPC means the plan is pure cost");
    }

    #[test]
    fn availability_distinguishes_pruned_from_empty() {
        let avail = SegmentAvailability {
            segment: EvmPruneSegment::LogPostings,
            available_from: 500,
            retention: SegmentRetention::Blocks(10),
        };
        assert!(!avail.covers(499), "below the floor the answer is 'pruned', not 'no results'");
        assert!(avail.covers(500));

        let off = SegmentAvailability { segment: EvmPruneSegment::LogPostings, available_from: 0, retention: SegmentRetention::Off };
        assert!(!off.covers(0), "a segment that is off covers nothing, at any height");
    }
}

// ---------------------------------------------------------------------------
// Store-driven execution.
// ---------------------------------------------------------------------------

use crate::model::stores::evm::{
    DbEvmBlockHashMapStore, DbEvmBlockStateRootStore, DbEvmLogIndexStore, DbEvmNumberStore, DbEvmPayloadStore, DbEvmPruneCursorStore,
    DbEvmRawTxOwnersStore, DbEvmRawTxStore, DbEvmReceiptsStore, DbEvmStateCheckpointStore, DbEvmStateCheckpointV2Store,
    DbEvmStateDiffStore, DbEvmTraceReplayStore, DbEvmTxIndexStore, EvmNumberStoreReader, EvmReceiptsStoreReader,
};
use kaspa_consensus_core::evm::{LogPostingKind, LogPostingLoc};
use kaspa_database::prelude::{DB, StoreError};
use rocksdb::WriteBatch;
use std::sync::Arc;

/// Every store a prune pass touches.
pub struct EvmPruneStores {
    pub db: Arc<DB>,
    pub cursors: Arc<DbEvmPruneCursorStore>,
    pub numbers: Arc<DbEvmNumberStore>,
    pub receipts: Arc<DbEvmReceiptsStore>,
    pub log_index: Arc<DbEvmLogIndexStore>,
    pub tx_index: Arc<DbEvmTxIndexStore>,
    pub raw_tx: Arc<DbEvmRawTxStore>,
    pub raw_tx_owners: Arc<DbEvmRawTxOwnersStore>,
    pub payloads: Arc<DbEvmPayloadStore>,
    pub block_hash_map: Arc<DbEvmBlockHashMapStore>,
    pub traces: Arc<DbEvmTraceReplayStore>,
    pub diffs: Arc<DbEvmStateDiffStore>,
    pub anchors_v1: Arc<DbEvmStateCheckpointStore>,
    pub anchors_v2: Arc<DbEvmStateCheckpointV2Store>,
    pub state_roots: Arc<DbEvmBlockStateRootStore>,
    /// EIP-2718 transaction hashing. Injected rather than called directly because
    /// it lives behind the `evm` feature, and the pruner's logic should not.
    pub tx_hash: fn(&[u8]) -> kaspa_hashes::EvmH256,
}

/// The result of one segment's pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PruneOutcome {
    pub rows_deleted: u64,
    /// Numbers actually covered; the cursor advances to this.
    pub pruned_through: u64,
}

/// Re-derive every log posting a block wrote, from its receipts.
///
/// The alternative — journalling the posting keys when they are written — was
/// rejected: it would add a second index-sized write per block to bound the
/// first one. Receipts already hold everything the derivation needs, and the
/// pruner's cursor ordering guarantees they outlive the postings.
pub fn derive_log_postings(
    receipts: &kaspa_consensus_core::evm::EvmBlockReceipts,
    evm_number: u64,
    l1_hash: BlockHash,
) -> Vec<(LogPostingKind, Vec<u8>, LogPostingLoc)> {
    let mut out = Vec::new();
    for (rcpt_idx, receipt) in receipts.receipts.iter().enumerate() {
        for (in_rcpt_idx, log) in receipt.logs.iter().enumerate() {
            let loc = LogPostingLoc { evm_number, l1_hash, tx_index: rcpt_idx as u32, in_receipt_log_index: in_rcpt_idx as u32 };
            out.push((LogPostingKind::Address, log.address.as_bytes().to_vec(), loc));
            for (ti, topic) in log.topics.iter().take(4).enumerate() {
                if let Some(kind) = LogPostingKind::topic(ti) {
                    out.push((kind, topic.as_bytes().to_vec(), loc));
                }
            }
        }
    }
    out
}

impl EvmPruneStores {
    /// Execute one plan. Deletions and the cursor advance go into ONE batch, so a
    /// crash cannot leave a cursor claiming progress the deletions never made.
    pub fn execute(&self, plan: SegmentPlan, now_ms: u64) -> Result<PruneOutcome, StoreError> {
        let mut batch = WriteBatch::default();
        let mut rows = 0u64;
        let mut reached = plan.from;

        for evm_number in plan.from..plan.to {
            reached = evm_number + 1;
            // The canonical number map is the enumeration. A number with no row is
            // simply not canonical (or already gone) — skip, do not stall.
            let Some(block) = self.numbers.get(evm_number)? else { continue };
            rows += self.prune_block(&mut batch, plan.segment, evm_number, block)?;
        }

        let mut cursor = self.cursors.get(plan.segment)?;
        cursor.advance(reached, rows, now_ms);
        self.cursors.set_batch(&mut batch, plan.segment, cursor)?;
        // Log postings additionally publish an RPC-facing floor, so a range query
        // below it can answer "pruned" instead of "no results".
        if plan.segment == EvmPruneSegment::LogPostings {
            self.log_index.set_history_available_from_batch(&mut batch, reached)?;
        }
        self.db.write(batch).map_err(StoreError::DbError)?;
        Ok(PruneOutcome { rows_deleted: rows, pruned_through: reached })
    }

    fn prune_block(
        &self,
        batch: &mut WriteBatch,
        segment: EvmPruneSegment,
        evm_number: u64,
        block: BlockHash,
    ) -> Result<u64, StoreError> {
        match segment {
            EvmPruneSegment::LogPostings => {
                let Ok(receipts) = self.receipts.get(block) else { return Ok(0) };
                let postings = derive_log_postings(&receipts, evm_number, block);
                for (kind, selector, loc) in &postings {
                    self.log_index.delete_posting_batch(batch, *kind, selector, loc)?;
                }
                Ok(postings.len() as u64)
            }
            EvmPruneSegment::Receipts => {
                if !self.receipts.has(block)? {
                    return Ok(0);
                }
                self.receipts.delete_batch(batch, block)?;
                Ok(1)
            }
            EvmPruneSegment::TransactionLookup => {
                // The payload is the authoritative list of the transactions this
                // block carried. `tx_hashes` reads the slim references directly (or
                // re-hashes a legacy row) WITHOUT reconstructing raws from 217 — so
                // it is robust to pruning order and does no wasted round trip.
                let Ok(Some(hashes)) = self.payloads.tx_hashes(block, self.tx_hash) else { return Ok(0) };
                let mut rows = 0;
                for txh in hashes {
                    if self.tx_index.remove_block_locations_batch(batch, txh, block)? {
                        rows += 1;
                    }
                }
                Ok(rows)
            }
            EvmPruneSegment::RawTransactions => {
                let Ok(Some(hashes)) = self.payloads.tx_hashes(block, self.tx_hash) else { return Ok(0) };
                let mut rows = 0;
                for txh in hashes {
                    // Only the LAST owner's departure reclaims the bytes. A tx can
                    // sit in several payloads, and deleting on the first one would
                    // break `eth_getTransactionByHash` for the rest.
                    if self.raw_tx_owners.decrement_batch(batch, txh)? == 0 {
                        self.raw_tx.delete_batch(batch, txh)?;
                        rows += 1;
                    }
                }
                Ok(rows)
            }
            EvmPruneSegment::BlockHashMap => {
                // The RPC id is the first 32 bytes of the L1 hash — the same
                // truncation the writer used.
                let mut id = [0u8; 32];
                id.copy_from_slice(&block.as_bytes()[..32]);
                self.block_hash_map.delete_batch(batch, kaspa_hashes::EvmH256::from_bytes(id))?;
                Ok(1)
            }
            EvmPruneSegment::NumberIndex => {
                self.numbers.delete_batch(batch, evm_number)?;
                Ok(1)
            }
            EvmPruneSegment::TraceReplay => {
                self.traces.delete_batch(batch, block)?;
                Ok(1)
            }
            EvmPruneSegment::StateDiffs => {
                self.diffs.delete_batch(batch, block)?;
                Ok(1)
            }
            EvmPruneSegment::StateAnchors => {
                self.anchors_v1.delete_batch(batch, block)?;
                self.anchors_v2.delete_batch(batch, block)?;
                Ok(1)
            }
            EvmPruneSegment::BlockStateRoots => {
                self.state_roots.delete_batch(batch, block)?;
                Ok(1)
            }
            // Reachability, not range. Handled by the mark-and-sweep pass.
            EvmPruneSegment::CodeGc => Ok(0),
        }
    }

    /// Current availability floors, for the RPC `history pruned` answer.
    pub fn availability(&self, policy: &EvmRetentionPolicy) -> Vec<SegmentAvailability> {
        EvmPruneSegment::ALL
            .iter()
            .map(|&segment| {
                let available_from = self.cursors.get(segment).map(|c| c.available_from).unwrap_or(0);
                SegmentAvailability { segment, available_from, retention: policy.retention(segment) }
            })
            .collect()
    }
}

/// The EIP-2718 transaction hash function, or a stand-in on a non-`evm` build.
///
/// Injected into `EvmPruneStores` so the pruner's logic is not feature-gated. On
/// a non-EVM build the lane is inert and no payload rows exist, so the stand-in
/// is never reached; it panics rather than returning a plausible-looking hash,
/// because a wrong hash here would delete the wrong raw transaction.
pub fn evm_tx_hash_fn() -> fn(&[u8]) -> kaspa_hashes::EvmH256 {
    #[cfg(feature = "evm")]
    {
        kaspa_evm::tx::tx_hash
    }
    #[cfg(not(feature = "evm"))]
    {
        fn unreachable_tx_hash(_: &[u8]) -> kaspa_hashes::EvmH256 {
            unreachable!("the EVM lane is inert without --features evm, so no payload transaction can be pruned")
        }
        unreachable_tx_hash
    }
}
