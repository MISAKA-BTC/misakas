//! **lane GC-DUTY: the Gate C drawn verifier's duty for chain claims (`'S'`) and owning39 mining claims (`'V'`)** — node policy; a
//! child module of `palw_panel`, whose loop holds its one book and calls [`PalwPanelService::cert_duty_tick_v1`] once a tick, after the
//! audit duty. The kernel route's `'K'` rows are the kernel roles' (`palw_kernel_roles`, same object, own round).
//!
//! **Why.** Past `palw_independently_work_certified_v1` (Gate C, RFC-0018) a governed claim's consensus credit and its held
//! issuance reward leg are released only at `SecurityCertified`: `k` (41) of `m` (75) stake-weighted draws from the global verifier pool
//! must attest `Reproduced` before the row's deadline. Without a node duty every chain claim expires uncertified and REAL mining
//! rewards stop at the fence.
//!
//! **Who** — identity decides, nothing opts in ([[protocol-duties-are-always-on-not-flags]]): a node whose bond is among a row's draws
//! (the chain's read, `palw_certification_duties_v1`, is empty for any other bond and below Gate C) and that carries
//! (`--palw-fee-outpoint`) with a key. Inert below the fence.
//!
//! **What it does.** For each owed row, earliest deadline first: re-execute the claim on lane B's replay seam (the same job descriptor,
//! backend resolution, host memory ledger reservation and verdict rule the audit duty uses) and sign inner 75
//! (`AttestEligibilityExamV1`, `exam` = the certification id): `Reproduced` with a transcript digest of what it replayed when the replay
//! reproduces the claim's roots, `NotReproducible` when it refutes them, `Unavailable` when it cannot check the claim here (an owning39
//! mining claim — no node replay exists for it yet —, a claim lane B does not replay, or a claim this host could not judge before the
//! last margin of the window). This is the drawn verifier's signed approval under the honest-fraction premise (PRINCIPLES §8): no
//! cryptographic proof of computation is made or used.
//!
//! **Resources (t12 BASE-0 starvation, 2026-10-26).** The duty owns ONE replay slot of its own (its own [`PalwSeatReplaysV1`], one in
//! flight), so a seat host whose seat replay slots stay full still attests; memory stays bounded by the shared host ledger
//! (`--palw-host-memory-share`) reservation, which refuses — and the duty waits a tick — when the host has no room.
//!
//! **What a restart loses**: the book; every duty re-derives from the chain.

use super::*;
use crate::palw_panel::court_outbox::CourtOutboxV1;
use kaspa_consensus_core::palw_kernel_route_v1::{
    PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1, palw_kernel_bond_id_v1, palw_kernel_route_message_v1,
};
use kaspa_consensus_core::palw_security_certification_v1::PalwCertificationDutyV1;
use kaspa_core::error;
use misaka_palw_kernel::model_eligibility_v1::{ELIGIBILITY_ATTESTATION_VERSION_V1, EligibilityAttestationV1, EligibilityVerdictV1};
use misaka_palw_kernel::route::KernelRouteObjectV1 as K;

/// The court-queue round the duty's attestations ride under (the kernel roles hold `u32::MAX − 40..=43`).
pub(super) const PALW_CERT_DUTY_ROUND_V1: u32 = u32::MAX - 44;
/// DAA after which an unjudged replay is tried again.
pub(super) const PALW_CERT_DUTY_RETRY_DAA_V1: u64 = 30;
/// DAA before the deadline at which an unjudged row is attested `Unavailable` instead of waiting further (room for carriage).
pub(super) const PALW_CERT_DUTY_MARGIN_DAA_V1: u64 = 60;
/// DAA after which a queued attestation not yet on chain is queued again.
const PALW_CERT_DUTY_RESEND_DAA_V1: u64 = 6;

/// **Which replay seam re-executes an owed row** (pure; unit-tested).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PalwCertDutyRouteV1 {
    /// An Attempt (LegacyPanelAttempt, and LegacyFusedLeaf: an Attempt of a fused-attention class): lane B's replay of the anchor's
    /// job; `heavy` for a class the panel room holds to `Final` (C7), which takes the heavy slot.
    Attempt { heavy: bool },
    /// A free-prompt claim (FpSelection / FpConstraint / FpPrefix / FpTokenizer): SEAT-R's free-prompt pass
    /// (`fp_seat_replay_pass_v1`) over the job material this node holds (pool and retention).
    FreePrompt { heavy: bool },
    /// An owning39 mining claim (`'V'`): no node replay (see the module doc).
    Native,
    /// The claim's facts or its class's registered root are gone: nobody can re-execute it.
    Gone,
}

