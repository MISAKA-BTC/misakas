//! **ADR-0128 — the DNS validators' BFT vote, read from the selected chain this node holds.**
//!
//! Three consumers, one walk:
//!
//! * `update_dns_state`, past the fence at the sink, replaces the StakeScore depth rule's
//!   confirmation with the newest DNS-final anchor ([`VirtualStateProcessor::dns_bft_confirmed_state`]);
//! * `dns_reorg_outcome`, past the fence at the incumbent sink, refuses a candidate that abandons
//!   that anchor before the PALW comparator runs ([`VirtualStateProcessor::dns_bft_gate_refusal`]);
//! * `ConsensusApi::get_precommit_duty` answers what round two asks of one bond
//!   ([`VirtualStateProcessor::dns_bft_precommit_duty`]).
//!
//! The rules themselves are pure and live in [`kaspa_consensus_core::dns_bft_v1`]; this file reads the
//! chain, verifies signatures, and hands the verified inputs over.

use super::VirtualStateProcessor;
use crate::model::{
    services::reachability::ReachabilityService,
    stores::{
        acceptance_data::AcceptanceDataStoreReader, dns_state::DnsStateStoreReader, headers::HeaderStoreReader,
        stake_bonds::StakeBondsStoreReader,
    },
};
use kaspa_consensus_core::{
    BlockHash, Hash64,
    config::params::DnsBftGateV1,
    dns_bft_v1::{
        DnsBftAttestationV1, DnsBftChainBlockV1, DnsBftEpochV1, DnsBftEpochVerdictV1, DnsBftRulesV1, PrecommitRecord,
        dns_bft_confirmed_anchor_v1, dns_bft_evaluate_epochs_v1, dns_bft_precommit_duty_v1, dns_bft_window_epochs_v1,
        newest_dns_final_v1, precommits_from_accepted_txs,
    },
    dns_finality::{
        ATTESTATION_MLDSA87_CONTEXT, DnsParams, DnsReorgOutcome, DnsRolloutStage, DnsState, PRECOMMIT_MLDSA87_CONTEXT, PrecommitDuty,
        PrecommitLock, StakeAttestation, StakeBondRecord, StakePrecommitPayload, anchor_cutoff_blue_score, attestations_from_accepted_txs,
        canonical_lagged_epoch_anchor, is_bond_active_at, ready_epoch_from_tip_blue_score, stake_attestation_message,
        stake_precommit_message,
    },
    tx::TransactionOutpoint,
};
use kaspa_core::{debug, info, warn};
use kaspa_txscript::verify_mldsa87_with_context;
use std::collections::HashMap;

/// **Everything one evaluation at a sink read, and what it decided.**
pub(crate) struct DnsBftEvaluation {
    pub(crate) rules: DnsBftRulesV1,
    /// The StakeScore window's epochs, decided, ascending.
    pub(crate) verdicts: Vec<DnsBftEpochVerdictV1>,
    /// Every signed precommit the walk read — the lock chains the duty is answered from.
    pub(crate) precommits: Vec<PrecommitRecord>,
    /// Selected-chain blocks the walk read, for the log.
    pub(crate) walked: usize,
}

