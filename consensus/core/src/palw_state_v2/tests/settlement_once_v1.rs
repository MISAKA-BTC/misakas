//! RFC-0008 once-only settlement across consumers (`palw_settlement_once_fold_v1`; dormant fence `palw_settlement_once_v1`,
//! kernel route aux 140). SOURCE ONLY — written, never compiled or executed in this lane. Each control settles one obligation
//! through two DIFFERENT consumers (or one consumer twice) and requires exactly one payment, one marker and an unchanged state for
//! the second attempt; and below the fence requires the historical bytes.
#![allow(warnings)]
use super::super::palw_settlement_once_fold_v1::{
    PalwSettleOnceV1, kernel_check_fee_at_v1, kernel_producer_reward_at_v1, palw_settled_once_v1,
};
use super::*;
use crate::palw_settlement_once_v1::*;
use misaka_palw_kernel::ledger::LedgerEventV1;
use misaka_palw_kernel::settle::{SettlementInstructionV1, SettlementKindV1};

#[path = "mesh_once_atomicity_v1.rs"]
mod mesh_once_atomicity_v1;

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

fn armed() -> PalwStateParamsV2 {
    params().with_settlement_once_from(Some(0))
}

fn markers(state: &PalwChainStateV2) -> usize {
    state.kernel_route.as_ref().map_or(0, |route| route.aux.keys().filter(|(t, _)| *t == PALW_SETTLEMENT_ONCE_TABLE_V1).count())
}

fn escrow_reward(claim: u8) -> (PalwSettlementObligationV1, PalwSettlementDebitV1) {
    (
        PalwSettlementObligationV1::of(PalwSettlementLegV1::EscrowFinalReward, [claim; 64]),
        PalwSettlementDebitV1::Escrow { poster: [0xAA; 64], amount: 10 },
    )
}

#[test]
fn below_the_fence_every_consumer_is_dormant_and_nothing_is_read_or_written() {
    let p = params();
    let extras = PalwTransitionExtrasV1::default();
    let parent = with_route(PalwChainStateV2::genesis());
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    let (obligation, debit) = escrow_reward(3);
    for consumer in [PalwSettlementConsumerV1::KernelTick, PalwSettlementConsumerV1::ExecKernelFee] {
        assert_eq!(b.settle_once_v1(100, obligation, consumer, 10, debit).unwrap(), PalwSettleOnceV1::Dormant);
    }
    assert_eq!(b.state, parent);
    assert!(b.entries.is_empty());
}

#[test]
fn the_same_escrow_reward_through_the_kernel_tick_then_the_exec_fee_route_pays_once() {
    let p = armed();
    let extras = PalwTransitionExtrasV1::default();
    let parent = with_route(PalwChainStateV2::genesis());
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    let (obligation, debit) = escrow_reward(3);
    assert_eq!(b.settle_once_v1(100, obligation, PalwSettlementConsumerV1::KernelTick, 10, debit).unwrap(), PalwSettleOnceV1::First);
    let after_first = b.state.clone();
    let journal = b.entries.len();
    // A different consumer of the SAME obligation: refused to pay, and it writes nothing.
    match b.settle_once_v1(101, obligation, PalwSettlementConsumerV1::ExecKernelFee, 10, debit).unwrap() {
        PalwSettleOnceV1::AlreadySettled(row) => {
            assert_eq!(row.consumer, PalwSettlementConsumerV1::KernelTick, "the journal names the consumer that paid");
            assert_eq!((row.amount, row.settled_daa), (10, 100));
        }
        other => panic!("a second consumer was allowed to settle: {other:?}"),
    }
    // The object arm too.
    assert!(!b.settle_once_v1(102, obligation, PalwSettlementConsumerV1::KernelObject, 10, debit).unwrap().pays());
    assert_eq!(b.state, after_first);
    assert_eq!(b.entries.len(), journal);
    assert_eq!(markers(&b.state), 1);
    assert!(palw_settled_once_v1(&b.state, &obligation).is_some());
    // Another claim is another obligation.
    let (other, _) = escrow_reward(4);
    assert!(b.settle_once_v1(103, other, PalwSettlementConsumerV1::ExecKernelFee, 10, debit).unwrap().pays());
    assert_eq!(markers(&b.state), 2);
}

