//! RFC-0007 §4.1 T6 in the fold (`palw_paid_claim_trap_fold_v1`; dormant fence `palw_paid_claim_trap_v1`, kernel route aux 142).
//! SOURCE ONLY — written, never compiled or executed in this lane. The pure rules' boundaries are pinned in
//! `crate::palw_paid_claim_trap_v1`'s own tests; these read and write the state: the mandatory trap-or-null commitment (the lock's
//! gate, once per claim, many per producer, one shape, the claim's own lifetime), the reveal's inputs, the Clean signers'
//! liability, the bounty's seat, the held fee's release and forfeiture, the sweep's waiting rule, and that below the fence (and
//! for every claim outside a paid V2 enrollment) nothing changes.
use super::super::palw_paid_claim_trap_fold_v1::*;
use super::*;
use crate::palw_claim_challenge_v2::{PAID_CLAIM_LOCKS_V2, PAID_CLAIM_RECEIPTS_V2, PalwPaidClaimLockV2};
use crate::palw_kernel_route_v1::palw_kernel_bond_id_v1;
use crate::palw_mesh_v1::{PalwTrapRevealedV1, PalwTrapRowV1};
use crate::palw_paid_claim_trap_v1::*;
use crate::palw_settlement_once_v1::*;
use misaka_palw_kernel::ledger::LedgerEventV1;
use misaka_palw_kernel::settle::{SettlementInstructionV1, SettlementKindV1};

const BASE: [u8; 64] = [0xB1; 64];
const ENROLLMENT: [u8; 64] = [0xE1; 64];
const SEED: [u8; 64] = [0x5E; 64];

fn with_route(mut state: PalwChainStateV2) -> PalwChainStateV2 {
    state.kernel_route = Some(crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
        crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(h64(1), h64(2)),
        None,
        misaka_palw_kernel::rows::LedgerScalarsV1 { daa: 1, burned: 0 },
    ));
    state
}

fn bond_record(collateral: u64, payout: u64) -> PalwBondStateV2 {
    PalwBondStateV2 {
        pubkey: vec![7; 4],
        operator_id: h64(0xE0 + payout),
        collateral,
        slashed: 0,
        status: PalwBondStatusV2::Active,
        registered_daa: 0,
        payout_payload: h64(0x9A00 + payout),
        capable_classes: Default::default(),
    }
}

/// Both the registry and the trap in force (the arming `Params` validation requires).
fn armed() -> PalwStateParamsV2 {
    params().with_settlement_once_from(Some(0)).with_paid_claim_trap_from(Some(0))
}

/// The trap alone, the registry dormant: NOT an armable configuration (validation refuses it) — used only to observe the release
/// mechanics without the combined predicate's terms.
fn trap_mechanics_only() -> PalwStateParamsV2 {
    params().with_paid_claim_trap_from(Some(0))
}

fn lifetime() -> misaka_palw_kernel::claim_paid_v2::PaidClaimLifetimeV2 {
    misaka_palw_kernel::claim_paid_v2::PaidClaimLifetimeV2 {
        committed_daa: 0,
        checking_until: 200,
        challenge_until: 300,
        dispute_until: 400,
        liability_until: 10_000,
    }
}

fn paid(producer: PalwBondKeyV2) -> PalwPaidTrapClaimV1 {
    PalwPaidTrapClaimV1 { base_claim: BASE, enrollment: ENROLLMENT, producer, interactive: false, lifetime: lifetime() }
}

fn install_lock(state: &mut PalwChainStateV2, seats: Vec<PalwBondKeyV2>, locked_daa: u64) {
    let lock = PalwPaidClaimLockV2 {
        enrollment: ENROLLMENT,
        context: [0; 64],
        anchor: [0; 64],
        seed: SEED,
        locked_daa,
        assignment: [0; 64],
        seats,
        quorum: 1,
        source_valid: true,
    };
    state.kernel_route.as_mut().unwrap().aux.insert((PAID_CLAIM_LOCKS_V2, ENROLLMENT.to_vec()), borsh::to_vec(&lock).unwrap());
}

