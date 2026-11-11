//! Pre-notice MaterialRequest capacity for one normalized drawn rank. This
//! reserves an inclusion opportunity, not request authenticity or acceptance.
use crate::{
    Hash64,
    palw_batch_court_v1::BatchCourtErrorV1,
    palw_court_service_v1::{ServiceCost, ServicePacket},
    palw_court_shared_material_intake_v1::batch::admission::protected::{BatchInitialRequestV1, manifest_request_packet_id_v1},
};
use misaka_palw_positive_contract::public_one_honest::{FirstNoticeDemand, PendingManifestCaseRecordV1, VerificationSchedule};

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize)]
pub(crate) struct InitialMaterialRequestPlanV1 {
    ready_offset: u64,
    before_offset: u64,
    service: ServiceCost,
    retained_bytes: u64,
}

impl InitialMaterialRequestPlanV1 {
    pub(crate) fn minimum_retained_bytes_v1() -> u64 {
        (PendingManifestCaseRecordV1::WIRE_BYTES + super::super::manifest_paid::PaidManifestRequestRowV1::WIRE_BYTES) as u64
    }

    pub(crate) fn retained_bytes_v1(&self) -> u64 {
        self.retained_bytes
    }

    pub(crate) fn new_v1(
        accepted_daa: u64,
        notice: FirstNoticeDemand,
        service: ServiceCost,
        retained_bytes: u64,
    ) -> Result<Self, BatchCourtErrorV1> {
        use BatchCourtErrorV1::{Clock, DeliveryCost, Window};
        let known = notice.draw_ready_by.max(notice.missing_material_detected_by);
        if notice.draw_ready_by < accepted_daa
            || notice.missing_material_detected_by < accepted_daa
            || notice.material_request_inclusion_daa == 0
        {
            return Err(Window);
        }
        let offset = known.checked_sub(accepted_daa).ok_or(Clock)?;
        let ready_offset = offset.checked_add(1).ok_or(Clock)?;
        let before_offset = offset.checked_add(notice.material_request_inclusion_daa).and_then(|n| n.checked_add(1)).ok_or(Clock)?;
        if before_offset > crate::palw_court_policy_v1::NOTICE_WINDOW_DAA + 1 {
            return Err(Window);
        }
        let carrier_bytes = super::super::manifest_carrier::MANIFEST_REQUEST_CARRIER_BYTES_V1 as u64;
        // Floors only. The native price must additionally include signature,
        // carrier overhead, complete receipt/index/undo and slot-search work.
        if service.bytes < carrier_bytes
            || service.work < carrier_bytes
            || service.io < retained_bytes
            || service.controls != 1
            || retained_bytes < Self::minimum_retained_bytes_v1()
            || retained_bytes > service.bytes
        {
            return Err(DeliveryCost);
        }
        Ok(Self { ready_offset, before_offset, service, retained_bytes })
    }

    pub(crate) fn matches_notice_v1(&self, accepted_daa: u64, notice: FirstNoticeDemand) -> bool {
        Self::new_v1(accepted_daa, notice, self.service, self.retained_bytes).is_ok_and(|plan| plan == *self)
    }

    pub(crate) fn check_schedule_v1(
        &self,
        accepted_daa: u64,
        prerequisites_ready_by: u64,
        schedule: VerificationSchedule,
    ) -> Result<(), BatchCourtErrorV1> {
        let ready = accepted_daa.checked_add(self.ready_offset).ok_or(BatchCourtErrorV1::Clock)?;
        if Some(ready) != prerequisites_ready_by.checked_add(1) {
            return Err(BatchCourtErrorV1::Window);
        }
        self.check_deadline_v1(accepted_daa, schedule)
    }

