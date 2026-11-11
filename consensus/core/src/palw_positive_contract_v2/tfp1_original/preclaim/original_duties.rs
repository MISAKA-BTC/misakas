//! Bind the off-chain claim-material supply obligation and reserve the
//! native first-notice controls. HTTP material remains in its own byte and
//! retention budget. HF contributes only the descriptor. Neither the signed
//! obligation nor a booked notice establishes delivery or admission.
use super::super::initial_supply::PublicTfp1InitialSupplyV1;
use super::*;
use crate::palw_court_service_v1::ServicePacket;
use misaka_palw_positive_contract::public_one_honest::{FirstNoticeDemand, OriginalClock};

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize)]
pub(crate) struct OriginalEarlyDutyCostV1 {
    pub service: ServiceCost,
    pub retained_bytes: u64,
    pub escrow_sompi: u64,
    pub future_carrier_sompi: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProtectedEarlyDutyPolicyV1 {
    /// Full notice carrier bound from the native notice resolver. The
    /// inner Selection frame is a floor; other notice codecs may need more.
    pub first_notice_wire_bytes: u64,
    pub first_notice: OriginalEarlyDutyCostV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize)]
pub(crate) struct OriginalEarlyDutyFeesV1 {
    pub escrow_sompi: u64,
    pub future_carrier_sompi: u64,
}

impl OriginalEarlyDutyFeesV1 {
    pub(super) fn total_v1(self) -> Option<u64> {
        self.escrow_sompi.checked_add(self.future_carrier_sompi)
    }
}

/// Native early service for one exact claim. Even a populated value does
/// not include the full protected profile (late requests, repair, settlement).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProtectedOriginalDutiesV1 {
    claim: Hash64,
    source: Hash64,
    clocks: (u64, u64),
    rescue: ProtectedRescueDemand,
    notice_packets: u64,
    first_notice_wire_bytes: u64,
    supply_bytes: u64,
    // Relative supply deadline and half-open native notice window.
    supply_ready_offset: u64,
    notice_window: (u64, u64),
    first_notice: OriginalEarlyDutyCostV1,
    fees: OriginalEarlyDutyFeesV1,
    supply_identity: Hash64,
    native_supply: Option<PublicTfp1InitialSupplyV1>,
}

