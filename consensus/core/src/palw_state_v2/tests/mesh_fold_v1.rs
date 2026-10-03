//! **RFC-0007 Part II (the witness profile's reads) and Part IV.1 (the audit mesh) through the fold** (spec 18): the audit draw at a
//! claim's acceptance, `Audited` leaves, audit pay, traps, the sweep, and the dormant refusals — on the R-core+ door fixtures
//! (testnet-12's windows). Fences: the vertex and the audit mesh at DAA 0 unless a test says otherwise.
//!
//! The fold does not verify signatures (the acceptance walk does, once per object; `consensus`'s processor tests drive that half), so
//! an object here carries a dummy signature of the right length.

use super::*;
use crate::palw_mesh_v1::*;
use crate::palw_panel_v2::PalwReceiptVerdictV2 as Verdict;
use crate::palw_vertex_v1::*;

const COLLATERAL: u64 = 1_000_000_000;
/// The producer (bond 1) posts enough for a trap deposit.
const PRODUCER_COLLATERAL: u64 = 20_000_000_000;
const SUBSIDY: u64 = 1_000_000_000;

fn mp(audit: Option<u64>) -> PalwStateParamsV2 {
    PalwStateParamsV2::new(100, 10, 600, 120, 500, 1000, h64(1), 4, 1000, 100, 1000, 0)
        .unwrap()
        .with_fp_quanta(8, 64)
        .unwrap()
        .with_rcore_plus_mirrors(Some(0), 0, Vec::new())
        .with_worker_carve_permille(620)
        .unwrap()
        .with_vertex_from_daa(Some(0))
        .with_audit_mesh_from_daa(audit)
}

fn mx() -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 { panel_economy_active: true, ..door_extras(true) }
}

fn dummy_sign(message: &[u8], _context: &[u8]) -> Option<Vec<u8>> {
    let mut signature = message.to_vec();
    signature.resize(crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN, 0);
    Some(signature)
}

fn step(
    parent: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    block_word: u64,
    daa: u64,
    objects: &[PalwConsensusObjectV2],
) -> Result<(PalwChainStateV2, PalwStateDeltaV2), PalwStateV2Error> {
    let applied =
        apply_palw_transition_v2_with_extras(parent, p, &ctx(block_word, daa, block_word), objects, None, true, false, false, false, &mx())?;
    check(parent, p, &applied);
    Ok(applied)
}

/// The consistency, deadline, delta and revert checks every block of these tests passes.
fn check(parent: &PalwChainStateV2, p: &PalwStateParamsV2, applied: &(PalwChainStateV2, PalwStateDeltaV2)) {
    applied.0.assert_internal_consistency(p).expect("internal consistency after apply");
    applied.0.assert_deadline_consistency(p).expect("deadline consistency after apply");
    assert_eq!(apply_delta_v2(parent, &applied.1, p).unwrap().state_root(), applied.0.state_root(), "the delta reproduces the fold");
    assert_eq!(revert_delta_v2(&applied.0, &applied.1, p).unwrap().state_root(), parent.state_root(), "the delta reverts to the parent");
}

/// A block that accepts `env` (the chain block's own attempt), with a subsidy so the claim escrows a reward.
fn accept(
    parent: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    block_word: u64,
    daa: u64,
    env: &PalwAttemptEnvelopeV2,
) -> (PalwChainStateV2, PalwStateDeltaV2) {
    let c = PalwBlockContextV2 { subsidy: SUBSIDY, ..ctx(block_word, daa, block_word) };
    let applied = apply_palw_transition_v2_with_extras(parent, p, &c, &[], Some(env), true, false, false, false, &mx())
        .expect("the claim is accepted");
    check(parent, p, &applied);
    applied
}