fn install_receipt(state: &mut PalwChainStateV2, seat: &PalwBondKeyV2, complete: bool) {
    let signer = palw_kernel_bond_id_v1(seat);
    let receipt = misaka_palw_kernel::claim_paid_v2::PaidClaimReceiptV2 {
        version: 2,
        enrollment: ENROLLMENT,
        verification_class: [0; 64],
        assignment: [0; 64],
        source_context: [0; 64],
        sample_seed: [0; 64],
        evidence_root: [0; 64],
        transcript_root: [0; 64],
        signer,
        signed_daa: 150,
        complete,
        round: 0,
        message_root: [0; 64],
    };
    let id = misaka_palw_kernel::hash::object_id(b"MISAKA/PALW/PAID-CLAIM-RECEIPT/V2", &(ENROLLMENT, signer));
    state.kernel_route.as_mut().unwrap().aux.insert((PAID_CLAIM_RECEIPTS_V2, id.to_vec()), borsh::to_vec(&receipt).unwrap());
}

fn salt_of(i: u64) -> [u8; 32] {
    let mut salt = [0u8; 32];
    salt[..8].copy_from_slice(&i.to_le_bytes());
    salt
}

/// The first salt whose fault does (or does not) count as a trap under [`SEED`] — deterministic.
fn salt_that_counts(counts: bool) -> [u8; 32] {
    (0u64..).map(salt_of).find(|salt| palw_paid_trap_counts_v1(&SEED, &ENROLLMENT, salt) == counts).unwrap()
}

/// The claim's commitment row as the fold writes it (direct setup surgery).
fn install_commitment(state: &mut PalwChainStateV2, producer: PalwBondKeyV2, commitment: [u8; 64], committed_daa: u64) {
    let row = PalwPaidTrapCommitmentRowV1 {
        version: PALW_PAID_CLAIM_TRAP_ROW_VERSION_V1,
        enrollment: ENROLLMENT,
        base_claim: BASE,
        producer,
        commitment,
        committed_daa,
        deposit: PALW_PAID_CLAIM_TRAP_COMMIT_DEPOSIT_SOMPI_V1,
        forget_after_daa: paid(producer).reveal_end(),
    };
    state
        .kernel_route
        .as_mut()
        .unwrap()
        .aux
        .insert((PALW_PAID_CLAIM_TRAP_TABLE_V1, palw_paid_trap_commitment_key_v1(&ENROLLMENT)), borsh::to_vec(&row).unwrap());
}

fn reveal(setter: PalwBondKeyV2, claim: [u8; 64], salt: [u8; 32]) -> PalwTrapRevealedV1 {
    PalwTrapRevealedV1 { setter_bond: setter, claim: Hash64::from_bytes(claim), fault_leaf: 3, tiles: 8, salt, signature: vec![] }
}

fn trap_rows(state: &PalwChainStateV2) -> usize {
    state.kernel_route.as_ref().map_or(0, |route| route.aux.keys().filter(|(t, _)| *t == PALW_PAID_CLAIM_TRAP_TABLE_V1).count())
}

fn commitment_row(state: &PalwChainStateV2, enrollment: &[u8; 64]) -> Option<Vec<u8>> {
    state.kernel_route.as_ref()?.aux.get(&(PALW_PAID_CLAIM_TRAP_TABLE_V1, palw_paid_trap_commitment_key_v1(enrollment))).cloned()
}

// ---- the mandatory commitment ----

#[test]
fn past_the_fence_a_lock_without_the_producers_commitment_is_refused() {
    let mut state = with_route(PalwChainStateV2::genesis());
    // Below the fence: the historical lock (no gate at all).
    assert_eq!(paid_trap_lock_admits_v1(&state, &params(), &ENROLLMENT, 100), Ok(()));
    // Past it: no seat is drawn without the claim's commitment...
    assert!(paid_trap_lock_admits_v1(&state, &armed(), &ENROLLMENT, 100).is_err());
    // ...whatever the commitment opens to (the gate cannot tell, and does not ask).
    install_commitment(&mut state, bond_key(1), palw_paid_trap_commitment_v1(&BASE, None, &salt_of(1)), 50);
    assert_eq!(paid_trap_lock_admits_v1(&state, &armed(), &ENROLLMENT, 100), Ok(()));
    install_commitment(&mut state, bond_key(1), palw_paid_trap_commitment_v1(&BASE, Some((3, 8)), &salt_of(1)), 50);
    assert_eq!(paid_trap_lock_admits_v1(&state, &armed(), &ENROLLMENT, 100), Ok(()));
}

