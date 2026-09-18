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
        acceptance_data::AcceptanceDataStoreReader, block_transactions::BlockTransactionsStoreReader, dns_state::DnsStateStoreReader,
        headers::HeaderStoreReader, stake_bonds::StakeBondsStoreReader,
    },
};
use kaspa_consensus_core::{
    BlockHash, Hash64,
    config::params::DnsBftGateV1,
    dns_bft_v1::{
        DnsBftAttestationV1, DnsBftChainBlockV1, DnsBftEpochV1, DnsBftEpochVerdictV1, DnsBftRulesV1, PrecommitRecord,
        decode_stake_precommit_v1, dns_bft_confirmed_anchor_v1, dns_bft_evaluate_epochs_v1, dns_bft_precommit_duty_v1,
        dns_bft_window_epochs_v1, newest_dns_final_v1,
    },
    dns_finality::{
        ATTESTATION_MLDSA87_CONTEXT, DnsParams, DnsReorgOutcome, DnsRolloutStage, DnsState, DnsTxKind, PRECOMMIT_MLDSA87_CONTEXT,
        PrecommitDuty, PrecommitLock, StakeAttestation, StakeBondRecord, StakePrecommitPayload, anchor_cutoff_blue_score,
        canonical_lagged_epoch_anchor, decode_attestation_shard, dns_tx_kind, is_bond_active_at, ready_epoch_from_tip_blue_score,
        stake_attestation_message, stake_precommit_message,
    },
    tx::{TransactionId, TransactionOutpoint},
};
use kaspa_core::{debug, info, warn};
use kaspa_txscript::verify_mldsa87_with_context;
use parking_lot::Mutex;
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

/// How many signature verdicts the memo keeps before it starts over. A walk that reaches back
/// `t_leak_daa` holds every vote of that span, so the bound is sized for a validator set's week of
/// attestations and precommits at a two-minute cadence, with room; starting over costs one cold
/// evaluation, never a different answer.
const DNS_BFT_VERIFIED_MEMO_LIMIT: usize = 1 << 18;

/// **ADR-0128's node-local runtime state.** Nothing in it is consensus: it records what this node
/// could read, and memoises work whose answer cannot change.
#[derive(Default)]
pub(crate) struct DnsBftRuntime {
    /// The last evaluation could not cover its walk: the gate abstains until one does.
    gate_abstains: AtomicBool,
    /// ML-DSA-87 verdicts by `(transaction id, the vote's index in it)`. The transaction id commits to
    /// the vote's bytes; the bond outpoint inside them fixes the key it verifies under (a bond's key is
    /// in the bond transaction's own payload); the network id is this node's. So the verdict for a key
    /// never changes, and a walk that reads a week of votes verifies each of them once.
    verified: Mutex<HashMap<(TransactionId, u32), bool>>,
    /// The last duty evaluation and the sink it was made at: a validator asks every heartbeat, and
    /// the chain it asks about moves only when the sink does.
    duty: Mutex<Option<(BlockHash, Arc<DnsBftEvaluation>)>>,
    /// **What each selected-chain block accepted, by block hash** ([`AcceptedVotesV1`]). A walk reads
    /// a week of chain blocks every epoch, and each of them merges every block of its mergeset — a
    /// hundred and twenty execution blocks at one a second past ADR-0125's height — so re-reading
    /// their transactions every epoch is the evaluation's whole cost. Only the blocks the latest
    /// covered walk visited are kept, so the memo is bounded by the walk.
    accepted: Mutex<HashMap<BlockHash, Arc<AcceptedVotesV1>>>,
}

impl DnsBftRuntime {
    /// Whether the last evaluation could not cover its walk.
    pub(crate) fn gate_abstains(&self) -> bool {
        self.gate_abstains.load(Ordering::Relaxed)
    }

    /// Drop every memoised block, as a restart does.
    #[cfg(test)]
    pub(crate) fn forget_accepted_votes(&self) {
        self.accepted.lock().clear();
    }

    fn verified(&self, key: (TransactionId, u32), verify: impl FnOnce() -> bool) -> bool {
        if let Some(&verdict) = self.verified.lock().get(&key) {
            return verdict;
        }
        let verdict = verify();
        let mut memo = self.verified.lock();
        if memo.len() >= DNS_BFT_VERIFIED_MEMO_LIMIT {
            memo.clear();
        }
        memo.insert(key, verdict);
        verdict
    }
}

