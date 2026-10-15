//! Audit-time Master32 composite Final (lead default 2026-10-15, pending user
//! review). Not a consensus object: one pinned header pair, its verified full aux
//! opening, the owning ruleset and public tokens/outputs determine the verdict.
use crate::native_audit_v32::native_inner_state_of_pinned_headers_v32;
use kaspa_consensus_core::{
    Hash64,
    config::params::Params,
    header::Header,
    palw_fork_choice_commitment_v1::PalwForkChoiceOpeningV1,
    palw_native_audit_proof_v32::{CheckedNativeMasterCompositeV32, derive_native_master_composite_v32, verify_native_aux_v32},
    palw_state_proof_v1::PalwFactProofV1,
};

#[allow(clippy::too_many_arguments)]
pub fn derive_anchored_master_composite_v32(
    child: &Header,
    pinned_child: Hash64,
    parent: &Header,
    pinned_selected_parent: Hash64,
    parameters: &Params,
    fork_opening: Option<&PalwForkChoiceOpeningV1>,
    auxiliary: &PalwFactProofV1,
    master: [u8; 64],
    jobs: &[[u8; 64]],
    values: &[Vec<misaka_palw_tir::Tensor>],
    generated: &[u32],
) -> Result<CheckedNativeMasterCompositeV32, String> {
    let inner =
        native_inner_state_of_pinned_headers_v32(child, pinned_child, parent, pinned_selected_parent, parameters, fork_opening)?;
    let verified = verify_native_aux_v32(auxiliary, inner)?;
    derive_native_master_composite_v32(&verified, parameters, parent.daa_score, master, jobs, values, generated)
}

/// Complete-model native receipt, distinct from any one component's Final.
#[allow(clippy::too_many_arguments)]
pub fn derive_anchored_master_execution_v32(
    child:&Header,pinned_child:Hash64,parent:&Header,pinned_parent:Hash64,parameters:&Params,
    fork_opening:Option<&PalwForkChoiceOpeningV1>,auxiliary:&PalwFactProofV1,master:[u8;64],
    jobs:&[[u8;64]],values:&[Vec<misaka_palw_tir::Tensor>],generated:&[u32],
)->Result<kaspa_consensus_core::palw_native_audit_proof_v32::CheckedNativeMasterExecutionV32,String>{
    derive_anchored_master_execution_certified_v32(child,pinned_child,parent,pinned_parent,parameters,fork_opening,auxiliary,master,jobs,values,None,generated)
}
#[allow(clippy::too_many_arguments)]
pub fn derive_anchored_master_execution_certified_v32(
    child:&Header,pinned_child:Hash64,parent:&Header,pinned_parent:Hash64,parameters:&Params,
    fork_opening:Option<&PalwForkChoiceOpeningV1>,auxiliary:&PalwFactProofV1,master:[u8;64],
    jobs:&[[u8;64]],values:&[Vec<misaka_palw_tir::Tensor>],certificates:Option<&[Vec<misaka_palw_kernel::seg::NodeOpeningV1>]>,generated:&[u32],
)->Result<kaspa_consensus_core::palw_native_audit_proof_v32::CheckedNativeMasterExecutionV32,String>{
    let final_policy=parameters.palw_native_final_v27.as_ref().filter(|f|f.activation.is_active(parent.daa_score))
        .ok_or("complete Master32 owning Final fence inactive")?;
    let stage=parameters.palw_stage_execution_v28.as_ref().filter(|f|f.activation.is_active(parent.daa_score))
        .ok_or("complete Master32 owning stage fence inactive")?;
    if stage.policies.final_policy!=final_policy.policy.root(){return Err("complete Master32 independent owning Final policy differs".into());}
    let inner=native_inner_state_of_pinned_headers_v32(child,pinned_child,parent,pinned_parent,parameters,fork_opening)?;
    let verified=verify_native_aux_v32(auxiliary,inner)?;
    kaspa_consensus_core::palw_native_audit_proof_v32::derive_native_master_execution_record_certified_v32(&verified,parameters,parent.daa_score,master,jobs,values,certificates,generated)
}
/// No raw DTO can mint this report: the input is the private checked Core view.
/// The master has an immutable SET of original component credits, never an
/// additional master coin credit. Delivery33/spend35 must cover every member.
pub fn master_execution_report_v32(checked:&kaspa_consensus_core::palw_native_audit_proof_v32::CheckedNativeMasterExecutionV32,
    parameters:&Params,child:Hash64,parent:Hash64)->serde_json::Value{
    let r=checked.record();let hex=crate::runtime_pack::commit::hex;
    let class_set=r.class_set;
    let components:Vec<_>=r.components.iter().map(|c|serde_json::json!({"stage":c.stage,"class":hex(&c.class),"program":hex(&c.program),
        "source":hex(&c.source),"inventory":hex(&c.inventory),"real_parameter":hex(&c.real_parameter),"job":hex(&c.job),
        "original_funding":hex(&c.original_funding),"initial_consent":hex(&c.consent),"maximum_contract":hex(&c.maximum_contract),
        "maximum_scope":hex(&c.maximum_scope),"maximum_resource":hex(&c.maximum_resource),"original_quota":hex(&c.original_quota),
        "maximum_invocation":hex(&c.maximum_invocation),"maximum_court":hex(&c.maximum_court),"invocation":hex(&c.invocation),
        "input_binding":hex(&c.input_binding),"claim":hex(&c.claim),"final_book":hex(&c.final_book),"settlement":hex(&c.settlement),
        "single_credit":hex(&c.single_credit),"payout_queue":hex(&c.payout_queue),"requester_operator":hex(&c.requester_operator),
        "producer_operator":hex(&c.producer_operator),"admitted_daa":c.admitted,"committed_daa":c.committed,
        "established_daa":c.established,"challenge_end":c.challenge_end,"deadline":c.deadline,"reward":c.reward})).collect();
    serde_json::json!({"schema":"misaka.palw.pinned-native-master-execution-audit.v32","namespace":32,
        "network":parameters.net.to_string(),"genesis":parameters.genesis.hash.to_string(),"child":child.to_string(),"selected_parent":parent.to_string(),
        "inner_state":checked.state().to_string(),"aux_root":checked.auxiliary_root().to_string(),"master":hex(&r.master),"composite":hex(&r.composite),
        "complete_source":hex(&r.complete_source),"manifest":hex(&r.manifest),"complete_inventory":hex(&r.complete_inventory),
        "program_set":hex(&r.program_set),"real_parameter_set":hex(&r.real_parameter_set),"maximum_scope_set":hex(&r.maximum_scope_set),
        "maximum_resource_set":hex(&r.maximum_resource_set),"single_credit_set":hex(&r.single_credit_set),"components":components,
        "complete_model_binding":{"namespace":32,"native_class_set":hex(&class_set),"complete_source":hex(&r.complete_source),"manifest":hex(&r.manifest),
            "complete_inventory":hex(&r.complete_inventory),"program_set":hex(&r.program_set),"real_parameter_set":hex(&r.real_parameter_set),
            "maximum_scope_set":hex(&r.maximum_scope_set),"maximum_resource_set":hex(&r.maximum_resource_set)},
        "local_qualified_validation_network":parameters.native_final_is_local_qualified_v34(),
        "complete_model_identity":{"native_class":hex(&class_set),"native_source":hex(&r.complete_source),"program_root":hex(&r.program_set),
            "inventory_root":hex(&r.complete_inventory),"real_parameter_root":hex(&r.real_parameter_set)},
        "pinned_headers_and_original_rows_verified":true,"all_original_component_finals_and_credits_verified":true,
        "qualified_native_profile":parameters.native_final_is_qualified_v34(),"final_receipt_class":if parameters.native_final_is_qualified_v34(){"qualified-final"}else{"measurement-final"},
        "calibration_receipts_are_not_A6_or_A8_evidence":!parameters.native_final_is_qualified_v34(),
        "master_mints_extra_credit":false,"A6_or_A8_credit":false})
}