#[test]
fn a_commitment_lands_once_per_claim_before_the_seal_window_and_a_producer_may_hold_many() {
    let p = armed();
    let extras = PalwTransitionExtrasV1::default();
    let mut parent = with_route(PalwChainStateV2::genesis());
    parent.bonds.insert(bond_key(1), bond_record(1_000, 1));
    parent.bonds.insert(bond_key(2), bond_record(1_000, 2));
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    let claim_of = |enrollment: u8| PalwPaidTrapClaimV1 { enrollment: [enrollment; 64], base_claim: [enrollment ^ 0xFF; 64], ..paid(bond_key(1)) };
    let null = |base: [u8; 64]| palw_paid_trap_commitment_v1(&base, None, &salt_of(9));
    let seal_start = 100;
    // Before the seal window, by its producer: accepted.
    let first = claim_of(0x11);
    apply_paid_trap_commitment_v1(&mut b, &ctx(1, 90, 90), &bond_key(1), &first, seal_start, &null(first.base_claim)).unwrap();
    assert!(commitment_row(&b.state, &first.enrollment).is_some());
    // The same claim again: refused (exactly one per claim).
    assert!(apply_paid_trap_commitment_v1(&mut b, &ctx(1, 91, 91), &bond_key(1), &first, seal_start, &null(first.base_claim)).is_err());
    // The same producer's other paid claims: each takes its own commitment — no one-open-per-setter limit.
    for e in [0x12u8, 0x13, 0x14] {
        let other = claim_of(e);
        apply_paid_trap_commitment_v1(&mut b, &ctx(1, 92, 92), &bond_key(1), &other, seal_start, &null(other.base_claim)).unwrap();
    }
    assert_eq!(trap_rows(&b.state), 4);
    // At the seal window (the seed may be forming), by another bond, or below the fence: refused, nothing written.
    let late = claim_of(0x15);
    assert!(apply_paid_trap_commitment_v1(&mut b, &ctx(1, 100, 100), &bond_key(1), &late, seal_start, &null(late.base_claim)).is_err());
    assert!(apply_paid_trap_commitment_v1(&mut b, &ctx(1, 93, 93), &bond_key(2), &late, seal_start, &null(late.base_claim)).is_err());
    let q = params();
    let mut d = TransitionBuilder::new(&parent, &q, false, false, false, false, &extras);
    assert!(apply_paid_trap_commitment_v1(&mut d, &ctx(1, 90, 90), &bond_key(1), &late, seal_start, &null(late.base_claim)).is_err());
    assert_eq!(d.state, parent);
    assert_eq!(trap_rows(&b.state), 4);
}

#[test]
fn a_commitment_after_the_seat_lock_is_refused() {
    let p = armed();
    let extras = PalwTransitionExtrasV1::default();
    let mut parent = with_route(PalwChainStateV2::genesis());
    parent.bonds.insert(bond_key(1), bond_record(1_000, 1));
    install_lock(&mut parent, vec![bond_key(2)], 80);
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    // Even before the recorded seal window, a claim already locked takes no commitment.
    assert!(apply_paid_trap_commitment_v1(&mut b, &ctx(1, 90, 90), &bond_key(1), &paid(bond_key(1)), 100, &[1; 64]).is_err());
    assert_eq!(b.state, parent);
}

#[test]
fn trap_and_null_commitments_are_the_same_shape_on_chain_until_revealed() {
    let p = armed();
    let extras = PalwTransitionExtrasV1::default();
    let mut parent = with_route(PalwChainStateV2::genesis());
    parent.bonds.insert(bond_key(1), bond_record(1_000, 1));
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    let trap_claim = PalwPaidTrapClaimV1 { enrollment: [0x21; 64], ..paid(bond_key(1)) };
    let null_claim = PalwPaidTrapClaimV1 { enrollment: [0x22; 64], ..paid(bond_key(1)) };
    let trap = palw_paid_trap_commitment_v1(&BASE, Some((3, 8)), &salt_of(5));
    let null = palw_paid_trap_commitment_v1(&BASE, None, &salt_of(6));
    apply_paid_trap_commitment_v1(&mut b, &ctx(1, 90, 90), &bond_key(1), &trap_claim, 100, &trap).unwrap();
    apply_paid_trap_commitment_v1(&mut b, &ctx(1, 90, 90), &bond_key(1), &null_claim, 100, &null).unwrap();
    let t = commitment_row(&b.state, &trap_claim.enrollment).unwrap();
    let n = commitment_row(&b.state, &null_claim.enrollment).unwrap();
    // The same encoded length; decoded, every field but the enrollment and the opaque digest is identical (deposit, poster,
    // DAA, lifetime) — nothing on chain before the reveal tells them apart.
    assert_eq!(t.len(), n.len());
    let (t, n): (PalwPaidTrapCommitmentRowV1, PalwPaidTrapCommitmentRowV1) = (borsh::from_slice(&t).unwrap(), borsh::from_slice(&n).unwrap());
    assert_eq!(
        PalwPaidTrapCommitmentRowV1 { enrollment: [0; 64], commitment: [0; 64], ..t },
        PalwPaidTrapCommitmentRowV1 { enrollment: [0; 64], commitment: [0; 64], ..n }
    );
}