#[test]
fn every_leg_family_settles_once_across_its_consumers() {
    let p = armed();
    let extras = PalwTransitionExtrasV1::default();
    let parent = with_route(PalwChainStateV2::genesis());
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    let held = PalwSettlementDebitV1::Held { amount: 5 };
    let cases = [
        (
            PalwSettlementObligationV1::of(PalwSettlementLegV1::GateDeferredRelease, [1; 64]),
            PalwSettlementConsumerV1::VerificationGate,
        ),
        (
            PalwSettlementObligationV1::of(PalwSettlementLegV1::CertificationRelease, [1; 64]),
            PalwSettlementConsumerV1::SecurityCertification,
        ),
        (PalwSettlementObligationV1::of(PalwSettlementLegV1::VestingRelease, [1; 64]), PalwSettlementConsumerV1::VestingMaturity),
        (
            PalwSettlementObligationV1::with_party(PalwSettlementLegV1::ExecLegPayment, [1; 64], [2; 64], 0),
            PalwSettlementConsumerV1::ExecLegMaturity,
        ),
        (
            PalwSettlementObligationV1::with_party(PalwSettlementLegV1::MaterialProviderFee, [1; 64], [2; 64], 0),
            PalwSettlementConsumerV1::MonetaryMaterial,
        ),
    ];
    for (obligation, consumer) in cases {
        assert!(b.settle_once_v1(10, obligation, consumer, 5, held).unwrap().pays(), "{obligation:?} first");
        // The same obligation through the native Final consumer (or any other) is refused.
        assert!(!b.settle_once_v1(11, obligation, PalwSettlementConsumerV1::NativeFinal, 5, held).unwrap().pays());
        assert!(!b.settle_once_v1(12, obligation, consumer, 5, held).unwrap().pays());
    }
    // A leg's ordinal is part of the obligation: the root's other fund is another leg.
    let fund_one = PalwSettlementObligationV1::with_party(PalwSettlementLegV1::ExecLegPayment, [1; 64], [2; 64], 1);
    assert!(b.settle_once_v1(13, fund_one, PalwSettlementConsumerV1::ExecLegMaturity, 5, held).unwrap().pays());
    // The same subject under ANOTHER leg is another obligation (an issuance Final and an escrow reward of one id both pay once).
    let issuance = PalwSettlementObligationV1::of(PalwSettlementLegV1::IssuanceFinal, [1; 64]);
    assert!(
        b.settle_once_v1(14, issuance, PalwSettlementConsumerV1::NativeFinal, 5, PalwSettlementDebitV1::Issuance { carve: 5 })
            .unwrap()
            .pays()
    );
    assert_eq!(markers(&b.state), cases.len() + 2);
}

#[test]
fn a_payment_beyond_its_debit_and_an_unmarked_route_both_fail_closed() {
    let p = armed();
    let extras = PalwTransitionExtrasV1::default();
    let parent = with_route(PalwChainStateV2::genesis());
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    let (obligation, _) = escrow_reward(3);
    let short = PalwSettlementDebitV1::Escrow { poster: [0xAA; 64], amount: 9 };
    assert!(matches!(
        b.settle_once_v1(100, obligation, PalwSettlementConsumerV1::KernelTick, 10, short),
        Err(PalwStateV2Error::KernelRouteRefused(_))
    ));
    assert_eq!(b.state, parent);
    assert!(b.entries.is_empty());
    // Past the fence with no route header there is no place for the marker: no payment may be made.
    let bare = PalwChainStateV2::genesis();
    let mut b = TransitionBuilder::new(&bare, &p, false, false, false, false, &extras);
    let (obligation, debit) = escrow_reward(3);
    assert!(matches!(
        b.settle_once_v1(100, obligation, PalwSettlementConsumerV1::KernelTick, 10, debit),
        Err(PalwStateV2Error::KernelRouteRefused(_))
    ));
    assert_eq!(b.state, bare);
}

#[test]
fn a_marker_under_a_foreign_key_is_refused_not_trusted() {
    let p = armed();
    let extras = PalwTransitionExtrasV1::default();
    let mut parent = with_route(PalwChainStateV2::genesis());
    let (obligation, debit) = escrow_reward(3);
    let (other, _) = escrow_reward(4);
    let row = PalwSettlementOnceRowV1 {
        version: PALW_SETTLEMENT_ONCE_ROW_VERSION_V1,
        obligation: other,
        consumer: PalwSettlementConsumerV1::KernelTick,
        amount: 10,
        debit,
        settled_daa: 1,
    };
    parent
        .kernel_route
        .as_mut()
        .unwrap()
        .aux
        .insert((PALW_SETTLEMENT_ONCE_TABLE_V1, obligation.id_v1().to_vec()), borsh::to_vec(&row).unwrap());
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    assert!(matches!(
        b.settle_once_v1(100, obligation, PalwSettlementConsumerV1::KernelTick, 10, debit),
        Err(PalwStateV2Error::KernelRouteRefused(_))
    ));
}

fn settlement(bond: [u8; 64], amount: u64, kind: SettlementKindV1, claim: [u8; 64]) -> LedgerEventV1 {
    LedgerEventV1::Settlement(SettlementInstructionV1 { bond, amount, kind, claim: Some(claim) })
}