/// The registry: the class, the producer (bond 1), the five panel seats (2 to 6) and six auditors (7 to 12); the class an IR class.
fn registry(p: &PalwStateParamsV2) -> PalwChainStateV2 {
    let mut objects = register_class_and_bond();
    if let Some(PalwConsensusObjectV2::BondRegistered { collateral, .. }) = objects.last_mut() {
        *collateral = PRODUCER_COLLATERAL;
    }
    objects.extend((2..=12).map(|n| seat_bond_reg(n, COLLATERAL)));
    let (mut s, _) = apply_door(&PalwChainStateV2::genesis(), p, &ctx(1, 100, 1), &objects, None, &mx()).expect("the registry");
    // The class is an IR class (the mesh audits those): a row of its `tir_classes` table.
    s.tir_classes.insert(h64(1), crate::palw_tir_admission_v1::PalwTirClassRecordV1::test_row_v1(h64(1)));
    s
}

fn auditor_bonds() -> Vec<PalwBondKeyV2> {
    (7..=12).map(bond_key).collect()
}

fn leaf(claim: Hash64, ticket: u64, result: u8) -> PalwVertexLeafV1 {
    PalwVertexLeafV1::Audited { claim: PalwClaimRefV1::Full(claim), leaf: ticket, result }
}

fn vertex(seat: PalwBondKeyV2, signed_daa: u64, leaves: Vec<PalwVertexLeafV1>) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::VerificationVertexV1 {
        vertex: Box::new(PalwVerificationVertexV1::sign_v1(h64(999), seat, signed_daa, leaves, dummy_sign).expect("a vertex signs")),
    }
}

/// A world with one accepted claim and its audit row.
fn world() -> (PalwStateParamsV2, PalwChainStateV2, Hash64) {
    let p = mp(Some(0));
    let s0 = registry(&p);
    let env = attempt(160, 1);
    let claim = attempt_id_v2(&env.attempt);
    let (s1, _) = accept(&s0, &p, 2, 101, &env);
    (p, s1, claim)
}

fn pay_of(s: &PalwChainStateV2, claim: Hash64) -> u64 {
    palw_mesh_audit_pay_v1(s.claim(&claim).expect("the claim").escrowed_reward)
}

/// **The draw**: a claim of an IR class, accepted past `palw_audit_mesh_v1`, draws its two auditors from all bonded seats but its
/// producer, each reserving the trap penalty on its bond; the row is the reorg-safe, carriage-safe, rooted one.
#[test]
fn a_claim_of_an_ir_class_draws_its_audits_at_acceptance() {
    let (p, s, claim) = world();
    let row = s.mesh_audit_row_v1(&claim).expect("the audit row");
    assert_eq!(row.assignments.len(), PALW_AUDITS_PER_CLAIM_V1);
    let pay = pay_of(&s, claim);
    assert!(pay > 0, "the claim escrowed a reward");
    assert_eq!((row.pay, row.penalty), (pay, palw_mesh_trap_penalty_v1(pay)), "pay is 4 ‰ of the escrow, the penalty ten pays");
    assert_eq!((row.drawn_daa, row.audit_end_daa, row.row_end_daa), (101, 101 + 240, 101 + 480));
    for assignment in &row.assignments {
        assert_ne!(assignment.auditor, bond_key(1), "never the producer");
        assert_eq!(assignment.reserved, row.penalty, "each auditor reserves the penalty");
        assert!(s.reserved_exposure(&assignment.auditor) >= row.penalty, "and the ledger holds it");
        assert!(assignment.outcome.is_none(), "silence so far");
    }
    let [a, b] = [row.assignments[0].auditor, row.assignments[1].auditor];
    assert_ne!(a, b);
    // The draw is a pure function of the claim: the same fold deals the same auditors.
    let (_, again, claim2) = world();
    assert_eq!(claim2, claim);
    assert_eq!(again.mesh_audit_row_v1(&claim2), Some(row), "deterministic");
    // The carriage round-trips with the row.
    // (A `tir_classes` row built for a test has no program to check, which the carriage's own loader refuses by design — the mesh's
    // rows are covered by the delta, the root and the consistency check every block of these tests passes.)
    assert_eq!(s.vertex.mesh.audits.len(), 1);
    assert!(!s.vertex.mesh.is_empty());
    s.assert_internal_consistency(&p).expect("consistent");
}

