//! **RFC-0007 Part IV.2 (staged onboarding) through the registry's span step** (spec 18): a `Prefetching` class that cannot fill a panel but
//! shows a DA certificate steps to `Capped`; a capped class admits claims at `capped_admission_permille` without a panel room; a capped
//! claim owes no bind deadline while its class is capped and gets its re-verification window when holders are seated; the weight cap
//! refuses a claim once the capped classes hold more than `w_cap`; and below the fence none of it exists.
//!
//! Run on the ADR-0135 fixture: Kimi (`kimi_id()`) is the class, bonds 2 to 8 its would-be holders.

use super::*;
use crate::palw_mesh_v1::*;
use crate::palw_vertex_v1::*;

fn cp(vertex: bool, mesh: bool, capped: Option<u64>) -> PalwStateParamsV2 {
    params()
        .with_vertex_from_daa(vertex.then_some(0))
        .with_audit_mesh_from_daa(mesh.then_some(0))
        .with_capped_from_daa(capped)
}

fn dummy_sign(message: &[u8], _context: &[u8]) -> Option<Vec<u8>> {
    let mut signature = message.to_vec();
    signature.resize(crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN, 0);
    Some(signature)
}

/// A vertex of `seat` attesting (a `Held` leaf of the capture of the class's probe job) that it holds `digest`.
fn attests(seat: u64, daa: u64, digest: Hash64) -> PalwConsensusObjectV2 {
    let leaf = PalwVertexLeafV1::Held { claim: PalwClaimRefV1::Full(kimi_id()), object: PALW_VERTEX_HELD_OBJECT_CAPTURE_V1, first: 0, last: 0, digest };
    PalwConsensusObjectV2::VerificationVertexV1 {
        vertex: Box::new(PalwVerificationVertexV1::sign_v1(h64(999), bond_key(seat), daa, vec![leaf], dummy_sign).unwrap()),
    }
}

fn state_of(s: &PalwChainStateV2) -> PalwModelLifecycleV1 {
    s.model_lifecycle(&kimi_id()).expect("Kimi's row").state
}

/// Kimi at its `Prefetching` row (DAA 110), then `n` attestations, one vertex each.
fn prefetching(p: &PalwStateParamsV2, root: Hash64, attestations: &[(u64, Hash64)]) -> (PalwChainStateV2, u64) {
    let f = fold(kimi_work());
    let (s1, _) = step(&PalwChainStateV2::genesis(), p, &ctx(1, 100, 1), &network(root), None, Some(f.clone())).unwrap();
    let (mut s, _) = step(&s1, p, &ctx(2, 110, 2), &[], None, Some(f.clone())).unwrap();
    assert_eq!(state_of(&s), PalwModelLifecycleV1::Prefetching, "the premise");
    let mut block = 3;
    for (seat, digest) in attestations {
        let daa = 110 + block;
        s = step(&s, p, &ctx(block, daa, block), &[attests(*seat, daa, *digest)], None, Some(f.clone())).unwrap().0;
        block += 1;
    }
    (s, block)
}

/// The next span boundary at or after `from` (spans are ten DAA).
fn boundary(from: u64) -> u64 {
    from.div_ceil(SPAN) * SPAN
}