impl ProtectedOriginalDutiesV1 {
    pub(crate) fn derive_v1(
        projected: PlainBatchProspectiveOriginalTermsV2,
        supply: &PublicTfp1InitialSupplyV1,
        blue_score: u64,
        notice: FirstNoticeDemand,
        schedule: VerificationSchedule,
        rescue: ProtectedRescueDemand,
        policy: ProtectedEarlyDutyPolicyV1,
    ) -> Result<Self, BatchCourtErrorV1> {
        use BatchCourtErrorV1::{CaseGraph, DeliveryCost, Window};
        let daa = projected.accepted_daa_v2();
        let (claim, source, _) = projected.obligation_v2().facts_v1().native_identity_v1();
        if !projected.matches_acceptance_v2(daa, blue_score) || claim != supply.claim_v1() || source != supply.source_v1() {
            return Err(CaseGraph);
        }
        let bytes = supply.claim_wire_bytes_v1();
        let notice_inner = crate::palw_court_shared_material_intake_v1::batch::selection::notice::SignedSelectionNoticeV3::BYTES;
        if bytes == 0
            || policy.first_notice_wire_bytes < notice_inner
            || policy.first_notice.service.bytes < policy.first_notice_wire_bytes
            || policy.first_notice.service.work == 0
            || policy.first_notice.service.controls != 1
            || policy.first_notice.retained_bytes == 0
        {
            return Err(DeliveryCost);
        }
        let notice_packets = rescue.max_drawn_bonds.checked_add(rescue.max_outsiders).ok_or(DeliveryCost)?;
        if rescue.max_drawn_bonds == 0 || notice.notice_inclusion_daa == 0 {
            return Err(Window);
        }
        let first_notice_by = notice
            .draw_ready_by
            .max(notice.material_ready_by)
            .checked_add(notice.cold_acquisition_daa)
            .and_then(|n| n.checked_add(notice.full_inspection_daa))
            .and_then(|n| n.checked_add(notice.notice_inclusion_daa))
            .ok_or(Window)?;
        let latest_notice = OriginalClock::new(daa).map_err(|_| Window)?.notice_until();
        let notice_before = first_notice_by.checked_sub(daa).and_then(|n| n.checked_add(1)).ok_or(Window)?;
        let notice_after = notice_before.checked_sub(notice.notice_inclusion_daa).ok_or(Window)?;
        let supply_ready_offset = notice.material_ready_by.checked_sub(daa).ok_or(Window)?;
        if notice_after <= supply_ready_offset || first_notice_by != schedule.first_notice_by() || first_notice_by > latest_notice {
            return Err(Window);
        }
        let fees = OriginalEarlyDutyFeesV1 {
            escrow_sompi: policy.first_notice.escrow_sompi.checked_mul(notice_packets).ok_or(DeliveryCost)?,
            future_carrier_sompi: policy.first_notice.future_carrier_sompi.checked_mul(notice_packets).ok_or(DeliveryCost)?,
        };
        fees.total_v1().ok_or(DeliveryCost)?;
        let supply_identity = crate::palw_court_shared_tir_stream_v1::stream_digest(
            b"misaka-palw/tfp1-native-initial-supply/v1",
            &(claim, source, supply.literal_prompt_v1(), supply.hf_content_root_v1(), supply.tokenizer_registration_v1(), bytes),
        )
        .map_err(|_| CaseGraph)?;
        Ok(Self {
            claim,
            source,
            clocks: (daa, blue_score),
            rescue,
            notice_packets,
            first_notice_wire_bytes: policy.first_notice_wire_bytes,
            supply_bytes: bytes,
            supply_ready_offset,
            notice_window: (notice_after, notice_before),
            first_notice: policy.first_notice,
            fees,
            supply_identity,
            native_supply: Some(*supply),
        })
    }

    #[cfg(test)]
    pub(super) fn reservation_fixture_v1(terms: &DrawnBatchPreclaimTermsV1, blue: u64, schedule: VerificationSchedule) -> Self {
        let (claim, source, _) = terms.obligation.facts_v1().native_identity_v1();
        let daa = terms.projected.accepted_daa_v2();
        let notice_packets = terms.rescue.max_drawn_bonds + terms.rescue.max_outsiders;
        let notice_start = schedule.first_notice_by() - daa;
        Self {
            claim,
            source,
            clocks: (daa, blue),
            rescue: terms.rescue,
            notice_packets,
            first_notice_wire_bytes: 8_000,
            // Deliberately much larger than the notice packets. No byte of
            // this HTTP fixture becomes an extra native service packet.
            supply_bytes: 1 << 20,
            supply_ready_offset: 1,
            notice_window: (notice_start, notice_start + 1),
            first_notice: OriginalEarlyDutyCostV1 {
                service: ServiceCost { bytes: 8_000, work: 1, io: 1, controls: 1 },
                retained_bytes: 64,
                escrow_sompi: 3,
                future_carrier_sompi: 7,
            },
            fees: OriginalEarlyDutyFeesV1 { escrow_sompi: 3 * notice_packets, future_carrier_sompi: 7 * notice_packets },
            supply_identity: Hash64::from_u64_word(91),
            native_supply: None,
        }
    }

    pub(super) fn check_initial_supply_v2(&self, supply: &PublicTfp1InitialSupplyV1) -> Result<(), BatchCourtErrorV1> {
        if self.native_supply.as_ref() != Some(supply) {
            return Err(BatchCourtErrorV1::CaseGraph);
        }
        Ok(())
    }

