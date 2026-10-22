//! audit(a) A-1 reproducer (reader level): a held text29 Final does NOT keep its producer reserve in `committed_at`.
//! `write_book` (this module) writes `h.reserved = 0` for every terminal other than Funded / Challengeable, and `reserved()` sums
//! only `h.reserved` — unlike native39's `reserved()`, which re-adds `original.reserve` while the job's fee is held in aux 144
//! (`palw_native_job_fold_v39_liability_tests::a_held_final_keeps_the_signed_reserve_in_the_bounded_producer_index`).
use super::*;
use misaka_palw_kernel::flat_quota11::{FUTURE_ACTION_KINDS_V11, QuotaPlanRecordV11, QuotaQuantitiesV11};
use crate::palw_native_liability_hold_v1::{
    PALW_NATIVE_FAMILY_TEXT_V29, PALW_NATIVE_LIABILITY_HOLD_ROW_VERSION_V1, PALW_NATIVE_LIABILITY_HOLD_TABLE_V1, PalwNativeHeldFeeRowV1,
    palw_native_held_fee_key_v1,
};

fn bond(n: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(n), 0))
}

fn header(terminal: NativeTextTerminalV29, reserved: u64) -> JobHeader {
    let d = |n: u8| [n; 64];
    let plan = QuotaPlanRecordV11 {
        version: 11,
        program_root: d(1),
        parameter_root: d(2),
        resource_root: d(3),
        class_scope_root: d(4),
        actual_scope_root: d(5),
        native_restore_work: 1,
        native_post_carrier_cap: 1,
        source_court_bound: misaka_palw_kernel::flat_execution11::NativeSourceCourtBoundV11 { max_proof_bytes: 1, work: 1, scratch_bytes: 1 },
        positions: 1,
        baseline: QuotaQuantitiesV11::default(),
        action_envelopes: std::array::from_fn(|_| QuotaQuantitiesV11::default()),
        action_carrier_caps: [1; FUTURE_ACTION_KINDS_V11],
    };
    JobHeader {
        job: d(50),
        class: d(51),
        funding: d(52),
        book: d(53),
        maximum: d(54),
        preparation: d(55),
        requester: bond(1),
        producer: bond(2),
        requester_payout: Hash64::from_u64_word(41),
        producer_payout: Hash64::from_u64_word(42),
        book_bytes: 0,
        post_bytes: 0,
        terminal,
        reserved,
        quota_root: plan.root(),
        quota_record: plan,
    }
}

fn route_with(h: &JobHeader) -> PalwKernelRouteStateV1 {
    let mut route = PalwKernelRouteStateV1::new(
        crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(Hash64::from_u64_word(1), Hash64::from_u64_word(2)),
        None,
        misaka_palw_kernel::rows::LedgerScalarsV1 { daa: 1, burned: 0 },
    );
    route.aux.insert((EXECUTION_JOB_TABLE_V11, k(0, &h.job)), key(h));
    route.aux.insert((EXECUTION_JOB_OWNER_TABLE_V11, k(1, &h.producer)), key(&vec![h.job]));
    route
}

#[test]
fn audit_a_text29_held_final_releases_the_producer_reserve_while_liable() {
    const RESERVE: u64 = 400;
    // Challengeable: the reserve is committed (what write_book writes for Funded / Challengeable).
    let live = header(NativeTextTerminalV29::Challengeable, RESERVE);
    assert_eq!(reserved(&route_with(&live), &live.producer), RESERVE as u128);
    // A held Final: write_book writes reserved = 0 for Final; the held row (aux 144) is present, the producer still liable to H.
    let fin = header(NativeTextTerminalV29::Final, 0);
    let mut route = route_with(&fin);
    let row = PalwNativeHeldFeeRowV1 {
        version: PALW_NATIVE_LIABILITY_HOLD_ROW_VERSION_V1,
        family: PALW_NATIVE_FAMILY_TEXT_V29,
        job: fin.job,
        claim: [43; 64],
        producer: fin.producer,
        queue: [44; 64],
        payload: fin.producer_payout.as_bytes(),
        amount: 100,
        income_root: fin.job,
        income_kind: 11,
        budget: [45; 64],
        refund_payload: fin.requester_payout.as_bytes(),
        final_daa: 200,
        horizon_daa: 500,
    };
    route.aux.insert((PALW_NATIVE_LIABILITY_HOLD_TABLE_V1, palw_native_held_fee_key_v1(PALW_NATIVE_FAMILY_TEXT_V29, &fin.job)), key(&row));
    assert_eq!(
        reserved(&route, &fin.producer),
        0,
        "A-1: a held text29 Final (accusable to H, slash = min(collateral, reserve)) leaves its reserve out of committed_at"
    );
}
