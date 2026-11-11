//! Allocation-independent planning shared by unsigned quote and signed intake.
//! Selected-bond graphs are only part of a complete profile; this module does
//! not authorize original admission, settlement or public activation.
pub(crate) mod initial_request;
use super::{PreparedTfp1PublicOriginalV1, Tfp1PublicOriginalObligationV1};
use crate::{
    Hash64,
    palw_batch_court_v1::BatchCourtErrorV1,
    palw_court_service_v1::{ServiceCost, ServiceRefusal, ServiceReservations, causal::CausalServicePacketV1},
    palw_court_shared_material_intake_v1::batch::{
        admission::{BatchAdmissionQuoteV1, BatchDeliveryV1, CompiledBatchAdmissionV1},
        plain_output::projection::{PlainBatchOriginalProjectionV2, PlainBatchProspectiveOriginalTermsV2},
    },
    palw_native_resource_v1::{NativeCoverageV1, NativeRetainedV1, TirNativeRequestsV1, reserve_native_pending_requests_v1},
    palw_positive_contract_v2::{PublicOriginalAdmissionV1, PublicOriginalReservationV1},
};
use initial_request::InitialMaterialRequestPlanV1;
use misaka_palw_positive_contract::{
    ContractError,
    public_one_honest::{OriginalCommit, OriginalId, ProtectedRescueDemand, VerificationSchedule},
};

type Booking = (NativeRetainedV1<Vec<BatchAdmissionQuoteV1>>, NativeRetainedV1<Vec<CausalServicePacketV1>>);

/// The signed resource identity includes the FULL promised rescue split,
/// even though this partial compiler books only the selected-bond graphs.
/// A full profile must add outsider/defense/Selection/recovery before success.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DrawnBatchPreclaimTermsV1 {
    projected: PlainBatchProspectiveOriginalTermsV2,
    rescue: ProtectedRescueDemand,
    obligation: Tfp1PublicOriginalObligationV1,
    initial_request: Option<InitialMaterialRequestPlanV1>,
    relative_graph: Hash64,
}

impl DrawnBatchPreclaimTermsV1 {
    pub(crate) fn new_v1(
        projected: PlainBatchProspectiveOriginalTermsV2,
        rescue: ProtectedRescueDemand,
    ) -> Result<Self, ContractError> {
        rescue.total()?;
        let resource = crate::palw_court_shared_tir_stream_v1::stream_digest(
            b"misaka-palw/tfp1-drawn-batch-resource/v1",
            &(projected.resource_id_v2(), projected.relative_graph_id_v2(), rescue),
        )
        .map_err(|_| ContractError::NonCanonical)?;
        let obligation = (*projected.obligation_v2().facts_v1()).service_obligation_v1(projected.profile_id_v2(), resource)?;
        Ok(Self { projected, rescue, obligation, initial_request: None, relative_graph: projected.relative_graph_id_v2() })
    }

    pub(crate) fn with_initial_request_v2(mut self, plan: InitialMaterialRequestPlanV1) -> Result<Self, ContractError> {
        let facts = *self.projected.obligation_v2().facts_v1();
        let relative_graph = plan
            .request_v1(facts.native_identity_v1().0, Hash64::from_u64_word(1), self.projected.accepted_daa_v2())
            .and_then(|request| request.relative_graph_id_v1(self.projected.relative_graph_id_v2(), self.projected.accepted_daa_v2()))
            .map_err(|_| ContractError::Budget)?;
        let resource = crate::palw_court_shared_tir_stream_v1::stream_digest(
            b"misaka-palw/tfp1-drawn-batch-resource/v2",
            &(self.projected.resource_id_v2(), relative_graph, self.rescue, plan),
        )
        .map_err(|_| ContractError::NonCanonical)?;
        self.obligation = facts.service_obligation_v1(self.projected.profile_id_v2(), resource)?;
        self.relative_graph = relative_graph;
        self.initial_request = Some(plan);
        Ok(self)
    }

    pub(crate) fn obligation_v1(&self) -> Tfp1PublicOriginalObligationV1 {
        self.obligation
    }

    pub(super) fn initial_request_v1(&self) -> Option<InitialMaterialRequestPlanV1> {
        self.initial_request
    }
}

pub(super) fn drawn_scope_v1(
    obligation: Tfp1PublicOriginalObligationV1,
    original: OriginalId,
    rank: u64,
    maximum: u64,
) -> Result<Hash64, ContractError> {
    if rank >= maximum {
        return Err(ContractError::Budget);
    }
    let scope = obligation.batch_scope_v1(original)?;
    crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/tfp1-drawn-batch-reservation/v1", &(scope, rank))
        .map_err(|_| ContractError::NonCanonical)
}