impl VirtualStateProcessor {
    /// **The one walk (ADR-0128 Decision 3): the sink's StakeScore window, then the evidence window
    /// below the oldest epoch it evaluates.**
    ///
    /// From the sink down the selected chain, reading each block's header and the attestations and
    /// precommits it accepted, with that block's blue and DAA scores. The first bound is the
    /// StakeScore window: the blocks inside it decide which epochs are evaluated, exactly as the
    /// credit walk decides them. The second is `L` below the oldest evaluated anchor, plus the first
    /// block beneath it, which is what decides the canonical anchors of the oldest epochs an
    /// attestation there can name.
    ///
    /// **Coverage.** The walk covers its bound when it reaches that block below the floor, or when
    /// the chain ends at genesis (a chain shorter than the bound has nothing more to read). A header
    /// or acceptance-data read that fails first, or a chain that ends at any other block (a pruning
    /// point above the floor, a sync still in progress), is an `Err`: this node cannot compute the
    /// counted sets every covering node computes, so it computes none (SA-3). The pruning-depth
    /// refusal in `validate_palw_v2` keeps a synced node from ever landing here.
    ///
    /// Then, verified against `bonds`: every attestation that names the canonical non-duplicate
    /// anchor of a decidable, ready epoch, bound to an existing bond `Active` at that anchor, with a
    /// zero validator-set commitment and a valid signature — and only when it can speak for an
    /// evaluated epoch (it names one, or its anchor is older than the newest); and every precommit
    /// with a self-consistent lock, bound to an existing bond, with a valid signature. The rules
    /// decide the rest.
    pub(crate) fn dns_bft_evaluate(
        &self,
        sink: BlockHash,
        bonds: &[StakeBondRecord],
        dns_params: &DnsParams,
        gate: &DnsBftGateV1,
    ) -> Result<DnsBftEvaluation, String> {
        let rules = DnsBftRulesV1::new(gate, dns_params).ok_or_else(|| "the evidence window does not fit in a u64".to_owned())?;
        let sink_blue = self.headers_store.get_blue_score(sink).map_err(|e| format!("the sink {sink}'s header does not read ({e})"))?;
        let window = dns_params.stake_score_window_blue_score;
        let epoch_len = rules.epoch_length_blue_score;
        let lag = dns_params.attestation_lag_blue_score;
        let backoff = dns_params.attestation_anchor_backoff_blue_score;

        let mut chain: Vec<DnsBftChainBlockV1> = Vec::new();
        let mut raw_attestations: Vec<(StakeAttestation, u64, u64)> = Vec::new();
        let mut raw_precommits: Vec<(StakePrecommitPayload, u64, u64)> = Vec::new();
        let mut epochs: Option<Vec<DnsBftEpochV1>> = None;
        let mut floor: Option<u64> = None;
        let mut covered = false;
        for block in self.reachability_service.default_backward_chain_iterator(sink) {
            let compact = self
                .headers_store
                .get_compact_header_data(block)
                .map_err(|e| format!("chain block {block}'s header does not read before the walk's bound ({e})"))?;
            if epochs.is_none() && sink_blue.saturating_sub(compact.blue_score) > window {
                let evaluated = dns_bft_window_epochs_v1(&chain, sink_blue, window, epoch_len, lag, backoff);
                floor = Some(
                    evaluated.iter().map(|e| rules.evidence_floor_blue_score(e)).min().unwrap_or_else(|| sink_blue.saturating_sub(window)),
                );
                epochs = Some(evaluated);
            }
            let block_point = DnsBftChainBlockV1 { hash: block, blue_score: compact.blue_score, daa_score: compact.daa_score };
            if floor.is_some_and(|floor| compact.blue_score < floor) {
                // Read for the anchors it decides, never for what it accepted.
                chain.push(block_point);
                covered = true;
                break;
            }
            let acceptance = self
                .acceptance_data_store
                .get(block)
                .map_err(|e| format!("chain block {block}'s acceptance data does not read before the walk's bound ({e})"))?;
            let txs = self.accepted_txs_from_acceptance_data(&acceptance);
            raw_attestations.extend(attestations_from_accepted_txs(&txs).into_iter().map(|a| (a, compact.blue_score, compact.daa_score)));
            raw_precommits.extend(precommits_from_accepted_txs(&txs).into_iter().map(|p| (p, compact.blue_score, compact.daa_score)));
            chain.push(block_point);
        }
        let reached_genesis = chain.last().is_some_and(|b| b.hash == self.genesis.hash);
        if !covered && !reached_genesis {
            return Err(format!(
                "the selected chain ends at {} before the walk's bound (floor {:?}, sink blue score {sink_blue})",
                chain.last().map_or_else(|| "nothing".to_owned(), |b| b.hash.to_string()),
                floor
            ));
        }
        let walked = chain.len();
        let epochs = epochs.unwrap_or_else(|| dns_bft_window_epochs_v1(&chain, sink_blue, window, epoch_len, lag, backoff));
        let (Some(newest), Some(latest_ready)) = (epochs.last().copied(), ready_epoch_from_tip_blue_score(sink_blue, epoch_len, lag))
        else {
            return Ok(DnsBftEvaluation { rules, verdicts: Vec::new(), precommits: Vec::new(), walked });
        };

        let net_id = self.genesis.hash;
        let bonds_by_outpoint: HashMap<TransactionOutpoint, &StakeBondRecord> = bonds.iter().map(|b| (b.bond_outpoint, b)).collect();
        let ancestors: Vec<(Hash64, u64, u64)> = chain.iter().map(|b| (b.hash, b.blue_score, b.daa_score)).collect();
        let oldest_blue = chain.last().map_or(sink_blue, |b| b.blue_score);
        let evaluated: HashMap<u64, DnsBftEpochV1> = epochs.iter().map(|e| (e.epoch, *e)).collect();
        let mut anchors: HashMap<u64, Option<(Hash64, u64)>> = HashMap::new();

        let mut attestations: Vec<DnsBftAttestationV1> = Vec::new();
        for (att, accepted_blue, accepted_daa) in raw_attestations {
            if att.epoch > latest_ready {
                continue;
            }
            // Decidable from what the walk holds: the previous epoch's cutoff is not below it.
            if !reached_genesis && anchor_cutoff_blue_score(att.epoch.saturating_sub(1), epoch_len, backoff) < oldest_blue {
                continue;
            }
            let anchor = *anchors.entry(att.epoch).or_insert_with(|| {
                canonical_lagged_epoch_anchor(att.epoch, epoch_len, backoff, &ancestors)
                    .filter(|a| !a.duplicate_of_previous_anchor)
                    .map(|a| (a.anchor_hash, a.anchor_daa_score))
            });
            let Some((anchor_hash, anchor_daa)) = anchor else {
                continue;
            };
            if att.target_hash != anchor_hash || att.target_daa_score != anchor_daa {
                continue;
            }
            // Speaks for no evaluated epoch: not one of them, and not older than the newest.
            if !evaluated.contains_key(&att.epoch) && anchor_daa >= newest.anchor_daa_score {
                continue;
            }
            let Some(bond) = bonds_by_outpoint.get(&att.bond_outpoint) else {
                continue;
            };
            if att.validator_id != bond.validator_pubkey_hash
                || !is_bond_active_at(bond, anchor_daa)
                || att.validator_set_commitment != Hash64::default()
            {
                continue;
            }
            let digest = stake_attestation_message(
                net_id.as_byte_slice(),
                att.epoch,
                att.target_hash,
                att.target_daa_score,
                att.validator_set_commitment,
                att.bond_outpoint,
            )
            .as_bytes();
            if !matches!(
                verify_mldsa87_with_context(&bond.validator_pubkey, &digest, &att.signature, ATTESTATION_MLDSA87_CONTEXT),
                Ok(true)
            ) {
                continue;
            }
            attestations.push(DnsBftAttestationV1 {
                validator_id: att.validator_id,
                bond_outpoint: att.bond_outpoint,
                epoch: att.epoch,
                anchor_hash,
                anchor_daa_score: anchor_daa,
                accepted_blue_score: accepted_blue,
                accepted_daa_score: accepted_daa,
            });
        }

        let mut precommits: Vec<PrecommitRecord> = Vec::new();
        for (p, accepted_blue, accepted_daa) in raw_precommits {
            if !p.lock_is_self_consistent() {
                continue;
            }
            let Some(bond) = bonds_by_outpoint.get(&p.bond_outpoint) else {
                continue;
            };
            if p.validator_id != bond.validator_pubkey_hash {
                continue;
            }
            let digest = stake_precommit_message(
                net_id.as_byte_slice(),
                p.epoch,
                p.target_hash,
                p.target_daa_score,
                p.locked_epoch,
                p.locked_hash,
                p.snapshot_commitment,
                p.bond_outpoint,
            )
            .as_bytes();
            if !matches!(verify_mldsa87_with_context(&bond.validator_pubkey, &digest, &p.signature, PRECOMMIT_MLDSA87_CONTEXT), Ok(true)) {
                continue;
            }
            precommits.push(PrecommitRecord {
                validator_id: p.validator_id,
                bond_outpoint: p.bond_outpoint,
                epoch: p.epoch,
                target_hash: p.target_hash,
                target_daa_score: p.target_daa_score,
                declared_lock: PrecommitLock { epoch: p.locked_epoch, anchor: p.locked_hash },
                snapshot_commitment: p.snapshot_commitment,
                accepted_blue_score: accepted_blue,
                accepted_daa_score: accepted_daa,
            });
        }

        let verdicts = dns_bft_evaluate_epochs_v1(&epochs, &chain, bonds, &attestations, &precommits, &rules);
        Ok(DnsBftEvaluation { rules, verdicts, precommits, walked })
    }