    pub(super) fn check_deadline_v1(&self, accepted_daa: u64, schedule: VerificationSchedule) -> Result<(), BatchCourtErrorV1> {
        let last = accepted_daa.checked_add(self.before_offset).and_then(|n| n.checked_sub(1)).ok_or(BatchCourtErrorV1::Clock)?;
        if last != schedule.first_material_request_by() {
            return Err(BatchCourtErrorV1::Window);
        }
        Ok(())
    }

    pub(crate) fn request_v1(
        &self,
        claim: Hash64,
        scope: Hash64,
        accepted_daa: u64,
    ) -> Result<BatchInitialRequestV1, BatchCourtErrorV1> {
        Ok(BatchInitialRequestV1 {
            packet: ServicePacket {
                id: manifest_request_packet_id_v1(claim, scope)?,
                release_daa: accepted_daa.checked_add(self.ready_offset).ok_or(BatchCourtErrorV1::Clock)?,
                service_before: accepted_daa.checked_add(self.before_offset).ok_or(BatchCourtErrorV1::Clock)?,
                cost: self.service,
            },
            retained_bytes: self.retained_bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_positive_contract::public_one_honest::{MaterialCaseDemand, OriginalClock};

    #[test]
    fn initial_request_binds_readiness_deadline_and_minimum_carriage() {
        let a = 100;
        let notice = FirstNoticeDemand {
            draw_ready_by: a + 1,
            material_ready_by: a + 1,
            cold_acquisition_daa: 1,
            full_inspection_daa: 1,
            notice_inclusion_daa: 1,
            missing_material_detected_by: a + 3_119,
            material_request_inclusion_daa: 1,
        };
        let retained = InitialMaterialRequestPlanV1::minimum_retained_bytes_v1();
        let bytes = super::super::super::manifest_carrier::MANIFEST_REQUEST_CARRIER_BYTES_V1 as u64;
        let cost = ServiceCost { bytes, work: bytes, io: retained, controls: 1 };
        let plan = InitialMaterialRequestPlanV1::new_v1(a, notice, cost, retained).unwrap();
        let schedule = VerificationSchedule::new(
            OriginalClock::new(a).unwrap(),
            notice,
            MaterialCaseDemand {
                material_delivery_daa: 1,
                full_inspection_daa: 1,
                localization_daa: 1,
                filing_daa: 1,
                adjudication_and_recovery_daa: 1,
            },
        )
        .unwrap();
        let request = plan.request_v1(Hash64::from_u64_word(1), Hash64::from_u64_word(2), a).unwrap();
        assert_eq!(request.packet.release_daa, a + 3_120);
        assert_eq!(request.packet.service_before, a + 3_121);
        plan.check_schedule_v1(a, a + 3_119, schedule).unwrap();
        for readiness in [a + 3_118, a + 3_120, u64::MAX] {
            assert_eq!(plan.check_schedule_v1(a, readiness, schedule), Err(BatchCourtErrorV1::Window));
        }
        for invalid in [
            FirstNoticeDemand { material_request_inclusion_daa: 0, ..notice },
            FirstNoticeDemand { material_request_inclusion_daa: 2, ..notice },
            FirstNoticeDemand { draw_ready_by: a - 1, ..notice },
            FirstNoticeDemand { missing_material_detected_by: a - 1, ..notice },
        ] {
            assert!(InitialMaterialRequestPlanV1::new_v1(a, invalid, cost, retained).is_err());
        }
        for invalid in [
            ServiceCost { bytes: bytes - 1, ..cost },
            ServiceCost { work: bytes - 1, ..cost },
            ServiceCost { io: retained - 1, ..cost },
            ServiceCost { controls: 0, ..cost },
        ] {
            assert_eq!(InitialMaterialRequestPlanV1::new_v1(a, notice, invalid, retained), Err(BatchCourtErrorV1::DeliveryCost));
        }
        assert!(plan.request_v1(Hash64::default(), Hash64::from_u64_word(2), a).is_err());
        assert!(plan.request_v1(Hash64::from_u64_word(1), Hash64::default(), a).is_err());
    }
}