impl PublicOriginalAdmissionV1<'_> {
    /// The unsigned companion is fully identity-checked before using its ID
    /// as a physical namespace. Quote callers provide a private candidate book.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn reserve_tfp1_drawn_batch_candidate_v1(
        &self,
        companion: &OriginalCommit,
        reservation: &PublicOriginalReservationV1<'_>,
        projection: &PlainBatchOriginalProjectionV2,
        terms: &DrawnBatchPreclaimTermsV1,
        book: &mut ServiceReservations,
        maximum_packets: usize,
        price: impl FnMut(BatchDeliveryV1, u64) -> Result<ServiceCost, String>,
    ) -> Result<Booking, BatchCourtErrorV1> {
        if reservation.rescue != terms.rescue
            || terms.initial_request.is_some_and(|plan| !plan.matches_notice_v1(reservation.origin.accepted_daa, reservation.notice))
            || !terms.projected.matches_acceptance_v2(reservation.origin.accepted_daa, reservation.origin.accepted_blue_score)
            || reservation.budget.retention_until < terms.projected.material_until_daa_v2()
        {
            return Err(BatchCourtErrorV1::CaseGraph);
        }
        self.check_tfp1_candidate_v1(companion, terms.obligation, reservation).map_err(|_| BatchCourtErrorV1::CaseGraph)?;
        let schedule = self.check_reservation_v1(companion, reservation).map_err(|_| BatchCourtErrorV1::CaseGraph)?.2;
        if let Some(plan) = terms.initial_request {
            plan.check_schedule_v1(
                reservation.origin.accepted_daa,
                reservation.notice.draw_ready_by.max(reservation.notice.missing_material_detected_by),
                schedule,
            )?;
        }
        let fee_remaining = reservation.budget.fee.checked_sub(reservation.usage.fee).ok_or(BatchCourtErrorV1::CaseGraph)?;
        reserve_drawn_v1(companion.id(), projection, terms, schedule, fee_remaining, book, maximum_packets, price)
    }
}

impl PreparedTfp1PublicOriginalV1 {
    /// Recompile on the actual native parent/clocks and current candidate book.
    /// A successful unsigned quote cannot substitute for this recheck.
    pub(crate) fn reserve_drawn_batch_candidates_v1(
        &self,
        projection: &PlainBatchOriginalProjectionV2,
        terms: &DrawnBatchPreclaimTermsV1,
        book: &mut ServiceReservations,
        maximum_packets: usize,
        price: impl FnMut(BatchDeliveryV1, u64) -> Result<ServiceCost, String>,
    ) -> Result<Booking, BatchCourtErrorV1> {
        let origin = self.companion.carrier.origin;
        if self.obligation != terms.obligation
            || self.companion.rescue != terms.rescue
            || !terms.projected.matches_acceptance_v2(origin.accepted_daa, origin.accepted_blue_score)
            || self.companion.budget.retention_until < terms.projected.material_until_daa_v2()
        {
            return Err(BatchCourtErrorV1::CaseGraph);
        }
        if let Some(plan) = terms.initial_request {
            plan.check_schedule_v1(origin.accepted_daa, self.companion.material_request_ready_by, self.companion.schedule)?;
        }
        let fee_remaining = self.companion.budget.fee.checked_sub(self.fee_used).ok_or(BatchCourtErrorV1::CaseGraph)?;
        reserve_drawn_v1(
            self.companion.original,
            projection,
            terms,
            self.companion.schedule,
            fee_remaining,
            book,
            maximum_packets,
            price,
        )
    }
}

fn exact_vec<T>(count: usize) -> Result<Vec<T>, BatchCourtErrorV1> {
    let coverage = || {
        BatchCourtErrorV1::LocalResource(NativeCoverageV1::unsupported::<Vec<T>>(
            "TFP1 drawn preclaim",
            "allocation refused or capacity differs from reserved request",
        ))
    };
    let mut values = Vec::new();
    values.try_reserve_exact(count).map_err(|_| coverage())?;
    if values.capacity() != count {
        return Err(coverage());
    }
    Ok(values)
}

/// Storage charges already priced by the native compiled graph. This is a
/// lower bound on the whole original's fee commitment: other roles, initial
/// supply and non-storage charges must be added by the complete planner.
/// Comparing a ceiling here neither burns funds nor charges them twice.
fn drawn_storage_fee_v1(quotes: &[BatchAdmissionQuoteV1]) -> Result<u64, ServiceRefusal> {
    quotes.iter().try_fold(0u64, |sum, quote| {
        sum.checked_add(quote.carrier_storage_rent_sompi)
            .and_then(|sum| sum.checked_add(quote.numerical_fault_storage_rent_sompi))
            .ok_or(ServiceRefusal::InvalidBudget)
    })
}