#[test]
fn a_commitment_lives_to_its_claims_reveal_window_not_the_mesh_ttl() {
    let p = armed();
    let extras = PalwTransitionExtrasV1::default();
    let mut parent = with_route(PalwChainStateV2::genesis());
    parent.bonds.insert(bond_key(1), bond_record(1_000, 1));
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    // A long receipt window: the claim's reveal window ends far past the mesh's commitment TTL.
    let long = misaka_palw_kernel::claim_paid_v2::PaidClaimLifetimeV2 { checking_until: 5_000, ..lifetime() };
    let claim = PalwPaidTrapClaimV1 { lifetime: long, ..paid(bond_key(1)) };
    apply_paid_trap_commitment_v1(&mut b, &ctx(1, 10, 10), &bond_key(1), &claim, 100, &[7; 64]).unwrap();
    let mesh_ttl_end = 10 + super::super::palw_mesh_fold_v1::PALW_TRAP_COMMIT_TTL_DAA_V1;
    release_held_check_fees_v1(&mut b, mesh_ttl_end + 1).unwrap();
    assert!(commitment_row(&b.state, &ENROLLMENT).is_some(), "still standing past the mesh TTL");
    release_held_check_fees_v1(&mut b, claim.reveal_end()).unwrap();
    assert!(commitment_row(&b.state, &ENROLLMENT).is_some(), "standing through its last DAA");
    release_held_check_fees_v1(&mut b, claim.reveal_end() + 1).unwrap();
    assert!(commitment_row(&b.state, &ENROLLMENT).is_none(), "forgotten after the claim's reveal window");
}

// ---- the reveal ----