pub(super) fn palw_cert_duty_route_v1(d: &PalwCertificationDutyV1) -> PalwCertDutyRouteV1 {
    if d.native {
        return PalwCertDutyRouteV1::Native;
    }
    match (d.job.as_ref(), d.seat_duty.as_ref()) {
        (Some(job), Some(_)) if job.artifact_root.is_some() => {
            if job.free_prompt {
                PalwCertDutyRouteV1::FreePrompt { heavy: job.held_to_final }
            } else {
                PalwCertDutyRouteV1::Attempt { heavy: job.held_to_final }
            }
        }
        _ => PalwCertDutyRouteV1::Gone,
    }
}

/// This node's verdict on one owed certification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PalwCertDutyVerdictV1 {
    Reproduced { execution_root: Hash64 },
    NotReproducible,
    Unavailable,
    Unjudged { retry_at: u64 },
}

/// **The duty's book** — the chain's owed rows (read once a DAA), the one replay in flight, verdicts and queue dates.
pub(super) struct PalwCertDutyV1 {
    owed: Vec<PalwCertificationDutyV1>,
    read_at: Option<u64>,
    verdicts: BTreeMap<Hash64, PalwCertDutyVerdictV1>,
    queued: BTreeMap<Hash64, u64>,
    pub(super) replays: PalwSeatReplaysV1,
    replaying: Option<(Hash64, PalwSeatReplayKeyV1)>,
    routing_owner: Option<Arc<kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingClaimAuthorityV2>>,
    /// The free-prompt row whose SEAT-R pass is in progress (one at a time, in the same slot).
    fp_active: Option<Hash64>,
}

/// The verdict to attest now for `duty`, or `None` while the duty still waits for a judgement (pure; unit-tested).
pub(super) fn palw_cert_duty_attest_now_v1(
    duty: &PalwCertificationDutyV1,
    verdict: Option<PalwCertDutyVerdictV1>,
    now_daa: u64,
) -> Option<PalwCertDutyVerdictV1> {
    let late = now_daa.saturating_add(PALW_CERT_DUTY_MARGIN_DAA_V1) >= duty.deadline_daa;
    match verdict {
        Some(PalwCertDutyVerdictV1::Unjudged { .. }) | None if late => Some(PalwCertDutyVerdictV1::Unavailable),
        Some(PalwCertDutyVerdictV1::Unjudged { .. }) | None => None,
        other => other,
    }
}

/// The transcript a `Reproduced` attestation commits: a digest of what this bond replayed (never all-zero).
pub(super) fn palw_cert_duty_transcript_v1(certification_id: &[u8; 64], me: &[u8; 64], execution_root: &Hash64) -> [u8; 64] {
    let mut seen = Vec::with_capacity(192);
    seen.extend_from_slice(certification_id);
    seen.extend_from_slice(me);
    seen.extend_from_slice(&execution_root.as_bytes());
    misaka_palw_kernel::hash::id(b"misaka-palw/kaspad/gate-c-duty/transcript/v1", &seen)
}

impl PalwCertDutyV1 {
    pub(super) fn new() -> Self {
        let mut replays = PalwSeatReplaysV1::default();
        // Its own single slot: never competes with the seat replays for theirs.
        replays.configure_slots(Some(1));
        Self {
            owed: Vec::new(),
            read_at: None,
            verdicts: BTreeMap::new(),
            queued: BTreeMap::new(),
            replays,
            replaying: None,
            routing_owner: None,
            fp_active: None,
        }
    }

    fn refresh(&mut self, owed: Vec<PalwCertificationDutyV1>, now_daa: u64) {
        let open: std::collections::BTreeSet<Hash64> = owed.iter().map(|d| d.claim_id).collect();
        self.verdicts.retain(|claim, _| open.contains(claim));
        self.queued.retain(|claim, _| open.contains(claim));
        self.owed = owed;
        self.read_at = Some(now_daa);
    }

    /// The next row to replay: a replayable chain claim not yet judged here (or whose retry is due), outside the last margin.
    fn next_replay(&self, now_daa: u64, free_prompt: bool) -> Option<&PalwCertificationDutyV1> {
        self.owed.iter().find(|d| {
            let open = match self.verdicts.get(&d.claim_id) {
                None => true,
                Some(PalwCertDutyVerdictV1::Unjudged { retry_at }) => now_daa >= *retry_at,
                Some(_) => false,
            };
            let route = palw_cert_duty_route_v1(d);
            let seam = if free_prompt {
                matches!(route, PalwCertDutyRouteV1::FreePrompt { .. })
            } else {
                matches!(route, PalwCertDutyRouteV1::Attempt { .. })
            };
            open && seam && now_daa.saturating_add(PALW_CERT_DUTY_MARGIN_DAA_V1) < d.deadline_daa
        })
    }
}

