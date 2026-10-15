//! Generated structural Borsh visitors; manual codecs are separate adapters.
#![allow(unused_mut)]
use super::wire::wire_struct;
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
wire_struct!(crate::evm::EvmAddress; [u8; crate::evm::EVM_ADDRESS_SIZE]);
impl NativeWireScanV1 for crate::evm::model_market::PalwEvmSettlementOutcomeV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::evm::model_market::PalwEvmSettlementV1; u32, crate::evm::EvmAddress, crate::Hash64, u32, u64, crate::evm::model_market::PalwEvmSettlementOutcomeV1);
wire_struct!(crate::palw_activation_pool_v1::PalwActivationCreditV1; crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, crate::Hash64);
wire_struct!(crate::palw_activation_pool_v1::PalwActivationPoolCountersV1; u128, u128, u128, u128, u128, u128);
wire_struct!(crate::palw_activation_pool_v1::PalwActivationPoolV1; u64, u64, u64, u64, u64, u64, u64, Vec<crate::Hash64>, Vec<crate::Hash64>, Vec<crate::palw_activation_pool_v1::PalwActivationCreditV1>);
wire_struct!(crate::palw_aggregate_liability_v1::PalwBondFreezeV1; u64, crate::Hash64, u64, bool);
wire_struct!(crate::palw_artifact::PalwArtifactOpeningV1; crate::palw_artifact::PalwArtifactOperandV1, u32, u32, Vec<crate::Hash64>);
wire_struct!(crate::palw_artifact::PalwArtifactOperandV1; String, Option<u16>, u32, Vec<u8>);
wire_struct!(crate::palw_attempt_material_v3::PalwNativeMaterialBindingV3; crate::Hash64, crate::Hash64, crate::palw_material_capacity_v1::PalwMaterialManifestV1);
wire_struct!(crate::palw_attempt_material_v3::PalwNativeMaterialSessionV3; u16, crate::Hash64, crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, crate::Hash64, crate::Hash64, crate::palw_material_capacity_v1::PalwMaterialCodecV1, u32, u8, u64, u64);
wire_struct!(crate::palw_attn_court_v1::PalwAttnCheckpointAnchorV1; crate::palw_step_leg::PalwCheckpointLeafV2, crate::palw_step_leg::PalwStepOpeningV1);
wire_struct!(crate::palw_attn_court_v1::PalwAttnChunkOpeningV1; u32, Vec<u8>, Vec<crate::Hash64>);
wire_struct!(crate::palw_attn_court_v1::PalwAttnDissectPhaseV1; crate::Hash64, u8, u32, u16, u16, u16, u32, i32, i64, crate::palw_attn_dissect::PalwAttnRangeClaimV1, u64, u64, u32, crate::palw_bisect::PalwBisectTurnV1, u64, Vec<crate::palw_attn_dissect::PalwAttnRangeClaimV1>);
wire_struct!(crate::palw_attn_dissect::PalwAttnRangeClaimV1; i32, i64, Vec<i64>);
wire_struct!(crate::palw_audit_door_v1::PalwAuditStatusV1; Vec<(crate::palw_state_v2::PalwBondKeyV2, u64)>);
wire_struct!(crate::palw_beacon_bootstrap_v1::PalwBootstrapJudgementV1; u64, [u8; 64], Option<[u8; 64]>, u32, u64, Option<String>, Option<String>);
wire_struct!(crate::palw_beacon_bootstrap_v1::PalwBootstrapRoundRowV1; misaka_palw_kernel::beacon_bootstrap_v1::BootstrapExactRoundV1, u64, Vec<crate::palw_beacon_bootstrap_v1::PalwBootstrapSealRowV1>, Option<crate::palw_beacon_bootstrap_v1::PalwBootstrapJudgementV1>, crate::Hash64);
wire_struct!(crate::palw_beacon_bootstrap_v1::PalwBootstrapSealRowV1; [u8; 64], [u8; 64], [u8; 64], u64, Option<([u8; 64], u64)>);
wire_struct!(crate::palw_beacon_source_fee_v1::PalwBeaconSourceCostBoundsV1; u64, u64, u64, u64, u64, u64, u64, u64, [u8; 64]);
wire_struct!(crate::palw_beacon_source_opt_in_v1::PalwBeaconSourcePolicyV1; u64, u32, u32, u64);
wire_struct!(crate::palw_bisect::PalwBisectLadderV1; crate::Hash64, u64, u64, u64, u32, crate::palw_bisect::PalwBisectTurnV1, u64, crate::Hash64, crate::Hash64, Vec<(u64, crate::Hash64)>);
impl NativeWireScanV1 for crate::palw_bisect::PalwBisectTurnV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_bond_budget_v1::PalwAllocationEpochV1; u64, u64, crate::palw_bond_budget_v1::PalwAllocationWeightV1, u64);
wire_struct!(crate::palw_bond_budget_v1::PalwBondBudgetHeaderV1; u16, u64, crate::Hash64, Option<crate::palw_bond_budget_v1::PalwAllocationEpochV1>);
wire_struct!(crate::palw_bond_budget_v1::PalwBudgetBondRowV1; crate::palw_bond_budget_v1::PalwBudgetVectorV1, u32, u64, u128, u128, u64);
wire_struct!(crate::palw_bond_budget_v1::PalwBudgetClaimRowV1; crate::palw_state_v2::PalwBondKeyV2, Option<(crate::Hash64, u64)>, u64, u64, crate::palw_bond_budget_v1::PalwBudgetVectorV1, crate::palw_bond_budget_v1::PalwBudgetVectorV1, bool, bool, bool, crate::palw_bond_budget_v1::PalwBudgetOriginV1, u128);
impl NativeWireScanV1 for crate::palw_bond_budget_v1::PalwBudgetOriginV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            6 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            7 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            8 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            9 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            10 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            11 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            12 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            13 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            14 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            15 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_bond_budget_v1::PalwBudgetVectorV1; u64, u64, u64, u128, u64);
wire_struct!(crate::palw_bond_budget_v1::PalwCapitalAssignmentRowV1; u64, Vec<(crate::Hash64, u64)>, Option<Vec<(crate::Hash64, u64)>>, u64);
wire_struct!(crate::palw_bond_budget_v1::PalwModelBudgetRowV1; u64, u128, crate::palw_bond_budget_v1::PalwAllocationWeightV1, u64);
wire_struct!(crate::palw_bond_budget_v1::PalwPaidConsumerContractV1; crate::palw_bond_budget_v1::PalwPaidConsumerTermsV1, crate::palw_bond_budget_v1::PalwPaidConsumerStatusV1);
impl NativeWireScanV1 for crate::palw_bond_budget_v1::PalwPaidConsumerStatusV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_bond_budget_v1::PalwPaidConsumerTermsV1; u64, u64, u64, u64, crate::Hash64);
wire_struct!(crate::palw_bond_budget_v1::PalwReporterIncomeBindingV1; Option<crate::palw_state_v2::PalwBondKeyV2>, crate::Hash64, u64, u64, Option<u64>, Option<crate::Hash64>, Option<u64>, Option<(u128, u64)>);
wire_struct!(crate::palw_bond_budget_v1::PalwMaterialIncomeV1; Option<crate::Hash64>, u64, u64, crate::palw_bond_budget_v1::PalwMaterialPaymentV1);
impl NativeWireScanV1 for crate::palw_bond_budget_v1::PalwMaterialPaymentV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<Option<crate::Hash64> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<Option<crate::Hash64> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<Option<u64> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<Option<u64> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_capacity_s567_v1::PalwBondBreakerV1; u64, u8, u8, u32, u32, bool);
impl NativeWireScanV1 for crate::palw_capacity_s567_v1::PalwCapacityLedgerKeyV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::palw_state_v2::PalwBondKeyV2 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for crate::palw_capacity_s567_v1::PalwCapacityLedgerRowV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::palw_capacity_s567_v1::PalwRhoBreakerV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum =
                    sum.merge(<crate::palw_capacity_s567_v1::PalwBondBreakerV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_capacity_s567_v1::PalwRhoBreakerV1; u64, u8, u8, u32, u32, u32, u32, u32, u32, u64, u64, Vec<crate::palw_state_v2::PalwBondKeyV2>);
wire_struct!(crate::palw_claim_challenge_v2::PalwPaidCheckerCandidateV2; crate::palw_state_v2::PalwBondKeyV2, crate::Hash64, u64);
wire_struct!(crate::palw_claim_challenge_v2::PalwPaidClaimAuthorityV2; u16, misaka_palw_kernel::claim_paid_v2::PaidClaimEnrollmentV2, misaka_palw_kernel::claim_paid_v2::PaidClaimClassV2, misaka_palw_kernel::claim_paid_v2::PaidClaimJobV2, misaka_palw_challenge::subject::ChallengeSubjectV1, misaka_palw_challenge::beacon::BeaconContextV1, misaka_palw_kernel::beacon_source::BeaconSourceTargetV1, crate::palw_beacon_source_opt_in_v1::PalwBeaconSourcePolicyV1, u64, crate::palw_state_v2::PalwBondKeyV2, Option<crate::palw_state_v2::PalwBondKeyV2>, Vec<crate::palw_claim_challenge_v2::PalwPaidCheckerCandidateV2>);
impl NativeWireScanV1 for crate::palw_claim_challenge_v2::PalwPaidClaimEvidenceV3 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(
                    <misaka_palw_challenge::approval_scope::SourceEconomicObservationV3 as NativeWireScanV1>::scan_native_wire_v1(
                        reader,
                    )?,
                )?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(
                    <misaka_palw_challenge::approval_scope::SourceIndependenceObservationV3 as NativeWireScanV1>::scan_native_wire_v1(
                        reader,
                    )?,
                )?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_claim_challenge_v2::PalwPaidClaimFinalV2; [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], misaka_palw_kernel::claim_paid_v2::PaidClaimLifetimeV2, u32, u64, u32, [u8;64]);
impl NativeWireScanV1 for crate::palw_claim_challenge_v2::PalwPaidClaimIndexKindV3 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_claim_challenge_v2::PalwPaidClaimIndexV3; u16, crate::palw_claim_challenge_v2::PalwPaidClaimIndexKindV3, [u8;64], [u8;64]);
wire_struct!(crate::palw_claim_challenge_v2::PalwPaidClaimLockV2; [u8;64], [u8;64], [u8;64], [u8;64], u64, [u8;64], Vec<crate::palw_state_v2::PalwBondKeyV2>, u32, bool);
wire_struct!(crate::palw_claim_interactive_v3::PalwPaidClaimRoundV3; [u8;64], u32, [u8;64], [u8;64], u64, misaka_palw_challenge::subject::ChallengeSubjectV1, misaka_palw_challenge::beacon::BeaconContextV1, misaka_palw_kernel::beacon_source::BeaconSourceTargetV1, Option<crate::palw_state_v2::PalwBondKeyV2>, Option<crate::palw_claim_challenge_v2::PalwPaidClaimLockV2>, [u8;64], Vec<misaka_palw_kernel::claim_paid_v2::PaidClaimReceiptV2>);
wire_struct!(crate::palw_class_semantics_v1::PalwClassSemanticsV1; u16, crate::Hash64, crate::Hash64, u64, crate::Hash64, crate::Hash64, crate::Hash64, crate::Hash64, crate::Hash64, Option<crate::Hash64>, Option<crate::palw_tokenizer_rendering_v1::PalwTokenizerRenderingRuleV1>, Option<u64>, Option<u64>, Option<crate::palw_class_semantics_v1::PalwPrefixStateRelationV1>);
impl NativeWireScanV1 for crate::palw_class_semantics_v1::PalwPrefixStateRelationV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_conformance_evidence_v1::CheckOutcomeV1; String, crate::palw_conformance_evidence_v1::ResultV1, crate::palw_conformance_evidence_v1::ResultV1, crate::palw_conformance_evidence_v1::ResultV1, Option<String>);
wire_struct!(crate::palw_conformance_evidence_v1::ConformanceEvidencePostV1; misaka_palw_challenge::conformance::BeaconConformanceEvidenceV1, crate::palw_conformance_evidence_v1::ConformanceScopeV1, Vec<crate::palw_conformance_evidence_v1::CheckOutcomeV1>);
wire_struct!(crate::palw_conformance_evidence_v1::ConformanceScopeV1; u16, String, u32, u32, u32, u32, u32, u32, bool, bool);
impl NativeWireScanV1 for crate::palw_conformance_evidence_v1::ResultV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<String as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<[u8; 32] as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<[u8; 32] as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<Vec<u32> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for crate::palw_constraint_stream_carriage_v1::PalwAuxiliaryPublicCourtSeedV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<bool as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<crate::palw_step_leg::PalwStepOpeningV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(
                    <crate::palw_class_semantics_v1::PalwPrefixStateRelationV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?,
                )?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(
                    <crate::palw_class_semantics_v1::PalwPrefixStateRelationV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?,
                )?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<crate::palw_state_v2::PalwBondKeyV2 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<crate::palw_state_v2::PalwBondKeyV2 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamNativeSeedV1; u16, crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, u64, crate::Hash64, u32, Option<Box<crate::palw_freeprompt_v3::PalwFreePromptJobV3>>, crate::palw_constraint_stream_carriage_v1::PalwNativeSeedFamilyV1, Vec<u8>);
wire_struct!(crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamSeedV1; u16, crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, u64, crate::palw_step_leg::PalwStepBindingV2, Option<Box<crate::palw_freeprompt_v3::PalwFreePromptJobV3>>, crate::palw_step_refute::PalwTiledDecodePinV1, Option<u32>, u32, crate::palw_step_leg::PalwStepOpeningV1, Option<crate::palw_constraint_stream_carriage_v1::PalwAuxiliaryPublicCourtSeedV1>, Vec<u8>, Option<crate::palw_constraint_stream_carriage_v1::PalwRetainedCourtProfileV1>);
impl NativeWireScanV1 for crate::palw_constraint_stream_carriage_v1::PalwNativeSeedFamilyV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::palw_step_refute::PalwTiledDecodePinV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<Option<u32> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<crate::palw_step_leg::PalwStepOpeningV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<bool as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<crate::palw_step_leg::PalwStepOpeningV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_constraint_stream_carriage_v1::PalwRetainedCourtProfileV1; crate::Hash64, u64);
wire_struct!(crate::palw_constraint_stream_court_v1::PalwConstraintStreamCourtV1; crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, u64, crate::Hash64, crate::Hash64, crate::Hash64, crate::Hash64, u32, crate::palw_decode_constraint_v1::PalwDecodeConstraintV1, Vec<u32>, u32, Option<u32>, Option<u32>, bool, crate::palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1, Option<crate::palw_eog_stream_v1::PalwEogStreamV1>, Option<u32>, Vec<u32>, crate::palw_decode_constraint_v1::PalwConstraintStateV1, Option<crate::palw_rendering_stream_v1::PalwRenderingStreamV1>, bool, Option<crate::palw_freeprompt_v3::PalwFpStopReasonV3>, Option<u32>);
impl NativeWireScanV1 for crate::palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            6 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_constraint_stream_owned_v1::PalwOwnedConstraintStreamV1; crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, u64, crate::palw_rendering_resources_v1::PalwRenderingClaimTermsV1, u64, u64, u64, u64, u64, crate::palw_constraint_stream_court_v1::PalwConstraintStreamCourtV1);
wire_struct!(crate::palw_da_rcore_v1::PalwDaClaimV1; u8, u8, u16, BTreeMap<crate::palw_state_v2::PalwBondKeyV2, u8>, Option<u64>, Option<u64>, BTreeSet<crate::palw_da_rcore_v1::PalwDaUnitV1>, bool, Vec<(crate::palw_state_v2::PalwBondKeyV2, u128)>);
wire_struct!(crate::palw_da_rcore_v1::PalwDaSessionV1; u64, u64, bool, u128, Vec<crate::palw_da_rcore_v1::PalwDaUnitV1>, crate::palw_da_rcore_v1::PalwDaStageV1);
impl NativeWireScanV1 for crate::palw_da_rcore_v1::PalwDaStageV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for crate::palw_da_rcore_v1::PalwDaUnitV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::palw_held_da_v1::PalwHeldMissingV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            6 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            7 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            8 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum
                    .merge(<crate::palw_legacy_held_da_v2::PalwLegacyHeldUnitV2 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for crate::palw_decode_constraint_v1::PalwConstraintActionV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_decode_constraint_v1::PalwConstraintEdgeV1; u8, u8, crate::palw_decode_constraint_v1::PalwConstraintActionV1);
