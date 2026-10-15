//! RFC-0008 once-only settlement across consumers (`palw_settlement_once_fold_v1`; dormant fence `palw_settlement_once_v1`,
//! kernel route aux 140). SOURCE ONLY — written, never compiled or executed in this lane. Each control settles one obligation
//! through two DIFFERENT consumers (or one consumer twice) and requires exactly one payment, one marker and an unchanged state for
//! the second attempt; and below the fence requires the historical bytes.
use super::super::palw_settlement_once_fold_v1::{PalwSettleOnceV1, kernel_producer_reward_at_v1, palw_settled_once_v1};
use super::*;
use crate::palw_settlement_once_v1::*;
use misaka_palw_kernel::ledger::LedgerEventV1;
use misaka_palw_kernel::settle::{SettlementInstructionV1, SettlementKindV1};

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
        (PalwSettlementObligationV1::of(PalwSettlementLegV1::GateDeferredRelease, [1; 64]), PalwSettlementConsumerV1::VerificationGate),
        (PalwSettlementObligationV1::of(PalwSettlementLegV1::CertificationRelease, [1; 64]), PalwSettlementConsumerV1::SecurityCertification),
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
    assert!(b.settle_once_v1(14, issuance, PalwSettlementConsumerV1::NativeFinal, 5, PalwSettlementDebitV1::Issuance { carve: 5 }).unwrap().pays());
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
fn applying_the_same_producer_settlement_batch_twice_debits_and_pays_once() {
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
    // The closing tick (lenient) applies it, then a second consumer re-applies the same batch.
    super::super::palw_kernel_route_fold_v1::apply_settlements(&mut b, &batch, false, 100).unwrap();
    let paid_once = b.state.clone();
    assert_eq!(paid_once.bonds[&bond_key(1)].collateral, 990, "the poster's escrow was debited once");
    super::super::palw_kernel_route_fold_v1::apply_settlements(&mut b, &batch, false, 101).unwrap();
    assert_eq!(b.state, paid_once, "the lenient second application debits and pays nothing");
    // A strict object arm reaching the same claim is refused (the object is dropped).
    assert!(super::super::palw_kernel_route_fold_v1::apply_settlements(&mut b, &batch, true, 102).is_err());
    assert_eq!(markers(&b.state), 1);
}