/// A claim of a class that is not an IR class is not audited; below the fence nothing is drawn.
#[test]
fn only_ir_classes_and_only_past_the_fence_are_audited() {
    let p = mp(Some(0));
    let mut s0 = registry(&p);
    s0.tir_classes.clear();
    let env = attempt(160, 1);
    let claim = attempt_id_v2(&env.attempt);
    let (s1, _) = accept(&s0, &p, 2, 101, &env);
    assert!(s1.mesh_audit_row_v1(&claim).is_none(), "the class has no IR row: nothing to audit");
    // The fence at 500: the claim accepted at 101 draws nothing, and the table stays empty.
    let late = mp(Some(500));
    let s0 = registry(&late);
    let (s1, _) = accept(&s0, &late, 2, 101, &env);
    assert!(s1.mesh_audit_row_v1(&claim).is_none() && s1.vertex.mesh.is_empty());
    assert_eq!(s1.state_root(), {
        let (plain, _) = accept(&registry(&mp(None)), &mp(None), 2, 101, &env);
        plain.state_root()
    }, "below the fence the state is byte-for-byte the fence-off state");
}

/// **An `Audited` leaf** is counted when its seat is a drawn auditor naming the drawn ticket inside the audit window; the audit is paid
/// out of the panel reserve; silence is never a match; and a leaf that does not count is ignored, never refused.
#[test]
fn audited_leaves_are_counted_and_paid_and_silence_is_not_a_match() {
    let (p, mut s, claim) = world();
    s.panel_reserve_sompi = 1_000_000_000;
    let row = s.mesh_audit_row_v1(&claim).unwrap().clone();
    let pay = row.pay;
    let (first, second) = (row.assignments[0], row.assignments[1]);
    let reserve0 = s.panel_reserve_sompi();
    // A wrong ticket, a seat that was not drawn, a leaf signed past the window: ignored.
    let stranger = auditor_bonds().into_iter().find(|b| *b != first.auditor && *b != second.auditor).unwrap();
    for (seat, ticket, signed) in
        [(first.auditor, first.ticket ^ 1, 110u64), (stranger, first.ticket, 111), (first.auditor, first.ticket, row.audit_end_daa + 1)]
    {
        let (next, _) = step(&s, &p, 10, signed, &[vertex(seat, signed, vec![leaf(claim, ticket, 0)])]).unwrap();
        assert!(next.mesh_audit_row_v1(&claim).unwrap().assignments.iter().all(|a| a.outcome.is_none()), "ignored");
        assert_eq!(next.panel_reserve_sompi(), reserve0, "and unpaid");
    }
    // The first auditor attests a match: recorded, paid from the reserve, the payout queued under its payload.
    let (s1, _) = step(&s, &p, 11, 120, &[vertex(first.auditor, 120, vec![leaf(claim, first.ticket, 0)])]).unwrap();
    let r1 = s1.mesh_audit_row_v1(&claim).unwrap();
    assert_eq!(r1.assignments[0].outcome, Some(PalwAuditOutcomeV1 { matched: true, signed_daa: 120 }));
    assert!(r1.assignments[1].outcome.is_none(), "the other auditor said nothing: silence is not a match");
    assert_eq!(s1.panel_reserve_sompi(), reserve0 - pay, "the audit is paid out of the reserve");
    let payload = s1.bond(&first.auditor).unwrap().payout_payload;
    assert_eq!(s1.pending_payouts.get(&palw_panel_payout_key_v1(&payload)).map(|row| row.amount), Some(pay));
    assert_eq!(r1.matches(), 1);
    assert!(!r1.all_matched(), "one of two is not every auditor");
    // The same auditor answers again (a mismatch this time): its first answer stands.
    let (s2, _) = step(&s1, &p, 12, 125, &[vertex(first.auditor, 125, vec![leaf(claim, first.ticket, 1)])]).unwrap();
    assert_eq!(s2.mesh_audit_row_v1(&claim).unwrap().assignments[0].outcome.unwrap().matched, true, "an answer stands");
    assert_eq!(s2.panel_reserve_sompi(), reserve0 - pay, "and is paid once");
    // The second auditor attests a mismatch: recorded, paid (a mismatch is an audit done), and every drawn auditor answered.
    let (s3, _) = step(&s2, &p, 13, 130, &[vertex(second.auditor, 130, vec![leaf(claim, second.ticket, 1)])]).unwrap();
    let r3 = s3.mesh_audit_row_v1(&claim).unwrap();
    assert_eq!(r3.assignments[1].outcome, Some(PalwAuditOutcomeV1 { matched: false, signed_daa: 130 }));
    assert_eq!(s3.panel_reserve_sompi(), reserve0 - 2 * pay);
    assert!(!r3.all_matched(), "a mismatch is not a match");
    // A compact reference names the audited claim by the DAA its audit was drawn at and an id prefix.
    let compact = PalwVertexLeafV1::Audited {
        claim: PalwClaimRefV1::compact_of(&claim, 101).unwrap(),
        leaf: second.ticket,
        result: 0,
    };
    let (s4, _) = step(&s, &p, 14, 140, &[vertex(second.auditor, 140, vec![compact])]).unwrap();
    assert!(s4.mesh_audit_row_v1(&claim).unwrap().assignments[1].outcome.is_some_and(|o| o.matched), "a compact reference resolves");
    // A pay the reserve cannot cover is clamped, never refused.
    let mut poor = s.clone();
    poor.panel_reserve_sompi = pay / 2;
    let (s5, _) = step(&poor, &p, 15, 150, &[vertex(first.auditor, 150, vec![leaf(claim, first.ticket, 0)])]).unwrap();
    assert_eq!(s5.panel_reserve_sompi(), 0, "clamped at the reserve");
}