#[test]
fn the_producer_reward_triple_is_told_apart_from_a_verifier_check_fee() {
    let claim = [7; 64];
    let producer_triple = vec![
        settlement([1; 64], 10, SettlementKindV1::PayJobEscrow, claim),
        settlement([1; 64], 0, SettlementKindV1::ReleaseJobEscrow, claim),
        settlement([2; 64], 10, SettlementKindV1::FinalReward, claim),
    ];
    assert_eq!(kernel_producer_reward_at_v1(&producer_triple, 0), Some((claim, [1; 64], [2; 64], 10)));
    assert_eq!(kernel_producer_reward_at_v1(&producer_triple, 1), None, "only a debit opens a triple");
    let mut check_fee = producer_triple.clone();
    check_fee.push(LedgerEventV1::CheckFeePaid { claim, verifier: [2; 64], amount: 10 });
    assert_eq!(kernel_producer_reward_at_v1(&check_fee, 0), None, "a verifier slot fee is the kernel's per-slot obligation");
    let mut mismatched = producer_triple.clone();
    mismatched[2] = settlement([2; 64], 9, SettlementKindV1::FinalReward, claim);
    assert_eq!(kernel_producer_reward_at_v1(&mismatched, 0), None, "a reward that is not exactly the debit is not this leg");
}

#[test]
fn applying_the_same_producer_settlement_batch_twice_debits_and_pays_at_most_once() {
    let p = armed();
    let extras = PalwTransitionExtrasV1::default();
    let mut parent = with_route(PalwChainStateV2::genesis());
    parent.bonds.insert(bond_key(1), bond_record(1_000, 1));
    parent.bonds.insert(bond_key(2), bond_record(1_000, 2));
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    let poster = b.note_kernel_bond(&bond_key(1)).unwrap();
    let producer = b.note_kernel_bond(&bond_key(2)).unwrap();
    let claim = [7; 64];
    let batch = vec![
        settlement(poster, 10, SettlementKindV1::PayJobEscrow, claim),
        settlement(poster, 0, SettlementKindV1::ReleaseJobEscrow, claim),
        settlement(producer, 10, SettlementKindV1::FinalReward, claim),
    ];
    // The closing tick (lenient) applies it, then a second consumer re-applies the same batch. Past the fence the combined
    // liability predicate (RFC08-2) reads the kernel claim's own row; this fixture's route has none, so the leg is unsourced: it
    // is refused, the obligation closes at zero and NOTHING is debited or paid (the fail-closed side of "at most once").
    super::super::palw_kernel_route_fold_v1::apply_settlements(&mut b, &batch, false, 100).unwrap();
    let first = b.state.clone();
    assert_eq!(first.bonds[&bond_key(1)].collateral, 1_000, "an unsourced leg moves nothing");
    let o = PalwSettlementObligationV1::of(PalwSettlementLegV1::EscrowFinalReward, claim);
    assert_eq!(palw_settled_once_v1(&first, &o).map(|row| row.amount), Some(0), "the refusal is journaled at zero");
    super::super::palw_kernel_route_fold_v1::apply_settlements(&mut b, &batch, false, 101).unwrap();
    assert_eq!(b.state, first, "the lenient second application debits and pays nothing");
    // A strict object arm reaching the same claim is refused (the object is dropped).
    assert!(super::super::palw_kernel_route_fold_v1::apply_settlements(&mut b, &batch, true, 102).is_err());
    assert_eq!(markers(&b.state), 1);
}

// ---- RFC08-1: the remaining legs (verifier slot fees, free-prompt and native receipt quanta) ----

fn check_fee_quad(poster: [u8; 64], verifier: [u8; 64], fee: u64, claim: [u8; 64]) -> Vec<LedgerEventV1> {
    vec![
        settlement(poster, fee, SettlementKindV1::PayJobEscrow, claim),
        settlement(poster, 0, SettlementKindV1::ReleaseJobEscrow, claim),
        settlement(verifier, fee, SettlementKindV1::FinalReward, claim),
        LedgerEventV1::CheckFeePaid { claim, verifier, amount: fee },
    ]
}

#[test]
fn the_check_fee_quad_is_matched_exactly_and_never_as_a_producer_reward() {
    let claim = [7; 64];
    let quad = check_fee_quad([1; 64], [3; 64], 5, claim);
    assert_eq!(kernel_check_fee_at_v1(&quad, 0), Some((claim, [1; 64], [3; 64], 5)));
    assert_eq!(kernel_producer_reward_at_v1(&quad, 0), None, "a check fee is not the escrow reward leg");
    // A CheckFeePaid naming another verifier or another amount is not this quad.
    let mut other_verifier = quad.clone();
    other_verifier[3] = LedgerEventV1::CheckFeePaid { claim, verifier: [4; 64], amount: 5 };
    assert_eq!(kernel_check_fee_at_v1(&other_verifier, 0), None);
    let mut other_amount = quad.clone();
    other_amount[3] = LedgerEventV1::CheckFeePaid { claim, verifier: [3; 64], amount: 4 };
    assert_eq!(kernel_check_fee_at_v1(&other_amount, 0), None);
    // A producer triple (no CheckFeePaid) is not a check fee.
    assert_eq!(kernel_check_fee_at_v1(&quad[..3], 0), None);
}