wire_struct!(crate::palw_decode_constraint_v1::PalwConstraintFrameV1; u16, Vec<crate::palw_decode_constraint_v1::PalwConstraintNodeV1>);
wire_struct!(crate::palw_decode_constraint_v1::PalwConstraintNodeV1; bool, Vec<crate::palw_decode_constraint_v1::PalwConstraintEdgeV1>);
wire_struct!(crate::palw_decode_constraint_v1::PalwConstraintStackEntryV1; u16, u16, u16);
wire_struct!(crate::palw_decode_constraint_v1::PalwConstraintStateV1; u16, u16, u16, Vec<crate::palw_decode_constraint_v1::PalwConstraintStackEntryV1>);
wire_struct!(crate::palw_decode_constraint_v1::PalwDecodeConstraintV1; u16, crate::Hash64, u16, Vec<crate::palw_decode_constraint_v1::PalwConstraintFrameV1>);
wire_struct!(crate::palw_decode_pipeline_v4::DecodeConfigV4; u32, u16, i32, i32, Vec<(u32, i32)>, Vec<Vec<u32>>);
wire_struct!(crate::palw_derived_v1::PalwDerivedKeyV1; crate::Hash64, crate::Hash64);
wire_struct!(crate::palw_derived_v1::PalwDerivedRowV1; crate::Hash64, crate::Hash64, u16, crate::Hash64, crate::Hash64, u64, u64);
wire_struct!(crate::palw_e2e_adjudicability::PalwE2eCoveringV1; bool, bool, bool, bool, bool, bool, u32, bool, std::collections::BTreeSet<crate::Hash64>);
wire_struct!(crate::palw_e2e_adjudicability::PalwE2eFamilyV1; crate::Hash64, crate::Hash64, BTreeSet<crate::Hash64>, crate::palw_e2e_adjudicability::PalwE2eCoveringV1);
wire_struct!(crate::palw_econ_contracts_v1::PalwHorizonExtensionRowV1; misaka_palw_kernel::econ_contracts_v1::HorizonExtensionTermsV1, Vec<[u8;64]>, u64, Option<u64>, crate::Hash64);
wire_struct!(crate::palw_econ_contracts_v1::PalwMinimumPriceRowV1; misaka_palw_kernel::econ_contracts_v1::MinimumPriceContractTermsV1, Vec<[u8;64]>, u64, Option<u64>, crate::Hash64);
wire_struct!(crate::palw_econ_contracts_v1::PalwModelVerificationContextV1; [u8;64], u64, [u8;64], misaka_palw_challenge::beacon::BeaconContextV1, u64, Option<u64>, crate::Hash64);
wire_struct!(crate::palw_econ_contracts_v1::PalwModelVerificationDecidedV1; [u8;64], u64, crate::palw_econ_contracts_v1::PalwModelVerificationOutcomeV1, u64);
wire_struct!(crate::palw_econ_contracts_v1::PalwModelVerificationDeferredV1; [u8;64], u64, [u8;64], crate::palw_state_v2::PalwBondKeyV2, crate::Hash64, u64, u64);
impl NativeWireScanV1 for crate::palw_econ_contracts_v1::PalwModelVerificationOutcomeV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_econ_contracts_v1::PalwModelVerificationTallyV1; [u8;64], u64, [u8;64], [u8;64], Option<[u8;64]>, Vec<([u8;64], misaka_palw_kernel::econ_contracts_v1::ModelVerificationVerdictV1)>, u64, u64, crate::Hash64);
wire_struct!(crate::palw_economic_payout_v1::PalwClaimEconomicsV1; u128, u128, u16, u128, u128, u64, u16);
impl NativeWireScanV1 for crate::palw_economic_safety_v1::PalwLicenceDoorTagV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_eog_stream_v1::PalwEogStreamV1; crate::Hash64, u32, u32, u32, Option<u32>, Option<u32>, Vec<u32>, Vec<bool>, crate::Hash64, crate::palw_rendering_stream_v1::LeafHashContinuationV1);
wire_struct!(crate::palw_exec_v2::PalwWorkRangeV1; u64, u64);
wire_struct!(crate::palw_execution_lane_v1::PalwExecBondV1; crate::palw_state_v2::PalwBondKeyV2, crate::Hash64, u64);
wire_struct!(crate::palw_execution_lane_v1::PalwExecDomainV1; crate::Hash64, u64, u16, u8, Vec<crate::palw_execution_lane_v1::PalwExecBondV1>);
wire_struct!(crate::palw_execution_lane_v1::PalwExecFinalV1; crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, crate::Hash64, crate::Hash64, crate::Hash64, u64, u64);
wire_struct!(crate::palw_execution_lane_v1::PalwExecScheduleV1; u64, crate::Hash64, Vec<crate::palw_execution_lane_v1::PalwExecDomainV1>, Vec<crate::palw_execution_lane_v1::PalwExecFinalV1>, Vec<crate::palw_execution_quanta_v1::PalwExecQuantumV1>);
wire_struct!(crate::palw_execution_lane_v1::PalwExecSeedAnchorV1; u64, crate::Hash64, crate::Hash64);
wire_struct!(crate::palw_execution_lane_v1::PalwExecSnapshotV1; u64, Vec<crate::palw_execution_lane_v1::PalwExecDomainV1>, Vec<crate::palw_execution_lane_v1::PalwExecFinalV1>);
wire_struct!(crate::palw_execution_quanta_v1::PalwExecQuantumV1; crate::Hash64, crate::Hash64, u32, crate::palw_state_v2::PalwBondKeyV2, crate::Hash64, crate::Hash64, u64);
impl NativeWireScanV1 for crate::palw_flat_constraint_stream_v1::PalwFlatConstraintCursorV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_flat_constraint_stream_v1::PalwFlatConstraintSourceSessionV1; crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, u64, crate::Hash64, crate::Hash64, u64, u64, u64, crate::palw_state_v2::PalwBondKeyV2, BTreeSet<crate::palw_state_v2::PalwBondKeyV2>, crate::palw_flat_constraint_stream_v1::PalwFlatConstraintStreamV1, BTreeMap<crate::palw_state_v2::PalwBondKeyV2, Option<crate::palw_flat_constraint_stream_v1::PalwFlatConstraintStreamV1>>, bool, Option<(u32, Option<u32>)>, Option<crate::palw_state_v2::PalwBondKeyV2>, Option<crate::palw_state_v2::PalwBondKeyV2>);
wire_struct!(crate::palw_flat_constraint_stream_v1::PalwFlatConstraintStreamV1; crate::Hash64, crate::Hash64, crate::Hash64, crate::Hash64, u32, u32, u32, u32, Option<u32>, Option<u32>, Option<i32>, Option<i32>, Vec<u32>, Option<Vec<u32>>, crate::palw_flat_constraint_stream_v1::PalwFlatConstraintCursorV1, crate::palw_rendering_stream_v1::LeafHashContinuationV1, u64, u64, bool);
wire_struct!(crate::palw_flat_execution_capacity_v22::NativeCarriageGlobalV22; u16, [u8;64], u32, u64, u64);
wire_struct!(crate::palw_flat_execution_capacity_v22::NativeCarriageLeaseV22; u16, [u8;64], [u8;64], [u8;64], u64, u64, bool);
wire_struct!(crate::palw_flat_execution_capacity_v22::NativeCarriagePlanV22; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], crate::palw_native_carriage_v22::NativeCarrierEnvelopeV22, crate::palw_native_carriage_v22::NativeCarrierEnvelopeV22, u64, u32, u32, u32, u64, u64, u64, u64, bool);
wire_struct!(crate::palw_flat_execution_capacity_v22::NativeExecutionConsentV23; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64]);
wire_struct!(crate::palw_flat_execution_capacity_v22::NativePhysicalSlotsV22; u16, [u8;64], [u8;64], [[u8;64]; 4], [u64; 4], [u64; 11], [[u8;64]; 11], [u8; 11]);
wire_struct!(crate::palw_flat_execution_pursuit_v19::NativePublicPursuitV19; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u32, u64, u32, u32, u32, u32, u32, u64, u64, u64, u64, u64, bool);
wire_struct!(crate::palw_flat_execution_v11::NativeExecutionClassJournalV11; misaka_palw_kernel::flat_execution11::ExecutionClassRecordV11, [u8;64], Option<u32>, crate::palw_state_v2::PalwBondKeyV2, u64, u32);
wire_struct!(crate::palw_flat_execution_v11::NativeExecutionClassRowV11; crate::palw_flat_execution_v11::NativeExecutionClassJournalV11);
wire_struct!(crate::palw_flat_execution_v11::NativeExecutionConsentV13; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], crate::palw_state_v2::PalwBondKeyV2, [u8;64], crate::palw_state_v2::PalwBondKeyV2, [u8;64], [u8;64], u64, u64, u64, u64);
wire_struct!(crate::palw_flat_execution_v11::NativeExecutionConsentV18; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64]);
wire_struct!(crate::palw_flat_execution_v11::NativeExecutionCreditV11; [u8;64], [u8;64], [u8;64], [u8;64], u64, u64, u64, Option<[u8;64]>, u64, u64, u64);
wire_struct!(crate::palw_flat_execution_v11::NativeExecutionDemandCountersV13; u32, u32, u32);
wire_struct!(crate::palw_flat_execution_v11::NativeExecutionDemandV11; [u8;64], u8, [u8;64], crate::palw_flat_model_vector_da_v9::FlatVectorDemandV9);
wire_struct!(crate::palw_flat_execution_v11::NativeExecutionDiscoveryV13; [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u64, u64, u64, u64, [u8;64], [u8;64], Option<[u8;64]>);
wire_struct!(crate::palw_flat_execution_v11::NativeExecutionFeePlanV19; u16, [u8;64], [u8;64], [u8;64], u64, u64, u32, u32, u64, u64);
wire_struct!(crate::palw_flat_execution_v11::NativeExecutionFeesV19; u16, [u8;64], [u8;64], [u8;64], crate::palw_flat_execution_v11::NativeExecutionFeePlanV19, u64);
wire_struct!(crate::palw_flat_execution_v11::NativeExecutionJobHeaderV11; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], crate::palw_state_v2::PalwBondKeyV2, [u8;64], crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, [u8;64], crate::Hash64, u32, u32, u64, u64, misaka_palw_kernel::flat_execution11::ExecutionTerminalV11, u64);
wire_struct!(crate::palw_flat_execution_v11::NativeExecutionJobRowV11; crate::palw_flat_execution_v11::NativeExecutionJobHeaderV11, Vec<u8>, misaka_palw_kernel::flat_quota11::QuotaPlanRecordV11, Vec<u32>);
wire_struct!(crate::palw_flat_execution_v11::NativeExecutionPoliciesV11; misaka_palw_kernel::flat_execution11::ExecutionPolicyV11, misaka_palw_kernel::flat_quota11::QuotaPolicyV11, misaka_palw_kernel::flat_quota11::discovery13::DiscoveryPolicyV13);
wire_struct!(crate::palw_flat_execution_v11::NativeExecutionPreparationV11; misaka_palw_kernel::flat_execution11::NativeExecutionPreparationRecordV11, misaka_palw_kernel::flat_quota11::QuotaPlanRecordV11, crate::palw_state_v2::PalwBondKeyV2, u64);
wire_struct!(crate::palw_flat_execution_v11::NativeExecutionProofGroupV11; [u8;64], [u8;64], crate::palw_state_v2::PalwBondKeyV2, u8, u64, u64, std::collections::BTreeMap<u8, Vec<u8>>);
wire_struct!(crate::palw_flat_execution_v11::NativeExecutionPursuitV11; u32, u32, Vec<[u8;64]>, Vec<[u8;64]>, u64, u64);
wire_struct!(crate::palw_flat_execution_v11::NativeExecutionSealV11; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], crate::palw_state_v2::PalwBondKeyV2, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u64, u64, u64, u64, u64, [u8;64], [u8;64], bool);
wire_struct!(crate::palw_flat_execution_v11::NativeRequesterNonceRowV11; [u8;64], u64);
wire_struct!(crate::palw_flat_execution_v11::NativeSourceDemandV16; crate::palw_flat_execution_v11::NativeSourceRequestV16, crate::palw_state_v2::PalwBondKeyV2, crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, crate::Hash64, [u8;64], u8, u8, [u64; 4], u64, u64, u64, u64, u8, bool);
wire_struct!(crate::palw_flat_execution_v11::NativeSourceHeaderFaultV16; u16, u32, misaka_palw_kernel::flat_quota11::source16::opening::HeaderV16);
wire_struct!(crate::palw_flat_execution_v11::NativeSourceRequestV16; [u8;64], u8, [u8;64], u8, u16, u32, u8, u8, u8, u64);
wire_struct!(crate::palw_flat_execution_v11::NativeVerifiedHeaderFactV16; u16, [u8;64], [u8;64], [u8;64], [u8;64], u64, [u8;64], crate::palw_flat_execution_v11::NativeSourceHeaderFaultV16);
wire_struct!(crate::palw_flat_execution_v11::NativeVerifiedSourceFactHeaderV11; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u64, [u8;64], [u8;64], u32);
wire_struct!(crate::palw_flat_execution_v11::NativeVerifiedSourceFactV11; crate::palw_flat_execution_v11::NativeVerifiedSourceFactHeaderV11, Vec<u8>);
wire_struct!(crate::palw_flat_model_seal_v6::FlatNativeSealRowV6; misaka_palw_kernel::flat_seal6::FlatSealRowV6, crate::palw_state_v2::PalwBondKeyV2, u64);
wire_struct!(crate::palw_flat_model_vector_da_v3::FlatVectorDemandV3; crate::Hash64, crate::palw_flat_model_vector_da_v3::FlatVectorUnitV3, crate::palw_state_v2::PalwBondKeyV2, crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, crate::Hash64, u8, u64, u64, u64, u64, u64, u64, u32, [u8;64], u8, u8, [u64; 4], bool);
wire_struct!(crate::palw_flat_model_vector_da_v3::FlatVectorServedHeaderV3; u64, [u8;64]);
wire_struct!(crate::palw_flat_model_vector_da_v3::FlatVectorServedRowV3; crate::palw_flat_model_vector_da_v3::FlatVectorServedHeaderV3, Vec<u8>);
wire_struct!(crate::palw_flat_model_vector_da_v3::FlatVectorUnitV3; u32, u16, u16, Option<u64>);
wire_struct!(crate::palw_flat_model_vector_da_v7::FlatVectorDemandV7; crate::Hash64, crate::palw_flat_model_vector_da_v7::FlatVectorUnitV7, crate::palw_state_v2::PalwBondKeyV2, crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, crate::Hash64, u8, u64, u64, u64, u64, u64, u64, u32, [u8;64], u8, u8, [u64; 4], bool);
wire_struct!(crate::palw_flat_model_vector_da_v7::FlatVectorServedHeaderV7; u64, [u8;64]);
wire_struct!(crate::palw_flat_model_vector_da_v7::FlatVectorServedRowV7; crate::palw_flat_model_vector_da_v7::FlatVectorServedHeaderV7, Vec<u8>);
impl NativeWireScanV1 for crate::palw_flat_model_vector_da_v7::FlatVectorUnitKindV7 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_flat_model_vector_da_v7::FlatVectorUnitV7; u32, u16, u16, crate::palw_flat_model_vector_da_v7::FlatVectorUnitKindV7);
wire_struct!(crate::palw_flat_model_vector_da_v9::FlatVectorDemandV9; crate::Hash64, crate::palw_flat_model_vector_da_v9::FlatVectorUnitV9, crate::palw_state_v2::PalwBondKeyV2, crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, crate::Hash64, u8, u64, u64, u64, u64, u64, u64, u32, [u8;64], [u8;64], u8, u8, [u64; 4], bool);
wire_struct!(crate::palw_flat_model_vector_da_v9::FlatVectorServedHeaderV9; u64, [u8;64]);
wire_struct!(crate::palw_flat_model_vector_da_v9::FlatVectorServedRowV9; crate::palw_flat_model_vector_da_v9::FlatVectorServedHeaderV9, Vec<u8>);
impl NativeWireScanV1 for crate::palw_flat_model_vector_da_v9::FlatVectorUnitKindV9 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_flat_model_vector_da_v9::FlatVectorUnitV9; u32, u16, u16, crate::palw_flat_model_vector_da_v9::FlatVectorUnitKindV9);
wire_struct!(crate::palw_flat_model_vector_v3::FlatModelVectorHeaderV3; [u8;64], crate::Hash64, crate::Hash64, [u8;64], [u8;64], u32, u32, u32, u64, u64, u64, crate::palw_state_v2::PalwBondKeyV2, u64, u64, u64, bool);
wire_struct!(crate::palw_flat_model_vector_v3::FlatModelVectorRowV3; crate::palw_flat_model_vector_v3::FlatModelVectorHeaderV3, Vec<u8>);
wire_struct!(crate::palw_flat_model_vector_v7::FlatNativeSealRowV7; misaka_palw_kernel::flat_seal7::FlatSealRowV7, crate::palw_state_v2::PalwBondKeyV2, u64);
wire_struct!(crate::palw_flat_model_vector_v9::FlatNativeSealRowV9; misaka_palw_kernel::flat_seal9::FlatSealRowV9, crate::palw_state_v2::PalwBondKeyV2, u64);
wire_struct!(crate::palw_flat_stage_v14::NativeStageAcceptanceV14; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], crate::palw_state_v2::PalwBondKeyV2, crate::palw_state_v2::PalwBondKeyV2, [u8;64], [u8;64], [u8;64], u64, u64, u64);
wire_struct!(crate::palw_flat_stage_v14::NativeStageAcceptanceV18; u16, [u8;64], crate::palw_flat_stage_v14::NativeStageAcceptanceV14, [u8;64], [u8;64], [u8;64], [u8;64]);
wire_struct!(crate::palw_flat_stage_v14::NativeStageClassHeaderV14; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u32, crate::palw_state_v2::PalwBondKeyV2, u64);
wire_struct!(crate::palw_flat_stage_v14::NativeStageClassRowV14; crate::palw_flat_stage_v14::NativeStageClassHeaderV14, Vec<u8>);
wire_struct!(crate::palw_flat_stage_v14::NativeStageCollectedV14; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u64);
wire_struct!(crate::palw_flat_stage_v14::NativeStageFeePlanV20; u16, [u8;64], [u8;64], [u8;64], [u8;64], u64, u64, u32, u32, u64, u64);
wire_struct!(crate::palw_flat_stage_v14::NativeStageFeesV20; u16, [u8;64], [u8;64], [u8;64], crate::palw_flat_stage_v14::NativeStageFeePlanV20, u64);
wire_struct!(crate::palw_flat_stage_v14::NativeStageJobHeaderV14; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], misaka_palw_kernel::flat_quota11::QuotaPlanRecordV11, [u8;64], [u8;64], u32, crate::palw_state_v2::PalwBondKeyV2, crate::palw_state_v2::PalwBondKeyV2, [u8;64], [u8;64], crate::Hash64, crate::Hash64, u32, u64, u16, u64, u64, misaka_palw_kernel::flat_quota11::stage14::StageTerminalV14);
wire_struct!(crate::palw_flat_stage_v14::NativeStageJobRowV14; crate::palw_flat_stage_v14::NativeStageJobHeaderV14, Vec<u8>, Vec<Vec<u8>>, Vec<u8>);
wire_struct!(crate::palw_flat_stage_v14::NativeStageNegativeSourceFactV14; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u64, crate::palw_state_v2::PalwBondKeyV2);
wire_struct!(crate::palw_flat_stage_v14::NativeStagePoliciesV14; u16, crate::palw_flat_execution_v11::NativeExecutionPoliciesV11, u64, u64, u64, u8);
wire_struct!(crate::palw_flat_stage_v14::NativeStagePrincipalRefundV14; u16, [u8;64], [u8;64], [u8;64], [u8;64], crate::palw_state_v2::PalwBondKeyV2, crate::Hash64, crate::Hash64, u64, u64);
wire_struct!(crate::palw_flat_stage_v14::NativeStagePursuitV14; u16, [u8;64], [u8;64], [u8;64], [u8;64], u32, u32, u32, [[u8;64]; 4], [[u8;64]; 4], [[u8;64]; 32], [[u8;64]; 32], u32, u64);
wire_struct!(crate::palw_flat_stage_v14::NativeStageSealV14; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], crate::palw_state_v2::PalwBondKeyV2, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u64, u64, u64, u64, [u8;64], u64, [u8;64], bool);
wire_struct!(crate::palw_flat_stage_v14::NativeStageSourceHeaderV14; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u32, u32, u32, u32, crate::palw_state_v2::PalwBondKeyV2, u64, u64, u64, u64, bool);
wire_struct!(crate::palw_flat_stage_v14::NativeStageSourceRowV14; crate::palw_flat_stage_v14::NativeStageSourceHeaderV14, Vec<u8>);
wire_struct!(crate::palw_flat_stage_v28::NativeStageConsentV28; u16, [u8;64], crate::palw_flat_stage_v14::NativeStageAcceptanceV14, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], crate::Hash64);
wire_struct!(crate::palw_flat_stage_v28::NativeStageMaximumV28; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], crate::palw_flat_stage_v28::NativeStageTemporalV28, crate::palw_native_final_v27::NativeFinalClassBasisV27, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u64);
wire_struct!(crate::palw_flat_stage_v28::NativeStagePendingV28; u16, [u8;64], [u8;64], crate::palw_flat_stage_v28::NativeStageRequestV28, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], crate::palw_state_v2::PalwBondKeyV2, [u8;64], crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, [u8;64], [u8;64], [u8;64], u64, u64, u64, u64, u8, bool);
wire_struct!(crate::palw_flat_stage_v28::NativeStagePoliciesV28; u16, [u8;64], crate::palw_flat_execution_v11::NativeExecutionPoliciesV11, [u8;64], [u8;64], [u8;64], u16, u64, u8, u32);
wire_struct!(crate::palw_flat_stage_v28::NativeStageTemporalV28; u16, [u8;64], [u8;64], [u8;64], u32, u32, u64, u64, u64, u64, u64, u64, u64, u64, [u8;64], [u8;64], [u8;64]);
wire_struct!(crate::palw_float_decode_v1::PalwFloatRowFactsV1; u32, u32, i32, BTreeMap<u32, i32>);
wire_struct!(crate::palw_float_decode_v1::PalwFloatRowStreamV1; crate::Hash64, u32, u32, u32, crate::palw_rendering_stream_v1::LeafHashContinuationV1, Vec<u32>, BTreeMap<u32, i32>, Option<(i32, u32)>);
wire_struct!(crate::palw_fp_constraint_job_v1::PalwFpConstraintTailV1; Vec<u8>, crate::Hash64);
wire_struct!(crate::palw_fp_job_v5::PalwFpV5TailV1; Vec<crate::palw_gen_class_v1::PalwGenImageInputRefV1>, Option<crate::palw_gen_class_v1::PalwGenSourceRefV1>);
impl NativeWireScanV1 for crate::palw_freeprompt_v3::PalwFpJobTailV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum =
                    sum.merge(<Box<crate::palw_gen_claim_v1::PalwGenJobTailV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::palw_fp_job_v5::PalwFpV5TailV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum =
                    sum.merge(<Box<crate::palw_improve_eval_v1::PalwEvalJobV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::palw_freeprompt_v3::PalwFpPrefixStateV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(
                    <crate::palw_fp_constraint_job_v1::PalwFpConstraintTailV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?,
                )?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_freeprompt_v3::PalwFpPrefixStateV1; crate::Hash64, u32, crate::Hash64);
impl NativeWireScanV1 for crate::palw_freeprompt_v3::PalwFpStopReasonV3 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_freeprompt_v3::PalwFreePromptJobV3; u16, crate::Hash64, crate::Hash64, crate::tx::TransactionOutpoint, Vec<u8>, crate::Hash64, crate::Hash64, u64, [u8; 32], crate::Hash64, crate::Hash64, u32, u32, u32, u8, u8, [u8; 32], u32, Option<crate::palw_decode_pipeline_v4::DecodeConfigV4>, Option<crate::palw_freeprompt_v3::PalwFpJobTailV1>);
wire_struct!(crate::palw_gen_claim_v1::PalwGenJobTailV1; [u8; 32], crate::palw_gen_job_v1::PalwGenBodyV1);
wire_struct!(crate::palw_gen_class_v1::PalwGenClassRecordV1; u16, crate::Hash64, crate::Hash64, u8, crate::Hash64, crate::Hash64, crate::Hash64, Vec<crate::palw_gen_class_v1::PalwGenImageOfferV1>, Option<u32>, u64, Vec<(u8, u8, u16)>, std::sync::Arc<crate::palw_gen_class_v1::PalwGenClassV1>);
wire_struct!(crate::palw_gen_class_v1::PalwGenClassV1; u16, u8, Vec<u8>, Vec<Vec<u8>>, Vec<crate::palw_tir_class_v1::PalwTirLayoutV1>, misaka_palw_gen::output::OutputSpecV1, crate::palw_gen_class_v1::PalwGenOffersV1, crate::Hash64);
wire_struct!(crate::palw_gen_class_v1::PalwGenEmbeddingOffersV1; u8, Vec<u32>);
wire_struct!(crate::palw_gen_class_v1::PalwGenGuidanceOfferV1; u8, u16, u16);
wire_struct!(crate::palw_gen_class_v1::PalwGenImageInputRefV1; crate::Hash64, u32, u32);
wire_struct!(crate::palw_gen_class_v1::PalwGenImageOfferV1; u32, u32, u32, u32);
wire_struct!(crate::palw_gen_class_v1::PalwGenImageOffersV1; crate::Hash64, Option<crate::palw_gen_class_v1::PalwGenGuidanceOfferV1>, Option<u8>);
wire_struct!(crate::palw_gen_class_v1::PalwGenOffersV1; Vec<u32>, Vec<crate::palw_gen_class_v1::PalwGenScalarOfferV1>, u32, u32, Vec<crate::palw_gen_class_v1::PalwGenImageOfferV1>, u32, u32, Vec<u32>, crate::palw_gen_class_v1::PalwGenProfileOffersV1);
impl NativeWireScanV1 for crate::palw_gen_class_v1::PalwGenProfileOffersV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::palw_gen_class_v1::PalwGenImageOffersV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum =
                    sum.merge(<crate::palw_gen_class_v1::PalwGenEmbeddingOffersV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::palw_task_heads_v1::PalwGenHeadOffersV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_gen_class_v1::PalwGenScalarOfferV1; i64, i64);
wire_struct!(crate::palw_gen_class_v1::PalwGenSourceRefV1; crate::Hash64, u32);
impl NativeWireScanV1 for crate::palw_gen_job_v1::PalwGenBodyV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::palw_gen_job_v1::PalwGenImageBodyV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::palw_gen_job_v1::PalwGenEmbeddingBodyV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::palw_task_heads_v1::PalwGenHeadBodyV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_gen_job_v1::PalwGenEmbeddingBodyV1; crate::palw_gen_job_v1::PalwGenEmbeddingInputV1, u8, u32, u8);
impl NativeWireScanV1 for crate::palw_gen_job_v1::PalwGenEmbeddingInputV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum =
                    sum.merge(<crate::palw_gen_class_v1::PalwGenImageInputRefV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_gen_job_v1::PalwGenImageBodyV1; crate::Hash64, u32, crate::Hash64, u32, u16, u16, crate::Hash64, u16, u16, u16, u8);
