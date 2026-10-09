//! **RFC-0012 — the native settlement snapshot: `latest` / `safe` / `finalized` from PALW evidence alone.**
//!
//! One function, [`VirtualStateProcessor::native_evm_settlement_snapshot`], derives the snapshot a virtual
//! change commits beside the legacy three-hash EVM-head row. It reads nothing from the DNS overlay.
//!
//! **Cost.** The selected chain between the sink and the pruning point is walked on every virtual change
//! (testnet-12's pruning point stays near genesis for a long time). Everything the walk needs from one block
//! is a pure function of immutable rows keyed by that block's hash, so it is read once and kept in
//! [`NativeRowCache`]; a virtual change then costs one cache lookup per chain block and one O(N + F) sweep
//! ([`certify_native_prefix_v1`]) instead of a database read and a delta decode per block and an
//! O(chain x claims) lifecycle scan. The cache is memory only and is rebuilt from the retained deltas after a
//! restart: nothing in it is authoritative, and a block whose rows cannot be read again stops the snapshot at
//! `MissingHistory` rather than being skipped.
//!
//! **Evidence.** REAL work comes from the `Final` transitions in each chain block's PALW delta, free-prompt
//! work from the slice spends in them, and a conviction after `Final` retracts what it had counted
//! ([`native_delta_evidence_v1`]). The sink's claims are consulted only for the lifecycle question (is every
//! claim at or before the effect resolved) and for the operator behind a bond. A claim leaves PALW state at
//! `terminal + claim_retirement`; its work stays counted.
//!
//! **Readiness (RFC-0012 D1).** [`VirtualStateProcessor::native_evaluate`] returns the snapshot AND the material it was weighed from
//! (or the reason nothing was weighed); [`VirtualStateProcessor::native_safe_readiness`] turns that into the structured explanation
//! `getPalwSettlement` serves as `nativeReadiness` (`palw_native_readiness_v1`). The snapshot path is `native_evaluate(..).snapshot`
//! and pays nothing for the explanation; the explanation is advisory, never persisted, and memoized per sink.

use super::VirtualStateProcessor;
use crate::model::{
    services::reachability::ReachabilityService,
    stores::{evm::EvmHeaderStoreReader, ghostdag::GhostdagStoreReader, headers::HeaderStoreReader, pruning::PruningStoreReader},
};
use kaspa_consensus_core::{
    BlockHash, Hash64,
    config::params::ForkActivation,
    palw_fork_choice_commitment_v1::{PalwForkChoiceDeltaV1, PalwForkChoiceLeafV1, palw_committed_root_of_parts_v1},
    palw_native_readiness_v1::{
        ClaimStageV1, FinalizedReadinessV1, FinalizedWaitV1, HistoryGapV1, NATIVE_READINESS_LISTED_V1, NativeReadinessInputV1,
        NativeSafeReadinessV1, OpenClaimV1, OpenSessionV1, SafeWaitV1, native_maturity_report_v1, native_safe_readiness_v1,
        native_stopped_readiness_v1,
    },
    palw_native_settlement_v1::{
        MatureUsefulWorkV1, NativeDeltaEvidenceV1, NativeEffectV1, NativeFactRulesV1, NativePrefixV1, NativeSettlementSnapshotV1,
        SettlementStopV1, SkippedEvidenceV1, certify_native_prefix_v1, native_delta_evidence_v1, native_facts_and_skips_v1,
        native_open_from_v1,
    },
    palw_state_v2::{PalwChainStateV2, PalwClaimPhaseV2},
};
use kaspa_core::error;
use kaspa_database::prelude::StoreError;
use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

