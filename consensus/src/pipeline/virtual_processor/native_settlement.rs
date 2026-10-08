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

use super::VirtualStateProcessor;
use crate::model::{
    services::reachability::ReachabilityService,
    stores::{evm::EvmHeaderStoreReader, ghostdag::GhostdagStoreReader, headers::HeaderStoreReader, pruning::PruningStoreReader},
};
use kaspa_consensus_core::{
    BlockHash, Hash64,
    palw_native_settlement_v1::{
        MatureUsefulWorkV1, NativeDeltaEvidenceV1, NativeEffectV1, NativeFactRulesV1, NativeSettlementSnapshotV1, SettlementStopV1,
        certify_native_prefix_v1, native_delta_evidence_v1, native_facts_of_block_v1,
    },
    palw_state_v2::PalwClaimPhaseV2,
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

/// Why the chain walk could not produce rows: the snapshot then carries `MissingHistory` (never a guess).
struct WalkFault;

impl VirtualStateProcessor {
    /// Read (or load and remember) the row of `cursor` if it is executed with a root-verified EVM result.
    /// `Ok(None)`: not executed (yet). `Err`: an unreadable row, or a result that disagrees with the header.
    fn native_row(&self, cursor: BlockHash, pruning: BlockHash) -> Result<Result<Arc<NativeChainRow>, (u64, BlockHash)>, WalkFault> {
        if let Some(row) = self.native_rows.lock().rows.get(&cursor) {
            return Ok(Ok(row.clone()));
        }
        let Ok(header) = self.headers_store.get_header(cursor) else {
            return Err(WalkFault);
        };
        match self.evm_header_store.get(cursor) {
            Ok(execution) if execution.commitment_root() == header.evm_commitment_root => {}
            Ok(_) => return Err(WalkFault),
            Err(StoreError::KeyNotFound(_)) => {
                let parent = self.ghostdag_store.get_selected_parent(cursor).map_err(|_| WalkFault)?;
                return Ok(Err((header.daa_score, parent)));
            }
            Err(_) => return Err(WalkFault),
        }
        let (delta_root, evidence) = if cursor == pruning || header.daa_score < self.evm_activation_daa_score {
            (None, NativeDeltaEvidenceV1::default())
        } else {
            // A missing or undecodable delta is a gap in the verified history, not an empty block.
            let Ok((root, delta)) = self.palw_state_v2_store.read().delta_of(cursor) else {
                return Err(WalkFault);
            };
            (Some(root), native_delta_evidence_v1(&delta))
        };
        let parent = self.ghostdag_store.get_selected_parent(cursor).map_err(|_| WalkFault)?;
        let row = Arc::new(NativeChainRow {
            hash: cursor,
            parent,
            daa: header.daa_score,
            blue: header.blue_score,
            header_palw_root: header.palw_state_root,
            delta_root,
            evidence,
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
        let retirement = self.palw_dns_retirement.expect("called only after the retirement fence");
        let sink_daa = self.headers_store.get_daa_score(sink).unwrap_or(0);
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
        let previous = match self.evm_heads_store.read().native_snapshot() {
            Ok(Some(s))
                if s.version == 1 && s.policy_id == retirement.settlement.id() && s.ruleset_id == self.palw_native_ruleset_id =>
            {
                Some(s)
            }
            Ok(None) | Err(StoreError::KeyNotFound(_)) => None,
            _ => return result, // Corrupt or incompatible evidence is never silently forgotten.
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
                            return Err(WalkFault);
                        }
                        if cursor == pruning || daa < self.evm_activation_daa_score {
                            return Ok(());
                        }
                        cursor = parent;
                    }
                }
                if cursor == kaspa_consensus_core::blockhash::ORIGIN {
                    return Err(WalkFault);
                }
            }
        })();

        // 2. A below-finalized conflict remains an alarm until a validated resync/import clears it.
        // Publishing absent heads once must not let the next block forget the conflict.
        if previous.as_ref().is_some_and(|s| s.stop == Some(SettlementStopV1::FinalizedConflict)) {
            result.stop = Some(SettlementStopV1::FinalizedConflict);
            return result;
        }
        if let Some(previous_finalized) = previous.and_then(|s| s.finalized) {
            match self.reachability_service.try_is_chain_ancestor_of(previous_finalized, sink) {
                Ok(false) => {
                    error!("[native-settlement] FINALIZED CONFLICT: {sink} abandons {previous_finalized}; resync required");
                    result.stop = Some(SettlementStopV1::FinalizedConflict);
                    return result;
                }
                Err(_) => return result,
                Ok(true) => {}
            }
        }
        if walked.is_err() {
            return result;
        }
        if rows.is_empty() {
            result.stop = Some(SettlementStopV1::Unexecuted);
            return result;
        }

        // 3. The PALW state the fork choice weighs, and its frontier.
        let Some(state) = self.palw_candidate_state_v2(sink) else {
            return result;
        };
        let Some(params) = self.palw_state_params_v2.as_ref() else {
            return result;
        };
        let (frontier_blue, frontier) = state.safe_frontier();
        result.frontier = (frontier != BlockHash::default()).then_some(frontier);
        let frontier_on_branch =
            frontier != BlockHash::default() && self.reachability_service.try_is_dag_ancestor_of(frontier, sink).unwrap_or(false);

        // 4. The roots chain: each block's delta root is what its child's header committed to as the parent state.
        for (i, row) in rows.iter().enumerate() {
            let Some(root) = row.delta_root else {
                continue;
            };
            let expected = if i == 0 { state.state_root() } else { rows[i - 1].header_palw_root };
            if root != expected {
                return result;
            }
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
        for row in rows.iter().filter(|r| r.hash != pruning) {
            facts.extend(native_facts_of_block_v1(&rules, &row.evidence, &voided, (row.daa, row.blue)));
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
        let open_claims = state
            .claims_iter()
            .filter(|(_, c)| {
                !(matches!(c.phase, PalwClaimPhaseV2::Voided { .. })
                    || (matches!(c.phase, PalwClaimPhaseV2::Final { .. }) && c.trace_retention_daa <= sink_daa))
            })
            .map(|(_, c)| c.accepted_blue_score)
            .min();
        let open_sessions = state.da_sessions_iter().map(|((claim, _), _)| state.claim(claim).map_or(0, |c| c.accepted_blue_score)).min();
        let open_from = open_claims.into_iter().chain(open_sessions).min().unwrap_or(u64::MAX);

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
        result
    }
}