impl NativeWireScanV1 for crate::palw_held_da_v1::PalwHeldDisclosureV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum =
                    sum.merge(<crate::palw_prompt_ids_v1::PalwPromptIdsOpeningV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::palw_attn_court_v1::PalwAttnCheckpointAnchorV1 as NativeWireScanV1>::scan_native_wire_v1(
                    reader,
                )?)?;
                sum =
                    sum.merge(<crate::palw_attn_court_v1::PalwAttnChunkOpeningV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::palw_step_leg::PalwStepRangeOpeningV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum
                    .merge(<Box<crate::palw_shard_court_v1::PalwLeafEvidenceV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for crate::palw_held_da_v1::PalwHeldMissingV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_history_custody_v1::PalwHistoryCustodyContractV1; crate::palw_history_custody_v1::PalwHistoryCustodyPledgeV1, crate::palw_state_v2::PalwBondKeyV2, u64, u64, crate::palw_prefix_custody_v1::PalwPrefixCustodyContractV1);
wire_struct!(crate::palw_history_custody_v1::PalwHistoryCustodyPledgeV1; u16, crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, crate::Hash64, crate::Hash64, crate::Hash64, crate::Hash64, u64, u64, u64, u64, Vec<u8>);
wire_struct!(crate::palw_history_custody_v1::PalwHistoryRecoveryBufferV1; u32, Vec<u8>);
wire_struct!(crate::palw_history_custody_v1::PalwHistoryRecoveryDefaultV1; crate::palw_prefix_history_v1::PalwPrefixHistoryRequestV1, u64, u64, u64, BTreeSet<crate::palw_state_v2::PalwBondKeyV2>);
wire_struct!(crate::palw_history_custody_v1::PalwHistoryRecoveryDemandV1; crate::palw_prefix_history_v1::PalwPrefixHistoryRequestV1, u64, u64, BTreeMap<crate::palw_state_v2::PalwBondKeyV2, crate::palw_history_custody_v1::PalwHistoryRecoveryBufferV1>);
wire_struct!(crate::palw_history_custody_v1::PalwHistoryRecoveryV1; crate::palw_prefix_history_v1::PalwPrefixHistoryRequestV1, u64, crate::Hash64, u64, u64, u64, u64, u64, BTreeSet<crate::palw_state_v2::PalwBondKeyV2>, Option<crate::palw_history_custody_v1::PalwHistoryRecoveryDemandV1>, Option<crate::palw_history_custody_v1::PalwHistoryRecoveryDefaultV1>);
impl NativeWireScanV1 for crate::palw_improve_artifact_v1::PalwTirArtifactRefV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_improve_composite_v1::PalwTirCompositeRefV1; crate::Hash64, crate::Hash64, crate::Hash64, u32);
wire_struct!(crate::palw_improve_eval_v1::PalwEvalClaimRefV1; crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, u64, crate::Hash64, Option<crate::Hash64>, Option<u64>, Option<i64>);
wire_struct!(crate::palw_improve_eval_v1::PalwEvalJobStateV1; crate::palw_improve_eval_v1::PalwEvalJobV1, Option<crate::palw_improve_eval_v1::PalwEvalClaimRefV1>);
wire_struct!(crate::palw_improve_eval_v1::PalwEvalJobV1; crate::Hash64, u64, u32, crate::palw_improve_state_v1::PalwEvalSubjectV1, crate::palw_improve_state_v1::PalwScoringKindV1, u8, crate::palw_improve_eval_v1::PalwEvalModeV1);
impl NativeWireScanV1 for crate::palw_improve_eval_v1::PalwEvalModeV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<Vec<u32> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for crate::palw_improve_material_v1::PalwCaseReferenceV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for crate::palw_improve_material_v1::PalwCaseSourceV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_improve_material_v1::PalwDataUseOptInRecordV1; crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, crate::Hash64, bool, crate::Hash64, u32, u64, u64);
wire_struct!(crate::palw_improve_material_v1::PalwDatasetRecordV1; crate::palw_improve_material_v1::PalwDatasetV1, crate::palw_state_v2::PalwBondKeyV2, u64, u64, u64);
wire_struct!(crate::palw_improve_material_v1::PalwDatasetV1; crate::Hash64, crate::Hash64, crate::Hash64, u64, Vec<crate::Hash64>, u8, crate::Hash64);
wire_struct!(crate::palw_improve_material_v1::PalwHardCaseRecordV1; crate::palw_improve_material_v1::PalwHardCaseV1, crate::palw_state_v2::PalwBondKeyV2, u64, u64, bool, Option<Vec<u32>>);
wire_struct!(crate::palw_improve_material_v1::PalwHardCaseV1; crate::Hash64, crate::Hash64, u16, Vec<u32>, crate::palw_improve_material_v1::PalwCaseReferenceV1, crate::palw_improve_material_v1::PalwCaseSourceV1, Option<crate::Hash64>);
wire_struct!(crate::palw_improve_material_v1::PalwSetterSetCommitmentV1; crate::Hash64, u64, crate::Hash64, u32, crate::Hash64, crate::Hash64);
wire_struct!(crate::palw_improve_material_v1::PalwSetterSetRecordV1; crate::palw_improve_material_v1::PalwSetterSetCommitmentV1, crate::palw_state_v2::PalwBondKeyV2, u64, u64, Option<Vec<Vec<u32>>>, Option<Vec<Vec<u32>>>);
wire_struct!(crate::palw_improve_material_v1::PalwTeacherLicenceRecordV1; crate::palw_improve_material_v1::PalwTeacherLicenceV1, u64);
wire_struct!(crate::palw_improve_material_v1::PalwTeacherLicenceV1; crate::Hash64, Vec<u8>, crate::Hash64, Vec<u16>, u8, u64, u64);
wire_struct!(crate::palw_improve_material_v1::PalwTeachingArtifactCommitV1; crate::Hash64, crate::Hash64);
wire_struct!(crate::palw_improve_material_v1::PalwTeachingArtifactRecordV1; crate::palw_improve_material_v1::PalwTeachingArtifactCommitV1, crate::palw_state_v2::PalwBondKeyV2, u64, u64, u64, Option<crate::palw_improve_material_v1::PalwTeachingArtifactV1>, u64, u64);
wire_struct!(crate::palw_improve_material_v1::PalwTeachingArtifactV1; crate::Hash64, crate::palw_improve_state_v1::PalwTeachingArtifactKindV1, crate::Hash64, crate::palw_improve_state_v1::PalwTeacherClassV1, crate::Hash64, crate::Hash64, crate::Hash64, crate::Hash64, crate::palw_improve_state_v1::PalwVerificationTypeV1, Vec<u32>, crate::Hash64);
wire_struct!(crate::palw_improve_state_v1::PalwEpochCandidateV1; crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, crate::palw_improve_artifact_v1::PalwTirArtifactRefV1, crate::Hash64, Vec<(crate::Hash64, u16)>, u64, u64, u64, u64, u64, Option<crate::palw_improve_state_v1::PalwPromotionCountsV1>);
wire_struct!(crate::palw_improve_state_v1::PalwEpochEscrowV1; u64, u64, u64, u64);
impl NativeWireScanV1 for crate::palw_improve_state_v1::PalwEpochRetireV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for crate::palw_improve_state_v1::PalwEpochStateV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            6 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            7 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_improve_state_v1::PalwEpochTimesV1; u64, u64, u64, u64, u64, u64);
wire_struct!(crate::palw_improve_state_v1::PalwEpochWindowsV1; u64, u64, u64, u64, u64, u64, u64);
wire_struct!(crate::palw_improve_state_v1::PalwEvalItemV1; u32, crate::Hash64, crate::palw_improve_state_v1::PalwItemSourceV1, Option<crate::palw_state_v2::PalwBondKeyV2>, crate::Hash64, Option<crate::Hash64>, bool);
wire_struct!(crate::palw_improve_state_v1::PalwEvalResultV1; u32, crate::palw_improve_state_v1::PalwEvalSubjectV1, Vec<crate::palw_improve_state_v1::PalwEvalScoreV1>);
wire_struct!(crate::palw_improve_state_v1::PalwEvalScoreV1; crate::palw_improve_state_v1::PalwScoringKindV1, i64);
wire_struct!(crate::palw_improve_state_v1::PalwEvalSpecV1; Vec<crate::palw_improve_state_v1::PalwScoringStageV1>, crate::Hash64, u32, crate::Hash64, u32, Vec<crate::Hash64>, Option<crate::palw_improve_state_v1::PalwJudgeSpecV1>, Option<crate::palw_improve_state_v1::PalwJudgeSpecV1>, u16, u16, u32, u32, u16, u16, u16, u16, u32, Vec<u32>, u16, u64);
impl NativeWireScanV1 for crate::palw_improve_state_v1::PalwEvalSubjectV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for crate::palw_improve_state_v1::PalwHeadCauseV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_improve_state_v1::PalwImprovementEpochV1; crate::Hash64, u64, crate::palw_improve_state_v1::PalwEpochStateV1, crate::palw_improve_state_v1::PalwEpochTimesV1, crate::Hash64, Option<crate::Hash64>, crate::Hash64, Option<crate::Hash64>, u32, u32, u32, u32, Option<crate::Hash64>, u32, u32, Option<crate::palw_improve_state_v1::PalwPromotionCountsV1>, Option<crate::palw_improve_state_v1::PalwPromotionOutcomeV1>, crate::palw_improve_state_v1::PalwEpochEscrowV1, u32, Option<u64>, crate::palw_improve_state_v1::PalwEpochRetireV1);
wire_struct!(crate::palw_improve_state_v1::PalwImprovementFeesV1; u64, u64, u64, u64, u64, u64, u64, u64, u64);
impl NativeWireScanV1 for crate::palw_improve_state_v1::PalwImprovementLineStatusV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<Option<u64> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_improve_state_v1::PalwImprovementLineV1; crate::Hash64, crate::Hash64, crate::Hash64, u64, crate::palw_improve_state_v1::PalwImprovementLineStatusV1, u64, crate::Hash64, u32, u64, Option<u64>, u64, u64, Vec<(crate::palw_state_v2::PalwBondKeyV2, u64)>, Option<crate::palw_improve_state_v1::PalwLastPromotionV1>, Option<u64>, Option<crate::Hash64>);
wire_struct!(crate::palw_improve_state_v1::PalwImprovementPolicyRecordV1; crate::palw_improve_state_v1::PalwImprovementPolicyV1, Option<Box<crate::palw_improve_state_v1::PalwImprovementPolicyV1>>);
wire_struct!(crate::palw_improve_state_v1::PalwImprovementPolicyV1; u16, crate::palw_improve_state_v1::PalwUsageThresholdV1, crate::palw_improve_state_v1::PalwEpochWindowsV1, crate::palw_improve_state_v1::PalwEvalSpecV1, u8, crate::palw_improve_state_v1::PalwImprovementFeesV1, u16, u16, u16, u16, u16, u16, crate::palw_improve_state_v1::PalwProvenancePolicyV1, u32, u32, u32);
wire_struct!(crate::palw_improve_state_v1::PalwImprovementPoolV1; u64, u64, u64, u64, u128, u128, u128, u128, u128, u128, u128);
wire_struct!(crate::palw_improve_state_v1::PalwImprovementUsageV1; u128, u64);
impl NativeWireScanV1 for crate::palw_improve_state_v1::PalwItemSourceV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_improve_state_v1::PalwJudgeSpecV1; crate::Hash64, Vec<u32>, Vec<u32>, i32);
wire_struct!(crate::palw_improve_state_v1::PalwLastPromotionV1; u64, u64, u64);
wire_struct!(crate::palw_improve_state_v1::PalwLineageHeadEntryV1; u64, crate::Hash64, Option<crate::Hash64>, u64, crate::palw_improve_state_v1::PalwHeadCauseV1);
wire_struct!(crate::palw_improve_state_v1::PalwMaterialFrontierV1; u32, Vec<crate::Hash64>);
impl NativeWireScanV1 for crate::palw_improve_state_v1::PalwNoChangeReasonV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            6 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_improve_state_v1::PalwPairedCountsV1; u32, u32, u32);
impl NativeWireScanV1 for crate::palw_improve_state_v1::PalwPoolEntryV2 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<crate::palw_state_v2::PalwBondKeyV2 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<crate::palw_state_v2::PalwBondKeyV2 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_improve_state_v1::PalwPromotionCountsV1; crate::palw_improve_state_v1::PalwPairedCountsV1, crate::palw_improve_state_v1::PalwPairedCountsV1, crate::palw_improve_state_v1::PalwPairedCountsV1, crate::palw_improve_state_v1::PalwPairedCountsV1, crate::palw_improve_state_v1::PalwPairedCountsV1, bool);
impl NativeWireScanV1 for crate::palw_improve_state_v1::PalwPromotionOutcomeV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum =
                    sum.merge(<crate::palw_improve_state_v1::PalwNoChangeReasonV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_improve_state_v1::PalwProvenancePolicyV1; u8, Vec<crate::Hash64>, bool, crate::Hash64);
wire_struct!(crate::palw_improve_state_v1::PalwRewardGrantV1; crate::palw_state_v2::PalwBondKeyV2, crate::palw_improve_state_v1::PalwRewardStageV1, u64, crate::palw_improve_state_v1::PalwTrustLabelV1, u64, u64, u32, u64, bool);
impl NativeWireScanV1 for crate::palw_improve_state_v1::PalwRewardStageV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            6 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for crate::palw_improve_state_v1::PalwScoringKindV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for crate::palw_improve_state_v1::PalwScoringParamsV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<i32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<i32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<i32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<i32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<i32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<i32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_improve_state_v1::PalwScoringStageV1; crate::palw_improve_state_v1::PalwScoringKindV1, crate::palw_improve_state_v1::PalwScoringParamsV1);
impl NativeWireScanV1 for crate::palw_improve_state_v1::PalwTeacherClassV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            6 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for crate::palw_improve_state_v1::PalwTeachingArtifactKindV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            6 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for crate::palw_improve_state_v1::PalwTrustLabelV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for crate::palw_improve_state_v1::PalwUsageMeasureV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_improve_state_v1::PalwUsageThresholdV1; crate::palw_improve_state_v1::PalwUsageMeasureV1, u128);
impl NativeWireScanV1 for crate::palw_improve_state_v1::PalwVerificationTypeV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_issuance_slots_v1::PalwIssuanceBucketV1; u64, u64);
wire_struct!(crate::palw_json_key_set_v1::PalwJsonKeySetV1; crate::Hash64, u64);
wire_struct!(crate::palw_kernel_route_v1::KernelAssignmentV1; Vec<crate::palw_kernel_route_v1::KernelSeatV1>, [u8;64], [u8;64], [u8;64], u64, u8);
wire_struct!(crate::palw_kernel_route_v1::KernelSeatV1; crate::palw_state_v2::PalwBondKeyV2, [u8;64], misaka_palw_kernel::verify::ScopeV1);
wire_struct!(crate::palw_kernel_route_v1::PalwKernelChunkGroupV1; crate::palw_kernel_route_v1::PalwKernelChunkTargetV1, u8, u64, u64, u64, BTreeMap<u8, Vec<u8>>);
impl NativeWireScanV1 for crate::palw_kernel_route_v1::PalwKernelChunkTargetV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            6 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            7 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_kernel_route_v1::PalwKernelRouteHeaderV1; misaka_palw_kernel::ledger::LedgerPolicyV1, [u8;64], misaka_palw_kernel::rows::LedgerScalarsV1, Option<misaka_palw_kernel::opv::OpvPolicyV1>, Option<u64>);
wire_struct!(crate::palw_legacy_answer_chunks_v2::PalwLegacyAnswerDeclarationV2; u16, crate::Hash64, crate::palw_legacy_held_da_v2::PalwLegacyHeldUnitV2, crate::palw_state_v2::PalwBondKeyV2, u32, u64, Vec<crate::Hash64>, crate::Hash64, Vec<u8>);
wire_struct!(crate::palw_legacy_answer_chunks_v2::PalwPendingLegacyAnswerV2; crate::palw_legacy_answer_chunks_v2::PalwLegacyAnswerDeclarationV2, u64, BTreeMap<u8, Vec<u8>>);
impl NativeWireScanV1 for crate::palw_legacy_held_da_v2::PalwLegacyHeldUnitV2 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_legacy_public_filer_v1::PalwDisputeClaimV1; u64, u64, BTreeMap<crate::palw_state_v2::PalwBondKeyV2, crate::palw_legacy_public_filer_v1::PalwDisputeReservationRowV1>, BTreeSet<crate::palw_state_v2::PalwBondKeyV2>, Vec<(crate::palw_state_v2::PalwBondKeyV2, u128)>, BTreeMap<crate::palw_state_v2::PalwBondKeyV2, crate::palw_da_rcore_v1::PalwDaUnitV1>);
wire_struct!(crate::palw_legacy_public_filer_v1::PalwDisputeReservationRowV1; u64, u128, u8, u16);
wire_struct!(crate::palw_legs::PalwCheckpointProfileV1; u16, u32, crate::Hash64);
wire_struct!(crate::palw_material_capacity_v1::PalwHeldMaterialBundleV1; crate::Hash64, Box<crate::palw_step_leg::PalwStepBindingV2>, Vec<(crate::palw_held_da_v1::PalwHeldMissingV1, crate::palw_held_da_v1::PalwHeldDisclosureV1)>);
impl NativeWireScanV1 for crate::palw_material_capacity_v1::PalwMaterialCodecV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_material_capacity_v1::PalwMaterialManifestV1; crate::Hash64, Vec<crate::palw_material_capacity_v1::PalwMaterialScopeV1>);
wire_struct!(crate::palw_material_capacity_v1::PalwMaterialScopeV1; crate::Hash64, crate::Hash64, crate::palw_material_capacity_v1::PalwMaterialCodecV1, u64, u32, crate::Hash64);
wire_struct!(crate::palw_mesh_v1::PalwAuditAssignmentV1; crate::palw_state_v2::PalwBondKeyV2, u64, u128, Option<crate::palw_mesh_v1::PalwAuditOutcomeV1>);
wire_struct!(crate::palw_mesh_v1::PalwAuditOutcomeV1; bool, u64);
wire_struct!(crate::palw_mesh_v1::PalwAuditRowV1; u64, u64, u64, u64, u128, Vec<crate::palw_mesh_v1::PalwAuditAssignmentV1>, bool);
wire_struct!(crate::palw_mesh_v1::PalwCappedClaimRowV1; crate::Hash64, u64, Option<u64>);
wire_struct!(crate::palw_mesh_v1::PalwMeshStateV1; std::collections::BTreeMap<crate::Hash64, crate::palw_mesh_v1::PalwWitnessProfileRowV1>, std::collections::BTreeMap<crate::Hash64, crate::palw_mesh_v1::PalwAuditRowV1>, std::collections::BTreeMap<crate::Hash64, crate::palw_mesh_v1::PalwTrapRowV1>, std::collections::BTreeMap<crate::Hash64, crate::palw_mesh_v1::PalwCappedClaimRowV1>);
wire_struct!(crate::palw_mesh_v1::PalwTrapRowV1; crate::palw_state_v2::PalwBondKeyV2, u64, u128, Option<crate::Hash64>);
wire_struct!(crate::palw_mesh_v1::PalwWitnessProfileRowV1; u64, u32, u64, u32);
wire_struct!(crate::palw_model_artifact_v2::ModelArtifactBindingRowV2; [u8;64], [u8;64], u32, crate::Hash64, crate::Hash64, u32, u32, crate::palw_state_v2::PalwBondKeyV2, u64, u64, u64, u64, bool, Vec<u8>);
wire_struct!(crate::palw_model_benefits_v1::PalwModelBenefitPendingV1; Vec<crate::palw_model_benefits_v1::PalwModelBenefitTierV1>, u64, u64, u64);
wire_struct!(crate::palw_model_benefits_v1::PalwModelBenefitTierV1; u64, u32, u64, u64, Vec<u8>);
wire_struct!(crate::palw_model_benefits_v1::PalwModelBenefitsV1; Vec<crate::palw_model_benefits_v1::PalwModelBenefitTierV1>, u64, u64, u64, Option<crate::palw_state_v2::PalwBondKeyV2>, Option<crate::palw_model_benefits_v1::PalwModelBenefitPendingV1>);
wire_struct!(crate::palw_model_conformance_v2::ModelConformanceRowV2; crate::Hash64, [u8;64], [u8;64], [u8;64], crate::Hash64, [u8;64], [u8;64], u64, u64, u64, u64);
wire_struct!(crate::palw_model_lines_v1::PalwArtifactOwnerV1; crate::Hash64, crate::Hash64, u32);
wire_struct!(crate::palw_model_lines_v1::PalwModelEvaluationV1; crate::Hash64, u32, crate::Hash64, u64, bool);
impl NativeWireScanV1 for crate::palw_model_lines_v1::PalwModelLineStatusV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_model_lines_v1::PalwModelLineV1; crate::Hash64, Option<crate::palw_state_v2::PalwBondKeyV2>, Option<crate::palw_state_v2::PalwBondKeyV2>, Option<crate::palw_state_v2::PalwBondKeyV2>, Vec<u8>, u64, u32, Vec<u32>, u32, u16, crate::palw_model_lines_v1::PalwModelLineStatusV1, Option<u64>);
wire_struct!(crate::palw_model_lines_v1::PalwModelProposalV1; crate::Hash64, crate::Hash64, crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, u64, Option<u32>);
wire_struct!(crate::palw_model_lines_v1::PalwModelVersionV1; crate::Hash64, Option<u32>, Option<crate::Hash64>, Option<crate::Hash64>, Option<crate::Hash64>, Option<crate::Hash64>, Option<crate::Hash64>, u64, Option<crate::palw_state_v2::PalwBondKeyV2>, crate::palw_model_lines_v1::PalwVersionStatusV1, crate::palw_model_lines_v1::PalwVersionUsageV1);
impl NativeWireScanV1 for crate::palw_model_lines_v1::PalwVersionStatusV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_model_lines_v1::PalwVersionUsageV1; u64, u64, u128, Option<u64>, Option<u64>);
wire_struct!(crate::palw_model_refutation_seal_v4::ModelRefutationSealRowV4; u16, [u8;64], u64, u64, u64, u64);
wire_struct!(crate::palw_model_registry_v1::PalwDerivedProfileV1; u16, u32, u32, u32, u32, u64, u64);
wire_struct!(crate::palw_model_registry_v1::PalwModelLifecycleRowV1; crate::palw_model_registry_v1::PalwModelLifecycleV1, crate::palw_model_registry_v1::PalwModelWorkV1, crate::palw_model_registry_v1::PalwDerivedProfileV1, u64, u32, u32, u32, u32, u32, u32, u32, u64, u32, u16);
impl NativeWireScanV1 for crate::palw_model_registry_v1::PalwModelLifecycleV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            6 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            7 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_model_registry_v1::PalwModelWorkV1; u128, u128, u64, u64, bool);
wire_struct!(crate::palw_model_registry_v1::PalwProbationMemoryV1; Option<u32>, Vec<crate::palw_state_v2::PalwBondKeyV2>);
wire_struct!(crate::palw_model_registry_v1::PalwSeatReadinessRowV1; u64, u64, u32, u8, u32);
wire_struct!(crate::palw_model_vector_da_v2::ModelVectorDemandV2; crate::Hash64, crate::palw_model_vector_da_v2::ModelVectorUnitV2, crate::palw_state_v2::PalwBondKeyV2, crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, crate::Hash64, u8, u64, u64, u64, u64, u64, u64, u32, [u8;64], bool);
wire_struct!(crate::palw_model_vector_da_v2::ModelVectorServedV2; u64, [u8;64]);
wire_struct!(crate::palw_model_vector_da_v2::ModelVectorUnitV2; u32, u16, u16, u8, u64, u64);
wire_struct!(crate::palw_model_vector_v2::ModelVectorHeaderV2; [u8;64], crate::Hash64, crate::Hash64, u32, u32, u64, u64, crate::palw_state_v2::PalwBondKeyV2, u64, u64, u64, bool, bool);
wire_struct!(crate::palw_model_vector_v2::ModelVectorRowV2; crate::palw_model_vector_v2::ModelVectorHeaderV2, Vec<u8>);
wire_struct!(crate::palw_monetary_material_v1::PalwMonetaryMaterialChallengeV1; crate::palw_state_v2::PalwBondKeyV2, crate::Hash64, u8, u32, crate::palw_state_v2::PalwBondKeyV2, u64, bool, Option<crate::palw_held_da_v1::PalwHeldMissingV1>, Option<Box<crate::palw_step_leg::PalwStepBindingV2>>, Option<u32>);
wire_struct!(crate::palw_monetary_material_v1::PalwMonetaryMaterialPaymentObservationV1; crate::Hash64, crate::Hash64, crate::Hash64, u64, u64, crate::Hash64, u64, crate::Hash64, Option<crate::Hash64>);
wire_struct!(crate::palw_monetary_material_v1::PalwMonetaryMaterialProviderV1; crate::palw_state_v2::PalwBondKeyV2, u64, u64, bool, crate::Hash64, crate::Hash64, u64, bool, bool);
wire_struct!(crate::palw_monetary_material_v1::PalwMonetaryMaterialRowV1; crate::palw_monetary_material_v1::PalwMonetaryMaterialTermsV1, crate::Hash64, u64, u64, bool, Option<crate::palw_attempt_material_v3::PalwNativeMaterialBindingV3>, Vec<crate::palw_monetary_material_v1::PalwMonetaryMaterialProviderV1>, Vec<crate::palw_monetary_material_v1::PalwMonetaryMaterialChallengeV1>, Option<u64>, Option<crate::Hash64>, Option<u64>, Option<crate::palw_monetary_material_v1::PalwMonetaryMaterialPaymentObservationV1>, crate::Hash64, u64, u128, u128, crate::Hash64, std::collections::BTreeMap<crate::Hash64, crate::Hash64>);
wire_struct!(crate::palw_monetary_material_v1::PalwMonetaryMaterialTermsV1; u16, crate::Hash64, Vec<crate::Hash64>, Option<crate::Hash64>, crate::palw_state_v2::PalwBondKeyV2, crate::Hash64, crate::Hash64, u64, Option<crate::palw_material_capacity_v1::PalwMaterialManifestV1>, Option<crate::palw_attempt_material_v3::PalwNativeMaterialSessionV3>, u64, u64, bool);
wire_struct!(crate::palw_native_carriage_v22::NativeCarrierEnvelopeV22; u64, u32, u32, u64, u64, u64, u64);
wire_struct!(crate::palw_native_final_v27::NativeFinalClassBasisV27; [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u64, u64, u64, u64, u64, u64, u64, u64, u64);
wire_struct!(crate::palw_native_initial_v31::NativeInitialHeaderV31; u16, [u8;64], crate::palw_state_v2::PalwBondKeyV2, [u8;64], u64, crate::palw_native_initial_v31::NativeInitialStartV31, [u8; 64], [u8; 512], u8);
wire_struct!(crate::palw_native_initial_v31::NativeInitialStartV31; u16, u16, u8, u64, u32, u16, [u8;64], u64);
wire_struct!(crate::palw_native_paged_carriage_v29::NativePagedCarriageRecordV29; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], crate::palw_native_carriage_v22::NativeCarrierEnvelopeV22, crate::palw_native_carriage_v22::NativeCarrierEnvelopeV22, u32, u32, u16, u32, u64, u64, u64, u64);
wire_struct!(crate::palw_native_paged_proof_v29::NativePagedJobStatusV29; u16, u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], bool, [[u8;64]; 4], u8, u64, u64, u64, u64);
wire_struct!(crate::palw_native_paged_proof_v29::NativePagedLeaseJournalV29; u16, u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], crate::palw_flat_execution_capacity_v22::NativeCarriageLeaseV22, crate::palw_native_carriage_v22::NativeCarrierEnvelopeV22);
wire_struct!(crate::palw_native_paged_proof_v29::NativePagedProofPlanRecordV29; u16, [u8;64], [u8;64], [u8;64], u32, u16, u64, u64, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11);
wire_struct!(crate::palw_native_paged_proof_v29::NativePagedSealV29; u16, u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], crate::palw_state_v2::PalwBondKeyV2, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u16, u32, u64, u64, u64, u64, u64, [u8;64], [u8; 512], [u8; 64], u8, u8, bool, [u8;64], u8, bool, bool, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11);
wire_struct!(crate::palw_native_queue_v30::NativeQueueLeaseV30; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u32, u64, u64, bool);
wire_struct!(crate::palw_native_queue_v30::NativeQueuePlanRecordV30; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u32, u64, u64, u64);
wire_struct!(crate::palw_native_reply_parts_v29::NativeReplyProgressV29; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u8, [u8;64], u16, u32, u8, bool, bool);
wire_struct!(crate::palw_native_text_stop_v29::NativeTextBookV29; crate::palw_native_text_stop_v29::NativeTextFundingV29, Option<crate::palw_native_text_stop_v29::NativeTextClaimV29>, crate::palw_native_text_stop_v29::NativeTextTerminalV29, u64, Option<crate::palw_native_text_stop_v29::NativeTextSettlementV29>);
wire_struct!(crate::palw_native_text_stop_v29::NativeTextClaimV29; [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u32, u32, [u8;64], [u8;64], [u8;64], crate::palw_native_text_stop_v29::NativeTextStopV29, u64, u64, u64);
wire_struct!(crate::palw_native_text_stop_v29::NativeTextClassV29; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u32, [u8;64], [u8;64], [u8;64]);
wire_struct!(crate::palw_native_text_stop_v29::NativeTextConsentV29; u16, [u8;64], [u8;64], crate::palw_native_text_stop_v29::NativeTextMaximumV29, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u64, u64, u64, [u8;64], [u8;64], [u8;64]);
wire_struct!(crate::palw_native_text_stop_v29::NativeTextFundingV29; crate::palw_native_text_stop_v29::NativeTextRequestV29, crate::palw_native_text_stop_v29::NativeTextMaximumV29, [u8;64], [u8;64], [u8;64], [u8;64], u64, u64, u64, u64);
wire_struct!(crate::palw_native_text_stop_v29::NativeTextMaximumV29; u16, [u8;64], [u8;64], [u8;64], [u8;64], u32, [u8;64], [u8;64], u32, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u64, u64, u64, u64, u64, u64);
wire_struct!(crate::palw_native_text_stop_v29::NativeTextPrepareV29; u16, crate::palw_native_text_stop_v29::NativeTextRequestV29, Vec<u32>);
wire_struct!(crate::palw_native_text_stop_v29::NativeTextRequestV29; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u32, u32, u32, u64);
wire_struct!(crate::palw_native_text_stop_v29::NativeTextSettlementV29; [u8;64], [u8;64], Option<[u8;64]>, crate::palw_native_text_stop_v29::NativeTextTerminalV29, u64, u64, u64, u64, [u8;64], u64, u64, [u8;64]);
impl NativeWireScanV1 for crate::palw_native_text_stop_v29::NativeTextStopV29 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for crate::palw_native_text_stop_v29::NativeTextTerminalV29 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_native_trace_da_v29::NativePublicRequestV29; u16, [u8;64], [u8;64], u8, crate::palw_native_trace_da_v29::NativePublicUnitV29);
impl NativeWireScanV1 for crate::palw_native_trace_da_v29::NativePublicUnitV29 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::palw_flat_model_vector_da_v9::FlatVectorUnitV9 as NativeWireScanV1>::scan_native_wire_v1(
                    reader,
                )?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_offence_v1::PalwConsumedOffenceV1; crate::palw_offence_v1::PalwOffenceKindV1, crate::tx::TransactionOutpoint, u64, u64, crate::Hash64, u64, crate::Hash64);
