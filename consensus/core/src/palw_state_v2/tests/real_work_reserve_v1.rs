//! **ADR-0165 A through the fold** — the base floor as a reserve (`Params::palw_floor_reserve_v1`): the
//! ledger a Final real-class claim writes, the mode it reads, the refusal that writes no claim, the exact
//! revert, and an all-floor network that stays live.

use super::*;
use crate::palw_real_share_v1::*;

const FENCE: u64 = 100;

fn fp() -> PalwStateParamsV2 {
    params().with_floor_reserve_from_daa(Some(FENCE))
}

/// A world with the floor (class 1) and a real class (class 2), both registered at daa 100.
fn world(p: &PalwStateParamsV2) -> PalwChainStateV2 {
    let mut objects = register_class_and_bond();
    objects.push(registration(h64(2), 100, None));
    let (s, _) = apply(&PalwChainStateV2::genesis(), p, &ctx(1, FENCE, 1), &objects, None);
    s
}

/// Walk one attempt of `class` to `Final` from `from_daa` and return the state, the claim and the DAA it final at.
fn to_final(p: &PalwStateParamsV2, s: &PalwChainStateV2, class: Hash64, nonce: u64, from_daa: u64, blue: u64) -> (PalwChainStateV2, Hash64, u64) {
    let env = attempt_for_class(40, nonce, class, bond_key(1), vec![7; 4], op_id(21), if class == h64(1) { h64(11) } else { h64(0xA1) });
    let claim_id = attempt_id_v2(&env.attempt);
    let (s1, _) = apply(s, p, &ctx(blue, from_daa, blue), &[], Some(&env));
    let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: h64(90) }];
    let (s2, _) = apply(&s1, p, &ctx(blue + 1, from_daa + 1, blue + 1), &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(77), seats }], None);
    let (s3, _) = apply(
        &s2,
        p,
        &ctx(blue + 2, from_daa + 2, blue + 2),
        &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: seat_says(true) }],
        None,
    );
    let final_daa = from_daa + 2 + 21;
    let (s4, _) = apply(&s3, p, &ctx(blue + 3, final_daa, blue + 3), &[], None);
    assert!(matches!(s4.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "the claim reached Final");
    (s4, claim_id, final_daa)
}

fn units(s: &PalwChainStateV2) -> u64 {
    s.real_work.iter().filter(|(k, _)| **k < PALW_REAL_WORK_EVAL_KEY_V1).fold(0, |a, (_, n)| a + n)
}

#[test]
fn a_final_real_class_claim_is_one_unit_and_a_final_floor_claim_is_none() {
    let p = fp();
    let s = world(&p);
    let (after_real, _, at) = to_final(&p, &s, h64(2), 1, 105, 2);
    assert_eq!(after_real.real_work.get(&palw_real_work_bucket_v1(at)), Some(&1), "one Final real-class claim, in its bucket");
    assert_eq!(units(&after_real), 1);
    let (after_floor, _, _) = to_final(&p, &s, h64(1), 2, 105, 2);
    assert_eq!(units(&after_floor), 0, "the floor's own Final is not real work");
    // Below the fence nothing is written at all: the root a build without the field computes.
    let below = params();
    let s0 = world(&below);
    let (b, _, _) = to_final(&below, &s0, h64(2), 3, 105, 2);
    assert!(b.real_work.is_empty(), "no ledger below the fence");
}

