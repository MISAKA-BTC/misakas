//! **Lane B of the panel-seed stopgap (2026-09-26): the operator's non-seat accusation, on
//! testnet-12's own fold.**
//!
//! `palw_operator_da_v1::palw_operator_da_candidates_v1` is the read kaspad's operator filer asks
//! once a DAA. These tests run it against states the real fold wrote (`rcore_common`'s [`Chain`]:
//! every block folded, its delta re-applied and reverted, its carriage reloaded), and then play the
//! accusation it leads to through the same fold:
//!
//! * a floor claim licensed through the coverage door by signers outside the "operator" set is
//!   offered at `Licensed`, and after `Final` at `FinalRow`; one the operator produced, or licensed
//!   by operator signers alone, is not; nothing is offered below `palw_rcore_plus`;
//! * the operator bond's accusation is what the fold opens as a NON-seat session (no pause), after
//!   which the read names the operator accuser (every other node backs off);
//! * the claim is `junk_attempt`'s — every root a made-up hash, P0-10's junk — so nobody can answer,
//!   and the session's default voids it `ProducerWithholding` with a `DaDefault` record, the
//!   accuser's exposure returned: a junk claim licensed by a captured panel still meets an honest
//!   accuser and is convicted.
//!
//! The harness's genesis bonds play both sides: the "operator" set here is a subset of them, so the
//! producer (bond 0) and the five coverage signers (bonds 1..5) stand for outside bonds.
//!
//! Run: cargo test -p kaspa-consensus-core --test palw_operator_da_candidates

#[path = "rcore_common.rs"]
mod common;
use common::*;
use kaspa_consensus_core::palw_da_rcore_v1::PalwDaStageV1;
use kaspa_consensus_core::palw_operator_da_v1::palw_operator_da_candidates_v1;
use kaspa_consensus_core::palw_producer_v2::{PalwDaAccusationCheckV1, palw_da_accusation_check_v1};
use kaspa_consensus_core::palw_state_v2::{PalwVoidReasonV2, palw_accuser_exposure_v1};

/// A floor claim licensed through the coverage door by the five genesis seats after its producer
/// (`rcore_m3_da_court`'s): every signer holds a lock.
fn covered_floor_claim(c: &mut Chain, seed: u64) -> (Hash64, Vec<(PalwBondKeyV2, Hash64)>) {
    let id = c.floor_claim(seed);
    let seats = c.floor_seats();
    let bound = c.bind(id, &seats);
    let receipts = covered(id, c.anchor(&id), &seats, &[0, 1, 2, 3, 4], bound);
    c.step(&[PalwConsensusObjectV2::ReceiptLicensedV2 { claim: id, receipts }]);
    assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "the coverage licence folds");
    (id, seats)
}

fn keys(p: &Params) -> Vec<PalwBondKeyV2> {
    genesis_bonds(p).into_iter().map(|(k, _, _)| k).collect()
}

