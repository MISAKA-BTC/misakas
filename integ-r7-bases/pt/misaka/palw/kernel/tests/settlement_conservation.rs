//! Checked reference settlement, separate from native coin/reward admission.
use misaka_palw_kernel::{
    hash::Digest,
    ledger::LedgerEventV1,
    settle::{SettlementBookV1, SettlementInstructionV1, SettlementKindV1 as K},
};
fn instruction(bond: Digest, amount: u64, kind: K) -> SettlementInstructionV1 {
    SettlementInstructionV1 { bond, amount, kind, claim: Some([9; 64]) }
}
fn event(bond: Digest, amount: u64, kind: K) -> LedgerEventV1 {
    LedgerEventV1::Settlement(instruction(bond, amount, kind))
}
fn capital(book: &SettlementBookV1) -> u128 {
    book.collateral.values().map(|v| *v as u128).sum::<u128>()
        + book.paid.values().map(|v| *v as u128).sum::<u128>()
        + book.burned as u128
}
#[test]
fn failed_receipt_batch_is_atomic_for_instruction_and_final_balance_refusals() {
    let mut book = SettlementBookV1::default();
    book.set_collateral([1; 64], 100);
    let before = book.clone();
    assert!(book.apply_events(&[event([1; 64], 30, K::ReserveClaim), event([1; 64], 90, K::ReserveDemand)]).is_err());
    assert_eq!(book, before);
    assert!(book.apply_events(&[event([1; 64], 30, K::ReserveClaim), event([1; 64], 20, K::SlashFraud)]).is_err());
    assert_eq!(book, before);
    assert!(
        book.apply_events(&[
            event([1; 64], 30, K::ReserveClaim),
            event([1; 64], 20, K::SlashFraud),
            event([2; 64], 21, K::AccuserReward)
        ])
        .is_err()
    );
    assert_eq!(book, before);
}
#[test]
fn each_accumulated_total_overflow_refuses_before_any_bond_or_map_mutation() {
    for kind in [K::SlashFraud, K::PayJobEscrow, K::AdmissionFee, K::AccuserReward, K::DemanderShare, K::Burn, K::FinalReward] {
        let mut book = SettlementBookV1::default();
        book.set_collateral([1; 64], 100);
        book.reserved.insert([1; 64], 50);
        book.slashed = u64::MAX;
        book.routed = u64::MAX;
        book.burned = u64::MAX;
        book.escrow_spent = u64::MAX;
        book.final_rewards = u64::MAX;
        book.paid.insert([1; 64], u64::MAX);
        let before = book.clone();
        let err = book.apply(&instruction([1; 64], 1, kind)).unwrap_err();
        assert!(err.contains("overflows"), "{kind:?}: {err}");
        assert_eq!(book, before);
    }
    // Exercise escrow and payout overflow independently from the other totals.
    let mut book = SettlementBookV1::default();
    book.set_collateral([1; 64], 100);
    book.reserved.insert([1; 64], 50);
    book.escrow_spent = u64::MAX;
    let before = book.clone();
    assert!(book.apply(&instruction([1; 64], 1, K::PayJobEscrow)).is_err());
    assert_eq!(book, before);
    book.escrow_spent = 10;
    book.final_rewards = 0;
    book.slashed = 10;
    book.routed = 0;
    book.paid.insert([1; 64], u64::MAX);
    let before = book.clone();
    assert!(book.apply(&instruction([1; 64], 1, K::FinalReward)).is_err());
    assert_eq!(book, before);
}
#[test]
fn unfunded_payout_burn_or_final_reward_cannot_create_original_capital() {
    for kind in [K::AccuserReward, K::DemanderShare, K::Burn, K::FinalReward] {
        let mut book = SettlementBookV1::default();
        book.set_collateral([1; 64], 100);
        let before = book.clone();
        assert!(book.apply(&instruction([2; 64], 1, kind)).is_err());
        assert_eq!(book, before);
    }
    // Fraud slash is real debited collateral but is not a job reward escrow.
    let mut book = SettlementBookV1::default();
    book.set_collateral([1; 64], 100);
    book.apply(&instruction([1; 64], 50, K::ReserveClaim)).unwrap();
    book.apply(&instruction([1; 64], 50, K::SlashFraud)).unwrap();
    let before = book.clone();
    assert!(book.apply(&instruction([2; 64], 1, K::FinalReward)).is_err());
    assert_eq!(book, before);
    book.apply(&instruction([2; 64], 40, K::AccuserReward)).unwrap();
    book.apply(&instruction([1; 64], 10, K::Burn)).unwrap();
    book.balanced().unwrap();
    assert_eq!(capital(&book), 100);
    let before = book.clone();
    assert!(book.apply(&instruction([2; 64], 1, K::AccuserReward)).is_err());
    assert_eq!(book, before);
}
#[test]
fn original_collateral_is_shared_by_all_reservations_and_all_routes_conserve() {
    let mut book = SettlementBookV1::default();
    book.set_collateral([1; 64], 1000);
    book.set_collateral([2; 64], 600);
    let original = capital(&book);
    book.apply_events(&[
        event([1; 64], 300, K::ReserveClaim),
        event([1; 64], 100, K::ReserveDemand),
        event([1; 64], 200, K::ReserveJobEscrow),
        event([1; 64], 150, K::ReserveSealDeposit),
    ])
    .unwrap();
    assert_eq!(book.free(&[1; 64]), 250);
    let before = book.clone();
    assert!(book.apply_events(&[event([1; 64], 251, K::ReserveClaim)]).is_err());
    assert_eq!(book, before);
    book.apply_events(&[
        event([1; 64], 300, K::SlashFraud),
        event([2; 64], 120, K::AccuserReward),
        event([1; 64], 180, K::Burn),
        event([1; 64], 100, K::ForfeitDemandBond),
        event([2; 64], 60, K::DemanderShare),
        event([1; 64], 40, K::Burn),
        event([1; 64], 200, K::PayJobEscrow),
        event([2; 64], 200, K::FinalReward),
        event([1; 64], 150, K::ForfeitSealDeposit),
        event([1; 64], 150, K::Burn),
        event([1; 64], 10, K::JobFee),
        event([1; 64], 10, K::Burn),
        event([2; 64], 20, K::AdmissionFee),
        event([2; 64], 20, K::Burn),
    ])
    .unwrap();
    assert_eq!(capital(&book), original);
    assert_eq!(book.final_rewards, 200);
    assert_eq!(book.escrow_spent, 200);
    assert_eq!(book.reserved[&[1; 64]], 0);
    assert_eq!(book.slashed, book.routed);
    let before = book.clone();
    assert!(book.apply_events(&[event([2; 64], 1, K::FinalReward)]).is_err());
    assert_eq!(book, before);
}
#[test]
fn valid_u64_boundary_receipt_is_exact_and_duplicate_or_overdrawn_batch_is_refused() {
    let mut book = SettlementBookV1::default();
    book.set_collateral([1; 64], u64::MAX);
    let batch = [
        event([1; 64], u64::MAX, K::ReserveJobEscrow),
        event([1; 64], u64::MAX, K::PayJobEscrow),
        event([2; 64], u64::MAX, K::FinalReward),
    ];
    book.apply_events(&batch).unwrap();
    assert_eq!(capital(&book), u64::MAX as u128);
    let before = book.clone();
    assert!(book.apply_events(&batch).is_err());
    assert_eq!(book, before);
    assert!(book.apply(&instruction([2; 64], 1, K::FinalReward)).is_err());
    assert_eq!(book, before);
}
#[test]
fn inconsistent_synced_collateral_is_refused_and_release_then_withdraw_returns_exact_owner_capital() {
    let mut book = SettlementBookV1::default();
    book.set_collateral([1; 64], 100);
    book.reserved.insert([1; 64], 101);
    let before = book.clone();
    assert!(book.apply(&instruction([1; 64], 0, K::ReserveDemand)).is_err());
    assert_eq!(book, before);
    book.reserved.insert([1; 64], 100);
    book.apply_events(&[event([1; 64], 100, K::ReleaseClaim), event([1; 64], 100, K::Withdraw)]).unwrap();
    assert!(!book.collateral.contains_key(&[1; 64]));
    assert!(!book.reserved.contains_key(&[1; 64]));
    assert!(book.paid.is_empty());
    assert_eq!(book.slashed, 0);
    assert_eq!(book.routed, 0);
}

