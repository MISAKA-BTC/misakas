//! Exhaustive fixed Assignment contract inventory. Native acceptance remains
//! a separate owner; these traits prevent hidden heap fields in a future edit.
use super::wire::{NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1, wire_bad, wire_struct};
use super::{NativeCoverageV1, NativeHeapV1, native_enum, native_struct, native_tuple_struct};
use misaka_palw_positive_contract::assignment::{consents::*, policy::*, role_actions::*, *};
use misaka_palw_positive_contract::assignment::{journal::*, rows::*};
use misaka_palw_positive_contract::{identity::*, rows::*};
use std::io::{self, Read};

native_tuple_struct!(AssignmentId; inner);
native_enum!(AssignmentModeV2; CanonicalTirAssignmentV1);
native_enum!(AssignmentRoleV2; Poster, Worker, Prover);
native_struct!(AssignmentTermsV2; version, revision, network, ruleset, branch_anchor, mode,
    registered_class, poster, worker, prover, payer, refund_payload, poster_nonce,
    derivation_policy, source_policy, reserve_schedule, worker_reward, max_assignment_fee,
    max_assignment_capital, max_assignment_locked, accept_before_daa, result_before_daa, reserve_until_daa);
native_struct!(FutureCaseFundingBoundsV2; assignment, payer, refund_payload, worker,
    max_case_fee, max_case_capital, max_case_locked, case_admission_before_daa, service_before_daa);
native_struct!(AssignmentFundingAuthorizationV2; assignment, quote, payer, funding_output_index,
    refund_payload, max_fee, max_capital, max_reward, max_locked, admission_expiry);
native_struct!(AssignmentAdmissionBundleV2; terms, job_context, quote, source_catalog,
    funding, future_case, source_consent, prover_consent, cancellation_policy);
native_struct!(AssignmentNonceV2; next, exhausted);
native_struct!(AssignmentSourceConsentV2; version, revision, assignment, worker, source_catalog,
    source_policy, budget, service_before_daa);
native_struct!(AssignmentProverConsentV2; version, revision, assignment, prover, positive_policy,
    max_attempts, max_build_work, max_submitted_bytes, max_retained_bytes, retention_until_daa,
    build_before_daa, max_service_fee, cancellation_policy);
native_struct!(AssignmentLiveCapsV2; global, per_poster, per_worker, per_prover, per_class);
native_struct!(AssignmentPolicyV2; version, revision, mode, positive_policy, derivation_policy,
    source_policy, assignment_fee_schedule, case_fee_schedule, reserve_schedule,
    physical_service_profile, cancellation_policy, live, max_frame_bytes, max_program_bytes,
    max_layout_entries, max_source_catalog_entries, max_role_candidates, max_prover_joins,
    max_history_frames, max_history_bytes, max_retained_bytes, max_replay_bytes,
    max_replay_work, max_heap_bytes, accept_window_daa, result_window_daa,
    build_window_daa, check_window_daa, fraud_tail_daa, refund_window_daa, retention_window_daa);
native_struct!(AssignmentCancellationV2; assignment, original_bundle, cancellation_policy, cancel_before_daa);
native_struct!(AssignmentProverJoinV2; version, revision, assignment, original_bundle, case,
    prover, payer, funding_output_index, max_fee, max_capital, max_locked, max_attempts,
    max_build_work, max_submitted_bytes, max_retained_bytes, admission_before_daa,
    service_before_daa, retention_until_daa, cancellation_policy);

