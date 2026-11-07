//! Accounting-reader controls only: rooted header/index/held-row relations.
//! These do not replace ordinary admission or post-Final court integration tests.
use super::*;

fn bond(n: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(n), 0))
}

fn fixture() -> (PalwKernelRouteStateV1, JobHeader) {
    let d = |n: u8| [n; 64];
    let reserve = 400;
    let finder = 100;
    let plan = QuotaPlanRecordV11 {
        version: 11,
        program_root: d(1),
        parameter_root: d(2),
        resource_root: d(3),
        class_scope_root: d(4),
        actual_scope_root: d(5),
        native_restore_work: 1,
        native_post_carrier_cap: 1,
        source_court_bound: misaka_palw_kernel::flat_execution11::NativeSourceCourtBoundV11 {
            max_proof_bytes: 1,
            work: 1,
            scratch_bytes: 1,
        },
        positions: 1,
        baseline: QuotaQuantitiesV11::default(),
        action_envelopes: std::array::from_fn(|_| QuotaQuantitiesV11::default()),
        action_carrier_caps: [1; FUTURE_ACTION_KINDS_V11],
    };
    let original = NativeJobOriginalV39 {
        version: 39,
        class: d(1),
        source: d(2),
        policy: d(3),
        maximum: d(4),
        class_scope: d(5),
        class_resources: d(6),
        maximum_scope: d(7),
        maximum_resources: d(8),
        maximum_positions: 16,
        input_manifest: d(9),
        quota: d(10),
        shared_plan: plan.root(),
        source_plan: d(12),
        paged_plan: d(13),
        consent: d(14),
        maximum_contract: d(15),
        transport: d(16),
        queue: d(17),
        physical: d(18),
        final_policy: d(19),
        final_authority: d(20),
        final_basis: d(21),
        requester: bond(1),
        producer: bond(2),
        requester_operator: d(22),
        producer_operator: d(23),
        nonce: d(24),
        reward: 100,
        reserve,
        finder,
        actual_debit: 100,
        debit_receipt: d(25),
        admitted_block: d(26),
        admitted: 10,
        interval: 0,
        commit_until: 20,
        challenge_window: 30,
        final_tail: 5,
        deadline: 200,
    };
    let h = JobHeader {
        version: 39,
        id: original.job(),
        original,
        book: d(40),
        book_bytes: 0,
        consent_bytes: 0,
        class_bytes: 0,
        source_bytes: 0,
        post_bytes: 0,
        plan_root: plan.root(),
        plan,
        terminal: NativeJobTerminalV39::Final,
        requester_payout: Hash64::from_u64_word(41),
        producer_payout: Hash64::from_u64_word(42),
    };
    let mut route = PalwKernelRouteStateV1::new(
        crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(Hash64::from_u64_word(1), Hash64::from_u64_word(2)),
        None,
        misaka_palw_kernel::rows::LedgerScalarsV1 { daa: 1, burned: 0 },
    );
    route.aux.insert((TABLE, k(16, &h.id)), b(&(h.root(), &h)));
    route.aux.insert((TABLE, k(22, &h.original.producer)), b(&vec![h.id]));
    (route, h)
}

fn held(h: &JobHeader) -> PalwNativeHeldFeeRowV1 {
    PalwNativeHeldFeeRowV1 {
        version: PALW_NATIVE_LIABILITY_HOLD_ROW_VERSION_V1,
        family: PALW_NATIVE_FAMILY_JOB_V39,
        job: h.id,
        claim: [43; 64],
        producer: h.original.producer,
        queue: [44; 64],
        payload: h.producer_payout.as_bytes(),
        amount: h.original.reward,
        income_root: h.id,
        income_kind: 11,
        budget: [45; 64],
        refund_payload: h.requester_payout.as_bytes(),
        final_daa: 200,
        horizon_daa: 500,
    }
}

#[test]
fn a_held_final_keeps_the_signed_reserve_in_the_bounded_producer_index() {
    let (mut route, h) = fixture();
    assert!(header(&route, h.id).is_ok());
    assert_eq!(reserved(&route, &h.original.producer), 0, "a legacy Final has no held reservation");
    let key = palw_native_held_fee_key_v1(PALW_NATIVE_FAMILY_JOB_V39, &h.id);
    let row = held(&h);
    route.aux.insert((PALW_NATIVE_LIABILITY_HOLD_TABLE_V1, key.clone()), b(&row));
    assert_eq!(reserved(&route, &h.original.producer), 400, "the held Final retains its original collectible reserve");
    assert_eq!(reserved(&route, &h.original.requester), 0, "a different bond cannot inherit the reserve");
    let mut wrong = row;
    wrong.producer = h.original.requester;
    route.aux.insert((PALW_NATIVE_LIABILITY_HOLD_TABLE_V1, key.clone()), b(&wrong));
    assert_eq!(reserved(&route, &h.original.producer), 0, "a held row for another producer does not join the header");
    route.aux.remove(&(PALW_NATIVE_LIABILITY_HOLD_TABLE_V1, key));
    assert_eq!(reserved(&route, &h.original.producer), 0, "actual release/refund removes the reservation");
}
