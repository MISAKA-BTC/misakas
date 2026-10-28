//! Real finite-consumer planner guard: a producer must authorize material first.
#![allow(warnings)]
use super::*;
use crate::palw_bond_budget_v1::{
    PalwBondBudgetMirrorV1, PalwBondBudgetPolicyV1, PalwPaidConsumerLifecycleMirrorV1, PalwPaidConsumerLifecyclePolicyV1,
};

#[test]
fn finite_consumer_retention_is_authorized_before_the_contract_is_planned() {
    let policy =
        PalwPaidConsumerLifecyclePolicyV1 { version: 1, service_span_daa: 1_000, dispute_tail_daa: 3_000, material_tail_daa: 2_000 };
    let p = params()
        .with_bond_budget(Some(PalwBondBudgetMirrorV1 {
            from_daa: 0,
            policy: PalwBondBudgetPolicyV1::unapproved_probe_v1(),
            allocation: None,
        }))
        .with_reporter_income_from(Some(0))
        .with_material_income_from(Some(0))
        .with_paid_consumer_lifecycle(Some(PalwPaidConsumerLifecycleMirrorV1 { from_daa: 200, policy }));
    let state = PalwChainStateV2::genesis();
    let extras = PalwTransitionExtrasV1::default();
    let b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
    let id = h64(0xC1);
    let mut claim = palw_claim_template_v1(p.base_class_id(), bond_key(5), 200, 0, 0);
    claim.trace_retention_daa = 200 + crate::palw_producer_v2::palw_min_trace_retention_daa_v1(&p);
    let error = b.plan_paid_consumer_v1(id, &claim).unwrap_err();
    assert!(error.to_string().contains("already authorized material"));
    claim.trace_retention_daa = 200 + crate::palw_producer_v2::palw_min_trace_retention_daa_at_v2(&p, 200);
    let contract = b.plan_paid_consumer_v1(id, &claim).unwrap().unwrap();
    assert_eq!(contract.terms, policy.terms(200).unwrap());
    assert_eq!(claim.trace_retention_daa, contract.terms.material_until);
    assert!(state.paid_consumer_contract_v1(&id).is_none(), "planning alone creates no contract or authorization");
}