#[test]
fn a_verifiers_slot_fees_are_marked_once_per_slot_across_the_tick_and_the_object_arm() {
    let p = armed();
    let extras = PalwTransitionExtrasV1::default();
    let mut parent = with_route(PalwChainStateV2::genesis());
    parent.bonds.insert(bond_key(1), bond_record(1_000, 1));
    parent.bonds.insert(bond_key(3), bond_record(1_000, 3));
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    let poster = b.note_kernel_bond(&bond_key(1)).unwrap();
    let verifier = b.note_kernel_bond(&bond_key(3)).unwrap();
    let claim = [7; 64];
    // The verifier holds two drawn slots of the claim: the kernel pays both in one batch, two distinct obligations.
    let mut batch = check_fee_quad(poster, verifier, 5, claim);
    batch.extend(check_fee_quad(poster, verifier, 5, claim));
    super::super::palw_kernel_route_fold_v1::apply_settlements(&mut b, &batch, false, 100).unwrap();
    let first = b.state.clone();
    // RFC08-2: a drawn verifier's own liability horizon is undefined (RFC07-5), so past the fence its fee is refused, never
    // defaulted: each slot's obligation is closed at zero and the poster's escrow is not debited.
    assert_eq!(first.bonds[&bond_key(1)].collateral, 1_000);
    for slot in 0..2 {
        let o = PalwSettlementObligationV1::with_party(PalwSettlementLegV1::VerifierCheckFee, claim, verifier, slot);
        let row = palw_settled_once_v1(&first, &o).expect("each slot has its own marker");
        assert_eq!((row.consumer, row.amount), (PalwSettlementConsumerV1::KernelTick, 0));
    }
    assert_eq!(markers(&first), 2);
    // The same batch again: past the fence the markers, not the kernel's per-slot paid flag, stop it.
    super::super::palw_kernel_route_fold_v1::apply_settlements(&mut b, &batch, false, 101).unwrap();
    assert_eq!(b.state, first, "the lenient repeat debits and pays nothing");
    // A strict object arm naming the first slot again is refused (the object is dropped).
    let one = check_fee_quad(poster, verifier, 5, claim);
    assert!(super::super::palw_kernel_route_fold_v1::apply_settlements(&mut b, &one, true, 102).is_err());
    assert_eq!(markers(&b.state), 2, "a refused repeat writes no marker");
}

#[test]
fn below_the_fence_a_check_fee_batch_writes_no_marker() {
    let p = params();
    let extras = PalwTransitionExtrasV1::default();
    let mut parent = with_route(PalwChainStateV2::genesis());
    parent.bonds.insert(bond_key(1), bond_record(1_000, 1));
    parent.bonds.insert(bond_key(3), bond_record(1_000, 3));
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    let poster = b.note_kernel_bond(&bond_key(1)).unwrap();
    let verifier = b.note_kernel_bond(&bond_key(3)).unwrap();
    super::super::palw_kernel_route_fold_v1::apply_settlements(&mut b, &check_fee_quad(poster, verifier, 5, [7; 64]), false, 100)
        .unwrap();
    assert_eq!(markers(&b.state), 0, "dormant: the historical path, no registry read or write");
}

#[test]
fn a_free_prompt_receipt_quantum_settles_once_even_if_its_spent_set_is_rewound() {
    let base = params();
    let certified = certify_fp_claim(&base, 60, 3);
    // The registry armed from the spend's block, with a route header to hold the markers.
    let p = base.clone().with_settlement_once_from(Some(130));
    let parent = with_route(certified.clone());
    let (spent_once, _) =
        apply_palw_transition_v3(&parent, &p, &ctx(6, 130, 6), &[], PalwBlockWorkV3::ReceiptSpend(&fp_spend(0xFC, 0))).unwrap();
    let quantum_0 = PalwSettlementObligationV1::with_party(PalwSettlementLegV1::ReceiptSpend, h64(0xFC).as_bytes(), [0; 64], 0);
    let row = palw_settled_once_v1(&spent_once, &quantum_0).expect("the quantum's worker share is marked");
    assert_eq!(row.consumer, PalwSettlementConsumerV1::ReceiptBlock);
    // A second consumer of the SAME quantum — modelled by rewinding the claim's own guard (its `spent` set) — is refused by the
    // registry alone.
    let mut rewound = spent_once.clone();
    if let PalwClaimSourceV2::FreePrompt { spent, .. } = &mut rewound.claims.get_mut(&h64(0xFC)).unwrap().source {
        spent.remove(&0);
    }
    let again = apply_palw_transition_v3(&rewound, &p, &ctx(7, 131, 7), &[], PalwBlockWorkV3::ReceiptSpend(&fp_spend(0xFC, 0)));
    assert!(matches!(again, Err(PalwStateV2Error::QuantumAlreadySpent { index: 0, .. })), "{again:?}");
    // The next quantum is another obligation and pays.
    let (next, _) =
        apply_palw_transition_v3(&spent_once, &p, &ctx(7, 131, 7), &[], PalwBlockWorkV3::ReceiptSpend(&fp_spend(0xFC, 1))).unwrap();
    assert_eq!(markers(&next), 2);
    // Below the fence the same spend reads and writes no marker.
    let (dormant, _) =
        apply_palw_transition_v3(&parent, &base, &ctx(6, 130, 6), &[], PalwBlockWorkV3::ReceiptSpend(&fp_spend(0xFC, 0))).unwrap();
    assert_eq!(markers(&dormant), 0);
}