/// A vote the walk read, before any rule looked at it.
struct RawVote<T> {
    vote: T,
    /// `(transaction id, index in it)` — the memo key.
    key: (TransactionId, u32),
    accepted_blue_score: u64,
    accepted_daa_score: u64,
    /// The signature's verdict when the vote was read under a bond this node held — the vote's bytes
    /// then no longer carry the signature — or `None`, with the signature kept, when it did not.
    judged: Option<bool>,
}

/// **The votes one selected-chain block accepted, read once.** A block's acceptance data is a function
/// of the block, so the entry for a hash is the same whichever chain holds it and whichever sink walks
/// it, and it holds exactly what the stores hold: an evaluation from the memo is the evaluation a node
/// reading the stores makes. A signature is judged when its bond is known at the read and its bytes are
/// dropped (an ML-DSA-87 signature is 4,627 of a vote's ~4,900 bytes); the verdict is a function of the
/// vote and its bond's key, which a bond outpoint fixes.
struct AcceptedVotesV1 {
    attestations: Vec<RawVote<StakeAttestation>>,
    precommits: Vec<RawVote<StakePrecommitPayload>>,
}

/// **Everything one evaluation at a sink read, and what it decided.**
pub(crate) struct DnsBftEvaluation {
    pub(crate) rules: DnsBftRulesV1,
    /// The StakeScore window's epochs, decided, ascending.
    pub(crate) verdicts: Vec<DnsBftEpochVerdictV1>,
    /// Every signed precommit the walk read — the lock chains the duty is answered from.
    pub(crate) precommits: Vec<PrecommitRecord>,
    /// Selected-chain blocks the walk read, for the log.
    pub(crate) walked: usize,
    /// Of those, the blocks whose votes were read from the stores rather than the walk's memo.
    pub(crate) read: usize,
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
        let sink_blue =
            self.headers_store.get_blue_score(sink).map_err(|e| format!("the sink {sink}'s header does not read ({e})"))?;
        let window = dns_params.stake_score_window_blue_score;
        let epoch_len = rules.epoch_length_blue_score;
        let lag = dns_params.attestation_lag_blue_score;
        let backoff = dns_params.attestation_anchor_backoff_blue_score;