#[test]
fn a_dormant_reserve_skips_the_floor_attempt_writing_no_claim_and_the_real_class_is_untouched() {
    let p = fp();
    let mut s = world(&p);
    // Twelve Final real-class claims in the window ending before bucket 12 (DAA 120).
    s.real_work.insert(11, PALW_REAL_WORK_DORMANT_AT_V1);
    let floor = attempt(40, 7);
    let floor_id = attempt_id_v2(&floor.attempt);
    let (out, delta) = apply(&s, &p, &ctx(2, 125, 2), &[], Some(&floor));
    assert!(out.claim(&floor_id).is_none(), "the dormant reserve writes no claim: no reward escrowed, no weight");
    assert_eq!((out.safe_weight(), out.bounded_immature()), (0, 0));
    assert_eq!(out.real_work.get(&PALW_REAL_WORK_MODE_KEY_V1), Some(&1), "the mode is stored dormant");
    // A real-class attempt in the same block is taken.
    let real = attempt_for_class(40, 8, h64(2), bond_key(1), vec![7; 4], op_id(21), h64(0xA1));
    let real_id = attempt_id_v2(&real.attempt);
    let (out2, _) = apply(&s, &p, &ctx(2, 125, 2), &[], Some(&real));
    assert!(out2.claim(&real_id).is_some(), "real work is not touched by the reserve");
    // Reorg: the delta reverts exactly and re-applies exactly, ledger included.
    let reverted = revert_delta_v2(&out, &delta, &p).expect("revert");
    assert_eq!(reverted.real_work, s.real_work);
    assert_eq!(reverted.state_root(), s.state_root(), "a reorg drags no ledger row");
    assert_eq!(apply_delta_v2(&s, &delta, &p).expect("apply").state_root(), out.state_root());
    // The reserve's gate is the producer's pre-check too: the same refusal from outside the fold.
    let (ledger_rolled, _) = apply(&s, &p, &ctx(3, 126, 3), &[], None);
    let refusal = palw_class_admits_claim_v1(&ledger_rolled, &p, &PalwTransitionExtrasV1::default(), &h64(1), 127).unwrap_err();
    assert!(matches!(refusal, PalwStateV2Error::FloorDormant { .. }), "{refusal:?}");
    assert!(palw_class_admits_claim_v1(&ledger_rolled, &p, &PalwTransitionExtrasV1::default(), &h64(2), 127).is_ok());
}

#[test]
fn an_all_floor_network_stays_live_and_the_reserve_returns_when_real_work_stalls() {
    let p = fp();
    let s = world(&p);
    // No real work ever: the floor attempt is taken, block after block, past the fence.
    let (a, _, _) = to_final(&p, &s, h64(1), 11, 105, 2);
    let (b, _, _) = to_final(&p, &a, h64(1), 12, 140, 10);
    assert_eq!(units(&b), 0);
    assert!(!palw_real_work_dormant_at_v1(&b.real_work, 200), "the reserve is active with an empty ledger");
    // Dormant, then the window drains below the low threshold: the floor is admitted again.
    let mut d = world(&p);
    d.real_work.insert(11, PALW_REAL_WORK_DORMANT_AT_V1);
    let (d1, _) = apply(&d, &p, &ctx(2, 125, 2), &[], None);
    assert!(palw_real_work_dormant_at_v1(&d1.real_work, 126));
    // Seven buckets later the ledger's only row has left the window.
    let (d2, _) = apply(&d1, &p, &ctx(3, 125 + 70, 3), &[], None);
    assert!(!palw_real_work_dormant_at_v1(&d2.real_work, 195), "real work stalled: the reserve is active again");
    let floor = attempt(40, 21);
    let (d3, _) = apply(&d2, &p, &ctx(4, 196, 4), &[], Some(&floor));
    assert!(d3.claim(&attempt_id_v2(&floor.attempt)).is_some(), "the floor earns again once the reserve returns");
}

#[test]
fn the_carriage_round_trips_the_ledger_and_a_dormant_network_carries_nothing() {
    let p = fp();
    let (s, _, _) = to_final(&p, &world(&p), h64(2), 31, 105, 2);
    let carriage = PalwStateCarriageV2::from_state(&s);
    assert!(!carriage.real_work.is_empty());
    let bytes = borsh::to_vec(&carriage).expect("encode");
    let back: PalwStateCarriageV2 = borsh::from_slice(&bytes).expect("decode");
    assert_eq!(back.real_work, s.real_work);
    let plain = world(&params());
    assert!(PalwStateCarriageV2::from_state(&plain).real_work.is_empty(), "nothing is carried below the fence");
}
