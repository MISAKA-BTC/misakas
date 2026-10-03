//! **RFC-0007 Part I through the fold** — verification vertices, licence by tally, equivocation, `Held` leaves and the path rule
//! (spec 18), on the R-core+ door fixtures (five sybil seats on one panel, testnet-12's windows). Fences: the vertex at DAA 0 (the
//! panel binds at 102, so the claim licenses by tally) unless a test says otherwise.
//!
//! The fold does not verify signatures — the acceptance walk does, once per vertex (`consensus`'s processor tests drive that half) —
//! so a vertex here carries a dummy signature of the right length.

use super::*;
use crate::palw_panel_v2::PalwReceiptVerdictV2 as Verdict;
use crate::palw_vertex_v1::*;

const COLLATERAL: u64 = 1_000_000_000;

fn vp(fence: Option<u64>) -> PalwStateParamsV2 {
    PalwStateParamsV2::new(100, 10, 600, 120, 500, 1000, h64(1), 4, 1000, 100, 1000, 0)
        .unwrap()
        .with_fp_quanta(8, 64)
        .unwrap()
        .with_rcore_plus_mirrors(Some(0), 0, Vec::new())
        .with_vertex_from_daa(fence)
}

fn vx() -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 { panel_economy_active: true, ..door_extras(true) }
}

fn seats() -> Vec<PalwBondKeyV2> {
    sybil_seats().iter().map(|seat| seat.bond).collect()
}

fn dummy_sign(message: &[u8], context: &[u8]) -> Option<Vec<u8>> {
    assert_eq!(context, PALW_VERTEX_MLDSA87_CONTEXT_V1);
    let mut signature = message.to_vec();
    signature.resize(crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN, 0);
    Some(signature)
}

fn leaf(claim: Hash64, verdict: Verdict) -> PalwVertexLeafV1 {
    PalwVertexLeafV1::Verdict { claim: PalwClaimRefV1::Full(claim), verdict }
}

fn vertex(seat: PalwBondKeyV2, signed_daa: u64, leaves: Vec<PalwVertexLeafV1>) -> PalwVerificationVertexV1 {
    PalwVerificationVertexV1::sign_v1(h64(999), seat, signed_daa, leaves, dummy_sign).expect("a well-formed vertex signs")
}

fn object(v: PalwVerificationVertexV1) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::VerificationVertexV1 { vertex: Box::new(v) }
}

fn step(
    parent: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    block_word: u64,
    daa: u64,
    objects: &[PalwConsensusObjectV2],
) -> Result<(PalwChainStateV2, PalwStateDeltaV2), PalwStateV2Error> {
    let applied = apply_palw_transition_v2_with_extras(
        parent,
        p,
        &ctx(block_word, daa, block_word),
        objects,
        None,
        true,
        false,
        false,
        false,
        &vx(),
    )?;
    applied.0.assert_internal_consistency(p).expect("internal consistency after apply");
    applied.0.assert_deadline_consistency(p).expect("deadline consistency after apply");
    // The delta reproduces the fold and reverts to the parent.
    assert_eq!(apply_delta_v2(parent, &applied.1, p).unwrap().state_root(), applied.0.state_root(), "the delta reproduces the fold");
    assert_eq!(
        revert_delta_v2(&applied.0, &applied.1, p).unwrap().state_root(),
        parent.state_root(),
        "the delta reverts to the parent"
    );
    Ok(applied)
}

fn world(fence: Option<u64>) -> (PalwStateParamsV2, PalwChainStateV2, Hash64) {
    let p = vp(fence);
    let (s, claim) = door_claim_bound(&p, &vx(), |_| COLLATERAL);
    (p, s, claim)
}

fn phase(s: &PalwChainStateV2, claim: Hash64) -> PalwClaimPhaseV2 {
    s.claim(&claim).expect("the claim").phase.clone()
}

