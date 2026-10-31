//! Actual bound A2 inference/signing -> A3 builder -> A1 terminal -> A5 private
//! policy bridge. Source-journal/native authority is not exercised or granted.
mod pipeline_support;
#[allow(dead_code)]
mod support;

use misaka_palw_positive_accounting::credit::{
    ALL_CREDIT_USES, CreditDenial, CreditLimits, EconomicReadiness, FraudState, GateCApproval, PositiveCreditGate,
    UnavailableNativeCredit, consumers::CreditConsumers,
};
use misaka_palw_positive_accounting_node::tir::{TerminalEffect, TirCreditView};
use misaka_palw_positive_contract::{
    messages::ClaimStatus,
    ports::{CreditGate, NodeChecker},
};
use misaka_palw_tir_exec::node::{positive_check::CheckOutcome, positive_contract::TirNodeChecker};
use pipeline_support::*;

#[test]
fn bound_a2_rejects_parameters_that_do_not_open_to_the_fixed_registration() {
    use misaka_palw_remote::miner::positive_v2::inference::AuthenticatedTirParamsV2;
    use misaka_palw_tir_exec::{TirParams, TirPlan};
    let trusted = support::registration(false, 1, 7);
    let plan = TirPlan::compile(&support::program()).unwrap();
    let foreign = support::map(true);
    let foreign_params = TirParams::from_map(&plan, &foreign).unwrap();
    assert!(AuthenticatedTirParamsV2::authenticate(&plan, &foreign_params, support::d(trusted.artifact), parameter_limits()).is_err());
    let registered = support::map(false);
    let registered_params = TirParams::from_map(&plan, &registered).unwrap();
    let held =
        AuthenticatedTirParamsV2::authenticate(&plan, &registered_params, support::d(trusted.artifact), parameter_limits()).unwrap();
    assert_eq!(held.artifact(), support::d(trusted.artifact));
    assert_eq!(held.bytes_hashed(), 16);
}

#[test]
fn bound_a2_foreign_class_job_and_prompt_never_reach_the_signer() {
    use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
    use misaka_palw_remote::miner::positive_v2::{
        InferenceLimitsV2,
        inference::{AuthenticatedTirParamsV2, TirMinerJobV2, infer_and_sign_tir_result_v2},
    };
    use misaka_palw_tir_exec::{TirParams, TirPlan};
    use std::cell::Cell;
    let trusted = support::registration(false, 1, 7);
    let signed = a2_result(&trusted, false, 1, false);
    let plan = TirPlan::compile(&support::program()).unwrap();
    let map = support::map(false);
    let params = TirParams::from_map(&plan, &map).unwrap();
    let held = AuthenticatedTirParamsV2::authenticate(&plan, &params, support::d(trusted.artifact), parameter_limits()).unwrap();
    for dimension in 0..3 {
        let mut class = trusted.class.clone();
        let mut job = trusted.job.clone();
        let prompt = [if dimension == 2 { 2 } else { 1 }];
        match dimension {
            0 => class.tokenizer_id = support::h(888),
            1 => job.job_id = support::h(999),
            _ => {}
        }
        let calls = Cell::new(0);
        let result = infer_and_sign_tir_result_v2(
            &trusted.admission(),
            signed.result.clone(),
            &signed.service,
            &held,
            TirMinerJobV2 { class: &class, context: &job, prompt: &prompt, prompt_form: PalwPromptIdsFormV1::Flat },
            InferenceLimitsV2 { max_prefill: 4, max_decode: 4, max_positions: 8 },
            1,
            &signed.key,
            |_, _| {
                calls.set(calls.get() + 1);
                Err("foreign context must be refused before signing".to_string())
            },
        );
        assert!(result.is_err(), "dimension {dimension}");
        assert_eq!(calls.get(), 0, "dimension {dimension}");
    }
}

#[test]
fn committed_a2_a3_a1_and_a5_connect_with_independent_expected_tokens() {
    for prompt in 0..4 {
        let trusted = support::registration(false, prompt, 7);
        let a2 = a2_result(&trusted, false, prompt, false);
        assert_eq!(a2.generated, support::oracle(false, prompt).0);
        let packet = a3_packet(&trusted, &a2, false, prompt);
        assert_eq!(packet.miner_sig, a2.signature.as_slice());
        let (descriptor, terminal) = a1_complete(&packet, &trusted);
        assert_eq!(terminal.outcome(), CheckOutcome::Matched);
        let economic = PrivateEconomicFixture::new(&a2.result, descriptor);
        economic.set_fixture_ready();
        let mut view = TirCreditView::new(&economic, 1).unwrap();
        assert_private_consumers(&view, a2.result.claim_id(), false);
        let authenticated = a2.result.authenticate(&a2.key, &a2.signature).unwrap();
        assert_eq!(view.record_checked(&authenticated, terminal), Ok(TerminalEffect::Recorded));
        assert_private_consumers(&view, a2.result.claim_id(), true);
        // A passing local/private policy view never manufactures a native row.
        let native = CreditConsumers::new(&UnavailableNativeCredit);
        assert!(native.inference_reward(&a2.result.claim_id()).is_err());
        assert!(native.work_receipt(&a2.result.claim_id()).is_err());
        assert_eq!(native.dag_weight(&a2.result.claim_id(), u128::MAX), 0);
        assert_eq!(native.rule_e_capital(&a2.result.claim_id(), u128::MAX), 0);
        assert!(native.settlement(&a2.result.claim_id()).is_err());
        assert!(native.prefix_reuse(&a2.result.claim_id()).is_err());
        assert!(native.dependent_claim(&a2.result.claim_id()).is_err());
        assert!(misaka_palw_positive_contract::require_native_activation().is_err());
    }
}