// Fixed Borsh field inventory. No hidden Vec or collecting decoder.
wire_struct!(AssignmentId; Digest);
wire_struct!(AssignmentTermsV2; u16,u16,Digest,Digest,NativeHash64,AssignmentModeV2,Digest,NativeOutpoint,NativeOutpoint,NativeOutpoint,PrincipalId,Digest,u64,Digest,Digest,Digest,u64,u64,u64,u64,u64,u64,u64);
wire_struct!(FutureCaseFundingBoundsV2; AssignmentId,PrincipalId,Digest,NativeOutpoint,u64,u64,u64,u64,u64);
wire_struct!(AssignmentFundingAuthorizationV2; AssignmentId,Digest,PrincipalId,u32,Digest,u64,u64,u64,u64,u64);
wire_struct!(AssignmentAdmissionBundleV2; AssignmentTermsV2,Digest,Digest,Digest,AssignmentFundingAuthorizationV2,FutureCaseFundingBoundsV2,Digest,Digest,Digest);
wire_struct!(AssignmentNonceV2; u64,bool);
wire_struct!(AssignmentSourceConsentV2; u16,u16,AssignmentId,NativeOutpoint,Digest,Digest,misaka_palw_positive_contract::messages::AggregateBudget,u64);
wire_struct!(AssignmentProverConsentV2; u16,u16,AssignmentId,NativeOutpoint,Digest,u32,u64,u64,u64,u64,u64,u64,Digest);
wire_struct!(AssignmentProverJoinV2; u16,u16,AssignmentId,Digest,Option<(CaseId,ResultId)>,NativeOutpoint,PrincipalId,u32,u64,u64,u64,u32,u64,u64,u64,u64,u64,u64,Digest);
macro_rules! assignment_wire_enum {
    ($ty:ty; $($tag:literal => ($($field:ty),*)),* $(,)?) => {
        impl NativeWireScanV1 for $ty {
            fn scan_native_wire_v1<R:Read>(reader:&mut NativeWireReaderV1<R>)->io::Result<NativeWireShapeV1> {
                reader.step()?;
                match reader.byte()? {
                    $($tag => { #[allow(unused_mut)] let mut sum=NativeWireShapeV1::default();
                        $(sum=sum.merge(<$field as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;)* Ok(sum)
                    },)*
                    _=>Err(wire_bad("unknown assignment enum variant")),
                }
            }
        }
    };
}
assignment_wire_enum!(AssignmentModeV2; 0=>());
assignment_wire_enum!(AssignmentRoleV2; 0=>(),1=>(),2=>());
native_enum!(AssignmentStatusV2; Open, Consumed{case,result}, Cancelled, ExpiredUnconsumed);
assignment_wire_enum!(AssignmentStatusV2; 0=>(),1=>(CaseId,ResultId),2=>(),3=>());
native_enum!(AssignmentSettlementLegV2; WorkerPayment, ServiceFee, CapitalReturn, Refund, Burn);
assignment_wire_enum!(AssignmentSettlementLegV2; 0=>(),1=>(),2=>(),3=>(),4=>());
native_enum!(AssignmentRoleAuthorizationV2; Initial, Joined(x));
assignment_wire_enum!(AssignmentRoleAuthorizationV2; 0=>(),1=>(AssignmentProverJoinV2));
native_enum!(AssignmentJournalEventKindV2; Open, JoinProver, MetadataReceipt, Cancel, Expire, BindResult, Settlement);
assignment_wire_enum!(AssignmentJournalEventKindV2; 0=>(),1=>(),2=>(),3=>(),4=>(),5=>(),6=>());
native_struct!(AssignmentRow; bundle,source_consent,prover_consent,origin,worker_key,positive_policy,assignment_policy,fired_rules,job,input,context,registered_metadata,status,derivation,dependencies,reservation,liability,last_event,admission_material);
wire_struct!(AssignmentRow; AssignmentAdmissionBundleV2,AssignmentSourceConsentV2,AssignmentProverConsentV2,AcceptedOrigin,[u8;PUBKEY_BYTES],Digest,Digest,Digest,Digest,Digest,Digest,Digest,AssignmentStatusV2,AcceptedMaterial,AcceptedMaterial,AcceptedMaterial,AcceptedMaterial,AcceptedOrigin,AcceptedMaterial);
native_struct!(AssignmentNonceRow; poster,nonce);
wire_struct!(AssignmentNonceRow; NativeOutpoint,AssignmentNonceV2);
native_struct!(AssignmentUseRow; assignment,case,result,origin,binding);
wire_struct!(AssignmentUseRow; AssignmentId,CaseId,ResultId,AcceptedOrigin,AcceptedMaterial);
native_struct!(AssignmentFundingRow; assignment,authorization,payer_key,payer_signature,funding_origin,original_outpoint,current_outpoint,initial_locked,worker,fee,capital,surplus,worker_reserve,earned_service_due,liability_floor,reservation,liability,last_settlement);
wire_struct!(AssignmentFundingRow; AssignmentId,AssignmentFundingAuthorizationV2,[u8;PUBKEY_BYTES],[u8;SIGNATURE_BYTES],AcceptedOrigin,NativeOutpoint,Option<NativeOutpoint>,u64,BalanceLeg,BalanceLeg,BalanceLeg,BalanceLeg,u64,u64,u64,AcceptedMaterial,AcceptedMaterial,Option<AcceptedMaterial>);
native_struct!(AssignmentSettlementRow; assignment,funding,leg,party,ordinal,debit,recipient,amount,receipt,once,origin);
wire_struct!(AssignmentSettlementRow; AssignmentId,AssignmentSettlementFundingV2,AssignmentSettlementLegV2,PrincipalId,u64,NativeOutpoint,Digest,u64,AcceptedMaterial,AcceptedMaterial,AcceptedOrigin);
native_struct!(AssignmentRoleRow; assignment,role,bond,key,signature,bundle,authorization,admission,funding,lease,origin,retired);
wire_struct!(AssignmentRoleRow; AssignmentId,AssignmentRoleV2,NativeOutpoint,[u8;PUBKEY_BYTES],[u8;SIGNATURE_BYTES],Digest,AssignmentRoleAuthorizationV2,AcceptedMaterial,Option<AcceptedMaterial>,AcceptedMaterial,AcceptedOrigin,bool);
native_struct!(AssignmentJournalHeaderV2; version,revision,assignment,ordinal,previous,kind,origin,payload_digest,payload_bytes);
wire_struct!(AssignmentJournalHeaderV2; u16,u16,AssignmentId,u64,Digest,AssignmentJournalEventKindV2,AcceptedOrigin,Digest,u32);
native_struct!(AssignmentJournalHeadRow; assignment,next_ordinal,last_frame,origin,retention_until_daa);
wire_struct!(AssignmentJournalHeadRow; AssignmentId,u64,Digest,AcceptedOrigin,u64);
native_struct!(AssignmentJournalFrameRow; header,payload);
wire_struct!(AssignmentJournalFrameRow; AssignmentJournalHeaderV2,AcceptedMaterial);

native_struct!(crate::palw_positive_assignment_funds_v2::AssignmentAuthorizedDueV2; source_leg,kind,party,recipient,amount,receipt);
native_struct!(crate::palw_positive_assignment_funds_v2::AssignmentSpendAuthorizationV2; before,dues,protected_after,daa,max_frame_bytes,max_history_frames,payload);
native_struct!(crate::palw_positive_assignment_funds_v2::AssignmentFundingProjectionV2; worker,fee,capital,surplus,worker_reserve,earned_service_due,liability_floor,current_outpoint);
native_struct!(crate::palw_positive_assignment_funds_v2::AssignmentSpendPlanV2; old,new,transaction,daa,body_digest,burned_now,paid_now,returned_now,continued_now,authorization);

native_struct!(AssignmentProverFundingRow; assignment,consent,payer_key,payer_signature,funding_origin,original_outpoint,current_outpoint,initial_locked,fee,capital,surplus,earned_service_due,liability_floor,reservation,liability,last_settlement);
wire_struct!(AssignmentProverFundingRow; AssignmentId,AssignmentProverJoinV2,[u8;PUBKEY_BYTES],[u8;SIGNATURE_BYTES],AcceptedOrigin,NativeOutpoint,Option<NativeOutpoint>,u64,BalanceLeg,BalanceLeg,BalanceLeg,u64,u64,AcceptedMaterial,AcceptedMaterial,Option<AcceptedMaterial>);

native_enum!(AssignmentMetadataPurposeV2; Admission, FixedMetadataRetention);
assignment_wire_enum!(AssignmentMetadataPurposeV2;0=>(),1=>());
native_struct!(AssignmentMetadataReceiptV2;version,revision,assignment,purpose,provider,bytes,work,fee,schedule);
wire_struct!(AssignmentMetadataReceiptV2;u16,u16,AssignmentId,AssignmentMetadataPurposeV2,PrincipalId,u64,u64,u64,Digest);

native_enum!(AssignmentSettlementFundingV2;Poster,Prover(x));
assignment_wire_enum!(AssignmentSettlementFundingV2;0=>(),1=>(NativeOutpoint));

// Frozen admission material: exhaustive visitors + wire preflight before its
// fixed Borsh decode. Any future heap field requires a reviewed visitor update.
use crate::{
    Hash64,
    config::params::{AssignmentNativePreimagesV2, PositiveCourtPolicyV2},
    palw_positive_assignment_funds_v2::{
        AssignmentFeeScheduleV2, AssignmentMoneyQuoteV2, AssignmentPhysicalProfileV2, AssignmentPhysicalUsageV2,
    },
    palw_positive_assignment_material_v2::*,
    palw_positive_assignment_resources_v2::*,
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_security_certification_v1::PalwSecurityCertificationPolicyV1,
};
use misaka_palw_positive_contract::{assignment::open_material::ASSIGNMENT_OPEN_PREIMAGES_V2, messages::CheckProfile};
native_struct!(PositiveCourtPolicyV2;version,revision,profile,derivation_id,selector_id,service_profile_id,funding_schedule_id,max_frame_bytes,max_part_heap,max_relations,max_attempts,max_all_attempt_bytes,max_all_attempt_elements,max_all_attempt_terms,max_case_heap_bytes,max_case_journal_bytes,max_case_work,max_retention_daa);
wire_struct!(PositiveCourtPolicyV2;u16,u16,CheckProfile,Hash64,Hash64,Hash64,Hash64,u64,u64,u64,u32,u64,u64,u64,u64,u64,u64,u64);
native_struct!(AssignmentNativePreimagesV2;assignment_fees,physical,source_policy,case_fees,cancellation);
wire_struct!(AssignmentNativePreimagesV2;crate::palw_positive_assignment_funds_v2::AssignmentFeeScheduleV2,crate::palw_positive_assignment_funds_v2::AssignmentPhysicalProfileV2,crate::palw_positive_assignment_resources_v2::AssignmentSourcePolicyV2,crate::palw_positive_assignment_resources_v2::AssignmentCaseFeeScheduleV2,crate::palw_positive_assignment_resources_v2::AssignmentCancellationPolicyV2);
native_struct!(AssignmentFeeScheduleV2;version,revision,admission,per_signature,per_admission_byte,per_retained_byte_daa,per_replay_byte,per_utxo_write,capital_per_retained_byte);
wire_struct!(AssignmentFeeScheduleV2;u16,u16,u64,u64,u64,u64,u64,u64,u64);
native_struct!(AssignmentPhysicalProfileV2;version,revision,max_admission_bytes,max_retained_bytes,max_replay_bytes,max_heap_bytes,max_utxo_writes,max_signatures,max_retention_daa);
wire_struct!(AssignmentPhysicalProfileV2;u16,u16,u64,u64,u64,u64,u64,u64,u64);
native_struct!(AssignmentPhysicalUsageV2;admission_bytes,retained_bytes,replay_bytes,heap_bytes,utxo_writes,signatures);
wire_struct!(AssignmentPhysicalUsageV2;u64,u64,u64,u64,u64,u64);
native_struct!(AssignmentMoneyQuoteV2;fee,capital,usage,retention_daa,fee_schedule,reserve_schedule,physical_profile);
wire_struct!(AssignmentMoneyQuoteV2;u64,u64,AssignmentPhysicalUsageV2,u64,Digest,Digest,Digest);
native_struct!(AssignmentSourcePolicyV2;version,revision,work,max_parameter_instances,max_requests,max_responses,max_history_events,max_history_bytes,max_intervals,max_units_per_request,max_native_body_bytes);
wire_struct!(AssignmentSourcePolicyV2;u16,u16,AssignmentWorkScheduleV2,u64,u64,u64,u64,u64,u64,u64,u64);
native_struct!(AssignmentCaseFeeScheduleV2;version,revision,admission,per_source_unit,per_source_opening_byte,per_checked_relation,per_record_byte,capital_per_retained_byte);
wire_struct!(AssignmentCaseFeeScheduleV2;u16,u16,u64,u64,u64,u64,u64,u64);
native_struct!(AssignmentCancellationPolicyV2;version,revision,rule);
wire_struct!(AssignmentCancellationPolicyV2;u16,u16,AssignmentCancellationRuleV2);
native_struct!(AssignmentJobDescriptorV2;version,network_length,network,job_id,job_nullifier,assignment_id,execution_seed,model_profile_id,runtime_manifest_hash,runtime_class_id,shape_profile_id,trace_scheme_id,cu_ruleset_id,tokenizer_id,prompt_token_ids_hash,declared_prefill_tokens,exact_decode_tokens,max_context_tokens,context_hash);
wire_struct!(AssignmentJobDescriptorV2;u16,u8,[u8; 64],Hash64,Hash64,Hash64,[u8; 32],Hash64,Hash64,Hash64,Hash64,Hash64,Hash64,Hash64,Hash64,u32,u32,u32,Hash64);
native_struct!(AssignmentRegisteredDescriptorV2;class,artifact,program,layout,registered_tokenizer,registration);
wire_struct!(AssignmentRegisteredDescriptorV2;Digest,Digest,Digest,Digest,Digest,AcceptedMaterial);
native_struct!(AssignmentSourceQuoteDescriptorV2;digest,units,data_bytes,opening_bytes,max_unit_bytes);
wire_struct!(AssignmentSourceQuoteDescriptorV2;Digest,u64,u64,u64,u64);
native_struct!(AssignmentCustodyDescriptorV2;reservation,liability,preimage_bytes,retention_until_daa,retained_bytes,replay_bytes,replay_work,heap_bytes);
wire_struct!(AssignmentCustodyDescriptorV2;AcceptedMaterial,AcceptedMaterial,[Digest; ASSIGNMENT_OPEN_PREIMAGES_V2],u64,u64,u64,u64,u64);
native_struct!(AssignmentAdmissionMaterialV2;version,revision,assignment,unsigned_bundle,origin,network,ruleset,fired_rules,branch_anchor,assignment_policy,positive_policy,native_preimages,certification_policy,network_prompt_form,class_prompt_form,job,registered,source,money,custody);
wire_struct!(AssignmentAdmissionMaterialV2;u16,u16,AssignmentId,Digest,AcceptedOrigin,Digest,Digest,Digest,NativeHash64,AssignmentPolicyV2,PositiveCourtPolicyV2,AssignmentNativePreimagesV2,PalwSecurityCertificationPolicyV1,PalwPromptIdsFormV1,PalwPromptIdsFormV1,AssignmentJobDescriptorV2,AssignmentRegisteredDescriptorV2,AssignmentSourceQuoteDescriptorV2,AssignmentMoneyQuoteV2,AssignmentCustodyDescriptorV2);
wire_struct!(AssignmentLiveCapsV2;u32,u32,u32,u32,u32);
wire_struct!(AssignmentPolicyV2;u16,u16,AssignmentModeV2,Digest,Digest,Digest,Digest,Digest,Digest,Digest,Digest,AssignmentLiveCapsV2,u64,u32,u32,u32,u32,u32,u32,u64,u64,u64,u64,u64,u64,u64,u64,u64,u64,u64,u64);
native_enum!(AssignmentCancellationRuleV2;PreserveEarnedAndLiabilityV1);
assignment_wire_enum!(AssignmentCancellationRuleV2;0=>());

native_struct!(AssignmentWorkScheduleV2;version,revision,per_codec_step,per_hash_byte,per_signature,per_element,per_term,per_io_byte,per_control);

wire_struct!(AssignmentWorkScheduleV2;u16,u16,u64,u64,u64,u64,u64,u64,u64);
