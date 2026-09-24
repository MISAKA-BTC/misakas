//! **ADR-0152 v3.1 M4 part 1 through the public fold: `Sampled` (Q-1) and SR-10's V3
//! supplementary door**, on what the module tests (`palw_state_v2::tests::rcore_m4_sampled_and_v3_door`)
//! do not reach — adopted from the M4 review's probe.
//!
//! * The door on the record a REAL S2 licence leaves in this tree: S-2 has not landed, so the
//!   licence stages no R-core+ record and nothing here is hand-written. When S-2 lands, the licence
//!   stages its record and this test's pre-S-2 expectations are to be re-derived (its tripwire).
//! * A re-carried abstention: refused by both layers once `unserved_seen` is latched (review
//!   finding 4), and the seat may still serve later.
//! * `Incapable` on the liveness floor, and a `Sampled` over a mask it was not assigned.
//! * The assembler's licence predicate on both sides of the fence, and the fold's one `Sampled`
//!   check below it, ahead of every arm.
//! * An assembler's reading of the new refusals and SR-1b's window, through the public API.

use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2, challenge_v2,
};
use kaspa_consensus_core::palw_optimistic_licence_v2::palw_optimistic_full_seat_bond_v2;
use kaspa_consensus_core::palw_panel_v2::{
    PalwPanelV2Error, PalwReceiptQuorumV2, PalwReceiptVerdictV2, PalwSeatReceiptV2, PalwSeatReceiptV3,
    validate_supplementary_receipts_v3,
};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwClaimRcoreV1, PalwConsensusObjectV2 as Obj,
    PalwPanelSeatV2, PalwPwuRuleV2, PalwStateDeltaV2, PalwStateParamsV2, PalwStateV2Error, PalwTransitionExtrasV1, apply_delta_v2,
    apply_palw_transition_v2_with_extras, palw_object_sampled_receipt_v1, palw_operator_id_v2, palw_rcore_counts_licensed_v1,
    palw_rcore_lock_v1, palw_rcore_release_window_closes_v1, palw_receipt_set_basis_k_v1, palw_v2_object_licenses_claim_v1,
    revert_delta_v2,
};
use kaspa_consensus_core::palw_verification_v2::{PalwSegmentMaskV2, palw_segment_assignment_v2};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_consensus_core::palw_economic_safety_v1::PalwLicenceDoorTagV1;
use kaspa_hashes::Hash64;

const L: u64 = 103;
const CUT: u16 = 4;

fn h64(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn bond_key(v: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 })
}

fn op_key(v: u64) -> Vec<u8> {
    vec![v as u8; 8]
}

fn ctx(block_word: u64, daa: u64) -> PalwBlockContextV2 {
    PalwBlockContextV2 { block: Hash64::from_u64_word(block_word), daa_score: daa, blue_score: daa, subsidy: 0 }
}

fn params(rcore: bool) -> PalwStateParamsV2 {
    let p =
        PalwStateParamsV2::new(100, 10, 600, 120, 500, 1000, h64(1), 4, 1000, 100, 1000, 0).unwrap().with_fp_quanta(8, 64).unwrap();
    if rcore { p.with_rcore_plus_mirrors(Some(0), 0, Vec::new()) } else { p }
}

fn extras() -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 {
        objective_offence_daa: Some(0),
        audit_2026_09_23_active: true,
        verification_v2_active: true,
        verification_s2_active: true,
        panel_economy_active: true,
        ..Default::default()
    }
}

fn apply(
    parent: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    c: &PalwBlockContextV2,
    objects: &[Obj],
    att: Option<&PalwAttemptEnvelopeV2>,
) -> Result<(PalwChainStateV2, PalwStateDeltaV2), PalwStateV2Error> {
    let applied = apply_palw_transition_v2_with_extras(parent, p, c, objects, att, false, false, false, false, &extras())?;
    applied.0.assert_internal_consistency(p).expect("internal consistency");
    applied.0.assert_deadline_consistency(p).expect("deadline consistency");
    Ok(applied)
}