#[test]
fn checked_math_does_not_replace_gate_c_funding_maturity_or_current_fraud_state() {
    let trusted = support::registration(false, 1, 7);
    let a2 = a2_result(&trusted, false, 1, false);
    let packet = a3_packet(&trusted, &a2, false, 1);
    let (descriptor, terminal) = a1_complete(&packet, &trusted);
    let economic = PrivateEconomicFixture::new(&a2.result, descriptor);
    let mut view = TirCreditView::new(&economic, 1).unwrap();
    view.record_checked(&a2.result.authenticate(&a2.key, &a2.signature).unwrap(), terminal).unwrap();
    let claim = a2.result.claim_id();
    for stage in 0..3 {
        assert_private_consumers(&view, claim, false);
        let mut facts = economic.facts.get();
        match stage {
            0 => facts.gate_c = GateCApproval::Approved(facts.binding),
            1 => facts.maturity_and_liability = EconomicReadiness::Ready,
            2 => facts.carrier_and_funding = EconomicReadiness::Ready,
            _ => unreachable!(),
        }
        economic.facts.set(facts);
    }
    assert_private_consumers(&view, claim, true);
    let mut facts = economic.facts.get();
    facts.fraud = FraudState::AcceptedPending;
    economic.facts.set(facts);
    assert_private_consumers(&view, claim, false);
    facts.fraud = FraudState::Clear;
    facts.gate_c = GateCApproval::Pending;
    economic.facts.set(facts);
    let gate = PositiveCreditGate::new(&view, CreditLimits { max_claims: 8, max_edges: 8 }).unwrap();
    for usage in ALL_CREDIT_USES {
        assert_eq!(gate.authorize(&claim, usage).err(), Some(CreditDenial::GateC(claim)));
    }
    economic.set_fixture_ready();
    let mut other_scope = economic.scope.get();
    other_scope.branch = support::d(support::h(555));
    economic.scope.set(other_scope);
    assert_private_consumers(&view, claim, false);
}

#[test]
fn a2_signed_wrong_output_is_checked_mismatch_and_never_private_credit() {
    let trusted = support::registration(false, 1, 7);
    // A signer can lie. A2 result authentication is not computation checking.
    let a2 = a2_result(&trusted, false, 1, true);
    a2.result.authenticate(&a2.key, &a2.signature).unwrap();
    let packet = a3_packet(&trusted, &a2, false, 1);
    let (descriptor, terminal) = a1_complete(&packet, &trusted);
    assert_eq!(terminal.outcome(), CheckOutcome::OutputMismatch);
    let economic = PrivateEconomicFixture::new(&a2.result, descriptor);
    economic.set_fixture_ready();
    let mut view = TirCreditView::new(&economic, 1).unwrap();
    view.record_checked(&a2.result.authenticate(&a2.key, &a2.signature).unwrap(), terminal).unwrap();
    assert_private_consumers(&view, a2.result.claim_id(), false);
    let gate = PositiveCreditGate::new(&view, CreditLimits { max_claims: 8, max_edges: 8 }).unwrap();
    for usage in ALL_CREDIT_USES {
        assert_eq!(gate.authorize(&a2.result.claim_id(), usage).err(), Some(CreditDenial::PositiveNotPassed(a2.result.claim_id())));
    }
}

#[test]
fn missing_last_a3_frame_cannot_produce_a5_terminal_authority() {
    let trusted = support::registration(false, 1, 7);
    let a2 = a2_result(&trusted, false, 1, false);
    let packet = a3_packet(&trusted, &a2, false, 1);
    let context = packet.authenticate(&trusted).unwrap();
    let descriptor = TirNodeChecker.derive_plan(&context).unwrap();
    let mut progress = context.start().unwrap();
    for bytes in &packet.frames[..packet.frames.len() - 1] {
        TirNodeChecker.check_authenticated_part(&context, &mut progress, packet.authenticated(bytes).unwrap()).unwrap();
    }
    assert!(TirNodeChecker.finish(&context, progress).is_err());
    let economic = PrivateEconomicFixture::new(&a2.result, descriptor);
    economic.set_fixture_ready();
    assert_eq!(economic.facts.get().status, ClaimStatus::PositivePassed); // deliberately untrusted saved flag
    let view = TirCreditView::new(&economic, 1).unwrap();
    assert_private_consumers(&view, a2.result.claim_id(), false);
    let gate = PositiveCreditGate::new(&view, CreditLimits { max_claims: 8, max_edges: 8 }).unwrap();
    for usage in ALL_CREDIT_USES {
        assert_eq!(
            gate.authorize(&a2.result.claim_id(), usage).err(),
            Some(CreditDenial::MissingLocalVerification(a2.result.claim_id()))
        );
    }
}