impl NativeWireScanV1 for crate::palw_offence_v1::PalwOffenceKindV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            6 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            7 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_onboarding_v1::ArtifactBindingRowV1; crate::palw_state_v2::PalwBondKeyV2, u64, u64, u64, u64, bool);
impl NativeWireScanV1 for crate::palw_onboarding_v1::ConformanceAttemptEndV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            6 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_onboarding_v1::ConformanceAttemptRowV1; misaka_palw_challenge::lifecycle::OnboardingRecordV1, misaka_palw_challenge::conformance::ConformanceCommitmentV1, u64, u64, Vec<crate::Hash64>, Vec<crate::Hash64>, Option<crate::palw_onboarding_v1::PostedEvidenceRowV1>, Option<(crate::palw_onboarding_v1::ConformanceAttemptEndV1, u64)>, Vec<crate::palw_state_v2::PalwBondKeyV2>);
wire_struct!(crate::palw_onboarding_v1::ConformanceRowV1; crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, u64);
wire_struct!(crate::palw_onboarding_v1::KernelBindingRowV1; crate::Hash64, crate::Hash64, crate::Hash64, crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, u64);
wire_struct!(crate::palw_onboarding_v1::PostedEvidenceRowV1; crate::Hash64, crate::Hash64, crate::Hash64, u64, u64, u64);
wire_struct!(crate::palw_panel_lifetime_v2::PalwPanelAuthorizationRowV2; misaka_palw_kernel::panel_lifetime_v2::PanelLifetimeAuthorizationV2, crate::palw_state_v2::PalwBondKeyV2, u64, crate::Hash64, crate::Hash64, Option<crate::Hash64>);
wire_struct!(crate::palw_panel_lifetime_v2::PalwPanelConsentCaptureV2; [u8; 64], crate::Hash64, u64, crate::Hash64);
wire_struct!(crate::palw_panel_lifetime_v2::PalwPanelLifetimeEnrollmentV2; misaka_palw_kernel::panel_lifetime_v2::PanelConsumerLifetimeTermsV2, crate::palw_state_v2::PalwBondKeyV2, u64, crate::Hash64, crate::Hash64, Option<u64>, u64, u8);
wire_struct!(crate::palw_panel_source_fee_v2::PalwPanelSourceFeeAuthorityV2; u16, crate::palw_panel_source_fee_v2::PalwPanelSourceRequestV2, [u8;64], misaka_palw_challenge::policy::PostCommitChallengePolicyV1, misaka_palw_panel::PanelPolicyV1, crate::palw_beacon_source_opt_in_v1::PalwBeaconSourcePolicyV1, u64, u64, crate::palw_state_v2::PalwBondKeyV2);
wire_struct!(crate::palw_panel_source_fee_v2::PalwPanelSourceRequestV2; crate::Hash64, crate::Hash64, crate::Hash64, u64, u64, u64);
wire_struct!(crate::palw_panel_source_snapshot_v1::PalwPanelSourceSnapshotV1; u16, crate::Hash64, crate::Hash64, crate::Hash64, u64, u64, u64, u64, crate::Hash64, crate::Hash64, u64, BTreeSet<[u8;64]>, BTreeSet<[u8;64]>, BTreeSet<[u8;64]>);
impl NativeWireScanV1 for crate::palw_panel_v2::PalwReceiptVerdictV2 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_panel_var_v1::PalwPanelLiabilityRecordV1; crate::Hash64, crate::Hash64, crate::Hash64, crate::Hash64, crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, Option<u64>, Option<crate::palw_state_v2::PalwVoidReasonV2>, Vec<(crate::tx::TransactionOutpoint, crate::Hash64)>, u128, u64, u64, crate::Hash64, bool, crate::Hash64, u16, Option<crate::palw_economic_safety_v1::PalwLicenceDoorTagV1>, u8, u128, u64);
wire_struct!(crate::palw_panel_var_v1::PalwSlashableLockV1; crate::Hash64, u128, u64, u64, crate::palw_verification_v2::PalwSegmentMaskV2, u16);
wire_struct!(crate::palw_prefix_base0_codec_v1::PalwBase0PrefixDigestV1; crate::Hash64, u32, u64, u32, u64, crate::palw_rendering_stream_v1::LeafHashContinuationV1);
wire_struct!(crate::palw_prefix_custody_v1::PalwPrefixCustodyContractV1; crate::Hash64, crate::Hash64, crate::Hash64, u64, crate::Hash64, BTreeSet<crate::palw_state_v2::PalwBondKeyV2>, BTreeMap<crate::palw_state_v2::PalwBondKeyV2, crate::palw_prefix_custody_v1::PalwPrefixCustodyReceiptV1>);
wire_struct!(crate::palw_prefix_custody_v1::PalwPrefixCustodyReceiptV1; u64, u64, crate::Hash64);
impl NativeWireScanV1 for crate::palw_prefix_family_state_v1::PalwPrefixCacheUnitV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_prefix_family_state_v1::PalwQwen36PrefixCacheDigestV1; crate::palw_prefix_family_state_v1::PalwQwen36PrefixCachePlanV1, u64, u64, crate::palw_rendering_stream_v1::LeafHashContinuationV1);
wire_struct!(crate::palw_prefix_family_state_v1::PalwQwen36PrefixCachePlanV1; crate::Hash64, u16, u16, u16, u32, u64, u16, u64, u16, u64, u64, u64);
wire_struct!(crate::palw_prefix_gdn_contract_v1::PalwPrefixGdnContractV1; crate::Hash64, crate::Hash64, crate::Hash64, u16, u32, u32, u32, u32, [u8; 68]);
wire_struct!(crate::palw_prefix_gdn_replay_v1::PalwPrefixGdnHeadReplayV1; crate::Hash64, crate::Hash64, crate::Hash64, u16, u32, u32, u32, u32, u32, u32, u64, [u8; 68], Vec<i32>);
wire_struct!(crate::palw_prefix_history_v1::PalwPrefixHistoryRequestV1; crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, u64, crate::Hash64, crate::Hash64, u64, u64);
wire_struct!(crate::palw_prefix_owned_court_v1::PalwOwnedPrefixCourtV1; crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, u64, crate::palw_rendering_resources_v1::PalwRenderingClaimTermsV1, u64, u64, u64, u64, u64, crate::Hash64, crate::Hash64, u64, u64, u64, u64, crate::palw_prefix_family_court_v1::PalwPrefixFamilyCourtV1);
wire_struct!(crate::palw_prefix_public_owned_v1::PalwOwnedPublicPrefixCourtV1; crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, u64, crate::palw_rendering_resources_v1::PalwRenderingClaimTermsV1, u64, u64, crate::palw_state_v2::PalwBondKeyV2, crate::palw_prefix_custody_v1::PalwPrefixCustodyContractV1, crate::palw_prefix_public_source_v1::PalwPrefixPublicAcquisitionPlanV1, u64, crate::palw_prefix_owned_court_v1::PalwOwnedPrefixCourtV1, Option<crate::palw_prefix_public_source_v1::PalwPrefixPublicPendingV1>, u64, u64, u64, u64, bool, bool, bool, u64, crate::Hash64);
wire_struct!(crate::palw_prefix_public_source_v1::PalwPrefixPublicAcquisitionPlanV1; crate::palw_rendering_resources_v1::PalwRenderingResourcePlanV1, u64, u64, u64, u64, u64);
wire_struct!(crate::palw_prefix_public_source_v1::PalwPrefixPublicSourceV1; crate::palw_step::PalwStepCoordinateV1, u64, u32, u32, crate::Hash64, u64, u64, crate::palw_rendering_stream_v1::LeafHashContinuationV1);
wire_struct!(crate::palw_prefix_source_stream_v1::PalwPrefixSourceRangeV1; u32, u32, u32, u64, u64, u64);
wire_struct!(crate::palw_prompt_ids_v1::PalwPromptIdsOpeningV1; u32, u32, Vec<u32>, crate::palw_step_leg::PalwStepOpeningV1);
wire_struct!(crate::palw_provider_court_v1::ProviderChallengeRowV1; crate::palw_state_v2::PalwBondKeyV2, u64, u64, u64, bool);
wire_struct!(crate::palw_provider_court_v1::ProviderLeaseRowV1; u64, u64, u64, bool);
wire_struct!(crate::palw_provider_court_v1::ProviderSubjectRowV1; Option<u64>, Option<u64>, Vec<crate::palw_state_v2::PalwBondKeyV2>, Vec<(crate::Hash64, Vec<(u8, u32)>)>);
impl NativeWireScanV1 for crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(
                    <crate::palw_public_binding_v1::PalwPublicLegacyBindingHeaderV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?,
                )?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(
                    <crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?,
                )?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_public_binding_v1::PalwPublicLegacyBindingHeaderV1; u16, crate::palw_v2::PalwJobContextV2, crate::palw_legs::PalwCheckpointProfileV1, crate::Hash64, crate::Hash64, crate::Hash64, u64, crate::Hash64, u32, crate::Hash64, crate::Hash64);
wire_struct!(crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1; u16, crate::palw_v2::PalwJobContextV2, crate::Hash64, crate::Hash64, u64, crate::Hash64, crate::Hash64);
impl NativeWireScanV1 for crate::palw_real_share_v1::PalwFloorModeV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_real_share_v1::PalwFloorStateV1; crate::palw_real_share_v1::PalwFloorModeV1, Option<u64>);
wire_struct!(crate::palw_rendering_resources_v1::PalwRenderingClaimTermsV1; u64, u64, u64, u64, u64, u64, crate::palw_rendering_resources_v1::PalwRenderingResourcePlanV1);
wire_struct!(crate::palw_rendering_resources_v1::PalwRenderingResourcePlanV1; u32, u32, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64);
wire_struct!(crate::palw_rendering_resources_v1::PalwRenderingResourcePricesV1; u64, u64, u64, u64, u64, u64);
wire_struct!(crate::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1; crate::palw_rendering_resources_v1::PalwRenderingResourcePricesV1, u64, u64, u64, u32);
wire_struct!(crate::palw_rendering_stream_v1::LeafHashContinuationV1; [u64; 8], u128, Vec<u8>);
wire_struct!(crate::palw_rendering_stream_v1::PalwRenderingStreamV1; crate::Hash64, crate::Hash64, u32, bool, u32, u32, crate::Hash64, crate::palw_rendering_stream_v1::LeafHashContinuationV1, Option<crate::palw_decode_constraint_v1::PalwConstraintStateV1>);
wire_struct!(crate::palw_seat_availability_v1::PalwSeatAvailEpochV1; u64, u32, u32);
wire_struct!(crate::palw_seat_availability_v1::PalwSeatAvailabilityV1; Option<u64>, Vec<crate::palw_seat_availability_v1::PalwSeatAvailEpochV1>);
wire_struct!(crate::palw_selected_event_source_v1::PalwSelectedEventCoordinateV1; u32, u32);
wire_struct!(crate::palw_selected_event_source_v1::PalwSelectedEventRequestV1; crate::palw_selected_event_source_v1::PalwSelectedEventCoordinateV1, crate::Hash64, BTreeSet<crate::palw_state_v2::PalwBondKeyV2>, Option<crate::palw_selected_event_source_v1::PalwSelectedTiledEventV1>, bool);
wire_struct!(crate::palw_selected_event_source_v1::PalwSelectedEventSessionV1; crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, u64, crate::Hash64, u64, u64, u64, BTreeSet<crate::palw_state_v2::PalwBondKeyV2>, BTreeMap<crate::palw_selected_event_source_v1::PalwSelectedEventCoordinateV1, crate::palw_selected_event_source_v1::PalwSelectedEventRequestV1>);
wire_struct!(crate::palw_selected_event_source_v1::PalwSelectedTiledEventV1; Vec<u32>, crate::Hash64, crate::palw_step_leg::PalwStepOpeningV1, Vec<i32>, crate::palw_step_leg::PalwStepOpeningV1);
wire_struct!(crate::palw_shard_court_v1::PalwLeafEvidenceV1; crate::palw_step_refute::PalwExecutionStepRefutationV1, Vec<crate::palw_artifact::PalwArtifactOpeningV1>, Option<crate::palw_prompt_ids_v1::PalwPromptIdsOpeningV1>);
wire_struct!(crate::palw_shard_licensing_v1::PalwClassShardPlanV1; u32, u64);
wire_struct!(crate::palw_shard_licensing_v1::PalwShardLicensingProgressV1; u32, Vec<u64>);
wire_struct!(crate::palw_staged_bundle_v1::PalwNativeSourceContextV1; Box<crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1>, Box<crate::palw_freeprompt_v3::PalwFreePromptJobV3>);
wire_struct!(crate::palw_staged_bundle_v1::PalwOwnedStagedBundleV1; crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, u64, crate::Hash64, crate::palw_staged_bundle_v1::PalwStagedBundlePlanV1, Option<crate::palw_staged_bundle_v1::PalwTokenizerInventoryV1>, Option<crate::palw_selected_event_source_v1::PalwSelectedEventSessionV1>, Option<crate::palw_staged_court_state_v1::PalwSharedCourtV1<crate::palw_staged_bundle_v1::PalwSelectedSourceContextV1>>, Option<crate::palw_staged_court_state_v1::PalwSharedCourtV1<crate::palw_staged_bundle_v1::PalwNativeSourceContextV1>>, Option<
        crate::palw_staged_court_state_v1::PalwSharedCourtV1<crate::palw_flat_constraint_stream_v1::PalwFlatConstraintSourceSessionV1>,
    >, Option<crate::palw_staged_court_state_v1::PalwSharedCourtV1<crate::palw_staged_bundle_v1::PalwSelectedSourceContextV1>>, Option<
        crate::palw_staged_court_state_v1::PalwSharedCourtV1<
            crate::palw_float_constraint_source_v1::PalwFloatConstraintSourceSessionV1,
        >,
    >, Option<crate::palw_staged_court_state_v1::PalwSharedCourtV1<crate::palw_staged_bundle_v1::PalwSelectedSourceContextV1>>, std::collections::BTreeMap<
        crate::Hash64,
        crate::palw_staged_court_state_v1::PalwSharedCourtV1<crate::palw_history_custody_v1::PalwHistoryRecoveryV1>,
    >, Option<u8>, Vec<crate::palw_staged_bundle_v1::PalwStagedOpenedFamilyV1>);
wire_struct!(crate::palw_staged_bundle_v1::PalwSelectedSourceContextV1; Box<crate::palw_step_leg::PalwStepBindingV2>, Box<crate::palw_freeprompt_v3::PalwFreePromptJobV3>);
wire_struct!(crate::palw_staged_bundle_v1::PalwStagedBundleContractV1; crate::palw_staged_court_state_v1::PalwStagedClaimContractV1, crate::palw_staged_bundle_v1::PalwStagedBundlePlanV1, crate::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1, crate::palw_history_custody_v1::PalwHistoryCustodyContractV1, crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1, Option<crate::palw_state_v2::PalwBondKeyV2>);
wire_struct!(crate::palw_staged_bundle_v1::PalwStagedBundlePlanV1; crate::palw_rendering_resources_v1::PalwRenderingClaimTermsV1, Option<crate::palw_staged_bundle_v1::PalwTokenizerInventoryPlanV1>, Vec<crate::palw_staged_bundle_v1::PalwStagedFamilyPlanV1>);
wire_struct!(crate::palw_staged_bundle_v1::PalwStagedFamilyPlanV1; crate::palw_staged_bundle_v1::PalwStagedFamilyV1, Option<crate::palw_rendering_resources_v1::PalwRenderingResourcePlanV1>, crate::palw_rendering_resources_v1::PalwRenderingClaimTermsV1);
wire_struct!(crate::palw_staged_bundle_v1::PalwStagedFamilyRowV1; crate::palw_staged_court_state_v1::PalwStagedCourtRowV1);
impl NativeWireScanV1 for crate::palw_staged_bundle_v1::PalwStagedFamilyV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_staged_bundle_v1::PalwStagedNativeSeedContextV1; Box<crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamNativeSeedV1>, Box<crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1>, Box<crate::palw_freeprompt_v3::PalwFreePromptJobV3>);
wire_struct!(crate::palw_staged_bundle_v1::PalwStagedOpenedFamilyV1; crate::palw_staged_bundle_v1::PalwStagedFamilyRowV1, crate::palw_staged_court_state_v1::PalwSharedCourtV1<crate::palw_staged_bundle_v1::PalwStagedStageContextV1>);
wire_struct!(crate::palw_staged_bundle_v1::PalwStagedSeedContextV1; Box<crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamSeedV1>, Box<crate::palw_step_leg::PalwStepBindingV2>, Box<crate::palw_freeprompt_v3::PalwFreePromptJobV3>);
impl NativeWireScanV1 for crate::palw_staged_bundle_v1::PalwStagedStageContextV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::palw_staged_bundle_v1::PalwStagedSeedContextV1 as NativeWireScanV1>::scan_native_wire_v1(
                    reader,
                )?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(
                    <crate::palw_staged_bundle_v1::PalwStagedNativeSeedContextV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?,
                )?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_staged_bundle_v1::PalwTokenizerInventoryPlanV1; u64, crate::palw_rendering_resources_v1::PalwRenderingResourcePlanV1);