#[allow(clippy::too_many_arguments)]
fn reserve_drawn_v1(
    original: OriginalId,
    projection: &PlainBatchOriginalProjectionV2,
    terms: &DrawnBatchPreclaimTermsV1,
    schedule: VerificationSchedule,
    fee_remaining: u64,
    book: &mut ServiceReservations,
    maximum_packets: usize,
    mut price: impl FnMut(BatchDeliveryV1, u64) -> Result<ServiceCost, String>,
) -> Result<Booking, BatchCourtErrorV1> {
    use BatchCourtErrorV1::{CaseGraph, LocalResource};
    let count = usize::try_from(terms.rescue.max_drawn_bonds).map_err(|_| CaseGraph)?;
    if count == 0 || count > maximum_packets {
        return Err(CaseGraph);
    }
    // Both unsigned quote and signed intake use this native Source geometry.
    // A caller's small manifest cap cannot understate the actual claim Trace.
    // Other material, openings, fee and all-branch costs remain additional.
    let trace_bytes = projection.public_trace_payload_bytes_v1()?;
    if terms.rescue.per_drawn_bond.transmitted_bytes < trace_bytes || terms.rescue.per_drawn_bond.retained_bytes < trace_bytes {
        return Err(BatchCourtErrorV1::Reservation(ServiceRefusal::InvalidBudget));
    }
    let scope = |rank| drawn_scope_v1(terms.obligation, original, rank as u64, count as u64).map_err(|_| CaseGraph);
    // Compile only one graph until the entire set's structural cap is known.
    let first_scope = scope(0)?;
    let first = projection.compile_before_allocation_v2(first_scope, &terms.projected, &mut price)?;
    let width = first.packet_count_v1().checked_add(usize::from(terms.initial_request.is_some())).ok_or(CaseGraph)?;
    if width == 0 || width.checked_mul(count).is_none_or(|n| n > maximum_packets) {
        return Err(CaseGraph);
    }
    let mut scratch = TirNativeRequestsV1::new();
    scratch.exact_v1::<(Hash64, NativeRetainedV1<CompiledBatchAdmissionV1>)>(count as u64).map_err(LocalResource)?;
    scratch.exact_v1::<(Hash64, &CompiledBatchAdmissionV1)>(count as u64).map_err(LocalResource)?;
    if terms.initial_request.is_some() {
        scratch
            .exact_v1::<crate::palw_court_shared_material_intake_v1::batch::admission::protected::BatchInitialRequestV1>(count as u64)
            .map_err(LocalResource)?;
    }
    let _owner = reserve_native_pending_requests_v1(scratch.bytes_v1()).map_err(LocalResource)?;
    let mut compiled = exact_vec(count)?;
    compiled.push((first_scope, first));
    for rank in 1..count {
        let namespace = scope(rank)?;
        let next = projection.compile_before_allocation_v2(namespace, &terms.projected, &mut price)?;
        if next.packet_count_v1().checked_add(usize::from(terms.initial_request.is_some())) != Some(width) {
            return Err(CaseGraph);
        }
        compiled.push((namespace, next));
    }
    let mut cases = exact_vec(count)?;
    for (namespace, graph) in &compiled {
        cases.push((*namespace, &**graph));
    }
    let initial = if let Some(plan) = terms.initial_request {
        let a = terms.projected.accepted_daa_v2();
        // Both public entrypoints check the exact native readiness floor.
        // The common kernel also binds the last slot to the original clock.
        plan.check_deadline_v1(a, schedule)?;
        let claim = terms.obligation.facts_v1().native_identity_v1().0;
        let mut requests = exact_vec(count)?;
        for (namespace, _) in &cases {
            requests.push(plan.request_v1(claim, *namespace, a)?);
        }
        Some(requests)
    } else {
        None
    };
    let allowance = terms.rescue.per_drawn_bond;
    CompiledBatchAdmissionV1::reserve_protected_with_requests_v3(
        &cases,
        initial.as_deref(),
        book,
        maximum_packets,
        |quotes, candidate| {
            if quotes.len() != count {
                return Err(ServiceRefusal::InvalidState);
            }
            // All ranks, PMR carriers and Node-produced fault storage share
            // the signed original fee ceiling. Check before publishing any
            // of this candidate's physical reservations to the caller book.
            if drawn_storage_fee_v1(quotes)? > fee_remaining {
                return Err(ServiceRefusal::InvalidBudget);
            }
            for (rank, quote) in quotes.iter().enumerate() {
                cases[rank].1.check_public_original_schedule_v1(candidate, schedule)?;
                if quote.scope != cases[rank].0
                    || quote.relative_graph_id != terms.relative_graph
                    || quote.service.bytes > allowance.transmitted_bytes
                    || quote.service.work > allowance.additional_work
                    || quote
                        .retained_payload_bytes
                        .checked_add(quote.retained_numerical_fault_bytes)
                        .is_none_or(|bytes| bytes > allowance.retained_bytes)
                {
                    return Err(ServiceRefusal::InvalidBudget);
                }
            }
            Ok(())
        },
    )
}

#[cfg(test)]
pub(crate) mod tests;
