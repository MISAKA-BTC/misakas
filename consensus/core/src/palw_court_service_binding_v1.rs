//! **G14 lane court-f02 (2026-10-22): pure / rooted pieces of RFC-0026 F02, RFC-0028 and RFC-0031** — written, not compiled.
//!
//! * [`court_service_binding_candidate_v1`] (RFC-0028): the service binding an ORIGINAL admission installs — the claim's canonical
//!   service packets reserved all-or-nothing in the rooted [`CourtServiceStateV2`] book. Pure: the caller writes the returned row
//!   through the native staged-court writer (which re-runs `check_context` and journals it, so reorg undo is the rooted delta).
//! * [`attribute_service_default_v1`] (RFC-0026 §4 / ADR-0192 A): the adopted attribution — a non-response default only when the
//!   accepting chain RECORDED every reserved unit of the claim as supplied (`ServiceAssessment::OpportunityProvided`), the explicit
//!   inclusion premise is in force, and each named actor's own native duty clock contained one of those supplied units. Missing
//!   supply, or an actor with no supplied unit inside its window, is a NO-SLASH hold (recover / keep collateral), never a default.
//! * [`classify_shared_notice_v1`] (RFC-0031 / ADR-0198 A): the initial notice is `a <= d <= a + 3,120`; follow-up requests bind to
//!   that notice and stay strictly before the original hard `a + 10,000`. Neither resets `a`.
//!
//! The capacity profile (budget, packet cost, packets per liable signer, per-packet service span) is a USER DECISION and is left
//! typed-missing: [`PALW_COURT_SERVICE_PROFILE_V1`] is `None`, so no production admission installs a binding and the completeness
//! gate keeps refusing `ServiceOpportunityIncomplete` until a reviewed profile is pinned.
use crate::{
    Hash64,
    palw_court_duty_clock_v1::CourtDutyClockV1,
    palw_court_policy_v1::{CourtPolicyError, DISPUTE_WINDOW_DAA, shared_initial_notice_daa_in_window_v1},
    palw_court_service_state_v2::{CourtServiceStateV2, ServiceClaimBinding, court_service_profile_id_v2},
    palw_court_service_v1::{ServiceAssessment, ServiceCost, ServicePacket, ServiceRefusal, ServiceReservations},
    palw_state_v2::PalwBondKeyV2,
};
use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::BTreeSet;

/// The deterministic service capacity profile a binding is reserved under. Every field is a policy value (RFC-0026 §4.1 item 1–2):
/// none is measured here and none may be zero-filled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CourtServiceProfileV1 {
    pub version: u16,
    /// Per-accepting-DAA reserved court capacity (disjoint from ordinary transaction capacity).
    pub budget: ServiceCost,
    /// Cost of one atomic service packet (`controls` must be 1).
    pub packet_cost: ServiceCost,
    /// Canonical packets per liable signer (one per response turn the profile allows).
    pub packets_per_signer: u64,
    /// Width of each packet's service interval in DAA.
    pub service_span_daa: u64,
}

impl CourtServiceProfileV1 {
    /// The source profile id the rooted book records (`CourtServiceStateV2::source_profile_id`).
    pub fn id(&self) -> Hash64 {
        keyed(b"misaka-palw/court-service-profile/v1", &borsh::to_vec(self).expect("profile serialization"))
    }
}

/// **V1 — USER-APPROVED 2026-10-26 (estimate used as is; 「目安の値をそのまま使う」承認). Review condition: replace by measurement.** Basis: lane VAL [`crate::palw_g14_values_v1::PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1`].
/// Read only past an armed court fence.
pub const PALW_COURT_SERVICE_PROFILE_V1: Option<CourtServiceProfileV1> =
    Some(crate::palw_g14_values_v1::PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1);