impl PalwPanelService {
    /// **One tick of the Gate C `'S'`/`'V'` duty** — read the owed rows once a DAA, poll the replay in flight, start the next replay
    /// in the duty's own slot, and queue a signed attestation for every row with a verdict to attest.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn cert_duty_tick_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        duty: &mut PalwCertDutyV1,
        current_daa: u64,
        network_domain: Hash64,
        bond_key: PalwBondKeyV2,
        fp_materials: &HashMap<Hash64, Vec<Vec<u8>>>,
        court_outbox: &mut CourtOutboxV1,
    ) {
        let mine = |round: u32, responder: bool| (round, responder) == (PALW_CERT_DUTY_ROUND_V1, false);
        court_outbox.retain_submitted(|(claim, round, responder), at| {
            if !mine(*round, *responder) {
                return true;
            }
            info!("[{PALW_PANEL}] claim {claim}: this drawn verifier's Gate C attestation was carried at DAA {at} (GC-DUTY)");
            false
        });
        if self.config.fee_outpoint.is_none() || self.keypair.is_none() {
            return;
        }
        if duty.read_at != Some(current_daa) {
            let owed = session.clone().spawn_blocking(move |c| c.palw_certification_duties_v1(bond_key, current_daa)).await;
            duty.refresh(owed, current_daa);
        }
        if duty.owed.is_empty() && duty.replaying.is_none() {
            court_outbox.retain_pending(|(_, round, responder, _)| !mine(*round, *responder));
            return;
        }
        // Structurally uncheckable here: attest Unavailable at once (an owning39 mining claim; a claim whose class left the registry).
        for d in &duty.owed {
            let route = palw_cert_duty_route_v1(d);
            if !duty.verdicts.contains_key(&d.claim_id) && matches!(route, PalwCertDutyRouteV1::Native | PalwCertDutyRouteV1::Gone) {
                warn!(
                    "[{PALW_PANEL}] claim {}: drawn for its Gate C certification ({} position(s)) and this node cannot re-execute it ({}) — \
                     attesting Unavailable (GC-DUTY)",
                    d.claim_id,
                    d.positions,
                    if d.native { "owning39 mining claim: no node replay" } else { "its facts or class root are gone" }
                );
                duty.verdicts.insert(d.claim_id, PalwCertDutyVerdictV1::Unavailable);
            }
        }
        // The replay in flight.
        if let Some((claim, key)) = duty.replaying {
            match duty.replays.poll(&key, current_daa).await {
                PalwSeatReplayPollV1::Running => {}
                PalwSeatReplayPollV1::Absent => duty.replaying = None,
                PalwSeatReplayPollV1::Done { result, .. } => {
                    duty.replaying = None;
                    let routing_owner = duty.routing_owner.take();
                    if let Err(why) = self.refresh_native_routing_replay_v2(session, routing_owner.as_ref()) {
                        warn!("[{PALW_PANEL}] claim {claim}: Gate C replay native routing context changed: {why}");
                    }
                    if let Some(job) = duty.owed.iter().find(|d| d.claim_id == claim).and_then(|d| d.job.clone()) {
                        let verdict = match super::palw_operator_da::palw_operator_da_verdict_v1(&result, &job) {
                            Some(super::palw_operator_da::PalwOperatorDaVerdictV1::Reproduces) => {
                                PalwCertDutyVerdictV1::Reproduced { execution_root: job.execution_root }
                            }
                            Some(super::palw_operator_da::PalwOperatorDaVerdictV1::Refuted) => {
                                error!(
                                    "[{PALW_PANEL}] claim {claim}: this drawn verifier's replay does NOT reproduce the claim (claimed execution {}) — \
                                     attesting NotReproducible (GC-DUTY)",
                                    job.execution_root
                                );
                                PalwCertDutyVerdictV1::NotReproducible
                            }
                            other => {
                                warn!("[{PALW_PANEL}] claim {claim}: Gate C replay came to no verdict ({other:?}); retried (GC-DUTY)");
                                PalwCertDutyVerdictV1::Unjudged { retry_at: current_daa.saturating_add(PALW_CERT_DUTY_RETRY_DAA_V1) }
                            }
                        };
                        duty.verdicts.insert(claim, verdict);
                    }
                }
            }
        }
        // A free-prompt row's SEAT-R pass (polled every tick until it decides), in the same slot.
        if duty.replaying.is_none() {
            if duty.fp_active.is_none() {
                duty.fp_active = duty.next_replay(current_daa, true).map(|d| d.claim_id);
            }
            let active = duty.fp_active.and_then(|claim| duty.owed.iter().find(|d| d.claim_id == claim).cloned());
            match active.as_ref().map(|d| (d, palw_cert_duty_route_v1(d))) {
                Some((d, PalwCertDutyRouteV1::FreePrompt { heavy })) if d.seat_duty.is_some() => {
                    let claim = d.claim_id;
                    let seat_duty = d.seat_duty.as_ref().expect("checked");
                    let seat_r_duty = PalwSeatRDutyV1 { deadline: d.deadline_daa, role: PalwSeatRRoleV1::FullSeat, heavy };
                    let pooled = fp_materials.get(&claim).map(|v| v.as_slice()).unwrap_or(&[]);
                    let (step, _) =
                        self.fp_seat_replay_pass_v1(session, seat_duty, seat_r_duty, current_daa, pooled, &mut duty.replays).await;
                    let verdict = match step {
                        PalwSeatReplayStepV1::Waiting => None,
                        PalwSeatReplayStepV1::Licensed => {
                            Some(PalwCertDutyVerdictV1::Reproduced { execution_root: seat_duty.execution_root })
                        }
                        PalwSeatReplayStepV1::Refuted => {
                            error!(
                                "[{PALW_PANEL}] claim {claim}: the free-prompt replay does NOT reproduce the claim — attesting NotReproducible (GC-DUTY)"
                            );
                            Some(PalwCertDutyVerdictV1::NotReproducible)
                        }
                        PalwSeatReplayStepV1::NoVerdict => {
                            warn!(
                                "[{PALW_PANEL}] claim {claim}: no free-prompt job material held here yet, or no verdict; retried (GC-DUTY)"
                            );
                            Some(PalwCertDutyVerdictV1::Unjudged { retry_at: current_daa.saturating_add(PALW_CERT_DUTY_RETRY_DAA_V1) })
                        }
                    };
                    if let Some(v) = verdict {
                        duty.verdicts.insert(claim, v);
                        duty.fp_active = None;
                    }
                }
                _ => duty.fp_active = None,
            }
        }
        // The next attempt replay (LegacyPanelAttempt, LegacyFusedLeaf, held-to-Final), in the duty's own slot.
        if duty.replaying.is_none()
            && duty.fp_active.is_none()
            && let Some(d) = duty.next_replay(current_daa, false).cloned()
            && let PalwCertDutyRouteV1::Attempt { heavy } = palw_cert_duty_route_v1(&d)
            && duty.replays.has_room(heavy)
        {
            self.cert_duty_start_replay_v1(session, duty, &d, current_daa, network_domain, heavy);
        }
        // Attest every row with a verdict due now.
        let me = palw_kernel_bond_id_v1(&bond_key);
        let mut out = Vec::new();
        for d in &duty.owed {
            let Some(verdict) = palw_cert_duty_attest_now_v1(d, duty.verdicts.get(&d.claim_id).copied(), current_daa) else {
                continue;
            };
            if duty.queued.get(&d.claim_id).is_some_and(|at| current_daa < at.saturating_add(PALW_CERT_DUTY_RESEND_DAA_V1)) {
                continue;
            }
            let (verdict, transcript) = match verdict {
                PalwCertDutyVerdictV1::Reproduced { execution_root } => {
                    (EligibilityVerdictV1::Reproduced, palw_cert_duty_transcript_v1(&d.certification_id, &me, &execution_root))
                }
                PalwCertDutyVerdictV1::NotReproducible => (EligibilityVerdictV1::NotReproducible, [0u8; 64]),
                _ => (EligibilityVerdictV1::Unavailable, [0u8; 64]),
            };
            let attestation = EligibilityAttestationV1 {
                version: ELIGIBILITY_ATTESTATION_VERSION_V1,
                network: network_domain.as_bytes(),
                exam: d.certification_id,
                verifier: me,
                verdict,
                transcript,
                claim_mismatches: vec![],
            };
            let bytes = K::AttestEligibilityExamV1 { attestation }.encode();
            let message = palw_kernel_route_message_v1(network_domain, &bond_key, &bytes);
            let Some(signature) = self.sign(message.as_byte_slice(), PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1) else {
                warn!("[{PALW_PANEL}] claim {}: Gate C attestation owed and no key to sign it (GC-DUTY)", d.claim_id);
                continue;
            };
            info!(
                "[{PALW_PANEL}] claim {}: this drawn verifier attests {verdict:?} for its Gate C certification ({} position(s), deadline DAA {}) (GC-DUTY)",
                d.claim_id, d.positions, d.deadline_daa
            );
            out.push((d.claim_id, PalwConsensusObjectV2::KernelRouteV1 { bytes, signer: bond_key, signature }));
        }
        for (claim, object) in out {
            court_outbox.retain_pending(|(c, round, responder, _)| !(mine(*round, *responder) && *c == claim));
            court_outbox.set_due((claim, PALW_CERT_DUTY_ROUND_V1, false), current_daa);
            court_outbox.push_pending((claim, PALW_CERT_DUTY_ROUND_V1, false, object));
            duty.queued.insert(claim, current_daa);
        }
    }

    /// **Start this node's Gate C replay of `d`** — lane B's replay of the claim's job, off the loop, in the duty's own slot, under the
    /// host ledger's reservation. A claim this host cannot replay now is unjudged (retried); a ledger refusal waits the next tick.
    fn cert_duty_start_replay_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        duty: &mut PalwCertDutyV1,
        d: &PalwCertificationDutyV1,
        current_daa: u64,
        network_domain: Hash64,
        heavy: bool,
    ) {
        let claim = d.claim_id;
        let unjudged = |duty: &mut PalwCertDutyV1, why: &str| {
            warn!("[{PALW_PANEL}] claim {claim}: drawn for Gate C and cannot replay it now — {why}; retried (GC-DUTY)");
            duty.verdicts
                .insert(claim, PalwCertDutyVerdictV1::Unjudged { retry_at: current_daa.saturating_add(PALW_CERT_DUTY_RETRY_DAA_V1) });
        };
        let (Some(job), Some(producer)) = (d.job.as_ref(), d.producer) else { return unjudged(duty, "its facts are gone") };
        let Some(artifact_root) = job.artifact_root else { return unjudged(duty, "no artifact root") };
        let mut backend = match self.resolve_backend(session, job.class_id, artifact_root) {
            Ok(backend) => backend,
            Err(why) => return unjudged(duty, &format!("its class does not resolve on this host ({why})")),
        };
        let Some((ctx, prompt)) =
            self.attempt_job_for_claim(session, backend.as_ref(), network_domain, job.accepted_block, job.class_id, &producer)
        else {
            return unjudged(duty, "its block's header is not held here");
        };
        let key = (claim, ctx.job_id);
        let routing_owner = match self.bind_native_routing_replay_v2(session, backend.as_mut(), claim, &ctx) {
            Ok(owner) => owner,
            Err(why) => return unjudged(duty, &format!("native routing context unavailable ({why})")),
        };
        let backends = self.backends();
        let role = super::palw_operator_da::PALW_OPERATOR_DA_REPLAY_ROLE_V1;
        let class_id = job.class_id;
        let reserved = match self.reserve_replay_at_widest_run_v1(role, backend.as_mut(), class_id, claim, |b| {
            backends.role_memory_need_for_backend_or_chain_v1(b, class_id, artifact_root, Some(&ctx), super::FULL_SEAT_V1, |id| {
                self.chain_carriage_v1(session, id)
            })
        }) {
            Ok((reserved, _)) => reserved,
            Err(why) => {
                crate::palw_backends::note_throttled_v1("panel-cert-duty-ledger", || {
                    format!("[{PALW_PANEL}] claim {claim}: the Gate C replay waits: {why} (GC-DUTY)")
                });
                return;
            }
        };
        info!(
            "[{PALW_PANEL}] claim {claim}: drawn for its Gate C certification ({} position(s), deadline DAA {}) — re-executing (GC-DUTY)",
            d.positions, d.deadline_daa
        );
        duty.replays.start(key, class_id, heavy, current_daa, Some(reserved), backend, move |b| b.execute_for_verdict(&ctx, &prompt));
        duty.replaying = Some((claim, key));
        duty.routing_owner = routing_owner;
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    fn duty(deadline_daa: u64) -> PalwCertificationDutyV1 {
        PalwCertificationDutyV1 {
            claim_id: Hash64::from_bytes([7; 64]),
            certification_id: [9; 64],
            native: false,
            drawn_daa: 0,
            deadline_daa,
            positions: 2,
            job: None,
            producer: None,
            seat_duty: None,
        }
    }

    #[test]
    fn an_unjudged_row_waits_until_the_margin_then_attests_unavailable() {
        let d = duty(1_000);
        assert_eq!(palw_cert_duty_attest_now_v1(&d, None, 100), None);
        assert_eq!(palw_cert_duty_attest_now_v1(&d, Some(PalwCertDutyVerdictV1::Unjudged { retry_at: 130 }), 100), None);
        assert_eq!(
            palw_cert_duty_attest_now_v1(&d, None, 1_000 - PALW_CERT_DUTY_MARGIN_DAA_V1),
            Some(PalwCertDutyVerdictV1::Unavailable)
        );
        let r = PalwCertDutyVerdictV1::Reproduced { execution_root: Hash64::from_bytes([1; 64]) };
        assert_eq!(palw_cert_duty_attest_now_v1(&d, Some(r), 100), Some(r), "a judgement is attested at once");
        assert_eq!(
            palw_cert_duty_attest_now_v1(&d, Some(PalwCertDutyVerdictV1::NotReproducible), 100),
            Some(PalwCertDutyVerdictV1::NotReproducible)
        );
    }

    #[test]
    fn the_transcript_is_never_zero_and_binds_the_verifier() {
        let root = Hash64::from_bytes([3; 64]);
        let a = palw_cert_duty_transcript_v1(&[9; 64], &[1; 64], &root);
        assert_ne!(a, [0u8; 64]);
        assert_ne!(a, palw_cert_duty_transcript_v1(&[9; 64], &[2; 64], &root));
    }

    fn job(free_prompt: bool, held_to_final: bool) -> kaspa_consensus_core::palw_operator_da_v1::PalwOperatorDaJobV1 {
        kaspa_consensus_core::palw_operator_da_v1::PalwOperatorDaJobV1 {
            accepted_block: Hash64::from_bytes([1; 64]),
            class_id: Hash64::from_bytes([2; 64]),
            artifact_root: Some(Hash64::from_bytes([3; 64])),
            execution_root: Hash64::from_bytes([4; 64]),
            trace_root: Hash64::from_bytes([5; 64]),
            output_root: Hash64::from_bytes([6; 64]),
            work_leaves: if free_prompt { 9 } else { 0 },
            free_prompt,
            held_to_final,
        }
    }

    fn seat_duty(
        j: &kaspa_consensus_core::palw_operator_da_v1::PalwOperatorDaJobV1,
    ) -> kaspa_consensus_core::palw_producer_v2::PalwSeatDutyV2 {
        let bond = PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
            kaspa_consensus_core::tx::TransactionId::from_u64_word(1),
            0,
        ));
        kaspa_consensus_core::palw_producer_v2::PalwSeatDutyV2 {
            model_panel_one: false,
            accepted_block: j.accepted_block,
            claim_id: Hash64::from_bytes([7; 64]),
            class_id: j.class_id,
            artifact_root: j.artifact_root.unwrap(),
            seat_bond: bond,
            executor_bond: bond,
            execution_root: j.execution_root,
            trace_root: j.trace_root,
            output_root: j.output_root,
            bound_daa: 0,
            receipt_deadline: 1_000,
            panel_anchor: Hash64::default(),
            seat_index: 0,
            panel_seat_count: 1,
            pwu: 1,
            quanta: 0,
            free_prompt: j.free_prompt,
            work_leaves: j.work_leaves,
            job_identity: Hash64::default(),
            tir_shard: None,
        }
    }

    fn owed(free_prompt: bool, held_to_final: bool) -> PalwCertificationDutyV1 {
        let j = job(free_prompt, held_to_final);
        PalwCertificationDutyV1 { seat_duty: Some(seat_duty(&j)), job: Some(j), ..duty(1_000) }
    }

    /// Every chain-claim type has a replay seam: LegacyPanelAttempt and LegacyFusedLeaf (an Attempt of a fused class, held to `Final`)
    /// lane B's attempt replay (heavy slot when held), every FP family the SEAT-R free-prompt pass; only 'V' and a gone class are
    /// Unavailable.
    #[test]
    fn every_chain_claim_type_routes_to_a_replay_seam() {
        use PalwCertDutyRouteV1 as R;
        assert_eq!(palw_cert_duty_route_v1(&owed(false, false)), R::Attempt { heavy: false }, "LegacyPanelAttempt");
        assert_eq!(palw_cert_duty_route_v1(&owed(false, true)), R::Attempt { heavy: true }, "LegacyFusedLeaf / held-to-Final");
        assert_eq!(palw_cert_duty_route_v1(&owed(true, false)), R::FreePrompt { heavy: false }, "FP families");
        assert_eq!(palw_cert_duty_route_v1(&owed(true, true)), R::FreePrompt { heavy: true }, "FP held to Final");
        assert_eq!(palw_cert_duty_route_v1(&PalwCertificationDutyV1 { native: true, ..owed(false, false) }), R::Native);
        assert_eq!(palw_cert_duty_route_v1(&PalwCertificationDutyV1 { seat_duty: None, ..owed(false, false) }), R::Gone);
        let mut gone = owed(false, false);
        gone.job.as_mut().unwrap().artifact_root = None;
        assert_eq!(palw_cert_duty_route_v1(&gone), R::Gone);
    }

    /// The verdict each seam reaches on a replay: the claim's own roots reproduce (→ Reproduced), a lie's do not (→ NotReproducible).
    /// Attempt / fused / held: lane B's `palw_operator_da_verdict_v1`; free prompt: SEAT-R's `palw_seat_replay_step_v1`, which
    /// `fp_seat_replay_pass_v1` applies.
    #[test]
    fn each_seam_reproduces_the_honest_roots_and_refutes_a_lie() {
        use super::super::palw_operator_da::{PalwOperatorDaVerdictV1 as V, palw_operator_da_verdict_v1};
        use kaspa_consensus_core::palw_backend::PalwReplayRootsV1;
        for held in [false, true] {
            let j = job(false, held);
            let honest = PalwReplayRootsV1 {
                execution_root: j.execution_root,
                trace_root: j.trace_root,
                work_leaves: None,
                output_root: Some(j.output_root),
            };
            let lie = PalwReplayRootsV1 { execution_root: Hash64::from_bytes([0xEE; 64]), ..honest.clone() };
            assert_eq!(palw_operator_da_verdict_v1(&Ok(honest), &j), Some(V::Reproduces));
            assert_eq!(palw_operator_da_verdict_v1(&Ok(lie), &j), Some(V::Refuted));
        }
        let j = job(true, false);
        let honest = PalwReplayRootsV1 {
            execution_root: j.execution_root,
            trace_root: j.trace_root,
            work_leaves: Some(j.work_leaves),
            output_root: Some(j.output_root),
        };
        let step =
            |r: PalwReplayRootsV1| palw_seat_replay_step_v1(&Ok(r), j.execution_root, j.trace_root, j.work_leaves, j.output_root);
        assert_eq!(step(honest.clone()), PalwSeatReplayStepV1::Licensed);
        assert_eq!(
            step(PalwReplayRootsV1 { output_root: Some(Hash64::from_bytes([0xEE; 64])), ..honest.clone() }),
            PalwSeatReplayStepV1::Refuted
        );
        assert_eq!(
            step(PalwReplayRootsV1 { work_leaves: Some(j.work_leaves + 1), ..honest }),
            PalwSeatReplayStepV1::Refuted,
            "a mispriced FP"
        );
    }

    /// **The drawn verifiers' verdicts through the chain's decision** (`palw_certification_verdict_v1`, the fold's rule): five drawn
    /// bonds, each attesting what its duty decided, the attestation shaped as `cert_duty_tick_v1` signs it. `now` is past the
    /// prosecution window and the attestation deadline.
    pub(in crate::palw_panel) fn decide_with_duty_verdicts_v1(
        verdict: PalwCertDutyVerdictV1,
    ) -> Option<kaspa_consensus_core::palw_security_certification_v1::PalwCertificationOutcomeV1> {
        use kaspa_consensus_core::palw_model_eligibility_v1::{PalwEligibilityAttestationRowV1, PalwEligibilityVerdictV1 as V};
        use kaspa_consensus_core::palw_security_certification_v1::*;
        let policy = PalwSecurityCertificationPolicyV1 { m: 5, k: 3, ..PalwSecurityCertificationPolicyV1::approved_v1(720) };
        let draws: Vec<[u8; 64]> = (1..=5u8).map(|n| [n; 64]).collect();
        let attestations = draws
            .iter()
            .map(|me| {
                let (verdict, transcript) = match verdict {
                    PalwCertDutyVerdictV1::Reproduced { execution_root } => {
                        (V::Reproduced, palw_cert_duty_transcript_v1(&[9; 64], me, &execution_root))
                    }
                    PalwCertDutyVerdictV1::NotReproducible => (V::NotReproducible, [0; 64]),
                    _ => (V::Unavailable, [0; 64]),
                };
                PalwEligibilityAttestationRowV1 { verifier: *me, verdict, transcript, attested_daa: 20 }
            })
            .collect();
        let row = PalwClaimCertificationRowV1 {
            claim: [7; 64],
            class_id: [8; 64],
            producer_bond: [0xAA; 64],
            producer_principal: [0xAB; 64],
            attempt: true,
            final_daa: 5,
            committed_daa: 5,
            committed_span: 0,
            anchor: Some((1, [0; 64])),
            draws,
            drawn_daa: 10,
            deadline_daa: 10 + policy.attest_window_daa,
            attestations,
            held_reward: 0,
            payout_payload: [0; 64],
            outcome: None,
        };
        let now = (5 + policy.prosecution_window_daa).max(row.deadline_daa) + 1;
        palw_certification_verdict_v1(
            &policy,
            &row,
            PalwCertificationClaimFactsV1 { still_final: true, accusation_pending: false },
            now,
        )
    }

    /// **FP family (FpSelection; the same seam serves FpConstraint/FpPrefix/FpTokenizer) on a REAL backend** — the shipped floor
    /// class (`PALW-BASE-0/rc`). The producer runs a user job; the drawn bond's own backend replays the job decoded from its material
    /// (`palw_fp_job_material_decode_v1`, what `fp_seat_replay_pass_v1` reads) with `execute_free_prompt`, mapped exactly as
    /// `BoundFpReplayV2::execute` does, and SEAT-R's rule decides: honest → Reproduced → Certified; a producer that committed an
    /// injected fault → Refuted → NotReproducible → not certified.
    #[test]
    fn gc_duty_real_backend_fp_selection_reproduces_and_certifies_and_a_lie_is_refuted() {
        use kaspa_consensus_core::palw_backend::{PalwExecutionBackendV1, PalwReplayRootsV1};
        use kaspa_consensus_core::palw_freeprompt_v3::*;
        use kaspa_consensus_core::palw_security_certification_v1::PalwCertificationOutcomeV1 as O;
        fn floor() -> misaka_palw_base0::backend::Base0Backend {
            use misaka_palw_base0::classes::{canonical_class_by_model_id_v1, resolve_class_v1};
            let court = kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2::new(
                kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES,
                4,
                2,
            )
            .expect("the shipped court");
            let entry = canonical_class_by_model_id_v1(&court, "PALW-BASE-0/rc").expect("the floor is registered");
            let root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().expect("the floor's pinned root");
            misaka_palw_base0::backend::Base0Backend::new(resolve_class_v1(&court, entry.class_id(), root, &[]).expect("resolves"))
        }
        let producer = floor();
        let prompt: Vec<u32> = vec![7, 11, 13, 17];
        let ids: Vec<usize> = prompt.iter().map(|t| *t as usize).collect();
        let job = PalwFreePromptJobV3 {
            version: PALW_FP_V3_VERSION,
            network_domain: Hash64::from_u64_word(0xD0),
            class_id: producer.profile().shape_profile_id(),
            executor_bond: kaspa_consensus_core::tx::TransactionOutpoint::new(
                kaspa_consensus_core::tx::TransactionId::from_u64_word(0xB0),
                0,
            ),
            executor_pubkey: vec![0x11; 32],
            operator_id: Hash64::from_u64_word(0x0B),
            anchor_block: Hash64::from_u64_word(0xA0),
            anchor_daa: 4242,
            job_nonce: [0x5A; 32],
            tokenizer_id: Hash64::default(),
            prompt_token_ids_hash: kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(&prompt),
            prompt_tokens: prompt.len() as u32,
            decode_token_limit: 6,
            max_context_tokens: producer.profile().n_ctx,
            privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
            prompt_mode: PALW_FP_PROMPT_MODE_USER,
            sampling_seed: [0; 32],
            temperature_q: 0,
            decode: None,
            tail: None,
        };
        let roots = |run: &kaspa_consensus_core::palw_backend::PalwFpRunV1| PalwReplayRootsV1 {
            execution_root: run.outcome.execution_root,
            trace_root: run.facts.full_logits_trace_root,
            work_leaves: Some(run.facts.step_leaf_count),
            output_root: Some(run.outcome.output_root),
        };
        let material = palw_fp_job_material_decode_v1(
            &palw_fp_material_encode_v1(&job, &prompt),
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        )
        .expect("the material decodes");
        assert_eq!(material.job, job);
        // The drawn verifier's own backend and replay.
        let verifier = floor();
        let replay = roots(&verifier.execute_free_prompt(&material.job, &ids).expect("the verifier's replay"));
        let judge = |claimed: &PalwReplayRootsV1| match palw_seat_replay_step_v1(
            &Ok(replay.clone()),
            claimed.execution_root,
            claimed.trace_root,
            claimed.work_leaves.unwrap(),
            claimed.output_root.unwrap(),
        ) {
            PalwSeatReplayStepV1::Licensed => PalwCertDutyVerdictV1::Reproduced { execution_root: claimed.execution_root },
            PalwSeatReplayStepV1::Refuted => PalwCertDutyVerdictV1::NotReproducible,
            other => panic!("no verdict: {other:?}"),
        };
        let honest = roots(&producer.execute_free_prompt(&job, &ids).expect("the honest run"));
        let v = judge(&honest);
        assert!(matches!(v, PalwCertDutyVerdictV1::Reproduced { .. }), "honest FP → Reproduced");
        assert_eq!(decide_with_duty_verdicts_v1(v), Some(O::Certified));
        let lie = roots(&producer.execute_free_prompt_with_injected_fault(&job, &ids, 0).expect("the lie commits"));
        assert_ne!(lie.execution_root, honest.execution_root, "the injected fault moves the committed roots");
        let v = judge(&lie);
        assert_eq!(v, PalwCertDutyVerdictV1::NotReproducible, "lying FP → Refuted");
        assert_eq!(decide_with_duty_verdicts_v1(v), Some(O::NotCertified));
    }

    #[test]
    fn the_duty_owns_one_slot_of_its_own() {
        let d = PalwCertDutyV1::new();
        assert!(d.replays.has_room(false));
    }
}