/// **Below the fence**: an audit leaf and a trap object are refused by name; the vertex fence alone does not take them.
#[test]
fn mesh_moves_are_refused_by_name_below_their_fence() {
    let p = mp(None);
    let s0 = registry(&p);
    let seat = bond_key(7);
    let why = step(&s0, &p, 10, 110, &[vertex(seat, 110, vec![leaf(h64(5), 1, 0)])]).expect_err("an audit leaf below the mesh fence").to_string();
    assert!(why.contains("audit mesh") && why.contains("not armed"), "{why}");
    let commit = PalwConsensusObjectV2::TrapCommittedV1 {
        trap: Box::new(PalwTrapCommittedV1 { setter_bond: bond_key(1), commitment: h64(9), signature: vec![0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN] }),
    };
    assert!(palw_object_is_mesh_v1(&commit));
    let why = step(&s0, &p, 10, 110, &[commit]).expect_err("a trap below the fence").to_string();
    assert!(why.contains("below") && why.contains("palw_audit_mesh_v1"), "{why}");
}

/// The first DAA at the start of a slot in which `bond` is drawn to set a trap.
fn drawn_slot_start(bond: &PalwBondKeyV2, from_slot: u64) -> u64 {
    (from_slot..).map(|slot| slot * PALW_TRAP_SLOT_DAA_V1).find(|daa| palw_trap_slot_drawn_v1(bond, *daa)).expect("a slot is drawn")
}

fn commit_object(setter: PalwBondKeyV2, claim: &Hash64, fault: u64, tiles: u64, salt: [u8; 32]) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::TrapCommittedV1 {
        trap: Box::new(PalwTrapCommittedV1::sign_v1(h64(999), setter, claim, fault, tiles, &salt, dummy_sign).unwrap()),
    }
}

fn reveal_object(setter: PalwBondKeyV2, claim: Hash64, fault: u64, tiles: u64, salt: [u8; 32]) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::TrapRevealedV1 {
        reveal: Box::new(PalwTrapRevealedV1::sign_v1(h64(999), setter, claim, fault, tiles, salt, dummy_sign).unwrap()),
    }
}

