//! **ADR-0165 A′ through the fold** — the retired floor: a floor attempt past the fence writes no claim, a REAL class
//! attempt is taken and records the idle ledger's last-accept DAA, claims the floor won earlier settle normally,
//! an all-floor network crossing the fence stays idle (so FALLBACK-eligible) and live, and a reorg reverts exactly.

use super::*;
use crate::palw_real_share_v1::*;

const FENCE: u64 = 100;

fn fp() -> PalwStateParamsV2 {
    params().with_floor_reserve_from_daa(Some(FENCE))
}

/// A world with the floor (class 1) and a real class (class 2), both registered at daa 100.
fn world(p: &PalwStateParamsV2) -> PalwChainStateV2 {
    let mut objects = register_class_and_bond();
    let mut real = registration(h64(2), 100, None);
    if let PalwConsensusObjectV2::ClassRegistered { slash_value_per_pwu, .. } = &mut real {
        *slash_value_per_pwu = 5; // the network's price, as the floor registers it
    }
    objects.push(real);
    let (s, _) = apply(&PalwChainStateV2::genesis(), p, &ctx(1, FENCE, 1), &objects, None);
    s
}

fn env(class: Hash64, nonce: u64) -> PalwAttemptEnvelopeV2 {
    attempt_for_class(40, nonce, class, bond_key(1), vec![7; 4], op_id(21), if class == h64(1) { h64(11) } else { h64(0xA1) })
}

/// Walk one attempt to `Final` from `from_daa` and return the state and the claim.
fn to_final(p: &PalwStateParamsV2, s: &PalwChainStateV2, e: &PalwAttemptEnvelopeV2, from_daa: u64, blue: u64) -> (PalwChainStateV2, Hash64) {
    let claim_id = attempt_id_v2(&e.attempt);
    let (s1, _) = apply(s, p, &ctx(blue, from_daa, blue), &[], Some(e));
    let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: h64(90) }];
    let (s2, _) =
        apply(&s1, p, &ctx(blue + 1, from_daa + 1, blue + 1), &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(77), seats }], None);
    let (s3, _) = apply(
        &s2,
        p,
        &ctx(blue + 2, from_daa + 2, blue + 2),
        &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: seat_says(true) }],
        None,
    );
    let (s4, _) = apply(&s3, p, &ctx(blue + 3, from_daa + 23, blue + 3), &[], None);
    (s4, claim_id)
}

#[test]
fn a_floor_attempt_past_the_fence_writes_no_claim_and_a_real_one_is_taken_and_recorded() {
    let p = fp();
    let s = world(&p);
    let floor = env(h64(1), 7);
    let (out, delta) = apply(&s, &p, &ctx(2, 105, 2), &[], Some(&floor));
    assert!(out.claim(&attempt_id_v2(&floor.attempt)).is_none(), "the retired floor writes no claim: no reward escrowed, no weight");
    assert_eq!((out.safe_weight(), out.bounded_immature()), (0, 0));
    assert!(out.real_work.is_empty(), "a floor attempt is not real work");
    let real = env(h64(2), 8);
    let (out2, delta2) = apply(&s, &p, &ctx(2, 105, 2), &[], Some(&real));
    assert!(out2.claim(&attempt_id_v2(&real.attempt)).is_some(), "real work is not touched");
    assert_eq!(palw_real_last_accept_v1(&out2.real_work), Some(105), "and it is the idle ledger's last accept");
    assert!(!palw_real_idle_at_v1(&out2.real_work, 107) && palw_real_idle_at_v1(&out2.real_work, 108));
    // Reorg: each delta reverts exactly and re-applies exactly.
    for (child, d) in [(&out, &delta), (&out2, &delta2)] {
        let reverted = revert_delta_v2(child, d, &p).expect("revert");
        assert_eq!(reverted.state_root(), s.state_root(), "a reorg drags no ledger row");
        assert_eq!(apply_delta_v2(&s, d, &p).expect("apply").state_root(), child.state_root());
    }
    // The producer's pre-check is the same gate.
    let refusal = palw_class_admits_claim_v1(&s, &p, &PalwTransitionExtrasV1::default(), &h64(1), 105).unwrap_err();
    assert!(matches!(refusal, PalwStateV2Error::FloorRetired { .. }), "{refusal:?}");
    assert!(palw_class_admits_claim_v1(&s, &p, &PalwTransitionExtrasV1::default(), &h64(2), 105).is_ok());
    // Below the fence the floor is taken as ever, and nothing is recorded.
    let below = params();
    let s0 = world(&below);
    let (b, _) = apply(&s0, &below, &ctx(2, 105, 2), &[], Some(&floor));
    assert!(b.claim(&attempt_id_v2(&floor.attempt)).is_some() && b.real_work.is_empty());
}

#[test]
fn a_floor_claim_won_before_the_fence_settles_to_final_and_pays() {
    // The fence at 110: the claim is accepted at 105 (below it), licensed, and finalizes past the fence.
    let p = params().with_floor_reserve_from_daa(Some(110));
    let s = world(&p);
    let floor = env(h64(1), 9);
    let (done, claim_id) = to_final(&p, &s, &floor, 105, 2);
    assert!(matches!(done.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "an open floor claim settles normally");
    assert_eq!(done.safe_weight(), 40, "its weight is credited at Final as ever");
    // …and a floor attempt AFTER the fence is skipped.
    let late = env(h64(1), 10);
    let (after, _) = apply(&done, &p, &ctx(20, 140, 20), &[], Some(&late));
    assert!(after.claim(&attempt_id_v2(&late.attempt)).is_none());
}

#[test]
fn an_all_floor_network_crossing_the_fence_is_idle_and_stays_live() {
    let p = fp();
    let s = world(&p);
    assert!(palw_real_idle_at_v1(&s.real_work, FENCE + 1), "no real work ever: idle, so the fallback is eligible");
    // The chain goes on: blocks with no attempt apply, and real work arriving ends the idleness.
    let (a, _) = apply(&s, &p, &ctx(2, 200, 2), &[], None);
    assert!(palw_real_idle_at_v1(&a.real_work, 201));
    let real = env(h64(2), 11);
    let (b, _) = apply(&a, &p, &ctx(3, 201, 3), &[], Some(&real));
    assert!(!palw_real_idle_at_v1(&b.real_work, 202) && palw_real_idle_at_v1(&b.real_work, 204));
}

#[test]
fn the_carriage_round_trips_the_ledger_and_a_dormant_network_carries_nothing() {
    let p = fp();
    let (s, _) = apply(&world(&p), &p, &ctx(2, 105, 2), &[], Some(&env(h64(2), 31)));
    let carriage = PalwStateCarriageV2::from_state(&s);
    assert!(!carriage.real_work.is_empty());
    let bytes = borsh::to_vec(&carriage).expect("encode");
    let back: PalwStateCarriageV2 = borsh::from_slice(&bytes).expect("decode");
    assert_eq!(back.real_work, s.real_work);
    let plain = world(&params());
    assert!(PalwStateCarriageV2::from_state(&plain).real_work.is_empty(), "nothing is carried below the fence");
}
