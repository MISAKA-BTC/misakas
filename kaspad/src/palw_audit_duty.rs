//! **ADR-0160 F-Q (stage 2, rcore/cap-s1): the operator's audit duty** — node policy; a child module of
//! `palw_panel`, whose loop holds its one book and calls [`PalwPanelService::audit_duty_tick_v1`] once a
//! tick, after lane B's.
//!
//! **Why.** Past `Params::palw_capacity_audit_door` a CREDITED claim (its escrow slot cut below its reward
//! by a ramp step's credit) reaches `Final` only once `k_aud` distinct members of its audit pool have
//! posted a receipt that its roots and answer reproduce (`palw_audit_door_v1`). The pool is the operator's
//! bonds that are neither its producer nor its panel's seats. So an operator node replays each credited
//! claim in whose pool it sits, and receipts it when the replay reproduces: the detection the credit rests
//! on is an on-chain object (`AuditReceiptBatchV1`, tag 60), not a desk estimate.
//!
//! **Who** — identity decides, nothing opts in ([[protocol-duties-are-always-on-not-flags]]): a node whose
//! bond is in a claim's pool (the chain's read, `palw_capacity_audit_candidates_v1`, is empty for any other
//! bond and below the fence) and that carries (`--palw-fee-outpoint`).
//!
//! **Turns** ([`palw_audit_on_turn_v1`]): the pool's first `k_aud` members at once, one more every 30
//! DAA after the licence, so a member that is down is covered by the next in rank.
//!
//! **What it does on a verdict** (lane B's replay rule, `palw_operator_da_verdict_v1`): *reproduces* — an
//! entry `(claim, execution root)` joins this node's next batch, signed under its bond key over the
//! network-bound message (`palw_audit_receipt_batch_message_v1`) and carried on the court queue;
//! *refuted* — no receipt, said as the error it is (the door holds the claim: without receipts it never
//! reaches `Final`, never pays; the conviction is the filers' — lane B's DA accusation, the replay filer);
//! *unjudged* (the class does not resolve here, the block is not held) — said loudly, the next member's
//! turn covers it.
//!
//! **What a restart loses**: the book (the queued batch is re-built from the chain's read next DAA).

use super::*;
use kaspa_consensus_core::palw_audit_door_v1::{
    PALW_AUDIT_RECEIPT_BATCH_MAX_ENTRIES_V1, PALW_AUDIT_RECEIPT_V1_MLDSA87_CONTEXT, PalwAuditCandidateV1, PalwAuditEntryV1,
    palw_audit_on_turn_v1, palw_audit_receipt_batch_message_v1,
};
use kaspa_core::error;

/// The court-queue round the audit duty's batches ride under (lane B's is `u32::MAX − 7`).
pub(super) const PALW_AUDIT_QUEUE_ROUND_V1: u32 = u32::MAX - 8;

/// DAA after which an entry queued and not yet on chain is queued again.
const PALW_AUDIT_RESEND_DAA_V1: u64 = 3;

/// Whether a court-queue entry is this lane's.
pub(super) fn palw_audit_queued_v1(round: u32, responder: bool, object: &PalwConsensusObjectV2) -> bool {
    matches!(object, PalwConsensusObjectV2::AuditReceiptBatchV1 { .. }) && (round, responder) == (PALW_AUDIT_QUEUE_ROUND_V1, false)
}

/// **The audit duty's book** — the chain's candidates for this bond (read once a DAA), the one replay in
/// flight, this node's verdicts, and the entries it reproduced and has not seen on chain.
pub(super) struct PalwAuditDutyV1 {
    candidates: Vec<PalwAuditCandidateV1>,
    read_at: Option<u64>,
    /// Claims this node judged: `true` reproduced (receipt owed until the chain shows it), `false` not.
    verdicts: BTreeMap<Hash64, bool>,
    /// claim → (the reproduced root, the DAA its batch was last queued; `None` not yet queued).
    owed: BTreeMap<Hash64, (Hash64, Option<u64>)>,
    pub(super) replays: PalwSeatReplaysV1,
    pub(super) replaying: Option<(Hash64, PalwSeatReplayKeyV1)>,
}

impl PalwAuditDutyV1 {
    pub(super) fn new() -> Self {
        Self {
            candidates: Vec::new(),
            read_at: None,
            verdicts: BTreeMap::new(),
            owed: BTreeMap::new(),
            replays: PalwSeatReplaysV1::default(),
            replaying: None,
        }
    }