/// **Traps settle the verifier's dilemma**: a setter drawn by the slot lottery commits, produces a claim of its own, an auditor who
/// attests a match on the planted tile is slashed the penalty, one who attests the mismatch earns the bounty, the trap claim is
/// voided without slashing its setter, and the setter's deposit comes back.
#[test]
fn a_trap_slashes_the_lazy_auditor_pays_the_honest_one_and_spares_its_setter() {
    let p = mp(Some(0));
    let s0 = registry(&p);
    let setter = bond_key(1);
    let slot_start = drawn_slot_start(&setter, 1);
    let salt = [3u8; 32];
    let env = attempt(160, 1);
    let claim = attempt_id_v2(&env.attempt);
    // The commitment is carried first (a block at the slot's start), then the claim.
    let commit_daa = slot_start + 1;
    let (s1, _) = step(&s0, &p, 10, commit_daa, &[commit_object(setter, &claim, 0, 1, salt)]).expect("a drawn setter commits");
    let commitment = palw_trap_commitment_v1(&claim, 0, 1, &salt);
    let trap = s1.mesh_trap_row_v1(&commitment).expect("the trap row").clone();
    assert_eq!((trap.setter, trap.deposit, trap.committed_daa), (setter, PALW_TRAP_DEPOSIT_SOMPI_V1, commit_daa));
    assert_eq!(s1.reserved_exposure(&setter), PALW_TRAP_DEPOSIT_SOMPI_V1, "the deposit is reserved on the bond");
    let claim_daa = commit_daa + 1;
    let (mut s2, _) = accept(&s1, &p, 11, claim_daa, &env);
    s2.panel_reserve_sompi = 1_000_000_000;
    let row = s2.mesh_audit_row_v1(&claim).unwrap().clone();
    let (lazy, honest) = (row.assignments[0], row.assignments[1]);
    // One tile: every ticket lands on the planted one. The lazy auditor attests a match, the honest one the mismatch.
    let (s3, _) = step(&s2, &p, 12, claim_daa + 5, &[vertex(lazy.auditor, claim_daa + 5, vec![leaf(claim, lazy.ticket, 0)])]).unwrap();
    let (s4, _) = step(&s3, &p, 13, claim_daa + 6, &[vertex(honest.auditor, claim_daa + 6, vec![leaf(claim, honest.ticket, 1)])]).unwrap();
    let before_lazy = s4.bond(&lazy.auditor).unwrap().collateral;
    let before_setter = s4.bond(&setter).unwrap().collateral;
    let reserve = s4.panel_reserve_sompi();
    // Before the audit window closes a reveal is refused; after the reveal window it is refused too.
    let early = step(&s4, &p, 14, claim_daa + 10, &[reveal_object(setter, claim, 0, 1, salt)]).expect_err("early").to_string();
    assert!(early.contains("before the audit window closes"), "{early}");
    let reveal_daa = row.audit_end_daa + 5;
    let late = step(&s4, &p, 15, row.row_end_daa + 1, &[reveal_object(setter, claim, 0, 1, salt)]).expect_err("late").to_string();
    assert!(late.contains("past its reveal window") || late.contains("not a claim"), "{late}");
    let (s5, _) = step(&s4, &p, 16, reveal_daa, &[reveal_object(setter, claim, 0, 1, salt)]).expect("the reveal settles");
    // The lazy auditor lost the penalty; the honest one earned the bounty; the setter lost nothing.
    assert_eq!(before_lazy - s5.bond(&lazy.auditor).unwrap().collateral, u64::try_from(row.penalty).unwrap(), "the lazy auditor is slashed the penalty");
    assert_eq!(s5.bond(&setter).unwrap().collateral, before_setter, "the setter is not slashed");
    assert_eq!(s5.bond(&setter).unwrap().slashed, s4.bond(&setter).unwrap().slashed);
    assert_eq!(s5.panel_reserve_sompi(), reserve - palw_mesh_trap_bounty_v1(row.pay), "the honest auditor is paid the bounty from the reserve");
    assert_eq!(s5.reserved_exposure(&setter), 0, "the deposit is returned");
    assert!(s5.mesh_trap_row_v1(&commitment).is_none(), "the commitment is spent");
    assert!(matches!(s5.claim(&claim).unwrap().phase, PalwClaimPhaseV2::Voided { .. }), "the trap claim is voided");
    assert!(s5.mesh_audit_row_v1(&claim).unwrap().trap_settled);
    // It cannot be revealed again.
    let again = step(&s5, &p, 17, reveal_daa + 1, &[reveal_object(setter, claim, 0, 1, salt)]).expect_err("spent").to_string();
    assert!(again.contains("opens no commitment"), "{again}");
}