    /// **ADR-0128 Decision 5, in `update_dns_state`: the confirmed anchor follows the vote.**
    ///
    /// `depth_state` is the state the StakeScore depth rule computed at this sink — every field of it
    /// (StakeScore, work depth, health, rollout stage) is kept, and only the confirmed anchor and its
    /// DAA are replaced:
    ///
    /// * the previous confirmation is carried forward when this rule made it (the previous state was
    ///   written past the fence — an anchor the depth rule confirmed below it gains no veto it never
    ///   had) and it is still a chain ancestor of the sink;
    /// * the newest DNS-final anchor of the sink's StakeScore window replaces it when newer;
    /// * when the walk cannot cover its bound, nothing is confirmed in this state, so the gate
    ///   abstains (`GateInactive`) until an evaluation that covers it, and the log says why.
    pub(super) fn dns_bft_confirmed_state(
        &self,
        mut depth_state: DnsState,
        prev: Option<&DnsState>,
        sink: BlockHash,
        bonds: &[StakeBondRecord],
        dns_params: &DnsParams,
        gate: &DnsBftGateV1,
    ) -> DnsState {
        let confirmed = match self.dns_bft_evaluate(sink, bonds, dns_params, gate) {
            Err(gap) => {
                warn!(
                    "[dns-bft] sink {sink}: the evaluation walk does not cover its bound — {gap}. No anchor is confirmed at this \
                     evaluation and the stake reorg gate abstains until one covers it"
                );
                None
            }
            Ok(evaluation) => {
                // The newest epoch at info, once per evaluation (once per blue-score epoch); the rest
                // of the window at debug.
                for (i, verdict) in evaluation.verdicts.iter().rev().enumerate() {
                    let line = format!(
                        "[dns-bft] sink={} epoch={} anchor={} counted={} validators={} W={} leaked={}{} attested={} precommitted={} round1={} final={}",
                        sink,
                        verdict.epoch.epoch,
                        verdict.epoch.anchor_hash,
                        verdict.counted.bonds.len(),
                        verdict.counted.validator_count(),
                        verdict.counted.total_stake,
                        verdict.counted.leaked.len(),
                        if verdict.counted.floor_held { " (floor held)" } else { "" },
                        verdict.attested_stake,
                        verdict.precommitted_stake,
                        if verdict.round_one() { "met" } else { "no" },
                        if verdict.dns_final() { "yes" } else { "no" },
                    );
                    if i == 0 {
                        info!("{line}");
                    } else {
                        debug!("{line}");
                    }
                }
                debug!("[dns-bft] sink={sink}: walked {} chain blocks, {} epochs evaluated", evaluation.walked, evaluation.verdicts.len());
                let carried = prev
                    .filter(|p| gate.activation.is_active(p.anchor_daa_score))
                    .map(|p| (p.last_dns_confirmed_anchor, p.last_dns_confirmed_anchor_daa_score))
                    .filter(|(anchor, _)| *anchor != Hash64::default())
                    // Unreadable reachability is an anchor behind the pruning point: kept, as the gate
                    // treats it (included).
                    .filter(|(anchor, _)| self.reachability_service.try_is_chain_ancestor_of(*anchor, sink).unwrap_or(true));
                let newest = newest_dns_final_v1(&evaluation.verdicts).map(|e| (e.anchor_hash, e.anchor_daa_score));
                let confirmed = dns_bft_confirmed_anchor_v1(carried, newest);
                if confirmed.is_some() && confirmed != carried {
                    info!(
                        "[dns-bft] sink={sink}: DNS-final anchor {} (DAA {}) is now the confirmed anchor",
                        confirmed.map_or(Hash64::default(), |c| c.0),
                        confirmed.map_or(0, |c| c.1)
                    );
                }
                confirmed
            }
        };
        let (anchor, anchor_daa) = confirmed.unwrap_or((Hash64::default(), 0));
        depth_state.last_dns_confirmed_anchor = anchor;
        depth_state.last_dns_confirmed_anchor_daa_score = anchor_daa;
        depth_state
    }