fn attempt(pwu: u64, nonce: u64) -> PalwAttemptEnvelopeV2 {
    let network_domain = h64(999);
    let bond = bond_key(1).0;
    PalwAttemptEnvelopeV2 {
        attempt: PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain,
            challenge: challenge_v2(network_domain, h64(5), 1_700, nonce, h64(1), &bond),
            class_id: h64(1),
            executor_bond: bond,
            executor_pubkey: vec![7; 4],
            operator_id: palw_operator_id_v2(&op_key(21)),
            artifact_root: h64(11),
            trace_root: h64(31),
            output_root: h64(32),
            pwu,
            trace_manifest_root: h64(33),
            trace_chunk_count: 4,
            trace_retention_daa: 999_999,
            execution_root: h64(41),
        },
        signature: vec![0; 8],
    }
}

fn seats() -> Vec<PalwPanelSeatV2> {
    (2..=6).map(|n| PalwPanelSeatV2 { bond: bond_key(n), operator_id: palw_operator_id_v2(&op_key(20 + n)) }).collect()
}

fn bonds() -> Vec<PalwBondKeyV2> {
    seats().iter().map(|seat| seat.bond).collect()
}

/// A 160-pwu floor claim bound at 102 to five seats, each posting `collateral`.
fn bound(p: &PalwStateParamsV2) -> (PalwChainStateV2, Hash64) {
    let mut objects = vec![
        Obj::ClassRegistered {
            class_id: h64(1),
            artifact_root: h64(11),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(160),
            initial_target: u128::MAX / 2,
            share_permille: 1000,
            activation_daa: 0,
            admission: None,
        },
        Obj::BondRegistered {
            bond: bond_key(1),
            pubkey: vec![7; 4],
            operator_pubkey: op_key(21),
            collateral: 1_000,
            payout_payload: h64(0x9A11),
            capable_classes: Default::default(),
            signature: Vec::new(),
        },
    ];
    objects.extend((2..=6).map(|n| Obj::BondRegistered {
        bond: bond_key(n),
        pubkey: vec![0x40 + n as u8; 4],
        operator_pubkey: op_key(20 + n),
        collateral: 1_000_000_000,
        payout_payload: h64(0x9A00 + n),
        capable_classes: Default::default(),
        signature: Vec::new(),
    }));
    let (s0, _) = apply(&PalwChainStateV2::genesis(), p, &ctx(1, 100), &objects, None).expect("registry");
    let env = attempt(160, 1);
    let claim_id = attempt_id_v2(&env.attempt);
    let (s1, _) = apply(&s0, p, &ctx(2, 101), &[], Some(&env)).expect("claim");
    let (s2, _) =
        apply(&s1, p, &ctx(3, 102), &[Obj::PanelBound { claim: claim_id, anchor: h64(77), seats: seats() }], None).expect("bind");
    (s2, claim_id)
}

fn mask_of(claim_id: Hash64, bond: PalwBondKeyV2) -> PalwSegmentMaskV2 {
    let index = bonds().iter().position(|b| *b == bond).unwrap() as u16;
    palw_segment_assignment_v2(h64(77), claim_id, 5).mask_of(index)
}

fn geometry(claim_id: Hash64) -> (PalwBondKeyV2, Vec<PalwBondKeyV2>) {
    let assignment = palw_segment_assignment_v2(h64(77), claim_id, 5);
    let full = palw_optimistic_full_seat_bond_v2(&assignment, &bonds()).expect("a full seat");
    (full, bonds().into_iter().filter(|b| *b != full).collect())
}

fn v2(claim_id: Hash64, seat: PalwBondKeyV2, verdict: PalwReceiptVerdictV2, signed_daa: u64) -> PalwSeatReceiptV2 {
    PalwSeatReceiptV2 { claim: claim_id, verdict, seat_bond: seat, signed_daa, signature: Vec::new() }
}

fn v3(claim_id: Hash64, seat: PalwBondKeyV2, verdict: PalwReceiptVerdictV2, signed_daa: u64) -> PalwSeatReceiptV3 {
    PalwSeatReceiptV3 { receipt: v2(claim_id, seat, verdict, signed_daa), segments: mask_of(claim_id, seat) }
}

fn door(claim_id: Hash64, receipts: Vec<PalwSeatReceiptV3>) -> Obj {
    Obj::ReceiptLicensedV2 { claim: claim_id, receipts }
}