/// **Entry**: three seats' equal `Held` leaves on the class are a DA certificate; with it, past the fence, with fewer ready seats than a
/// panel, the next span boundary steps the class to `Capped` at that boundary's DAA. Without any one of the three it stays `Prefetching`.
#[test]
fn a_certified_prefetching_class_steps_to_capped_and_otherwise_stays() {
    let (_, root) = inventory();
    let digest = h64(0xD1);
    let f = fold(kimi_work());
    let certified = [(2u64, digest), (3, digest), (4, digest)];
    let p = cp(true, true, Some(0));
    let (s, next_block) = prefetching(&p, root, &certified);
    assert!(palw_mesh_fold_v1_certificate(&s), "three equal Held leaves are a certificate");
    let at = boundary(110 + next_block) + SPAN;
    let (capped, delta) = step(&s, &p, &ctx(next_block, at, next_block), &[], None, Some(f.clone())).unwrap();
    assert_eq!(state_of(&capped), PalwModelLifecycleV1::Capped { since_daa: at }, "stepped to Capped at the boundary's DAA");
    let row = capped.model_lifecycle(&kimi_id()).unwrap();
    assert_eq!(row.state.admission_permille(), PALW_CAPPED_ADMISSION_PERMILLE_V1);
    assert!(row.state.admits_claims());
    // The delta reproduces and reverts it; the carriage keeps the variant.
    assert_eq!(apply_delta_v2(&s, &delta, &p).unwrap().state_root(), capped.state_root());
    assert_eq!(revert_delta_v2(&capped, &delta, &p).unwrap().state_root(), s.state_root());
    // A capped class stays capped at later boundaries while no holders are seated, and keeps its entry DAA.
    let (later, _) = step(&capped, &p, &ctx(next_block + 1, at + 3 * SPAN, next_block + 1), &[], None, Some(f.clone())).unwrap();
    assert_eq!(state_of(&later), PalwModelLifecycleV1::Capped { since_daa: at }, "still capped, still since the entry");

    // Two seats' attestations are not a certificate; two of three equal are not either.
    let two = [(2u64, digest), (3, digest)];
    let (s2, nb) = prefetching(&p, root, &two);
    let (next, _) = step(&s2, &p, &ctx(nb, boundary(110 + nb) + SPAN, nb), &[], None, Some(f.clone())).unwrap();
    assert_eq!(state_of(&next), PalwModelLifecycleV1::Prefetching, "two attestations are no certificate");
    let split = [(2u64, digest), (3, digest), (4, h64(0xD2))];
    let (s3, nb) = prefetching(&p, root, &split);
    let (next, _) = step(&s3, &p, &ctx(nb, boundary(110 + nb) + SPAN, nb), &[], None, Some(f.clone())).unwrap();
    assert_eq!(state_of(&next), PalwModelLifecycleV1::Prefetching, "three different digests are no certificate");
    // Below the capped fence the certificate is read by nothing: the class stays where it always did.
    let dormant = cp(true, true, None);
    let (s4, nb) = prefetching(&dormant, root, &certified);
    let (next, _) = step(&s4, &dormant, &ctx(nb, boundary(110 + nb) + SPAN, nb), &[], None, Some(f.clone())).unwrap();
    assert_eq!(state_of(&next), PalwModelLifecycleV1::Prefetching, "no fence, no Capped");
    // And a class-level attestation below the capped fence is not even recorded.
    assert!(s4.vertex.held.get(&kimi_id()).is_none(), "nothing recorded below the fence");
}

fn palw_mesh_fold_v1_certificate(s: &PalwChainStateV2) -> bool {
    super::super::palw_mesh_fold_v1::palw_class_da_certificate_v1(s, &kimi_id())
}

/// **A capped claim**: a claim of a `Capped` class is accepted without a panel room; it owes no bind deadline while its class is capped
/// (it is not voided by the bind window), and its row lives exactly while it waits `Provisional`.
#[test]
fn a_capped_class_admits_a_claim_that_waits_for_its_holders() {
    let (_, root) = inventory();
    let digest = h64(0xD1);
    let f = fold(kimi_work());
    let p = cp(true, true, Some(0));
    let (s, nb) = prefetching(&p, root, &[(2, digest), (3, digest), (4, digest)]);
    let at = boundary(110 + nb) + SPAN;
    let (capped, _) = step(&s, &p, &ctx(nb, at, nb), &[], None, Some(f.clone())).unwrap();
    // A claim of a class still `Prefetching` is refused; of the capped class, accepted.
    // (DAA 118 is inside the span the premise's attestations are in: no boundary steps the class first.)
    let refused = step(&s, &p, &ctx(nb, 118, nb), &[], Some(&kimi_attempt(1, root)), Some(f.clone()));
    assert!(refused.is_err(), "a Prefetching class admits no claim");
    let env = kimi_attempt(1, root);
    let claim = attempt_id_v2(&env.attempt);
    let (s1, _) = step(&capped, &p, &ctx(nb + 1, at + 1, nb + 1), &[], Some(&env), Some(f.clone())).expect("a capped class admits a claim");
    let row = s1.mesh_capped_row_v1(&claim).expect("the claim's capped row");
    assert_eq!((row.class_id, row.accepted_daa, row.window_end_daa), (kimi_id(), at + 1, None));
    s1.assert_deadline_consistency(&p).expect("the derived deadline is the index's");
    assert!(matches!(s1.claim(&claim).unwrap().phase, PalwClaimPhaseV2::Provisional));
    // The bind window passes and the claim is not voided: it waits.
    let (waited, _) = step(&s1, &p, &ctx(nb + 2, at + 1 + p.window_bind() + 500, nb + 2), &[], None, Some(f.clone())).unwrap();
    assert!(matches!(waited.claim(&claim).unwrap().phase, PalwClaimPhaseV2::Provisional), "a capped claim is not voided by the bind window");
    waited.assert_deadline_consistency(&p).expect("and the deadline index agrees");
    // The same claim on the same chain below the capped fence cannot exist: the class is not admitting.
    let dormant = cp(true, true, None);
    let (d, nbd) = prefetching(&dormant, root, &[(2, digest), (3, digest), (4, digest)]);
    assert!(step(&d, &dormant, &ctx(nbd, boundary(110 + nbd) + SPAN, nbd), &[], Some(&env), Some(f)).is_err());
}