/// What one executed selected-chain block contributes to the snapshot, independent of the sink.
pub(super) struct NativeChainRow {
    hash: BlockHash,
    parent: BlockHash,
    daa: u64,
    blue: u64,
    /// The block's header commitment to its PARENT's PALW state root.
    header_palw_root: Hash64,
    /// The root recorded with the block's own delta (the PALW state AT the block); `None` where no delta is read
    /// (the pruning point, whose contributions are never guessed).
    delta_root: Option<Hash64>,
    evidence: NativeDeltaEvidenceV1,
    /// RFC-0009 L2: the part of the delta the fork-choice leaf reads, so the roots chain can rebuild each post-state's leaf backwards
    /// from the sink's and check a header past `palw_fork_choice_commitment_v1` (it commits the leaf with the root). Kept only where that
    /// fence is configured at all (`None` on every shipped network, and with the delta), and boxed so a dormant row stays its size.
    fork_choice: Option<Box<PalwForkChoiceDeltaV1>>,
}

/// **Step 4 of [`VirtualStateProcessor::native_evaluate`], pure: the roots chain.** `rows` is the selected chain newest first (the
/// sink's row first, each next row the previous row's selected parent); each row's delta root is the PALW state at that block, and the
/// row before it (its chain child) committed that state in its header. Past `palw_fork_choice_commitment_v1` (read at the committed
/// state's own point) the header commits `H(leaf ‖ root)`, so each row's leaf is rebuilt backwards from the sink's state, one delta at a
/// time ([`PalwForkChoiceLeafV1::parent_by`]); dormant, the committed root is the delta root itself and this is the comparison it always
/// was. `Err` names the first row whose root breaks the chain; a leaf that cannot be rebuilt is compared in the flat form and so breaks
/// the chain past the fence rather than being assumed.
fn native_roots_chain_v1(
    rows: &[Arc<NativeChainRow>],
    sink_state: &PalwChainStateV2,
    fence: Option<ForkActivation>,
) -> Result<(), BlockHash> {
    let mut leaf = (if fence.is_some() { PalwForkChoiceLeafV1::of(sink_state) } else { None })
        .filter(|leaf| rows.first().is_some_and(|row| row.hash == leaf.block));
    for (i, row) in rows.iter().enumerate() {
        if i > 0 {
            leaf = match (leaf, rows[i - 1].fork_choice.as_deref()) {
                (Some(child), Some(delta)) => child.parent_by(delta).ok(),
                _ => None,
            };
        }
        let Some(root) = row.delta_root else {
            continue;
        };
        let holds = if i == 0 {
            root == sink_state.state_root()
        } else {
            let committed = match leaf.as_ref() {
                Some(leaf) => palw_committed_root_of_parts_v1(leaf, &root, fence),
                None => root,
            };
            committed == rows[i - 1].header_palw_root
        };
        if !holds {
            return Err(row.hash);
        }
    }
    Ok(())
}

/// Memory-only, rebuildable index of [`NativeChainRow`]s by block hash. Only blocks that are executed, with
/// a root-verified EVM result and a readable delta, are ever stored.
#[derive(Default)]
pub(super) struct NativeRowCache {
    rows: HashMap<BlockHash, Arc<NativeChainRow>>,
}

impl NativeRowCache {
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.rows.len()
    }
    #[cfg(test)]
    pub(super) fn clear(&mut self) {
        self.rows.clear();
    }
}

/// Why the chain walk could not produce rows: the snapshot then carries `MissingHistory` (never a guess). The cause and the block
/// are kept for [`VirtualStateProcessor::native_safe_readiness`]; the snapshot itself only ever says `MissingHistory`.
struct WalkFault(HistoryGapV1, Option<BlockHash>);

/// What the evaluation of one sink produced beyond the snapshot, for the readiness explanation. Moved out of the evaluation, not
/// recomputed: the snapshot path pays nothing for it.
pub(super) enum NativeDetail {
    /// Weighing never happened.
    Stopped(SafeWaitV1),
    Weighed(Box<NativeWeighed>),
}

pub(super) struct NativeWeighed {
    sink_daa: u64,
    sink_blue: u64,
    state: PalwChainStateV2,
    /// Executed effects, oldest first.
    chain: Vec<(Hash64, NativeEffectV1)>,
    prefix: NativePrefixV1,
    facts: Vec<MatureUsefulWorkV1>,
    frontier_blue: u64,
    frontier_on_branch: bool,
    skipped: SkippedEvidenceV1,
}