/// A REAL S2 licence of the full seat and one partial at `L`, with the R-core+ record S-2's licence
/// staging writes (`Optimistic`, Q-3's `basis_k` 1, the two seats served) — the state the door meets.
fn s2_licensed(p: &PalwStateParamsV2) -> (PalwChainStateV2, Hash64, PalwBondKeyV2, Vec<PalwBondKeyV2>) {
    let (s2, claim_id) = bound(p);
    let (full, partial) = geometry(claim_id);
    let receipts = [full, partial[0]].iter().map(|seat| v3(claim_id, *seat, PalwReceiptVerdictV2::Valid, L)).collect();
    let (s3, _) = apply(&s2, p, &ctx(4, L), &[Obj::OptimisticLicensed { claim: claim_id, receipts }], None).expect("S2 licenses");
    assert!(matches!(s3.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: L }));
    (s3, claim_id, full, partial)
}

fn accept_all(_: &[u8], _: &[u8], _: &[u8], _: &[u8]) -> bool {
    true
}

fn credited(state: &PalwChainStateV2, claim_id: Hash64, seat: PalwBondKeyV2) -> bool {
    state.panel_duties_of(&claim_id).and_then(|row| row.get(&seat)).is_some_and(|at| *at != 0)
}

fn round_trips(before: &PalwChainStateV2, after: &PalwChainStateV2, delta: &PalwStateDeltaV2, p: &PalwStateParamsV2) {
    assert_eq!(revert_delta_v2(after, delta, p).unwrap().state_root(), before.state_root());
    assert_eq!(apply_delta_v2(before, delta, p).unwrap().state_root(), after.state_root());
}

/// **What an assembler and SR-1b read, through the public API.** Below the fence a `Sampled`
/// poisons the set rather than falling short, so an assembler drops the receipt instead of waiting
/// for more; a supplementary set is not a shortfall either. SR-10 prices `lock_{max(k, 2)}` (0, 1
/// and 2 price alike, 3 is cheaper), and SR-1b's window is `min(L + wc/2, receipt deadline)` —
/// both terms bind.
#[test]
fn an_assembler_reads_the_new_refusals_and_sr1b_s_window() {
    assert!(!PalwPanelV2Error::SampledBelowRcore(bond_key(2)).is_receipt_set_shortfall());
    assert!(!PalwPanelV2Error::SupplementaryV3Refused("any").is_receipt_set_shortfall());
    let (g, e) = (10_000_000u128, 4_000_000u64);
    assert_eq!(palw_rcore_lock_v1(g, e, 0, 0), palw_rcore_lock_v1(g, e, 0, 2));
    assert_eq!(palw_rcore_lock_v1(g, e, 0, 1), palw_rcore_lock_v1(g, e, 0, 2));
    assert!(palw_rcore_lock_v1(g, e, 0, 3) < palw_rcore_lock_v1(g, e, 0, 2));
    let p = params(true);
    assert_eq!(palw_rcore_release_window_closes_v1(&p, L, 702), L + 60, "the challenge half binds");
    assert_eq!(palw_rcore_release_window_closes_v1(&p, L, L + 10), L + 10, "the receipt deadline binds");
    assert_eq!(palw_object_sampled_receipt_v1(&Obj::ReceiptLicensed { claim: h64(0xC1), receipts: vec![] }), None);
}