/// **RFC-0026 §4.1 item 4 / ADR-0192 A: the explicit delivery / inclusion premise** (lane F02, 2026-10-26). A non-response default
/// is attributed only UNDER this premise: a correctly formed response an honest actor broadcasts at a supplied reserved unit `u` is
/// assumed included in the accepting chain no later than `u + inclusion_delay_daa`. A supplied unit therefore counts as the
/// actor's opportunity only when `u + inclusion_delay_daa <= response_deadline_daa` (and inside the duty's window). The premise is
/// a stated assumption of the attribution, never measured here and never evidence of personal delivery; when it is not adopted
/// (typed-missing) every default is undecided (`Err`), which is a no-slash outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CourtServiceInclusionPremiseV1 {
    pub version: u16,
    /// Assumed worst-case DAA between an honest broadcast at a supplied unit and its inclusion.
    pub inclusion_delay_daa: u64,
}

impl CourtServiceInclusionPremiseV1 {
    pub fn id(&self) -> Hash64 {
        keyed(b"misaka-palw/court-service-inclusion-premise/v1", &borsh::to_vec(self).expect("premise serialization"))
    }
    /// **G14 S3c''' (kernel route):** a reserved unit `unit` of a packet whose service interval is `[release_daa, service_before)` is
    /// the producer's opportunity to answer a demand due at `response_deadline_daa` under this premise — the same F02 rule as
    /// [`Self::admits_supplied_unit_v1`]: inside the duty window and `unit + inclusion_delay_daa <= response_deadline_daa`.
    pub fn admits_reserved_unit_v1(&self, release_daa: u64, service_before: u64, unit: u64, response_deadline_daa: u64) -> bool {
        self.version == 1
            && release_daa <= unit
            && unit < service_before
            && unit.checked_add(self.inclusion_delay_daa).is_some_and(|included| included <= response_deadline_daa)
    }

    /// A supplied unit at `daa` is the actor's opportunity for the duty `clock` under this premise.
    pub fn admits_supplied_unit_v1(&self, clock: &CourtDutyClockV1, daa: u64) -> bool {
        self.version == 1
            && clock.stage_started_before_unit(daa)
            && daa.checked_add(self.inclusion_delay_daa).is_some_and(|included| included <= clock.response_deadline_daa)
    }
}

/// **Inclusion delay — USER-APPROVED 2026-10-26 (estimate used as is; 「目安の値をそのまま使う」承認). Review condition: replace by measurement.** Basis: lane VAL
/// [`crate::palw_g14_values_v1::PALW_G14_COURT_SERVICE_INCLUSION_DELAY_DAA_ESTIMATE_V1`] (4 DAA). The processor copies this into
/// `PalwTransitionExtrasV1::court_service_inclusion_premise_v1`; fixtures set their own explicitly named test premise.
pub const PALW_COURT_SERVICE_INCLUSION_PREMISE_V1: Option<CourtServiceInclusionPremiseV1> = Some(CourtServiceInclusionPremiseV1 {
    version: 1,
    inclusion_delay_daa: crate::palw_g14_values_v1::PALW_G14_COURT_SERVICE_INCLUSION_DELAY_DAA_ESTIMATE_V1,
});