    /// **ADR-0128 Decision 5, in `dns_reorg_outcome`: the gate follows the vote.**
    ///
    /// Past the fence at the incumbent sink, a candidate — a reorg or an extension alike — that does
    /// not contain the confirmed anchor is refused (`HardCheckpointReject`), unless that anchor is
    /// stale under `dns_veto_ttl_daa_score` measured on this node's own chain, which releases it.
    /// `None` is "this gate does not refuse": the caller goes on to the PALW comparator and ADR-0065
    /// D2 (or, on a network without a PALW authority, the gate below) exactly as it does without the
    /// fence. It is `None` below the fence; where no state is written; where the state predates the
    /// fence (the depth rule's anchor is not the vote's); outside the `Active` stage; with nothing
    /// confirmed (including an evaluation that could not cover its walk); where the anchor is behind
    /// the pruning point; and for a candidate that contains the anchor.
    ///
    /// It only refuses. It never selects a tip, and a released candidate still has to win the
    /// comparator.
    pub(super) fn dns_bft_gate_refusal(&self, candidate: BlockHash, prev_sink: BlockHash) -> Option<DnsReorgOutcome> {
        let dns_params = self.dns_params.as_ref()?;
        let incumbent_daa = self.headers_store.get_daa_score(prev_sink).ok()?;
        let gate = self.dns_bft_gate.filter(|gate| gate.activation.is_active(incumbent_daa))?;
        let state = self.dns_state_store.read().get().ok()?;
        if !gate.activation.is_active(state.anchor_daa_score)
            || state.rollout_stage != DnsRolloutStage::Active
            || state.last_dns_confirmed_anchor == Hash64::default()
        {
            return None;
        }
        let anchor = state.last_dns_confirmed_anchor;
        match self.reachability_service.try_is_chain_ancestor_of(anchor, candidate) {
            Ok(true) => None,
            Err(_) => {
                debug!("[dns-bft] gate: confirmed anchor {anchor} has no reachability (behind the pruning point); the gate does not refuse");
                None
            }
            Ok(false) if dns_params.confirmed_anchor_is_stale(incumbent_daa, state.last_dns_confirmed_anchor_daa_score) => {
                warn!(
                    "[dns-bft] gate: DNS-final anchor {anchor} is STALE — this node's chain advanced {} DAA past it (TTL {}) without a \
                     newer DNS-final anchor; releasing the veto for candidate {candidate}, which the PALW comparator still weighs",
                    incumbent_daa.saturating_sub(state.last_dns_confirmed_anchor_daa_score),
                    dns_params.dns_veto_ttl_daa_score
                );
                None
            }
            Ok(false) => {
                debug!("[dns-bft] gate: candidate {candidate} abandons DNS-final anchor {anchor}; refused");
                Some(DnsReorgOutcome::HardCheckpointReject)
            }
        }
    }

