//! Explicit private mechanics inputs; no production policy or accepted-work campaign.
use super::*;
use crate::palw_material_release_recovery_v2::{PalwMaterialReleaseRecoveryMirrorV2, PalwMaterialReleaseRecoveryPolicyV2};
use crate::palw_monetary_material_v1::{PalwMonetaryMaterialMirrorV1, PalwMonetaryMaterialPolicyV1};
use crate::tx::{TransactionId, TransactionOutpoint};

fn hash(n: u8) -> Hash64 {
    Hash64::from_bytes([n; 64])
}
fn bond() -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_bytes([1; 64]), 0))
}
fn context() -> PalwBlockContextV2 {
    PalwBlockContextV2 { block: Default::default(), daa_score: 100, blue_score: 100, subsidy: 100 }
}
fn inputs() -> (PalwChainStateV2, PalwStateParamsV2) {
    let mut policy = PalwBondBudgetPolicyV1::unapproved_probe_v1();
    policy.capital_unit_sompi = 1_000;
    policy.claims_per_unit = 2;
    policy.block_units_per_unit = 2 * PALW_BUDGET_BLOCK_UNIT_V1;
    policy.reward_per_unit_sompi = 2_000;
    policy.final_weight_per_unit = 1_000;
    policy.max_open_claims_per_bond = 10;
    let params = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, hash(1), 4, 1000, 100, 1000, 0)
        .unwrap()
        .with_bond_budget(Some(PalwBondBudgetMirrorV1 { from_daa: 100, policy: policy.clone(), allocation: None }))
        .with_reporter_income_from(Some(100))
        .with_material_income_from(Some(100))
        .with_monetary_material(Some(PalwMonetaryMaterialMirrorV1 {
            from_daa: 100,
            policy: PalwMonetaryMaterialPolicyV1 {
                version: 1,
                max_contracts: 8,
                max_contracts_per_provider: 8,
                max_bytes_per_contract: 100,
                max_bytes_per_provider: 800,
                minimum_collateral_sompi: 1,
                minimum_fee_sompi: 1,
                window_daa: 10,
                response_window_daa: 10,
                max_enrollment_daa: 10,
                cost_report_root: hash(2),
            },
        }))
        .with_material_release_recovery_v2(Some(PalwMaterialReleaseRecoveryMirrorV2 {
            from_daa: 100,
            policy: PalwMaterialReleaseRecoveryPolicyV2 {
                version: 2,
                max_reserved_contracts: 8,
                max_generations_per_contract: 2,
                max_provider_streams: 16,
                max_reserved_history_cells: 16,
                measurement_root: hash(3),
            },
        }));
    let mut state = PalwChainStateV2::genesis();
    state.bonds.insert(
        bond(),
        PalwBondStateV2 {
            pubkey: vec![1],
            operator_id: hash(4),
            collateral: 10_000,
            slashed: 0,
            status: PalwBondStatusV2::Active,
            registered_daa: 0,
            payout_payload: hash(5),
            capable_classes: Default::default(),
        },
    );
    let mut book = PalwBondBudgetStateV1::new(100, &policy);
    book.activate_material_income_v3(&mut Vec::new());
    state.bond_budget = Some(book);
    state.claims.insert(hash(6), palw_claim_template_v1(params.base_class_id(), bond(), 100, 10, 40));
    (state, params)
}
fn plan(builder: &TransitionBuilder<'_>, origin: PalwBudgetOriginV1) -> PalwBudgetPlanV1 {
    builder
        .plan_bond_budget_v1(
            &context(),
            bond(),
            None,
            PalwBudgetAskV1 { block_units: PALW_BUDGET_BLOCK_UNIT_V1, reward_sompi: 40, final_weight: 10, liability_sompi: 10 },
            origin,
            PALW_BUDGET_BLOCK_UNIT_V1,
        )
        .unwrap()
        .unwrap()
}
fn missing_enrollment(error: PalwStateV2Error) {
    assert!(error.to_string().contains("new material lacks signed historical-release recovery enrollment"), "{error}");
}

