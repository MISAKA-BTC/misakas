//! Production-core and node compatibility paths must share opaque types.
//! Behavioral checking remains covered by the actual signed-material fixtures.
#![cfg(feature = "positive-contract")]

use kaspa_consensus_core::{
    Hash64, palw_positive_check_v2 as core_check, palw_positive_contract_v2 as core_contract, palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_tir_class_v1::PalwTirClassV1, palw_v2::PalwJobContextV2,
};
use misaka_palw_positive_contract::ports::NodeChecker;
use misaka_palw_tir_exec::node::{positive_check as node_check, positive_contract as node_contract};
use std::any::TypeId;

fn same_type<Core: 'static, Compatibility: 'static>() {
    // Equal layout or matching field names would not satisfy TypeId equality.
    assert_eq!(TypeId::of::<Core>(), TypeId::of::<Compatibility>());
}

type CorePort = dyn NodeChecker<
        Context = core_contract::CheckContext,
        Progress = core_contract::ContractProgress,
        CheckedTerminal = core_contract::CheckedTerminal,
        Error = core_contract::AdapterError,
    >;
type CompatibilityPort = dyn NodeChecker<
        Context = node_contract::CheckContext,
        Progress = node_contract::ContractProgress,
        CheckedTerminal = node_contract::CheckedTerminal,
        Error = node_contract::AdapterError,
    >;

#[test]
fn both_public_paths_implement_the_same_nodechecker_port_with_identical_opaque_types() {
    // Each implementation must coerce to the OTHER path's fully constrained
    // public port. Separate copies of Context/Progress/Terminal would not compile.
    let _: &CorePort = &node_contract::TirNodeChecker;
    let _: &CompatibilityPort = &core_contract::TirNodeChecker;
    same_type::<core_contract::TirNodeChecker, node_contract::TirNodeChecker>();
    same_type::<core_contract::PreparedCheck, node_contract::PreparedCheck>();
    same_type::<core_contract::CheckContext, <node_contract::TirNodeChecker as NodeChecker>::Context>();
    same_type::<core_contract::ContractProgress, <node_contract::TirNodeChecker as NodeChecker>::Progress>();
    same_type::<core_contract::CheckedTerminal, <node_contract::TirNodeChecker as NodeChecker>::CheckedTerminal>();
    same_type::<core_contract::AdapterError, <node_contract::TirNodeChecker as NodeChecker>::Error>();
    same_type::<core_contract::SignedTransportQuote, node_contract::SignedTransportQuote>();
}

#[test]
fn record_cap_and_error_values_cross_compatibility_paths_without_conversion() {
    let limits: core_contract::record::RecordLimits = node_contract::record::RecordLimits::from(257u64);
    assert_eq!(limits, core_contract::record::RecordLimits { max_record_bytes: 257, max_generated_heap_bytes: 257 });
    let error: core_contract::record::RecordDeliveryError =
        node_contract::record::RecordDeliveryError::Adapter(node_contract::AdapterError::Check(node_check::CheckError::Budget));
    assert_eq!(
        error,
        core_contract::record::RecordDeliveryError::Adapter(core_contract::AdapterError::Check(core_check::CheckError::Budget),)
    );
    same_type::<core_contract::record::RecordLimits, node_contract::record::RecordLimits>();
    same_type::<core_contract::record::RecordDeliveryError, node_contract::record::RecordDeliveryError>();
}

#[test]
fn pure_plan_and_diagnostic_types_are_identical_across_the_production_boundary() {
    same_type::<core_check::CoveragePlan, node_check::CoveragePlan>();
    same_type::<core_check::ExactChecker, node_check::ExactChecker>();
    same_type::<core_check::CheckLimits, node_check::CheckLimits>();
    same_type::<core_check::CheckError, node_check::CheckError>();
    same_type::<core_check::CheckOutcome, node_check::CheckOutcome>();
    same_type::<core_check::CheckProgress, node_check::CheckProgress>();
    same_type::<core_check::CostEnvelope, node_check::CostEnvelope>();
    use core_check::expanded_plan as core_expanded;
    use node_check::expanded_plan as node_expanded;
    same_type::<core_expanded::ExpandedPlan, node_expanded::ExpandedPlan>();
    same_type::<core_expanded::ExpandedPlanLimits, node_expanded::ExpandedPlanLimits>();
    same_type::<core_expanded::ExpandedPlanError, node_expanded::ExpandedPlanError>();
    same_type::<core_expanded::ExpandedSummary, node_expanded::ExpandedSummary>();
    same_type::<core_expanded::ExpandedRelation, node_expanded::ExpandedRelation>();
    same_type::<core_expanded::ExpandedRelationKind, node_expanded::ExpandedRelationKind>();
    type MixedDeriver = fn(
        &PalwTirClassV1,
        Hash64,
        &PalwJobContextV2,
        PalwPromptIdsFormV1,
        node_check::CheckLimits,
    ) -> Result<core_check::CoveragePlan, core_check::CheckError>;
    let _: MixedDeriver = node_check::CoveragePlan::derive;
    type MixedExpandedDeriver = fn(
        &PalwTirClassV1,
        Hash64,
        &PalwJobContextV2,
        node_expanded::ExpandedPlanLimits,
    ) -> Result<core_expanded::ExpandedPlan, core_expanded::ExpandedPlanError>;
    let _: MixedExpandedDeriver = core_expanded::ExpandedPlan::derive;
}