        let bonds_by_outpoint: HashMap<TransactionOutpoint, &StakeBondRecord> = bonds.iter().map(|b| (b.bond_outpoint, b)).collect();
        let mut chain: Vec<DnsBftChainBlockV1> = Vec::new();
        let mut accepted: Vec<Arc<AcceptedVotesV1>> = Vec::new();
        let mut read = 0usize;
        let mut epochs: Option<Vec<DnsBftEpochV1>> = None;
        // ADR-0138: two floors — the blue span and the DAA span (`in_evidence_window`).
        let mut floor: Option<(u64, u64)> = None;
        let mut covered = false;
        for block in self.reachability_service.default_backward_chain_iterator(sink) {
            let compact = self
                .headers_store
                .get_compact_header_data(block)
                .map_err(|e| format!("chain block {block}'s header does not read before the walk's bound ({e})"))?;
            if epochs.is_none() && sink_blue.saturating_sub(compact.blue_score) > window {
                let evaluated = dns_bft_window_epochs_v1(&chain, sink_blue, window, epoch_len, lag, backoff);
                floor = Some((
                    evaluated
                        .iter()
                        .map(|e| rules.evidence_floor_blue_score(e))
                        .min()
                        .unwrap_or_else(|| sink_blue.saturating_sub(window)),
                    evaluated.iter().map(|e| rules.evidence_floor_daa_score(e)).min().unwrap_or(0),
                ));
                epochs = Some(evaluated);
            }
            let block_point = DnsBftChainBlockV1 { hash: block, blue_score: compact.blue_score, daa_score: compact.daa_score };
            // The walk ends only where BOTH spans are behind it: a blue-only bound reaches back
            // fewer DAA than the leak is decided over, once the attempt lane stops ticking the DAA
            // clock (ADR-0138), and the leak would then never fire.
            if floor.is_some_and(|(blue_floor, daa_floor)| compact.blue_score < blue_floor && compact.daa_score < daa_floor) {
                // Read for the anchors it decides, never for what it accepted.
                chain.push(block_point);
                covered = true;
                break;
            }
            let memoised = self.dns_bft_runtime.accepted.lock().get(&block).cloned();
            let votes = match memoised {
                Some(votes) => votes,
                None => {
                    let votes = Arc::new(self.dns_bft_read_accepted_votes(block, &block_point, &bonds_by_outpoint)?);
                    read += 1;
                    self.dns_bft_runtime.accepted.lock().insert(block, votes.clone());
                    votes
                }
            };
            accepted.push(votes);
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
        // The walk covered its bound: keep exactly the blocks it visited.
        {
            let visited: HashSet<BlockHash> = chain.iter().map(|b| b.hash).collect();
            self.dns_bft_runtime.accepted.lock().retain(|hash, _| visited.contains(hash));
        }
        let epochs = epochs.unwrap_or_else(|| dns_bft_window_epochs_v1(&chain, sink_blue, window, epoch_len, lag, backoff));
        let (Some(newest), Some(latest_ready)) = (epochs.last().copied(), ready_epoch_from_tip_blue_score(sink_blue, epoch_len, lag))
        else {
            return Ok(DnsBftEvaluation { rules, verdicts: Vec::new(), precommits: Vec::new(), walked, read });
        };

        let ancestors: Vec<(Hash64, u64, u64)> = chain.iter().map(|b| (b.hash, b.blue_score, b.daa_score)).collect();
        let oldest_blue = chain.last().map_or(sink_blue, |b| b.blue_score);
        let evaluated: HashMap<u64, DnsBftEpochV1> = epochs.iter().map(|e| (e.epoch, *e)).collect();
        let mut anchors: HashMap<u64, Option<(Hash64, u64)>> = HashMap::new();

        let mut attestations: Vec<DnsBftAttestationV1> = Vec::new();
        for RawVote { vote: att, key, accepted_blue_score, accepted_daa_score, judged } in
            accepted.iter().flat_map(|votes| votes.attestations.iter())
        {
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
            let signed = judged.unwrap_or_else(|| self.dns_bft_attestation_signed(*key, att, bond));
            if !signed {
                continue;
            }
            attestations.push(DnsBftAttestationV1 {
                validator_id: att.validator_id,
                bond_outpoint: att.bond_outpoint,
                epoch: att.epoch,
                anchor_hash,
                anchor_daa_score: anchor_daa,
                accepted_blue_score: *accepted_blue_score,
                accepted_daa_score: *accepted_daa_score,
            });
        }

        let mut precommits: Vec<PrecommitRecord> = Vec::new();
        for RawVote { vote: p, key, accepted_blue_score, accepted_daa_score, judged } in
            accepted.iter().flat_map(|votes| votes.precommits.iter())
        {
            if !p.lock_is_self_consistent() {
                continue;
            }
            let Some(bond) = bonds_by_outpoint.get(&p.bond_outpoint) else {
                continue;
            };
            if p.validator_id != bond.validator_pubkey_hash {
                continue;
            }
            let signed = judged.unwrap_or_else(|| self.dns_bft_precommit_signed(*key, p, bond));
            if !signed {
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
                accepted_blue_score: *accepted_blue_score,
                accepted_daa_score: *accepted_daa_score,
            });
        }