pub(super) struct NativeEvaluation {
    pub(super) snapshot: NativeSettlementSnapshotV1,
    /// The pruning point the evaluation used.
    pub(super) pruning: BlockHash,
    pub(super) detail: NativeDetail,
}

impl VirtualStateProcessor {
    /// Read (or load and remember) the row of `cursor` if it is executed with a root-verified EVM result.
    /// `Ok(None)`: not executed (yet). `Err`: an unreadable row, or a result that disagrees with the header.
    fn native_row(&self, cursor: BlockHash, pruning: BlockHash) -> Result<Result<Arc<NativeChainRow>, (u64, BlockHash)>, WalkFault> {
        if let Some(row) = self.native_rows.lock().rows.get(&cursor) {
            return Ok(Ok(row.clone()));
        }
        let Ok(header) = self.headers_store.get_header(cursor) else {
            return Err(WalkFault(HistoryGapV1::HeaderUnreadable, Some(cursor)));
        };
        match self.evm_header_store.get(cursor) {
            Ok(execution) if execution.commitment_root() == header.evm_commitment_root => {}
            Ok(_) => return Err(WalkFault(HistoryGapV1::ExecutionDisagreesWithHeader, Some(cursor))),
            Err(StoreError::KeyNotFound(_)) => {
                let parent = self
                    .ghostdag_store
                    .get_selected_parent(cursor)
                    .map_err(|_| WalkFault(HistoryGapV1::ParentUnreadable, Some(cursor)))?;
                return Ok(Err((header.daa_score, parent)));
            }
            Err(_) => return Err(WalkFault(HistoryGapV1::ExecutionRowUnreadable, Some(cursor))),
        }
        let (delta_root, evidence, fork_choice) = if cursor == pruning || header.daa_score < self.evm_activation_daa_score {
            (None, NativeDeltaEvidenceV1::default(), None)
        } else {
            // A missing or undecodable delta is a gap in the verified history, not an empty block.
            let Ok((root, delta)) = self.palw_state_v2_store.read().delta_of(cursor) else {
                return Err(WalkFault(HistoryGapV1::DeltaNotRetained, Some(cursor)));
            };
            let fork_choice = self.palw_fork_choice_commitment_v1.is_some().then(|| Box::new(PalwForkChoiceDeltaV1::of(&delta)));
            (Some(root), native_delta_evidence_v1(&delta), fork_choice)
        };
        let parent =
            self.ghostdag_store.get_selected_parent(cursor).map_err(|_| WalkFault(HistoryGapV1::ParentUnreadable, Some(cursor)))?;
        let row = Arc::new(NativeChainRow {
            hash: cursor,
            parent,
            daa: header.daa_score,
            blue: header.blue_score,
            header_palw_root: header.palw_state_root,
            delta_root,
            evidence,
            fork_choice,
        });
        self.native_rows.lock().rows.insert(cursor, row.clone());
        Ok(Ok(row))
    }

    /// RFC-0012: derive one crash-safe snapshot from the same candidate state as fork choice.
    ///
    /// Every early return carries the `latest` found so far: the newest executed, root-verified result on the
    /// selected chain is a fact about execution, not about evidence, and it must not disappear because a
    /// certificate could not be built. `safe` and `finalized` are `None` whenever `stop` is set.
    pub(super) fn native_evm_settlement_snapshot(&self, sink: BlockHash) -> NativeSettlementSnapshotV1 {
        self.native_evaluate(sink).snapshot
    }

