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
    reservations::RemainingCapacity,
};

type Booking = (NativeRetainedV1<Vec<BatchAdmissionQuoteV1>>, NativeRetainedV1<Vec<CausalServicePacketV1>>);

/// The checked protected reservation result. The original and obligation pin
/// the quoted rents to their booking and both native allocation owners stay
/// live for as long as those quotes and the packet graph can be read.
pub(crate) struct ProtectedBatchBookingV1 {
    original: OriginalId,
    obligation: Tfp1PublicOriginalObligationV1,
    accepted_daa: u64,
    accepted_blue_score: u64,
    book_identity: Hash64,
    quotes: NativeRetainedV1<Vec<BatchAdmissionQuoteV1>>,
    graph: NativeRetainedV1<Vec<CausalServicePacketV1>>,
}

impl ProtectedBatchBookingV1 {
    pub(crate) fn original_v1(&self) -> OriginalId {
        self.original
    }

    pub(crate) fn obligation_v1(&self) -> Tfp1PublicOriginalObligationV1 {
        self.obligation
    }

    pub(crate) fn matches_acceptance_v1(&self, daa: u64, blue_score: u64) -> bool {
        self.accepted_daa == daa && self.accepted_blue_score == blue_score
    }

    pub(crate) fn quotes_v1(&self) -> &[BatchAdmissionQuoteV1] {
        self.quotes.as_slice()
    }

    pub(crate) fn graph_v1(&self) -> &[CausalServicePacketV1] {
        self.graph.as_slice()
    }

    /// Immutable per-original promises, excluding recoverable physical slots.
    /// This is content identity, not delivery or computation authentication.
    pub(crate) fn service_graph_identity_v1(&self) -> Result<Hash64, ContractError> {
        super::service_graph::protected_graph_identity_v1(
            self.original,
            self.obligation,
            self.accepted_daa,
            self.accepted_blue_score,
            self.graph_v1(),
        )
    }

    /// The receiving native owner must use the exact successful candidate,
    /// including its capacity and physical assignments, not just an equal
    /// packet graph rebooked against another prefix.
    pub(crate) fn check_book_v1(&self, book: &ServiceReservations) -> Result<(), BatchCourtErrorV1> {
        if self.book_identity != protected_book_identity_v1(book) {
            return Err(BatchCourtErrorV1::CaseGraph);
        }
        Ok(())
    }
}

/// A transient ownership seal, not a new wire ID or a service receipt. Use
/// fixed scalar encodings and u64 collection counts so this infallible hash
/// cannot fail after the scheduler has committed its candidate. No Vec or
/// Borsh collection-length conversion is needed.
fn protected_book_identity_v1(book: &ServiceReservations) -> Hash64 {
    fn cost(h: &mut blake2b_simd::State, value: ServiceCost) {
        for n in [value.bytes, value.work, value.io, value.controls] {
            h.update(&n.to_le_bytes());
        }
    }
    let mut h = blake2b_simd::Params::new().hash_length(64).key(b"misaka/tfp1/protected-book/v1").to_state();
    cost(&mut h, book.budget);
    h.update(&(book.remaining.len() as u64).to_le_bytes());
    for (daa, capacity) in &book.remaining {
        h.update(&daa.to_le_bytes());
        cost(&mut h, *capacity);
    }
    h.update(&(book.assignments.len() as u64).to_le_bytes());
    for (id, daa) in &book.assignments {
        h.update(id);
        h.update(&daa.to_le_bytes());
    }
    h.update(&(book.packets.len() as u64).to_le_bytes());
    for (id, packet) in &book.packets {
        h.update(id);
        h.update(&packet.id);
        h.update(&packet.release_daa.to_le_bytes());
        h.update(&packet.service_before.to_le_bytes());
        cost(&mut h, packet.cost);
    }
    h.update(&(book.supply.len() as u64).to_le_bytes());
    for (daa, supply) in &book.supply {
        h.update(&daa.to_le_bytes());
        h.update(&supply.accepting_block);
        cost(&mut h, supply.reserved);
    }
    h.update(&[u8::from(book.last_observed_daa.is_some())]);
    h.update(&book.last_observed_daa.unwrap_or(0).to_le_bytes());
    Hash64::from_bytes(h.finalize().as_bytes().try_into().expect("64-byte ownership seal"))
}

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

