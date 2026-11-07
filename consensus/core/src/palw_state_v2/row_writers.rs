//! Table-specific entry points for rows without dependent indexes.
//!
//! The transition keeps its existing writer API. Each entry hands the scoped
//! writer only the named map, journal and construction reservation. Policy,
//! cascading writes, and indexed records remain in their dedicated writers.

use super::row_writer::JournaledMapWriterV1;
use super::*;

impl TransitionBuilder<'_> {
    pub(super) fn write_exposure(&mut self, key: PalwBondKeyV2, new: Option<u128>) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.reserved_exposure,
            &mut self.entries,
            &mut self.native_reservation,
            "native reserved_exposure writer",
        )
        .write(key, new, |key, old, new| PalwDeltaEntryV2::Exposure { key, old, new })
    }

    pub(super) fn write_registration_exposure(&mut self, key: PalwBondKeyV2, new: Option<u128>) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.registration_exposure,
            &mut self.entries,
            &mut self.native_reservation,
            "native registration_exposure writer",
        )
        .write(key, new, |key, old, new| PalwDeltaEntryV2::RegistrationExposure { key, old, new })
    }

    pub(super) fn write_payout(&mut self, key: Hash64, new: Option<PalwPayoutV2>) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.pending_payouts,
            &mut self.entries,
            &mut self.native_reservation,
            "native pending_payouts writer",
        )
        .write(key, new, |key, old, new| PalwDeltaEntryV2::Payout { key, old, new })
    }

    pub(super) fn write_model_market(
        &mut self,
        key: Hash64,
        new: Option<crate::palw_model_market_v1::PalwModelMarketV1>,
    ) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.model_markets,
            &mut self.entries,
            &mut self.native_reservation,
            "native model_markets writer",
        )
        .write(key, new, |key, old, new| PalwDeltaEntryV2::ModelMarket { key, old, new })
    }

    pub(super) fn write_model_position(&mut self, key: (Hash64, Hash64), units: u64) -> Result<(), PalwStateV2Error> {
        let new = (units > 0).then_some(units);
        JournaledMapWriterV1::new(
            &mut self.state.model_positions,
            &mut self.entries,
            &mut self.native_reservation,
            "native model_positions writer",
        )
        .write_if_changed(key, new, |key, old, new| PalwDeltaEntryV2::ModelPosition { key, old, new })
    }

    /// ADR-0095 §4.1.
    pub(super) fn write_model_benefits(
        &mut self,
        key: Hash64,
        new: Option<crate::palw_model_benefits_v1::PalwModelBenefitsV1>,
    ) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.model_benefits,
            &mut self.entries,
            &mut self.native_reservation,
            "native model_benefits writer",
        )
        .write_if_changed(key, new, |key, old, new| PalwDeltaEntryV2::ModelBenefits { key, old, new })
    }

    pub(super) fn write_model_line(
        &mut self,
        key: Hash64,
        new: Option<crate::palw_model_lines_v1::PalwModelLineV1>,
    ) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.model_lines,
            &mut self.entries,
            &mut self.native_reservation,
            "native model_lines writer",
        )
        .write_if_changed(key, new, |key, old, new| PalwDeltaEntryV2::ModelLine { key, old, new })
    }

    pub(super) fn write_model_proposal(
        &mut self,
        key: Hash64,
        new: Option<crate::palw_model_lines_v1::PalwModelProposalV1>,
    ) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.model_proposals,
            &mut self.entries,
            &mut self.native_reservation,
            "native model_proposals writer",
        )
        .write_if_changed(key, new, |key, old, new| PalwDeltaEntryV2::ModelProposal { key, old, new })
    }

    pub(super) fn write_model_evaluation(
        &mut self,
        key: (Hash64, u32, PalwBondKeyV2),
        new: Option<crate::palw_model_lines_v1::PalwModelEvaluationV1>,
    ) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.model_evaluations,
            &mut self.entries,
            &mut self.native_reservation,
            "native model_evaluations writer",
        )
        .write_if_changed(key, new, |key, old, new| PalwDeltaEntryV2::ModelEvaluation { key, old, new })
    }

    pub(super) fn write_evm_settlement(
        &mut self,
        key: u32,
        new: Option<crate::evm::model_market::PalwEvmSettlementV1>,
    ) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.evm_settlements,
            &mut self.entries,
            &mut self.native_reservation,
            "native evm_settlements writer",
        )
        .write_if_changed(key, new, |key, old, new| PalwDeltaEntryV2::EvmSettlement { key, old, new })
    }

    pub(super) fn write_shard_licensing(
        &mut self,
        key: Hash64,
        new: Option<crate::palw_shard_licensing_v1::PalwShardLicensingProgressV1>,
    ) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.shard_licensing,
            &mut self.entries,
            &mut self.native_reservation,
            "native shard_licensing writer",
        )
        .write_if_changed(key, new, |key, old, new| PalwDeltaEntryV2::ShardLicensing { key, old, new })
    }

    pub(super) fn write_held_da_missing(
        &mut self,
        key: Hash64,
        new: Option<crate::palw_held_da_v1::PalwHeldMissingV1>,
    ) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.held_da_missing,
            &mut self.entries,
            &mut self.native_reservation,
            "native held_da_missing writer",
        )
        .write_if_changed(key, new, |key, old, new| PalwDeltaEntryV2::HeldDaMissing { key, old, new })
    }

    /// **RFC-0006: the one writer of `tir_shard_claims`**, journaled `TirShardClaim`.
    pub(super) fn write_tir_shard_claim(
        &mut self,
        key: Hash64,
        new: Option<crate::palw_tir_shard_v1::PalwTirShardClaimV1>,
    ) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.tir_shard_claims,
            &mut self.entries,
            &mut self.native_reservation,
            "native tir_shard_claims writer",
        )
        .write_if_changed(key, new, |key, old, new| PalwDeltaEntryV2::TirShardClaim { key, old, new })
    }

    pub(super) fn write_seat_root_readiness(
        &mut self,
        key: (PalwBondKeyV2, Hash64, Hash64),
        new: Option<crate::palw_model_registry_v1::PalwSeatReadinessRowV1>,
    ) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.seat_root_readiness,
            &mut self.entries,
            &mut self.native_reservation,
            "native seat_root_readiness writer",
        )
        .write_if_changed(key, new, |key, old, new| PalwDeltaEntryV2::SeatRootReadiness { key, old, new })
    }

    pub(super) fn write_seat_availability(
        &mut self,
        key: PalwBondKeyV2,
        new: Option<crate::palw_seat_availability_v1::PalwSeatAvailabilityV1>,
    ) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.seat_availability,
            &mut self.entries,
            &mut self.native_reservation,
            "native seat_availability writer",
        )
        .write_if_changed(key, new, |key, old, new| PalwDeltaEntryV2::SeatAvailability { key, old, new })
    }

    pub(super) fn write_held_leaf_demands(
        &mut self,
        key: Hash64,
        new: Option<BTreeMap<PalwBondKeyV2, u64>>,
    ) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.held_leaf_demands,
            &mut self.entries,
            &mut self.native_reservation,
            "native held_leaf_demands writer",
        )
        .write_if_changed(key, new, |key, old, new| PalwDeltaEntryV2::HeldLeafDemands { key, old, new })
    }

    pub(super) fn write_panel_duties(&mut self, key: Hash64, new: Option<PalwPanelDutyRowV1>) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.panel_duties,
            &mut self.entries,
            &mut self.native_reservation,
            "native panel_duties writer",
        )
        .write_if_changed(key, new, |key, old, new| PalwDeltaEntryV2::PanelDuties { key, old, new })
    }

    pub(super) fn write_round_final(
        &mut self,
        key: Hash64,
        new: Option<crate::palw_execution_lane_v1::PalwExecFinalV1>,
    ) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.round_finals,
            &mut self.entries,
            &mut self.native_reservation,
            "native round_finals writer",
        )
        .write_if_changed(key, new, |key, old, new| PalwDeltaEntryV2::RoundFinal { key, old, new })
    }

    pub(super) fn write_round_schedule(
        &mut self,
        key: u64,
        new: Option<crate::palw_execution_lane_v1::PalwExecScheduleV1>,
    ) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.round_schedules,
            &mut self.entries,
            &mut self.native_reservation,
            "native round_schedules writer",
        )
        .write_if_changed(key, new, |key, old, new| PalwDeltaEntryV2::RoundSchedule { key, old, new })
    }

    pub(super) fn write_round_permits_used(&mut self, key: (u64, u64), new: Option<u16>) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.round_permits_used,
            &mut self.entries,
            &mut self.native_reservation,
            "native round_permits_used writer",
        )
        .write_if_changed(key, new, |key, old, new| PalwDeltaEntryV2::RoundPermitsUsed { key, old, new })
    }

    pub(super) fn write_round_equivocations(&mut self, key: (u64, u64), new: Option<u16>) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.round_equivocations,
            &mut self.entries,
            &mut self.native_reservation,
            "native round_equivocations writer",
        )
        .write_if_changed(key, new, |key, old, new| PalwDeltaEntryV2::RoundEquivocations { key, old, new })
    }

    pub(super) fn write_round_pending(
        &mut self,
        key: u64,
        new: Option<crate::palw_execution_lane_v1::PalwExecSnapshotV1>,
    ) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.round_pending,
            &mut self.entries,
            &mut self.native_reservation,
            "native round_pending writer",
        )
        .write_if_changed(key, new, |key, old, new| PalwDeltaEntryV2::RoundPending { key, old, new })
    }

    pub(super) fn write_seat_readiness(
        &mut self,
        key: (PalwBondKeyV2, Hash64),
        new: Option<crate::palw_model_registry_v1::PalwSeatReadinessRowV1>,
    ) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.seat_readiness,
            &mut self.entries,
            &mut self.native_reservation,
            "native seat_readiness writer",
        )
        .write_if_changed(key, new, |key, old, new| PalwDeltaEntryV2::SeatReadiness { key, old, new })
    }

    pub(super) fn write_claim_economics(
        &mut self,
        key: Hash64,
        new: Option<crate::palw_economic_payout_v1::PalwClaimEconomicsV1>,
    ) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.claim_economics,
            &mut self.entries,
            &mut self.native_reservation,
            "native claim_economics writer",
        )
        .write_if_changed(key, new, |key, old, new| PalwDeltaEntryV2::ClaimEconomics { key, old, new })
    }

    /// **The 2026-09-25 sweep's V03(2)/V05: write or drop a class's probation memory** — the rooted
    /// write and its delta entry (81). An empty memory is no row.
    /// **ADR-0160 F-Q: the one writer of `audit_receipts`**, journaled `AuditReceipts` (83).
    pub(super) fn write_audit_receipts_v1(
        &mut self,
        key: Hash64,
        new: Option<crate::palw_audit_door_v1::PalwAuditStatusV1>,
    ) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.audit_receipts,
            &mut self.entries,
            &mut self.native_reservation,
            "native audit_receipts writer",
        )
        .write_if_changed(key, new, |key, old, new| PalwDeltaEntryV2::AuditReceipts { key, old, new })
    }

    /// **ADR-0160 F-Q (§5.9 (g)): the one writer of `excluded_auditors`**, journaled `AuditorExcluded` (84).
    pub(super) fn write_auditor_excluded_v1(&mut self, key: PalwBondKeyV2, new: Option<u64>) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.excluded_auditors,
            &mut self.entries,
            &mut self.native_reservation,
            "native excluded_auditors writer",
        )
        .write_if_changed(key, new, |key, old, new| PalwDeltaEntryV2::AuditorExcluded { key, old, new })
    }

    /// **ADR-0160 F-N: the one writer of `network_demand`**, journaled `NetworkDemand` (86).
    pub(super) fn write_network_demand_v1(&mut self, key: (PalwBondKeyV2, Hash64), new: Option<u64>) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.network_demand,
            &mut self.entries,
            &mut self.native_reservation,
            "native network_demand writer",
        )
        .write_if_changed(key, new, |key, old, new| PalwDeltaEntryV2::NetworkDemand { key, old, new })
    }

    /// **ADR-0164: the one writer of `capacity_ledger`**, journaled `CapacityLedger` (the last variant).
    pub(super) fn write_capacity_ledger_v1(
        &mut self,
        key: crate::palw_capacity_s567_v1::PalwCapacityLedgerKeyV1,
        new: Option<crate::palw_capacity_s567_v1::PalwCapacityLedgerRowV1>,
    ) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.capacity_ledger,
            &mut self.entries,
            &mut self.native_reservation,
            "native capacity_ledger writer",
        )
        .write_if_changed(key, new, |key, old, new| PalwDeltaEntryV2::CapacityLedger { key, old, new })
    }

    /// **ADR-0160 F-S: the one writer of `issuance_buckets`**, journaled `IssuanceBucket` (85).
    pub(super) fn write_issuance_bucket_v1(
        &mut self,
        key: PalwBondKeyV2,
        new: Option<crate::palw_issuance_slots_v1::PalwIssuanceBucketV1>,
    ) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.issuance_buckets,
            &mut self.entries,
            &mut self.native_reservation,
            "native issuance_buckets writer",
        )
        .write_if_changed(key, new, |key, old, new| PalwDeltaEntryV2::IssuanceBucket { key, old, new })
    }

    /// **ADR-0160 lane liab (AG-3): the one writer of `bond_freezes`**, journaled `BondFreeze` (82).
    pub(super) fn write_bond_freeze_v1(
        &mut self,
        key: PalwBondKeyV2,
        new: Option<crate::palw_aggregate_liability_v1::PalwBondFreezeV1>,
    ) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.bond_freezes,
            &mut self.entries,
            &mut self.native_reservation,
            "native bond_freezes writer",
        )
        .write_if_changed(key, new, |key, old, new| PalwDeltaEntryV2::BondFreeze { key, old, new })
    }

    pub(super) fn write_probation_memory_v1(
        &mut self,
        key: Hash64,
        new: Option<crate::palw_model_registry_v1::PalwProbationMemoryV1>,
    ) -> Result<(), PalwStateV2Error> {
        let new = new.filter(|memory| !memory.is_empty());
        JournaledMapWriterV1::new(
            &mut self.state.probation_memory,
            &mut self.entries,
            &mut self.native_reservation,
            "native probation_memory writer",
        )
        .write_if_changed(key, new, |key, old, new| PalwDeltaEntryV2::ProbationMemory { key, old, new })
    }

    /// **4-ter.3 step 6: write or drop one held forfeit** — the rooted write and its delta entry (80).
    pub(super) fn write_held_forfeit(
        &mut self,
        key: (Hash64, Hash64),
        new: Option<PalwHeldForfeitV1>,
    ) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.held_forfeits,
            &mut self.entries,
            &mut self.native_reservation,
            "native held_forfeits writer",
        )
        .write_if_changed(key, new, |key, old, new| PalwDeltaEntryV2::HeldForfeit { key, old, new })
    }

    pub(super) fn write_epoch(&mut self, key: Hash64, new: Option<PalwEpochCounterV2>) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.epoch_counters,
            &mut self.entries,
            &mut self.native_reservation,
            "native epoch_counters writer",
        )
        .write(key, new, |key, old, new| PalwDeltaEntryV2::Epoch { key, old, new })
    }

    pub(super) fn write_receipt_epoch(&mut self, key: Hash64, new: Option<PalwEpochCounterV2>) -> Result<(), PalwStateV2Error> {
        JournaledMapWriterV1::new(
            &mut self.state.receipt_epoch_counters,
            &mut self.entries,
            &mut self.native_reservation,
            "native receipt_epoch_counters writer",
        )
        .write(key, new, |key, old, new| PalwDeltaEntryV2::ReceiptEpoch { key, old, new })
    }
}