    /// The chain's read: keep verdicts and owed entries only for claims still offered (a claim that left —
    /// audited, receipted by this node, gone — owes nothing more).
    fn refresh(&mut self, candidates: Vec<PalwAuditCandidateV1>, now_daa: u64) {
        let offered: std::collections::BTreeSet<Hash64> = candidates.iter().map(|c| c.claim_id).collect();
        self.verdicts.retain(|claim, _| offered.contains(claim));
        self.owed.retain(|claim, _| offered.contains(claim));
        self.candidates = candidates;
        self.read_at = Some(now_daa);
    }

    /// The next claim to replay: on turn for `me`, not yet judged here.
    fn next_replay(&self, me: &PalwBondKeyV2, now_daa: u64) -> Option<&PalwAuditCandidateV1> {
        self.candidates.iter().find(|c| !self.verdicts.contains_key(&c.claim_id) && palw_audit_on_turn_v1(c, me, now_daa))
    }

    /// The entries due in a batch now: owed, never queued or queued more than the resend delay ago, in
    /// claim order, at most one batch's worth.
    fn due_entries(&self, now_daa: u64) -> Vec<PalwAuditEntryV1> {
        self.owed
            .iter()
            .filter(|(_, (_, queued))| queued.is_none_or(|at| now_daa >= at.saturating_add(PALW_AUDIT_RESEND_DAA_V1)))
            .map(|(claim, (root, _))| PalwAuditEntryV1 { claim_id: *claim, reproduced_root: *root })
            .take(PALW_AUDIT_RECEIPT_BATCH_MAX_ENTRIES_V1)
            .collect()
    }
}