#[test]
fn a_reveal_opens_the_claims_commitment_committed_before_the_lock_to_a_fault_that_counts() {
    let producer = bond_key(1);
    let mut state = with_route(PalwChainStateV2::genesis());
    let salt = salt_that_counts(true);
    let r = reveal(producer, BASE, salt);
    install_commitment(&mut state, producer, palw_paid_trap_commitment_v1(&BASE, Some((3, 8)), &salt), 100);
    install_lock(&mut state, vec![bond_key(2), bond_key(3)], 101);
    let p = paid(producer);
    // Committed at 100, locked at 101, counted, revealed after the receipt window (200) and inside the reveal window, convicted.
    palw_paid_trap_reveal_admits_v1(&state.paid_trap_reveal_input_v1(&p, &r, 201, true)).unwrap();
    // The lock at the commitment's own DAA (or before it): refused.
    install_lock(&mut state, vec![bond_key(2), bond_key(3)], 100);
    assert!(palw_paid_trap_reveal_admits_v1(&state.paid_trap_reveal_input_v1(&p, &r, 201, true)).is_err());
    install_lock(&mut state, vec![bond_key(2), bond_key(3)], 101);
    // Another salt (or tile) opens nothing; a reveal over another claim opens nothing for this paid claim.
    assert!(palw_paid_trap_reveal_admits_v1(&state.paid_trap_reveal_input_v1(&p, &reveal(producer, BASE, salt_of(u64::MAX)), 201, true)).is_err());
    assert!(palw_paid_trap_reveal_admits_v1(&state.paid_trap_reveal_input_v1(&p, &reveal(producer, [0xC2; 64], salt), 201, true)).is_err());
    // A null commitment opened as such is no reveal.
    let null_salt = salt_of(77);
    install_commitment(&mut state, producer, palw_paid_trap_commitment_v1(&BASE, None, &null_salt), 100);
    let null_reveal = PalwTrapRevealedV1 { fault_leaf: 0, tiles: 0, ..reveal(producer, BASE, null_salt) };
    assert!(palw_paid_trap_reveal_admits_v1(&state.paid_trap_reveal_input_v1(&p, &null_reveal, 201, true)).is_err());
    // A fault the seed did not select as a trap: refused (its claim stays an ordinary convicted claim).
    let quiet = salt_that_counts(false);
    install_commitment(&mut state, producer, palw_paid_trap_commitment_v1(&BASE, Some((3, 8)), &quiet), 100);
    assert!(palw_paid_trap_reveal_admits_v1(&state.paid_trap_reveal_input_v1(&p, &reveal(producer, BASE, quiet), 201, true)).is_err());
    // Another setter than the producer, no conviction, inside the receipt window, past the reveal window: refused.
    install_commitment(&mut state, producer, palw_paid_trap_commitment_v1(&BASE, Some((3, 8)), &salt), 100);
    assert!(palw_paid_trap_reveal_admits_v1(&state.paid_trap_reveal_input_v1(&paid(bond_key(9)), &r, 201, true)).is_err());
    assert!(palw_paid_trap_reveal_admits_v1(&state.paid_trap_reveal_input_v1(&p, &r, 201, false)).is_err());
    assert!(palw_paid_trap_reveal_admits_v1(&state.paid_trap_reveal_input_v1(&p, &r, 200, true)).is_err());
    let late = p.reveal_end() + 1;
    assert!(palw_paid_trap_reveal_admits_v1(&state.paid_trap_reveal_input_v1(&p, &r, late, true)).is_err());
}

#[test]
fn below_the_fence_and_outside_a_paid_enrollment_a_reveal_keeps_the_mesh_rule_byte_for_byte() {
    let mut state = with_route(PalwChainStateV2::genesis());
    let r = reveal(bond_key(1), BASE, salt_of(7));
    state.vertex.mesh.traps.insert(
        r.commitment(),
        PalwTrapRowV1 { setter: r.setter_bond, committed_daa: 100, deposit: 0, revealed_claim: None },
    );
    for p in [params(), armed()] {
        // No paid V2 authority names BASE (and below the fence none is even looked for): the mesh rule judges it.
        assert!(state.paid_trap_target_v1(&p, &r.claim, 300).is_none());
        let mesh = state.mesh_trap_revealed_admissible_v2(&r, 300, p.audit_1004_active_at(300)).map_err(|e| e.to_string());
        assert_eq!(state.trap_revealed_admissible_v3(&p, &r, 300), mesh);
    }
}

// ---- consequences ----

#[test]
fn a_convicted_paid_claims_clean_signers_are_charged_once_and_nobody_else() {
    let p = armed();
    let extras = PalwTransitionExtrasV1::default();
    let mut parent = with_route(PalwChainStateV2::genesis());
    for i in 2..=4 {
        parent.bonds.insert(bond_key(i), bond_record(1_000, i));
    }
    install_lock(&mut parent, vec![bond_key(2), bond_key(3), bond_key(4)], 101);
    install_receipt(&mut parent, &bond_key(2), true);
    install_receipt(&mut parent, &bond_key(3), true);
    install_receipt(&mut parent, &bond_key(4), false); // not a Clean receipt
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    let claim = paid(bond_key(1));
    let signers = clean_receipt_signers_v1(b.state.kernel_route.as_ref().unwrap(), &claim);
    assert_eq!(signers, vec![bond_key(2), bond_key(3)]);
    charge_receipt_signers_v1(&mut b, &claim, 300).unwrap();
    let l = PALW_PAID_CLAIM_TRAP_SIGNER_LIABILITY_SOMPI_V1;
    assert_eq!(b.state.bonds[&bond_key(2)].collateral, 1_000 - l);
    assert_eq!(b.state.bonds[&bond_key(3)].collateral, 1_000 - l);
    assert_eq!(b.state.bonds[&bond_key(4)].collateral, 1_000, "a seat with no Clean receipt answers for nothing");
    for seat in [bond_key(2), bond_key(3)] {
        let o = PalwSettlementObligationV1::with_party(
            PalwSettlementLegV1::ReceiptSignerLiability,
            BASE,
            palw_kernel_bond_id_v1(&seat),
            0,
        );
        let row = super::super::palw_settlement_once_fold_v1::palw_settled_once_v1(&b.state, &o).expect("charged once, journaled");
        assert_eq!((row.consumer, row.amount), (PalwSettlementConsumerV1::PaidClaimTrap, l));
    }
    // Again (the trap's reveal after the conviction, or a second batch): nothing more.
    let after = b.state.clone();
    charge_receipt_signers_v1(&mut b, &claim, 301).unwrap();
    assert_eq!(b.state, after);
    // An interactive (staged) paid claim has no single Clean receipt: not covered (reported, not guessed).
    let staged = PalwPaidTrapClaimV1 { interactive: true, ..claim };
    assert!(clean_receipt_signers_v1(b.state.kernel_route.as_ref().unwrap(), &staged).is_empty());
}