    /// **ADR-0128 Decision 6: what round two asks of `(validator_id, bond_outpoint)` at `sink`.**
    ///
    /// `None` where the network runs no overlay. Below the fence `round_active` is `false` and nothing
    /// else is answered. Past it, the evaluation `update_dns_state` makes, re-made at `sink` over the
    /// bond store as it stands: the lock the chain shows the bond holding and the epochs it owes a
    /// precommit for ([`dns_bft_precommit_duty_v1`]). An evaluation that cannot cover its walk owes
    /// nothing — a node that cannot count the set cannot tell a validator what to sign.
    pub(crate) fn dns_bft_precommit_duty(
        &self,
        sink: BlockHash,
        validator_id: Hash64,
        bond_outpoint: TransactionOutpoint,
    ) -> Option<PrecommitDuty> {
        let dns_params = self.dns_params.as_ref()?;
        let sink_daa = self.headers_store.get_daa_score(sink).ok()?;
        let Some(gate) = self.dns_bft_gate.filter(|gate| gate.activation.is_active(sink_daa)) else {
            return Some(PrecommitDuty { round_active: false, sink_daa_score: sink_daa, ..Default::default() });
        };
        let bonds: Vec<StakeBondRecord> =
            self.stake_bonds_store.read().iterator().filter_map(|r| r.ok().map(|(_, rec)| (*rec).clone())).collect();
        match self.dns_bft_evaluate(sink, &bonds, dns_params, &gate) {
            Ok(evaluation) => Some(dns_bft_precommit_duty_v1(
                true,
                sink_daa,
                &evaluation.verdicts,
                &evaluation.precommits,
                validator_id,
                bond_outpoint,
                &evaluation.rules,
            )),
            Err(gap) => {
                warn!("[dns-bft] precommit duty at sink {sink}: the evaluation walk does not cover its bound — {gap}; nothing is due");
                Some(PrecommitDuty { round_active: true, sink_daa_score: sink_daa, ..Default::default() })
            }
        }
    }
}