/// **Hostile trap moves are refused by name**: a setter the lottery did not draw, a second open trap, an unaffordable deposit, a
/// reveal that opens nothing, by the wrong setter, with an impossible tile, about a claim that is not the setter's.
#[test]
fn hostile_trap_moves_are_refused_by_name() {
    let p = mp(Some(0));
    let s0 = registry(&p);
    let setter = bond_key(1);
    let drawn = drawn_slot_start(&setter, 1) + 1;
    let undrawn = (1..).map(|slot| slot * PALW_TRAP_SLOT_DAA_V1 + 1).find(|daa| !palw_trap_slot_drawn_v1(&setter, *daa)).unwrap();
    let salt = [4u8; 32];
    let claim = h64(0xC1);
    // (A refused block is a block that never lands: its word may repeat, and sits above every parent's.)
    let refused = |s: &PalwChainStateV2, daa: u64, o: PalwConsensusObjectV2| step(s, &p, 900, daa, &[o]).expect_err("refused").to_string();
    assert!(refused(&s0, undrawn, commit_object(setter, &claim, 0, 1, salt)).contains("not drawn to set a trap"));
    assert!(refused(&s0, drawn, commit_object(bond_key(77), &claim, 0, 1, salt)).contains("not registered"));
    // A bond that cannot afford the deposit (a panel seat posts 1 MSK-scale collateral, below the 100 MSK deposit).
    let poor = bond_key(2);
    let poor_drawn = drawn_slot_start(&poor, 1) + 1;
    assert!(refused(&s0, poor_drawn, commit_object(poor, &claim, 0, 1, salt)).contains("sompi free"));
    // One at a time, and one commitment once.
    let (s1, _) = step(&s0, &p, 31, drawn, &[commit_object(setter, &claim, 0, 1, salt)]).unwrap();
    let second = refused(&s1, drawn + 1, commit_object(setter, &claim, 0, 2, salt));
    assert!(second.contains("already has a trap open"), "{second}");
    // Reveals: nothing committed; wrong setter; impossible tiles; a claim that has no audit row.
    assert!(refused(&s1, drawn + 300, reveal_object(setter, claim, 0, 1, [9u8; 32])).contains("opens no commitment"));
    assert!(refused(&s1, drawn + 300, reveal_object(bond_key(2), claim, 0, 1, salt)).contains("that committed it"), "another setter's reveal");
    assert!(refused(&s1, drawn + 300, reveal_object(setter, claim, 1, 1, salt)).contains("tiles"));
    assert!(refused(&s1, drawn + 300, reveal_object(setter, claim, 0, PALW_TRAP_MAX_TILES_V1 + 1, salt)).contains("tiles"));
    assert!(refused(&s1, drawn + 300, reveal_object(setter, claim, 0, 1, salt)).contains("audit row"), "a claim with no audit row");
}