#[test]
fn the_bounty_goes_to_the_drawn_seat_that_convicted_and_never_to_the_setter_or_an_outsider() {
    let mut state = with_route(PalwChainStateV2::genesis());
    install_lock(&mut state, vec![bond_key(2), bond_key(3)], 101);
    let claim = paid(bond_key(1));
    let convicted_by = |state: &mut PalwChainStateV2, accuser: &PalwBondKeyV2| {
        let row = PalwPaidClaimConvictionRowV1 {
            version: PALW_PAID_CLAIM_TRAP_ROW_VERSION_V1,
            base_claim: BASE,
            enrollment: ENROLLMENT,
            accuser: palw_kernel_bond_id_v1(accuser),
            convicted_daa: 250,
            forget_after_daa: claim.reveal_end(),
        };
        state
            .kernel_route
            .as_mut()
            .unwrap()
            .aux
            .insert((PALW_PAID_CLAIM_TRAP_TABLE_V1, palw_paid_trap_conviction_key_v1(&BASE)), borsh::to_vec(&row).unwrap());
    };
    // No conviction recorded: no bounty.
    assert_eq!(trap_bounty_seat_v1(state.kernel_route.as_ref().unwrap(), &claim, &bond_key(1)), None);
    // The FileProof/FileDemand filer the court recorded is a drawn seat: it is the bounty's seat.
    convicted_by(&mut state, &bond_key(3));
    assert_eq!(trap_bounty_seat_v1(state.kernel_route.as_ref().unwrap(), &claim, &bond_key(1)), Some(bond_key(3)));
    // A seat that is the setter itself earns nothing; an accuser that is no drawn seat is not paid by the trap.
    assert_eq!(trap_bounty_seat_v1(state.kernel_route.as_ref().unwrap(), &claim, &bond_key(3)), None);
    convicted_by(&mut state, &bond_key(8));
    assert_eq!(trap_bounty_seat_v1(state.kernel_route.as_ref().unwrap(), &claim, &bond_key(1)), None);
}

#[test]
fn a_held_fee_is_released_once_and_a_clean_signer_of_a_convicted_claim_forfeits_it() {
    let p = trap_mechanics_only();
    let extras = PalwTransitionExtrasV1::default();
    let mut parent = with_route(PalwChainStateV2::genesis());
    parent.bonds.insert(bond_key(3), bond_record(1_000, 3));
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    let verifier = b.note_kernel_bond(&bond_key(3)).unwrap();
    let payload = b.state.bonds[&bond_key(3)].payout_payload;
    let queued = |b: &TransitionBuilder<'_>| {
        b.state.pending_payouts.get(&crate::palw_kernel_route_v1::palw_kernel_payout_key_v1(&payload)).map_or(0, |row| row.amount)
    };
    let held = |slot: u64| PalwHeldCheckFeeRowV1 {
        version: PALW_PAID_CLAIM_TRAP_ROW_VERSION_V1,
        claim: BASE,
        verifier,
        slot,
        poster: [0xAA; 64],
        fee: 1_000,
        release_after_daa: 440,
    };
    // An honest verifier's fee: held, then released whole.
    write_held_check_fee_v1(&mut b, &held(0)).unwrap();
    assert_eq!(trap_rows(&b.state), 1);
    release_held_check_fee_v1(&mut b, &held(0), false, 441).unwrap();
    assert_eq!(trap_rows(&b.state), 0, "the held row is gone once released");
    assert_eq!(queued(&b), 1_000);
    // A Clean signer of a convicted claim: the forfeiture is taken; only what is kept is paid.
    write_held_check_fee_v1(&mut b, &held(1)).unwrap();
    release_held_check_fee_v1(&mut b, &held(1), true, 441).unwrap();
    let (_, kept) = palw_paid_trap_forfeit_v1(1_000);
    assert_eq!(queued(&b), 1_000 + kept);
    assert_eq!(trap_rows(&b.state), 0);
    // A zero fee holds nothing.
    write_held_check_fee_v1(&mut b, &PalwHeldCheckFeeRowV1 { fee: 0, ..held(2) }).unwrap();
    assert_eq!(trap_rows(&b.state), 0);
}