#[test]
fn a_native_receipt_quantum_and_a_free_prompt_quantum_of_the_same_id_are_different_obligations() {
    let p = armed();
    let extras = PalwTransitionExtrasV1::default();
    let parent = with_route(PalwChainStateV2::genesis());
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    let carve = PalwSettlementDebitV1::Issuance { carve: 7 };
    let native = PalwSettlementObligationV1::with_party(PalwSettlementLegV1::NativeReceiptSpend, [9; 64], [0; 64], 2);
    let fp = PalwSettlementObligationV1::with_party(PalwSettlementLegV1::ReceiptSpend, [9; 64], [0; 64], 2);
    assert!(b.settle_once_v1(10, native, PalwSettlementConsumerV1::ReceiptBlock, 7, carve).unwrap().pays());
    assert!(b.settle_once_v1(10, fp, PalwSettlementConsumerV1::ReceiptBlock, 7, carve).unwrap().pays());
    // The same native quantum through any other consumer is refused.
    assert!(!b.settle_once_v1(11, native, PalwSettlementConsumerV1::NativeFinal, 7, carve).unwrap().pays());
    assert!(!b.settle_once_v1(11, native, PalwSettlementConsumerV1::ReceiptBlock, 7, carve).unwrap().pays());
    assert_eq!(markers(&b.state), 2);
}

// ---- RFC08-1 (coverage audit): reporter awards and tranches, mesh reserve pay, improvement earnings, source-fee promises ----

#[test]
fn a_mesh_reserve_payment_is_paid_once_per_claim_and_auditor_across_its_paths() {
    use super::super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1 as L;
    let p = armed();
    let extras = PalwTransitionExtrasV1::default();
    let mut parent = with_route(PalwChainStateV2::genesis());
    parent.bonds.insert(bond_key(3), bond_record(1_000, 3));
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    b.write_panel_reserve(100).unwrap();
    let claim = h64(0xA1);
    let stood = L::NoLiableClaim("an audit answer that stood");
    // The sweep pays the answer that stood...
    assert_eq!(b.mesh_pay_from_reserve(50, &bond_key(3), 10, PalwSettlementLegV1::MeshAuditPay, &claim, stood).unwrap(), 10);
    assert_eq!(b.state.panel_reserve_sompi(), 90);
    let after_first = b.state.clone();
    // ...and the same (claim, auditor) reached again — another sweep, or the answer path — pays nothing and leaves the reserve.
    assert_eq!(b.mesh_pay_from_reserve(51, &bond_key(3), 10, PalwSettlementLegV1::MeshAuditPay, &claim, stood).unwrap(), 0);
    assert_eq!(b.state, after_first);
    // The trap bounty of the same pair is another obligation (paid once too).
    let bounty = L::NoLiableClaim("a trap bounty");
    assert_eq!(b.mesh_pay_from_reserve(52, &bond_key(3), 5, PalwSettlementLegV1::MeshTrapBounty, &claim, bounty).unwrap(), 5);
    assert_eq!(b.mesh_pay_from_reserve(53, &bond_key(3), 5, PalwSettlementLegV1::MeshTrapBounty, &claim, bounty).unwrap(), 0);
    assert_eq!(b.state.panel_reserve_sompi(), 85);
    // An answer paid before its trap window has no sourced liability term: refused, nothing leaves the reserve.
    let early = L::Unsourced("an audit answer paid before its row's trap window");
    let other = h64(0xA2);
    assert_eq!(b.mesh_pay_from_reserve(54, &bond_key(3), 10, PalwSettlementLegV1::MeshAuditPay, &other, early).unwrap(), 0);
    assert_eq!(b.state.panel_reserve_sompi(), 85);
}

#[test]
fn below_the_fence_a_mesh_payment_is_the_historical_payment() {
    use super::super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1 as L;
    let p = params();
    let extras = PalwTransitionExtrasV1::default();
    let mut parent = with_route(PalwChainStateV2::genesis());
    parent.bonds.insert(bond_key(3), bond_record(1_000, 3));
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    b.write_panel_reserve(100).unwrap();
    let claim = h64(0xA1);
    let leg = L::Unsourced("ignored below the fence");
    // Dormant: every call pays (the historical rule had no registry), and no marker is written.
    assert_eq!(b.mesh_pay_from_reserve(50, &bond_key(3), 10, PalwSettlementLegV1::MeshAuditPay, &claim, leg).unwrap(), 10);
    assert_eq!(b.mesh_pay_from_reserve(51, &bond_key(3), 10, PalwSettlementLegV1::MeshAuditPay, &claim, leg).unwrap(), 10);
    assert_eq!(markers(&b.state), 0);
}