/// Every seat of the panel says `verdict` on `claim`, one vertex a block at DAA `daa0 + i`; returns the state after the first `n`.
fn seats_say(
    p: &PalwStateParamsV2,
    mut s: PalwChainStateV2,
    claim: Hash64,
    n: usize,
    daa0: u64,
    verdict: Verdict,
) -> PalwChainStateV2 {
    for (i, seat) in seats().into_iter().take(n).enumerate() {
        let daa = daa0 + i as u64;
        let v = vertex(seat, daa, vec![leaf(claim, verdict)]);
        s = step(&s, p, 10 + i as u64, daa, &[object(v)]).expect("the vertex is folded").0;
    }
    s
}

/// **The honest path: licence by tally.** Five seats each sign one vertex carrying their `Valid`; the claim stays `PanelBound`
/// until the counted leaves license it (the coverage door needs every segment attested twice, which is the full seat and the four
/// partial seats), the licence is the one a carried `ReceiptLicensedV2` of the same seats writes, and no licence object is carried.
#[test]
fn the_licence_is_the_tally() {
    let (p, s0, claim) = world(Some(0));
    assert!(matches!(phase(&s0, claim), PalwClaimPhaseV2::PanelBound { .. }));
    // Two and then three `Valid`s: counted, not yet a licence (the coverage door needs the whole cut).
    let s2 = seats_say(&p, s0.clone(), claim, 2, 104, Verdict::Valid);
    assert_eq!(s2.vertex_tally_of_v1(&claim).expect("a tally").valid(), 2);
    assert!(matches!(phase(&s2, claim), PalwClaimPhaseV2::PanelBound { .. }));
    let s4 = seats_say(&p, s0.clone(), claim, 4, 104, Verdict::Valid);
    assert!(matches!(phase(&s4, claim), PalwClaimPhaseV2::PanelBound { .. }), "four of five do not cover every segment twice");
    // The fifth completes the cut: the fold licenses it.
    let s5 = seats_say(&p, s0.clone(), claim, 5, 104, Verdict::Valid);
    assert!(matches!(phase(&s5, claim), PalwClaimPhaseV2::ReceiptLicensed { .. }), "the tally licensed the claim");
    assert!(s5.vertex_tally_of_v1(&claim).is_none(), "a licensed claim holds no tally");
    // The licence is the receipt path's: the same door record, the same locks.
    let record = s5.claim(&claim).unwrap().rcore;
    assert_eq!(record.licence_door, Some(crate::palw_economic_safety_v1::PalwLicenceDoorTagV1::Coverage));
    assert!(record.basis_k >= 2);
    for seat in seats() {
        assert!(s5.slashable_lock(seat, claim).is_some(), "every Valid seat holds a lock");
        assert!(s5.panel_duties_of(&claim).and_then(|row| row.get(&seat)).is_some_and(|at| *at != 0), "and is credited");
    }
    // One row per `(round, seat)`, and the round rows reproduce through the carriage.
    assert_eq!(s5.vertex_counts_v1().0, 5);
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&s5)).unwrap();
    let back = borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap().into_state(&p, Some(s5.state_root())).expect("a restart");
    assert_eq!(back.state_root(), s5.state_root());
    assert_eq!(back.vertex_counts_v1(), s5.vertex_counts_v1());

    // The twin by receipts: the same five seats carried as a `ReceiptLicensedV2` on a claim bound BEFORE the fence license it the same
    // way — and so the tally's licence equals the receipt path's licence, record for record.
    let (pr, sr, claim_r) = world(None);
    let seats = seats();
    let assignment = crate::palw_verification_v2::palw_segment_assignment_v2(h64(77), claim_r, 5);
    let receipts: Vec<_> = seats
        .iter()
        .enumerate()
        .map(|(i, seat)| crate::palw_panel_v2::PalwSeatReceiptV3 {
            receipt: valid_receipt(claim_r, *seat, 104),
            segments: assignment.mask_of(i as u16),
        })
        .collect();
    let (by_receipts, _) = step(&sr, &pr, 30, 105, &[PalwConsensusObjectV2::ReceiptLicensedV2 { claim: claim_r, receipts }])
        .expect("the receipts license");
    assert_eq!(by_receipts.claim(&claim_r).unwrap().rcore.licence_door, s5.claim(&claim).unwrap().rcore.licence_door, "the same door");
    assert_eq!(by_receipts.claim(&claim_r).unwrap().rcore.basis_k, record.basis_k, "the same recount");
}