impl PalwPanelService {
    /// **One tick of the audit duty** — read the chain's candidates once a DAA, poll the replay in flight
    /// (a verdict: reproduced → owed; refuted → said), start the next replay on turn, and queue one signed
    /// batch of the owed entries on the court queue.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn audit_duty_tick_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        duty: &mut PalwAuditDutyV1,
        current_daa: u64,
        network_domain: Hash64,
        bond_key: PalwBondKeyV2,
        seat_replay_room: bool,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_due: &mut HashMap<(Hash64, u32, bool), u64>,
    ) {
        if self.config.fee_outpoint.is_none() {
            return;
        }
        if duty.read_at != Some(current_daa) {
            let candidates = session.clone().spawn_blocking(move |c| c.palw_capacity_audit_candidates_v1(bond_key)).await;
            duty.refresh(candidates, current_daa);
        }
        if duty.candidates.is_empty() && duty.replaying.is_none() {
            court_pending.retain(|(_, round, responder, object)| !palw_audit_queued_v1(*round, *responder, object));
            return;
        }
        // The replay in flight, polled every tick until it returns.
        if let Some((claim, key)) = duty.replaying {
            match duty.replays.poll(&key, current_daa).await {
                PalwSeatReplayPollV1::Running => {}
                PalwSeatReplayPollV1::Absent => duty.replaying = None,
                PalwSeatReplayPollV1::Done { result, .. } => {
                    duty.replaying = None;
                    if let Some(candidate) = duty.candidates.iter().find(|c| c.claim_id == claim).cloned() {
                        match super::palw_operator_da::palw_operator_da_verdict_v1(&result, &candidate.job) {
                            Some(super::palw_operator_da::PalwOperatorDaVerdictV1::Reproduces) => {
                                info!(
                                    "[{PALW_PANEL}] claim {claim}: this operator's audit reproduces the credited claim — receipt owed (ADR-0160 F-Q)"
                                );
                                duty.verdicts.insert(claim, true);
                                duty.owed.insert(claim, (candidate.job.execution_root, None));
                            }
                            Some(super::palw_operator_da::PalwOperatorDaVerdictV1::Refuted) => {
                                error!(
                                    "[{PALW_PANEL}] claim {claim}: this operator's audit does NOT reproduce the credited claim (replayed {:?}; \
                                     claimed execution {}) — no receipt: the audit door holds it from Final and payment; the filers \
                                     take it to a conviction (ADR-0160 F-Q §5.9 (e))",
                                    result.as_ref().ok(),
                                    candidate.job.execution_root
                                );
                                duty.verdicts.insert(claim, false);
                            }
                            other => {
                                warn!(
                                    "[{PALW_PANEL}] claim {claim}: this operator's audit came to no verdict ({other:?}, {:?}); the next pool \
                                     member's turn covers it (ADR-0160 F-Q)",
                                    result.as_ref().err()
                                );
                                duty.verdicts.insert(claim, false);
                            }
                        }
                    }
                }
            }
        }
        // The next replay on turn.
        if duty.replaying.is_none()
            && seat_replay_room
            && let Some(candidate) = duty.next_replay(&bond_key, current_daa).cloned()
        {
            self.audit_duty_start_replay_v1(session, duty, &candidate, current_daa, network_domain, bond_key);
        }
        // One batch of the owed entries, signed, on the court queue.
        let queued = court_pending.iter().any(|(_, round, responder, object)| palw_audit_queued_v1(*round, *responder, object));
        let entries = duty.due_entries(current_daa);
        if queued || entries.is_empty() {
            return;
        }
        let message = palw_audit_receipt_batch_message_v1(network_domain, &bond_key, &entries);
        let Some(signature) = self.sign(message.as_byte_slice(), PALW_AUDIT_RECEIPT_V1_MLDSA87_CONTEXT) else {
            warn!("[{PALW_PANEL}] the audit duty holds {} receipt(s) and no key to sign them (ADR-0160 F-Q)", entries.len());
            return;
        };
        let key = (message, PALW_AUDIT_QUEUE_ROUND_V1, false);
        info!("[{PALW_PANEL}] the audit duty queues a receipt batch of {} credited claim(s) (ADR-0160 F-Q, tag 60)", entries.len());
        for entry in &entries {
            if let Some((_, queued)) = duty.owed.get_mut(&entry.claim_id) {
                *queued = Some(current_daa);
            }
        }
        court_due.insert(key, current_daa);
        court_pending.push((
            key.0,
            key.1,
            key.2,
            PalwConsensusObjectV2::AuditReceiptBatchV1 { auditor: bond_key, entries, signature },
        ));
    }

    /// **Start this node's audit replay of `candidate`** — lane B's replay of the anchor's job, off the loop,
    /// under the host ledger's reservation at the full seat's need. A claim this host cannot replay is judged
    /// not reproduced here (loudly); a ledger refusal waits the next tick.
    fn audit_duty_start_replay_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        duty: &mut PalwAuditDutyV1,
        candidate: &PalwAuditCandidateV1,
        current_daa: u64,
        network_domain: Hash64,
        bond_key: PalwBondKeyV2,
    ) {
        let claim = candidate.claim_id;
        let job = &candidate.job;
        let unjudged = |duty: &mut PalwAuditDutyV1, why: &str| {
            warn!(
                "[{PALW_PANEL}] claim {claim}: this operator is in the credited claim's audit pool and cannot replay it — {why} (ADR-0160 F-Q)"
            );
            duty.verdicts.insert(claim, false);
        };
        let Some(artifact_root) = job.artifact_root.filter(|_| super::palw_operator_da::palw_operator_da_replayable_v1(job)) else {
            return unjudged(duty, "the lane does not replay this claim");
        };
        let backend = match self.resolve_backend(session, job.class_id, artifact_root) {
            Ok(backend) => backend,
            Err(why) => return unjudged(duty, &format!("its class does not resolve on this host ({why})")),
        };
        let Some((ctx, prompt)) = self.attempt_job_for_claim(
            session,
            backend.as_ref(),
            network_domain,
            job.accepted_block,
            job.class_id,
            &candidate.producer,
        ) else {
            return unjudged(duty, "its block's header is not held here");
        };
        let key = (claim, ctx.job_id);
        let need = self.backends().role_memory_need_for_backend_or_chain_v1(
            backend.as_ref(),
            job.class_id,
            artifact_root,
            Some(&ctx),
            kaspa_consensus_core::palw_resource_profile_v1::PalwResourceRoleV1::FullSeat,
            |id| self.chain_carriage_v1(session, id),
        );
        let reserved =
            match self.reserve_replay_v1(super::palw_operator_da::PALW_OPERATOR_DA_REPLAY_ROLE_V1, &need, job.class_id, claim) {
                Ok(reserved) => reserved,
                Err(why) => {
                    crate::palw_backends::note_throttled_v1("panel-audit-duty-ledger", || {
                        format!("[{PALW_PANEL}] claim {claim}: the audit replay waits: {why} (ADR-0160 F-Q)")
                    });
                    return;
                }
            };
        info!(
            "[{PALW_PANEL}] claim {claim}: auditing the credited claim by replaying its job (ADR-0160 F-Q; pool rank {:?} of {}, k_aud {})",
            candidate.pool.iter().position(|bond| *bond == bond_key),
            candidate.pool.len(),
            candidate.k_aud
        );
        duty.replays
            .start(key, job.class_id, false, current_daa, Some(reserved), backend, move |b| b.execute_for_verdict(&ctx, &prompt));
        duty.replaying = Some((claim, key));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_operator_da_v1::PalwOperatorDaJobV1;

    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(Hash64::from_u64_word(0xB0), n as u32))
    }

    fn candidate(claim: u64, licensed_daa: u64, pool: Vec<PalwBondKeyV2>, k_aud: usize) -> PalwAuditCandidateV1 {
        PalwAuditCandidateV1 {
            claim_id: Hash64::from_u64_word(claim),
            producer: bond(20),
            licensed_daa,
            pool,
            k_aud,
            receipted: vec![],
            job: PalwOperatorDaJobV1 {
                accepted_block: Hash64::from_u64_word(0xB10C),
                class_id: Hash64::from_u64_word(0xF1),
                artifact_root: Some(Hash64::from_u64_word(0xA7)),
                execution_root: Hash64::from_u64_word(0xE0 + claim),
                trace_root: Hash64::from_u64_word(0x7E),
                output_root: Hash64::from_u64_word(0x0E),
                work_leaves: 0,
                free_prompt: false,
                held_to_final: false,
            },
        }
    }

    /// The turn rule: the first `k_aud` at once, one more a turn; a bond outside the pool never.
    #[test]
    fn the_pool_audits_k_aud_at_once_and_one_more_a_turn() {
        let c = candidate(1, 1_000, vec![bond(1), bond(2), bond(3)], 1);
        assert!(palw_audit_on_turn_v1(&c, &bond(1), 1_000));
        assert!(!palw_audit_on_turn_v1(&c, &bond(2), 1_029));
        assert!(palw_audit_on_turn_v1(&c, &bond(2), 1_030), "rank 1 joins after one turn");
        assert!(palw_audit_on_turn_v1(&c, &bond(3), 1_060));
        assert!(!palw_audit_on_turn_v1(&c, &bond(9), 5_000), "outside the pool, never");
        let two = candidate(2, 1_000, vec![bond(1), bond(2), bond(3)], 2);
        assert!(palw_audit_on_turn_v1(&two, &bond(2), 1_000), "k_aud 2: two at once");
    }

    /// The book: a refresh keeps only offered claims; owed entries are batched in claim order and resent
    /// after the delay, never twice within it.
    #[test]
    fn the_book_batches_owed_receipts_and_forgets_what_left() {
        let mut duty = PalwAuditDutyV1::new();
        duty.refresh(vec![candidate(1, 1_000, vec![bond(1)], 1), candidate(2, 1_000, vec![bond(1)], 1)], 1_001);
        duty.owed.insert(Hash64::from_u64_word(2), (Hash64::from_u64_word(0xE2), None));
        duty.owed.insert(Hash64::from_u64_word(1), (Hash64::from_u64_word(0xE1), None));
        let entries = duty.due_entries(1_001);
        assert_eq!(entries.iter().map(|e| e.claim_id).collect::<Vec<_>>(), vec![Hash64::from_u64_word(1), Hash64::from_u64_word(2)]);
        for (_, queued) in duty.owed.values_mut() {
            *queued = Some(1_001);
        }
        assert!(duty.due_entries(1_002).is_empty(), "not resent within the delay");
        assert_eq!(duty.due_entries(1_004).len(), 2, "resent after it");
        duty.refresh(vec![candidate(2, 1_000, vec![bond(1)], 1)], 1_005);
        assert_eq!(duty.owed.len(), 1, "claim 1 left the candidates (audited): nothing owed for it");
        assert!(duty.next_replay(&bond(1), 1_005).is_some_and(|c| c.claim_id == Hash64::from_u64_word(2)));
    }
}