/// Empty blocks until the session `(claim, accuser)` is gone: its deadline's block, then the first
/// past it (`rcore_m3_da_court::run_out`), each block checked as `Chain::step` checks one.
fn run_out(c: &mut Chain, claim: Hash64, accuser: PalwBondKeyV2) -> u64 {
    let deadline = c.s.da_session(&claim, &accuser).expect("an open session").deadline_daa;
    if deadline > c.daa + 1 {
        c.step_at(deadline, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    }
    let daa = deadline + 1;
    c.step_at(daa, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(c.s.da_session(&claim, &accuser).is_none(), "the session is gone the first block past its deadline");
    daa
}

/// **Offered exactly when outside signers carried the licence; the operator's accusation is a
/// non-seat session; the junk defaults and is convicted.**
#[test]
fn a_claim_licensed_by_outside_signers_meets_the_operators_accusation_and_the_junk_is_convicted() {
    let mut c = Chain::new(t12());
    let g = keys(&c.p);
    assert_eq!(g.len(), 8, "testnet-12's eight genesis bonds");
    let (id, seats) = covered_floor_claim(&mut c, 0x0B01);
    let (producer, _, _) = floor_producer(&c.p);
    assert_eq!(producer, g[0]);
    let seat_keys: Vec<PalwBondKeyV2> = seats.iter().map(|(k, _)| *k).collect();
    assert_eq!(seat_keys, g[1..6].to_vec(), "the five genesis seats after the producer");

    // The "operator" set: the two bonds that are neither the producer nor a seat.
    let operators = vec![g[6], g[7]];
    let now = c.daa + 1;
    let offered = palw_operator_da_candidates_v1(&c.s, &c.sp, &operators, now);
    assert_eq!(offered.len(), 1, "the one licensed claim");
    let cand = &offered[0];
    let PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } = c.claim(&id).phase else { unreachable!() };
    assert_eq!((cand.claim_id, cand.producer, cand.stage, cand.stage_daa), (id, producer, PalwDaStageV1::Licensed, licensed_daa));
    assert_eq!(cand.seats, seat_keys, "the current panel, in seat order");
    let mut signers = seat_keys.clone();
    signers.sort();
    assert_eq!(cand.outside_signers, signers, "every coverage signer holds a lock, and none is an operator's");
    assert!(cand.operator_accusers.is_empty());
    assert_eq!(cand.accuse_until_daa, c.claim(&id).trace_retention_daa - c.sp.window_challenge());

    // Not offered: the operator's own claim; a licence carried by operator signers alone.
    assert!(palw_operator_da_candidates_v1(&c.s, &c.sp, &g, now).is_empty(), "the producer is an operator's");
    assert!(palw_operator_da_candidates_v1(&c.s, &c.sp, &g[1..].to_vec(), now).is_empty(), "every signer is an operator's");
    assert!(palw_operator_da_candidates_v1(&c.s, &c.sp, &[], now).is_empty(), "no operator set, nothing");

    // The fold's own gate for the operator bond: a NON-seat session at `Licensed`.
    let accuser = g[6];
    let PalwDaAccusationCheckV1::File { admission, .. } =
        palw_da_accusation_check_v1(&c.s, &c.sp, &c.extras_at(now), &id, &accuser, now)
    else {
        panic!("the fold admits the operator's accusation");
    };
    assert!(!admission.accuser_is_seat, "a non-seat's session");
    assert_eq!(admission.stage, PalwDaStageV1::Licensed);
    let exposure = admission.exposure;
    assert!(exposure > 0);

    c.step(&[da_accuse(id, accuser, 0)]);
    let session = c.s.da_session(&id, &accuser).expect("the session opens").clone();
    assert!(!session.accuser_is_seat);
    assert!(c.s.deadline_of(&id).is_some(), "a non-seat session does not pause the claim (DA-5)");
    assert_eq!(palw_accuser_exposure_v1(&c.s, &accuser), exposure, "the exposure sits on the accuser's free half");
    let offered = palw_operator_da_candidates_v1(&c.s, &c.sp, &operators, c.daa + 1);
    assert_eq!(offered[0].operator_accusers, vec![accuser], "every other operator node now backs off");
    assert_eq!(offered[0].open_non_seat, 1);

    // Junk: every root a made-up hash, so no answer can come. The session defaults.
    let closed = run_out(&mut c, id, accuser);
    assert!(
        matches!(c.claim(&id).phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, voided_daa } if voided_daa == closed),
        "the junk claim is convicted as the withholding it is: {:?}",
        c.claim(&id).phase
    );
    assert!(
        c.s.consumed_offence(&kaspa_consensus_core::palw_da_rcore_v1::palw_da_offence_id_v1(&producer.0, &id)).is_some(),
        "one DaDefault record under (producer, claim)"
    );
    assert_eq!(palw_accuser_exposure_v1(&c.s, &accuser), 0, "the accuser's exposure is returned at a conviction");
    assert!(palw_operator_da_candidates_v1(&c.s, &c.sp, &operators, c.daa + 1).is_empty(), "a voided claim is no one's to accuse");
}

/// **After `Final`, while the vesting row is unmatured, the claim is offered at `FinalRow`** — the
/// row's credited seats count as its signers.
#[test]
fn a_final_claim_with_its_row_unmatured_is_offered_at_final_row() {
    let mut c = Chain::new(t12());
    let g = keys(&c.p);
    let (id, _) = covered_floor_claim(&mut c, 0x0B02);
    c.finalize(id);
    let PalwClaimPhaseV2::Final { final_daa } = c.claim(&id).phase else { panic!("Final") };
    let row = c.s.vesting_row(&id).expect("the row written at Final").clone();
    assert!(row.matured_at.is_none());
    let offered = palw_operator_da_candidates_v1(&c.s, &c.sp, &[g[6], g[7]], c.daa + 1);
    assert_eq!(offered.len(), 1);
    assert_eq!((offered[0].claim_id, offered[0].stage, offered[0].stage_daa), (id, PalwDaStageV1::FinalRow, final_daa));
    assert!(!offered[0].outside_signers.is_empty());
    for (seat, _) in &row.seats {
        assert!(offered[0].outside_signers.contains(seat), "a credited seat of the row is a signer");
    }
}

/// **Below `palw_rcore_plus` the DA court is ADR-0062's and the read offers nothing.**
#[test]
fn nothing_is_offered_below_rcore_plus() {
    let mut t = Chain::new(twin(&t12()));
    let g = keys(&t.p);
    let id = t.floor_claim(0x0B03);
    let seats = t.floor_seats();
    let bound = t.bind(id, &seats);
    t.step(&[PalwConsensusObjectV2::ReceiptLicensed {
        claim: id,
        receipts: seats.iter().map(|(k, _)| valid(id, *k, bound)).collect(),
    }]);
    assert!(matches!(t.claim(&id).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    assert!(palw_operator_da_candidates_v1(&t.s, &t.sp, &[g[6], g[7]], t.daa + 1).is_empty());
}