#[test]
fn an_improvement_credit_is_credited_once_per_source_row_across_its_paths() {
    use super::super::palw_improve_fold_v1::{PALW_IMPROVE_HOLD_ARTIFACT_V1, PalwImproveCreditV1 as C};
    let p = armed();
    let extras = PalwTransitionExtrasV1::default();
    let parent = with_route(PalwChainStateV2::genesis());
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    let teacher = bond_key(4);
    let owed = |b: &TransitionBuilder<'_>| b.state.improvement_earnings.get(&teacher).copied().unwrap_or(0);
    // One artifact's hold, refunded by its deadline path and again by the material sweep: credited once.
    let hold = C::HoldRefund { line: h64(0x10), kind: PALW_IMPROVE_HOLD_ARTIFACT_V1, id: [7; 64] };
    b.credit_improvement_earnings_v1(hold, 60, &teacher, 30).unwrap();
    b.credit_improvement_earnings_v1(hold, 61, &teacher, 30).unwrap();
    assert_eq!(owed(&b), 30);
    // Another artifact is another source.
    b.credit_improvement_earnings_v1(
        C::HoldRefund { line: h64(0x10), kind: PALW_IMPROVE_HOLD_ARTIFACT_V1, id: [8; 64] },
        62,
        &teacher,
        30,
    )
    .unwrap();
    assert_eq!(owed(&b), 60);
    // A grant tranche is keyed by its vested target: the same target twice credits once, the next target is new.
    let tranche = C::GrantTranche { line: h64(0x10), epoch: 1, index: 0, target: 10 };
    b.credit_improvement_earnings_v1(tranche, 63, &teacher, 10).unwrap();
    b.credit_improvement_earnings_v1(tranche, 64, &teacher, 10).unwrap();
    b.credit_improvement_earnings_v1(C::GrantTranche { line: h64(0x10), epoch: 1, index: 0, target: 20 }, 65, &teacher, 10).unwrap();
    assert_eq!(owed(&b), 80);
    // A dissolution pays its owner once.
    let dissolve = C::Dissolution { line: h64(0x10) };
    b.credit_improvement_earnings_v1(dissolve, 66, &teacher, 5).unwrap();
    b.credit_improvement_earnings_v1(dissolve, 67, &teacher, 5).unwrap();
    assert_eq!(owed(&b), 85);
    // An evaluation fee is its claim's reward leg: an unknown claim has no sourced terms, so it is refused (closed at zero).
    let fee = C::EvalFee { claim: h64(0xEE), final_daa: 60, role: 0 };
    b.credit_improvement_earnings_v1(fee, 68, &teacher, 9).unwrap();
    assert_eq!(owed(&b), 85, "refused by the combined liability predicate: never defaulted");
}

#[test]
fn reporter_awards_and_their_tranches_settle_once_and_the_predicate_never_binds_a_reporter() {
    use super::super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1 as L;
    let p = armed();
    let extras = PalwTransitionExtrasV1::default();
    let parent = with_route(PalwChainStateV2::genesis());
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    let reporter = L::NoLiableClaim("a reporter's slash-funded award stands behind no claim");
    let held = PalwSettlementDebitV1::Held { amount: 40 };
    // The award of one offence: named once, whichever path (the native binding or the legacy reward row) reaches it.
    let award = PalwSettlementObligationV1::of(PalwSettlementLegV1::ReporterAward, [0x0F; 64]);
    assert_eq!(
        b.settle_once_liable_v1(10, award, PalwSettlementConsumerV1::ReporterReveal, 40, held, reporter).unwrap(),
        PalwSettleOnceV1::First
    );
    assert!(!b.settle_once_liable_v1(11, award, PalwSettlementConsumerV1::ReporterReveal, 40, held, reporter).unwrap().pays());
    // Its tranches are keyed by progress: the same progress twice pays once; the next progress is the next tranche.
    let tranche =
        |progress| PalwSettlementObligationV1::with_party(PalwSettlementLegV1::ReporterAwardTranche, [0x0F; 64], [0; 64], progress);
    let part = PalwSettlementDebitV1::Held { amount: 20 };
    assert!(b.settle_once_liable_v1(12, tranche(0), PalwSettlementConsumerV1::BondBudgetTick, 20, part, reporter).unwrap().pays());
    assert!(!b.settle_once_liable_v1(13, tranche(0), PalwSettlementConsumerV1::BondBudgetTick, 20, part, reporter).unwrap().pays());
    assert!(b.settle_once_liable_v1(14, tranche(20), PalwSettlementConsumerV1::BondBudgetTick, 20, part, reporter).unwrap().pays());
    assert_eq!(markers(&b.state), 3);
}