#[test]
fn past_both_fences_a_release_settles_once_and_never_twice() {
    let p = armed();
    let extras = PalwTransitionExtrasV1::default();
    let mut parent = with_route(PalwChainStateV2::genesis());
    parent.bonds.insert(bond_key(3), bond_record(1_000, 3));
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    let verifier = b.note_kernel_bond(&bond_key(3)).unwrap();
    let row = PalwHeldCheckFeeRowV1 {
        version: PALW_PAID_CLAIM_TRAP_ROW_VERSION_V1,
        claim: BASE,
        verifier,
        slot: 0,
        poster: [0xAA; 64],
        fee: 1_000,
        release_after_daa: 440,
    };
    write_held_check_fee_v1(&mut b, &row).unwrap();
    release_held_check_fee_v1(&mut b, &row, false, 441).unwrap();
    // This fixture has no kernel claim row, so the verifier's terms are unsourced: refused by name, closed at zero, nothing paid.
    let o = PalwSettlementObligationV1::with_party(PalwSettlementLegV1::VerifierCheckFee, BASE, verifier, 0);
    let marker = super::super::palw_settlement_once_fold_v1::palw_settled_once_v1(&b.state, &o).expect("closed once");
    assert_eq!((marker.consumer, marker.amount), (PalwSettlementConsumerV1::PaidClaimTrap, 0));
    // The slot can never be held again (a later batch naming it is a repeat).
    assert!(!check_fee_hold_admits_v1(&mut b, &o).unwrap());
}

#[test]
fn the_sweep_waits_for_a_terminal_claim_and_forgets_old_convictions() {
    let p = trap_mechanics_only();
    let extras = PalwTransitionExtrasV1::default();
    let mut parent = with_route(PalwChainStateV2::genesis());
    let held = PalwHeldCheckFeeRowV1 {
        version: PALW_PAID_CLAIM_TRAP_ROW_VERSION_V1,
        claim: BASE,
        verifier: [3; 64],
        slot: 0,
        poster: [0xAA; 64],
        fee: 1_000,
        release_after_daa: 440,
    };
    let route = parent.kernel_route.as_mut().unwrap();
    route.aux.insert(
        (PALW_PAID_CLAIM_TRAP_TABLE_V1, palw_paid_trap_held_fee_key_v1(&BASE, &[3; 64], 0)),
        borsh::to_vec(&held).unwrap(),
    );
    for (claim, forget_after_daa) in [([0xC1; 64], 100u64), ([0xC2; 64], 900)] {
        let row = PalwPaidClaimConvictionRowV1 {
            version: PALW_PAID_CLAIM_TRAP_ROW_VERSION_V1,
            base_claim: claim,
            enrollment: ENROLLMENT,
            accuser: [4; 64],
            convicted_daa: 50,
            forget_after_daa,
        };
        route.aux.insert((PALW_PAID_CLAIM_TRAP_TABLE_V1, palw_paid_trap_conviction_key_v1(&claim)), borsh::to_vec(&row).unwrap());
    }
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    release_held_check_fees_v1(&mut b, 500).unwrap();
    let route = b.state.kernel_route.as_ref().unwrap();
    // The claim has no kernel row (not terminal): its held fee waits.
    assert!(route.aux.contains_key(&(PALW_PAID_CLAIM_TRAP_TABLE_V1, palw_paid_trap_held_fee_key_v1(&BASE, &[3; 64], 0))));
    // The conviction past its window is forgotten; the other stays.
    assert!(!route.aux.contains_key(&(PALW_PAID_CLAIM_TRAP_TABLE_V1, palw_paid_trap_conviction_key_v1(&[0xC1; 64]))));
    assert!(route.aux.contains_key(&(PALW_PAID_CLAIM_TRAP_TABLE_V1, palw_paid_trap_conviction_key_v1(&[0xC2; 64]))));
}