/// **The re-verification window**: when holders are seated the class leaves `Capped` (to `Probation`), every capped claim still waiting gets
/// a window of `PALW_CAPPED_REVERIFY_WINDOW_DAA_V1` from that boundary, and a claim that has not bound a panel by the window's end is
/// voided like any claim that never bound — its escrow never minted, nobody slashed.
#[test]
fn holders_seated_open_the_reverification_window_and_an_unverified_claim_is_voided() {
    let (operands, root) = inventory();
    let digest = h64(0xD1);
    let f = fold(kimi_work());
    let p = cp(true, true, Some(0));
    let (s, nb) = prefetching(&p, root, &[(2, digest), (3, digest), (4, digest)]);
    let at = boundary(110 + nb) + SPAN;
    let (capped, _) = step(&s, &p, &ctx(nb, at, nb), &[], None, Some(f.clone())).unwrap();
    let env = kimi_attempt(1, root);
    let claim = attempt_id_v2(&env.attempt);
    let (s1, _) = step(&capped, &p, &ctx(nb + 1, at + 1, nb + 1), &[], Some(&env), Some(f.clone())).unwrap();
    // Seven holders prove possession in the span before the next boundary.
    let proof_daa = at + 2 * SPAN + 1;
    let span = proof_daa / SPAN;
    let proofs: Vec<PalwConsensusObjectV2> = (2..=8).map(|n| proof(&operands, bond_key(n), span)).collect();
    let (s2, _) = step(&s1, &p, &ctx(nb + 2, proof_daa, nb + 2), &proofs, None, Some(f.clone())).expect("the proofs fold");
    let exit_at = at + 3 * SPAN;
    let (left, _) = step(&s2, &p, &ctx(nb + 3, exit_at, nb + 3), &[], None, Some(f.clone())).unwrap();
    assert!(!matches!(state_of(&left), PalwModelLifecycleV1::Capped { .. }), "the class left Capped: {:?}", state_of(&left));
    assert_eq!(state_of(&left), PalwModelLifecycleV1::Probation { probes_passed: 0 }, "where Prefetching would have put it");
    let row = left.mesh_capped_row_v1(&claim).expect("the claim still waits");
    assert_eq!(row.window_end_daa, Some(exit_at + PALW_CAPPED_REVERIFY_WINDOW_DAA_V1), "its re-verification window opened at the boundary");
    left.assert_deadline_consistency(&p).expect("the deadline moved with the row");
    // At the window's end the claim, never bound, is voided; no row remains; nobody was slashed.
    let end = exit_at + PALW_CAPPED_REVERIFY_WINDOW_DAA_V1 + 1;
    let before = left.bond(&bond_key(1)).unwrap().collateral;
    let (done, _) = step(&left, &p, &ctx(nb + 4, end, nb + 4), &[], None, Some(f)).unwrap();
    assert!(matches!(done.claim(&claim).unwrap().phase, PalwClaimPhaseV2::Voided { .. }), "an unverified capped claim is voided");
    assert!(done.mesh_capped_row_v1(&claim).is_none(), "its row left with it");
    assert_eq!(done.bond(&bond_key(1)).unwrap().collateral, before, "the producer is not slashed");
}