#[test]
fn a_source_fee_promise_is_closed_once_and_refused_past_the_fence_for_want_of_sourced_terms() {
    use super::super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1 as L;
    let p = armed();
    let extras = PalwTransitionExtrasV1::default();
    let parent = with_route(PalwChainStateV2::genesis());
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    let promise = PalwSettlementObligationV1::with_party(PalwSettlementLegV1::SourceFeePromise, [0x5C; 64], [0x51; 64], 0);
    let prepaid = PalwSettlementDebitV1::Prepaid { sponsor: [0x50; 64], amount: 6 };
    let unsourced = L::Unsourced("a beacon source's fee: no claim reservation, X3 deadline or material TTL backs the source");
    let first = b.settle_once_liable_v1(10, promise, PalwSettlementConsumerV1::KernelTick, 6, prepaid, unsourced).unwrap();
    assert!(matches!(first, PalwSettleOnceV1::Refused(_)), "{first:?}");
    // Closed at zero: a second consumer (the object arm, the EXEC route) finds it settled and pays nothing.
    let again = b.settle_once_liable_v1(11, promise, PalwSettlementConsumerV1::KernelObject, 6, prepaid, unsourced).unwrap();
    assert!(matches!(again, PalwSettleOnceV1::AlreadySettled(row) if row.amount == 0));
    // Below the fence it is dormant (the batch's own `fee_paid` set is the historical guard).
    let q = params();
    let mut d = TransitionBuilder::new(&parent, &q, false, false, false, false, &extras);
    assert_eq!(
        d.settle_once_liable_v1(10, promise, PalwSettlementConsumerV1::KernelTick, 6, prepaid, unsourced).unwrap(),
        PalwSettleOnceV1::Dormant
    );
}

/// **audit A-4: a source fee payout the once-only predicate refuses goes back to the sponsor.** The sponsor prepaid 50 into the
/// source fee escrow (`FundSourceFeeEscrow`, an earlier fold); past `palw_settlement_once_v1` the producer's `PaySourceFeeEscrow`
/// is `Unsourced` → `Refused` (closed at zero), and the kernel ledger's escrow has already counted the promise as discharged, so
/// no refund instruction would ever follow. Expected: the sponsor's payout +50 once, the producer 0, the marker 0; a second
/// application pays nothing more (strict: refused). Before the fix the 50 was destroyed.
#[test]
fn audit_a4_a_refused_source_fee_payout_returns_the_prepaid_fee_to_the_sponsor() {
    use misaka_palw_kernel::beacon_source::{BeaconSourceTargetV1, BeaconSourceTermsV1};
    use misaka_palw_kernel::beacon_source_fee::{BeaconSourceFeeFundingV1, BeaconSourceFeePromiseV1, BeaconSourceFeeTermsV1};
    let p = armed();
    let extras = PalwTransitionExtrasV1::default();
    let mut parent = with_route(PalwChainStateV2::genesis());
    parent.bonds.insert(bond_key(1), bond_record(1_000, 1));
    parent.bonds.insert(bond_key(2), bond_record(1_000, 2));
    let mut b0 = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    let sponsor = b0.note_kernel_bond(&bond_key(1)).unwrap();
    let producer = b0.note_kernel_bond(&bond_key(2)).unwrap();
    let mut s1 = b0.state.clone();
    let context = misaka_palw_challenge::BeaconContextV1 {
        chain_genesis: [1; 64],
        ruleset_id: [2; 64],
        policy: misaka_palw_challenge::sealed_source_policy_v3(2, 2, 40, 1, 1),
        subject_kind: misaka_palw_challenge::SubjectKindV1::ModelConformance,
        commitment_root: [0xD1; 64],
        commitment_position: 10,
        challenge_epoch: 0,
        eligible_profiles: Default::default(),
        excluded_profiles: Default::default(),
        candidate_profile_id: misaka_palw_challenge::RootV1::Present([0xEF; 64]),
    };
    let source = BeaconSourceTermsV1 {
        context: context.id(),
        seal_start: 12,
        seal_end: 52,
        reveal_end: 92,
        final_by: 180,
        hold_until: 400,
        deposit: 100,
        max_per_producer: 1,
        max_total: 1,
    };
    let ctx = context.id();
    let terms = BeaconSourceFeeTermsV1 { source, sponsor, fee_per_source: 50, max_sources: 1 };
    {
        let route = s1.kernel_route.as_mut().unwrap();
        let mut ledger = route.ledger_checked_v1().unwrap();
        let l = ledger.preview_mut_v1();
        l.beacon_source_fees.insert(
            ctx,
            BeaconSourceFeeFundingV1 {
                target: BeaconSourceTargetV1 { context, terms: source },
                terms,
                funded_daa: 12,
                held: 50,
                allocated: 1,
                paid: 0,
                refunded: 0,
                unused_refunded: false,
            },
        );
        l.beacon_source_fee_promises
            .insert((ctx, producer), BeaconSourceFeePromiseV1 { terms_id: terms.id(), accepted_daa: 12, fee: 50, outcome: None });
        route.rows = ledger.to_rows();
        route.ledger_cache.set(ledger.as_rebuilt_from_v1(&route.rows));
    }
    let payout = |s: &PalwChainStateV2, v: u64| {
        s.pending_payouts.get(&crate::palw_kernel_route_v1::palw_kernel_payout_key_v1(&h64(0x9A00 + v))).map_or(0, |row| row.amount)
    };
    let batch = vec![settlement(producer, 50, SettlementKindV1::PaySourceFeeEscrow, ctx)];
    let mut b = TransitionBuilder::new(&s1, &p, false, false, false, false, &extras);
    super::super::palw_kernel_route_fold_v1::apply_settlements(&mut b, &batch, false, 401).unwrap();
    let promise = PalwSettlementObligationV1::with_party(PalwSettlementLegV1::SourceFeePromise, ctx, producer, 0);
    eprintln!(
        "[fix-a A-4] sponsor payout {}, producer payout {}, marker {:?}",
        payout(&b.state, 1),
        payout(&b.state, 2),
        palw_settled_once_v1(&b.state, &promise).map(|row| row.amount)
    );
    assert_eq!(payout(&b.state, 1), 50, "A-4: the withheld fee returns to the sponsor");
    assert_eq!(payout(&b.state, 2), 0, "the unsourced producer leg pays nothing");
    assert_eq!(palw_settled_once_v1(&b.state, &promise).map(|row| row.amount), Some(0), "the obligation is closed at zero");
    assert_eq!(b.state.bonds[&bond_key(1)].collateral, 1_000, "the sponsor's bond is not debited again");
    // Once: a strict re-application is refused (AlreadySettled) and nothing more is paid.
    assert!(super::super::palw_kernel_route_fold_v1::apply_settlements(&mut b, &batch, true, 402).is_err());
    assert_eq!(payout(&b.state, 1), 50);
}