/// **The door on the record a real S2 licence leaves** (re-derived when S-2 landed, as this test's
/// first version said it would be). The licence stages S-2's record — `Optimistic`, `basis_k` 1, the
/// full seat's and the first partial's served bits — records each signer's attested mask on its lock
/// (S-3: the full seat the full cut, the partial its own), and does not tick the anchor (V-8: an S2
/// licence recounted below 2 never ticks). The door then recounts the counted locks with the three
/// new partial `Valid`s to 2, which is the first crossing: the door upgrades to Coverage (a counted
/// mask is partial), the anchor ticks once, the served mask is the whole panel, and SR-1b flips the
/// escrow inside its window (`L + 1 ≤ L + 60`).
#[test]
fn the_door_on_a_real_s2_licence() {
    let p = params(true);
    let (s2, _) = bound(&p);
    let (s3, claim_id, full, partial) = s2_licensed(&p);
    let bit = |seat: &PalwBondKeyV2| 1u32 << bonds().iter().position(|b| b == seat).unwrap();
    let licensed = s3.claim(&claim_id).unwrap().rcore;
    assert_eq!(
        licensed,
        PalwClaimRcoreV1 {
            licence_door: Some(PalwLicenceDoorTagV1::Optimistic),
            basis_k: 1,
            escrow_released: false,
            served_mask: bit(&full) | bit(&partial[0]),
            unserved_seen: false,
            g_res_sompi: licensed.g_res_sompi,
        },
        "S-2's licence staging"
    );
    assert!(licensed.g_res_sompi > 0, "the S-4 review: the first licence records its G_res");
    assert_eq!(s3.settled_attempt_finals(), s2.settled_attempt_finals(), "V-8: an S2 licence below 2 does not tick");
    let lock = *s3.slashable_lock(full, claim_id).expect("the full seat locked at licence");
    assert_eq!((lock.attested, lock.segments), (PalwSegmentMaskV2::full(CUT), CUT), "S-3: the full seat's mask on its lock");
    let lock = *s3.slashable_lock(partial[0], claim_id).expect("the partial locked at licence");
    assert_eq!((lock.attested, lock.segments), (mask_of(claim_id, partial[0]), CUT), "S-3: the partial's own mask");

    let rest: Vec<PalwSeatReceiptV3> =
        partial[1..].iter().map(|seat| v3(claim_id, *seat, PalwReceiptVerdictV2::Valid, L + 1)).collect();
    assert_eq!(
        validate_supplementary_receipts_v3(&s3, &p, &ctx(5, L + 1), h64(999), &claim_id, &rest, accept_all),
        Ok(PalwReceiptQuorumV2::Supplementary { credited: 3 })
    );
    let (s4, d4) = apply(&s3, &p, &ctx(5, L + 1), &[door(claim_id, rest)], None).expect("the door");
    let claim = s4.claim(&claim_id).unwrap().clone();
    assert_eq!(claim.rcore.basis_k, 2, "full + p0 + p1..p3: every segment twice");
    assert_eq!(claim.rcore.licence_door, Some(PalwLicenceDoorTagV1::Coverage), "the first crossing upgrades: a counted mask is partial");
    assert_eq!(s4.settled_attempt_finals(), s3.settled_attempt_finals() + 1, "and ticks the anchor once");
    let panel = bonds().iter().take(5).fold(0u32, |a, seat| a | bit(seat));
    assert_eq!(claim.rcore.served_mask, panel, "the licence's bits and the door's: the whole panel");
    assert!(palw_rcore_counts_licensed_v1(&claim));
    assert!(claim.rcore.escrow_released, "SR-1b flips inside min(L + 60, deadline)");
    for seat in &partial[1..] {
        assert!(credited(&s4, claim_id, *seat));
        let lock = s4.slashable_lock(*seat, claim_id).expect("locked");
        assert_eq!((lock.attested, lock.segments), (mask_of(claim_id, *seat), CUT));
    }
    round_trips(&s3, &s4, &d4, &p);
}

/// **A re-carried abstention is refused once `unserved_seen` is latched** (M4 review, finding 4).
/// An `Unavailable` is neither credited nor locked, so its seat stays uncounted; the first carriage
/// latches `unserved_seen`, and the same signed receipt carried again would fold to nothing — so
/// both layers refuse it by the same test. The seat may still carry a `Valid` through the same door
/// later (it is still not credited), and the latch never un-latches.
#[test]
fn a_recarried_abstention_is_refused_and_the_seat_may_still_serve() {
    let p = params(true);
    let (s3, claim_id, _, partial) = s2_licensed(&p);
    let abstain = vec![v3(claim_id, partial[1], PalwReceiptVerdictV2::Unavailable { chunk_index: 0, requested_daa: 102 }, L + 1)];
    let (s4, d4) = apply(&s3, &p, &ctx(5, L + 1), &[door(claim_id, abstain.clone())], None).expect("the first abstention");
    assert!(s4.claim(&claim_id).unwrap().rcore.unserved_seen);
    assert!(!credited(&s4, claim_id, partial[1]) && s4.slashable_lock(partial[1], claim_id).is_none());
    round_trips(&s3, &s4, &d4, &p);
    assert_eq!(
        validate_supplementary_receipts_v3(&s4, &p, &ctx(6, L + 2), h64(999), &claim_id, &abstain, accept_all),
        Err(PalwPanelV2Error::SupplementaryV3Refused("abstentions only, on a claim whose unserved_seen is already latched")),
        "the acceptance layer refuses the same receipt again"
    );
    assert!(
        matches!(
            apply(&s4, &p, &ctx(6, L + 2), &[door(claim_id, abstain)], None),
            Err(PalwStateV2Error::SupplementaryV3Refused { .. })
        ),
        "and so does the fold"
    );
    let (served, _) =
        apply(&s4, &p, &ctx(6, L + 2), &[door(claim_id, vec![v3(claim_id, partial[1], PalwReceiptVerdictV2::Valid, L + 2)])], None)
            .expect("the same seat later serves");
    assert!(served.slashable_lock(partial[1], claim_id).is_some());
    assert!(served.claim(&claim_id).unwrap().rcore.unserved_seen, "the latch never un-latches");
}