    /// The snapshot AND the material it was weighed from (or the reason nothing was weighed). One function, so the explanation
    /// served by `getPalwSettlement` can never be about different evidence than the certificate.
    pub(super) fn native_evaluate(&self, sink: BlockHash) -> NativeEvaluation {
        let retirement = self.palw_dns_retirement.expect("called only after the retirement fence");
        let sink_daa = self.headers_store.get_daa_score(sink).unwrap_or(0);
        let sink_blue = self.headers_store.get_blue_score(sink).unwrap_or(0);
        let pruning = self.pruning_point_store.read().pruning_point().unwrap();
        let mut result = NativeSettlementSnapshotV1 {
            version: 1,
            ruleset_id: self.palw_native_ruleset_id,
            policy_id: retirement.settlement.id(),
            generation: sink,
            retirement_daa: retirement.activation.daa_score(),
            frontier: None,
            latest: None,
            safe: None,
            finalized: None,
            depth: 0,
            unique_work: "0".into(),
            stop: Some(SettlementStopV1::MissingHistory),
        };
        let gap = |gap: HistoryGapV1, block: Option<BlockHash>| SafeWaitV1::MissingHistory { gap, block };
        let stopped = |snapshot: NativeSettlementSnapshotV1, wait: SafeWaitV1| NativeEvaluation {
            snapshot,
            pruning,
            detail: NativeDetail::Stopped(wait),
        };
        let previous = match self.evm_heads_store.read().native_snapshot() {
            Ok(Some(s))
                if s.version == 1 && s.policy_id == retirement.settlement.id() && s.ruleset_id == self.palw_native_ruleset_id =>
            {
                Some(s)
            }
            Ok(None) | Err(StoreError::KeyNotFound(_)) => None,
            // Corrupt or incompatible evidence is never silently forgotten.
            _ => return stopped(result, gap(HistoryGapV1::PersistedSnapshotIncompatible, None)),
        };

        // 1. The selected chain, newest first: executed rows only, an execution gap inside them is a stop.
        let mut rows: Vec<Arc<NativeChainRow>> = Vec::new();
        let genesis = self.genesis.hash;
        let walked = (|| -> Result<(), WalkFault> {
            let mut cursor = sink;
            loop {
                match self.native_row(cursor, pruning)? {
                    Ok(row) => {
                        if rows.is_empty() {
                            result.latest = Some(row.hash);
                        }
                        let (parent, daa) = (row.parent, row.daa);
                        rows.push(row);
                        if cursor == pruning || cursor == genesis || daa < self.evm_activation_daa_score {
                            return Ok(());
                        }
                        cursor = parent;
                    }
                    Err((daa, parent)) => {
                        // Genesis carries no EVM result (the first executed block has no EVM parent): the executed
                        // chain starts above it, and that is not a gap.
                        if cursor == genesis {
                            return Ok(());
                        }
                        // An execution gap inside a supposedly connected result chain is not a safe prefix.
                        if !rows.is_empty() && daa >= self.evm_activation_daa_score {
                            return Err(WalkFault(HistoryGapV1::ExecutionGap, Some(cursor)));
                        }
                        if cursor == pruning || daa < self.evm_activation_daa_score {
                            return Ok(());
                        }
                        cursor = parent;
                    }
                }
                if cursor == kaspa_consensus_core::blockhash::ORIGIN {
                    return Err(WalkFault(HistoryGapV1::ChainOpenEnded, None));
                }
            }
        })();

        // 2. A below-finalized conflict remains an alarm until a validated resync/import clears it.
        // Publishing absent heads once must not let the next block forget the conflict.
        if previous.as_ref().is_some_and(|s| s.stop == Some(SettlementStopV1::FinalizedConflict)) {
            result.stop = Some(SettlementStopV1::FinalizedConflict);
            return stopped(result, SafeWaitV1::FinalizedConflict);
        }
        if let Some(previous_finalized) = previous.and_then(|s| s.finalized) {
            match self.reachability_service.try_is_chain_ancestor_of(previous_finalized, sink) {
                Ok(false) => {
                    error!("[native-settlement] FINALIZED CONFLICT: {sink} abandons {previous_finalized}; resync required");
                    result.stop = Some(SettlementStopV1::FinalizedConflict);
                    return stopped(result, SafeWaitV1::FinalizedConflict);
                }
                Err(_) => return stopped(result, gap(HistoryGapV1::ReachabilityUnreadable, Some(previous_finalized))),
                Ok(true) => {}
            }
        }
        if let Err(WalkFault(cause, block)) = walked {
            return stopped(result, gap(cause, block));
        }
        if rows.is_empty() {
            result.stop = Some(SettlementStopV1::Unexecuted);
            return stopped(result, SafeWaitV1::Unexecuted);
        }

        // 3. The PALW state the fork choice weighs, and its frontier.
        let Some(state) = self.palw_candidate_state_v2(sink) else {
            return stopped(result, gap(HistoryGapV1::StateUnavailable, Some(sink)));
        };
        let Some(params) = self.palw_state_params_v2.as_ref() else {
            return stopped(result, gap(HistoryGapV1::ParamsAbsent, None));
        };
        let (frontier_blue, frontier) = state.safe_frontier();
        result.frontier = (frontier != BlockHash::default()).then_some(frontier);
        let frontier_on_branch =
            frontier != BlockHash::default() && self.reachability_service.try_is_dag_ancestor_of(frontier, sink).unwrap_or(false);

        // 4. The roots chain: each block's delta root is what its child's header committed to as the parent state — in the form
        //    `palw_fork_choice_commitment_v1` says (RFC-0009 L2; `native_roots_chain_v1`).
        if let Err(broken) = native_roots_chain_v1(&rows, &state, self.palw_fork_choice_commitment_v1) {
            return stopped(result, gap(HistoryGapV1::RootChainBroken, Some(broken)));
        }

        // 5. Facts: oldest first is not needed, only the set. The pruning point's own delta is never read.
        let voided: BTreeSet<Hash64> = rows.iter().flat_map(|r| r.evidence.voided.iter().copied()).collect();
        let claims_with_open_da: BTreeSet<Hash64> = state.da_sessions_iter().map(|((claim, _), _)| *claim).collect();
        let rules = NativeFactRulesV1 {
            state: &state,
            params,
            canonical_work_daa: self.palw_canonical_work_daa,
            quantum_maturity_daa: self.palw_exec_quantum_maturity_daa,
            claims_with_open_da: &claims_with_open_da,
        };
        let mut facts: Vec<MatureUsefulWorkV1> = Vec::new();
        let mut skipped = SkippedEvidenceV1::default();
        for row in rows.iter().filter(|r| r.hash != pruning) {
            let (block_facts, block_skipped) = native_facts_and_skips_v1(&rules, &row.evidence, &voided, (row.daa, row.blue));
            facts.extend(block_facts);
            skipped.add(&block_skipped);
        }
        #[cfg(test)]
        {
            let injected = self.native_fact_override.lock();
            for row in rows.iter().filter(|r| r.hash != pruning) {
                facts.extend(injected.get(&row.hash).into_iter().flatten().copied());
            }
        }

        // 6. The lifecycle question, once: an effect at `blue` is closed iff every claim accepted at or before it is
        // resolved (Voided, or Final with its trace retention lapsed) and no DA session names a claim at or before it
        // (a session whose claim is gone is open for every effect). Claims already retired are absent from state and
        // therefore closed.
        let open_from = native_open_from_v1(
            state.claims_iter().map(|(_, c)| (c.accepted_blue_score, c.phase.clone(), c.trace_retention_daa)),
            state.da_sessions_iter().map(|((claim, _), _)| state.claim(claim).map(|c| c.accepted_blue_score)),
            sink_daa,
        );

        // 7. The longest contiguous certified prefix, oldest first.
        let effects: Vec<NativeEffectV1> = rows
            .iter()
            .rev()
            .map(|row| NativeEffectV1 {
                daa: row.daa,
                blue: row.blue,
                frontier_covers: frontier_on_branch && frontier_blue >= row.blue,
                lifecycle_closed: row.blue < open_from,
            })
            .collect();
        let prefix = certify_native_prefix_v1(retirement.settlement, &effects, sink_daa, &facts);
        result.stop = prefix.stop;
        if let Some(index) = prefix.safe {
            result.safe = Some(rows[rows.len() - 1 - index].hash);
            result.depth = prefix.evidence.depth;
            result.unique_work = prefix.evidence.work.to_string();
        }

        // 8. Pruning is a checkpoint safety condition, not a replacement for the PALW certificate.
        if result.safe.is_some_and(|safe| self.reachability_service.try_is_chain_ancestor_of(pruning, safe).unwrap_or(false))
            && self.evm_header_store.has(pruning).unwrap_or(false)
        {
            result.finalized = Some(pruning);
        }

        // 9. The cache holds the chain it just walked; drop what a reorg or a moved pruning point left behind.
        let mut cache = self.native_rows.lock();
        if cache.rows.len() > rows.len().saturating_mul(2).saturating_add(4096) {
            let keep: BTreeSet<BlockHash> = rows.iter().map(|r| r.hash).collect();
            cache.rows.retain(|hash, _| keep.contains(hash));
        }
        drop(cache);
        let chain: Vec<(Hash64, NativeEffectV1)> = rows.iter().rev().map(|r| r.hash).zip(effects).collect();
        NativeEvaluation {
            snapshot: result,
            pruning,
            detail: NativeDetail::Weighed(Box::new(NativeWeighed {
                sink_daa,
                sink_blue,
                state,
                chain,
                prefix,
                facts,
                frontier_blue,
                frontier_on_branch,
                skipped,
            })),
        }
    }