/// Separate namespaces for the protected planning profile. The tags below
/// are local hash preimage values; they do not reinterpret a claimant wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProtectedBatchRoleV1 {
    Drawn,
    Outsider,
    Defense,
}

impl ProtectedBatchRoleV1 {
    fn coordinates_v1(self, rescue: ProtectedRescueDemand) -> (u8, u64, RemainingCapacity) {
        match self {
            Self::Drawn => (0, rescue.max_drawn_bonds, rescue.per_drawn_bond),
            Self::Outsider => (1, rescue.max_outsiders, rescue.per_outsider),
            Self::Defense => (2, 1, rescue.defensive),
        }
    }
}

/// Covers each selected and outsider rank and one defensive opportunity.
/// Every opportunity books the same material/verification graph as a
/// conservative bound. This gives no claimant or responder authority, does
/// not establish initial supply, and does not book the first notice or full
/// inspection outside the graph. A full profile still needs those duties.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProtectedBatchPreclaimTermsV1(DrawnBatchPreclaimTermsV1);

impl ProtectedBatchPreclaimTermsV1 {
    pub(crate) fn new_v1(
        projected: PlainBatchProspectiveOriginalTermsV2,
        rescue: ProtectedRescueDemand,
        initial_request: Option<InitialMaterialRequestPlanV1>,
    ) -> Result<Self, ContractError> {
        let mut terms = DrawnBatchPreclaimTermsV1::new_v1(projected, rescue)?;
        if let Some(plan) = initial_request {
            terms = terms.with_initial_request_v2(plan)?;
        }
        let resource = crate::palw_court_shared_tir_stream_v1::stream_digest(
            b"misaka-palw/tfp1-protected-batch-resource/v1",
            &(projected.resource_id_v2(), terms.relative_graph, rescue, initial_request),
        )
        .map_err(|_| ContractError::NonCanonical)?;
        terms.obligation = (*projected.obligation_v2().facts_v1()).service_obligation_v1(projected.profile_id_v2(), resource)?;
        Ok(Self(terms))
    }

    pub(crate) fn obligation_v1(&self) -> Tfp1PublicOriginalObligationV1 {
        self.0.obligation
    }

    pub(crate) fn relative_graph_id_v1(&self) -> Hash64 {
        self.0.relative_graph
    }

    pub(super) fn initial_request_v1(&self) -> Option<InitialMaterialRequestPlanV1> {
        self.0.initial_request
    }

    pub(super) fn rescue_v1(&self) -> ProtectedRescueDemand {
        self.0.rescue
    }

    pub(crate) fn reservation_scope_v1(
        &self,
        original: OriginalId,
        role: ProtectedBatchRoleV1,
        rank: u64,
    ) -> Result<Hash64, ContractError> {
        protected_scope_v1(self.0.obligation, original, self.0.rescue, role, rank)
    }
}

/// Physical identity only. The consumer must independently authenticate the
/// protected profile, companion, rank membership and booked packet.
pub(crate) fn protected_scope_v1(
    obligation: Tfp1PublicOriginalObligationV1,
    original: OriginalId,
    rescue: ProtectedRescueDemand,
    role: ProtectedBatchRoleV1,
    rank: u64,
) -> Result<Hash64, ContractError> {
    let (tag, count, _) = role.coordinates_v1(rescue);
    if rank >= count {
        return Err(ContractError::Budget);
    }
    let scope = obligation.batch_scope_v1(original)?;
    crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/tfp1-protected-batch-reservation/v1", &(scope, tag, rank))
        .map_err(|_| ContractError::NonCanonical)
}

#[derive(Clone, Copy)]
enum BookingScopeV1 {
    DrawnOnly,
    Protected,
}