/// **The weight cap**: a capped class's open claims hold at most `w_cap` of the immature weight; past it the gate refuses the next claim.
#[test]
fn the_weight_cap_refuses_a_claim_once_the_capped_classes_hold_more_than_w_cap() {
    let (_, root) = inventory();
    let digest = h64(0xD1);
    let f = fold(kimi_work());
    let p = cp(true, true, Some(0));
    let (s, nb) = prefetching(&p, root, &[(2, digest), (3, digest), (4, digest)]);
    let at = boundary(110 + nb) + SPAN;
    let (capped, _) = step(&s, &p, &ctx(nb, at, nb), &[], None, Some(f.clone())).unwrap();
    let env = kimi_attempt(1, root);
    let claim = attempt_id_v2(&env.attempt);
    let (mut s1, _) = step(&capped, &p, &ctx(nb + 1, at + 1, nb + 1), &[], Some(&env), Some(f)).unwrap();
    let gate = |s: &PalwChainStateV2| super::super::palw_mesh_fold_v1::capped_class_admits_v1(s);
    // The capped claim holds 100 of immature weight; the chain holds 10,000 in all: 1 % exactly — admitted; 9,999: refused.
    s1.claims.get_mut(&claim).unwrap().immature_contribution = 100;
    s1.bounded_immature = 10_000;
    assert!(gate(&s1).is_ok(), "exactly w_cap is within it");
    s1.bounded_immature = 9_999;
    let why = gate(&s1).expect_err("past w_cap").to_string();
    assert!(why.contains("past w_cap of 10"), "{why}");
    // No capped weight, no refusal, whatever the total.
    s1.claims.get_mut(&claim).unwrap().immature_contribution = 0;
    s1.bounded_immature = 1;
    assert!(gate(&s1).is_ok());
    assert_eq!(PALW_CAPPED_WEIGHT_CAP_PERMILLE_V1, 10, "1 %");
}

/// **The holders re-verify through the ordinary path**: once the class has left `Capped`, a capped claim that binds a panel (its holders
/// are the panel) drops its capped row and is licensed by the seats' receipts and finalized exactly as any claim is — its reward is paid
/// at `Final`, which is what "provisional" meant.
#[test]
fn a_capped_claim_the_holders_license_finalizes_on_the_ordinary_path() {
    let (operands, root) = inventory();
    let digest = h64(0xD1);
    let f = fold(kimi_work());
    let p = cp(true, true, Some(0));
    let (s, nb) = prefetching(&p, root, &[(2, digest), (3, digest), (4, digest)]);
    let at = boundary(110 + nb) + SPAN;
    let (capped, _) = step(&s, &p, &ctx(nb, at, nb), &[], None, Some(f.clone())).unwrap();
    let env = kimi_attempt(1, root);
    let claim = attempt_id_v2(&env.attempt);
    let (s1, _) = step(&capped, &p, &ctx(nb + 1, at + 1, nb + 1), &[], Some(&env), Some(f.clone())).unwrap();
    let proof_daa = at + 2 * SPAN + 1;
    let proofs: Vec<PalwConsensusObjectV2> = (2..=8).map(|n| proof(&operands, bond_key(n), proof_daa / SPAN)).collect();
    let (s2, _) = step(&s1, &p, &ctx(nb + 2, proof_daa, nb + 2), &proofs, None, Some(f.clone())).unwrap();
    let (left, _) = step(&s2, &p, &ctx(nb + 3, at + 3 * SPAN, nb + 3), &[], None, Some(f.clone())).unwrap();
    assert!(left.mesh_capped_row_v1(&claim).is_some(), "the premise: the claim waits in its window");
    // The holders' panel binds (the derived panel's own validation is the acceptance layer's; the fold takes the object).
    let bind_daa = at + 3 * SPAN + 1;
    let seats = vec![PalwPanelSeatV2 { bond: bond_key(2), operator_id: op_id(22) }];
    let (bound, _) = step(
        &left,
        &p,
        &ctx(nb + 4, bind_daa, nb + 4),
        &[PalwConsensusObjectV2::PanelBound { claim, anchor: h64(77), seats }],
        None,
        Some(f.clone()),
    )
    .expect("a panel binds the capped claim once its class has holders");
    assert!(matches!(bound.claim(&claim).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }));
    assert!(bound.mesh_capped_row_v1(&claim).is_none(), "the capped row leaves the moment the claim binds");
    bound.assert_deadline_consistency(&p).expect("the receipt window is the ordinary one");
    // The ordinary licence, then `Final`.
    let (licensed, _) = step(
        &bound,
        &p,
        &ctx(nb + 5, bind_daa + 1, nb + 5),
        &[PalwConsensusObjectV2::ReceiptLicensed { claim, receipts: seat_says(true) }],
        None,
        Some(f.clone()),
    )
    .expect("the holders' receipts license it");
    assert!(matches!(licensed.claim(&claim).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    let (done, _) = step(&licensed, &p, &ctx(nb + 6, bind_daa + 200, nb + 6), &[], None, Some(f)).unwrap();
    assert!(matches!(done.claim(&claim).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "and it reaches Final like any claim");
}
