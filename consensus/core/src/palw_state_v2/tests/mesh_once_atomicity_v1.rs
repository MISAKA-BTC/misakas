//! Artificial overflow-parent controls at the actual pre-object mesh sweep.
//! This is not evidence that a lawful native history can create this balance.
#![allow(warnings)]
use super::*;
use crate::palw_mesh_v1::{PalwAuditAssignmentV1, PalwAuditOutcomeV1, PalwAuditRowV1};

fn fixture(once: bool, queued: u64) -> (PalwChainStateV2, PalwStateParamsV2, Hash64, Hash64) {
    let p = params()
        .with_settlement_once_from(if once { Some(0) } else { None })
        .with_audit_mesh_from_daa(Some(0))
        .with_audit_1004_from_daa(Some(0));
    let mut parent = {
        let extras = PalwTransitionExtrasV1::default();
        let initial = with_route(PalwChainStateV2::genesis());
        let mut builder = TransitionBuilder::new(&initial, &p, false, false, false, false, &extras);
        builder.write_panel_reserve(100).unwrap();
        builder.state
    };
    let auditor = bond_key(3);
    let bond = bond_record(1_000, 3);
    let payload = bond.payout_payload;
    parent.bonds.insert(auditor, bond);
    let payout = palw_panel_payout_key_v1(&payload);
    // The real pre-object drain consumes the first N rows. Keep this payee
    // beyond that prefix so the actual subsequent mesh sweep sees its balance.
    for index in 0..PALW_V2_MAX_PAYOUTS_PER_BLOCK {
        let mut bytes = [0u8; 64];
        bytes[56..].copy_from_slice(&(index as u64).to_be_bytes());
        let key = Hash64::from_bytes(bytes);
        assert!(key < payout);
        parent.pending_payouts.insert(key, PalwPayoutV2 { payload: key, amount: 1 });
    }
    parent.pending_payouts.insert(payout, PalwPayoutV2 { payload, amount: queued });
    let claim = h64(0xA17);
    parent.vertex.mesh.audits.insert(
        claim,
        PalwAuditRowV1 {
            drawn_daa: 1,
            audit_end_daa: 20,
            row_end_daa: 40,
            pay: 10,
            penalty: 0,
            assignments: vec![PalwAuditAssignmentV1 {
                auditor,
                ticket: 0,
                reserved: 0,
                outcome: Some(PalwAuditOutcomeV1 { matched: true, signed_daa: 10 }),
            }],
            trap_settled: false,
        },
    );
    (parent, p, claim, payout)
}

fn fold(parent: &PalwChainStateV2, p: &PalwStateParamsV2) -> Result<(PalwChainStateV2, PalwStateDeltaV2), PalwStateV2Error> {
    apply_palw_transition_v2_with_extras(
        parent,
        p,
        &ctx(0xA18, 50, 0xA18),
        &[],
        None,
        false,
        false,
        false,
        false,
        &PalwTransitionExtrasV1::default(),
    )
}

#[test]
fn mesh_once_actual_preparation_refuses_overflow_and_same_parent_can_retry_without_phantom_marker() {
    let (parent, p, claim, payout) = fixture(true, u64::MAX);
    let root = parent.state_root();
    let actual = fold(&parent, &p);
    if let Ok((accepted, _)) = &actual {
        eprintln!(
            "MESH_ONCE_OVERFLOW unexpected_accepted markers={} reserve={} queue={} audit_retained={}",
            markers(accepted),
            accepted.panel_reserve_sompi(),
            accepted.pending_payouts[&payout].amount,
            accepted.vertex.mesh.audits.contains_key(&claim),
        );
    }
    assert!(matches!(actual, Err(PalwStateV2Error::Overflow("panel payout"))), "actual result: {:?}", actual.as_ref().map(|_| ()));
    assert_eq!(parent.state_root(), root);
    assert_eq!(markers(&parent), 0);
    assert_eq!(parent.panel_reserve_sompi(), 100);
    assert!(parent.vertex.mesh.audits.contains_key(&claim));

    // Repair only the artificial queued balance, leaving the obligation open.
    let mut retry = parent.clone();
    retry.pending_payouts.get_mut(&payout).unwrap().amount = u64::MAX - 10;
    let (paid, delta) = fold(&retry, &p).expect("same actual sweep succeeds with queue headroom");
    assert_eq!(paid.pending_payouts[&payout].amount, u64::MAX);
    assert_eq!(paid.panel_reserve_sompi(), 90);
    assert_eq!(markers(&paid), 1);
    assert!(!paid.vertex.mesh.audits.contains_key(&claim));
    assert_eq!(apply_delta_v2(&retry, &delta, &p).unwrap().state_root(), paid.state_root());
    assert_eq!(revert_delta_v2(&paid, &delta, &p).unwrap().state_root(), retry.state_root());

    let obligation = PalwSettlementObligationV1::with_party(
        PalwSettlementLegV1::MeshAuditPay,
        claim.as_bytes(),
        crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&bond_key(3)),
        0,
    );
    let marker = palw_settled_once_v1(&paid, &obligation).unwrap();
    assert_eq!(marker.amount, 10);
    // A repeat skips before the now-full payout queue is touched.
    let extras = PalwTransitionExtrasV1::default();
    let mut builder = TransitionBuilder::new(&paid, &p, false, false, false, false, &extras);
    assert_eq!(
        builder
            .mesh_pay_from_reserve(
                51,
                &bond_key(3),
                10,
                PalwSettlementLegV1::MeshAuditPay,
                &claim,
                super::super::super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1::NoLiableClaim("stood audit"),
            )
            .unwrap(),
        0
    );
    assert_eq!(builder.state, paid);
    assert!(builder.entries.is_empty());
}

#[test]
fn mesh_once_dormant_actual_preparation_keeps_historical_overflow_skip() {
    let (parent, p, claim, payout) = fixture(false, u64::MAX);
    let (after, delta) = fold(&parent, &p).expect("legacy overflow remains a skipped payment");
    assert_eq!(after.pending_payouts[&payout].amount, u64::MAX);
    assert_eq!(after.panel_reserve_sompi(), 100);
    assert_eq!(markers(&after), 0);
    assert!(!after.vertex.mesh.audits.contains_key(&claim));
    assert_eq!(apply_delta_v2(&parent, &delta, &p).unwrap().state_root(), after.state_root());
    assert_eq!(revert_delta_v2(&after, &delta, &p).unwrap().state_root(), parent.state_root());
}