        let verdicts = dns_bft_evaluate_epochs_v1(&epochs, &chain, bonds, &attestations, &precommits, &rules);
        Ok(DnsBftEvaluation { rules, verdicts, precommits, walked, read })
    }

    /// **What `block` accepted, read from the stores** ([`AcceptedVotesV1`]): its acceptance data, the
    /// transactions of every block it merged, and of those only the two vote subnetworks, decoded, with
    /// `block`'s scores. A read that fails is the walk's coverage gap.
    fn dns_bft_read_accepted_votes(
        &self,
        block: BlockHash,
        at: &DnsBftChainBlockV1,
        bonds_by_outpoint: &HashMap<TransactionOutpoint, &StakeBondRecord>,
    ) -> Result<AcceptedVotesV1, String> {
        let acceptance = self
            .acceptance_data_store
            .get(block)
            .map_err(|e| format!("chain block {block}'s acceptance data does not read before the walk's bound ({e})"))?;
        let (accepted_blue_score, accepted_daa_score) = (at.blue_score, at.daa_score);
        let mut votes = AcceptedVotesV1 { attestations: Vec::new(), precommits: Vec::new() };
        for mergeset in acceptance.iter() {
            let block_txs = self.block_transactions_store.get(mergeset.block_hash).map_err(|e| {
                format!("the transactions of {}, merged by chain block {block}, do not read ({e})", mergeset.block_hash)
            })?;
            for entry in mergeset.accepted_transactions.iter() {
                let Some(tx) = block_txs.get(entry.index_within_block as usize) else {
                    continue;
                };
                match dns_tx_kind(&tx.subnetwork_id) {
                    Some(DnsTxKind::StakeAttestationShard) => {
                        let Some(shard) = decode_attestation_shard(tx) else { continue };
                        for (i, mut vote) in shard.attestations.into_iter().enumerate() {
                            let key = (entry.transaction_id, i as u32);
                            let judged = bonds_by_outpoint
                                .get(&vote.bond_outpoint)
                                .map(|bond| self.dns_bft_attestation_signed(key, &vote, bond));
                            if judged.is_some() {
                                vote.signature = Vec::new();
                            }
                            votes.attestations.push(RawVote { vote, key, accepted_blue_score, accepted_daa_score, judged });
                        }
                    }
                    Some(DnsTxKind::StakePrecommit) => {
                        let Some(mut vote) = decode_stake_precommit_v1(tx) else { continue };
                        let key = (entry.transaction_id, 0);
                        let judged =
                            bonds_by_outpoint.get(&vote.bond_outpoint).map(|bond| self.dns_bft_precommit_signed(key, &vote, bond));
                        if judged.is_some() {
                            vote.signature = Vec::new();
                        }
                        votes.precommits.push(RawVote { vote, key, accepted_blue_score, accepted_daa_score, judged });
                    }
                    _ => {}
                }
            }
        }
        Ok(votes)
    }

    /// Whether `att`'s signature verifies under `bond`'s key, memoised by vote.
    fn dns_bft_attestation_signed(&self, key: (TransactionId, u32), att: &StakeAttestation, bond: &StakeBondRecord) -> bool {
        let digest = stake_attestation_message(
            self.genesis.hash.as_byte_slice(),
            att.epoch,
            att.target_hash,
            att.target_daa_score,
            att.validator_set_commitment,
            att.bond_outpoint,
        )
        .as_bytes();
        self.dns_bft_runtime.verified(key, || {
            matches!(
                verify_mldsa87_with_context(&bond.validator_pubkey, &digest, &att.signature, ATTESTATION_MLDSA87_CONTEXT),
                Ok(true)
            )
        })
    }

    /// Whether `p`'s signature verifies under `bond`'s key, memoised by vote.
    fn dns_bft_precommit_signed(&self, key: (TransactionId, u32), p: &StakePrecommitPayload, bond: &StakeBondRecord) -> bool {
        let digest = stake_precommit_message(
            self.genesis.hash.as_byte_slice(),
            p.epoch,
            p.target_hash,
            p.target_daa_score,
            p.locked_epoch,
            p.locked_hash,
            p.snapshot_commitment,
            p.bond_outpoint,
        )
        .as_bytes();
        self.dns_bft_runtime.verified(key, || {
            matches!(verify_mldsa87_with_context(&bond.validator_pubkey, &digest, &p.signature, PRECOMMIT_MLDSA87_CONTEXT), Ok(true))
        })
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
    /// * when the walk cannot cover its bound, the confirmation does not advance — the previous one is
    ///   carried by the same rule — and the gate abstains ([`Self::dns_bft_gate_refusal`] reads the
    ///   flag this sets) until an evaluation covers it; the log says why.
    pub(super) fn dns_bft_confirmed_state(
        &self,
        mut depth_state: DnsState,
        prev: Option<&DnsState>,
        sink: BlockHash,
        bonds: &[StakeBondRecord],
        dns_params: &DnsParams,
        gate: &DnsBftGateV1,
    ) -> DnsState {
        let carried = prev
            .filter(|p| gate.activation.is_active(p.anchor_daa_score))
            .map(|p| (p.last_dns_confirmed_anchor, p.last_dns_confirmed_anchor_daa_score))
            .filter(|(anchor, _)| *anchor != Hash64::default())
            // Unreadable reachability is an anchor behind the pruning point: kept, as the gate treats it
            // (included).
            .filter(|(anchor, _)| self.reachability_service.try_is_chain_ancestor_of(*anchor, sink).unwrap_or(true));
        let evaluation = self.dns_bft_evaluate(sink, bonds, dns_params, gate);
        self.dns_bft_runtime.gate_abstains.store(evaluation.is_err(), Ordering::Relaxed);
        let confirmed = match evaluation {
            Err(gap) => {
                warn!(
                    "[dns-bft] sink {sink}: the evaluation walk does not cover its bound — {gap}. The confirmed anchor does not \
                     advance, and the stake reorg gate abstains until an evaluation covers its walk"
                );
                carried
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
                debug!(
                    "[dns-bft] sink={sink}: walked {} chain blocks ({} read from the stores), {} epochs evaluated",
                    evaluation.walked,
                    evaluation.read,
                    evaluation.verdicts.len()
                );
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
    /// fence. It is `None` below the fence; while the last evaluation could not cover its walk (the
    /// gate abstains rather than judge on evidence this node does not hold); where no state is
    /// written; where the state predates the fence (the depth rule's anchor is not the vote's);
    /// outside the `Active` stage; with nothing confirmed; where the anchor is behind the pruning
    /// point; and for a candidate that contains the anchor.
    ///
    /// It only refuses. It never selects a tip, and a released candidate still has to win the
    /// comparator.
    pub(super) fn dns_bft_gate_refusal(&self, candidate: BlockHash, prev_sink: BlockHash) -> Option<DnsReorgOutcome> {
        let dns_params = self.dns_params.as_ref()?;
        let incumbent_daa = self.headers_store.get_daa_score(prev_sink).ok()?;
        let gate = self.dns_bft_gate.filter(|gate| gate.activation.is_active(incumbent_daa))?;
        if self.dns_bft_runtime.gate_abstains() {
            debug!("[dns-bft] gate: the last evaluation could not cover its walk; abstaining for candidate {candidate}");
            return None;
        }
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
                debug!(
                    "[dns-bft] gate: confirmed anchor {anchor} has no reachability (behind the pruning point); the gate does not refuse"
                );
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
    /// precommit for ([`dns_bft_precommit_duty_v1`]). The evaluation is remembered for the sink it was
    /// made at, so a validator asking every heartbeat walks once per sink. An evaluation that cannot
    /// cover its walk owes nothing — a node that cannot count the set cannot tell a validator what
    /// to sign.
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
        let remembered =
            self.dns_bft_runtime.duty.lock().as_ref().filter(|(at, _)| *at == sink).map(|(_, evaluation)| evaluation.clone());
        let evaluation = match remembered {
            Some(evaluation) => evaluation,
            None => {
                let bonds: Vec<StakeBondRecord> =
                    self.stake_bonds_store.read().iterator().filter_map(|r| r.ok().map(|(_, rec)| (*rec).clone())).collect();
                match self.dns_bft_evaluate(sink, &bonds, dns_params, &gate) {
                    Ok(evaluation) => {
                        let evaluation = Arc::new(evaluation);
                        *self.dns_bft_runtime.duty.lock() = Some((sink, evaluation.clone()));
                        evaluation
                    }
                    Err(gap) => {
                        warn!(
                            "[dns-bft] precommit duty at sink {sink}: the evaluation walk does not cover its bound — {gap}; nothing is due"
                        );
                        return Some(PrecommitDuty { round_active: true, sink_daa_score: sink_daa, ..Default::default() });
                    }
                }
            }
        };
        Some(dns_bft_precommit_duty_v1(
            true,
            sink_daa,
            &evaluation.verdicts,
            &evaluation.precommits,
            validator_id,
            bond_outpoint,
            &evaluation.rules,
        ))
    }
}