pub fn derive_anchored_master_lifecycle_v32(child:&Header,child_pin:Hash64,parent:&Header,parent_pin:Hash64,params:&Params,aux:&PalwFactProofV1,phase:kaspa_consensus_core::palw_native_audit_proof_v32::NativeLifecycleStageV32,master:[u8;64],jobs:&[[u8;64]])->Result<kaspa_consensus_core::palw_native_audit_proof_v32::CheckedNativeMasterLifecycleV32,String>{
    let inner=native_inner_state_of_pinned_headers_v32(child,child_pin,parent,parent_pin,params,None)?;
    let verified=verify_native_aux_v32(aux,inner)?;
    kaspa_consensus_core::palw_native_audit_proof_v32::derive_native_master_lifecycle_v32(&verified,params,parent.daa_score,phase,master,jobs)
}
pub fn master_lifecycle_report_v32(checked:&kaspa_consensus_core::palw_native_audit_proof_v32::CheckedNativeMasterLifecycleV32,params:&Params,child:Hash64,parent:Hash64)->serde_json::Value{
    let r=checked.record();let hex=crate::runtime_pack::commit::hex;
    let components:Vec<_>=r.components.iter().map(|c|serde_json::json!({"stage":c.stage,"class":hex(&c.class),"source":hex(&c.source),"program":hex(&c.program),"inventory":hex(&c.inventory),"real_parameter":hex(&c.real_parameter),"maximum_scope":hex(&c.maximum_scope),"maximum_resource":hex(&c.maximum_resource),"job":hex(&c.job),"original_funding":hex(&c.funding),"initial_consent":hex(&c.consent),"requester_operator":hex(&c.requester_operator),"producer_operator":hex(&c.producer_operator),"admitted_daa":c.admitted,"terminal":c.terminal})).collect();
    serde_json::json!({"schema":"misaka.palw.pinned-native-master-lifecycle-audit.v32","namespace":32,"stage":if r.phase==0{"register"}else{"seat"},"master":hex(&r.master),"network":params.net.to_string(),"genesis":params.genesis.hash.to_string(),"child":child.to_string(),"selected_parent":parent.to_string(),"inner_state":checked.state().to_string(),"aux_root":checked.auxiliary_root().to_string(),"complete_source":hex(&r.complete_source),"manifest":hex(&r.manifest),"complete_inventory":hex(&r.complete_inventory),"native_class_set":hex(&r.class_set),"program_set":hex(&r.program_set),"real_parameter_set":hex(&r.real_parameter_set),"class_scope_set":hex(&r.class_scope_set),"class_resource_set":hex(&r.class_resource_set),"original_nonce":hex(&r.original_nonce),"requester_operator":hex(&r.requester_operator),"registered_daa":r.registered,"components":components,"pinned_headers_and_original_rows_verified":true,"all_original_components_verified":true,"qualified_native_profile":params.native_final_is_qualified_v34(),"local_qualified_validation_network":params.native_final_is_local_qualified_v34(),"Final_or_coin_credit_granted":false})
}