wire_struct!(crate::palw_staged_bundle_v1::PalwTokenizerInventoryV1; u64, crate::Hash64, u64, u64, u64, bool, u64, crate::palw_rendering_stream_v1::LeafHashContinuationV1);
wire_struct!(crate::palw_staged_court_state_v1::PalwStagedClaimContractV1; crate::palw_rendering_resources_v1::PalwRenderingClaimTermsV1, crate::palw_state_v2::PalwBondKeyV2, std::collections::BTreeSet<crate::palw_state_v2::PalwBondKeyV2>, Option<crate::palw_prefix_custody_v1::PalwPrefixCustodyContractV1>);
impl NativeWireScanV1 for crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<crate::palw_state_v2::PalwBondKeyV2 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<crate::palw_state_v2::PalwBondKeyV2 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for crate::palw_staged_court_state_v1::PalwStagedCourtRowV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(
                    <crate::palw_staged_court_state_v1::PalwStagedClaimContractV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?,
                )?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::palw_staged_court_state_v1::PalwSharedCourtV1<
                    crate::palw_constraint_stream_owned_v1::PalwOwnedConstraintStreamV1,
                > as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::palw_staged_court_state_v1::PalwSharedCourtV1<
                    crate::palw_tokenizer_owned_court_v1::PalwOwnedTokenizerCourtV1,
                > as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::palw_staged_court_state_v1::PalwSharedCourtV1<
                    crate::palw_prefix_owned_court_v1::PalwOwnedPrefixCourtV1,
                > as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::palw_staged_court_state_v1::PalwSharedCourtV1<
                    crate::palw_prefix_public_owned_v1::PalwOwnedPublicPrefixCourtV1,
                > as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(
                    <Box<crate::palw_staged_bundle_v1::PalwStagedBundleContractV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?,
                )?;
                Ok(sum)
            }
            6 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::palw_staged_court_state_v1::PalwSharedCourtV1<
                    crate::palw_staged_bundle_v1::PalwOwnedStagedBundleV1,
                > as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            7 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::palw_staged_court_state_v1::PalwSharedCourtV1<
                    crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryV1,
                > as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            8 => {
                let mut sum = NativeWireShapeV1::default();
                sum=sum.merge(<crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryCertificateV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            9 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::palw_staged_court_state_v1::PalwSharedCourtV1<
                    crate::palw_tir_prefix_owned_v1::PalwOwnedTirPrefixCourtV1,
                > as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            10 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<Box<crate::palw_tir_class_v1::PalwTirLayoutV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_state_v2::PalwBlockContextV2; crate::BlockHash, u64, u64, u64);
wire_struct!(crate::palw_state_v2::PalwBondKeyV2; crate::tx::TransactionOutpoint);
wire_struct!(crate::palw_state_v2::PalwBondStateV2; Vec<u8>, crate::Hash64, u64, u64, crate::palw_state_v2::PalwBondStatusV2, u64, crate::Hash64, std::collections::BTreeSet<crate::Hash64>);
impl NativeWireScanV1 for crate::palw_state_v2::PalwBondStatusV2 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_state_v2::PalwCapabilityStateV2; crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, u64);
wire_struct!(crate::palw_state_v2::PalwCertifiedFamilyStateV2; crate::palw_e2e_adjudicability::PalwE2eFamilyV1, u64);
wire_struct!(crate::palw_state_v2::PalwChainStateV2; BTreeMap<crate::palw_state_v2::PalwBondKeyV2, crate::palw_state_v2::PalwBondStateV2>, BTreeMap<crate::palw_state_v2::PalwBondKeyV2, u128>, BTreeMap<crate::Hash64, crate::palw_state_v2::PalwClassStateV2>, BTreeMap<crate::Hash64, crate::palw_state_v2::PalwClassTargetV2>, BTreeMap<crate::Hash64, u16>, Option<crate::palw_state_v2::PalwEpochBudgetsV2>, BTreeMap<crate::Hash64, crate::palw_state_v2::PalwClassTargetV2>, BTreeMap<crate::Hash64, crate::palw_state_v2::PalwCapabilityStateV2>, BTreeMap<crate::Hash64, crate::palw_state_v2::PalwClaimStateV2>, BTreeMap<crate::Hash64, crate::palw_state_v2::PalwPanelStateV2>, BTreeMap<crate::Hash64, crate::palw_state_v2::PalwCourtSessionStateV2>, BTreeMap<crate::Hash64, crate::palw_state_v2::PalwEpochCounterV2>, BTreeMap<crate::Hash64, crate::palw_state_v2::PalwEpochCounterV2>, BTreeMap<crate::Hash64, crate::palw_model_market_v1::PalwModelMarketV1>, BTreeMap<(crate::Hash64, crate::Hash64), u64>, BTreeMap<crate::Hash64, crate::palw_model_lines_v1::PalwModelLineV1>, BTreeMap<(crate::Hash64, u32), crate::palw_model_lines_v1::PalwModelVersionV1>, BTreeMap<crate::Hash64, crate::palw_model_benefits_v1::PalwModelBenefitsV1>, BTreeMap<(crate::Hash64, crate::Hash64), u64>, BTreeMap<crate::Hash64, crate::palw_model_lines_v1::PalwModelProposalV1>, BTreeMap<(crate::Hash64, u32, crate::palw_state_v2::PalwBondKeyV2), crate::palw_model_lines_v1::PalwModelEvaluationV1>, BTreeMap<crate::Hash64, crate::Hash64>, BTreeMap<u32, crate::evm::model_market::PalwEvmSettlementV1>, BTreeMap<crate::Hash64, crate::palw_shard_licensing_v1::PalwClassShardPlanV1>, BTreeMap<(crate::palw_state_v2::PalwBondKeyV2, crate::Hash64), Vec<u32>>, BTreeMap<crate::Hash64, crate::palw_shard_licensing_v1::PalwShardLicensingProgressV1>, BTreeMap<crate::Hash64, crate::palw_held_da_v1::PalwHeldMissingV1>, BTreeMap<crate::Hash64, BTreeMap<crate::palw_state_v2::PalwBondKeyV2, u64>>, BTreeMap<crate::Hash64, u64>, BTreeMap<crate::Hash64, u64>, crate::palw_vertex_v1::PalwVertexStateV1, crate::palw_work_slice_v2::PalwExecV2StateV1, BTreeMap<crate::Hash64, crate::palw_work_slice_v2::PalwExecV2RewardRowV1>, BTreeMap<(crate::Hash64, crate::palw_state_v2::PalwBondKeyV2), u64>, BTreeMap<crate::Hash64, crate::palw_work_slice_v2::PalwExecOriginV1>, BTreeMap<crate::Hash64, crate::palw_work_slice_v2::PalwExecJobUseV1>, BTreeSet<(u64, crate::Hash64)>, BTreeMap<crate::palw_state_v2::PalwBondKeyV2, BTreeSet<crate::Hash64>>, BTreeMap<crate::Hash64, crate::palw_tir_shard_v1::PalwTirShardPlanV1>, BTreeMap<crate::Hash64, crate::palw_tir_shard_v1::PalwTirShardClaimV1>, Option<crate::palw_real_share_v1::PalwFloorStateV1>, BTreeMap<crate::palw_state_v2::PalwBondKeyV2, crate::palw_seat_availability_v1::PalwSeatAvailabilityV1>, BTreeMap<(crate::palw_state_v2::PalwBondKeyV2, crate::Hash64, crate::Hash64), crate::palw_model_registry_v1::PalwSeatReadinessRowV1>, Option<crate::palw_kernel_route_v1::PalwKernelRouteStateV1>, Option<crate::palw_bond_budget_v1::PalwBondBudgetStateV1>, BTreeMap<crate::Hash64, crate::palw_state_v2::PalwPanelDutyRowV1>, u64, u64, BTreeMap<crate::Hash64, crate::palw_execution_lane_v1::PalwExecFinalV1>, BTreeMap<u64, crate::palw_execution_lane_v1::PalwExecScheduleV1>, BTreeMap<u64, crate::palw_execution_lane_v1::PalwExecSnapshotV1>, BTreeMap<(u64, u64), u16>, BTreeMap<(u64, u64), u16>, Option<crate::palw_execution_lane_v1::PalwExecSeedAnchorV1>, BTreeMap<crate::Hash64, crate::palw_model_registry_v1::PalwModelLifecycleRowV1>, BTreeMap<(crate::palw_state_v2::PalwBondKeyV2, crate::Hash64), crate::palw_model_registry_v1::PalwSeatReadinessRowV1>, BTreeMap<crate::Hash64, crate::palw_economic_payout_v1::PalwClaimEconomicsV1>, Option<crate::palw_work_target_v1::PalwWorkTargetV2>, BTreeMap<u64, BTreeMap<crate::Hash64, u128>>, Option<crate::palw_work_target_v1::PalwWorkTargetV2>, BTreeMap<(crate::Hash64, crate::Hash64), crate::palw_model_lines_v1::PalwArtifactOwnerV1>, BTreeMap<crate::Hash64, Box<crate::palw_step::PalwShapeProfileV3>>, BTreeMap<(crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, crate::Hash64), crate::palw_state_v2::PalwFpClaimPromptV1>, BTreeMap<crate::Hash64, crate::palw_offence_v1::PalwConsumedOffenceV1>, BTreeMap<(crate::palw_state_v2::PalwBondKeyV2, crate::Hash64), crate::palw_panel_var_v1::PalwSlashableLockV1>, BTreeMap<crate::Hash64, crate::palw_panel_var_v1::PalwPanelLiabilityRecordV1>, BTreeMap<crate::palw_state_v2::PalwBondKeyV2, u128>, BTreeMap<crate::Hash64, crate::palw_state_v2::PalwClassWalkV2>, BTreeMap<crate::Hash64, crate::palw_state_v2::PalwPayoutV2>, BTreeMap<crate::Hash64, crate::palw_state_v2::PalwCertifiedFamilyStateV2>, BTreeMap<crate::Hash64, crate::palw_state_v2::PalwCertifiedFamilyStateV2>, BTreeMap<crate::Hash64, crate::palw_state_v2::PalwClassLaneCertificationV2>, BTreeMap<crate::Hash64, crate::palw_state_v2::PalwPendingChunksV2>, BTreeMap<(crate::Hash64, crate::palw_state_v2::PalwCourtSideV1), crate::palw_state_v2::PalwCourtCloseGroupV2>, BTreeMap<crate::palw_derived_v1::PalwDerivedKeyV1, crate::palw_derived_v1::PalwDerivedRowV1>, u128, u128, u64, crate::BlockHash, Option<crate::palw_state_v2::PalwBlockContextV2>, u128, u64, Vec<u64>, BTreeMap<crate::palw_state_v2::PalwBondKeyV2, Vec<u64>>, BTreeMap<crate::Hash64, crate::palw_state_v2::PalwPendingRewardV1>, BTreeMap<crate::Hash64, crate::palw_state_v2::PalwReporterCommitV1>, BTreeMap<crate::Hash64, crate::palw_state_v2::PalwPayoutV2>, crate::palw_state_v2::PalwReporterCountersV1, BTreeMap<crate::Hash64, crate::palw_vesting_v1::PalwVestingRowV1>, crate::palw_vesting_v1::PalwVestingCountersV1, BTreeMap<(crate::Hash64, crate::palw_state_v2::PalwBondKeyV2), crate::palw_da_rcore_v1::PalwDaSessionV1>, BTreeMap<crate::Hash64, crate::palw_da_rcore_v1::PalwDaClaimV1>, BTreeMap<crate::Hash64, crate::palw_activation_pool_v1::PalwActivationPoolV1>, crate::palw_activation_pool_v1::PalwActivationPoolCountersV1, BTreeMap<(crate::Hash64, crate::Hash64), u64>, BTreeMap<(crate::Hash64, crate::palw_state_v2::PalwBondKeyV2), u64>, BTreeMap<(crate::Hash64, crate::Hash64), crate::palw_state_v2::PalwHeldForfeitV1>, BTreeMap<crate::Hash64, crate::palw_model_registry_v1::PalwProbationMemoryV1>, BTreeMap<crate::palw_state_v2::PalwBondKeyV2, crate::palw_aggregate_liability_v1::PalwBondFreezeV1>, BTreeMap<crate::Hash64, crate::palw_audit_door_v1::PalwAuditStatusV1>, BTreeMap<crate::palw_state_v2::PalwBondKeyV2, u64>, BTreeMap<crate::palw_state_v2::PalwBondKeyV2, crate::palw_issuance_slots_v1::PalwIssuanceBucketV1>, BTreeMap<(crate::palw_state_v2::PalwBondKeyV2, crate::Hash64), u64>, BTreeMap<u64, u32>, BTreeMap<crate::palw_capacity_s567_v1::PalwCapacityLedgerKeyV1, crate::palw_capacity_s567_v1::PalwCapacityLedgerRowV1>, BTreeMap<crate::Hash64, crate::palw_tir_admission_v1::PalwTirClassRecordV1>, BTreeMap<crate::Hash64, Box<crate::palw_tir_dissect_v1::PalwTirDissectPhaseV1>>, BTreeMap<crate::Hash64, crate::palw_gen_class_v1::PalwGenClassRecordV1>, BTreeMap<crate::Hash64, crate::palw_improve_state_v1::PalwImprovementLineV1>, BTreeMap<(crate::Hash64, u64), crate::palw_improve_state_v1::PalwImprovementEpochV1>, BTreeMap<crate::Hash64, crate::palw_improve_state_v1::PalwImprovementPolicyRecordV1>, BTreeMap<crate::Hash64, crate::palw_improve_state_v1::PalwImprovementUsageV1>, BTreeMap<(crate::Hash64, u32), crate::palw_improve_state_v1::PalwLineageHeadEntryV1>, BTreeMap<crate::Hash64, crate::palw_improve_state_v1::PalwImprovementPoolV1>, BTreeMap<(crate::Hash64, u64), crate::palw_improve_state_v1::PalwMaterialFrontierV1>, BTreeMap<(crate::Hash64, u64, u32), crate::palw_improve_state_v1::PalwEpochCandidateV1>, BTreeMap<(crate::Hash64, u64, u8, crate::Hash64), crate::palw_improve_state_v1::PalwPoolEntryV2>, BTreeMap<(crate::Hash64, u64, u32), crate::palw_improve_state_v1::PalwEvalItemV1>, BTreeMap<(crate::Hash64, u64, u32, crate::palw_improve_state_v1::PalwEvalSubjectV1), crate::palw_improve_state_v1::PalwEvalResultV1>, BTreeMap<(crate::Hash64, u64, u32), crate::palw_improve_state_v1::PalwRewardGrantV1>, BTreeMap<crate::palw_state_v2::PalwBondKeyV2, u64>, BTreeMap<(crate::Hash64, crate::Hash64), crate::palw_improve_material_v1::PalwHardCaseRecordV1>, BTreeMap<crate::Hash64, crate::palw_improve_material_v1::PalwDataUseOptInRecordV1>, BTreeMap<(crate::Hash64, crate::Hash64), crate::palw_improve_material_v1::PalwSetterSetRecordV1>, BTreeMap<(crate::Hash64, crate::Hash64), crate::palw_improve_material_v1::PalwDatasetRecordV1>, BTreeMap<(crate::Hash64, crate::Hash64), crate::palw_improve_material_v1::PalwTeachingArtifactRecordV1>, BTreeMap<crate::Hash64, crate::palw_improve_material_v1::PalwTeacherLicenceRecordV1>, BTreeMap<crate::Hash64, crate::palw_improve_composite_v1::PalwTirCompositeRefV1>, BTreeMap<crate::palw_improve_eval_v1::PalwEvalJobKeyV1, crate::palw_improve_eval_v1::PalwEvalJobStateV1>, Option<misaka_palw_panel::PermissionlessPanelStateV1>, BTreeMap<crate::Hash64, crate::palw_legacy_public_filer_v1::PalwDisputeClaimV1>, BTreeMap<crate::Hash64, Vec<u8>>, BTreeMap<crate::Hash64, crate::palw_class_semantics_v1::PalwClassSemanticsV1>, BTreeMap<crate::Hash64, crate::palw_class_semantics_v1::PalwClassSemanticsV1>, BTreeMap<
        crate::palw_legacy_answer_chunks_v2::PalwLegacyAnswerKeyV2,
        crate::palw_legacy_answer_chunks_v2::PalwPendingLegacyAnswerV2,
    >, BTreeMap<crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1, crate::palw_staged_court_state_v1::PalwStagedCourtRowV1>, BTreeMap<crate::palw_terminal_carriage_v1::PalwTerminalKeyV1, crate::palw_terminal_carriage_v1::PalwPendingTerminalV1>, BTreeSet<(crate::palw_state_v2::PalwBondKeyV2, u64, crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1)>, BTreeSet<(u64, crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1)>, BTreeSet<(crate::palw_state_v2::PalwBondKeyV2, crate::Hash64)>, BTreeSet<(crate::palw_state_v2::PalwBondKeyV2, crate::Hash64)>, BTreeSet<(crate::palw_state_v2::PalwBondKeyV2, u64, crate::Hash64, crate::palw_state_v2::PalwBondKeyV2)>, BTreeSet<(crate::palw_state_v2::PalwBondKeyV2, u64, crate::Hash64, crate::palw_state_v2::PalwBondKeyV2)>, BTreeSet<(crate::palw_state_v2::PalwBondKeyV2, u64, crate::Hash64, crate::palw_state_v2::PalwBondKeyV2)>, BTreeSet<(crate::palw_state_v2::PalwBondKeyV2, u64, crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, crate::Hash64)>, BTreeSet<(u64, crate::Hash64)>, BTreeMap<crate::Hash64, BTreeSet<crate::Hash64>>, BTreeSet<(u64, crate::Hash64, u64)>, u32, u64, BTreeSet<(crate::Hash64, crate::Hash64, u64, crate::Hash64)>, BTreeSet<(crate::Hash64, crate::Hash64, u64, crate::Hash64)>, BTreeSet<(u64, crate::Hash64, crate::Hash64)>, BTreeSet<(crate::Hash64, u64, u8, crate::Hash64)>, BTreeSet<(u64, crate::Hash64)>, BTreeSet<(u64, crate::Hash64)>, BTreeMap<crate::Hash64, crate::palw_improve_eval_v1::PalwEvalJobKeyV1>, BTreeSet<(u64, crate::Hash64)>, BTreeSet<(u64, crate::Hash64)>, BTreeMap<crate::Hash64, crate::Hash64>, BTreeMap<crate::Hash64, u32>, BTreeSet<(u64, crate::Hash64)>, BTreeSet<(u64, (crate::Hash64, crate::palw_state_v2::PalwCourtSideV1))>, BTreeSet<(crate::palw_state_v2::PalwBondKeyV2, crate::Hash64)>, BTreeMap<crate::palw_state_v2::PalwBondKeyV2, u32>, BTreeSet<(u64, crate::Hash64)>, BTreeSet<(u64, crate::Hash64)>, BTreeSet<(u64, crate::Hash64)>, BTreeSet<(u64, crate::Hash64)>, BTreeSet<(crate::palw_state_v2::PalwBondKeyV2, bool, u64, crate::Hash64)>, BTreeSet<(u64, crate::Hash64, crate::palw_state_v2::PalwBondKeyV2)>, BTreeSet<(crate::palw_state_v2::PalwBondKeyV2, crate::Hash64)>, BTreeSet<(u64, crate::Hash64)>, BTreeSet<(crate::palw_state_v2::PalwBondKeyV2, crate::Hash64)>, crate::palw_weight_cap_v1::PalwBondWeightIndexV1);