#[test]
fn native_floor_issuer_commits_budget_income_and_provenance_without_paid_session() {
    for origin in [PalwBudgetOriginV1::Attempt, PalwBudgetOriginV1::Rider] {
        let (state, params) = inputs();
        let extras = PalwTransitionExtrasV1::default();
        let mut builder = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        let planned = plan(&builder, origin);
        let expected = planned.reservation;
        assert_eq!(
            expected,
            PalwBudgetVectorV1 {
                claims: 1,
                block_units: PALW_BUDGET_BLOCK_UNIT_V1,
                reward_sompi: 40,
                final_weight: 10,
                round_rights: 0
            }
        );
        builder.commit_bond_budget_v1(&context(), hash(6), planned).unwrap();
        let book = builder.state.bond_budget.as_ref().unwrap();
        let allocation = book.claim_row(&hash(6)).unwrap();
        assert_eq!(allocation.origin, origin);
        assert_eq!(allocation.reserved, PalwBudgetVectorV1 { reward_sompi: 0, ..expected });
        assert_eq!(allocation.consumed.block_units, PALW_BUDGET_BLOCK_UNIT_V1);
        assert_eq!(book.bond_row(&bond()).unwrap().window, expected, "R reclassification does not free quota");
        let income_id = native_income_id_v1(hash(6), &bond(), 0);
        let income = book.claim_row(&income_id).unwrap();
        assert_eq!(income.origin, PalwBudgetOriginV1::NativeIncome);
        assert_eq!(income.reserved.reward_sompi, 40);
        assert!(income.live && !income.open);
        let provenance = book.material_income.get(&material_pending_id_v1(income_id)).unwrap();
        assert_eq!(provenance.root, Some(hash(6)));
        assert_eq!(provenance.amount, 40);
        assert_eq!(book.material_hold_until_v1(hash(6), false), Some(u64::MAX));
        assert!(!builder.entries.is_empty(), "reservation, income and provenance are journaled");
        // Generic income on this same BASE root still requires the signed session.
        let before = builder.state.clone();
        let entries = builder.entries.len();
        missing_enrollment(builder.bind_material_income_root_v1(100, hash(6), income_id).unwrap_err());
        assert_eq!(builder.state, before);
        assert_eq!(builder.entries.len(), entries);
    }
}

#[test]
fn paid_commit_and_issuer_binding_keep_all_non_floor_guards_without_dirty_writes() {
    // Both the commit and issuer binding inspect actual claim/source/origin rows.
    for case in 0..7 {
        let (state, params) = inputs();
        let extras = PalwTransitionExtrasV1::default();
        let mut builder = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        let mut origin = PalwBudgetOriginV1::Attempt;
        match case {
            0 => builder.state.claims.get_mut(&hash(6)).unwrap().class_id = hash(99),
            1 => {
                builder.state.claims.get_mut(&hash(6)).unwrap().source =
                    PalwClaimSourceV2::FreePrompt { quanta: 1, spent: Default::default() }
            }
            2 => {
                builder.state.claims.remove(&hash(6));
            }
            3 => origin = PalwBudgetOriginV1::Legacy,
            4 => origin = PalwBudgetOriginV1::FreePrompt,
            5 => origin = PalwBudgetOriginV1::NativeIncome,
            6 => origin = PalwBudgetOriginV1::MarketFee,
            _ => unreachable!(),
        }
        let planned = plan(&builder, origin);
        let before = builder.state.clone();
        missing_enrollment(builder.commit_bond_budget_v1(&context(), hash(6), planned).unwrap_err());
        assert_eq!(builder.state, before, "commit case {case}");
        assert!(builder.entries.is_empty());

        // Obtain a real native allocation/income first, then independently corrupt
        // only the authoritative binding inputs; the dedicated consumer must refuse.
        let (state, _) = inputs();
        let mut binding = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        let planned = plan(&binding, PalwBudgetOriginV1::Attempt);
        binding.commit_bond_budget_v1(&context(), hash(6), planned).unwrap();
        binding.state.claims = before.claims.clone();
        binding.state.bond_budget.as_mut().unwrap().claims.get_mut(&hash(6)).unwrap().origin = origin;
        let before_binding = binding.state.clone();
        let entries = binding.entries.len();
        missing_enrollment(binding.bind_issuer_material_income_root_v1(100, hash(6)).unwrap_err());
        assert_eq!(binding.state, before_binding, "binding case {case}");
        assert_eq!(binding.entries.len(), entries);
    }
}

