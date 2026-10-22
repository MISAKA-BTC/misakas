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

fn held_row(fin: &JobHeader) -> PalwNativeHeldFeeRowV1 {
    PalwNativeHeldFeeRowV1 {
        version: PALW_NATIVE_LIABILITY_HOLD_ROW_VERSION_V1,
        family: PALW_NATIVE_FAMILY_TEXT_V29,
        job: fin.job,
        claim: [43; 64],
        producer: fin.producer,
        queue: [44; 64],
        payload: fin.producer_payout.as_bytes(),
        amount: 20,
        income_root: fin.job,
        income_kind: 11,
        budget: [45; 64],
        refund_payload: fin.requester_payout.as_bytes(),
        final_daa: 200,
        horizon_daa: 500,
    }
}

/// **audit(verify-1) A-1, fold level**: `settle`'s own `write_book` turns a Challengeable header (reserve 1,000 committed) into a
/// Final header with `reserved = 0`; with the job's fee HELD in aux 144 (the producer accusable to H = 500, a post-Final conviction
/// collecting `min(collateral, reserve)`), the reserve must stay in the producer's committed collateral. It does not: the producer's
/// whole bond is free for a second job while the first is still liable. Expected 1,000 committed; actual 0. (native39 keeps it.)
#[test]
fn audit_a_text29_held_final_releases_the_producer_reserve_while_liable() {
    const RESERVE: u64 = 1_000;
    let book = crate::palw_native_text_stop_v29::audit_verify1_unpaid_final_v29();
    assert_eq!(book.terminal(), NativeTextTerminalV29::Final);
    assert_eq!(book.funding().maximum.producer_reserve, RESERVE);
    let live = header(NativeTextTerminalV29::Challengeable, RESERVE);
    let mut state = crate::palw_state_v2::PalwChainStateV2::genesis();
    state.bonds.insert(
        live.producer,
        crate::palw_state_v2::PalwBondStateV2 {
            pubkey: vec![2],
            operator_id: Hash64::from_u64_word(2),
            collateral: RESERVE,
            slashed: 0,
            status: crate::palw_state_v2::PalwBondStatusV2::Active,
            registered_daa: 0,
            payout_payload: Hash64::from_u64_word(42),
            capable_classes: Default::default(),
        },
    );
    state.kernel_route = Some(route_with(&live));
    let params = crate::palw_state_v2::PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, Hash64::from_u64_word(1), 4, 1000, 100, 1000, 0)
        .unwrap();
    let extras = crate::palw_state_v2::PalwTransitionExtrasV1 {
        kernel_route: Some(crate::palw_kernel_route_v1::PalwKernelRouteExtrasV1 {
            network_domain: Hash64::from_u64_word(1),
            ruleset_digest: Hash64::from_u64_word(2),
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut tb = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
    let committed = |tb: &TransitionBuilder<'_>| {
        crate::palw_state_v2::palw_bond_committed_v1(&tb.state, &live.producer, 300, None, params.window_court)
    };
    assert_eq!(committed(&tb), RESERVE as u128, "premise: a Challengeable job commits its reserve");
    // The Final as `settle` writes it (held branch): the header via `write_book`, the fee row in aux 144 with horizon 500.
    let mut fin = live.clone();
    write_book(&mut tb, &mut fin, &book).unwrap();
    assert_eq!((fin.terminal, fin.reserved), (NativeTextTerminalV29::Final, 0), "write_book zeroes the reserve at Final");
    let row = held_row(&fin);
    tb.write_kernel_row(PALW_NATIVE_LIABILITY_HOLD_TABLE_V1, palw_native_held_fee_key_v1(PALW_NATIVE_FAMILY_TEXT_V29, &fin.job), Some(key(&row)))
        .unwrap();
    let now_committed = committed(&tb);
    let free = (RESERVE as u128).saturating_sub(now_committed);
    eprintln!("[verify-1 A-1] held Final at DAA 300 (H = 500): committed {now_committed}, free collateral for new work {free}");
    assert_eq!(
        now_committed, RESERVE as u128,
        "A-1: a held text29 Final (accusable to H, conviction collects min(collateral, reserve)) must keep its reserve committed"
    );
}
