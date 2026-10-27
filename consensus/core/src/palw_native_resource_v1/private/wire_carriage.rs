//! Generated manual carriage field/tail dispatch. Core manual decoder remains authority.
use super::*;
use crate::palw_native_resource_v1::{NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1};
pub(crate) fn scan_carriage_native_wire_v1<R: std::io::Read>(
    reader: &mut NativeWireReaderV1<R>,
) -> std::io::Result<(NativeWireShapeV1, Vec<(&'static str, u64, u64)>)> {
    let mut sum = NativeWireShapeV1::default();
    let mut counts = Vec::with_capacity(128);
    if reader.short()? != PALW_STATE_V2_VERSION {
        return Err(crate::palw_native_resource_v1::wire::wire_bad("unknown native carriage version"));
    }
    let shape=<BTreeMap<crate::palw_state_v2::PalwBondKeyV2, crate::palw_state_v2::PalwBondStateV2> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
    sum = sum.merge(shape)?;
    counts.push(("bonds", shape.outer_entries, shape.collection_entries));
    let shape = <BTreeMap<crate::palw_state_v2::PalwBondKeyV2, u128> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
    sum = sum.merge(shape)?;
    counts.push(("reserved_exposure", shape.outer_entries, shape.collection_entries));
    let shape = <BTreeMap<crate::Hash64, crate::palw_state_v2::PalwClassStateV2> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
    sum = sum.merge(shape)?;
    counts.push(("classes", shape.outer_entries, shape.collection_entries));
    let shape = <BTreeMap<crate::Hash64, crate::palw_state_v2::PalwClassTargetV2> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
    sum = sum.merge(shape)?;
    counts.push(("class_targets", shape.outer_entries, shape.collection_entries));
    let shape = <BTreeMap<crate::Hash64, u16> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
    sum = sum.merge(shape)?;
    counts.push(("class_shares", shape.outer_entries, shape.collection_entries));
    let shape = <BTreeMap<crate::palw_state_v2::PalwBondKeyV2, u128> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
    sum = sum.merge(shape)?;
    counts.push(("registration_exposure", shape.outer_entries, shape.collection_entries));
    let shape = <BTreeMap<crate::Hash64, crate::palw_state_v2::PalwClassWalkV2> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
    sum = sum.merge(shape)?;
    counts.push(("class_walks", shape.outer_entries, shape.collection_entries));
    let shape = <Option<crate::palw_state_v2::PalwEpochBudgetsV2> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
    sum = sum.merge(shape)?;
    counts.push(("epoch_budgets", shape.outer_entries, shape.collection_entries));
    let shape = <BTreeMap<crate::Hash64, crate::palw_state_v2::PalwClassTargetV2> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
    sum = sum.merge(shape)?;
    counts.push(("receipt_targets", shape.outer_entries, shape.collection_entries));
    let shape =
        <BTreeMap<crate::Hash64, crate::palw_state_v2::PalwCapabilityStateV2> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
    sum = sum.merge(shape)?;
    counts.push(("capabilities", shape.outer_entries, shape.collection_entries));
    let shape = <BTreeMap<crate::Hash64, crate::palw_state_v2::PalwClaimStateV2> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
    sum = sum.merge(shape)?;
    counts.push(("claims", shape.outer_entries, shape.collection_entries));
    let shape = <BTreeMap<crate::Hash64, crate::palw_state_v2::PalwPayoutV2> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
    sum = sum.merge(shape)?;
    counts.push(("pending_payouts", shape.outer_entries, shape.collection_entries));
    let shape =
        <BTreeMap<crate::Hash64, crate::palw_state_v2::PalwCertifiedFamilyStateV2> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
    sum = sum.merge(shape)?;
    counts.push(("certified_families", shape.outer_entries, shape.collection_entries));
    let shape =
        <BTreeMap<crate::Hash64, crate::palw_state_v2::PalwCertifiedFamilyStateV2> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
    sum = sum.merge(shape)?;
    counts.push(("fp_certified_families", shape.outer_entries, shape.collection_entries));
    let shape =
        <BTreeMap<crate::Hash64, crate::palw_state_v2::PalwClassLaneCertificationV2> as NativeWireScanV1>::scan_native_wire_v1(
            reader,
        )?;
    sum = sum.merge(shape)?;
    counts.push(("fp_certified_classes", shape.outer_entries, shape.collection_entries));
    let shape = <BTreeMap<crate::Hash64, crate::palw_state_v2::PalwPendingChunksV2> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
    sum = sum.merge(shape)?;
    counts.push(("pending_chunks", shape.outer_entries, shape.collection_entries));
    let shape=<BTreeMap<(crate::Hash64, crate::palw_state_v2::PalwCourtSideV1), crate::palw_state_v2::PalwCourtCloseGroupV2> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
    sum = sum.merge(shape)?;
    counts.push(("court_close_groups", shape.outer_entries, shape.collection_entries));
    let shape=<BTreeMap<crate::palw_derived_v1::PalwDerivedKeyV1, crate::palw_derived_v1::PalwDerivedRowV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
    sum = sum.merge(shape)?;
    counts.push(("derived_artifacts", shape.outer_entries, shape.collection_entries));
    let shape = <BTreeMap<crate::Hash64, crate::palw_state_v2::PalwPanelStateV2> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
    sum = sum.merge(shape)?;
    counts.push(("panels", shape.outer_entries, shape.collection_entries));
    let shape =
        <BTreeMap<crate::Hash64, crate::palw_state_v2::PalwCourtSessionStateV2> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
    sum = sum.merge(shape)?;
    counts.push(("court_sessions", shape.outer_entries, shape.collection_entries));
    let shape = <BTreeMap<crate::Hash64, crate::palw_state_v2::PalwEpochCounterV2> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
    sum = sum.merge(shape)?;
    counts.push(("epoch_counters", shape.outer_entries, shape.collection_entries));
    let shape = <BTreeMap<crate::Hash64, crate::palw_state_v2::PalwEpochCounterV2> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
    sum = sum.merge(shape)?;
    counts.push(("receipt_epoch_counters", shape.outer_entries, shape.collection_entries));
    let shape = <u128 as NativeWireScanV1>::scan_native_wire_v1(reader)?;
    sum = sum.merge(shape)?;
    counts.push(("safe_weight", shape.outer_entries, shape.collection_entries));
    let shape = <u128 as NativeWireScanV1>::scan_native_wire_v1(reader)?;
    sum = sum.merge(shape)?;
    counts.push(("retired_safe_weight", shape.outer_entries, shape.collection_entries));
    let shape = <u128 as NativeWireScanV1>::scan_native_wire_v1(reader)?;
    sum = sum.merge(shape)?;
    counts.push(("bounded_immature", shape.outer_entries, shape.collection_entries));
    let shape = <u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?;
    sum = sum.merge(shape)?;
    counts.push(("safe_frontier_blue_score", shape.outer_entries, shape.collection_entries));
    let shape = <crate::BlockHash as NativeWireScanV1>::scan_native_wire_v1(reader)?;
    sum = sum.merge(shape)?;
    counts.push(("safe_frontier", shape.outer_entries, shape.collection_entries));
    let shape = <Option<crate::palw_state_v2::PalwBlockContextV2> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
    sum = sum.merge(shape)?;
    counts.push(("last_point", shape.outer_entries, shape.collection_entries));
    let mut seen = [false; 256];
    while reader.remaining() != 0 {
        let tag = reader.byte()?;
        if seen[tag as usize] {
            return Err(crate::palw_native_resource_v1::wire::wire_bad("duplicate carriage tail"));
        }
        seen[tag as usize] = true;
        match tag {
            PALW_CARRIAGE_MODEL_TAIL_V1 => {
                let shape=<BTreeMap<crate::Hash64, crate::palw_model_market_v1::PalwModelMarketV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("model_markets", shape.outer_entries, shape.collection_entries));
                let shape = <BTreeMap<(crate::Hash64, crate::Hash64), u64> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("model_positions", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_LINES_TAIL_V1 => {
                let shape =
                    <BTreeMap<crate::Hash64, crate::palw_model_lines_v1::PalwModelLineV1> as NativeWireScanV1>::scan_native_wire_v1(
                        reader,
                    )?;
                sum = sum.merge(shape)?;
                counts.push(("model_lines", shape.outer_entries, shape.collection_entries));
                let shape=<BTreeMap<(crate::Hash64, u32), crate::palw_model_lines_v1::PalwModelVersionV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("model_versions", shape.outer_entries, shape.collection_entries));
                let shape=<BTreeMap<crate::Hash64, crate::palw_model_lines_v1::PalwModelProposalV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("model_proposals", shape.outer_entries, shape.collection_entries));
                let shape = <BTreeMap<
                    (crate::Hash64, u32, crate::palw_state_v2::PalwBondKeyV2),
                    crate::palw_model_lines_v1::PalwModelEvaluationV1,
                > as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("model_evaluations", shape.outer_entries, shape.collection_entries));
                let shape = <BTreeMap<crate::Hash64, crate::Hash64> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("claim_roots", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_SETTLEMENTS_TAIL_V1 => {
                let shape =
                    <BTreeMap<u32, crate::evm::model_market::PalwEvmSettlementV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("evm_settlements", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_BENEFITS_TAIL_V1 => {
                let shape=<BTreeMap<crate::Hash64, crate::palw_model_benefits_v1::PalwModelBenefitsV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("model_benefits", shape.outer_entries, shape.collection_entries));
                let shape = <BTreeMap<(crate::Hash64, crate::Hash64), u64> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("model_position_since", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_HELD_TAIL_V1 => {
                let shape =
                    <BTreeMap<crate::Hash64, crate::palw_held_da_v1::PalwHeldMissingV1> as NativeWireScanV1>::scan_native_wire_v1(
                        reader,
                    )?;
                sum = sum.merge(shape)?;
                counts.push(("held_da_missing", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_HELD_DEMANDS_TAIL_V1 => {
                let shape=<BTreeMap<crate::Hash64, BTreeMap<crate::palw_state_v2::PalwBondKeyV2, u64>> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("held_leaf_demands", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_CLASS_LADDERS_TAIL_V1 => {
                let shape = <BTreeMap<crate::Hash64, u64> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("class_step_ladders", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_PANEL_ECONOMY_TAIL_V1 => {
                let shape =
                    <BTreeMap<crate::Hash64, crate::palw_state_v2::PalwPanelDutyRowV1> as NativeWireScanV1>::scan_native_wire_v1(
                        reader,
                    )?;
                sum = sum.merge(shape)?;
                counts.push(("panel_duties", shape.outer_entries, shape.collection_entries));
                let shape = <u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("panel_reserve_sompi", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_ROUND_LANE_TAIL_V1 => {
                let shape = <u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("round_span", shape.outer_entries, shape.collection_entries));
                let shape=<BTreeMap<crate::Hash64, crate::palw_execution_lane_v1::PalwExecFinalV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("round_finals", shape.outer_entries, shape.collection_entries));
                let shape =
                    <BTreeMap<u64, crate::palw_execution_lane_v1::PalwExecScheduleV1> as NativeWireScanV1>::scan_native_wire_v1(
                        reader,
                    )?;
                sum = sum.merge(shape)?;
                counts.push(("round_schedules", shape.outer_entries, shape.collection_entries));
                let shape = <BTreeMap<(u64, u64), u16> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("round_permits_used", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_ROUND_EQUIVOCATIONS_TAIL_V1 => {
                let shape = <BTreeMap<(u64, u64), u16> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("round_equivocations", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_ROUND_SCHEDULER_TAIL_V1 => {
                let shape =
                    <BTreeMap<u64, crate::palw_execution_lane_v1::PalwExecSnapshotV1> as NativeWireScanV1>::scan_native_wire_v1(
                        reader,
                    )?;
                sum = sum.merge(shape)?;
                counts.push(("round_pending", shape.outer_entries, shape.collection_entries));
                let shape =
                    <Option<crate::palw_execution_lane_v1::PalwExecSeedAnchorV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("round_seed_anchor", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_MODEL_REGISTRY_TAIL_V1 => {
                let shape=<BTreeMap<crate::Hash64, crate::palw_model_registry_v1::PalwModelLifecycleRowV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("model_lifecycles", shape.outer_entries, shape.collection_entries));
                let shape = <BTreeMap<
                    (crate::palw_state_v2::PalwBondKeyV2, crate::Hash64),
                    crate::palw_model_registry_v1::PalwSeatReadinessRowV1,
                > as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("seat_readiness", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_CLAIM_ECONOMICS_TAIL_V1 => {
                let shape=<BTreeMap<crate::Hash64, crate::palw_economic_payout_v1::PalwClaimEconomicsV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("claim_economics", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_WORK_TARGET_SHADOW_TAIL_V1 => {
                let shape = <Option<crate::palw_work_target_v1::PalwWorkTargetV2> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("work_target_shadow", shape.outer_entries, shape.collection_entries));
                let shape = <BTreeMap<u64, BTreeMap<crate::Hash64, u128>> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("final_work", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_WORK_TARGET_TAIL_V1 => {
                let shape = <Option<crate::palw_work_target_v1::PalwWorkTargetV2> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("work_target", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_ARTIFACT_OWNERS_TAIL_V1 => {
                let shape=<BTreeMap<(crate::Hash64, crate::Hash64), crate::palw_model_lines_v1::PalwArtifactOwnerV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("artifact_owners", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_FP_DERIVED_WORK_TAIL_V1 => {
                let shape =
                    <BTreeMap<crate::Hash64, Box<crate::palw_step::PalwShapeProfileV3>> as NativeWireScanV1>::scan_native_wire_v1(
                        reader,
                    )?;
                sum = sum.merge(shape)?;
                counts.push(("fp_work_profiles", shape.outer_entries, shape.collection_entries));
                let shape = <BTreeMap<
                    (crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, crate::Hash64),
                    crate::palw_state_v2::PalwFpClaimPromptV1,
                > as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("fp_claim_prompts", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_SETTLED_FINALS_TAIL_V1 => {
                let shape = <u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("settled_attempt_finals", shape.outer_entries, shape.collection_entries));
                let shape = <Vec<u64> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("recent_anchor_daas", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_RCORE_PLUS_TAIL_V1 => {
                let shape =
                    <BTreeMap<crate::palw_state_v2::PalwBondKeyV2, Vec<u64>> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("withholding_strikes", shape.outer_entries, shape.collection_entries));
                let shape =
                    <BTreeMap<crate::Hash64, crate::palw_state_v2::PalwPendingRewardV1> as NativeWireScanV1>::scan_native_wire_v1(
                        reader,
                    )?;
                sum = sum.merge(shape)?;
                counts.push(("reward_pending", shape.outer_entries, shape.collection_entries));
                let shape =
                    <BTreeMap<crate::Hash64, crate::palw_state_v2::PalwReporterCommitV1> as NativeWireScanV1>::scan_native_wire_v1(
                        reader,
                    )?;
                sum = sum.merge(shape)?;
                counts.push(("reporter_commitments", shape.outer_entries, shape.collection_entries));
                let shape =
                    <BTreeMap<crate::Hash64, crate::palw_state_v2::PalwPayoutV2> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("reporter_rewards", shape.outer_entries, shape.collection_entries));
                let shape = <crate::palw_state_v2::PalwReporterCountersV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("reporter_counters", shape.outer_entries, shape.collection_entries));
                let shape =
                    <BTreeMap<crate::Hash64, crate::palw_vesting_v1::PalwVestingRowV1> as NativeWireScanV1>::scan_native_wire_v1(
                        reader,
                    )?;
                sum = sum.merge(shape)?;
                counts.push(("vesting", shape.outer_entries, shape.collection_entries));
                let shape = <crate::palw_vesting_v1::PalwVestingCountersV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("vesting_counters", shape.outer_entries, shape.collection_entries));
                let shape=<BTreeMap<(crate::Hash64, crate::palw_state_v2::PalwBondKeyV2), crate::palw_da_rcore_v1::PalwDaSessionV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("da_sessions", shape.outer_entries, shape.collection_entries));
                let shape =
                    <BTreeMap<crate::Hash64, crate::palw_da_rcore_v1::PalwDaClaimV1> as NativeWireScanV1>::scan_native_wire_v1(
                        reader,
                    )?;
                sum = sum.merge(shape)?;
                counts.push(("da_claims", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_ACTIVATION_POOL_TAIL_V1 => {
                let shape=<BTreeMap<crate::Hash64, crate::palw_activation_pool_v1::PalwActivationPoolV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("activation_pools", shape.outer_entries, shape.collection_entries));
                let shape =
                    <crate::palw_activation_pool_v1::PalwActivationPoolCountersV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("activation_pool_counters", shape.outer_entries, shape.collection_entries));
                let shape = <BTreeMap<(crate::Hash64, crate::Hash64), u64> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("activation_pool_scheduled", shape.outer_entries, shape.collection_entries));
                let shape =
                    <BTreeMap<(crate::Hash64, crate::palw_state_v2::PalwBondKeyV2), u64> as NativeWireScanV1>::scan_native_wire_v1(
                        reader,
                    )?;
                sum = sum.merge(shape)?;
                counts.push(("activation_readiness_landed", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_HELD_FORFEITS_TAIL_V1 => {
                let shape=<BTreeMap<(crate::Hash64, crate::Hash64), crate::palw_state_v2::PalwHeldForfeitV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("held_forfeits", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_PROBATION_MEMORY_TAIL_V1 => {
                let shape=<BTreeMap<crate::Hash64, crate::palw_model_registry_v1::PalwProbationMemoryV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("probation_memory", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_BOND_FREEZES_TAIL_V1 => {
                let shape=<BTreeMap<crate::palw_state_v2::PalwBondKeyV2, crate::palw_aggregate_liability_v1::PalwBondFreezeV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("bond_freezes", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_CAPACITY_QS_TAIL_V1 => {
                let shape =
                    <BTreeMap<crate::Hash64, crate::palw_audit_door_v1::PalwAuditStatusV1> as NativeWireScanV1>::scan_native_wire_v1(
                        reader,
                    )?;
                sum = sum.merge(shape)?;
                counts.push(("audit_receipts", shape.outer_entries, shape.collection_entries));
                let shape = <BTreeMap<crate::palw_state_v2::PalwBondKeyV2, u64> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("excluded_auditors", shape.outer_entries, shape.collection_entries));
                let shape=<BTreeMap<crate::palw_state_v2::PalwBondKeyV2, crate::palw_issuance_slots_v1::PalwIssuanceBucketV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("issuance_buckets", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_CAPACITY_N_TAIL_V1 => {
                let shape =
                    <BTreeMap<(crate::palw_state_v2::PalwBondKeyV2, crate::Hash64), u64> as NativeWireScanV1>::scan_native_wire_v1(
                        reader,
                    )?;
                sum = sum.merge(shape)?;
                counts.push(("network_demand", shape.outer_entries, shape.collection_entries));
                let shape = <BTreeMap<u64, u32> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("operator_ring", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_CAPACITY_S567_TAIL_V1 => {
                let shape = <BTreeMap<
                    crate::palw_capacity_s567_v1::PalwCapacityLedgerKeyV1,
                    crate::palw_capacity_s567_v1::PalwCapacityLedgerRowV1,
                > as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("capacity_ledger", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_TIR_CLASSES_TAIL_V1 => {
                let shape=<BTreeMap<crate::Hash64, crate::palw_tir_admission_v1::PalwTirClassRecordV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("tir_classes", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_TIR_DISSECTIONS_TAIL_V1 => {
                let shape=<BTreeMap<crate::Hash64, Box<crate::palw_tir_dissect_v1::PalwTirDissectPhaseV1>> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("tir_dissections", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_GEN_CLASSES_TAIL_V1 => {
                let shape=<BTreeMap<crate::Hash64, crate::palw_gen_class_v1::PalwGenClassRecordV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("gen_classes", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_IMPROVEMENT_LINES_TAIL_V1 => {
                let shape=<BTreeMap<crate::Hash64, crate::palw_improve_state_v1::PalwImprovementLineV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("improvement_lines", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_IMPROVEMENT_EPOCHS_TAIL_V1 => {
                let shape=<BTreeMap<(crate::Hash64, u64), crate::palw_improve_state_v1::PalwImprovementEpochV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("improvement_epochs", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_IMPROVEMENT_POLICIES_TAIL_V1 => {
                let shape=<BTreeMap<crate::Hash64, crate::palw_improve_state_v1::PalwImprovementPolicyRecordV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("improvement_policies", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_IMPROVEMENT_USAGE_TAIL_V1 => {
                let shape=<BTreeMap<crate::Hash64, crate::palw_improve_state_v1::PalwImprovementUsageV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("improvement_usage", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_IMPROVEMENT_HEADS_TAIL_V1 => {
                let shape=<BTreeMap<(crate::Hash64, u32), crate::palw_improve_state_v1::PalwLineageHeadEntryV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("improvement_heads", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_IMPROVEMENT_POOLS_TAIL_V1 => {
                let shape=<BTreeMap<crate::Hash64, crate::palw_improve_state_v1::PalwImprovementPoolV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("improvement_pools", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_IMPROVEMENT_MATERIAL_TAIL_V1 => {
                let shape=<BTreeMap<(crate::Hash64, u64), crate::palw_improve_state_v1::PalwMaterialFrontierV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("improvement_material", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_IMPROVEMENT_CANDIDATES_TAIL_V1 => {
                let shape=<BTreeMap<(crate::Hash64, u64, u32), crate::palw_improve_state_v1::PalwEpochCandidateV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("improvement_candidates", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_IMPROVEMENT_POOL_ENTRIES_TAIL_V1 => {
                let shape=<BTreeMap<(crate::Hash64, u64, u8, crate::Hash64), crate::palw_improve_state_v1::PalwPoolEntryV2> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("improvement_pool_entries", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_IMPROVEMENT_ITEMS_TAIL_V1 => {
                let shape=<BTreeMap<(crate::Hash64, u64, u32), crate::palw_improve_state_v1::PalwEvalItemV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("improvement_items", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_IMPROVEMENT_RESULTS_TAIL_V1 => {
                let shape = <BTreeMap<
                    (crate::Hash64, u64, u32, crate::palw_improve_state_v1::PalwEvalSubjectV1),
                    crate::palw_improve_state_v1::PalwEvalResultV1,
                > as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("improvement_results", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_IMPROVEMENT_GRANTS_TAIL_V1 => {
                let shape=<BTreeMap<(crate::Hash64, u64, u32), crate::palw_improve_state_v1::PalwRewardGrantV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("improvement_grants", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_IMPROVEMENT_EARNINGS_TAIL_V1 => {
                let shape = <BTreeMap<crate::palw_state_v2::PalwBondKeyV2, u64> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("improvement_earnings", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_IMPROVEMENT_CASES_TAIL_V1 => {
                let shape=<BTreeMap<(crate::Hash64, crate::Hash64), crate::palw_improve_material_v1::PalwHardCaseRecordV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("improvement_cases", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_IMPROVEMENT_OPT_INS_TAIL_V1 => {
                let shape=<BTreeMap<crate::Hash64, crate::palw_improve_material_v1::PalwDataUseOptInRecordV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("improvement_opt_ins", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_IMPROVEMENT_SETTER_SETS_TAIL_V1 => {
                let shape=<BTreeMap<(crate::Hash64, crate::Hash64), crate::palw_improve_material_v1::PalwSetterSetRecordV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("improvement_setter_sets", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_IMPROVEMENT_DATASETS_TAIL_V1 => {
                let shape=<BTreeMap<(crate::Hash64, crate::Hash64), crate::palw_improve_material_v1::PalwDatasetRecordV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("improvement_datasets", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_IMPROVEMENT_ARTIFACTS_TAIL_V1 => {
                let shape=<BTreeMap<(crate::Hash64, crate::Hash64), crate::palw_improve_material_v1::PalwTeachingArtifactRecordV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("improvement_artifacts", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_IMPROVEMENT_LICENCES_TAIL_V1 => {
                let shape=<BTreeMap<crate::Hash64, crate::palw_improve_material_v1::PalwTeacherLicenceRecordV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("improvement_licences", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_IMPROVEMENT_COMPOSITE_CLASSES_TAIL_V1 => {
                let shape=<BTreeMap<crate::Hash64, crate::palw_improve_composite_v1::PalwTirCompositeRefV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("improvement_composite_classes", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_IMPROVEMENT_EVAL_JOBS_TAIL_V1 => {
                let shape=<BTreeMap<crate::palw_improve_eval_v1::PalwEvalJobKeyV1, crate::palw_improve_eval_v1::PalwEvalJobStateV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("improvement_eval_jobs", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_CLASS_COURT_WINDOWS_TAIL_V1 => {
                let shape = <BTreeMap<crate::Hash64, u64> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("class_court_windows", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_VERTEX_TAIL_V1 => {
                let shape = <crate::palw_vertex_v1::PalwVertexStateV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("vertex", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_TIR_SHARD_TAIL_V1 => {
                let shape =
                    <BTreeMap<crate::Hash64, crate::palw_tir_shard_v1::PalwTirShardPlanV1> as NativeWireScanV1>::scan_native_wire_v1(
                        reader,
                    )?;
                sum = sum.merge(shape)?;
                counts.push(("tir_shard_plans", shape.outer_entries, shape.collection_entries));
                let shape =
                    <BTreeMap<crate::Hash64, crate::palw_tir_shard_v1::PalwTirShardClaimV1> as NativeWireScanV1>::scan_native_wire_v1(
                        reader,
                    )?;
                sum = sum.merge(shape)?;
                counts.push(("tir_shard_claims", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_FLOOR_STATE_TAIL_V1 => {
                let shape = <crate::palw_real_share_v1::PalwFloorStateV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("floor_state", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_EXEC_V2_TAIL_V1 => {
                let shape = <crate::palw_work_slice_v2::PalwExecV2StateV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("exec_v2", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_EXEC_V2_REWARDS_TAIL_V1 => {
                let shape=<BTreeMap<crate::Hash64, crate::palw_work_slice_v2::PalwExecV2RewardRowV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("exec_v2_rewards", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_EXEC_V2_ORIGINS_TAIL_V1 => {
                let shape =
                    <BTreeMap<crate::Hash64, crate::palw_work_slice_v2::PalwExecOriginV1> as NativeWireScanV1>::scan_native_wire_v1(
                        reader,
                    )?;
                sum = sum.merge(shape)?;
                counts.push(("exec_v2_origins", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_EXEC_V2_JOB_USES_TAIL_V1 => {
                let shape =
                    <BTreeMap<crate::Hash64, crate::palw_work_slice_v2::PalwExecJobUseV1> as NativeWireScanV1>::scan_native_wire_v1(
                        reader,
                    )?;
                sum = sum.merge(shape)?;
                counts.push(("exec_v2_job_uses", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_EXEC_V2_REWARD_HOLDS_TAIL_V1 => {
                let shape =
                    <BTreeMap<(crate::Hash64, crate::palw_state_v2::PalwBondKeyV2), u64> as NativeWireScanV1>::scan_native_wire_v1(
                        reader,
                    )?;
                sum = sum.merge(shape)?;
                counts.push(("exec_v2_reward_holds", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_SEAT_ROOT_READINESS_TAIL_V1 => {
                let shape = <BTreeMap<
                    (crate::palw_state_v2::PalwBondKeyV2, crate::Hash64, crate::Hash64),
                    crate::palw_model_registry_v1::PalwSeatReadinessRowV1,
                > as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("seat_root_readiness", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_PANEL_V3_TAIL_V1 => {
                let shape = <misaka_palw_panel::PermissionlessPanelStateV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("panel_v3", shape.outer_entries, shape.collection_entries));
            }
            crate::palw_kernel_route_v1::PALW_CARRIAGE_KERNEL_ROUTE_TAIL_V1 => {
                let shape = <crate::palw_kernel_route_v1::PalwKernelRouteStateV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("kernel_route", shape.outer_entries, shape.collection_entries));
            }
            crate::palw_constraint_stream_carriage_v1::PALW_CONSTRAINT_STREAM_TAIL_V1 => {
                let shape = <BTreeMap<
                    crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1,
                    crate::palw_staged_court_state_v1::PalwStagedCourtRowV1,
                > as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("staged_courts", shape.outer_entries, shape.collection_entries));
            }
            crate::palw_terminal_carriage_v1::PALW_TERMINAL_TAIL_V1 => {
                let shape = <BTreeMap<
                    crate::palw_terminal_carriage_v1::PalwTerminalKeyV1,
                    crate::palw_terminal_carriage_v1::PalwPendingTerminalV1,
                > as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("terminal_evidence", shape.outer_entries, shape.collection_entries));
            }
            crate::palw_legacy_answer_chunks_v2::PALW_CARRIAGE_LEGACY_ANSWERS_TAIL_V2 => {
                let shape = <BTreeMap<
                    crate::palw_legacy_answer_chunks_v2::PalwLegacyAnswerKeyV2,
                    crate::palw_legacy_answer_chunks_v2::PalwPendingLegacyAnswerV2,
                > as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("legacy_answers", shape.outer_entries, shape.collection_entries));
            }
            crate::palw_class_semantics_v1::PALW_CARRIAGE_CLASS_SEMANTICS_TAIL_V1 => {
                let shape=<BTreeMap<crate::Hash64, crate::palw_class_semantics_v1::PalwClassSemanticsV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("class_semantics", shape.outer_entries, shape.collection_entries));
                let shape=<BTreeMap<crate::Hash64, crate::palw_class_semantics_v1::PalwClassSemanticsV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("claim_semantics", shape.outer_entries, shape.collection_entries));
            }
            crate::palw_panel_da_disclosure_v1::PALW_CARRIAGE_PANEL_DA_DISCLOSURES_TAIL_V1 => {
                let shape=<BTreeMap<crate::Hash64, crate::palw_panel_da_disclosure_v1::PalwPanelDaDisclosureRowV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("panel_da_disclosures", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_PUBLIC_FP_JOBS_TAIL_V1 => {
                let shape = <BTreeMap<crate::Hash64, Vec<u8>> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("fp_public_jobs", shape.outer_entries, shape.collection_entries));
            }
            crate::palw_legacy_public_filer_v1::PALW_CARRIAGE_LEGACY_DISPUTES_TAIL_V1 => {
                let shape=<BTreeMap<crate::Hash64, crate::palw_legacy_public_filer_v1::PalwDisputeClaimV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("legacy_disputes", shape.outer_entries, shape.collection_entries));
            }
            crate::palw_funded_escrow_v1::PALW_CARRIAGE_FUNDED_ESCROW_TAIL_V1 => {
                let shape=<BTreeMap<crate::Hash64, crate::palw_funded_escrow_v1::PalwFundedEscrowRowV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("funded_escrows", shape.outer_entries, shape.collection_entries));
            }
            crate::palw_court_model_scope_routes_v1::readiness_disclosure::PALW_CARRIAGE_D2_READINESS_TAIL_V1 => {
                let shape=<crate::palw_court_model_scope_routes_v1::readiness_disclosure::PalwD2ReadinessRegistryV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("d2_readiness", shape.outer_entries, shape.collection_entries));
            }
            crate::palw_tir_designated_answers_v1::PALW_CARRIAGE_TIR_DESIGNATED_TAIL_V1 => {
                let shape =
                    <crate::palw_tir_designated_answers_v1::PalwTirDesignatedRegistryV1 as NativeWireScanV1>::scan_native_wire_v1(
                        reader,
                    )?;
                sum = sum.merge(shape)?;
                counts.push(("tir_designated", shape.outer_entries, shape.collection_entries));
            }
            crate::palw_bond_budget_v1::PALW_CARRIAGE_BOND_BUDGET_TAIL_V1 => {
                let shape = <crate::palw_bond_budget_v1::PalwBondBudgetStateV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("bond_budget", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_SEAT_AVAILABILITY_TAIL_V1 => {
                let shape=<BTreeMap<crate::palw_state_v2::PalwBondKeyV2, crate::palw_seat_availability_v1::PalwSeatAvailabilityV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("seat_availability", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_OBJECTIVE_OFFENCE_TAIL_V1 => {
                let shape =
                    <BTreeMap<crate::Hash64, crate::palw_offence_v1::PalwConsumedOffenceV1> as NativeWireScanV1>::scan_native_wire_v1(
                        reader,
                    )?;
                sum = sum.merge(shape)?;
                counts.push(("consumed_offences", shape.outer_entries, shape.collection_entries));
                let shape = <BTreeMap<
                    (crate::palw_state_v2::PalwBondKeyV2, crate::Hash64),
                    crate::palw_panel_var_v1::PalwSlashableLockV1,
                > as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("slashable_locks", shape.outer_entries, shape.collection_entries));
                let shape=<BTreeMap<crate::Hash64, crate::palw_panel_var_v1::PalwPanelLiabilityRecordV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("panel_liabilities", shape.outer_entries, shape.collection_entries));
            }
            PALW_CARRIAGE_SHARDS_TAIL_V1 => {
                let shape=<BTreeMap<crate::Hash64, crate::palw_shard_licensing_v1::PalwClassShardPlanV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("class_shard_plans", shape.outer_entries, shape.collection_entries));
                let shape=<BTreeMap<(crate::palw_state_v2::PalwBondKeyV2, crate::Hash64), Vec<u32>> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("bond_shards", shape.outer_entries, shape.collection_entries));
                let shape=<BTreeMap<crate::Hash64, crate::palw_shard_licensing_v1::PalwShardLicensingProgressV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?;
                sum = sum.merge(shape)?;
                counts.push(("shard_licensing", shape.outer_entries, shape.collection_entries));
            }
            _ => return Err(crate::palw_native_resource_v1::wire::wire_bad("unknown carriage tail")),
        }
    }
    Ok((sum, counts))
}

pub(crate) fn restore_index_native_shape_v1(counts: &[(&str, u64, u64)]) -> std::io::Result<NativeWireShapeV1> {
    let count = |name: &str, nested: bool| -> std::io::Result<u64> {
        let Some((_, outer, members)) = counts.iter().find(|(field, _, _)| *field == name) else { return Ok(0) };
        if nested {
            outer.checked_add(*members).ok_or_else(|| crate::palw_native_resource_v1::wire::wire_bad("restore membership overflow"))
        } else {
            Ok(*outer)
        }
    };
    fn checked_counts_v1(counts: &[u64]) -> std::io::Result<u64> {
        counts.iter().try_fold(0u64, |n, v| {
            n.checked_add(*v).ok_or_else(|| crate::palw_native_resource_v1::wire::wire_bad("restore source count overflow"))
        })
    }
    let mut sum = NativeWireShapeV1::default();
    let n = (count("exec_v2_job_uses", false)?);
    sum = sum
        .allocation(n, std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(u64, crate::Hash64), ()>>())?;
    let n = (count("exec_v2_rewards", true)?);
    sum = sum.allocation(
        n,
        std::mem::size_of::<
            crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::palw_state_v2::PalwBondKeyV2, BTreeSet<crate::Hash64>>,
        >(),
    )?;
    sum = sum.allocation(n, std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, ()>>())?;
    let n = (count("staged_courts", true)?);
    sum = sum.allocation(
        n,
        std::mem::size_of::<
            crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
                (crate::palw_state_v2::PalwBondKeyV2, u64, crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1),
                (),
            >,
        >(),
    )?;
    let n = (count("staged_courts", false)?);
    sum = sum.allocation(
        n,
        std::mem::size_of::<
            crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
                (u64, crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1),
                (),
            >,
        >(),
    )?;
    let n = (count("staged_courts", false)?);
    sum = sum.allocation(
        n,
        std::mem::size_of::<
            crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(crate::palw_state_v2::PalwBondKeyV2, crate::Hash64), ()>,
        >(),
    )?;
    let n = (count("improvement_lines", false)?);
    sum = sum
        .allocation(n, std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(u64, crate::Hash64), ()>>())?;
    let n = (count("improvement_lines", false)?);
    sum = sum.allocation(
        n,
        std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, BTreeSet<crate::Hash64>>>(),
    )?;
    sum = sum.allocation(n, std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, ()>>())?;
    let n = (count("improvement_epochs", false)?);
    sum = sum.allocation(
        n,
        std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(u64, crate::Hash64, u64), ()>>(),
    )?;
    let n = (count("improvement_eval_jobs", true)?);
    sum = sum.allocation(
        n,
        std::mem::size_of::<
            crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_improve_eval_v1::PalwEvalJobKeyV1>,
        >(),
    )?;
    let n = (count("improvement_artifacts", false)?);
    sum = sum.allocation(
        n,
        std::mem::size_of::<
            crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(crate::Hash64, crate::Hash64, u64, crate::Hash64), ()>,
        >(),
    )?;
    let n = (count("improvement_artifacts", false)?);
    sum = sum.allocation(
        n,
        std::mem::size_of::<
            crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(crate::Hash64, crate::Hash64, u64, crate::Hash64), ()>,
        >(),
    )?;
    let n = (count("improvement_artifacts", false)?);
    sum = sum.allocation(
        n,
        std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(u64, crate::Hash64, crate::Hash64), ()>>(),
    )?;
    let n = (checked_counts_v1(&[
        count("improvement_artifacts", false)?,
        count("improvement_cases", false)?,
        count("improvement_setter_sets", false)?,
    ])?);
    sum = sum.allocation(
        n,
        std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(crate::Hash64, u64, u8, crate::Hash64), ()>>(),
    )?;
    let n = (count("improvement_opt_ins", false)?);
    sum = sum
        .allocation(n, std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(u64, crate::Hash64), ()>>())?;
    let n = (count("improvement_licences", false)?);
    sum = sum
        .allocation(n, std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(u64, crate::Hash64), ()>>())?;
    let n = (count("claims", false)?);
    sum = sum
        .allocation(n, std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(u64, crate::Hash64), ()>>())?;
    let n = (count("claims", false)?);
    sum = sum
        .allocation(n, std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(u64, crate::Hash64), ()>>())?;
    let n = (count("claims", false)?);
    sum = sum.allocation(
        n,
        std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::Hash64>>(),
    )?;
    let n = (count("court_sessions", false)?);
    sum = sum.allocation(n, std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, u32>>())?;
    let n = (count("court_sessions", false)?);
    sum = sum
        .allocation(n, std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(u64, crate::Hash64), ()>>())?;
    let n = (count("court_close_groups", false)?);
    sum = sum.allocation(
        n,
        std::mem::size_of::<
            crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
                (u64, (crate::Hash64, crate::palw_state_v2::PalwCourtSideV1)),
                (),
            >,
        >(),
    )?;
    let n = (count("court_sessions", false)?);
    sum = sum.allocation(
        n,
        std::mem::size_of::<
            crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(crate::palw_state_v2::PalwBondKeyV2, crate::Hash64), ()>,
        >(),
    )?;
    let n = (count("reporter_commitments", false)?);
    sum = sum.allocation(
        n,
        std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::palw_state_v2::PalwBondKeyV2, u32>>(),
    )?;
    let n = (count("reporter_commitments", false)?);
    sum = sum
        .allocation(n, std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(u64, crate::Hash64), ()>>())?;
    let n = (count("vesting", false)?);
    sum = sum
        .allocation(n, std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(u64, crate::Hash64), ()>>())?;
    let n = (count("vesting", false)?);
    sum = sum
        .allocation(n, std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(u64, crate::Hash64), ()>>())?;
    let n = (count("vesting", false)?);
    sum = sum
        .allocation(n, std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(u64, crate::Hash64), ()>>())?;
    let n = (count("vesting", true)?);
    sum = sum.allocation(
        n,
        std::mem::size_of::<
            crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
                (crate::palw_state_v2::PalwBondKeyV2, bool, u64, crate::Hash64),
                (),
            >,
        >(),
    )?;
    let n = (count("da_sessions", false)?);
    sum = sum.allocation(
        n,
        std::mem::size_of::<
            crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(u64, crate::Hash64, crate::palw_state_v2::PalwBondKeyV2), ()>,
        >(),
    )?;
    let n = (checked_counts_v1(&[count("da_sessions", true)?, count("da_claims", true)?])?);
    sum = sum.allocation(
        n,
        std::mem::size_of::<
            crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(crate::palw_state_v2::PalwBondKeyV2, crate::Hash64), ()>,
        >(),
    )?;
    let n = (count("legacy_disputes", false)?);
    sum = sum
        .allocation(n, std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(u64, crate::Hash64), ()>>())?;
    let n = (count("legacy_disputes", false)?);
    sum = sum.allocation(
        n,
        std::mem::size_of::<
            crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(crate::palw_state_v2::PalwBondKeyV2, crate::Hash64), ()>,
        >(),
    )?;
    let n = (count("claims", false)?);
    sum = sum.allocation(
        n,
        std::mem::size_of::<
            crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
                crate::palw_state_v2::PalwBondKeyV2,
                crate::palw_weight_cap_v1::PalwBondWeightV1,
            >,
        >(),
    )?;
    Ok(sum)
}

/// Maximum of the actual primary, derived and nested-index node layouts used
/// by delta replay. Byte-vector lengths never determine a node size.
pub(crate) fn transition_node_unit_v1() -> u64 {
    let mut n = 0u64;
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (crate::Hash64, crate::Hash64),
            crate::palw_improve_material_v1::PalwDatasetRecordV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (crate::Hash64, crate::Hash64),
            crate::palw_improve_material_v1::PalwHardCaseRecordV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (crate::Hash64, crate::Hash64),
            crate::palw_improve_material_v1::PalwSetterSetRecordV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (crate::Hash64, crate::Hash64),
            crate::palw_improve_material_v1::PalwTeachingArtifactRecordV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (crate::Hash64, crate::Hash64),
            crate::palw_model_lines_v1::PalwArtifactOwnerV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (crate::Hash64, crate::Hash64),
            crate::palw_state_v2::PalwHeldForfeitV1,
        >,
    >() as u64);
    n = n
        .max(std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(crate::Hash64, crate::Hash64), u64>>()
            as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(crate::Hash64, crate::Hash64, u64, crate::Hash64), ()>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (crate::Hash64, crate::palw_state_v2::PalwBondKeyV2),
            crate::palw_da_rcore_v1::PalwDaSessionV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(crate::Hash64, crate::palw_state_v2::PalwBondKeyV2), u64>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, crate::Hash64),
            crate::palw_state_v2::PalwFpClaimPromptV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (crate::Hash64, crate::palw_state_v2::PalwCourtSideV1),
            crate::palw_state_v2::PalwCourtCloseGroupV2,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (crate::Hash64, u32),
            crate::palw_improve_state_v1::PalwLineageHeadEntryV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (crate::Hash64, u32),
            crate::palw_model_lines_v1::PalwModelVersionV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (crate::Hash64, u32),
            crate::palw_work_slice_v2::PalwWorkSliceRowV2,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (crate::Hash64, u32, crate::palw_state_v2::PalwBondKeyV2),
            crate::palw_model_lines_v1::PalwModelEvaluationV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (crate::Hash64, u64),
            crate::palw_improve_state_v1::PalwImprovementEpochV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (crate::Hash64, u64),
            crate::palw_improve_state_v1::PalwMaterialFrontierV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (crate::Hash64, u64, u32),
            crate::palw_improve_state_v1::PalwEpochCandidateV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (crate::Hash64, u64, u32),
            crate::palw_improve_state_v1::PalwEvalItemV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (crate::Hash64, u64, u32),
            crate::palw_improve_state_v1::PalwRewardGrantV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (crate::Hash64, u64, u32, crate::palw_improve_state_v1::PalwEvalSubjectV1),
            crate::palw_improve_state_v1::PalwEvalResultV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(crate::Hash64, u64, u8, crate::Hash64), ()>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (crate::Hash64, u64, u8, crate::Hash64),
            crate::palw_improve_state_v1::PalwPoolEntryV2,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (crate::palw_state_v2::PalwBondKeyV2, bool, u64, crate::Hash64),
            (),
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(crate::palw_state_v2::PalwBondKeyV2, crate::Hash64), ()>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(crate::palw_state_v2::PalwBondKeyV2, crate::Hash64), Vec<u32>>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (crate::palw_state_v2::PalwBondKeyV2, crate::Hash64),
            crate::palw_model_registry_v1::PalwSeatReadinessRowV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (crate::palw_state_v2::PalwBondKeyV2, crate::Hash64),
            crate::palw_panel_var_v1::PalwSlashableLockV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(crate::palw_state_v2::PalwBondKeyV2, crate::Hash64), u64>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (crate::palw_state_v2::PalwBondKeyV2, crate::Hash64, crate::Hash64),
            (),
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (crate::palw_state_v2::PalwBondKeyV2, crate::Hash64, crate::Hash64),
            crate::palw_model_registry_v1::PalwSeatReadinessRowV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (crate::palw_state_v2::PalwBondKeyV2, u64, crate::Hash64, crate::Hash64),
            (),
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (crate::palw_state_v2::PalwBondKeyV2, u64, crate::Hash64, crate::palw_state_v2::PalwBondKeyV2),
            (),
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (crate::palw_state_v2::PalwBondKeyV2, u64, crate::Hash64, crate::palw_state_v2::PalwBondKeyV2, crate::Hash64),
            (),
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (
                crate::palw_state_v2::PalwBondKeyV2,
                u64,
                crate::Hash64,
                crate::palw_state_v2::PalwBondKeyV2,
                crate::Hash64,
                u64,
                crate::Hash64,
            ),
            (),
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (crate::palw_state_v2::PalwBondKeyV2, u64, crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1),
            (),
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(u64, (crate::Hash64, crate::palw_state_v2::PalwCourtSideV1)), ()>,
    >() as u64);
    n = n.max(std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(u64, crate::Hash64), ()>>() as u64);
    n = n.max(
        std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(u64, crate::Hash64, crate::Hash64), ()>>()
            as u64,
    );
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(u64, crate::Hash64, crate::palw_state_v2::PalwBondKeyV2), ()>,
    >() as u64);
    n = n.max(std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(u64, crate::Hash64, u64), ()>>() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(u64, crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1), ()>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(u64, crate::palw_state_v2::PalwBondKeyV2), ()>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (u64, crate::palw_state_v2::PalwBondKeyV2),
            crate::palw_vertex_v1::PalwVertexRoundRowV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(u64, u64), u16>>() as u64);
    n = n.max(std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(u8, Vec<u8>), Vec<u8>>>() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            (u8, u64, crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1),
            (),
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<Option<crate::Hash64>, BTreeSet<crate::Hash64>>,
    >() as u64);
    n = n.max(std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, ()>>() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, BTreeMap<crate::palw_state_v2::PalwBondKeyV2, u64>>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::Hash64,
            BTreeSet<(u64, crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1)>,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::Hash64,
            BTreeSet<(u8, u64, crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1)>,
        >,
    >() as u64);
    n = n.max(
        std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, BTreeSet<crate::Hash64>>>()
            as u64,
    );
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, Box<crate::palw_step::PalwShapeProfileV3>>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::Hash64,
            Box<crate::palw_tir_dissect_v1::PalwTirDissectPhaseV1>,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, Vec<crate::palw_vertex_v1::PalwVertexHeldRowV1>>,
    >() as u64);
    n = n.max(std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, Vec<u8>>>() as u64);
    n = n.max(std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::Hash64>>() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::Hash64,
            crate::palw_activation_pool_v1::PalwActivationPoolV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_audit_door_v1::PalwAuditStatusV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_bond_budget_v1::PalwBudgetClaimRowV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_bond_budget_v1::PalwMaterialIncomeV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_bond_budget_v1::PalwModelBudgetRowV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::Hash64,
            crate::palw_bond_budget_v1::PalwPaidConsumerContractV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::Hash64,
            crate::palw_bond_budget_v1::PalwReporterIncomeBindingV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::Hash64,
            crate::palw_class_semantics_v1::PalwClassSemanticsV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_da_rcore_v1::PalwDaClaimV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::Hash64,
            crate::palw_economic_payout_v1::PalwClaimEconomicsV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_execution_lane_v1::PalwExecFinalV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_funded_escrow_v1::PalwFundedEscrowRowV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_gen_class_v1::PalwGenClassRecordV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_held_da_v1::PalwHeldMissingV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::Hash64,
            crate::palw_improve_composite_v1::PalwTirCompositeRefV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_improve_eval_v1::PalwEvalJobKeyV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::Hash64,
            crate::palw_improve_material_v1::PalwDataUseOptInRecordV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::Hash64,
            crate::palw_improve_material_v1::PalwTeacherLicenceRecordV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_improve_state_v1::PalwImprovementLineV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::Hash64,
            crate::palw_improve_state_v1::PalwImprovementPolicyRecordV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_improve_state_v1::PalwImprovementPoolV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::Hash64,
            crate::palw_improve_state_v1::PalwImprovementUsageV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::Hash64,
            crate::palw_legacy_public_filer_v1::PalwDisputeClaimV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_mesh_v1::PalwAuditRowV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_mesh_v1::PalwCappedClaimRowV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_mesh_v1::PalwTrapRowV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_mesh_v1::PalwWitnessProfileRowV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_model_benefits_v1::PalwModelBenefitsV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_model_lines_v1::PalwModelLineV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_model_lines_v1::PalwModelProposalV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_model_market_v1::PalwModelMarketV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::Hash64,
            crate::palw_model_registry_v1::PalwModelLifecycleRowV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::Hash64,
            crate::palw_model_registry_v1::PalwProbationMemoryV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_offence_v1::PalwConsumedOffenceV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::Hash64,
            crate::palw_panel_da_disclosure_v1::PalwPanelDaDisclosureRowV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::Hash64,
            crate::palw_panel_var_v1::PalwPanelLiabilityRecordV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::Hash64,
            crate::palw_shard_licensing_v1::PalwClassShardPlanV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::Hash64,
            crate::palw_shard_licensing_v1::PalwShardLicensingProgressV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_state_v2::PalwCapabilityStateV2>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_state_v2::PalwCertifiedFamilyStateV2>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_state_v2::PalwClaimStateV2>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_state_v2::PalwClassLaneCertificationV2>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_state_v2::PalwClassStateV2>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_state_v2::PalwClassTargetV2>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_state_v2::PalwClassWalkV2>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_state_v2::PalwCourtSessionStateV2>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_state_v2::PalwEpochCounterV2>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_state_v2::PalwPanelDutyRowV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_state_v2::PalwPanelStateV2>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_state_v2::PalwPayoutV2>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_state_v2::PalwPendingChunksV2>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_state_v2::PalwPendingRewardV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_state_v2::PalwReporterCommitV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_tir_admission_v1::PalwTirClassRecordV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_tir_shard_v1::PalwTirShardClaimV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_tir_shard_v1::PalwTirShardPlanV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_vertex_v1::PalwVertexTallyV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_vesting_v1::PalwVestingRowV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_work_slice_v2::PalwExecJobUseV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_work_slice_v2::PalwExecOriginV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_work_slice_v2::PalwExecV2RewardRowV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, crate::palw_work_slice_v2::PalwWorkRootV2>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, misaka_palw_panel::ClaimRecordV3>,
    >() as u64);
    n = n.max(std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, u128>>() as u64);
    n = n.max(std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, u16>>() as u64);
    n = n.max(std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, u32>>() as u64);
    n = n.max(std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::Hash64, u64>>() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::palw_capacity_s567_v1::PalwCapacityLedgerKeyV1,
            crate::palw_capacity_s567_v1::PalwCapacityLedgerRowV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::palw_derived_v1::PalwDerivedKeyV1,
            crate::palw_derived_v1::PalwDerivedRowV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::palw_improve_eval_v1::PalwEvalJobKeyV1,
            crate::palw_improve_eval_v1::PalwEvalJobStateV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::palw_legacy_answer_chunks_v2::PalwLegacyAnswerKeyV2,
            crate::palw_legacy_answer_chunks_v2::PalwPendingLegacyAnswerV2,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1,
            crate::palw_staged_court_state_v1::PalwStagedCourtRowV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::palw_state_v2::PalwBondKeyV2, BTreeSet<crate::Hash64>>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::palw_state_v2::PalwBondKeyV2, Vec<u64>>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::palw_state_v2::PalwBondKeyV2,
            crate::palw_aggregate_liability_v1::PalwBondFreezeV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::palw_state_v2::PalwBondKeyV2,
            crate::palw_bond_budget_v1::PalwBudgetBondRowV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::palw_state_v2::PalwBondKeyV2,
            crate::palw_bond_budget_v1::PalwCapitalAssignmentRowV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::palw_state_v2::PalwBondKeyV2,
            crate::palw_issuance_slots_v1::PalwIssuanceBucketV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::palw_state_v2::PalwBondKeyV2,
            crate::palw_seat_availability_v1::PalwSeatAvailabilityV1,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::palw_state_v2::PalwBondKeyV2,
            crate::palw_state_v2::PalwBondStateV2,
        >,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::palw_state_v2::PalwBondKeyV2,
            crate::palw_weight_cap_v1::PalwBondWeightV1,
        >,
    >() as u64);
    n = n.max(
        std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::palw_state_v2::PalwBondKeyV2, u128>>()
            as u64,
    );
    n = n.max(
        std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::palw_state_v2::PalwBondKeyV2, u32>>()
            as u64,
    );
    n = n.max(
        std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::palw_state_v2::PalwBondKeyV2, u64>>()
            as u64,
    );
    n = n.max(
        std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::palw_state_v2::PalwBondKeyV2, usize>>()
            as u64,
    );
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<
            crate::palw_terminal_carriage_v1::PalwTerminalKeyV1,
            crate::palw_terminal_carriage_v1::PalwPendingTerminalV1,
        >,
    >() as u64);
    n = n
        .max(std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<misaka_palw_panel::BondIdV1, u128>>()
            as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<u32, crate::evm::model_market::PalwEvmSettlementV1>,
    >() as u64);
    n = n
        .max(std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<u64, BTreeMap<crate::Hash64, u128>>>()
            as u64);
    n = n.max(std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<u64, crate::Hash64>>() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<u64, crate::palw_execution_lane_v1::PalwExecScheduleV1>,
    >() as u64);
    n = n.max(std::mem::size_of::<
        crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<u64, crate::palw_execution_lane_v1::PalwExecSnapshotV1>,
    >() as u64);
    n = n.max(
        std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<u64, misaka_palw_panel::EpochSourceSetV1>>()
            as u64,
    );
    n = n.max(std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<u64, u32>>() as u64);
    n
}
pub(crate) fn scan_vertex_delta_row_v1(
    state: &PalwChainStateV2,
    table: u8,
    key: &[u8],
    old: &Option<Vec<u8>>,
    new: &Option<Vec<u8>>,
) -> std::io::Result<NativeWireShapeV1> {
    match table {
        PALW_VERTEX_TABLE_ROUNDS_V1 => scan_delta_map_row_v1(&state.vertex.rounds, key, old, new),
        PALW_VERTEX_TABLE_TALLIES_V1 => scan_delta_map_row_v1(&state.vertex.tallies, key, old, new),
        PALW_VERTEX_TABLE_HELD_V1 => scan_delta_map_row_v1(&state.vertex.held, key, old, new),
        PALW_MESH_TABLE_WITNESS_V1 => scan_delta_map_row_v1(&state.vertex.mesh.witness, key, old, new),
        PALW_MESH_TABLE_AUDITS_V1 => scan_delta_map_row_v1(&state.vertex.mesh.audits, key, old, new),
        PALW_MESH_TABLE_TRAPS_V1 => scan_delta_map_row_v1(&state.vertex.mesh.traps, key, old, new),
        PALW_MESH_TABLE_CAPPED_V1 => scan_delta_map_row_v1(&state.vertex.mesh.capped, key, old, new),
        _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown delta row table")),
    }
}
pub(crate) fn scan_improvement_delta_row_v1(
    state: &PalwChainStateV2,
    table: u8,
    key: &[u8],
    old: &Option<Vec<u8>>,
    new: &Option<Vec<u8>>,
) -> std::io::Result<NativeWireShapeV1> {
    match table {
        PALW_IMPROVE_TABLE_EPOCHS_V1 => scan_delta_map_row_v1(&state.improvement_epochs, key, old, new),
        PALW_IMPROVE_TABLE_POLICIES_V1 => scan_delta_map_row_v1(&state.improvement_policies, key, old, new),
        PALW_IMPROVE_TABLE_USAGE_V1 => scan_delta_map_row_v1(&state.improvement_usage, key, old, new),
        PALW_IMPROVE_TABLE_HEADS_V1 => scan_delta_map_row_v1(&state.improvement_heads, key, old, new),
        PALW_IMPROVE_TABLE_POOLS_V1 => scan_delta_map_row_v1(&state.improvement_pools, key, old, new),
        PALW_IMPROVE_TABLE_MATERIAL_V1 => scan_delta_map_row_v1(&state.improvement_material, key, old, new),
        PALW_IMPROVE_TABLE_CANDIDATES_V1 => scan_delta_map_row_v1(&state.improvement_candidates, key, old, new),
        PALW_IMPROVE_TABLE_POOL_ENTRIES_V1 => scan_delta_map_row_v1(&state.improvement_pool_entries, key, old, new),
        PALW_IMPROVE_TABLE_ITEMS_V1 => scan_delta_map_row_v1(&state.improvement_items, key, old, new),
        PALW_IMPROVE_TABLE_RESULTS_V1 => scan_delta_map_row_v1(&state.improvement_results, key, old, new),
        PALW_IMPROVE_TABLE_GRANTS_V1 => scan_delta_map_row_v1(&state.improvement_grants, key, old, new),
        PALW_IMPROVE_TABLE_EARNINGS_V1 => scan_delta_map_row_v1(&state.improvement_earnings, key, old, new),
        PALW_IMPROVE_TABLE_EVAL_JOBS_V1 => scan_delta_map_row_v1(&state.improvement_eval_jobs, key, old, new),
        _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown delta row table")),
    }
}
pub(crate) fn scan_exec_delta_row_v1(
    state: &PalwChainStateV2,
    table: u8,
    key: &[u8],
    old: &Option<Vec<u8>>,
    new: &Option<Vec<u8>>,
) -> std::io::Result<NativeWireShapeV1> {
    match table {
        PALW_EXEC_V2_TABLE_ROOTS_V1 => scan_delta_map_row_v1(&state.exec_v2.roots, key, old, new),
        PALW_EXEC_V2_TABLE_SLICES_V1 => scan_delta_map_row_v1(&state.exec_v2.slices, key, old, new),
        PALW_EXEC_V2_TABLE_JOBS_V1 => scan_delta_map_row_v1(&state.exec_v2.jobs, key, old, new),
        PALW_EXEC_V2_TABLE_ANCHORED_V1 => scan_delta_map_row_v1(&state.exec_v2.anchored, key, old, new),
        PALW_EXEC_V2_TABLE_REWARDS_V1 => scan_delta_map_row_v1(&state.exec_v2_rewards, key, old, new),
        PALW_EXEC_V2_TABLE_REWARD_HOLDS_V1 => scan_delta_map_row_v1(&state.exec_v2_reward_holds, key, old, new),
        PALW_EXEC_V2_TABLE_ORIGINS_V1 => scan_delta_map_row_v1(&state.exec_v2_origins, key, old, new),
        PALW_EXEC_V2_TABLE_JOB_USES_V1 => scan_delta_map_row_v1(&state.exec_v2_job_uses, key, old, new),
        _ => Err(crate::palw_native_resource_v1::wire::wire_bad("unknown delta row table")),
    }
}

fn scan_delta_value_v1<T: NativeWireScanV1>(bytes: &[u8]) -> std::io::Result<NativeWireShapeV1> {
    let mut reader = NativeWireReaderV1::new(bytes, bytes.len() as u64);
    let shape = T::scan_native_wire_v1(&mut reader)?;
    if reader.remaining() != 0 {
        return Err(crate::palw_native_resource_v1::wire::wire_bad("delta row trailing bytes"));
    }
    Ok(shape)
}
fn scan_delta_map_row_v1<K: NativeWireScanV1, V: NativeWireScanV1>(
    _: &BTreeMap<K, V>,
    key: &[u8],
    old: &Option<Vec<u8>>,
    new: &Option<Vec<u8>>,
) -> std::io::Result<NativeWireShapeV1> {
    let mut sum = scan_delta_value_v1::<K>(key)?;
    for bytes in [old, new].into_iter().flatten() {
        sum = sum.merge(scan_delta_value_v1::<V>(bytes)?)?;
    }
    Ok(sum)
}
pub(crate) fn scan_budget_delta_row_v1(
    table: u8,
    key: &[u8],
    old: &Option<Vec<u8>>,
    new: &Option<Vec<u8>>,
) -> std::io::Result<NativeWireShapeV1> {
    let mut sum = NativeWireShapeV1::default();
    match table {
        1 => {
            sum = sum.merge(scan_delta_value_v1::<crate::palw_state_v2::PalwBondKeyV2>(key)?)?;
            for bytes in [old, new].into_iter().flatten() {
                sum = sum.merge(scan_delta_value_v1::<crate::palw_bond_budget_v1::PalwBudgetBondRowV1>(bytes)?)?;
            }
        }
        2 => {
            sum = sum.merge(scan_delta_value_v1::<crate::Hash64>(key)?)?;
            for bytes in [old, new].into_iter().flatten() {
                sum = sum.merge(scan_delta_value_v1::<crate::palw_bond_budget_v1::PalwBudgetClaimRowV1>(bytes)?)?;
            }
        }
        3 => {
            sum = sum.merge(scan_delta_value_v1::<(u64, crate::Hash64)>(key)?)?;
            for bytes in [old, new].into_iter().flatten() {
                sum = sum.merge(scan_delta_value_v1::<()>(bytes)?)?;
            }
        }
        4 => {
            sum = sum.merge(scan_delta_value_v1::<crate::palw_state_v2::PalwBondKeyV2>(key)?)?;
            for bytes in [old, new].into_iter().flatten() {
                sum = sum.merge(scan_delta_value_v1::<crate::palw_bond_budget_v1::PalwCapitalAssignmentRowV1>(bytes)?)?;
            }
        }
        5 => {
            sum = sum.merge(scan_delta_value_v1::<crate::Hash64>(key)?)?;
            for bytes in [old, new].into_iter().flatten() {
                sum = sum.merge(scan_delta_value_v1::<crate::palw_bond_budget_v1::PalwModelBudgetRowV1>(bytes)?)?;
            }
        }
        6 => {
            sum = sum.merge(scan_delta_value_v1::<(u64, crate::palw_state_v2::PalwBondKeyV2)>(key)?)?;
            for bytes in [old, new].into_iter().flatten() {
                sum = sum.merge(scan_delta_value_v1::<()>(bytes)?)?;
            }
        }
        7 => {
            sum = sum.merge(scan_delta_value_v1::<crate::Hash64>(key)?)?;
            for bytes in [old, new].into_iter().flatten() {
                sum = sum.merge(scan_delta_value_v1::<crate::palw_bond_budget_v1::PalwMaterialIncomeV1>(bytes)?)?;
            }
        }
        8 => {
            sum = sum.merge(scan_delta_value_v1::<crate::Hash64>(key)?)?;
            for bytes in [old, new].into_iter().flatten() {
                sum = sum.merge(scan_delta_value_v1::<crate::palw_bond_budget_v1::PalwPaidConsumerContractV1>(bytes)?)?;
            }
        }
        9 => {
            sum = sum.merge(scan_delta_value_v1::<crate::Hash64>(key)?)?;
            for bytes in [old, new].into_iter().flatten() {
                sum = sum.merge(scan_delta_value_v1::<crate::palw_bond_budget_v1::PalwReporterIncomeBindingV1>(bytes)?)?;
            }
        }
        _ => return Err(crate::palw_native_resource_v1::wire::wire_bad("unknown budget delta table")),
    }
    Ok(sum)
}
pub(crate) fn transition_existing_index_shape_v1(state: &PalwChainStateV2) -> std::io::Result<NativeWireShapeV1> {
    use crate::palw_native_resource_v1::NativeHeapV1;
    let counts = [
        (
            "claims",
            state.claims.len() as u64,
            state
                .claims
                .native_members_v1()
                .ok_or_else(|| crate::palw_native_resource_v1::wire::wire_bad("actual restore membership overflow"))?,
        ),
        (
            "court_close_groups",
            state.court_close_groups.len() as u64,
            state
                .court_close_groups
                .native_members_v1()
                .ok_or_else(|| crate::palw_native_resource_v1::wire::wire_bad("actual restore membership overflow"))?,
        ),
        (
            "court_sessions",
            state.court_sessions.len() as u64,
            state
                .court_sessions
                .native_members_v1()
                .ok_or_else(|| crate::palw_native_resource_v1::wire::wire_bad("actual restore membership overflow"))?,
        ),
        (
            "da_claims",
            state.da_claims.len() as u64,
            state
                .da_claims
                .native_members_v1()
                .ok_or_else(|| crate::palw_native_resource_v1::wire::wire_bad("actual restore membership overflow"))?,
        ),
        (
            "da_sessions",
            state.da_sessions.len() as u64,
            state
                .da_sessions
                .native_members_v1()
                .ok_or_else(|| crate::palw_native_resource_v1::wire::wire_bad("actual restore membership overflow"))?,
        ),
        (
            "exec_v2_job_uses",
            state.exec_v2_job_uses.len() as u64,
            state
                .exec_v2_job_uses
                .native_members_v1()
                .ok_or_else(|| crate::palw_native_resource_v1::wire::wire_bad("actual restore membership overflow"))?,
        ),
        (
            "exec_v2_rewards",
            state.exec_v2_rewards.len() as u64,
            state
                .exec_v2_rewards
                .native_members_v1()
                .ok_or_else(|| crate::palw_native_resource_v1::wire::wire_bad("actual restore membership overflow"))?,
        ),
        (
            "improvement_artifacts",
            state.improvement_artifacts.len() as u64,
            state
                .improvement_artifacts
                .native_members_v1()
                .ok_or_else(|| crate::palw_native_resource_v1::wire::wire_bad("actual restore membership overflow"))?,
        ),
        (
            "improvement_cases",
            state.improvement_cases.len() as u64,
            state
                .improvement_cases
                .native_members_v1()
                .ok_or_else(|| crate::palw_native_resource_v1::wire::wire_bad("actual restore membership overflow"))?,
        ),
        (
            "improvement_epochs",
            state.improvement_epochs.len() as u64,
            state
                .improvement_epochs
                .native_members_v1()
                .ok_or_else(|| crate::palw_native_resource_v1::wire::wire_bad("actual restore membership overflow"))?,
        ),
        (
            "improvement_eval_jobs",
            state.improvement_eval_jobs.len() as u64,
            state
                .improvement_eval_jobs
                .native_members_v1()
                .ok_or_else(|| crate::palw_native_resource_v1::wire::wire_bad("actual restore membership overflow"))?,
        ),
        (
            "improvement_licences",
            state.improvement_licences.len() as u64,
            state
                .improvement_licences
                .native_members_v1()
                .ok_or_else(|| crate::palw_native_resource_v1::wire::wire_bad("actual restore membership overflow"))?,
        ),
        (
            "improvement_lines",
            state.improvement_lines.len() as u64,
            state
                .improvement_lines
                .native_members_v1()
                .ok_or_else(|| crate::palw_native_resource_v1::wire::wire_bad("actual restore membership overflow"))?,
        ),
        (
            "improvement_opt_ins",
            state.improvement_opt_ins.len() as u64,
            state
                .improvement_opt_ins
                .native_members_v1()
                .ok_or_else(|| crate::palw_native_resource_v1::wire::wire_bad("actual restore membership overflow"))?,
        ),
        (
            "improvement_setter_sets",
            state.improvement_setter_sets.len() as u64,
            state
                .improvement_setter_sets
                .native_members_v1()
                .ok_or_else(|| crate::palw_native_resource_v1::wire::wire_bad("actual restore membership overflow"))?,
        ),
        (
            "legacy_disputes",
            state.legacy_disputes.len() as u64,
            state
                .legacy_disputes
                .native_members_v1()
                .ok_or_else(|| crate::palw_native_resource_v1::wire::wire_bad("actual restore membership overflow"))?,
        ),
        (
            "reporter_commitments",
            state.reporter_commitments.len() as u64,
            state
                .reporter_commitments
                .native_members_v1()
                .ok_or_else(|| crate::palw_native_resource_v1::wire::wire_bad("actual restore membership overflow"))?,
        ),
        (
            "staged_courts",
            state.staged_courts.len() as u64,
            state
                .staged_courts
                .native_members_v1()
                .ok_or_else(|| crate::palw_native_resource_v1::wire::wire_bad("actual restore membership overflow"))?,
        ),
        (
            "vesting",
            state.vesting.len() as u64,
            state
                .vesting
                .native_members_v1()
                .ok_or_else(|| crate::palw_native_resource_v1::wire::wire_bad("actual restore membership overflow"))?,
        ),
    ];
    restore_index_native_shape_v1(&counts)
}