    /// **`finalized` and what it waits for.** It is the validated pruning point under a certified safe prefix, so what it waits
    /// for is the pruning point (and an EVM result for it) — never a maturity number.
    fn native_finalized_readiness(&self, snapshot: &NativeSettlementSnapshotV1, pruning: BlockHash) -> FinalizedReadinessV1 {
        let blue_of = |block: BlockHash| self.headers_store.get_blue_score(block).ok();
        let wait = if snapshot.finalized.is_some() {
            None
        } else if snapshot.stop == Some(SettlementStopV1::FinalizedConflict) {
            Some(FinalizedWaitV1::Conflict)
        } else if let Some(safe) = snapshot.safe {
            if !self.evm_header_store.has(pruning).unwrap_or(false) {
                Some(FinalizedWaitV1::PruningPointNotExecuted)
            } else {
                Some(FinalizedWaitV1::PruningPointNotUnderSafe { safe_blue: blue_of(safe).unwrap_or(0) })
            }
        } else {
            Some(FinalizedWaitV1::NoSafePrefix)
        };
        FinalizedReadinessV1 { finalized: snapshot.finalized, pruning_point: pruning, pruning_blue: blue_of(pruning), wait }
    }

    /// **RFC-0012 D1: why `safe` stands where it does, at `sink`** — the structured reasons served on `getPalwSettlement`
    /// (`nativeReadiness`). Advisory and never persisted: it is derived from the same evaluation as the snapshot, decides nothing,
    /// and is memoized per sink so repeated RPC calls cost one evaluation per virtual change, not one per call.
    pub(crate) fn native_safe_readiness(&self, sink: BlockHash) -> Option<NativeSafeReadinessV1> {
        let retirement = self.palw_dns_retirement?;
        let mut memo = self.native_readiness_memo.lock();
        if let Some((at, readiness)) = memo.as_ref() {
            if *at == sink {
                return Some(readiness.clone());
            }
        }
        let evaluation = self.native_evaluate(sink);
        let finalized = self.native_finalized_readiness(&evaluation.snapshot, evaluation.pruning);
        let params = self.palw_state_params_v2.as_ref();
        let maturity = native_maturity_report_v1(params.map_or(0, |p| p.claim_retirement_daa()), self.palw_exec_quantum_maturity_daa);
        let readiness = match evaluation.detail {
            NativeDetail::Stopped(wait) => {
                let sink_daa = self.headers_store.get_daa_score(sink).unwrap_or(0);
                let sink_blue = self.headers_store.get_blue_score(sink).unwrap_or(0);
                native_stopped_readiness_v1(
                    sink,
                    (sink_daa, sink_blue),
                    retirement.settlement,
                    maturity,
                    wait,
                    finalized,
                    SkippedEvidenceV1::default(),
                )
            }
            NativeDetail::Weighed(w) => {
                let w = *w;
                // Every claim the lifecycle rule counts as open (and nothing else), oldest first; the sweep's next deadline is looked
                // up only for the few that get named.
                let mut open_claims: Vec<OpenClaimV1> = w
                    .state
                    .claims_iter()
                    .filter(|(_, c)| {
                        !(matches!(c.phase, PalwClaimPhaseV2::Voided { .. })
                            || (matches!(c.phase, PalwClaimPhaseV2::Final { .. }) && c.trace_retention_daa <= w.sink_daa))
                    })
                    .map(|(id, c)| OpenClaimV1 {
                        claim: *id,
                        stage: ClaimStageV1::of(&c.phase),
                        accepted_blue: c.accepted_blue_score,
                        retention_daa: c.trace_retention_daa,
                        next_deadline_daa: None,
                    })
                    .collect();
                open_claims.sort_by_key(|c| (c.accepted_blue, c.claim));
                for c in open_claims.iter_mut().take(NATIVE_READINESS_LISTED_V1) {
                    c.next_deadline_daa = w.state.deadline_of(&c.claim);
                }
                let open_sessions: Vec<OpenSessionV1> = w
                    .state
                    .da_sessions_iter()
                    .map(|((claim, _), session)| OpenSessionV1 {
                        claim: *claim,
                        claim_accepted_blue: w.state.claim(claim).map(|c| c.accepted_blue_score),
                        deadline_daa: session.deadline_daa,
                    })
                    .collect();
                native_safe_readiness_v1(&NativeReadinessInputV1 {
                    generation: sink,
                    sink_daa: w.sink_daa,
                    sink_blue: w.sink_blue,
                    policy: retirement.settlement,
                    claim_retirement_daa: maturity.claim_retirement_daa,
                    quantum_maturity_daa: maturity.quantum_maturity_daa,
                    chain: &w.chain,
                    prefix: &w.prefix,
                    facts: &w.facts,
                    frontier_blue: w.frontier_blue,
                    frontier_on_branch: w.frontier_on_branch,
                    open_claims: &open_claims,
                    open_sessions: &open_sessions,
                    skipped: w.skipped,
                    finalized,
                })
            }
        };
        *memo = Some((sink, readiness.clone()));
        Some(readiness)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cache's per-block footprint without evidence, for the record: evidence is a handful of claim records only in blocks
    /// that finalize or spend work.
    #[test]
    fn rfc0012_a_row_without_evidence_is_small() {
        let bytes =
            std::mem::size_of::<NativeChainRow>() + std::mem::size_of::<Arc<NativeChainRow>>() + std::mem::size_of::<BlockHash>();
        eprintln!(
            "[rfc0012-cost] NativeChainRow: {} bytes (+ map entry): about {bytes} bytes per chain block",
            std::mem::size_of::<NativeChainRow>()
        );
        assert!(bytes < 512);
    }

    /// **RFC-0009 L2: the roots chain in both forms.** Three chain blocks over the genesis state; each header commits its selected
    /// parent's post-state in the form the commitment fence says at that state's point. The pure walk accepts the chain dormant, armed
    /// from the start, and armed mid-chain (the form is the state's, so the fence straddles cleanly); it refuses a chain whose headers
    /// committed the flat form where the envelope was due, and names the row whose committed root a forged header breaks.
    #[test]
    fn rfc9_l2_the_roots_chain_reads_each_header_in_its_committed_form() {
        use kaspa_consensus_core::palw_fork_choice_commitment_v1::palw_committed_state_root_v1;
        use kaspa_consensus_core::palw_state_v2::{
            PalwBlockContextV2, PalwBondKeyV2, PalwConsensusObjectV2 as Obj, PalwStateParamsV2, apply_palw_transition_v2,
        };
        use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
        let h = Hash64::from_u64_word;
        let params = PalwStateParamsV2::new(100, 1, 1, 1, 500, 1_000, h(1), 4, 1_000, 100, 1_000, 0).unwrap();
        // One key, one bond: each bond its own key.
        let bond = |n: u64| Obj::BondRegistered {
            bond: PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(0xB0 + n), 0)),
            pubkey: [n.to_le_bytes().to_vec(), vec![7; 2584]].concat(),
            operator_pubkey: n.to_le_bytes().to_vec(),
            collateral: 1 << 40,
            payout_payload: h(0x9A),
            capable_classes: Default::default(),
            signature: Vec::new(),
        };
        let genesis = PalwChainStateV2::genesis();
        let mut states = vec![genesis.clone()];
        let mut deltas = Vec::new();
        for n in 1..=3u64 {
            let cx = PalwBlockContextV2 { block: h(10 + n), daa_score: 4 + n, blue_score: 6 + n, subsidy: 0 };
            let (s, d) = apply_palw_transition_v2(states.last().unwrap(), &params, &cx, &[bond(n)], None).unwrap();
            states.push(s);
            deltas.push(d);
        }
        // Rows newest first: block n's header commits states[n - 1] in `form`'s shape; its delta root is states[n]'s root.
        let rows = |form: Option<ForkActivation>, keep: bool| -> Vec<Arc<NativeChainRow>> {
            (1..=3usize)
                .rev()
                .map(|n| {
                    Arc::new(NativeChainRow {
                        hash: h(10 + n as u64),
                        parent: h(9 + n as u64),
                        daa: 4 + n as u64,
                        blue: 6 + n as u64,
                        header_palw_root: palw_committed_state_root_v1(&states[n - 1], form),
                        delta_root: Some(states[n].state_root()),
                        evidence: NativeDeltaEvidenceV1::default(),
                        fork_choice: keep.then(|| Box::new(PalwForkChoiceDeltaV1::of(&deltas[n - 1]))),
                    })
                })
                .collect()
        };
        let sink = &states[3];
        let (dormant, armed, mid) = (None, Some(ForkActivation::new(0)), Some(ForkActivation::new(6)));
        assert_eq!(native_roots_chain_v1(&rows(dormant, false), sink, dormant), Ok(()), "dormant: the flat comparison");
        assert_eq!(native_roots_chain_v1(&rows(armed, true), sink, armed), Ok(()), "armed: every header commits the envelope");
        assert_eq!(
            native_roots_chain_v1(&rows(mid, true), sink, mid),
            Ok(()),
            "armed at DAA 6: states at 5 flat, at 6 and 7 enveloped"
        );
        // Headers that committed the flat form where the envelope was due: the first enveloped row (block 12, at DAA 6) breaks.
        assert_eq!(native_roots_chain_v1(&rows(dormant, true), sink, mid), Err(h(12)));
        // Without the deltas' leaf part the leaf cannot be rebuilt past the sink, and is never assumed.
        assert_eq!(native_roots_chain_v1(&rows(armed, false), sink, armed), Err(h(12)));
        // A forged header root: block 13's header commits another state for block 12.
        let mut forged = rows(armed, true);
        forged[0] = Arc::new(NativeChainRow { header_palw_root: h(0xBAD), ..clone_row(&forged[0]) });
        assert_eq!(native_roots_chain_v1(&forged, sink, armed), Err(h(12)));
    }

    fn clone_row(r: &NativeChainRow) -> NativeChainRow {
        NativeChainRow {
            hash: r.hash,
            parent: r.parent,
            daa: r.daa,
            blue: r.blue,
            header_palw_root: r.header_palw_root,
            delta_root: r.delta_root,
            evidence: NativeDeltaEvidenceV1::default(),
            fork_choice: r.fork_choice.clone(),
        }
    }
}
