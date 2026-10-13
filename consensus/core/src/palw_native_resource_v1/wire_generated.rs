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
wire_struct!(crate::palw_attn_court_v1::PalwAttnDissectPhaseV1; crate::Hash64, u8, u32, u16, u16, u16, u32, i32, i64, crate::palw_attn_dissect::PalwAttnRangeClaimV1, u64, u64, u32, crate::palw_bisect::PalwBisectTurnV1, u64, Vec<crate::palw_attn_dissect::PalwAttnRangeClaimV1>);
wire_struct!(crate::palw_attn_dissect::PalwAttnRangeClaimV1; i32, i64, Vec<i64>);
wire_struct!(crate::palw_audit_door_v1::PalwAuditStatusV1; Vec<(crate::palw_state_v2::PalwBondKeyV2, u64)>);
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
            _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown Borsh enum variant")),
        }
    }
}
wire_struct!(crate::palw_bond_budget_v1::PalwBudgetVectorV1; u64, u64, u64, u128, u64);
wire_struct!(crate::palw_bond_budget_v1::PalwCapitalAssignmentRowV1; u64, Vec<(crate::Hash64, u64)>, Option<Vec<(crate::Hash64, u64)>>, u64);
wire_struct!(crate::palw_bond_budget_v1::PalwModelBudgetRowV1; u64, u128, crate::palw_bond_budget_v1::PalwAllocationWeightV1, u64);
wire_struct!(crate::palw_bond_budget_v1::PalwReporterIncomeBindingV1; Option<crate::palw_state_v2::PalwBondKeyV2>, crate::Hash64, u64, u64, Option<u64>, Option<crate::Hash64>, Option<u64>, Option<(u128, u64)>);
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
wire_struct!(crate::palw_constraint_stream_court_v1::PalwConstraintStreamCourtV1; crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, u64, crate::Hash64, crate::Hash64, crate::Hash64, crate::Hash64, u32, crate::palw_decode_constraint_v1::PalwDecodeConstraintV1, Vec<u32>, u32, Option<u32>, Option<u32>, bool, crate::palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1, Option<crate::palw_eog_stream_v1::PalwEogStreamV1>, Option<u32>, Vec<u32>, crate::palw_decode_constraint_v1::PalwConstraintStateV1, Option<crate::palw_rendering_stream_v1::PalwRenderingStreamV1>, bool);
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
wire_struct!(crate::palw_improve_state_v1::PalwImprovementLineV1; crate::Hash64, crate::Hash64, crate::Hash64, u64, PalwImprovementLineStatusV1, u64, crate::Hash64, u32, u64, Option<u64>, u64, u64, Vec<(crate::palw_state_v2::PalwBondKeyV2, u64)>, Option<crate::palw_improve_state_v1::PalwLastPromotionV1>, Option<u64>, Option<crate::Hash64>);
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
wire_struct!(crate::palw_model_benefits_v1::PalwModelBenefitPendingV1; Vec<crate::palw_model_benefits_v1::PalwModelBenefitTierV1>, u64, u64, u64);
wire_struct!(crate::palw_model_benefits_v1::PalwModelBenefitTierV1; u64, u32, u64, u64, Vec<u8>);
wire_struct!(crate::palw_model_benefits_v1::PalwModelBenefitsV1; Vec<crate::palw_model_benefits_v1::PalwModelBenefitTierV1>, u64, u64, u64, Option<crate::palw_state_v2::PalwBondKeyV2>, Option<crate::palw_model_benefits_v1::PalwModelBenefitPendingV1>);
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
wire_struct!(crate::palw_monetary_material_v1::PalwMonetaryMaterialChallengeV1; crate::palw_state_v2::PalwBondKeyV2, crate::Hash64, u8, u32, crate::palw_state_v2::PalwBondKeyV2, u64, bool, Option<crate::palw_held_da_v1::PalwHeldMissingV1>, Option<Box<crate::palw_step_leg::PalwStepBindingV2>>, Option<u32>);
wire_struct!(crate::palw_monetary_material_v1::PalwMonetaryMaterialPaymentObservationV1; crate::Hash64, crate::Hash64, crate::Hash64, u64, u64, crate::Hash64, u64, crate::Hash64, Option<crate::Hash64>);
wire_struct!(crate::palw_monetary_material_v1::PalwMonetaryMaterialProviderV1; crate::palw_state_v2::PalwBondKeyV2, u64, u64, bool, crate::Hash64, crate::Hash64, u64, bool, bool);
wire_struct!(crate::palw_monetary_material_v1::PalwMonetaryMaterialRowV1; crate::palw_monetary_material_v1::PalwMonetaryMaterialTermsV1, crate::Hash64, u64, u64, bool, Vec<crate::palw_monetary_material_v1::PalwMonetaryMaterialProviderV1>, Vec<crate::palw_monetary_material_v1::PalwMonetaryMaterialChallengeV1>, Option<u64>, Option<crate::Hash64>, Option<u64>, Option<crate::palw_monetary_material_v1::PalwMonetaryMaterialPaymentObservationV1>, crate::Hash64, u64, u128, u128, crate::Hash64, std::collections::BTreeMap<crate::Hash64, crate::Hash64>);
wire_struct!(crate::palw_monetary_material_v1::PalwMonetaryMaterialTermsV1; u16, crate::Hash64, Vec<crate::Hash64>, Option<crate::Hash64>, crate::palw_state_v2::PalwBondKeyV2, crate::Hash64, crate::Hash64, u64, Option<crate::palw_material_capacity_v1::PalwMaterialManifestV1>, u64, u64, bool);
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
wire_struct!(crate::palw_prefix_family_court_v1::PalwPrefixFamilyCourtV1; crate::Hash64, crate::Hash64, crate::palw_step_leg::PalwStepBindingV2, Box<crate::palw_freeprompt_v3::PalwFreePromptJobV3>, crate::palw_class_semantics_v1::PalwClassSemanticsV1, crate::Hash64, u32, u64, u64, crate::palw_prefix_family_court_v1::FamilyV1);
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
wire_struct!(crate::palw_prefix_gdn_paged_v1::PalwGdnPagedStateV1; crate::Hash64, u32, u32, u32, u32, u32, u8, crate::Hash64, [u8; 68], crate::palw_prefix_gdn_paged_v1::PhaseV1, u32, u32, i64, i32, i64, i64, Vec<i32>, Vec<i32>);
wire_struct!(crate::palw_prefix_gdn_replay_v1::PalwPrefixGdnHeadReplayV1; crate::Hash64, crate::Hash64, crate::Hash64, u16, u32, u32, u32, u32, u32, u32, u64, [u8; 68], Vec<i32>);
wire_struct!(crate::palw_prefix_owned_court_v1::PalwOwnedPrefixCourtV1; crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, u64, crate::palw_rendering_resources_v1::PalwRenderingClaimTermsV1, u64, u64, u64, u64, u64, crate::Hash64, crate::Hash64, u64, u64, u64, u64, crate::palw_prefix_family_court_v1::PalwPrefixFamilyCourtV1);
wire_struct!(crate::palw_prefix_source_stream_v1::PalwPrefixSourceRangeV1; u32, u32, u32, u64, u64, u64);
wire_struct!(crate::palw_prefix_source_stream_v1::PalwPrefixSourceStreamV1; crate::Hash64, crate::Hash64, crate::palw_prefix_source_stream_v1::PalwPrefixSourceRangeV1, u64, Vec<i32>, Option<crate::palw_prefix_source_stream_v1::PendingV1>);
wire_struct!(crate::palw_prefix_state_stream_v1::PalwPrefixStateStreamV1; crate::Hash64, crate::palw_step_leg::PalwStepBindingV2, u32, u64, u64, u64, u64, crate::palw_rendering_stream_v1::LeafHashContinuationV1, Option<crate::palw_prefix_state_stream_v1::PalwPrefixTileStreamV1>);
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
wire_struct!(crate::palw_rendering_stream_v1::LeafHashContinuationV1; [u64; 8], u128, Vec<u8>);
wire_struct!(crate::palw_rendering_stream_v1::PalwRenderingStreamV1; crate::Hash64, crate::Hash64, u32, bool, u32, u32, crate::Hash64, crate::palw_rendering_stream_v1::LeafHashContinuationV1, Option<crate::palw_decode_constraint_v1::PalwConstraintStateV1>);
wire_struct!(crate::palw_seat_availability_v1::PalwSeatAvailEpochV1; u64, u32, u32);
wire_struct!(crate::palw_seat_availability_v1::PalwSeatAvailabilityV1; Option<u64>, Vec<crate::palw_seat_availability_v1::PalwSeatAvailEpochV1>);
wire_struct!(crate::palw_shard_licensing_v1::PalwClassShardPlanV1; u32, u64);
wire_struct!(crate::palw_shard_licensing_v1::PalwShardLicensingProgressV1; u32, Vec<u64>);
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
                sum=sum.merge(<Box<crate::palw_constraint_stream_owned_v1::PalwOwnedConstraintStreamV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                Ok(sum)
            }
            2 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(
                    <Box<crate::palw_tokenizer_owned_court_v1::PalwOwnedTokenizerCourtV1> as NativeWireScanV1>::scan_native_wire_v1(
                        reader,
                    )?,
                )?;
                Ok(sum)
            }
            3 => {
                let mut sum = NativeWireShapeV1::default();
                sum = sum.merge(
                    <Box<crate::palw_prefix_owned_court_v1::PalwOwnedPrefixCourtV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?,
                )?;
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
    >, BTreeMap<crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1, crate::palw_staged_court_state_v1::PalwStagedCourtRowV1>, BTreeMap<crate::palw_terminal_carriage_v1::PalwTerminalKeyV1, crate::palw_terminal_carriage_v1::PalwPendingTerminalV1>, BTreeSet<(crate::palw_state_v2::PalwBondKeyV2, u64, crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1)>, BTreeSet<(u64, crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1)>, BTreeSet<(crate::palw_state_v2::PalwBondKeyV2, crate::Hash64)>, BTreeSet<(u64, crate::Hash64)>, BTreeMap<crate::Hash64, BTreeSet<crate::Hash64>>, BTreeSet<(u64, crate::Hash64, u64)>, u32, u64, BTreeSet<(crate::Hash64, crate::Hash64, u64, crate::Hash64)>, BTreeSet<(crate::Hash64, crate::Hash64, u64, crate::Hash64)>, BTreeSet<(u64, crate::Hash64, crate::Hash64)>, BTreeSet<(crate::Hash64, u64, u8, crate::Hash64)>, BTreeSet<(u64, crate::Hash64)>, BTreeSet<(u64, crate::Hash64)>, BTreeMap<crate::Hash64, crate::palw_improve_eval_v1::PalwEvalJobKeyV1>, BTreeSet<(u64, crate::Hash64)>, BTreeSet<(u64, crate::Hash64)>, BTreeMap<crate::Hash64, crate::Hash64>, BTreeMap<crate::Hash64, u32>, BTreeSet<(u64, crate::Hash64)>, BTreeSet<(u64, (crate::Hash64, crate::palw_state_v2::PalwCourtSideV1))>, BTreeSet<(crate::palw_state_v2::PalwBondKeyV2, crate::Hash64)>, BTreeMap<crate::palw_state_v2::PalwBondKeyV2, u32>, BTreeSet<(u64, crate::Hash64)>, BTreeSet<(u64, crate::Hash64)>, BTreeSet<(u64, crate::Hash64)>, BTreeSet<(u64, crate::Hash64)>, BTreeSet<(crate::palw_state_v2::PalwBondKeyV2, bool, u64, crate::Hash64)>, BTreeSet<(u64, crate::Hash64, crate::palw_state_v2::PalwBondKeyV2)>, BTreeSet<(crate::palw_state_v2::PalwBondKeyV2, crate::Hash64)>, BTreeSet<(u64, crate::Hash64)>, BTreeSet<(crate::palw_state_v2::PalwBondKeyV2, crate::Hash64)>, crate::palw_weight_cap_v1::PalwBondWeightIndexV1);
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
wire_struct!(crate::palw_step_leg::PalwStepBindingV2; u16, crate::palw_v2::PalwJobContextV2, crate::palw_step::PalwShapeProfileV3, crate::palw_legs::PalwCheckpointProfileV1, crate::Hash64, crate::Hash64, crate::Hash64, u64, crate::Hash64, u32, crate::Hash64, crate::Hash64);
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
wire_struct!(crate::palw_tir_shard_v1::PalwTirShardClaimV1; u16, u16, bool, crate::palw_shard_licensing_v1::PalwShardLicensingProgressV1, Vec<u8>, Vec<(crate::palw_state_v2::PalwBondKeyV2, u16, crate::palw_verification_v2::PalwSegmentMaskV2)>, Vec<u32>, bool);
wire_struct!(crate::palw_tir_shard_v1::PalwTirShardPlanV1; u16, u16, u64, Vec<u16>);
wire_struct!(crate::palw_tokenizer_json_stream_v1::NumberState; u8, bool, Option<u32>, bool, u64, u64, Vec<u8>, bool, u64, bool);
wire_struct!(crate::palw_tokenizer_json_stream_v1::PalwTokenizerJsonStreamV1; crate::Hash64, crate::Hash64, u64, u64, u32, crate::palw_rendering_stream_v1::LeafHashContinuationV1, Vec<Frame>, crate::palw_tokenizer_json_stream_v1::Lex, crate::palw_json_key_set_v1::PalwJsonKeySetV1, u64, bool, Option<(u64, crate::Hash64)>, u64, bool, bool, bool, bool, u32, Option<Option<crate::palw_tokenizer_json_stream_v1::PalwTokenizerRenderingDigestV1>>, Option<crate::palw_tokenizer_json_stream_v1::PalwTokenizerRenderingDigestV1>);
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
wire_struct!(misaka_palw_kernel::flat_candidate7::FlatCandidateRecordV7; [u8;64], misaka_palw_kernel::flat_candidate7::FlatCandidatePlanV7, Vec<u8>, misaka_palw_kernel::trace::ParamCommitmentsV1);
wire_struct!(misaka_palw_kernel::flat_candidate9::FlatCandidatePlanV9; u16, [u8;64], [u8;64], [u8;64], [u8;64], u32, Option<(u16, u32)>, [u8;64], [u8;64], [u8;64], u64, u32, u64);
wire_struct!(misaka_palw_kernel::flat_candidate9::FlatCandidateRecordV9; [u8;64], misaka_palw_kernel::flat_candidate9::FlatCandidatePlanV9, Vec<u8>, misaka_palw_kernel::trace::ParamCommitmentsV1, Vec<u8>, Vec<u8>);
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
wire_struct!(misaka_palw_panel::PanelPolicyV1; u64, u64, u64, u64, u64, u64, u64, u16, u16, u16, u64, u32, u32, u32, u32, u32, u32, u32, u32, crate::Hash64);
wire_struct!(misaka_palw_panel::PermissionlessPanelStateV1; u16, crate::Hash64, crate::Hash64, misaka_palw_panel::PanelPolicyV1, crate::Hash64, u64, u64, u64, BTreeMap<crate::Hash64, misaka_palw_panel::ClaimRecordV3>, BTreeSet<crate::Hash64>, BTreeMap<u64, crate::Hash64>, BTreeMap<misaka_palw_panel::BondIdV1, u128>, BTreeMap<u64, misaka_palw_panel::EpochSourceSetV1>);
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