    pub(super) fn identity_v1(&self) -> Result<Hash64, ContractError> {
        crate::palw_court_shared_tir_stream_v1::stream_digest(
            b"misaka-palw/tfp1-early-duty-graph/v1",
            &(
                self.supply_identity,
                self.notice_packets,
                self.rescue,
                self.first_notice_wire_bytes,
                self.supply_bytes,
                self.supply_ready_offset,
                self.notice_window,
                self.first_notice,
            ),
        )
        .map_err(|_| ContractError::NonCanonical)
    }

    pub(super) fn count_v1(&self) -> Result<usize, BatchCourtErrorV1> {
        usize::try_from(self.notice_packets).map_err(|_| BatchCourtErrorV1::CaseGraph)
    }

    pub(super) fn check_v1(
        &self,
        terms: &DrawnBatchPreclaimTermsV1,
        schedule: VerificationSchedule,
        initial_supply: RemainingCapacity,
    ) -> Result<(), BatchCourtErrorV1> {
        let (claim, source, _) = terms.obligation.facts_v1().native_identity_v1();
        if (claim, source) != (self.claim, self.source)
            || !terms.projected.matches_acceptance_v2(self.clocks.0, self.clocks.1)
            || terms.rescue != self.rescue
            || self.clocks.0.checked_add(self.notice_window.1 - 1) != Some(schedule.first_notice_by())
            || initial_supply.transmitted_bytes < self.supply_bytes
            || initial_supply.retained_bytes < self.supply_bytes
        {
            return Err(BatchCourtErrorV1::CaseGraph);
        }
        Ok(())
    }

    pub(super) fn notice_capacity_v1(&self, role: ProtectedBatchRoleV1) -> RemainingCapacity {
        if role == ProtectedBatchRoleV1::Defense {
            RemainingCapacity { transmitted_bytes: 0, additional_work: 0, retained_bytes: 0 }
        } else {
            capacity(self.first_notice)
        }
    }

    pub(super) fn fees_v1(&self) -> OriginalEarlyDutyFeesV1 {
        self.fees
    }

    pub(super) fn graph_v1(&self, original: OriginalId) -> Result<NativeRetainedV1<Vec<CausalServicePacketV1>>, BatchCourtErrorV1> {
        use BatchCourtErrorV1::{CaseGraph, LocalResource, Window};
        let count = self.count_v1()?;
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<Vec<CausalServicePacketV1>>(1).map_err(LocalResource)?;
        q.exact_v1::<CausalServicePacketV1>(count as u64).map_err(LocalResource)?;
        let owner = reserve_native_pending_requests_v1(q.bytes_v1()).map_err(LocalResource)?;
        let mut graph = exact_vec(count)?;
        let identity = self.identity_v1().map_err(|_| CaseGraph)?;
        let release_daa = self.clocks.0.checked_add(self.notice_window.0).ok_or(Window)?;
        let service_before = self.clocks.0.checked_add(self.notice_window.1).ok_or(Window)?;
        for rank in 0..self.notice_packets {
            let id = crate::palw_court_shared_tir_stream_v1::stream_digest(
                b"misaka-palw/tfp1-first-notice-packet/v1",
                &(original, identity, rank),
            )
            .map_err(|_| CaseGraph)?
            .as_bytes();
            graph.push(CausalServicePacketV1 {
                packet: ServicePacket { id, release_daa, service_before, cost: self.first_notice.service },
                predecessors: Vec::new(),
            });
        }
        Ok(NativeRetainedV1::from_owner_reserved_v1(graph, owner))
    }
}

fn capacity(cost: OriginalEarlyDutyCostV1) -> RemainingCapacity {
    RemainingCapacity {
        transmitted_bytes: cost.service.bytes,
        additional_work: cost.service.work,
        retained_bytes: cost.retained_bytes,
    }
}

pub(super) fn fits(required: RemainingCapacity, allowance: RemainingCapacity) -> bool {
    required.transmitted_bytes <= allowance.transmitted_bytes
        && required.additional_work <= allowance.additional_work
        && required.retained_bytes <= allowance.retained_bytes
}