/// A verdict stands (PALW-VC-4): a seat's first counted verdict is its verdict, whatever it says later, and a seat counts once.
#[test]
fn a_verdict_stands_and_a_seat_counts_once() {
    let (p, s0, claim) = world(Some(0));
    let seat = seats()[0];
    let abstain = Verdict::Unavailable { chunk_index: 0, requested_daa: 103 };
    let (s1, _) = step(&s0, &p, 10, 104, &[object(vertex(seat, 104, vec![leaf(claim, abstain)]))]).unwrap();
    assert_eq!(s1.vertex_tally_of_v1(&claim).unwrap().counted.len(), 1);
    // A later vertex of the same seat says `Valid`: ignored, the first stands.
    let (s2, _) = step(&s1, &p, 11, 105, &[object(vertex(seat, 105, vec![leaf(claim, Verdict::Valid)]))]).unwrap();
    let tally = s2.vertex_tally_of_v1(&claim).unwrap();
    assert_eq!(tally.counted.len(), 1, "the seat counts once");
    assert_eq!(tally.counted[0].verdict, abstain, "and its first verdict stands");
    assert_eq!(tally.valid(), 0);
}

/// Leaves that do not count are ignored, never refused: a seat not on the panel, a claim that does not exist, a signing DAA outside
/// the receipt window, `Incapable` on the liveness floor. The vertex itself is still the seat's one vertex of the round.
#[test]
fn a_leaf_that_does_not_count_is_ignored_and_the_vertex_stands() {
    let (p, s0, claim) = world(Some(0));
    let outsider = bond_key(50);
    // A bond that is registered but holds no seat on the panel.
    let mut registered = s0.clone();
    let (with_outsider, _) =
        apply_door(&registered, &p, &ctx(8, 103, 8), &[seat_bond_reg(50, COLLATERAL)], None, &vx()).expect("a bond registers");
    registered = with_outsider;
    let stranger = vertex(outsider, 104, vec![leaf(claim, Verdict::Valid)]);
    let (s1, _) = step(&registered, &p, 10, 104, &[object(stranger)]).unwrap();
    assert!(s1.vertex_tally_of_v1(&claim).is_none(), "a seat the panel does not hold counts for nothing");
    assert!(s1.vertex_round_row_v1(104, &outsider).is_some(), "but its vertex is its round's");
    // A claim no panel has, and a window the claim is not in: ignored.
    let seat = seats()[0];
    let ghost = h64(0xDEAD);
    let (s2, _) = step(&s0, &p, 10, 104, &[object(vertex(seat, 104, vec![leaf(ghost, Verdict::Valid)]))]).unwrap();
    assert_eq!(s2.vertex_counts_v1(), (1, 0, 0));
    let early = vertex(seat, 101, vec![leaf(claim, Verdict::Valid)]);
    let (s3, _) = step(&s0, &p, 10, 104, &[object(early)]).unwrap();
    assert!(s3.vertex_tally_of_v1(&claim).is_none(), "signed before the panel bound: outside the receipt window");
    let late = vertex(seat, 800, vec![leaf(claim, Verdict::Valid)]);
    let (s4, _) = step(&s0, &p, 10, 800, &[object(late)]).unwrap();
    assert!(s4.vertex_tally_of_v1(&claim).is_none(), "signed past the receipt deadline");
}