/// **`Incapable` on the liveness floor is refused by both layers** (this fixture's class is the
/// base class), and a `Sampled` over a mask it was not assigned rides harmlessly: credited, no
/// lock, no served bit, no count.
#[test]
fn incapable_on_the_floor_and_an_unassigned_sampled_mask() {
    let p = params(true);
    let (s3, claim_id, _, partial) = s2_licensed(&p);
    let plea = vec![v3(claim_id, partial[1], PalwReceiptVerdictV2::Incapable, L + 1)];
    assert!(matches!(
        validate_supplementary_receipts_v3(&s3, &p, &ctx(5, L + 1), h64(999), &claim_id, &plea, accept_all),
        Err(PalwPanelV2Error::UnmetObligationNotProven { .. })
    ));
    assert!(matches!(
        apply(&s3, &p, &ctx(5, L + 1), &[door(claim_id, plea)], None),
        Err(PalwStateV2Error::SupplementaryV3Refused { .. })
    ));

    let mut sampler = v3(claim_id, partial[1], PalwReceiptVerdictV2::Sampled, L + 1);
    sampler.segments = PalwSegmentMaskV2::full(CUT);
    assert!(
        validate_supplementary_receipts_v3(&s3, &p, &ctx(5, L + 1), h64(999), &claim_id, std::slice::from_ref(&sampler), accept_all)
            .is_ok()
    );
    let before_k = palw_receipt_set_basis_k_v1(&[PalwSegmentMaskV2::full(CUT), mask_of(claim_id, partial[0])], CUT);
    let (s4, d4) = apply(&s3, &p, &ctx(5, L + 1), &[door(claim_id, vec![sampler])], None).expect("a sampler rides");
    let rcore = s4.claim(&claim_id).unwrap().rcore;
    assert!(credited(&s4, claim_id, partial[1]) && s4.slashable_lock(partial[1], claim_id).is_none());
    assert_eq!(rcore.served_mask, s3.claim(&claim_id).unwrap().rcore.served_mask, "no served bit: the licence's alone");
    assert_eq!(rcore.basis_k, before_k, "the recount of the counted signers alone: the full mask it signed counts nowhere");
    round_trips(&s3, &s4, &d4, &p);
}

/// **The assembler's licence predicate on both sides of the fence.** Below it a V1 set on a
/// licensed claim is the V2 supplementary door and the predicate reads "licensed after" as it always
/// did; past it the same question answers `false` for any claim that was not `PanelBound`.
#[test]
fn the_licence_predicate_moves_only_past_the_fence() {
    for rcore in [false, true] {
        let p = params(rcore);
        let (s3, claim_id, _, partial) = s2_licensed(&p);
        let v1_supplementary =
            Obj::ReceiptLicensed { claim: claim_id, receipts: vec![v2(claim_id, partial[1], PalwReceiptVerdictV2::Valid, L + 1)] };
        let answer =
            palw_v2_object_licenses_claim_v1(&s3, &p, &ctx(5, L + 1), &v1_supplementary, false, false, false, false, &extras());
        assert_eq!(answer, !rcore, "rcore = {rcore}");
        // Either way the V2 door itself still folds the set.
        apply(&s3, &p, &ctx(5, L + 1), &[v1_supplementary], None).expect("the V2 door");
    }
}

/// **Below the fence a `Sampled` anywhere is refused by the fold's one check, ahead of every arm**:
/// on a licensed claim the object never reaches the door's phase test, so the refusal names the
/// verdict, not the phase. (The door itself closed below the fence is the module's fence-off twin.)
#[test]
fn below_the_fence_sampled_is_refused_ahead_of_every_arm() {
    let p = params(false);
    let (s3, claim_id, _, partial) = s2_licensed(&p);
    let set = vec![v3(claim_id, partial[1], PalwReceiptVerdictV2::Sampled, L + 1)];
    assert_eq!(
        apply(&s3, &p, &ctx(5, L + 1), &[door(claim_id, set)], None).unwrap_err(),
        PalwStateV2Error::SampledBelowRcore { claim: claim_id, seat: partial[1] }
    );
}