fn settlement(bond: [u8; 64], amount: u64, kind: SettlementKindV1, claim: [u8; 64]) -> LedgerEventV1 {
    LedgerEventV1::Settlement(SettlementInstructionV1 { bond, amount, kind, claim: Some(claim) })
}

#[test]
fn an_ordinary_kernel_claims_check_fee_is_unaffected_by_the_trap_fence() {
    // Past both fences a check fee of a claim with NO paid V2 enrollment keeps the RFC08-2 rule: refused by name, closed at zero,
    // nothing held, nothing debited.
    let p = armed();
    let extras = PalwTransitionExtrasV1::default();
    let mut parent = with_route(PalwChainStateV2::genesis());
    parent.bonds.insert(bond_key(1), bond_record(1_000, 1));
    parent.bonds.insert(bond_key(3), bond_record(1_000, 3));
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    let poster = b.note_kernel_bond(&bond_key(1)).unwrap();
    let verifier = b.note_kernel_bond(&bond_key(3)).unwrap();
    let claim = [7; 64];
    let quad = vec![
        settlement(poster, 5, SettlementKindV1::PayJobEscrow, claim),
        settlement(poster, 0, SettlementKindV1::ReleaseJobEscrow, claim),
        settlement(verifier, 5, SettlementKindV1::FinalReward, claim),
        LedgerEventV1::CheckFeePaid { claim, verifier, amount: 5 },
    ];
    assert_eq!(check_fee_hold_until_v1(&b, &claim, 100), None, "no paid V2 enrollment: nothing is held");
    super::super::palw_kernel_route_fold_v1::apply_settlements(&mut b, &quad, false, 100).unwrap();
    assert_eq!(b.state.bonds[&bond_key(1)].collateral, 1_000);
    assert_eq!(trap_rows(&b.state), 0);
    let o = PalwSettlementObligationV1::with_party(PalwSettlementLegV1::VerifierCheckFee, claim, verifier, 0);
    assert_eq!(super::super::palw_settlement_once_fold_v1::palw_settled_once_v1(&b.state, &o).map(|row| row.amount), Some(0));
}

#[test]
fn below_the_fence_nothing_is_read_or_written() {
    let p = params();
    let extras = PalwTransitionExtrasV1::default();
    let mut parent = with_route(PalwChainStateV2::genesis());
    parent.bonds.insert(bond_key(2), bond_record(1_000, 2));
    install_lock(&mut parent, vec![bond_key(2)], 101);
    install_receipt(&mut parent, &bond_key(2), true);
    install_commitment(&mut parent, bond_key(1), [9; 64], 50);
    let held = PalwHeldCheckFeeRowV1 {
        version: PALW_PAID_CLAIM_TRAP_ROW_VERSION_V1,
        claim: BASE,
        verifier: [3; 64],
        slot: 0,
        poster: [0xAA; 64],
        fee: 1_000,
        release_after_daa: 0,
    };
    parent.kernel_route.as_mut().unwrap().aux.insert(
        (PALW_PAID_CLAIM_TRAP_TABLE_V1, palw_paid_trap_held_fee_key_v1(&BASE, &[3; 64], 0)),
        borsh::to_vec(&held).unwrap(),
    );
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    assert_eq!(check_fee_hold_until_v1(&b, &BASE, 100), None);
    assert_eq!(paid_trap_lock_admits_v1(&b.state, &p, &[0x77; 64], 100), Ok(()));
    charge_receipt_signers_v1(&mut b, &paid(bond_key(1)), 100).unwrap();
    let convicted = [LedgerEventV1::Convicted {
        claim: BASE,
        accuser: palw_kernel_bond_id_v1(&bond_key(2)),
        slashed: 1,
        accuser_reward: 0,
        post_final: false,
    }];
    on_paid_claim_convictions_v1(&mut b, &convicted, 100).unwrap();
    release_held_check_fees_v1(&mut b, 100_000).unwrap();
    assert_eq!(b.state, parent);
    assert!(b.entries.is_empty());
}