/// **Hostile vertices are refused by name by the fold's second lock**, each with its own reason.
#[test]
fn hostile_vertices_are_refused_by_name() {
    let (p, s0, claim) = world(Some(0));
    let seat = seats()[0];
    let good = vertex(seat, 104, vec![leaf(claim, Verdict::Valid)]);
    let (s1, _) = step(&s0, &p, 10, 104, &[object(good.clone())]).unwrap();
    let refused = |state: &PalwChainStateV2, daa: u64, v: PalwVerificationVertexV1| {
        step(state, &p, 20, daa, &[object(v)]).expect_err("refused").to_string()
    };
    // One vertex per seat per round: a second of the same round, whatever it says.
    let second = vertex(seat, 104, vec![leaf(claim, Verdict::Incapable)]);
    assert!(refused(&s1, 104, second).contains("one vertex per seat per round"));
    assert!(refused(&s1, 104, good).contains("already on the chain"));
    // Signed after the block that carries it; older than the carry window; from an unregistered bond.
    assert!(refused(&s0, 104, vertex(seat, 105, vec![leaf(claim, Verdict::Valid)])).contains("signed after the block"));
    assert!(
        refused(&s0, 104 + PALW_VERTEX_MAX_CARRY_DAA_V1 + 1, vertex(seat, 104, vec![leaf(claim, Verdict::Valid)]))
            .contains("older than")
    );
    assert!(refused(&s0, 104, vertex(bond_key(77), 104, vec![leaf(claim, Verdict::Valid)])).contains("not registered"));
    // A malformed vertex (a root that does not recompute, an unsorted leaf list, an audit leaf).
    let mut bad_root = vertex(seat, 104, vec![leaf(claim, Verdict::Valid)]);
    bad_root.leaves_root = h64(1);
    assert!(refused(&s0, 104, bad_root).contains("root does not recompute"));
    let mut audited = vertex(seat, 104, vec![leaf(claim, Verdict::Valid)]);
    audited.leaves = vec![PalwVertexLeafV1::Audited { claim: PalwClaimRefV1::Full(claim), leaf: 1, result: 0 }];
    audited.leaves_root = palw_vertex_root_of_leaves_v1(&audited.leaves).unwrap();
    assert!(refused(&s0, 104, audited).contains("audit"));
    // Below the fence the object is the fold's refusal by name, and the processor's drop.
    let (pd, sd, claim_d) = world(None);
    let v = vertex(seat, 104, vec![leaf(claim_d, Verdict::Valid)]);
    let why = step(&sd, &pd, 10, 104, &[object(v)]).expect_err("dormant").to_string();
    assert!(why.contains("below palw_verification_vertex_v1"), "{why}");
    assert!(palw_object_is_vertex_v1(&object(vertex(seat, 104, vec![leaf(claim, Verdict::Valid)]))));
}

/// **The path rule (RFC-0007 migration, open question 4).** A claim bound across the fence licenses on the old path: its panel bound
/// before the fence, so its leaves count for nothing and its receipts license it; a claim bound at or after the fence refuses a
/// receipt licence by name and licenses by tally.
#[test]
fn a_claim_bound_across_the_fence_licenses_on_the_old_path() {
    // The fence is above the bind (102): the claim is the old path's.
    let (p, s0, claim) = world(Some(500));
    // The vertex is not active below the fence (the fold refuses it by name); carry the five leaves at the fence instead.
    let mut s = s0.clone();
    for (i, seat) in seats().into_iter().enumerate() {
        let daa = 500 + i as u64;
        let (next, _) = step(&s, &p, 10 + i as u64, daa, &[object(vertex(seat, daa, vec![leaf(claim, Verdict::Valid)]))]).unwrap();
        s = next;
    }
    assert!(s.vertex_tally_of_v1(&claim).is_none(), "the claim's panel bound before the fence: no leaf counts");
    assert!(matches!(phase(&s, claim), PalwClaimPhaseV2::PanelBound { .. }), "five leaves license nothing on the old path's claim");
    // Its receipts license it (the receipt window is open until 702).
    let assignment = crate::palw_verification_v2::palw_segment_assignment_v2(h64(77), claim, 5);
    let receipts: Vec<_> = seats()
        .iter()
        .enumerate()
        .map(|(i, seat)| crate::palw_panel_v2::PalwSeatReceiptV3 {
            receipt: valid_receipt(claim, *seat, 510),
            segments: assignment.mask_of(i as u16),
        })
        .collect();
    let (licensed, _) = step(&s, &p, 40, 511, &[PalwConsensusObjectV2::ReceiptLicensedV2 { claim, receipts: receipts.clone() }])
        .expect("the old path licenses");
    assert!(matches!(phase(&licensed, claim), PalwClaimPhaseV2::ReceiptLicensed { .. }));

    // A claim bound at the fence refuses the receipt object by name (the fold's second lock), and the licence by tally works.
    let (p2, s2, claim2) = world(Some(0));
    let receipts2: Vec<_> = receipts
        .iter()
        .map(|r| {
            let mut r = r.clone();
            r.receipt.claim = claim2;
            r.receipt.signed_daa = 104;
            r
        })
        .collect();
    let why = step(&s2, &p2, 41, 105, &[PalwConsensusObjectV2::ReceiptLicensedV2 { claim: claim2, receipts: receipts2 }])
        .expect_err("the receipt path is closed for this claim")
        .to_string();
    assert!(why.contains("licenses by tally"), "{why}");
    let by_tally = seats_say(&p2, s2, claim2, 5, 104, Verdict::Valid);
    assert!(matches!(phase(&by_tally, claim2), PalwClaimPhaseV2::ReceiptLicensed { .. }));
}