#[test]
fn another_subsystems_collateral_loss_allows_clamped_debit_and_release_without_a_phantom_bounty() {
    let mut book = SettlementBookV1::default();
    book.set_collateral([1; 64], 1000);
    book.apply_events(&[event([1; 64], 1000, K::ReserveClaim)]).unwrap();
    // External consumer takes700, leaving an existing1000 reservation over300.
    book.set_collateral([1; 64], 300);
    let before = book.clone();
    assert!(book.apply(&instruction([1; 64], 1, K::ReserveDemand)).is_err());
    assert_eq!(book, before);
    book.apply_events(&[
        event([1; 64], 300, K::SlashFraud),
        event([1; 64], 700, K::ReleaseClaim),
        event([2; 64], 147, K::AccuserReward),
        event([1; 64], 153, K::Burn),
    ])
    .unwrap();
    assert_eq!(book.collateral[&[1; 64]], 0);
    assert_eq!(book.reserved[&[1; 64]], 0);
    assert_eq!(book.paid[&[2; 64]], 147);
    assert_eq!(book.burned, 153);
    assert_eq!(capital(&book), 300);
    let before = book.clone();
    assert!(book.apply(&instruction([2; 64], 1, K::AccuserReward)).is_err());
    assert_eq!(book, before);
}