impl NativeWireScanV1 for crate::palw_state_v2::PalwClaimPhaseV2 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<crate::palw_state_v2::PalwVoidReasonV2 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<crate::palw_state_v2::PalwBondKeyV2 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u128 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<Box<crate::palw_state_v2::PalwClaimPhaseV2> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_state_v2::PalwClaimRcoreV1; Option<crate::palw_economic_safety_v1::PalwLicenceDoorTagV1>, u8, bool, u32, bool, u128);
impl NativeWireScanV1 for crate::palw_state_v2::PalwClaimSourceV2 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<BTreeSet<u32> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_state_v2::PalwClaimStateV2; crate::palw_state_v2::PalwClaimSourceV2, crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, u64, u64, Option<u64>, u64, crate::BlockHash, crate::Hash64, crate::Hash64, crate::Hash64, u32, u64, u128, u128, u64, u64, Option<crate::Hash64>, crate::palw_state_v2::PalwClaimPhaseV2, u128, crate::Hash64, crate::palw_state_v2::PalwClaimRcoreV1);
wire_struct!(crate::palw_state_v2::PalwClassAdmissionCarriageV2; crate::palw_step::PalwShapeProfileV3, crate::palw_v2::PalwJobContextV2, crate::palw_state_v2::PalwBondKeyV2, Vec<u8>);
wire_struct!(crate::palw_state_v2::PalwClassLaneCertificationV2; crate::Hash64, u64);
wire_struct!(crate::palw_state_v2::PalwClassStateV2; crate::Hash64, u64, crate::palw_state_v2::PalwPwuRuleV2, crate::palw_state_v2::PalwClassStatusV2, u64, Option<crate::palw_state_v2::PalwBondKeyV2>, bool);
impl NativeWireScanV1 for crate::palw_state_v2::PalwClassStatusV2 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_state_v2::PalwClassTargetV2; u128);
wire_struct!(crate::palw_state_v2::PalwClassWalkV2; u32, u64);
wire_struct!(crate::palw_state_v2::PalwCourtCloseGroupV2; crate::palw_state_v2::PalwBondKeyV2, u8, Vec<crate::Hash64>, crate::Hash64, crate::palw_state_v2::PalwCourtVerdictV2, u64, u64, u64, crate::tx::TransactionOutpoint, u64, BTreeMap<u8, Vec<u8>>);
wire_struct!(crate::palw_state_v2::PalwCourtSessionStateV2; crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, u64, u64, crate::palw_bisect::PalwBisectLadderV1, Option<crate::palw_attn_court_v1::PalwAttnDissectPhaseV1>);
impl NativeWireScanV1 for crate::palw_state_v2::PalwCourtSideV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for crate::palw_state_v2::PalwCourtVerdictV2 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_state_v2::PalwEpochBudgetsV2; u64, BTreeMap<crate::Hash64, u64>);
wire_struct!(crate::palw_state_v2::PalwEpochCounterV2; u64, u128, u64);
wire_struct!(crate::palw_state_v2::PalwFpClaimPromptV1; u32, crate::Hash64, Vec<u32>, u64, crate::Hash64, u32);
wire_struct!(crate::palw_state_v2::PalwHeldForfeitV1; crate::palw_state_v2::PalwBondKeyV2, u64, crate::Hash64, u32, u64, Vec<u32>, u64, bool);
wire_struct!(crate::palw_state_v2::PalwPanelDutyRowV1; BTreeMap<crate::palw_state_v2::PalwBondKeyV2, u64>, u128);
wire_struct!(crate::palw_state_v2::PalwPanelSeatV2; crate::palw_state_v2::PalwBondKeyV2, crate::Hash64);
wire_struct!(crate::palw_state_v2::PalwPanelStateV2; crate::Hash64, Vec<crate::palw_state_v2::PalwPanelSeatV2>, u64);
wire_struct!(crate::palw_state_v2::PalwPayoutV2; crate::Hash64, u64);
wire_struct!(crate::palw_state_v2::PalwPendingChunksV2; u8, u64, BTreeMap<u8, Vec<u8>>);
wire_struct!(crate::palw_state_v2::PalwPendingRewardV1; u64, u64, crate::Hash64, Option<crate::palw_state_v2::PalwRewardWinnerV1>);
impl NativeWireScanV1 for crate::palw_state_v2::PalwPwuRuleV2 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_state_v2::PalwReporterCommitV1; crate::palw_state_v2::PalwBondKeyV2, u64);
wire_struct!(crate::palw_state_v2::PalwReporterCountersV1; u128, u128);
wire_struct!(crate::palw_state_v2::PalwRewardWinnerV1; u64, crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, crate::Hash64);
impl NativeWireScanV1 for crate::palw_state_v2::PalwVoidReasonV2 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            6 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            7 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            8 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            9 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            10 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            120 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            121 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            122 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            130 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            131 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            132 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_step::PalwContractionFactV1; crate::palw_step::PalwContractionSiteV1, u8);
impl NativeWireScanV1 for crate::palw_step::PalwContractionSiteV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_step::PalwShapeProfileV3; u16, crate::palw_step::PalwStepLaneV1, u16, u16, u32, u32, u16, u16, u32, u16, [u16; 4], u32, u32, u32, i64, crate::Hash64, u16, u32, u32, u16, u32, u8, u8, u8, u8, u8, u8, u32, u32, u32, u32, u32, Vec<crate::palw_step::PalwStepNodeV1>, Vec<crate::palw_step::PalwStepNodeV1>, Vec<crate::palw_step::PalwStepNodeV1>, Vec<crate::palw_step::PalwStepNodeV1>, crate::Hash64, Vec<crate::palw_step::PalwTranscendentalBindingV1>, Vec<crate::palw_step::PalwContractionFactV1>, u32, crate::Hash64);
wire_struct!(crate::palw_step::PalwStepCoordinateV1; u32, u32, u32, u32);
impl NativeWireScanV1 for crate::palw_step::PalwStepLaneV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for crate::palw_step::PalwStepNodeRoleV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_step::PalwStepNodeV1; crate::palw_step::PalwStepOpKindV1, crate::palw_step::PalwStepNodeRoleV1, String, Vec<u8>, crate::palw_step::PalwStepOutLenV1, u32, crate::Hash64, Vec<u16>);
impl NativeWireScanV1 for crate::palw_step::PalwStepOpKindV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            6 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            7 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            8 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            9 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            10 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            11 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            12 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            13 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            14 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            15 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            16 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            17 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for crate::palw_step::PalwStepOutLenV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_step::PalwTranscendentalBindingV1; crate::palw_step::PalwTranscendentalSiteV1, crate::Hash64);
impl NativeWireScanV1 for crate::palw_step::PalwTranscendentalSiteV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_step_leg::PalwCheckpointLeafV2; u16, u32, u32, crate::Hash64, u32, crate::Hash64);
wire_struct!(crate::palw_step_leg::PalwStepBindingV2; u16, crate::palw_v2::PalwJobContextV2, crate::palw_step::PalwShapeProfileV3, crate::palw_legs::PalwCheckpointProfileV1, crate::Hash64, crate::Hash64, crate::Hash64, u64, crate::Hash64, u32, crate::Hash64, crate::Hash64);
wire_struct!(crate::palw_step_leg::PalwStepOpeningV1; u64, crate::Hash64, Vec<crate::Hash64>);
wire_struct!(crate::palw_step_leg::PalwStepRangeOpeningV1; u64, Vec<crate::Hash64>, Vec<crate::Hash64>);
wire_struct!(crate::palw_step_leg::PalwStepTileLeafV1; u16, crate::palw_step::PalwStepCoordinateV1, u32, Vec<u8>);
wire_struct!(crate::palw_step_refute::PalwBase0DecodeTokensV1; Vec<Vec<i32>>, Vec<u32>);
wire_struct!(crate::palw_step_refute::PalwCheckpointKvOperandsV1; crate::palw_step_leg::PalwCheckpointLeafV2, crate::palw_step_leg::PalwStepOpeningV1, Vec<Vec<u8>>);
impl NativeWireScanV1 for crate::palw_step_refute::PalwDecodeTokenPinV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::palw_step_refute::PalwDecodeTokensV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum =
                    sum.merge(<crate::palw_step_refute::PalwBase0DecodeTokensV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum =
                    sum.merge(<crate::palw_step_refute::PalwTiledDecodeTokensV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_step_refute::PalwDecodeTokensV1; crate::palw_v2::PalwTraceSummaryV2, crate::Hash64, Vec<u32>);
wire_struct!(crate::palw_step_refute::PalwExecutionStepRefutationV1; crate::palw_step_leg::PalwStepBindingV2, crate::palw_step_leg::PalwStepOpeningV1, crate::palw_step_leg::PalwStepTileLeafV1, Vec<crate::palw_step_refute::PalwStepInputRowV1>, Vec<u32>, Option<crate::palw_step_refute::PalwDecodeTokenPinV1>, Option<crate::palw_step_refute::PalwCheckpointKvOperandsV1>);
wire_struct!(crate::palw_step_refute::PalwStepInputRowV1; Vec<crate::palw_step_leg::PalwStepTileLeafV1>, Vec<Vec<crate::Hash64>>);
wire_struct!(crate::palw_step_refute::PalwTiledDecodePinV1; u32, Vec<u32>, crate::Hash64, crate::palw_step_leg::PalwStepOpeningV1, Vec<i32>, crate::palw_step_leg::PalwStepOpeningV1, Vec<i32>, crate::palw_step_leg::PalwStepOpeningV1, u32);
wire_struct!(crate::palw_step_refute::PalwTiledDecodeTokensV1; crate::Hash64, Vec<u32>);
wire_struct!(crate::palw_task_heads_v1::PalwGenHeadBodyV1; crate::palw_gen_job_v1::PalwGenEmbeddingInputV1, u8, u32, u8);
wire_struct!(crate::palw_task_heads_v1::PalwGenHeadOffersV1; u8, u8, u32, crate::Hash64, Vec<u32>, Option<u32>, Option<u8>);
wire_struct!(crate::palw_terminal_carriage_v1::PalwPendingTerminalV1; crate::palw_terminal_carriage_v1::PalwTerminalDeclarationV1, u64, BTreeMap<u8, Vec<u8>>);
wire_struct!(crate::palw_terminal_carriage_v1::PalwTerminalDeclarationV1; u16, crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, u64, crate::palw_offence_v1::PalwOffenceKindV1, crate::palw_state_v2::PalwBondKeyV2, crate::Hash64, u32, u64, u32, Vec<crate::Hash64>, Vec<u8>);
wire_struct!(crate::palw_tir_admission_v1::PalwTirClassRecordV1; u16, crate::palw_tir_attempt_v1::PalwTirJobFactsV1, crate::Hash64, crate::Hash64, crate::Hash64, crate::Hash64, u32, u32, Vec<(u8, u16)>, std::sync::Arc<Vec<u8>>);
wire_struct!(crate::palw_tir_attempt_v1::PalwTirJobFactsV1; crate::Hash64, u32, u32, bool, bool);
wire_struct!(crate::palw_tir_class_v1::PalwTirLayoutV1; u16, u32, u32, u32, Vec<u32>, Vec<u32>);
wire_struct!(crate::palw_tir_dissect_v1::PalwTirDissectPhaseV1; crate::Hash64, u8, u64, Vec<u16>, Vec<crate::palw_tir_dissect_v1::PalwTirFoldV1>, Vec<(i128, i128)>, Vec<Vec<u32>>, u32, u32, crate::palw_tir_dissect_v1::PalwTirRangeClaimV1, crate::palw_tir_dissect_v1::PalwTirRangeClaimV1, u64, u64, u32, crate::palw_bisect::PalwBisectTurnV1, u64, Vec<crate::palw_tir_dissect_v1::PalwTirRangeClaimV1>);
impl NativeWireScanV1 for crate::palw_tir_dissect_v1::PalwTirFoldV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_tir_dissect_v1::PalwTirRangeClaimV1; Vec<Vec<i128>>);
wire_struct!(crate::palw_tir_prefix_owned_v1::PalwOwnedTirPrefixCourtV1; crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, u64, crate::palw_rendering_resources_v1::PalwRenderingClaimTermsV1, u64, u64, u64, crate::Hash64, crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, BTreeSet<crate::palw_state_v2::PalwBondKeyV2>, u64, crate::palw_tir_prefix_state_v1::PalwTirPrefixStateStreamV1, BTreeMap<crate::palw_state_v2::PalwBondKeyV2, crate::palw_tir_prefix_state_v1::PalwTirPrefixStateStreamV1>, bool, Option<crate::palw_tir_prefix_state_v1::PalwTirPrefixStateVerdictV1>);
wire_struct!(crate::palw_tir_prefix_state_v1::PalwTirPrefixStateStreamV1; crate::Hash64, crate::Hash64, crate::Hash64, u64, u64, u32, crate::Hash64, u64, u64, u128, crate::palw_rendering_stream_v1::LeafHashContinuationV1, Option<crate::palw_tir_prefix_state_v1::PalwTirPrefixStateVerdictV1>);
impl NativeWireScanV1 for crate::palw_tir_prefix_state_v1::PalwTirPrefixStateVerdictV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_tir_shard_v1::PalwTirShardClaimV1; u16, u16, bool, crate::palw_shard_licensing_v1::PalwShardLicensingProgressV1, Vec<u8>, Vec<(crate::palw_state_v2::PalwBondKeyV2, u16, crate::palw_verification_v2::PalwSegmentMaskV2)>, Vec<u32>, bool);
wire_struct!(crate::palw_tir_shard_v1::PalwTirShardPlanV1; u16, u16, u64, Vec<u16>);
wire_struct!(crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryCertificateV1; crate::Hash64, u64, crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, u64, u64);
wire_struct!(crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryOfferV1; u16, crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, u64, u64, Vec<u8>);
wire_struct!(crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryV1; crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryOfferV1, u64, u64, crate::palw_rendering_resources_v1::PalwRenderingResourcePlanV1, u64, u64, Option<crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryCertificateV1>, crate::palw_rendering_stream_v1::LeafHashContinuationV1);
wire_struct!(crate::palw_tokenizer_json_stream_v1::NumberState; u8, bool, Option<u32>, bool, u64, u64, Vec<u8>, bool, u64, bool);
wire_struct!(crate::palw_tokenizer_json_stream_v1::PalwTokenizerRenderingDigestV1; u64, crate::Hash64);
wire_struct!(crate::palw_tokenizer_owned_court_v1::PalwOwnedTokenizerCourtV1; crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, u64, crate::palw_rendering_resources_v1::PalwRenderingClaimTermsV1, u64, u64, u64, u64, crate::Hash64, crate::Hash64, u64, crate::palw_tokenizer_table_stream_v1::PalwTokenizerTableStreamV1);
impl NativeWireScanV1 for crate::palw_tokenizer_rendering_v1::PalwTokenizerRenderingRuleV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_tokenizer_table_stream_v1::PalwAuthenticatedRenderingDigestStreamV1; crate::Hash64, bool, u32, u32, crate::Hash64, crate::palw_rendering_stream_v1::LeafHashContinuationV1, crate::palw_rendering_stream_v1::LeafHashContinuationV1);
wire_struct!(crate::palw_tokenizer_table_stream_v1::PalwTokenizerTableStreamV1; crate::Hash64, u64, u64, u64, crate::palw_tokenizer_json_stream_v1::PalwTokenizerJsonStreamV1, crate::palw_tokenizer_table_stream_v1::PalwAuthenticatedRenderingDigestStreamV1);
wire_struct!(crate::palw_v2::PalwJobContextV2; u16, Vec<u8>, crate::Hash64, crate::Hash64, crate::Hash64, [u8; 32], crate::Hash64, crate::Hash64, crate::Hash64, crate::Hash64, crate::Hash64, crate::Hash64, crate::Hash64, crate::Hash64, u32, u32, u32);
impl NativeWireScanV1 for crate::palw_v2::PalwLogitsDtypeV2 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for crate::palw_v2::PalwStopReasonV2 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for crate::palw_v2::PalwTracePhaseV2 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_v2::PalwTraceSummaryV2; u32, crate::palw_v2::PalwLogitsDtypeV2, u32, u32, u32, crate::palw_v2::PalwTracePhaseV2, crate::palw_v2::PalwTracePhaseV2, crate::Hash64, crate::palw_v2::PalwStopReasonV2);
wire_struct!(crate::palw_verification_v2::PalwSegmentMaskV2; u32);
wire_struct!(crate::palw_vertex_v1::PalwVertexCountedV1; crate::palw_state_v2::PalwBondKeyV2, crate::palw_panel_v2::PalwReceiptVerdictV2, u64);
wire_struct!(crate::palw_vertex_v1::PalwVertexHeldRowV1; crate::palw_state_v2::PalwBondKeyV2, u8, u32, u32, crate::Hash64, u64, bool);
wire_struct!(crate::palw_vertex_v1::PalwVertexRoundRowV1; crate::Hash64, u64, bool);
wire_struct!(crate::palw_vertex_v1::PalwVertexStateV1; std::collections::BTreeMap<(u64, crate::palw_state_v2::PalwBondKeyV2), crate::palw_vertex_v1::PalwVertexRoundRowV1>, std::collections::BTreeMap<crate::Hash64, crate::palw_vertex_v1::PalwVertexTallyV1>, std::collections::BTreeMap<crate::Hash64, Vec<crate::palw_vertex_v1::PalwVertexHeldRowV1>>, crate::palw_mesh_v1::PalwMeshStateV1);
wire_struct!(crate::palw_vertex_v1::PalwVertexTallyV1; Vec<crate::palw_vertex_v1::PalwVertexCountedV1>);
wire_struct!(crate::palw_vesting_v1::PalwVestingCountersV1; u128, u128, u128);
wire_struct!(crate::palw_vesting_v1::PalwVestingRowV1; crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, crate::Hash64, crate::Hash64, crate::Hash64, crate::Hash64, bool, crate::Hash64, u16, crate::palw_economic_safety_v1::PalwLicenceDoorTagV1, u8, u64, u64, crate::palw_state_v2::PalwPayoutV2, Vec<(crate::palw_state_v2::PalwBondKeyV2, crate::palw_state_v2::PalwPayoutV2)>, u64, u64, u64, u64, Option<u64>);
wire_struct!(crate::palw_weight_cap_v1::PalwBondWeightIndexV1; BTreeMap<crate::palw_state_v2::PalwBondKeyV2, crate::palw_weight_cap_v1::PalwBondWeightV1>);
wire_struct!(crate::palw_weight_cap_v1::PalwBondWeightV1; u128, u128);
wire_struct!(crate::palw_window_class_v37::NativeWindowClassV37; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u32, u32);
wire_struct!(crate::palw_window_class_v37::NativeWindowJobBindingV37; u16, [u8;64], [u8;64], [u8;64], u64);
wire_struct!(crate::palw_work_slice_v2::PalwExecJobUseV1; crate::Hash64, crate::Hash64, crate::Hash64, crate::Hash64, crate::Hash64, u64, u64);
wire_struct!(crate::palw_work_slice_v2::PalwExecOriginV1; crate::Hash64, u64);
impl NativeWireScanV1 for crate::palw_work_slice_v2::PalwExecV2RewardFundV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_work_slice_v2::PalwExecV2RewardRowV1; u64, u64, u64, std::collections::BTreeMap<(crate::palw_state_v2::PalwBondKeyV2, crate::palw_work_slice_v2::PalwExecV2RewardFundV1), crate::palw_state_v2::PalwPayoutV2>);
wire_struct!(crate::palw_work_slice_v2::PalwExecV2StateV1; std::collections::BTreeMap<crate::Hash64, crate::palw_work_slice_v2::PalwWorkRootV2>, std::collections::BTreeMap<(crate::Hash64, u32), crate::palw_work_slice_v2::PalwWorkSliceRowV2>, std::collections::BTreeMap<crate::Hash64, crate::Hash64>, std::collections::BTreeMap<crate::Hash64, u64>);
impl NativeWireScanV1 for crate::palw_work_slice_v2::PalwWorkPrefixStageV2 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for crate::palw_work_slice_v2::PalwWorkRootPhaseV2 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_work_slice_v2::PalwWorkRootV2; crate::Hash64, crate::Hash64, crate::Hash64, u32, crate::Hash64, u64, Vec<u64>, crate::Hash64, crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, Vec<crate::palw_state_v2::PalwBondKeyV2>, u128, crate::BlockHash, u64, u64, u32, crate::Hash64, u64, u64, u32, crate::palw_work_slice_v2::PalwWorkRootPhaseV2, crate::Hash64, crate::palw_work_slice_v2::PalwWorkPrefixStageV2);
wire_struct!(crate::palw_work_slice_v2::PalwWorkSliceRowV2; crate::Hash64, crate::palw_exec_v2::PalwWorkRangeV1, crate::palw_state_v2::PalwBondKeyV2, crate::BlockHash, u64, crate::Hash64, crate::Hash64, crate::Hash64, crate::Hash64, crate::palw_work_slice_v2::PalwWorkSliceStageV2);
impl NativeWireScanV1 for crate::palw_work_slice_v2::PalwWorkSliceStageV2 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_work_target_v1::PalwWorkTargetV2; u128, u128, u128, u128, u64, u64, u64);
wire_struct!(crate::tx::TransactionOutpoint; crate::TransactionId, crate::tx::TransactionIndexType);
wire_struct!(misaka_palw_challenge::approval_scope::LifetimeCounterV3; u128, u128, std::collections::BTreeSet<[u8;64]>, std::collections::BTreeSet<([u8;64], [u8;64])>);
wire_struct!(misaka_palw_challenge::approval_scope::LifetimeReviewScopeV3; u16, [u8;64], [u8;64], u64, u64, Vec<misaka_palw_challenge::policy::ReviewedTupleV2>, u128, u128, u128, [u8;64], [u8;64], [u8;64]);
wire_struct!(misaka_palw_challenge::approval_scope::SourceEconomicObservationV3; [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u64, u64, u64, u64, u64, u64, u64, u64, [u8;64]);
wire_struct!(misaka_palw_challenge::approval_scope::SourceIndependenceObservationV3; [u8;64], u64, u64, std::collections::BTreeMap<[u8;64], [u8;64]>, std::collections::BTreeSet<[u8;64]>, std::collections::BTreeSet<[u8;64]>, std::collections::BTreeSet<[u8;64]>, [u8;64]);
wire_struct!(misaka_palw_challenge::beacon::BeaconContextV1; [u8;64], [u8;64], misaka_palw_challenge::policy::PostCommitChallengePolicyV1, misaka_palw_challenge::subject::SubjectKindV1, [u8;64], u64, u64, BTreeSet<[u8;64]>, BTreeSet<[u8;64]>, misaka_palw_challenge::subject::RootV1);
wire_struct!(misaka_palw_challenge::conformance::BeaconConformanceEvidenceV1; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u64, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], misaka_palw_challenge::subject::RootV1, u64, u64, u64, Vec<String>, Vec<String>, [u8;64], [u8;64], u16, misaka_palw_challenge::conformance::ConformanceStatusV1, [u8;64]);
wire_struct!(misaka_palw_challenge::conformance::ConformanceCommitmentV1; u16, [u8;64], [u8;64], misaka_palw_challenge::subject::SubjectKindV1, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], misaka_palw_challenge::subject::RootV1, misaka_palw_challenge::subject::RootV1, [u8;64], [u8;64], misaka_palw_challenge::subject::RootV1, [u8;64], [u8;64], misaka_palw_challenge::subject::RootV1, misaka_palw_challenge::subject::RootV1, [u8;64], Option<[u8;64]>, Option<u64>);
impl NativeWireScanV1 for misaka_palw_challenge::conformance::ConformanceStatusV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for misaka_palw_challenge::lifecycle::OnboardingFailureV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            6 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            7 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            8 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            9 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_challenge::lifecycle::OnboardingRecordV1; misaka_palw_challenge::lifecycle::OnboardingStateV1, Option<misaka_palw_challenge::lifecycle::OnboardingFailureV1>, Option<[u8;64]>, Option<[u8;64]>, Option<[u8;64]>, Option<[u8;64]>, Option<[u8;64]>, u32, u32, u32);
impl NativeWireScanV1 for misaka_palw_challenge::lifecycle::OnboardingStateV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            6 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            7 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_challenge::policy::ApprovedTupleV1; [u8;64], [u8;64], [u8;64], u32, Vec<misaka_palw_challenge::soundness::RelationSoundnessV1>, u128, u32, u128);
impl NativeWireScanV1 for misaka_palw_challenge::policy::InteractiveModeV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_challenge::policy::PostCommitChallengePolicyV1; u16, [u8;64], u64, u32, u64, [u8;64], [u8;64], u64, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u32, [u8;64], u16, [u8;64], u32, [u8;64], [u8;64], misaka_palw_challenge::policy::InteractiveModeV1, [u8;64], [u8;64], [u8;64]);
impl NativeWireScanV1 for misaka_palw_challenge::policy::ReviewedFailureBoundV2 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_challenge::policy::ReviewedTupleV2; misaka_palw_challenge::policy::PostCommitChallengePolicyV1, misaka_palw_challenge::policy::ApprovedTupleV1, misaka_palw_challenge::policy::ReviewedFailureBoundV2, misaka_palw_challenge::policy::ReviewedFailureBoundV2, misaka_palw_challenge::policy::ReviewedFailureBoundV2, [u8;64]);
impl NativeWireScanV1 for misaka_palw_challenge::soundness::RelationSoundnessV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_challenge::subject::ChallengeSubjectV1; [u8;64], [u8;64], [u8;64], misaka_palw_challenge::subject::SubjectKindV1, [u8;64], misaka_palw_challenge::subject::RootV1, misaka_palw_challenge::subject::RootV1, misaka_palw_challenge::subject::RootV1, misaka_palw_challenge::subject::RootV1, misaka_palw_challenge::subject::RootV1, misaka_palw_challenge::subject::RootV1, misaka_palw_challenge::subject::RootV1, misaka_palw_challenge::subject::RootV1, misaka_palw_challenge::subject::RootV1, [u8;64]);
impl NativeWireScanV1 for misaka_palw_challenge::subject::RootV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for misaka_palw_challenge::subject::SubjectKindV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            6 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_gen::output::OutputSpecV1; u8, Vec<u32>, Vec<u8>);
wire_struct!(misaka_palw_kernel::beacon_bootstrap_v1::BootstrapExactRoundV1; [u8;64], [u8;64], u32);
wire_struct!(misaka_palw_kernel::beacon_source::BeaconSourceInputsV1; Option<u64>, BTreeMap<[u8;64], misaka_palw_kernel::beacon_source::BeaconSourceTargetV1>);
impl NativeWireScanV1 for misaka_palw_kernel::beacon_source::BeaconSourceOutcomeV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_kernel::beacon_source::BeaconSourceRowV1; [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u64, u64, misaka_palw_kernel::beacon_source::BeaconSourceTermsV1, u64, Option<([u8;64], u64)>, Option<misaka_palw_kernel::beacon_source::BeaconSourceOutcomeV1>);
wire_struct!(misaka_palw_kernel::beacon_source::BeaconSourceTargetV1; misaka_palw_challenge::beacon::BeaconContextV1, misaka_palw_kernel::beacon_source::BeaconSourceTermsV1);
wire_struct!(misaka_palw_kernel::beacon_source::BeaconSourceTermsV1; [u8;64], u64, u64, u64, u64, u64, u64, u32, u32);
wire_struct!(misaka_palw_kernel::beacon_source_fee::BeaconSourceFeeFundingV1; misaka_palw_kernel::beacon_source::BeaconSourceTargetV1, misaka_palw_kernel::beacon_source_fee::BeaconSourceFeeTermsV1, u64, u64, u32, u64, u64, bool);
wire_struct!(misaka_palw_kernel::beacon_source_fee::BeaconSourceFeeInputsV1; Option<u64>, BTreeMap<[u8;64], misaka_palw_kernel::beacon_source_fee::BeaconSourceFeeTargetV1>);
impl NativeWireScanV1 for misaka_palw_kernel::beacon_source_fee::BeaconSourceFeeOutcomeV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_kernel::beacon_source_fee::BeaconSourceFeePromiseV1; [u8;64], u64, u64, Option<misaka_palw_kernel::beacon_source_fee::BeaconSourceFeeOutcomeV1>);
wire_struct!(misaka_palw_kernel::beacon_source_fee::BeaconSourceFeeTargetV1; misaka_palw_kernel::beacon_source::BeaconSourceTargetV1, [u8;64], u64, u32);
wire_struct!(misaka_palw_kernel::beacon_source_fee::BeaconSourceFeeTermsV1; misaka_palw_kernel::beacon_source::BeaconSourceTermsV1, [u8;64], u64, u32);
wire_struct!(misaka_palw_kernel::claim_interactive_v3::CompiledClaimProofPlanV3; u16, [u8;64], misaka_palw_kernel::claim_paid_v2::PaidClaimTargetV2, misaka_palw_challenge::policy::InteractiveModeV1, u32, u64, u64);
impl NativeWireScanV1 for misaka_palw_kernel::claim_paid_v2::PaidClaimCheckingV2 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_kernel::claim_paid_v2::PaidClaimClassV2; u16, [u8;64], [u8;64], misaka_palw_kernel::claim_paid_v2::PaidClaimTargetV2, misaka_palw_kernel::claim_paid_v2::PaidClaimCheckingV2, [u8;64], misaka_palw_challenge::policy::PostCommitChallengePolicyV1, u64, u64, u64, u64, u64, u32, u64, Option<[u8;64]>, Option<misaka_palw_kernel::claim_interactive_v3::CompiledClaimProofPlanV3>);
wire_struct!(misaka_palw_kernel::claim_paid_v2::PaidClaimConsumerV2; misaka_palw_kernel::claim_paid_v2::PaidClaimLifetimeV2, bool);
wire_struct!(misaka_palw_kernel::claim_paid_v2::PaidClaimEnrollmentV2; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], misaka_palw_kernel::claim_paid_v2::PaidClaimLifetimeV2, u64, [u8;64], u32, [u8;64]);
wire_struct!(misaka_palw_kernel::claim_paid_v2::PaidClaimJobV2; u16, [u8;64], [u8;64], [u8;64], [u8;64], u64, u32);
wire_struct!(misaka_palw_kernel::claim_paid_v2::PaidClaimLifetimeV2; u64, u64, u64, u64, u64);
wire_struct!(misaka_palw_kernel::claim_paid_v2::PaidClaimReceiptV2; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u64, bool, u32, [u8;64]);
impl NativeWireScanV1 for misaka_palw_kernel::claim_paid_v2::PaidClaimTargetV2 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_kernel::court_scope::CourtScopePolicyV1; u64);
wire_struct!(misaka_palw_kernel::court_scope::CourtScopeRowV1; Vec<([u8;64], Vec<(u8, u32)>)>, Vec<([u8;64], u32)>);
wire_struct!(misaka_palw_kernel::descriptor::FamilySupportV1; misaka_palw_kernel::family::ConstraintFamilyV1, misaka_palw_kernel::family::CheckerIdV1, misaka_palw_kernel::family::CourtIdV1);
wire_struct!(misaka_palw_kernel::descriptor::KernelDescriptorV1; u32, u16, [u8;64], u32, [u8;64], u32, u32, u32, u32, u32, u32, u32, u32, u32, Vec<misaka_palw_kernel::descriptor::FamilySupportV1>, misaka_palw_kernel::descriptor::SoundnessPolicyV1, misaka_palw_kernel::descriptor::ResourceLimitsV1);
wire_struct!(misaka_palw_kernel::descriptor::KernelScheduleV1; Vec<([u8;64], misaka_palw_kernel::descriptor::KernelStatusV1)>);
impl NativeWireScanV1 for misaka_palw_kernel::descriptor::KernelStatusV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_kernel::descriptor::ResourceLimitsV1; u32, u64, u32, u128, u128, u64, u64);
wire_struct!(misaka_palw_kernel::descriptor::SoundnessPolicyV1; u16, u8, u16);
wire_struct!(misaka_palw_kernel::econ_contracts_v1::HorizonExtensionTermsV1; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u64, u64, u64, u64, [u8;64]);
wire_struct!(misaka_palw_kernel::econ_contracts_v1::MinimumPriceContractTermsV1; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u64, u64, u64, u64);
impl NativeWireScanV1 for misaka_palw_kernel::econ_contracts_v1::ModelVerificationVerdictV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_kernel::evidence::EvidenceHeaderV1; [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64]);
wire_struct!(misaka_palw_kernel::evidence::SegmentV1; u32, u32, [u8;64], [u8;64]);
wire_struct!(misaka_palw_kernel::evidence::SuiteParamsV1; [u8;64], u32, u32, u8, u16, u16);
wire_struct!(misaka_palw_kernel::evidence::VerificationEvidenceV1; u16, misaka_palw_kernel::evidence::EvidenceHeaderV1, [u8;64], u32, [u8;64], [u8;64], [u8;64], [u8;64], Vec<misaka_palw_kernel::evidence::SegmentV1>, misaka_palw_kernel::evidence::SuiteParamsV1);
wire_struct!(misaka_palw_kernel::exec_reward::ExecRewardHoldV1; [u8;64], u64);
impl NativeWireScanV1 for misaka_palw_kernel::family::CheckerIdV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for misaka_palw_kernel::family::ConstraintFamilyV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            6 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            7 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for misaka_palw_kernel::family::CourtIdV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_kernel::flat_candidate7::FlatCandidatePlanV7; u16, [u8;64], [u8;64], [u8;64], [u8;64], u32, Option<(u16, u32)>, [u8;64], [u8;64]);
wire_struct!(misaka_palw_kernel::flat_candidate7::FlatCandidateViewV7; misaka_palw_kernel::flat_candidate7::FlatCandidatePlanV7, misaka_palw_tir::program::TirProgramV1, Option<misaka_palw_kernel::seg_encoder::EncoderBindingV1>, misaka_palw_kernel::flat_scope7::FlatResourceScopeV7, misaka_palw_kernel::flat_replay7::FlatReplayEnvelopeV7);
wire_struct!(misaka_palw_kernel::flat_candidate9::FlatCandidatePlanV9; u16, [u8;64], [u8;64], [u8;64], [u8;64], u32, Option<(u16, u32)>, [u8;64], [u8;64], [u8;64], u64, u32, u64);
wire_struct!(misaka_palw_kernel::flat_candidate9::FlatCandidateViewV9; misaka_palw_kernel::flat_candidate9::FlatCandidatePlanV9, misaka_palw_tir::program::TirProgramV1, Option<misaka_palw_kernel::seg_encoder::EncoderBindingV1>, misaka_palw_kernel::flat_scope9::FlatResourceScopeV9, misaka_palw_kernel::flat_replay9::FlatReplayEnvelopeV9, misaka_palw_kernel::flat_resources10::FlatResourceVectorV10);
wire_struct!(misaka_palw_kernel::flat_execution11::ExecutionClassRecordV11; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u32, bool, [u8;64], [u8;64], [u8;64], misaka_palw_kernel::flat_execution11::NativeSourceCourtBoundV11, [u8;64], [u8;64], misaka_palw_kernel::flat_execution11::ExecutionHorizonsV11, u64, u64, u16);
wire_struct!(misaka_palw_kernel::flat_execution11::ExecutionHorizonsV11; u64, u64, u64, u64, u64, u64, u64);
wire_struct!(misaka_palw_kernel::flat_execution11::ExecutionJobInputV11; u16, [u8;64], u32, u32, misaka_palw_kernel::flat_lengths7::FlatJobLengthsV7, u32);
wire_struct!(misaka_palw_kernel::flat_execution11::ExecutionPolicyV11; u16, bool, [u8;64], [u8;64], misaka_palw_kernel::flat_execution11::ExecutionHorizonsV11, u64, u64, u64, u64, u16);
impl NativeWireScanV1 for misaka_palw_kernel::flat_execution11::ExecutionTerminalV11 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            6 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_kernel::flat_execution11::NativeExecutionPreparationRecordV11; u16, [u8;64], misaka_palw_kernel::flat_execution11::ExecutionJobInputV11, [u8;64], [u8;64], [u8;64], u64, [u8;64]);
wire_struct!(misaka_palw_kernel::flat_execution11::NativeSourceBindingV11; [u8;64], [u8;64], [u8;64], [u8;64], u64, u64, bool);
wire_struct!(misaka_palw_kernel::flat_execution11::NativeSourceCourtBoundV11; u64, u64, u64);
wire_struct!(misaka_palw_kernel::flat_lengths7::FlatJobLengthsV7; u32, u32, u32);
wire_struct!(misaka_palw_kernel::flat_quota11::FragmentProgressV11; u32, u64, [u8; 64], u64);
wire_struct!(misaka_palw_kernel::flat_quota11::ProtectedSealKeyV11; [u8;64], [u8;64], [u8;64], [u8;64], u64);
wire_struct!(misaka_palw_kernel::flat_quota11::ProtectedSealSummaryV11; misaka_palw_kernel::flat_quota11::ProtectedSealKeyV11, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u64, u64, u64, u32, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, [u32; 3], misaka_palw_kernel::flat_quota11::FragmentProgressV11);
wire_struct!(misaka_palw_kernel::flat_quota11::QuotaActorRowV11; [u8;64], u64, bool, misaka_palw_kernel::flat_quota11::QuotaIntervalSummaryV11, u32);
wire_struct!(misaka_palw_kernel::flat_quota11::QuotaGlobalRowV11; [u8;64], misaka_palw_kernel::flat_quota11::TrustedEpochV11, misaka_palw_kernel::flat_quota11::QuotaIntervalSummaryV11, misaka_palw_kernel::flat_quota11::QuotaIntervalSummaryV11, u32, u32, u32);
wire_struct!(misaka_palw_kernel::flat_quota11::QuotaIntervalSummaryV11; misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11);
wire_struct!(misaka_palw_kernel::flat_quota11::QuotaPlanRecordV11; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u64, u64, misaka_palw_kernel::flat_execution11::NativeSourceCourtBoundV11, u32, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, [misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11; misaka_palw_kernel::flat_quota11::FUTURE_ACTION_KINDS_V11], [u64; misaka_palw_kernel::flat_quota11::FUTURE_ACTION_KINDS_V11]);
wire_struct!(misaka_palw_kernel::flat_quota11::QuotaPolicyV11; u16, bool, [u8;64], [u8;64], u64, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, u32, u32, [u32; misaka_palw_kernel::flat_quota11::FUTURE_ACTION_KINDS_V11]);
wire_struct!(misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11; [u128; misaka_palw_kernel::flat_quota11::QUOTA_AXES_V11]);
wire_struct!(misaka_palw_kernel::flat_quota11::QuotaSubjectSummaryV11; misaka_palw_kernel::flat_quota11::SubjectBindingV11, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, [u32; misaka_palw_kernel::flat_quota11::FUTURE_ACTION_KINDS_V11], bool, u32);
wire_struct!(misaka_palw_kernel::flat_quota11::SubjectBindingV11; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u32, [u8;64], [u8;64], u64, u64);
wire_struct!(misaka_palw_kernel::flat_quota11::TrustedEpochV11; [u8;64], u64, u64);
wire_struct!(misaka_palw_kernel::flat_quota11::discovery13::DiscoveryPolicyV13; u16, [u8;64], u32, u32);
wire_struct!(misaka_palw_kernel::flat_quota11::discovery13::DiscoveryRowV13; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u64, [u8;64], u64, u64, u64, u32, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11);
wire_struct!(misaka_palw_kernel::flat_quota11::source16::SourceObligationV16; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u64, u64, u32, u32, u32, bool, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11);
wire_struct!(misaka_palw_kernel::flat_quota11::source16::SourcePlanRecordV16; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u32, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11);
wire_struct!(misaka_palw_kernel::flat_quota11::source16::opening::HeaderV16; u8, u8, [u64;4], [u8;64], [u8;64]);
impl NativeWireScanV1 for misaka_palw_kernel::flat_quota11::stage14::StageAdjudicationKindV14 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_kernel::flat_quota11::stage14::StageSettlementV14; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], misaka_palw_kernel::flat_quota11::stage14::StageTerminalV14, misaka_palw_kernel::flat_quota11::stage14::StageAdjudicationKindV14, u64, u64, u64, u64, [u8;64], u64, u64, u64);
impl NativeWireScanV1 for misaka_palw_kernel::flat_quota11::stage14::StageTerminalV14 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_kernel::flat_quota11::stage14::bounded28::StageSettlementV28; [u8;64], misaka_palw_kernel::flat_quota11::stage14::bounded28::StageTerminalV28, Option<[u8;64]>, [u8;64], u64, u64, u64, u64, u64, u64, u64);
impl NativeWireScanV1 for misaka_palw_kernel::flat_quota11::stage14::bounded28::StageTerminalV28 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_kernel::flat_quota11::stage14::source26::StageSourceCallerRowV26; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u64, u64, u32, u32, bool, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11);
wire_struct!(misaka_palw_kernel::flat_quota11::stage14::source26::StageSourcePlanRecordV26; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u32, u32, u64, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11);
wire_struct!(misaka_palw_kernel::flat_quota11::trace17::TraceSupplementV17; u16, u8, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u64, u64, u32, u32, bool, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11, misaka_palw_kernel::flat_quota11::QuotaQuantitiesV11);
wire_struct!(misaka_palw_kernel::flat_replay6::FlatReplayEnvelopeV6; u16, [u8;64], u32, u128, u128, u128, u128, u128, u128, u128, u128);
wire_struct!(misaka_palw_kernel::flat_replay7::FlatReplayEnvelopeV7; u16, [u8;64], misaka_palw_kernel::flat_replay6::FlatReplayEnvelopeV6, u128, u128, u128, u128);
wire_struct!(misaka_palw_kernel::flat_replay9::FlatReplayEnvelopeV9; u16, [u8;64], misaka_palw_kernel::flat_replay6::FlatReplayEnvelopeV6, u128, u128, u128, u128, u128, u128);
wire_struct!(misaka_palw_kernel::flat_resources10::FlatResourceVectorV10; u16, [u8;64], [u8;64], [u8;64], u32, Option<(u16, u32)>, Vec<misaka_palw_kernel::flat_resources10::PrimitiveResourcesV10>, misaka_palw_kernel::flat_resources10::ResourceQuantitiesV10);
wire_struct!(misaka_palw_kernel::flat_resources10::PrimitiveResourcesV10; u16, u16, u8, u128, u128, u128, u128, u128, u128, u128);
wire_struct!(misaka_palw_kernel::flat_resources10::ResourceQuantitiesV10; [u128; misaka_palw_kernel::flat_resources10::RESOURCE_DIMENSIONS_V10]);
impl NativeWireScanV1 for misaka_palw_kernel::flat_scope7::FlatRelationCourtV7 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_kernel::flat_scope7::FlatRelationScopeV7; u8, u16, u32, misaka_palw_kernel::flat_scope7::FlatRelationCourtV7, u64, u64, u64, Vec<u64>, u128, u128, u128);
wire_struct!(misaka_palw_kernel::flat_scope7::FlatResourceScopeV7; u16, [u8;64], [u8;64], [u8;64], u32, Option<(u16, u32)>, Vec<misaka_palw_kernel::flat_scope7::FlatRelationScopeV7>, u64, u128, u64, u64, u64, u64, u128, u128, u128, u128, u128, u128, u128, u128, u128, u128);
wire_struct!(misaka_palw_kernel::flat_scope9::FlatOccurrenceScopeV9; u16, u8, u16, bool, u64, u64, u64, Vec<u64>);
wire_struct!(misaka_palw_kernel::flat_scope9::FlatResourceScopeV9; u16, [u8;64], [u8;64], [u8;64], u32, Option<(u16, u32)>, Vec<misaka_palw_kernel::flat_scope9::FlatOccurrenceScopeV9>, u128, u128, u64, u64, u64, u128, u128, u128, u128);
wire_struct!(misaka_palw_kernel::flat_seal6::FlatSealRowV6; u16, [u8;64], [u8;64], [u8;64], u64, u64, u64, bool);
wire_struct!(misaka_palw_kernel::flat_seal7::FlatSealRowV7; u16, [u8;64], [u8;64], [u8;64], u64, u64, u64, bool);
wire_struct!(misaka_palw_kernel::flat_seal9::FlatSealRowV9; u16, [u8;64], [u8;64], [u8;64], u64, u64, u64, bool);
wire_struct!(misaka_palw_kernel::gate::ProsecutionBoundsV1; u128, u64, u64, u128, u128, u32, u64, u128, u128, u32, u64);
wire_struct!(misaka_palw_kernel::gate::ProsecutionPolicyV1; u64, u32, u128, u128, u128);
impl NativeWireScanV1 for misaka_palw_kernel::job::DecodeRuleV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_kernel::job::KernelClaimV1; [u8;64], [u8;64], Vec<u32>, [u8;64]);
wire_struct!(misaka_palw_kernel::job::KernelJobV1; [u8;64], Vec<u32>, u32, misaka_palw_kernel::job::DecodeRuleV1, [u8;64]);
wire_struct!(misaka_palw_kernel::ledger::BlockBudgetV1; u32, u64);
wire_struct!(misaka_palw_kernel::ledger::BondRowV1; u64, u64, Option<u64>);
impl NativeWireScanV1 for misaka_palw_kernel::ledger::ClaimBodyV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<misaka_palw_kernel::job::KernelClaimV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum
                    .merge(<misaka_palw_kernel::evidence::VerificationEvidenceV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<Vec<Vec<Vec<[u8; 64]>>> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum
                    .merge(<misaka_palw_kernel::pipeline_public::PipelineClaimV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum =
                    sum.merge(<misaka_palw_kernel::pipeline::PipelineEvidenceV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(
                    <Vec<misaka_palw_kernel::pipeline_public::StageCommitmentsV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?,
                )?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<misaka_palw_kernel::job::KernelClaimV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<misaka_palw_kernel::seg::SegmentedEvidenceV2 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<Box<misaka_palw_kernel::spec::SpecClaimBodyV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_kernel::ledger::ClaimRowV1; [u8;64], [u8;64], [u8;64], misaka_palw_kernel::ledger::ClaimBodyV1, u64, misaka_palw_kernel::lifecycle::ClaimLifecycleV1, u64, Option<u64>, bool, bool, u64);
wire_struct!(misaka_palw_kernel::ledger::ClassRowV1; misaka_palw_kernel::descriptor::KernelDescriptorV1, Vec<u8>, misaka_palw_tir::program::TirProgramV1, misaka_palw_kernel::plan::VerificationPlanV1, misaka_palw_kernel::trace::ParamCommitmentsV1, [u8;64], [u8;64], misaka_palw_kernel::gate::ProsecutionBoundsV1, u64, Option<misaka_palw_kernel::seg_scope::SegScopeReportV1>);
wire_struct!(misaka_palw_kernel::ledger::DemandRowV1; Vec<([u8;64], u64)>, u64, u64, Option<u8>);
wire_struct!(misaka_palw_kernel::ledger::ForfeitedSealRowV1; [u8;64], u64);
wire_struct!(misaka_palw_kernel::ledger::JobEscrowRowV1; [u8;64], u64, u64);
wire_struct!(misaka_palw_kernel::ledger::LedgerPolicyV1; [u8;64], [u8;64], [u8;64], u64, u64, u64, u64, u64, u64, u64, u64, u64, u16, u64, u64, u64, u64, u32, u16, u64, u64, u64, u64, misaka_palw_kernel::gate::ProsecutionPolicyV1);
wire_struct!(misaka_palw_kernel::ledger::PipelineClassRowV1; misaka_palw_kernel::descriptor::KernelDescriptorV1, Vec<u8>, misaka_palw_tir::pipeline::TirPipelineV1, Vec<Vec<u8>>, Vec<misaka_palw_tir::program_v2::TirProgramV2>, misaka_palw_kernel::pipeline::PipelinePlanV1, Vec<misaka_palw_kernel::trace::ParamCommitmentsV1>, misaka_palw_kernel::pipeline_public::PipelineClassV1, [u8;64], [u8;64], misaka_palw_kernel::gate::ProsecutionBoundsV1);
wire_struct!(misaka_palw_kernel::ledger::SealRowV1; [u8;64], u64, u64);
wire_struct!(misaka_palw_kernel::lifecycle::ClaimLifecycleV1; misaka_palw_kernel::lifecycle::LifecyclePolicyV1, misaka_palw_kernel::lifecycle::ClaimStateV1, bool, u64);
impl NativeWireScanV1 for misaka_palw_kernel::lifecycle::ClaimStateV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            6 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum =
                    sum.merge(<Box<misaka_palw_kernel::lifecycle::ClaimStateV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            7 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            8 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<bool as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            9 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            10 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_kernel::lifecycle::LifecyclePolicyV1; u64, u64);
impl NativeWireScanV1 for misaka_palw_kernel::mode::VerificationModeV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_kernel::opv::CarrierCapsV1; u64, u64, u64);
wire_struct!(misaka_palw_kernel::opv::OpvBudgetsV1; u64, u64, u64, u64, u64, u64, u64);
wire_struct!(misaka_palw_kernel::opv::OpvClaimRowV1; u64, u64, u64, u64, u64, bool);
wire_struct!(misaka_palw_kernel::opv::OpvEconomicsV1; u64, u64, u64, u16, u32, u32, u32, u16, u64);
wire_struct!(misaka_palw_kernel::opv::OpvPolicyV1; Option<u64>, misaka_palw_kernel::opv::OpvWindowV1, misaka_palw_kernel::opv::OpvBudgetsV1, misaka_palw_kernel::opv::OpvEconomicsV1, misaka_palw_kernel::opv::CarrierCapsV1);
wire_struct!(misaka_palw_kernel::opv::OpvStateV1; Option<misaka_palw_kernel::opv::OpvPolicyV1>, BTreeSet<[u8;64]>, BTreeSet<[u8;64]>, BTreeMap<[u8;64], misaka_palw_kernel::opv::OpvClaimRowV1>, BTreeSet<([u8;64], [u8;64])>);
wire_struct!(misaka_palw_kernel::opv::OpvWindowV1; u64, u64);
wire_struct!(misaka_palw_kernel::panel_lifetime_v2::PanelConsumerLifetimeTermsV2; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u64, u64, u64, u32, u64);
wire_struct!(misaka_palw_kernel::panel_lifetime_v2::PanelLifetimeAuthorizationV2; u16, [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], u64, u64, u64, u64, u32, u64, u64, u128, u64, u128);
wire_struct!(misaka_palw_kernel::pipeline::EdgeRelationV1; u8, u16, u8, bool, u64, misaka_palw_kernel::family::CheckerIdV1, misaka_palw_kernel::family::CourtIdV1);
wire_struct!(misaka_palw_kernel::pipeline::PipelineEvidenceV1; [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], [u8;64], Vec<misaka_palw_kernel::evidence::VerificationEvidenceV1>, [u8;64]);
wire_struct!(misaka_palw_kernel::pipeline::PipelinePlanV1; u16, [u8;64], [u8;64], Vec<misaka_palw_kernel::plan::VerificationPlanV1>, Vec<misaka_palw_kernel::pipeline::EdgeRelationV1>, u16);
wire_struct!(misaka_palw_kernel::pipeline_public::PipelineClaimV1; [u8;64], [u8;64], Vec<u32>, [u8;64], [u8;64]);
wire_struct!(misaka_palw_kernel::pipeline_public::PipelineClassV1; [u8;64], [u8;64], [u8;64], Vec<[u8;64]>, Option<misaka_palw_kernel::job::DecodeRuleV1>);
wire_struct!(misaka_palw_kernel::pipeline_public::PipelineJobFactsV1; Vec<u32>, Vec<u32>, u32, Vec<i64>, Vec<(u32, u32, Vec<u8>)>, Vec<u32>, Vec<u32>, Vec<(u8, u8, Vec<u32>)>, [u8; 32], u32);
wire_struct!(misaka_palw_kernel::pipeline_public::PipelineJobPostV1; [u8;64], misaka_palw_kernel::pipeline_public::PipelineJobFactsV1, u32, [u8;64]);
wire_struct!(misaka_palw_kernel::pipeline_public::StageCommitmentsV1; Vec<Vec<Vec<[u8;64]>>>, Vec<Vec<[u8;64]>>);
wire_struct!(misaka_palw_kernel::plan::MatMulDimsV1; u64, u64, u64, u64);
wire_struct!(misaka_palw_kernel::plan::PlanBudgetsV1; u128, u128, u128, u64, u64, u64);
wire_struct!(misaka_palw_kernel::plan::PlanRelationV1; u8, u16, u8, misaka_palw_kernel::family::ConstraintFamilyV1, misaka_palw_kernel::family::CheckerIdV1, misaka_palw_kernel::family::CourtIdV1, u8, u32, u8, u64, Vec<u64>, Vec<u8>, Option<misaka_palw_kernel::plan::MatMulDimsV1>);
wire_struct!(misaka_palw_kernel::plan::StateBoundaryV1; u16, u8, bool, u32);
wire_struct!(misaka_palw_kernel::plan::VerificationPlanV1; u16, [u8;64], [u8;64], u32, Vec<misaka_palw_kernel::plan::PlanRelationV1>, Vec<misaka_palw_kernel::plan::StateBoundaryV1>, u16, misaka_palw_kernel::plan::PlanBudgetsV1);
wire_struct!(misaka_palw_kernel::public::ServedPositionV1; Vec<Vec<Option<misaka_palw_kernel::public::TensorWireV1>>>, Vec<misaka_palw_kernel::public::TensorWireV1>);
wire_struct!(misaka_palw_kernel::public::TensorWireV1; u8, Vec<u64>, Vec<u8>);
wire_struct!(misaka_palw_kernel::receipt::PalwConstraintReceiptV1; [u8;64], [u8;64], [u8;64], [u8;64], u32, u16, [u8;64], [u8;64], misaka_palw_kernel::verify::ScopeV1, [u8;64], u32, [u8;64], [u8;64], u128, u32, u8, u32, u16, [u8;64], misaka_palw_kernel::receipt::ReceiptVerdictV1, [u8;64], [u8;64], u64);
impl NativeWireScanV1 for misaka_palw_kernel::receipt::ReceiptVerdictV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_kernel::rows::LedgerScalarsV1; u64, u64);
wire_struct!(misaka_palw_kernel::sealed_bounty::SealedBountyPolicyV1; u64, u64);
wire_struct!(misaka_palw_kernel::seg::SegmentedEvidenceV2; u16, misaka_palw_kernel::evidence::EvidenceHeaderV1, [u8;64], u32, u32, [u8;64], misaka_palw_kernel::evidence::SuiteParamsV1);
wire_struct!(misaka_palw_kernel::seg::TiledJobRowV1; misaka_palw_kernel::seg::TiledJobV1, Vec<u8>);
wire_struct!(misaka_palw_kernel::seg::TiledJobV1; [u8;64], u32, [u8;64], u32, misaka_palw_kernel::job::DecodeRuleV1, [u8;64]);
wire_struct!(misaka_palw_kernel::seg_da::SegProgressV1; u32, Vec<u8>, Option<u64>, Vec<([u8;64], u64)>, u64);
wire_struct!(misaka_palw_kernel::seg_encoder::EncoderBindingV1; u16, u32);
wire_struct!(misaka_palw_kernel::seg_scope::SegScopeReportV1; u64, u64, u64, u64, u64, Option<(u16, u16, u64, u64)>, u64);
wire_struct!(misaka_palw_kernel::spec::ComputationSpecV1; u16, misaka_palw_kernel::mode::VerificationModeV1, Vec<misaka_palw_kernel::spec::TypedRootV1>);
wire_struct!(misaka_palw_kernel::spec::SpecClaimBodyV1; misaka_palw_kernel::spec::SpecClaimV1, Vec<Vec<[u8;64]>>, Option<[u8;64]>);
impl NativeWireScanV1 for misaka_palw_kernel::spec::SpecClaimV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum =
                    sum.merge(<misaka_palw_kernel::spec::memory::MemoryClaimV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<misaka_palw_kernel::spec::retrieval::RetrievalClaimV1 as NativeWireScanV1>::scan_native_wire_v1(
                    reader,
                )?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<misaka_palw_kernel::spec::composite::CompositeClaimV1 as NativeWireScanV1>::scan_native_wire_v1(
                    reader,
                )?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for misaka_palw_kernel::spec::SpecClassKindV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<Box<misaka_palw_kernel::ledger::ClassRowV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<misaka_palw_kernel::spec::memory::MemoryRootV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<Vec<(u16, u16)> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum
                    .merge(<misaka_palw_kernel::spec::retrieval::RetrievalRootV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum
                    .merge(<misaka_palw_kernel::spec::composite::CompositeRootV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<Vec<misaka_palw_kernel::spec::composite::ComponentV1> as NativeWireScanV1>::scan_native_wire_v1(
                    reader,
                )?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_kernel::spec::SpecClassRowV1; misaka_palw_kernel::spec::ComputationSpecV1, misaka_palw_kernel::spec::SpecClassKindV1, misaka_palw_kernel::gate::ProsecutionBoundsV1);
impl NativeWireScanV1 for misaka_palw_kernel::spec::SpecJobV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<misaka_palw_kernel::spec::memory::MemoryJobV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum
                    .merge(<misaka_palw_kernel::spec::retrieval::RetrievalJobV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum
                    .merge(<misaka_palw_kernel::spec::composite::CompositeJobV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for misaka_palw_kernel::spec::TypedRootV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<misaka_palw_kernel::spec::WeightsRootV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<misaka_palw_kernel::spec::memory::MemoryRootV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum
                    .merge(<misaka_palw_kernel::spec::retrieval::RetrievalRootV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum
                    .merge(<misaka_palw_kernel::spec::composite::CompositeRootV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_kernel::spec::TypedStateV1; BTreeMap<[u8;64], misaka_palw_kernel::spec::SpecClassRowV1>, BTreeMap<[u8;64], misaka_palw_kernel::spec::SpecJobV1>, BTreeMap<[u8;64], misaka_palw_kernel::spec::memory::MemoryLineV1>);
wire_struct!(misaka_palw_kernel::spec::WeightsRootV1; [u8;64], Vec<u8>, misaka_palw_kernel::plan::VerificationPlanV1, misaka_palw_kernel::trace::ParamCommitmentsV1);
impl NativeWireScanV1 for misaka_palw_kernel::spec::composite::ComponentV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<Box<misaka_palw_kernel::ledger::ClassRowV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum
                    .merge(<misaka_palw_kernel::spec::retrieval::RetrievalRootV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_kernel::spec::composite::CompositeClaimV1; [u8;64], [u8;64], Vec<misaka_palw_kernel::spec::composite::StageClaimV1>);
wire_struct!(misaka_palw_kernel::spec::composite::CompositeJobV1; [u8;64], Vec<u32>, Vec<i32>, [u8;64]);
wire_struct!(misaka_palw_kernel::spec::composite::CompositeRootV1; [u8;64], Vec<misaka_palw_kernel::spec::composite::CompositeStageV1>);
wire_struct!(misaka_palw_kernel::spec::composite::CompositeStageV1; [u8;64], misaka_palw_kernel::spec::composite::StageInputV1);
impl NativeWireScanV1 for misaka_palw_kernel::spec::composite::QuerySourceV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for misaka_palw_kernel::spec::composite::StageClaimV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<Vec<u32> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum
                    .merge(<misaka_palw_kernel::evidence::VerificationEvidenceV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<Vec<Vec<Vec<[u8; 64]>>> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<Vec<i32> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<Vec<misaka_palw_kernel::spec::retrieval::RetrievedV1> as NativeWireScanV1>::scan_native_wire_v1(
                    reader,
                )?)?;
                sum = sum.merge(<Vec<Vec<u32>> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for misaka_palw_kernel::spec::composite::StageInputV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(
                    <Vec<misaka_palw_kernel::spec::composite::TokenSourceV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?,
                )?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum =
                    sum.merge(<misaka_palw_kernel::spec::composite::QuerySourceV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for misaka_palw_kernel::spec::composite::TokenSourceV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_kernel::spec::memory::AdvanceV1; [u8;64], Vec<[u8;64]>, Option<[u8;64]>, Vec<[u8;64]>, u64);
wire_struct!(misaka_palw_kernel::spec::memory::MemoryClaimV1; [u8;64], [u8;64], Vec<u32>, Vec<[u8;64]>, Vec<misaka_palw_kernel::spec::memory::StepCommitV1>, Vec<misaka_palw_kernel::public::TensorWireV1>);
wire_struct!(misaka_palw_kernel::spec::memory::MemoryJobV1; [u8;64], [u8;64], Vec<Vec<u32>>, [u8;64]);
wire_struct!(misaka_palw_kernel::spec::memory::MemoryLineV1; Vec<[u8;64]>, [u8;64], Option<[u8;64]>, Vec<misaka_palw_kernel::spec::memory::AdvanceV1>);
wire_struct!(misaka_palw_kernel::spec::memory::MemoryRootV1; [u8;64], [u8;64], Vec<misaka_palw_kernel::spec::memory::MemorySlotV1>, [u8;64], u32);
wire_struct!(misaka_palw_kernel::spec::memory::MemorySlotV1; (u16, Option<u16>), (u16, Option<u16>));
wire_struct!(misaka_palw_kernel::spec::memory::StepCommitV1; misaka_palw_kernel::evidence::VerificationEvidenceV1, Vec<Vec<Vec<[u8;64]>>>);
impl NativeWireScanV1 for misaka_palw_kernel::spec::retrieval::IndexV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_kernel::spec::retrieval::RetrievalClaimV1; [u8;64], [u8;64], Vec<misaka_palw_kernel::spec::retrieval::RetrievedV1>);
wire_struct!(misaka_palw_kernel::spec::retrieval::RetrievalJobV1; [u8;64], Vec<i32>, [u8;64]);
wire_struct!(misaka_palw_kernel::spec::retrieval::RetrievalRootV1; [u8;64], misaka_palw_kernel::spec::retrieval::SnapshotV1, misaka_palw_kernel::spec::retrieval::IndexV1, misaka_palw_kernel::spec::retrieval::RetrievalRuleV1);
impl NativeWireScanV1 for misaka_palw_kernel::spec::retrieval::RetrievalRuleV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_kernel::spec::retrieval::RetrievedV1; u64, i64, [u8;64], [u8;64]);
wire_struct!(misaka_palw_kernel::spec::retrieval::SnapshotV1; u64, u16, u16, u32, [u8;64]);
wire_struct!(misaka_palw_kernel::trace::ParamCommitmentsV1; BTreeMap<(u16, Option<u16>), [u8;64]>);
wire_struct!(misaka_palw_kernel::verifier_pay::VerifierPayPolicyV1; u64, u64, u8, u64, u64, u8);
impl NativeWireScanV1 for misaka_palw_kernel::verifier_pay::VerifierPayRowV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<Vec<([u8; 64], bool)> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<bool as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(
                    <misaka_palw_kernel::beacon_source_fee::BeaconSourceFeeFundingV1 as NativeWireScanV1>::scan_native_wire_v1(
                        reader,
                    )?,
                )?;
                Ok(sum)
            }
            6 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(
                    <misaka_palw_kernel::beacon_source_fee::BeaconSourceFeePromiseV1 as NativeWireScanV1>::scan_native_wire_v1(
                        reader,
                    )?,
                )?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for misaka_palw_kernel::verify::ScopeV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<Vec<u32> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<Vec<u32> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_panel::AdmittedClaimV1; crate::Hash64, crate::Hash64, crate::Hash64, misaka_palw_panel::BondIdV1, crate::Hash64, crate::Hash64, crate::Hash64, u64);
wire_struct!(misaka_palw_panel::BondIdV1; crate::Hash64, u32);
impl NativeWireScanV1 for misaka_palw_panel::ClaimPhaseV3 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<misaka_palw_panel::PanelBoundV3 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<misaka_palw_panel::NonFraudReasonV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            6 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<misaka_palw_panel::PanelRetiredClaimV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_panel::ClaimRecordV3; misaka_palw_panel::AdmittedClaimV1, crate::Hash64, u64, u64, u64, u32, Option<misaka_palw_panel::ClaimSealV1>, Option<misaka_palw_panel::PanelSnapshotV1>, Option<crate::Hash64>, Vec<crate::Hash64>, Vec<misaka_palw_panel::PanelBoundV3>, misaka_palw_panel::ClaimPhaseV3, Option<misaka_palw_panel::PanelStrataV1>);
wire_struct!(misaka_palw_panel::ClaimSealV1; crate::Hash64, crate::Hash64, u64, u64, u64, u32, crate::Hash64, u64, u64, u64, u64);
wire_struct!(misaka_palw_panel::EpochSourceSetV1; u64, Vec<crate::Hash64>);
impl NativeWireScanV1 for misaka_palw_panel::NonFraudReasonV1 {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_panel::PanelBoundV3; crate::Hash64, crate::Hash64, crate::Hash64, crate::Hash64, u64, u64, crate::Hash64, u64, u16, Vec<misaka_palw_panel::BondIdV1>, u64);
wire_struct!(misaka_palw_panel::PanelPolicyV1; u64, u64, u64, u64, u64, u64, u64, u16, u16, u16, u64, u32, u32, u32, u32, u32, u32, u32, u32, crate::Hash64);
wire_struct!(misaka_palw_panel::PanelPopulationArchiveV1; crate::Hash64, crate::Hash64, u64, u64, u32);
wire_struct!(misaka_palw_panel::PanelRetiredClaimV1; u64, misaka_palw_panel::PanelRetirementAuthorityV1, crate::Hash64, Option<misaka_palw_panel::PanelPopulationArchiveV1>, crate::Hash64, u32, crate::Hash64, Option<crate::Hash64>, Option<crate::Hash64>);
wire_struct!(misaka_palw_panel::PanelRetirementAuthorityV1; crate::Hash64, crate::Hash64);
wire_struct!(misaka_palw_panel::PanelSnapshotV1; crate::Hash64, crate::Hash64, u64, u64, crate::Hash64, crate::Hash64, misaka_palw_panel::BondIdV1, crate::Hash64, crate::Hash64, misaka_palw_panel::PanelCandidatesV1, Vec<u64>);
wire_struct!(misaka_palw_panel::PanelStrataV1; u16, u16, bool);
wire_struct!(misaka_palw_panel::PermissionlessPanelStateV1; u16, crate::Hash64, crate::Hash64, misaka_palw_panel::PanelPolicyV1, crate::Hash64, u64, u64, u64, BTreeMap<crate::Hash64, misaka_palw_panel::ClaimRecordV3>, BTreeSet<crate::Hash64>, BTreeMap<u64, crate::Hash64>, BTreeMap<misaka_palw_panel::BondIdV1, u128>, BTreeMap<u64, misaka_palw_panel::EpochSourceSetV1>);
wire_struct!(misaka_palw_panel::SeatCandidateV1; misaka_palw_panel::BondIdV1, crate::Hash64, crate::Hash64, u64, u64, crate::Hash64, crate::Hash64, u8);
impl NativeWireScanV1 for misaka_palw_tir::pipeline::Binding {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<misaka_palw_tir::pipeline::TokenRule as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<misaka_palw_tir::pipeline::TokenRule as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            6 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_tir::pipeline::StageDecl; String, u16, misaka_palw_tir::pipeline::TripRule, u32, Option<misaka_palw_tir::pipeline::TokenRule>, Vec<misaka_palw_tir::pipeline::Binding>);
wire_struct!(misaka_palw_tir::pipeline::TirPipelineV1; u16, Vec<misaka_palw_tir::pipeline::StageDecl>, u8);
wire_struct!(misaka_palw_tir::pipeline::TokenPad; u32, u32);
wire_struct!(misaka_palw_tir::pipeline::TokenRule; Vec<u32>, misaka_palw_tir::pipeline::TokenSource, Vec<u32>, Option<misaka_palw_tir::pipeline::TokenPad>);
impl NativeWireScanV1 for misaka_palw_tir::pipeline::TokenSource {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for misaka_palw_tir::pipeline::TripRule {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for misaka_palw_tir::prim::Cmp {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for misaka_palw_tir::prim::Prim {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<Vec<u8> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<i64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<i64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            6 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            7 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            8 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            9 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            10 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            11 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            12 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            13 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            14 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<misaka_palw_tir::prim::Rounding as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            15 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<i64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<i64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            16 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            17 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            18 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            19 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            20 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<misaka_palw_tir::prim::Cmp as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            21 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            22 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            23 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            24 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for misaka_palw_tir::prim::Rounding {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_tir::program::Block; String, Vec<misaka_palw_tir::types::TensorType>, Vec<misaka_palw_tir::program::Node>, Vec<u16>);
wire_struct!(misaka_palw_tir::program::ConstDecl; misaka_palw_tir::types::DType, Vec<u32>, Vec<u8>);
wire_struct!(misaka_palw_tir::program::Node; misaka_palw_tir::prim::Prim, Vec<misaka_palw_tir::program::Ref>, misaka_palw_tir::types::TensorType, bool);
wire_struct!(misaka_palw_tir::program::ParamDecl; String, misaka_palw_tir::types::DType, Vec<u32>, bool);
impl NativeWireScanV1 for misaka_palw_tir::program::Ref {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_tir::program::Schedule; u8, Vec<u8>, u8);
wire_struct!(misaka_palw_tir::program::StateDecl; String, misaka_palw_tir::program::StateKind, misaka_palw_tir::types::DType, Vec<u32>, bool);
impl NativeWireScanV1 for misaka_palw_tir::program::StateKind {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<i64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<i64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_tir::program::TirProgramV1; u16, [u8; 64], u32, u32, Vec<misaka_palw_tir::program::ParamDecl>, Vec<misaka_palw_tir::program::ConstDecl>, Vec<misaka_palw_tir::program::StateDecl>, Vec<misaka_palw_tir::program::Block>, misaka_palw_tir::program::Schedule, u16, [u8; 64]);
wire_struct!(misaka_palw_tir::program_v2::InputDecl; String, misaka_palw_tir::types::DType, Vec<u32>, misaka_palw_tir::program_v2::InputSource);
impl NativeWireScanV1 for misaka_palw_tir::program_v2::InputSource {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<i64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<i64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<misaka_palw_tir::program_v2::RandomDist as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<bool as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for misaka_palw_tir::program_v2::OutputDecl {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(<[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u16 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for misaka_palw_tir::program_v2::RandomDist {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_tir::program_v2::TirProgramV2; u16, [u8; 64], u32, u32, Vec<misaka_palw_tir::program_v2::InputDecl>, Vec<misaka_palw_tir::program::ParamDecl>, Vec<misaka_palw_tir::program::ConstDecl>, Vec<misaka_palw_tir::program::StateDecl>, Vec<misaka_palw_tir::program::Block>, misaka_palw_tir::program::Schedule, misaka_palw_tir::program_v2::OutputDecl);
impl NativeWireScanV1 for misaka_palw_tir::types::DType {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            4 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            5 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
impl NativeWireScanV1 for misaka_palw_tir::types::Dim {
    fn scan_native_wire_v1<R: std::io::Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            1 => {
                let mut sum = NativeWireShapeV1::default();
                Ok(sum)
            }
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(misaka_palw_tir::types::TensorType; misaka_palw_tir::types::DType, Vec<misaka_palw_tir::types::Dim>);