/// **Equivocation is caught and slashed** (RFC-0007 §I.6): a seat signs two vertices of one round with different roots; anyone carries
/// the evidence; the fold slashes 100 ‰ of the bond, forfeits the seat's lock on every claim the vertices name, ejects the bond, and
/// convicts the pair once.
#[test]
fn an_equivocating_seat_is_slashed_its_locks_forfeited_and_the_bond_ejected() {
    let (p, s0, claim) = world(Some(0));
    let licensed = seats_say(&p, s0, claim, 5, 104, Verdict::Valid);
    let seat = seats()[2];
    let lock = *licensed.slashable_lock(seat, claim).expect("the seat holds a lock on the licensed claim");
    let before = licensed.bond(&seat).unwrap().clone();
    assert!(matches!(before.status, PalwBondStatusV2::Active));
    // Round 150: the seat signs two vertices, one naming the claim, one saying nothing of it.
    let a = vertex(seat, 150, vec![leaf(claim, Verdict::Valid)]);
    let b = vertex(
        seat,
        150,
        vec![PalwVertexLeafV1::Held { claim: PalwClaimRefV1::Full(h64(0xAB)), object: 0, first: 0, last: 1, digest: h64(2) }],
    );
    assert_ne!(a.leaves_root, b.leaves_root);
    // The first lands as the round's vertex.
    let (s1, _) = step(&licensed, &p, 50, 151, &[object(a.clone())]).expect("the first vertex of the round lands");
    // The evidence carries both headers, and the leaves of the side that names the claim.
    let evidence = PalwVertexEquivocationV1 { a: a.header(), b: b.header(), a_leaves: a.leaves.clone(), b_leaves: Vec::new() };
    let (s2, _) = step(&s1, &p, 51, 152, &[PalwConsensusObjectV2::VertexEquivocationV1 { evidence: Box::new(evidence.clone()) }])
        .expect("the equivocation is convicted");
    let after = s2.bond(&seat).unwrap();
    let penalty = u64::try_from(
        palw_vertex_permille_of_v1(u128::from(before.collateral), PALW_VERTEX_EQUIVOCATION_PENALTY_PERMILLE_V1).unwrap(),
    )
    .unwrap();
    let forfeited = u64::try_from(lock.amount).unwrap();
    assert_eq!(
        before.collateral - after.collateral,
        (penalty + forfeited).min(before.collateral),
        "100 ‰ of the bond plus the forfeited lock"
    );
    assert_eq!(after.slashed - before.slashed, before.collateral - after.collateral, "burned, recorded");
    assert!(s2.slashable_lock(seat, claim).is_none(), "the lock on the named claim is forfeited");
    assert!(matches!(after.status, PalwBondStatusV2::Retiring { .. }), "the bond is ejected: it takes no new work");
    assert!(s2.vertex_round_row_v1(150, &seat).unwrap().convicted);
    // The pair is convicted once; a vertex of that round cannot land again.
    let again =
        step(&s2, &p, 52, 153, &[PalwConsensusObjectV2::VertexEquivocationV1 { evidence: Box::new(evidence) }]).expect_err("once");
    assert!(again.to_string().contains("already convicted"), "{again}");
    // Hostile evidence is refused by name: one vertex twice, two seats, two rounds, too old, an unregistered bond.
    let refused = |evidence: PalwVertexEquivocationV1| {
        step(&s1, &p, 60, 152, &[PalwConsensusObjectV2::VertexEquivocationV1 { evidence: Box::new(evidence) }])
            .expect_err("refused")
            .to_string()
    };
    let same = PalwVertexEquivocationV1 { a: a.header(), b: a.header(), a_leaves: Vec::new(), b_leaves: Vec::new() };
    assert!(refused(same).contains("same leaf root"));
    let other_seat = vertex(seats()[3], 150, vec![leaf(claim, Verdict::Incapable)]);
    let two_seats = PalwVertexEquivocationV1 { a: a.header(), b: other_seat.header(), a_leaves: Vec::new(), b_leaves: Vec::new() };
    assert!(refused(two_seats).contains("different seats"));
    let other_round = vertex(seat, 151, vec![leaf(claim, Verdict::Incapable)]);
    let two_rounds = PalwVertexEquivocationV1 { a: a.header(), b: other_round.header(), a_leaves: Vec::new(), b_leaves: Vec::new() };
    assert!(refused(two_rounds).contains("different rounds"));
    let mut lying = PalwVertexEquivocationV1 { a: a.header(), b: b.header(), a_leaves: b.leaves.clone(), b_leaves: Vec::new() };
    assert!(refused(lying.clone()).contains("do not match"), "leaves that are not the header's");
    lying.a_leaves.clear();
    let old_a = vertex(seat, 150, vec![leaf(claim, Verdict::Valid)]);
    let stale = step(
        &s1,
        &p,
        61,
        150 + PALW_VERTEX_EVIDENCE_WINDOW_DAA_V1 + 1,
        &[PalwConsensusObjectV2::VertexEquivocationV1 {
            evidence: Box::new(PalwVertexEquivocationV1 {
                a: old_a.header(),
                b: b.header(),
                a_leaves: Vec::new(),
                b_leaves: Vec::new(),
            }),
        }],
    )
    .expect_err("too old");
    assert!(stale.to_string().contains("stays provable") || stale.to_string().contains("past the"), "{stale}");
}