/// RFC-0026 F02 is operational only once every piece is wired AND its profile is pinned. The pieces (attribution, binding at
/// ordinary admission, shared lifecycle dispatch with bottom effects, native notice intake) are present in this lane; the profile
/// is not, so this is `false` on every build until a reviewed change pins [`PALW_COURT_SERVICE_PROFILE_V1`].
pub const fn palw_court_service_opportunity_complete_v1() -> bool {
    PALW_COURT_SERVICE_PROFILE_V1.is_some()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CourtServiceBindingRefusalV1 {
    ProfileInvalid,
    NoLiableSigner,
    Overflow,
    /// A packet interval would reach the original hard deadline.
    ProfileExceedsHard,
    /// The existing rooted book was reserved under another profile or capacity.
    ProfileMismatch,
    AlreadyBound,
    Reservation(ServiceRefusal),
    Inconsistent,
}

fn keyed(key: &[u8], bytes: &[u8]) -> Hash64 {
    Hash64::from_bytes(
        blake2b_simd::Params::new().hash_length(64).key(key).hash(bytes).as_bytes().try_into().expect("64-byte digest"),
    )
}

/// Canonical packet identity: binds claim, allocation, source, the liable signer and the packet's turn ordinal.
pub fn court_service_packet_id_v1(claim: Hash64, allocation: Hash64, source: Hash64, signer: &PalwBondKeyV2, k: u64) -> [u8; 64] {
    let bytes = borsh::to_vec(&(claim, allocation, source, signer, k)).expect("packet id serialization");
    let digest = keyed(b"misaka-palw/court-service-packet/v1", &bytes);
    *digest.as_byte_slice()
}

/// The canonical packet set of one original: for each liable signer (canonical order) and each turn `k`, one packet released at
/// `a + 1 + k·span` and serviceable before `min(a + 1 + (k+1)·span, a + 10,000)`. Refused (never truncated) when an interval would
/// be empty or reach the hard deadline.
pub fn court_service_packets_v1(
    claim: Hash64,
    allocation: Hash64,
    source: Hash64,
    accepted_daa: u64,
    signers: &BTreeSet<PalwBondKeyV2>,
    profile: &CourtServiceProfileV1,
) -> Result<Vec<ServicePacket>, CourtServiceBindingRefusalV1> {
    use CourtServiceBindingRefusalV1 as R;
    if profile.version != 1 || profile.packets_per_signer == 0 || profile.service_span_daa == 0 || profile.packet_cost.controls != 1 {
        return Err(R::ProfileInvalid);
    }
    if signers.is_empty() {
        return Err(R::NoLiableSigner);
    }
    let hard = accepted_daa.checked_add(DISPUTE_WINDOW_DAA).ok_or(R::Overflow)?;
    let first = accepted_daa.checked_add(1).ok_or(R::Overflow)?;
    let mut packets = Vec::new();
    for signer in signers {
        for k in 0..profile.packets_per_signer {
            let release = k.checked_mul(profile.service_span_daa).and_then(|o| first.checked_add(o)).ok_or(R::Overflow)?;
            let before = release.checked_add(profile.service_span_daa).ok_or(R::Overflow)?;
            if before > hard {
                return Err(R::ProfileExceedsHard);
            }
            packets.push(ServicePacket {
                id: court_service_packet_id_v1(claim, allocation, source, signer, k),
                release_daa: release,
                service_before: before,
                cost: profile.packet_cost,
            });
        }
    }
    Ok(packets)
}

/// **RFC-0028: the binding an original admission installs.** Pure: returns the next rooted service row (a fresh book under
/// `profile`, or the existing one extended), with the claim's packets reserved all-or-nothing and the whole row re-checked.
pub fn court_service_binding_candidate_v1(
    existing: Option<&CourtServiceStateV2>,
    claim: Hash64,
    allocation: Hash64,
    source: Hash64,
    accepted_daa: u64,
    signers: &BTreeSet<PalwBondKeyV2>,
    profile: &CourtServiceProfileV1,
) -> Result<CourtServiceStateV2, CourtServiceBindingRefusalV1> {
    use CourtServiceBindingRefusalV1 as R;
    let packets = court_service_packets_v1(claim, allocation, source, accepted_daa, signers, profile)?;
    let hard = accepted_daa.checked_add(DISPUTE_WINDOW_DAA).ok_or(R::Overflow)?;
    let mut next = match existing {
        Some(old) => {
            if old.source_profile_id != profile.id() || old.book.budget != profile.budget {
                return Err(R::ProfileMismatch);
            }
            if old.claims.contains_key(&claim) {
                return Err(R::AlreadyBound);
            }
            old.clone()
        }
        None => CourtServiceStateV2 {
            version: 2,
            profile_id: court_service_profile_id_v2(profile.id()),
            source_profile_id: profile.id(),
            book: ServiceReservations::new(profile.budget).map_err(R::Reservation)?,
            claims: Default::default(),
        },
    };
    next.book.reserve(&packets, hard).map_err(R::Reservation)?;
    next.claims.insert(claim, ServiceClaimBinding {
        claim,
        allocation_id: allocation,
        source_id: source,
        original_accepted_daa: accepted_daa,
        hard_deadline_daa: hard,
        packet_ids: packets.iter().map(|p| p.id).collect(),
    });
    next.check_live_v1().map_err(|_| R::Inconsistent)?;
    Ok(next)
}

/// The outcome of the adopted F02 attribution. Neither variant is itself a charge: `PartyDefault` lets the existing native
/// default path (S4 locks, exit holds, `staged_selected_default_charge_v1`) proceed; `NoSlashHold` refuses the default and keeps
/// the claim, collateral and evidence held for recovery (`CourtServiceStateV2::repair_missing_supply`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CourtServiceAttributionV1 {
    PartyDefault,
    NoSlashHold(&'static str),
}

/// **RFC-0026 §4 / ADR-0192 A: attribution from RECORDED reserved supply.** `Err` = not decidable yet (pending, unknown
/// obligation, inconsistent book, premise not adopted); never a default.
///
/// **audit B-03 (fix-b): bound to the duty being defaulted.** `clocks` are the clocks of the duty being defaulted (callers pass
/// exactly that duty's clock; a caller that hands over an actor's history is read the same way): for each actor only its LATEST clock
/// per acquisition (highest `(turn, first_eligible_daa)`) is the defaulted duty — an earlier turn's clock is superseded history and
/// never supplies the opportunity. That duty counts as opportunity-supplied only through the actor's OWN canonical packet for that
/// turn (`court_service_packet_id_v1(claim, allocation, source, actor, turn)`): reserved in the binding, `OpportunityProvided`, and
/// assigned to a DAA inside that clock's eligible window. Another signer's packet, another turn's packet, or a supplied unit outside
/// the window is a no-slash hold.
pub fn attribute_service_default_v1(
    service: &CourtServiceStateV2,
    claim: Hash64,
    clocks: &[&CourtDutyClockV1],
    unpaid: &BTreeSet<PalwBondKeyV2>,
    premise: Option<&CourtServiceInclusionPremiseV1>,
) -> Result<CourtServiceAttributionV1, &'static str> {
    let binding = service.claims.get(&claim).ok_or("court service binding is missing")?;
    if service.check_live_v1().is_err() {
        return Err("court service state is inconsistent");
    }
    let mut missing = false;
    for id in &binding.packet_ids {
        match service.book.assess(id) {
            ServiceAssessment::UnknownObligation => return Err("court service obligation is missing"),
            ServiceAssessment::Pending => return Err("court service opportunity is still pending"),
            ServiceAssessment::MissingSupply => missing = true,
            ServiceAssessment::OpportunityProvided => {
                service.book.assignments.get(id).ok_or("court service assignment is missing")?;
            }
        }
    }
    if missing {
        return Ok(CourtServiceAttributionV1::NoSlashHold(
            "court service supply is missing for a reserved unit; no party default, hold and recover",
        ));
    }
    let Some(premise) = premise else {
        return Err("court service inclusion assumptions are not adopted; no party default");
    };
    if premise.version != 1 {
        return Err("court service inclusion premise version is unknown; no party default");
    }
    if unpaid.is_empty() {
        return Err("court service default names no responsible actor");
    }
    for actor in unpaid {
        let mine: Vec<&CourtDutyClockV1> = clocks.iter().copied().filter(|c| c.claim == claim && c.answerer == *actor).collect();
        // The defaulted duty: the actor's latest clock of each acquisition (an earlier turn/stage is superseded history).
        let latest = mine.iter().copied().filter(|c| {
            !mine.iter().any(|o| {
                o.acquisition == c.acquisition && (o.turn, o.first_eligible_daa) > (c.turn, c.first_eligible_daa)
            })
        });
        let had = latest.into_iter().any(|c| {
            let own = court_service_packet_id_v1(claim, binding.allocation_id, binding.source_id, actor, c.turn);
            binding.packet_ids.contains(&own)
                && service.book.assess(&own) == ServiceAssessment::OpportunityProvided
                && service.book.assigned_daa(&own).is_some_and(|daa| premise.admits_supplied_unit_v1(c, daa))
        });
        if !had {
            return Ok(CourtServiceAttributionV1::NoSlashHold(
                "responsible actor had no supplied reserved unit of its own for the defaulted turn inside that duty's window; no party default",
            ));
        }
    }
    Ok(CourtServiceAttributionV1::PartyDefault)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharedNoticeV1 {
    /// This request opens the claim's shared court (its first native request row).
    Initial,
    /// A later adaptive request, bound to the earlier initial notice.
    FollowUp,
}

/// **RFC-0031 A at intake.** `prior` is the DAA of the claim's existing initial notice (the earliest native request row), if any.
pub fn classify_shared_notice_v1(original_acceptance: u64, notice_daa: u64, prior: Option<u64>) -> Result<SharedNoticeV1, &'static str> {
    let hard = original_acceptance.checked_add(DISPUTE_WINDOW_DAA).ok_or("shared notice original hard overflows")?;
    let window = |d: u64| match shared_initial_notice_daa_in_window_v1(original_acceptance, d) {
        Ok(ok) => Ok(ok),
        Err(CourtPolicyError::Overflow) | Err(CourtPolicyError::DependencyTooShort) => Err("shared notice boundary overflows"),
    };
    match prior {
        None => {
            if window(notice_daa)? { Ok(SharedNoticeV1::Initial) } else { Err("initial shared notice outside a..=a+3,120") }
        }
        Some(first) => {
            if !window(first)? {
                return Err("recorded initial shared notice lies outside its window");
            }
            if notice_daa < first {
                return Err("follow-up shared request precedes its initial notice");
            }
            if notice_daa >= hard {
                return Err("follow-up shared request reaches the original hard deadline");
            }
            Ok(SharedNoticeV1::FollowUp)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1 as Key;
    use crate::tx::TransactionOutpoint;

    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn bond(n: u32) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(h(0xB0), n))
    }
    /// TEST-ONLY premise (not a production value): zero inclusion delay.
    const TEST_ONLY_PREMISE: CourtServiceInclusionPremiseV1 = CourtServiceInclusionPremiseV1 { version: 1, inclusion_delay_daa: 0 };
    fn profile() -> CourtServiceProfileV1 {
        let unit = ServiceCost { bytes: 10, work: 10, io: 1, controls: 1 };
        CourtServiceProfileV1 {
            version: 1,
            budget: ServiceCost { bytes: 100, work: 100, io: 10, controls: 10 },
            packet_cost: unit,
            packets_per_signer: 2,
            service_span_daa: 50,
        }
    }
    fn bound(a: u64) -> (CourtServiceStateV2, BTreeSet<PalwBondKeyV2>) {
        let signers: BTreeSet<_> = [bond(1), bond(2)].into_iter().collect();
        (court_service_binding_candidate_v1(None, h(1), h(2), h(3), a, &signers, &profile()).unwrap(), signers)
    }
    fn clock(actor: PalwBondKeyV2, from: u64, until: u64) -> CourtDutyClockV1 {
        CourtDutyClockV1 {
            version: 1,
            profile_id: h(9),
            duty_id: h(10),
            claim: h(1),
            allocation_id: h(2),
            source_id: h(3),
            acquisition: Key::TirAcquisition(h(1), actor, h(11)),
            answerer: actor,
            demand_id: h(12),
            turn: 0,
            predecessor_block: h(13),
            predecessor_daa: from - 1,
            first_eligible_daa: from,
            response_deadline_daa: until,
            original_hard_daa: 1_000 + DISPUTE_WINDOW_DAA,
            material_until_daa: 1_000 + DISPUTE_WINDOW_DAA,
        }
    }
    fn observe_all(s: &mut CourtServiceStateV2, from: u64, to: u64, skip: Option<u64>) {
        for daa in from..=to {
            if Some(daa) == skip {
                continue;
            }
            s.book.observe_block(daa, [daa as u8; 64], ServiceCost::default()).unwrap();
        }
    }

    #[test]
    fn court_f02_binding_reserves_canonical_packets_all_or_nothing() {
        let (s, signers) = bound(1_000);
        assert_eq!(s.claims[&h(1)].packet_ids.len(), 4);
        assert_eq!(s.claims[&h(1)].hard_deadline_daa, 1_000 + DISPUTE_WINDOW_DAA);
        s.check().unwrap();
        // Same claim twice is refused; another profile cannot extend the book.
        assert_eq!(
            court_service_binding_candidate_v1(Some(&s), h(1), h(2), h(3), 1_000, &signers, &profile()),
            Err(CourtServiceBindingRefusalV1::AlreadyBound)
        );
        let mut other = profile();
        other.service_span_daa = 51;
        assert_eq!(
            court_service_binding_candidate_v1(Some(&s), h(4), h(2), h(3), 1_000, &signers, &other),
            Err(CourtServiceBindingRefusalV1::ProfileMismatch)
        );
        // A second claim extends the same rooted book.
        let next = court_service_binding_candidate_v1(Some(&s), h(4), h(5), h(6), 1_001, &signers, &profile()).unwrap();
        assert_eq!(next.claims.len(), 2);
        next.check_retains_unresolved(&s).unwrap();
    }

    #[test]
    fn court_f02_binding_refuses_typed_missing_or_overlong_profile() {
        let signers: BTreeSet<_> = [bond(1)].into_iter().collect();
        let mut p = profile();
        p.packets_per_signer = 0;
        assert_eq!(court_service_packets_v1(h(1), h(2), h(3), 5, &signers, &p), Err(CourtServiceBindingRefusalV1::ProfileInvalid));
        let mut p = profile();
        p.service_span_daa = DISPUTE_WINDOW_DAA;
        assert_eq!(court_service_packets_v1(h(1), h(2), h(3), 5, &signers, &p), Err(CourtServiceBindingRefusalV1::ProfileExceedsHard));
        assert_eq!(
            court_service_packets_v1(h(1), h(2), h(3), 5, &BTreeSet::new(), &profile()),
            Err(CourtServiceBindingRefusalV1::NoLiableSigner)
        );
        assert_eq!(court_service_packets_v1(h(1), h(2), h(3), u64::MAX - 10, &signers, &profile()), Err(CourtServiceBindingRefusalV1::Overflow));
        assert!(palw_court_service_opportunity_complete_v1(), "V1 pinned from the user-approved estimate (2026-10-26)");
    }

    #[test]
    fn court_f02_supplied_opportunity_inside_actor_window_defaults() {
        let (mut s, _) = bound(1_000);
        observe_all(&mut s, 1_001, 1_101, None);
        let unpaid: BTreeSet<_> = [bond(1)].into_iter().collect();
        let c = clock(bond(1), 1_001, 1_200);
        assert_eq!(attribute_service_default_v1(&s, h(1), &[&c], &unpaid, Some(&TEST_ONLY_PREMISE)), Ok(CourtServiceAttributionV1::PartyDefault));
        // Without the explicit inclusion premise: still undecided, never a default.
        assert!(attribute_service_default_v1(&s, h(1), &[&c], &unpaid, None).is_err());
    }

    #[test]
    fn court_f02_missing_supply_is_no_slash_hold() {
        let (mut s, _) = bound(1_000);
        let skipped = *s.book.assignments.values().next().unwrap();
        observe_all(&mut s, 1_001, 1_101, Some(skipped));
        let unpaid: BTreeSet<_> = [bond(1)].into_iter().collect();
        let c = clock(bond(1), 1_001, 1_200);
        assert!(matches!(
            attribute_service_default_v1(&s, h(1), &[&c], &unpaid, Some(&TEST_ONLY_PREMISE)),
            Ok(CourtServiceAttributionV1::NoSlashHold(_))
        ));
    }

    #[test]
    fn court_f02_actor_without_supplied_unit_in_window_is_no_slash_hold() {
        let (mut s, _) = bound(1_000);
        observe_all(&mut s, 1_001, 1_101, None);
        let unpaid: BTreeSet<_> = [bond(2)].into_iter().collect();
        // bond(2)'s clock opens after every supplied unit; bond(1)'s clock does not name bond(2).
        let late = clock(bond(2), 1_150, 1_200);
        let other = clock(bond(1), 1_001, 1_200);
        assert!(matches!(
            attribute_service_default_v1(&s, h(1), &[&late, &other], &unpaid, Some(&TEST_ONLY_PREMISE)),
            Ok(CourtServiceAttributionV1::NoSlashHold(_))
        ));
    }

    /// **audit(b) B-03 reproducer** (round 1, not a fix): the attribution is not bound to the duty being defaulted. The actor's
    /// CURRENT turn (turn 1, window opening after every supplied unit) had no supplied opportunity, yet an EARLIER clock of the
    /// same actor on the same claim (turn 0, already past) supplies the "opportunity" and the default is attributed.
    #[test]
    fn audit_b03_an_earlier_turns_clock_attributes_the_current_turns_default() {
        let (mut s, _) = bound(1_000);
        observe_all(&mut s, 1_001, 1_101, None);
        let unpaid: BTreeSet<_> = [bond(2)].into_iter().collect();
        let mut current = clock(bond(2), 1_150, 1_200);
        current.turn = 1;
        current.duty_id = h(20);
        // Alone, the current turn is a no-slash hold (correct).
        assert!(matches!(
            attribute_service_default_v1(&s, h(1), &[&current], &unpaid, Some(&TEST_ONLY_PREMISE)),
            Ok(CourtServiceAttributionV1::NoSlashHold(_))
        ));
        // With the actor's earlier turn-0 clock in the claim's history (what `court_duty_clocks_for_claim_v1` returns): fix-b binds
        // the attribution to the defaulted (latest) turn and its own packet, so the earlier window no longer attributes it.
        let earlier = clock(bond(2), 1_001, 1_040);
        assert!(matches!(
            attribute_service_default_v1(&s, h(1), &[&earlier, &current], &unpaid, Some(&TEST_ONLY_PREMISE)),
            Ok(CourtServiceAttributionV1::NoSlashHold(_))
        ), "B-03 fixed: the turn-1 default is not attributed from the turn-0 window");
    }

    /// **audit(verify-2) B-03** — expected-vs-actual: the duty being defaulted is the actor's CURRENT turn (window 1,150..1,200,
    /// after every supplied unit). Its attribution must not change because the claim's history also holds the actor's EARLIER,
    /// already-answered-or-expired turn-0 clock. Expected `NoSlashHold`; on the current code this FAILS with `PartyDefault`.
    #[test]
    fn verify2_b03_attribution_is_bound_to_the_defaulted_turn() {
        let (mut s, _) = bound(1_000);
        observe_all(&mut s, 1_001, 1_101, None);
        let unpaid: BTreeSet<_> = [bond(2)].into_iter().collect();
        let mut current = clock(bond(2), 1_150, 1_200);
        current.turn = 1;
        current.duty_id = h(20);
        let earlier = clock(bond(2), 1_001, 1_040);
        let alone = attribute_service_default_v1(&s, h(1), &[&current], &unpaid, Some(&TEST_ONLY_PREMISE));
        let with_history = attribute_service_default_v1(&s, h(1), &[&earlier, &current], &unpaid, Some(&TEST_ONLY_PREMISE));
        eprintln!("[verify2-b03] current turn alone: {alone:?}; with the earlier turn's clock in history: {with_history:?}");
        assert!(matches!(alone, Ok(CourtServiceAttributionV1::NoSlashHold(_))));
        assert!(
            matches!(with_history, Ok(CourtServiceAttributionV1::NoSlashHold(_))),
            "B-03: expected NoSlashHold for the unsupplied current turn, got {with_history:?}"
        );
    }

    #[test]
    fn court_f02_pending_or_unbound_is_undecided() {
        let (mut s, _) = bound(1_000);
        observe_all(&mut s, 1_001, 1_020, None);
        let unpaid: BTreeSet<_> = [bond(1)].into_iter().collect();
        let c = clock(bond(1), 1_001, 1_200);
        assert_eq!(attribute_service_default_v1(&s, h(1), &[&c], &unpaid, Some(&TEST_ONLY_PREMISE)), Err("court service opportunity is still pending"));
        assert_eq!(attribute_service_default_v1(&s, h(77), &[&c], &unpaid, Some(&TEST_ONLY_PREMISE)), Err("court service binding is missing"));
    }

    #[test]
    fn court_f02_notice_intake_boundaries_and_followups() {
        let a = 1_000;
        assert!(classify_shared_notice_v1(a, a - 1, None).is_err());
        assert_eq!(classify_shared_notice_v1(a, a, None), Ok(SharedNoticeV1::Initial));
        assert_eq!(classify_shared_notice_v1(a, a + 3_120, None), Ok(SharedNoticeV1::Initial));
        assert!(classify_shared_notice_v1(a, a + 3_121, None).is_err());
        // Follow-ups bind to the initial notice and may run until (excluding) the original hard.
        assert_eq!(classify_shared_notice_v1(a, a + 9_999, Some(a + 3_120)), Ok(SharedNoticeV1::FollowUp));
        assert!(classify_shared_notice_v1(a, a + 10_000, Some(a + 3_120)).is_err());
        assert!(classify_shared_notice_v1(a, a + 5, Some(a + 6)).is_err(), "no follow-up before its notice");
        assert!(classify_shared_notice_v1(a, a + 4_000, Some(a + 3_121)).is_err(), "a recorded notice outside A is refused");
        assert!(classify_shared_notice_v1(u64::MAX - 5_000, u64::MAX - 5_000, None).is_err(), "hard overflow refuses");
    }

    /// Lane F02 T07 (component): the TIR attribution's inclusion premise at unit + delay = deadline -1 / = / +1, and no premise.
    #[test]
    fn f02_t07_inclusion_premise_boundaries_on_tir_attribution() {
        let (mut s, _) = bound(1_000);
        observe_all(&mut s, 1_001, 1_101, None);
        let unpaid: BTreeSet<_> = [bond(1)].into_iter().collect();
        let unit = s.book.assigned_daa(&court_service_packet_id_v1(h(1), h(2), h(3), &bond(1), 0)).unwrap();
        let c = clock(bond(1), 1_001, 1_200);
        let with = |delay: u64| {
            let premise = CourtServiceInclusionPremiseV1 { version: 1, inclusion_delay_daa: delay };
            attribute_service_default_v1(&s, h(1), &[&c], &unpaid, Some(&premise))
        };
        let gap = c.response_deadline_daa - unit;
        assert_eq!(with(gap - 1), Ok(CourtServiceAttributionV1::PartyDefault));
        assert_eq!(with(gap), Ok(CourtServiceAttributionV1::PartyDefault));
        assert!(matches!(with(gap + 1), Ok(CourtServiceAttributionV1::NoSlashHold(_))));
        assert!(attribute_service_default_v1(&s, h(1), &[&c], &unpaid, None).is_err());
        assert!(PALW_COURT_SERVICE_INCLUSION_PREMISE_V1.is_some(), "production premise: the user-approved 4-DAA estimate");
    }
}