#[test]
fn held_fee_repeat_probe_reserves_before_lookup_and_retries_without_mutation() {
    use crate::palw_native_resource_v1::{NativeDemandCheckerV1, NativeSnapshotResourceLimitsV1, NativeTransitionReservationV1};
    let p = armed();
    let extras = PalwTransitionExtrasV1::default();
    let parent = with_route(PalwChainStateV2::genesis());
    let mut seed = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    let obligation = PalwSettlementObligationV1::with_party(PalwSettlementLegV1::VerifierCheckFee, [3; 64], [4; 64], 0);
    let debit = PalwSettlementDebitV1::Held { amount: 10 };
    seed.settle_once_v1(100, obligation, PalwSettlementConsumerV1::KernelTick, 10, debit).unwrap();
    let settled = seed.state;
    for limit in [0, 63, 64] {
        let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: limit, database_bytes: 0 });
        let mut b = TransitionBuilder::new(&settled, &p, false, false, false, false, &extras);
        b.native_reservation = NativeTransitionReservationV1::for_checker_v1(&checker);
        let result = super::super::palw_paid_claim_trap_fold_v1::check_fee_hold_admits_v1(&mut b, &obligation);
        if limit < 64 {
            assert!(matches!(result, Err(PalwStateV2Error::NativeResource(_))));
            assert_eq!(checker.outstanding(), (0, 0));
        } else {
            assert!(!result.unwrap(), "a settled fee cannot be held again");
            assert_eq!(checker.outstanding(), (64, 0));
        }
        assert_eq!(b.state, settled);
        assert!(b.entries.is_empty());
        drop(b);
        assert_eq!(checker.outstanding(), (0, 0));
    }
}

#[test]
fn held_fee_open_probe_reserves_both_keys_before_semantic_result() {
    use crate::palw_native_resource_v1::{NativeDemandCheckerV1, NativeSnapshotResourceLimitsV1, NativeTransitionReservationV1};
    use crate::palw_paid_claim_trap_v1::{PALW_PAID_CLAIM_TRAP_ROW_VERSION_V1, PalwHeldCheckFeeRowV1};
    let p = armed();
    let extras = PalwTransitionExtrasV1::default();
    let obligation = PalwSettlementObligationV1::with_party(PalwSettlementLegV1::VerifierCheckFee, [3; 64], [4; 64], 0);
    for held in [false, true] {
        let mut parent = with_route(PalwChainStateV2::genesis());
        if held {
            let mut seed = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
            super::super::palw_paid_claim_trap_fold_v1::write_held_check_fee_v1(
                &mut seed,
                &PalwHeldCheckFeeRowV1 {
                    version: PALW_PAID_CLAIM_TRAP_ROW_VERSION_V1,
                    claim: obligation.subject,
                    verifier: obligation.party,
                    slot: obligation.ordinal,
                    poster: [5; 64],
                    fee: 10,
                    release_after_daa: 100,
                },
            )
            .unwrap();
            parent = seed.state;
        }
        for limit in [0, 63, 64, 200, 201] {
            let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: limit, database_bytes: 0 });
            let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
            b.native_reservation = NativeTransitionReservationV1::for_checker_v1(&checker);
            let result = super::super::palw_paid_claim_trap_fold_v1::check_fee_hold_admits_v1(&mut b, &obligation);
            if limit < 201 {
                assert!(matches!(result, Err(PalwStateV2Error::NativeResource(_))));
                assert_eq!(checker.outstanding(), (if limit < 64 { 0 } else { 64 }, 0));
            } else {
                assert_eq!(result.unwrap(), !held, "an existing held fee cannot be held twice");
                assert_eq!(checker.outstanding(), (201, 0));
            }
            assert_eq!(b.state, parent);
            assert!(b.entries.is_empty());
            drop(b);
            assert_eq!(checker.outstanding(), (0, 0));
        }
    }
}