/// `Held` leaves record a seat's attestation once per range, count toward a DA certificate at quorum, and are charged when the claim's
/// data is concluded not served; the rows go with the claim.
#[test]
fn held_leaves_make_a_da_certificate_and_attesters_are_charged_on_a_default() {
    let (p, s0, claim) = world(Some(0));
    let held = |first, last, digest| PalwVertexLeafV1::Held {
        claim: PalwClaimRefV1::Full(claim),
        object: PALW_VERTEX_HELD_OBJECT_WITNESS_V1,
        first,
        last,
        digest,
    };
    let mut s = s0;
    for (i, seat) in seats().into_iter().take(3).enumerate() {
        let (next, _) =
            step(&s, &p, 10 + i as u64, 104 + i as u64, &[object(vertex(seat, 104 + i as u64, vec![held(0, 7, h64(0xAA))]))]).unwrap();
        s = next;
    }
    assert_eq!(s.vertex_held_of_v1(&claim).unwrap().len(), 3);
    let cert = palw_vertex_da_certificate_v1(&s, &claim, PALW_VERTEX_HELD_OBJECT_WITNESS_V1, 0, 7, &h64(0xAA))
        .expect("three equal Held leaves");
    assert_eq!(cert.len(), usize::from(PALW_VERTEX_DA_QUORUM_V1));
    assert!(
        palw_vertex_da_certificate_v1(&s, &claim, PALW_VERTEX_HELD_OBJECT_WITNESS_V1, 0, 7, &h64(0xAB)).is_none(),
        "another digest is no certificate"
    );
    assert!(
        palw_vertex_da_certificate_v1(&s, &claim, PALW_VERTEX_HELD_OBJECT_CAPTURE_V1, 0, 7, &h64(0xAA)).is_none(),
        "another object either"
    );
    // Two seats: not a certificate.
    let mut two = s.clone();
    two.vertex.held.get_mut(&claim).unwrap().pop();
    assert!(palw_vertex_da_certificate_v1(&two, &claim, PALW_VERTEX_HELD_OBJECT_WITNESS_V1, 0, 7, &h64(0xAA)).is_none());
    // A seat attests a range once: a second leaf of the same range with another digest is ignored.
    let seat = seats()[0];
    let (s_again, _) = step(&s, &p, 30, 110, &[object(vertex(seat, 110, vec![held(0, 7, h64(0xFF))]))]).unwrap();
    assert_eq!(s_again.vertex_held_of_v1(&claim).unwrap().len(), 3, "the first attestation of a range stands");
    // The attesters are the seats that attested; the charge is their `Held` exposure, once, and the rows are marked.
    let attesters = palw_vertex_fold_attesters_for_test(&s, &claim);
    assert_eq!(attesters.len(), 3);
    let extras = vx();
    let mut builder = TransitionBuilder::new(&s, &p, true, false, false, false, &extras);
    super::super::palw_vertex_fold_v1::charge_held_attesters_v1(&mut builder, &claim, &attesters).expect("the charge");
    let (charged, _) = builder.checkpoint();
    for seat in &attesters {
        let before = s.bond(seat).unwrap().collateral;
        let exposure =
            u64::try_from(palw_vertex_permille_of_v1(u128::from(before), PALW_VERTEX_HELD_EXPOSURE_PERMILLE_V1).unwrap()).unwrap();
        assert_eq!(before - charged.bond(seat).unwrap().collateral, exposure, "5 ‰ of the attester's collateral");
    }
    assert!(charged.vertex_held_of_v1(&claim).unwrap().iter().all(|row| row.charged), "charged once: the rows are marked");
    assert!(palw_vertex_fold_attesters_for_test(&charged, &claim).is_empty(), "and not charged again");
}