fn seat_bond() -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_bytes([2; 64]), 0))
}
fn install_selected_panel(builder: &mut TransitionBuilder<'_>) -> Vec<PalwPanelSeatV2> {
    let mut row = builder.state.bonds[&bond()].clone();
    row.operator_id = hash(22);
    builder.state.bonds.insert(seat_bond(), row);
    let seats = vec![PalwPanelSeatV2 { bond: seat_bond(), operator_id: hash(22) }];
    builder.state.panels.insert(hash(6), PalwPanelStateV2 { anchor: hash(7), seats: seats.clone(), bound_daa: 100 });
    seats
}

#[test]
fn native_floor_selected_panel_and_issuer_queue_exact_income_without_freeing_quota() {
    for origin in [PalwBudgetOriginV1::Attempt, PalwBudgetOriginV1::Rider] {
        let (state, params) = inputs();
        let extras = PalwTransitionExtrasV1::default();
        let mut builder = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        let planned = plan(&builder, origin);
        builder.commit_bond_budget_v1(&context(), hash(6), planned).unwrap();
        let seats = install_selected_panel(&mut builder);
        builder.hold_panel_recipients_v1(hash(6), &seats, 100).unwrap();
        let issuer = native_income_id_v1(hash(6), &bond(), 0);
        let panel = native_income_id_v1(hash(6), &seat_bond(), 1);
        let book = builder.state.bond_budget.as_ref().unwrap();
        assert_eq!(book.claim_row(&panel).unwrap().reserved.reward_sompi, 8, "unchanged 200 permille Panel pool");
        assert_eq!(book.material_income[&material_pending_id_v1(panel)].root, Some(hash(6)));
        let issuer_window = book.bond_row(&bond()).unwrap().window;
        let panel_window = book.bond_row(&seat_bond()).unwrap().window;
        builder.pay_native_recipient_v1(hash(6), bond(), 0, 101, 32, hash(10), true).unwrap();
        builder.pay_native_recipient_v1(hash(6), seat_bond(), 1, 101, 8, hash(11), true).unwrap();
        let book = builder.state.bond_budget.as_ref().unwrap();
        for (income, recipient, queue, amount) in [(issuer, bond(), hash(10), 32), (panel, seat_bond(), hash(11), 8)] {
            let queued = native_queued_income_id_v1(queue, &recipient);
            let row = book.claim_row(&queued).unwrap();
            assert_eq!(row.origin, PalwBudgetOriginV1::NativeIncomeQueued);
            assert_eq!(row.consumed.reward_sompi, amount);
            let receipt = book.material_income[&material_queue_receipt_id_v1(income, queued, 101)];
            assert_eq!(receipt.root, Some(hash(6)));
            assert_eq!(receipt.amount, amount);
            assert_eq!(
                receipt.payment,
                PalwMaterialPaymentV1::Queued {
                    income: Some(queued),
                    queue: None,
                    queued_daa: 101,
                    paid_daa: None,
                    release_daa: None,
                }
            );
            assert!(!book.material_income.contains_key(&material_pending_id_v1(income)));
        }
        assert_eq!(book.bond_row(&bond()).unwrap().window, issuer_window);
        assert_eq!(book.bond_row(&seat_bond()).unwrap().window, panel_window);
        assert_eq!(book.material_hold_until_v1(hash(6), false), Some(u64::MAX), "queue creation is not physical payment");
    }
}