impl BookingScopeV1 {
    fn count_v1(self, rescue: ProtectedRescueDemand) -> Option<u64> {
        match self {
            Self::DrawnOnly => Some(rescue.max_drawn_bonds),
            Self::Protected => rescue.max_drawn_bonds.checked_add(rescue.max_outsiders)?.checked_add(1),
        }
    }

    fn role_rank_v1(self, rescue: ProtectedRescueDemand, index: u64) -> (ProtectedBatchRoleV1, u64) {
        use ProtectedBatchRoleV1::*;
        if matches!(self, Self::DrawnOnly) || index < rescue.max_drawn_bonds {
            return (Drawn, index);
        }
        let remaining = index - rescue.max_drawn_bonds;
        if remaining < rescue.max_outsiders { (Outsider, remaining) } else { (Defense, remaining - rescue.max_outsiders) }
    }

    fn scope_v1(self, terms: &DrawnBatchPreclaimTermsV1, original: OriginalId, index: u64) -> Result<Hash64, ContractError> {
        match self {
            Self::DrawnOnly => drawn_scope_v1(terms.obligation, original, index, terms.rescue.max_drawn_bonds),
            Self::Protected => {
                let (role, rank) = self.role_rank_v1(terms.rescue, index);
                protected_scope_v1(terms.obligation, original, terms.rescue, role, rank)
            }
        }
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
        self.reserve_batch_candidate_v1(
            companion,
            reservation,
            projection,
            terms,
            BookingScopeV1::DrawnOnly,
            book,
            maximum_packets,
            price,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn reserve_tfp1_protected_batch_candidate_v1(
        &self,
        companion: &OriginalCommit,
        reservation: &PublicOriginalReservationV1<'_>,
        projection: &PlainBatchOriginalProjectionV2,
        terms: &ProtectedBatchPreclaimTermsV1,
        book: &mut ServiceReservations,
        maximum_packets: usize,
        price: impl FnMut(BatchDeliveryV1, u64) -> Result<ServiceCost, String>,
    ) -> Result<ProtectedBatchBookingV1, BatchCourtErrorV1> {
        let original = companion.id();
        let (quotes, graph) = self.reserve_batch_candidate_v1(
            companion,
            reservation,
            projection,
            &terms.0,
            BookingScopeV1::Protected,
            book,
            maximum_packets,
            price,
        )?;
        Ok(ProtectedBatchBookingV1 {
            original,
            obligation: terms.obligation_v1(),
            accepted_daa: reservation.origin.accepted_daa,
            accepted_blue_score: reservation.origin.accepted_blue_score,
            book_identity: protected_book_identity_v1(book),
            quotes,
            graph,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn reserve_batch_candidate_v1(
        &self,
        companion: &OriginalCommit,
        reservation: &PublicOriginalReservationV1<'_>,
        projection: &PlainBatchOriginalProjectionV2,
        terms: &DrawnBatchPreclaimTermsV1,
        scopes: BookingScopeV1,
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
        reserve_batch_v1(companion.id(), projection, terms, scopes, schedule, fee_remaining, book, maximum_packets, price)
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
        self.reserve_batch_candidates_v1(projection, terms, BookingScopeV1::DrawnOnly, book, maximum_packets, price)
    }

    pub(crate) fn reserve_protected_batch_candidates_v1(
        &self,
        projection: &PlainBatchOriginalProjectionV2,
        terms: &ProtectedBatchPreclaimTermsV1,
        book: &mut ServiceReservations,
        maximum_packets: usize,
        price: impl FnMut(BatchDeliveryV1, u64) -> Result<ServiceCost, String>,
    ) -> Result<ProtectedBatchBookingV1, BatchCourtErrorV1> {
        let (quotes, graph) =
            self.reserve_batch_candidates_v1(projection, &terms.0, BookingScopeV1::Protected, book, maximum_packets, price)?;
        Ok(ProtectedBatchBookingV1 {
            original: self.companion.original,
            obligation: self.obligation,
            accepted_daa: self.companion.carrier.origin.accepted_daa,
            accepted_blue_score: self.companion.carrier.origin.accepted_blue_score,
            book_identity: protected_book_identity_v1(book),
            quotes,
            graph,
        })
    }

    fn reserve_batch_candidates_v1(
        &self,
        projection: &PlainBatchOriginalProjectionV2,
        terms: &DrawnBatchPreclaimTermsV1,
        scopes: BookingScopeV1,
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
        reserve_batch_v1(
            self.companion.original,
            projection,
            terms,
            scopes,
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
            "TFP1 Batch preclaim",
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
/// lower bound on the whole original's fee commitment. Initial supply and
/// non-storage charges must be added by the complete planner.
/// Comparing a ceiling here neither burns funds nor charges them twice.
fn batch_storage_fee_v1(quotes: &[BatchAdmissionQuoteV1]) -> Result<u64, ServiceRefusal> {
    quotes.iter().try_fold(0u64, |sum, quote| {
        sum.checked_add(quote.carrier_storage_rent_sompi)
            .and_then(|sum| sum.checked_add(quote.numerical_fault_storage_rent_sompi))
            .ok_or(ServiceRefusal::InvalidBudget)
    })
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn reserve_drawn_v1(
    original: OriginalId,
    projection: &PlainBatchOriginalProjectionV2,
    terms: &DrawnBatchPreclaimTermsV1,
    schedule: VerificationSchedule,
    fee_remaining: u64,
    book: &mut ServiceReservations,
    maximum_packets: usize,
    price: impl FnMut(BatchDeliveryV1, u64) -> Result<ServiceCost, String>,
) -> Result<Booking, BatchCourtErrorV1> {
    reserve_batch_v1(original, projection, terms, BookingScopeV1::DrawnOnly, schedule, fee_remaining, book, maximum_packets, price)
}

#[allow(clippy::too_many_arguments)]
fn reserve_batch_v1(
    original: OriginalId,
    projection: &PlainBatchOriginalProjectionV2,
    terms: &DrawnBatchPreclaimTermsV1,
    scopes: BookingScopeV1,
    schedule: VerificationSchedule,
    fee_remaining: u64,
    book: &mut ServiceReservations,
    maximum_packets: usize,
    mut price: impl FnMut(BatchDeliveryV1, u64) -> Result<ServiceCost, String>,
) -> Result<Booking, BatchCourtErrorV1> {
    use BatchCourtErrorV1::{CaseGraph, LocalResource};
    let count = usize::try_from(scopes.count_v1(terms.rescue).ok_or(CaseGraph)?).map_err(|_| CaseGraph)?;
    if count == 0 || count > maximum_packets {
        return Err(CaseGraph);
    }
    // Both unsigned quote and signed intake use this native Source geometry.
    // A caller's small manifest cap cannot understate the actual claim Trace.
    // Other material, openings, fee and all-branch costs remain additional.
    let trace_bytes = projection.public_trace_payload_bytes_v1()?;
    let trace_fits =
        |allowance: RemainingCapacity| allowance.transmitted_bytes >= trace_bytes && allowance.retained_bytes >= trace_bytes;
    if !trace_fits(terms.rescue.per_drawn_bond)
        || (matches!(scopes, BookingScopeV1::Protected)
            && (!trace_fits(terms.rescue.defensive) || (terms.rescue.max_outsiders != 0 && !trace_fits(terms.rescue.per_outsider))))
    {
        return Err(BatchCourtErrorV1::Reservation(ServiceRefusal::InvalidBudget));
    }
    let scope = |rank| scopes.scope_v1(terms, original, rank as u64).map_err(|_| CaseGraph);
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
            if batch_storage_fee_v1(quotes)? > fee_remaining {
                return Err(ServiceRefusal::InvalidBudget);
            }
            for (index, (quote, (scope, graph))) in quotes.iter().zip(&cases).enumerate() {
                graph.check_public_original_schedule_v1(candidate, schedule)?;
                let (role, _) = scopes.role_rank_v1(terms.rescue, index as u64);
                let (_, _, allowance) = role.coordinates_v1(terms.rescue);
                if quote.scope != *scope
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