fn palw_vertex_fold_attesters_for_test(state: &PalwChainStateV2, claim: &Hash64) -> Vec<PalwBondKeyV2> {
    super::super::palw_vertex_fold_v1::held_attesters_to_charge_v1(state, claim)
}

/// A compact reference names the one claim bound at that DAA with that prefix; a leaf by a compact reference counts exactly as the full one;
/// an unknown or ambiguous reference is ignored.
#[test]
fn a_compact_reference_counts_as_the_full_one_and_an_unknown_one_is_ignored() {
    let (p, s0, claim) = world(Some(0));
    let panel_bound = s0.panel(&claim).unwrap().bound_daa;
    let compact = PalwClaimRefV1::compact_of(&claim, panel_bound).unwrap();
    let seat = seats()[0];
    let v = vertex(
        seat,
        104,
        vec![
            PalwVertexLeafV1::Verdict { claim: compact, verdict: Verdict::Valid },
            PalwVertexLeafV1::Verdict {
                claim: PalwClaimRefV1::Compact { bound_daa: 12_345, id_prefix: [7; 16] },
                verdict: Verdict::Valid,
            },
        ],
    );
    let (s1, _) = step(&s0, &p, 10, 104, &[object(v)]).unwrap();
    let tally = s1.vertex_tally_of_v1(&claim).expect("the compact leaf counted");
    assert_eq!(tally.counted.len(), 1);
    assert_eq!(s1.vertex_counts_v1().1, 1, "the unknown reference counted for nothing");
    // The licence by tally works through compact references alone (the carriage per licence is the point).
    let mut s = s0;
    for (i, seat) in seats().into_iter().enumerate() {
        let v = vertex(seat, 104 + i as u64, vec![PalwVertexLeafV1::Verdict { claim: compact, verdict: Verdict::Valid }]);
        s = step(&s, &p, 10 + i as u64, 104 + i as u64, &[object(v)]).unwrap().0;
    }
    assert!(matches!(phase(&s, claim), PalwClaimPhaseV2::ReceiptLicensed { .. }));
}