/// **The sweep**: an audit row past its reveal window releases every reservation and leaves; a trap never revealed forfeits its deposit.
#[test]
fn the_sweep_releases_reservations_and_forfeits_an_unrevealed_trap() {
    let p = mp(Some(0));
    let s0 = registry(&p);
    let setter = bond_key(1);
    let commit_daa = drawn_slot_start(&setter, 1) + 1;
    let salt = [5u8; 32];
    let env = attempt(160, 1);
    let claim = attempt_id_v2(&env.attempt);
    let (s1, _) = step(&s0, &p, 10, commit_daa, &[commit_object(setter, &claim, 0, 1, salt)]).unwrap();
    let (s2, _) = accept(&s1, &p, 11, commit_daa + 1, &env);
    let row = s2.mesh_audit_row_v1(&claim).unwrap().clone();
    let auditors: Vec<_> = row.assignments.iter().map(|a| a.auditor).collect();
    assert!(auditors.iter().all(|a| s2.reserved_exposure(a) >= row.penalty));
    let collateral = s2.bond(&setter).unwrap().collateral;
    // Past the audit row's end, the next block sweeps it.
    let (s3, _) = step(&s2, &p, 12, row.row_end_daa + 1, &[]).unwrap();
    assert!(s3.mesh_audit_row_v1(&claim).is_none(), "the audit row left");
    for a in &auditors {
        assert_eq!(s3.reserved_exposure(a), 0, "the reservation was returned");
    }
    // The trap is still open (its time-to-live is longer than the audit row's): never revealed, it forfeits.
    let ttl = commit_daa + palw_mesh_trap_ttl_for_tests();
    let (s4, _) = step(&s3, &p, 13, ttl + 1, &[]).unwrap();
    let commitment = palw_trap_commitment_v1(&claim, 0, 1, &salt);
    assert!(s4.mesh_trap_row_v1(&commitment).is_none(), "the unrevealed trap is gone");
    assert_eq!(collateral - s4.bond(&setter).unwrap().collateral, u64::try_from(PALW_TRAP_DEPOSIT_SOMPI_V1).unwrap(), "its deposit is forfeited");
    assert!(s3.reserved_exposure(&setter) - s4.reserved_exposure(&setter) >= PALW_TRAP_DEPOSIT_SOMPI_V1, "and the reservation returned");
}

fn palw_mesh_trap_ttl_for_tests() -> u64 {
    2 * (PALW_AUDIT_WINDOW_DAA_V1 + PALW_TRAP_REVEAL_WINDOW_DAA_V1)
}

/// **The witness profile's reads**: a class's recorded profile pins the chunk count an attempt of it commits and adds its bytes to the
/// verification window — never to the registry's `verification_ccu`, which pay reads.
#[test]
fn a_recorded_witness_profile_pins_the_chunk_count_and_widens_the_window() {
    let p = PalwStateParamsV2::new(100, 10, 600, 120, 500, 1000, h64(1), 4, 1000, 100, 1000, 0)
        .unwrap()
        .with_fp_quanta(8, 64)
        .unwrap()
        .with_witness_manifest_from_daa(Some(0));
    let mut s = PalwChainStateV2::genesis();
    let env = attempt(160, 1);
    // No profile: the v1 shape (one chunk), and a witness-bearing count is refused.
    let mut a = env.attempt.clone();
    a.trace_chunk_count = 1;
    assert!(crate::palw_admission_v2::check_palw_attempt_witness_pin_v1(&s, &p, &a, 100).is_ok());
    a.trace_chunk_count = 4;
    assert!(matches!(
        crate::palw_admission_v2::check_palw_attempt_witness_pin_v1(&s, &p, &a, 100),
        Err(crate::palw_admission_v2::PalwAdmissionV2Error::TraceChunkCountNotCanonical { claimed: 4, canonical: 1 })
    ));
    // A profile of three witness chunks: exactly four chunks.
    s.vertex.mesh.witness.insert(h64(1), PalwWitnessProfileRowV1 { elements_per_position: 72, max_context: 100, bytes: 3_000_000, chunks: 3 });
    for (count, ok) in [(1, false), (3, false), (4, true), (5, false)] {
        a.trace_chunk_count = count;
        assert_eq!(crate::palw_admission_v2::check_palw_attempt_witness_pin_v1(&s, &p, &a, 100).is_ok(), ok, "count {count}");
    }
    // Below the fence nothing is asked of the count.
    let dormant = p.clone().with_witness_manifest_from_daa(Some(500));
    a.trace_chunk_count = 1;
    assert!(crate::palw_admission_v2::check_palw_attempt_witness_pin_v1(&s, &dormant, &a, 100).is_ok());
    // The window term: the class's witness bytes at 320 MAC-eq a byte are added to the compute the window derives from.
    assert_eq!(palw_class_verify_ccu_v1(&s, &h64(1), 1_000), 1_000 + 3_000_000 * 320);
    assert_eq!(palw_class_verify_ccu_v1(&s, &h64(2), 1_000), 1_000, "a class with no profile is as it was");
}
