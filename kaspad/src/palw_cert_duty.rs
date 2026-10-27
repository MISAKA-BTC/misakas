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
use kaspa_consensus_core::palw_kernel_route_v1::{PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1, palw_kernel_bond_id_v1, palw_kernel_route_message_v1};
use kaspa_consensus_core::palw_security_certification_v1::PalwCertificationDutyV1;
use misaka_palw_kernel::model_eligibility_v1::{ELIGIBILITY_ATTESTATION_VERSION_V1, EligibilityAttestationV1, EligibilityVerdictV1};
use misaka_palw_kernel::route::KernelRouteObjectV1 as K;
use kaspa_core::error;

/// The court-queue round the duty's attestations ride under (the kernel roles hold `u32::MAX − 40..=43`).
pub(super) const PALW_CERT_DUTY_ROUND_V1: u32 = u32::MAX - 44;
/// DAA after which an unjudged replay is tried again.
pub(super) const PALW_CERT_DUTY_RETRY_DAA_V1: u64 = 30;
/// DAA before the deadline at which an unjudged row is attested `Unavailable` instead of waiting further (room for carriage).
pub(super) const PALW_CERT_DUTY_MARGIN_DAA_V1: u64 = 60;
/// DAA after which a queued attestation not yet on chain is queued again.
const PALW_CERT_DUTY_RESEND_DAA_V1: u64 = 6;

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
    fn next_replay(&self, now_daa: u64) -> Option<&PalwCertificationDutyV1> {
        self.owed.iter().find(|d| {
            let open = match self.verdicts.get(&d.claim_id) {
                None => true,
                Some(PalwCertDutyVerdictV1::Unjudged { retry_at }) => now_daa >= *retry_at,
                Some(_) => false,
            };
            open && !d.native
                && d.job.as_ref().is_some_and(super::palw_operator_da::palw_operator_da_replayable_v1)
                && now_daa.saturating_add(PALW_CERT_DUTY_MARGIN_DAA_V1) < d.deadline_daa
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
        // Structurally uncheckable here: attest Unavailable at once (an owning39 mining claim, a claim lane B does not replay).
        for d in &duty.owed {
            if !duty.verdicts.contains_key(&d.claim_id)
                && (d.native || !d.job.as_ref().is_some_and(super::palw_operator_da::palw_operator_da_replayable_v1))
            {
                warn!(
                    "[{PALW_PANEL}] claim {}: drawn for its Gate C certification ({} position(s)) and this node cannot re-execute it ({}) — \
                     attesting Unavailable (GC-DUTY)",
                    d.claim_id,
                    d.positions,
                    if d.native { "owning39 mining claim" } else { "lane B does not replay it" }
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
        // The next replay, in the duty's own slot.
        if duty.replaying.is_none()
            && duty.replays.has_room(false)
            && let Some(d) = duty.next_replay(current_daa).cloned()
        {
            self.cert_duty_start_replay_v1(session, duty, &d, current_daa, network_domain);
        }
        // Attest every row with a verdict due now.
        let me = palw_kernel_bond_id_v1(&bond_key);
        let mut out = Vec::new();
        for d in &duty.owed {
            let Some(verdict) = palw_cert_duty_attest_now_v1(d, duty.verdicts.get(&d.claim_id).copied(), current_daa) else { continue };
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
        duty.replays.start(key, class_id, false, current_daa, Some(reserved), backend, move |b| b.execute_for_verdict(&ctx, &prompt));
        duty.replaying = Some((claim, key));
        duty.routing_owner = routing_owner;
    }
}

#[cfg(test)]
mod tests {
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
        }
    }

    #[test]
    fn an_unjudged_row_waits_until_the_margin_then_attests_unavailable() {
        let d = duty(1_000);
        assert_eq!(palw_cert_duty_attest_now_v1(&d, None, 100), None);
        assert_eq!(palw_cert_duty_attest_now_v1(&d, Some(PalwCertDutyVerdictV1::Unjudged { retry_at: 130 }), 100), None);
        assert_eq!(palw_cert_duty_attest_now_v1(&d, None, 1_000 - PALW_CERT_DUTY_MARGIN_DAA_V1), Some(PalwCertDutyVerdictV1::Unavailable));
        let r = PalwCertDutyVerdictV1::Reproduced { execution_root: Hash64::from_bytes([1; 64]) };
        assert_eq!(palw_cert_duty_attest_now_v1(&d, Some(r), 100), Some(r), "a judgement is attested at once");
        assert_eq!(palw_cert_duty_attest_now_v1(&d, Some(PalwCertDutyVerdictV1::NotReproducible), 100), Some(PalwCertDutyVerdictV1::NotReproducible));
    }

    #[test]
    fn the_transcript_is_never_zero_and_binds_the_verifier() {
        let root = Hash64::from_bytes([3; 64]);
        let a = palw_cert_duty_transcript_v1(&[9; 64], &[1; 64], &root);
        assert_ne!(a, [0u8; 64]);
        assert_ne!(a, palw_cert_duty_transcript_v1(&[9; 64], &[2; 64], &root));
    }

    #[test]
    fn the_duty_owns_one_slot_of_its_own() {
        let d = PalwCertDutyV1::new();
        assert!(d.replays.has_room(false));
    }
}