#[test]
fn native_floor_panel_and_payment_authority_refuse_mismatches_and_generic_roles() {
    for case in 0..11 {
        let (state, params) = inputs();
        let extras = PalwTransitionExtrasV1::default();
        let mut builder = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        let planned = plan(&builder, PalwBudgetOriginV1::Attempt);
        builder.commit_bond_budget_v1(&context(), hash(6), planned).unwrap();
        let seats = install_selected_panel(&mut builder);
        builder.hold_panel_recipients_v1(hash(6), &seats, 100).unwrap();
        let mut recipient = seat_bond();
        let mut kind = 1;
        match case {
            0 => {
                builder.state.panels.remove(&hash(6));
            }
            1 => builder.state.panels.get_mut(&hash(6)).unwrap().seats[0].bond = bond(),
            2 => builder.state.claims.get_mut(&hash(6)).unwrap().class_id = hash(99),
            3 => {
                builder.state.claims.get_mut(&hash(6)).unwrap().source =
                    PalwClaimSourceV2::FreePrompt { quanta: 1, spent: Default::default() }
            }
            4 => builder.state.bond_budget.as_mut().unwrap().claims.get_mut(&hash(6)).unwrap().origin = PalwBudgetOriginV1::Legacy,
            5 => builder.state.claims.get_mut(&hash(6)).unwrap().bond = seat_bond(),
            6 => recipient = bond(), // issuer is not the selected seat
            7 => kind = 0,           // selected seat is not the issuer
            8 => {
                kind = 2;
                recipient = bond();
            }
            9 => {
                builder.state.claims.remove(&hash(6));
            }
            10 => builder.state.bond_budget.as_mut().unwrap().claims.get_mut(&hash(6)).unwrap().bond = seat_bond(),
            _ => unreachable!(),
        }
        let before = builder.state.clone();
        let entries = builder.entries.len();
        missing_enrollment(builder.pay_native_recipient_v1(hash(6), recipient, kind, 101, 1, hash(12), true).unwrap_err());
        assert_eq!(builder.state, before, "payment case {case}");
        assert_eq!(builder.entries.len(), entries);
    }
    // An arbitrary generic hold on the same BASE root never gains the selected
    // consumer exception. Its enclosing transaction checkpoint discards writes.
    for (kind, recipient) in [(0, bond()), (1, seat_bond()), (2, bond())] {
        let (state, params) = inputs();
        let extras = PalwTransitionExtrasV1::default();
        let mut builder = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        let planned = plan(&builder, PalwBudgetOriginV1::Attempt);
        builder.commit_bond_budget_v1(&context(), hash(6), planned).unwrap();
        install_selected_panel(&mut builder);
        let before = builder.state.clone();
        let entries = builder.entries.len();
        let checkpoint = builder.try_checkpoint().unwrap();
        missing_enrollment(builder.hold_native_recipient_v1(hash(6), recipient, kind, 100, 110, 1, &[]).unwrap_err());
        builder.restore(checkpoint);
        assert_eq!(builder.state, before);
        assert_eq!(builder.entries.len(), entries);
    }
    for absent in [true, false] {
        let (state, params) = inputs();
        let extras = PalwTransitionExtrasV1::default();
        let mut builder = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        let planned = plan(&builder, PalwBudgetOriginV1::Attempt);
        builder.commit_bond_budget_v1(&context(), hash(6), planned).unwrap();
        let seats = install_selected_panel(&mut builder);
        if absent {
            builder.state.panels.remove(&hash(6));
        }
        let before = builder.state.clone();
        let entries = builder.entries.len();
        let supplied = if absent { seats.as_slice() } else { &[] };
        let error = builder.hold_panel_recipients_v1(hash(6), supplied, 100).unwrap_err();
        assert!(error.to_string().contains("exact stored selected seats"));
        assert_eq!(builder.state, before);
        assert_eq!(builder.entries.len(), entries);
    }
}

#[test]
fn dormant_material_preserves_historical_native_income_without_allocation_row() {
    let (state, params) = inputs();
    let params = params.with_material_income_from(None);
    let extras = PalwTransitionExtrasV1::default();
    let mut builder = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
    assert!(builder.state.bond_budget.as_ref().unwrap().claim_row(&hash(6)).is_none());
    builder.hold_native_recipient_v1(hash(6), bond(), 0, 100, 110, 40, &[]).unwrap();
    builder.pay_native_recipient_v1(hash(6), bond(), 0, 101, 40, hash(10), true).unwrap();
    let book = builder.state.bond_budget.as_ref().unwrap();
    assert_eq!(book.claim_row(&native_queued_income_id_v1(hash(10), &bond())).unwrap().consumed.reward_sompi, 40);
    assert!(book.material_income.is_empty());
}