/// A claim that leaves `PanelBound` any way but a licence (a void) drops its tally; the rows of an old round are swept after their
/// evidence window, oldest first, and a reorg across the sweep restores them.
#[test]
fn tallies_go_with_their_claim_and_rounds_are_swept() {
    let (p, s0, claim) = world(Some(0));
    let seat = seats()[0];
    let (s1, _) = step(&s0, &p, 10, 104, &[object(vertex(seat, 104, vec![leaf(claim, Verdict::Valid)]))]).unwrap();
    assert_eq!(s1.vertex_counts_v1(), (1, 1, 0));
    // Past the receipt deadline the claim leaves `PanelBound` (redrawn to `Provisional` once, voided after) and its tally goes with it,
    // in the same block.
    let (s2, delta) = step(&s1, &p, 11, 102 + 600 + 1, &[]).expect("the sweep moves the claim");
    assert!(!matches!(phase(&s2, claim), PalwClaimPhaseV2::PanelBound { .. }), "{:?}", phase(&s2, claim));
    assert_eq!(s2.vertex_counts_v1().1, 0, "the tally went with the claim");
    assert_eq!(revert_delta_v2(&s2, &delta, &p).unwrap().vertex_counts_v1(), s1.vertex_counts_v1(), "and a reorg brings it back");
    // The round row outlives the claim until its evidence window ends, then is swept.
    assert_eq!(s2.vertex_counts_v1().0, 1);
    let late = 104 + PALW_VERTEX_ROUND_DAA_V1 + PALW_VERTEX_EVIDENCE_WINDOW_DAA_V1 + 1;
    let (s3, _) = step(&s2, &p, 12, late, &[]).expect("the sweep");
    assert_eq!(s3.vertex_counts_v1().0, 0, "the round row is past its evidence window");
}

/// **Dormant is byte-identical**: a chain that never carries a vertex has no vertex row, and its root, carriage and delta are those of a
/// build without the tables (the Some-only block and tail are absent).
#[test]
fn a_chain_with_no_vertex_row_roots_and_carries_as_before() {
    let (p, s0, _claim) = world(None);
    assert!(s0.vertex.is_empty());
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&s0)).unwrap();
    assert!(!bytes.windows(1).any(|_| false));
    assert_ne!(*bytes.last().unwrap(), PALW_CARRIAGE_VERTEX_TAIL_V1, "no tail is written for an empty table");
    let mut with_row = s0.clone();
    with_row.vertex.rounds.insert((1, bond_key(2)), PalwVertexRoundRowV1 { leaves_root: h64(1), accepted_daa: 1, convicted: false });
    assert_ne!(with_row.state_root(), s0.state_root(), "a row moves the root");
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&with_row)).unwrap();
    let imported = borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap().into_state(&p, None);
    assert!(imported.is_ok(), "a carriage with a vertex tail decodes");
}

/// The tail is `0xE4` and nothing else, last in the carriage.
#[test]
fn the_vertex_tail_is_pinned_at_0xe4() {
    assert_eq!(PALW_CARRIAGE_VERTEX_TAIL_V1, 0xE4);
    let (_p, s0, _claim) = world(None);
    let mut with_row = s0;
    with_row.vertex.rounds.insert((1, bond_key(2)), PalwVertexRoundRowV1 { leaves_root: h64(1), accepted_daa: 1, convicted: false });
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&with_row)).unwrap();
    // The tail: the byte, then the struct (three LE u32 map lengths follow the rounds'): rounds 1 row, tallies 0, held 0, then the
    // mesh's four (RFC-0007 Parts II and IV: witness, audits, traps, capped), all empty.
    let row_len = 8 + 64 + 4 + (64 + 8 + 1);
    let at = bytes.len() - (1 + 4 + row_len + 4 + 4 + 16);
    assert_eq!(bytes[at], 0xE4);
    assert_eq!(bytes[at + 1..at + 5], 1u32.to_le_bytes());
}
